use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

use super::double_option;
use crate::auth::{Admin, check_username, hash_password, random_token};
use crate::config::Settings;
use crate::db::{User, delete_items, now};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub async fn users(State(app): State<AppState>, Admin(_): Admin) -> AppResult<Json<Value>> {
    let rows = app
        .db
        .read(|c| {
            let mut st = c.prepare_cached(&format!("SELECT {}, (SELECT COUNT(*) FROM items i WHERE i.user_id = users.id) FROM users ORDER BY created_at", User::COLS))?;
            let rows = st
                .query_map([], |r| {
                    let u = User::from_row(r)?;
                    let n: i64 = r.get(User::NCOLS)?;
                    Ok((u, n))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;
    let list: Vec<Value> = rows
        .into_iter()
        .map(|(u, n)| {
            let mut v = serde_json::to_value(u).unwrap_or_default();
            v["items"] = json!(n);
            v
        })
        .collect();
    Ok(Json(json!(list)))
}

#[derive(Deserialize)]
pub struct UpdateUser {
    role: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    quota_mb: Option<Option<i64>>,
    disabled: Option<bool>,
}

async fn manageable(app: &AppState, me: &User, id: i64) -> AppResult<User> {
    let target = app.db.read(move |c| Ok(User::by_id(c, id)?)).await?.ok_or_else(AppError::not_found)?;
    if !me.may_manage(target.id, target.owner) {
        return Err(AppError::forbidden("The owner of this uploader can't be changed by other admins"));
    }
    Ok(target)
}

pub async fn update_user(State(app): State<AppState>, Admin(me): Admin, Path(id): Path<i64>, Json(req): Json<UpdateUser>) -> AppResult<StatusCode> {
    manageable(&app, &me, id).await?;
    if id == me.id && (req.role.as_deref().is_some_and(|r| r != "admin") || req.disabled == Some(true)) {
        return Err(AppError::bad("You can't take away your own admin rights"));
    }
    if let Some(r) = &req.role
        && r != "admin"
        && r != "user"
    {
        return Err(AppError::bad("Role must be admin or user"));
    }
    if let Some(Some(q)) = req.quota_mb
        && q < 0
    {
        return Err(AppError::bad("Quota can't be negative"));
    }
    let n = app
        .db
        .write(move |c| {
            let tx = c.transaction()?;
            let mut n = 0;
            if let Some(r) = &req.role {
                n = tx.execute("UPDATE users SET role = ? WHERE id = ?", rusqlite::params![r, id])?;
            }
            if let Some(q) = req.quota_mb {
                n = tx.execute("UPDATE users SET quota_mb = ? WHERE id = ?", rusqlite::params![q, id])?;
            }
            if let Some(d) = req.disabled {
                n = tx.execute("UPDATE users SET disabled = ? WHERE id = ?", rusqlite::params![d, id])?;
                if d {
                    tx.execute("DELETE FROM sessions WHERE user_id = ?", [id])?;
                }
            }
            tx.commit()?;
            Ok(n)
        })
        .await?;
    if n == 0 { Err(AppError::not_found()) } else { Ok(StatusCode::NO_CONTENT) }
}

pub async fn delete_user(State(app): State<AppState>, Admin(me): Admin, Path(id): Path<i64>) -> AppResult<StatusCode> {
    if id == me.id {
        return Err(AppError::bad("You can't delete yourself"));
    }
    manageable(&app, &me, id).await?;
    let store = app.store.clone();
    let n = app
        .db
        .write(move |c| {
            let ids: Vec<String> = c.prepare("SELECT id FROM items WHERE user_id = ?")?.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
            let unused = delete_items(c, &ids)?;
            let n = c.execute("DELETE FROM users WHERE id = ?", [id])?;
            store.remove(&unused);
            Ok(n)
        })
        .await?;
    if n == 0 { Err(AppError::not_found()) } else { Ok(StatusCode::NO_CONTENT) }
}

pub async fn reset_password(State(app): State<AppState>, Admin(me): Admin, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    manageable(&app, &me, id).await?;
    let pw = random_token(9);
    let hash = hash_password(pw.clone()).await?;
    let n = app
        .db
        .write(move |c| {
            let n = c.execute("UPDATE users SET password_hash = ? WHERE id = ?", rusqlite::params![hash, id])?;
            c.execute("DELETE FROM sessions WHERE user_id = ?", [id])?;
            Ok(n)
        })
        .await?;
    if n == 0 {
        return Err(AppError::not_found());
    }
    Ok(Json(json!({ "password": pw })))
}

pub async fn invites(State(app): State<AppState>, Admin(_): Admin) -> AppResult<Json<Value>> {
    let t = now();
    let rows = app
        .db
        .read(move |c| {
            let mut st = c.prepare_cached("SELECT code, uses_left, expires_at, created_at, username, role FROM invites WHERE uses_left > 0 AND (expires_at IS NULL OR expires_at > ?) ORDER BY created_at DESC")?;
            let rows = st
                .query_map([t], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, i64>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, String>(5)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;
    let list: Vec<Value> = rows
        .into_iter()
        .map(|(code, uses, exp, created, username, role)| {
            json!({ "code": code, "uses_left": uses, "expires_at": exp, "created_at": created, "username": username, "admin": role == "admin", "url": app.link(&format!("/register?invite={code}")) })
        })
        .collect();
    Ok(Json(json!(list)))
}

#[derive(Deserialize)]
pub struct InviteReq {
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    admin: bool,
    #[serde(default = "one")]
    uses: i64,
    #[serde(default)]
    days: Option<i64>,
}

fn one() -> i64 {
    1
}

pub async fn create_invite(State(app): State<AppState>, Admin(me): Admin, Json(req): Json<InviteReq>) -> AppResult<Json<Value>> {
    let username = match req.username.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        Some(u) => Some(check_username(u)?),
        None => None,
    };
    let uses = if username.is_some() { 1 } else { req.uses };
    if !(1..=1000).contains(&uses) {
        return Err(AppError::bad("Uses must be between 1 and 1000"));
    }
    let code = random_token(12);
    let exp = req.days.filter(|d| *d > 0).map(|d| now() + d.min(3650) * 86400);
    let role = if req.admin { "admin" } else { "user" };
    let (c2, uid, name) = (code.clone(), me.id, username.clone());
    let ok = app
        .db
        .write(move |c| {
            if let Some(n) = &name {
                let taken: bool = c.query_row(
                    "SELECT EXISTS(SELECT 1 FROM users WHERE username = ?1) OR EXISTS(SELECT 1 FROM invites WHERE username = ?1 COLLATE NOCASE AND uses_left > 0)",
                    [n],
                    |r| r.get(0),
                )?;
                if taken {
                    return Ok(false);
                }
            }
            c.execute(
                "INSERT INTO invites (code, created_by, uses_left, expires_at, created_at, username, role) VALUES (?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![c2, uid, uses, exp, now(), name, role],
            )?;
            Ok(true)
        })
        .await?;
    if !ok {
        return Err(AppError::conflict("That username is taken"));
    }
    Ok(Json(json!({ "code": code, "url": app.link(&format!("/register?invite={code}")) })))
}

pub async fn delete_invite(State(app): State<AppState>, Admin(_): Admin, Path(code): Path<String>) -> AppResult<StatusCode> {
    app.db.write(move |c| Ok(c.execute("DELETE FROM invites WHERE code = ?", [code])?)).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn settings_view(app: &AppState, s: &Settings) -> Value {
    let mut v = serde_json::to_value(s).unwrap_or_default();
    v["twitch_client_secret"] = json!("");
    v["twitch_secret_set"] = json!(!s.twitch_client_secret.is_empty());
    v["twitch_from_env"] = json!(app.cfg.twitch.is_some());
    v["twitch_redirect_url"] = json!(app.link("/api/auth/twitch/callback"));
    v
}

pub async fn get_settings(State(app): State<AppState>, Admin(_): Admin) -> Json<Value> {
    Json(settings_view(&app, &app.settings()))
}

pub async fn put_settings(State(app): State<AppState>, Admin(_): Admin, Json(mut s): Json<Settings>) -> AppResult<Json<Value>> {
    let old = app.settings();
    if s.twitch_client_secret.trim().is_empty() {
        s.twitch_client_secret = old.twitch_client_secret.clone();
    }
    if s.twitch_client_id.trim().is_empty() {
        s.twitch_client_secret.clear();
    }
    s.validate().map_err(AppError::bad)?;
    save_settings(&app, &s).await?;
    app.set_settings(s.clone());
    Ok(Json(settings_view(&app, &s)))
}

pub async fn save_settings(app: &AppState, s: &Settings) -> anyhow::Result<()> {
    let v = serde_json::to_string(s)?;
    app.db.write(move |c| Ok(c.execute("INSERT INTO settings (key, value) VALUES ('instance', ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [v])?)).await?;
    Ok(())
}

pub fn load_settings(c: &rusqlite::Connection) -> Settings {
    c.query_row("SELECT value FROM settings WHERE key = 'instance'", [], |r| r.get::<_, String>(0)).ok().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

pub async fn version(State(app): State<AppState>, Admin(_): Admin) -> Json<Value> {
    let cached = app.latest.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let latest = match cached {
        Some((at, v)) if at.elapsed() < Duration::from_secs(3600) => v,
        _ => {
            let v = crate::update::latest_version(&app.http, &app.cfg.repo).await.ok();
            *app.latest.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), v.clone()));
            v
        }
    };
    let current = env!("CARGO_PKG_VERSION");
    let newer = latest.as_deref().is_some_and(|l| crate::update::is_newer(l, current));
    Json(json!({ "current": current, "latest": latest, "update": newer, "repo": app.cfg.repo, "docker": crate::update::in_docker() }))
}

pub async fn update(State(app): State<AppState>, Admin(_): Admin) -> AppResult<Json<Value>> {
    if crate::update::in_docker() {
        return Err(AppError::bad("Running in Docker: run `docker compose pull && docker compose up -d` on the server"));
    }
    let installed = crate::update::install(&app.http, &app.cfg.repo).await.map_err(|e| AppError::bad(format!("{e:#}")))?;
    let Some(v) = installed else { return Ok(Json(json!({ "updated": false }))) };
    tracing::info!("updated to v{v}");
    let a = app.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let _ = crate::tasks::flush_views(&a).await;
        a.shutdown.notify_one();
    });
    Ok(Json(json!({ "updated": true, "version": v })))
}
