use std::time::{Duration, Instant};

use crate::db::{delete_items, now};
use crate::state::AppState;

pub fn spawn(app: AppState) {
    let a = app.clone();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(5));
        loop {
            t.tick().await;
            if let Err(e) = flush_views(&a).await {
                tracing::warn!("saving views: {e:#}");
            }
        }
    });
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(60));
        loop {
            t.tick().await;
            match cleanup(&app).await {
                Ok(0) => {}
                Ok(n) => tracing::info!("removed {n} expired items"),
                Err(e) => tracing::warn!("cleanup: {e:#}"),
            }
        }
    });
}

pub async fn flush_views(app: &AppState) -> anyhow::Result<()> {
    let views = std::mem::take(&mut *app.views.lock().unwrap_or_else(|e| e.into_inner()));
    if views.is_empty() {
        return Ok(());
    }
    app.db
        .write(move |c| {
            let tx = c.transaction()?;
            {
                let mut st = tx.prepare_cached("UPDATE items SET views = views + ? WHERE id = ?")?;
                for (id, n) in &views {
                    st.execute(rusqlite::params![n, id])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await
}

pub async fn cleanup(app: &AppState) -> anyhow::Result<usize> {
    let t = now();
    let store = app.store.clone();
    let n = app
        .db
        .write(move |c| {
            let ids: Vec<String> = c
                .prepare_cached("SELECT id FROM items WHERE (expires_at IS NOT NULL AND expires_at <= ?1) OR (max_views IS NOT NULL AND views >= max_views) LIMIT 1000")?
                .query_map([t], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            let unused = delete_items(c, &ids)?;
            store.remove(&unused);
            c.execute("DELETE FROM sessions WHERE expires_at <= ?", [t])?;
            c.execute("DELETE FROM invites WHERE expires_at IS NOT NULL AND expires_at <= ?", [t])?;
            Ok(ids.len())
        })
        .await?;
    let stale: Vec<std::path::PathBuf> = {
        let mut m = app.chunks.lock().unwrap_or_else(|e| e.into_inner());
        let old: Vec<String> = m.iter().filter(|(_, c)| c.touched.elapsed() > Duration::from_secs(3600)).map(|(k, _)| k.clone()).collect();
        old.into_iter().filter_map(|k| m.remove(&k)).map(|c| c.path).collect()
    };
    for p in stale {
        let _ = tokio::fs::remove_file(p).await;
    }
    let tmp = app.store.tmp_dir();
    let active: std::collections::HashSet<std::path::PathBuf> = app.chunks.lock().unwrap_or_else(|e| e.into_inner()).values().map(|c| c.path.clone()).collect();
    if let Ok(mut rd) = tokio::fs::read_dir(&tmp).await {
        while let Ok(Some(e)) = rd.next_entry().await {
            let old = e.metadata().await.ok().and_then(|m| m.modified().ok()).and_then(|m| m.elapsed().ok()).is_some_and(|d| d > Duration::from_secs(86400));
            if old && !active.contains(&e.path()) {
                let _ = tokio::fs::remove_file(e.path()).await;
            }
        }
    }
    Ok(n)
}

pub async fn clean_command(app: &AppState, dry_run: bool) -> anyhow::Result<()> {
    let started = Instant::now();
    if !dry_run {
        let n = cleanup(app).await?;
        println!("Removed {n} expired items");
    }
    let used: std::collections::HashSet<String> = app.db.read(|c| Ok(c.prepare("SELECT DISTINCT sha256 FROM items WHERE sha256 IS NOT NULL")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)).await?;
    let root = app.store.files_dir();
    let (mut count, mut bytes) = (0u64, 0u64);
    for dir in std::fs::read_dir(&root)?.flatten() {
        for f in std::fs::read_dir(dir.path())?.flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            if !used.contains(&name) {
                count += 1;
                bytes += f.metadata().map(|m| m.len()).unwrap_or(0);
                if dry_run {
                    println!("would remove {}", f.path().display());
                } else {
                    app.store.remove(&[name]);
                }
            }
        }
    }
    println!("{} {count} unused files ({:.1} MB) in {:.1?}", if dry_run { "Found" } else { "Removed" }, bytes as f64 / 1048576.0, started.elapsed());
    Ok(())
}
