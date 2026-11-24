use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use serde_json::{Value, json};

use super::ItemView;
use crate::auth::Auth;
use crate::db::{Item, now};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct StatsQuery {
    scope: Option<String>,
    days: Option<i64>,
}

pub async fn stats(State(app): State<AppState>, Auth(user): Auth, Query(q): Query<StatsQuery>) -> AppResult<Json<Value>> {
    let all = q.scope.as_deref() == Some("all");
    if all && !user.is_admin() {
        return Err(AppError::forbidden("Admins only"));
    }
    let days = q.days.unwrap_or(30).clamp(7, 365);
    let uid = user.id;
    let since = (now() / 86400 - days + 1) * 86400;
    let (filter, arg): (&str, Option<i64>) = if all { ("1 = 1", None) } else { ("user_id = ?1", Some(uid)) };
    let data = app
        .db
        .read(move |c| {
            let args: Vec<i64> = arg.into_iter().collect();
            let totals: (i64, i64, i64) = c.query_row(&format!("SELECT COUNT(*), COALESCE(SUM(size), 0), COALESCE(SUM(views), 0) FROM items WHERE {filter}"), rusqlite::params_from_iter(&args), |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?;
            let mut st = c.prepare(&format!(
                "SELECT CASE
                    WHEN kind = 'link' THEN 'link'
                    WHEN kind = 'paste' THEN 'text'
                    WHEN mime IN ('image/png','image/jpeg','image/gif','image/webp','image/avif','image/bmp','image/svg+xml','image/x-icon') THEN 'image'
                    WHEN mime IN ('video/mp4','video/webm','video/ogg','video/quicktime') THEN 'video'
                    WHEN mime LIKE 'audio/%' THEN 'audio'
                    WHEN mime = 'application/pdf' THEN 'pdf'
                    WHEN mime LIKE 'text/%' OR mime IN ('application/json','application/javascript','application/xml','application/x-sh','application/toml','application/x-yaml','application/sql') THEN 'text'
                    ELSE 'other' END AS t, COUNT(*), COALESCE(SUM(size), 0)
                 FROM items WHERE {filter} GROUP BY t ORDER BY 2 DESC"
            ))?;
            let types = st
                .query_map(rusqlite::params_from_iter(&args), |r| Ok(json!({ "type": r.get::<_, String>(0)?, "count": r.get::<_, i64>(1)?, "size": r.get::<_, i64>(2)? })))?
                .collect::<Result<Vec<_>, _>>()?;
            let mut dargs = args.clone();
            dargs.push(since);
            let n = dargs.len();
            let mut st = c.prepare(&format!("SELECT created_at / 86400, COUNT(*), COALESCE(SUM(size), 0) FROM items WHERE {filter} AND created_at >= ?{n} GROUP BY 1"))?;
            let per_day: std::collections::HashMap<i64, (i64, i64)> =
                st.query_map(rusqlite::params_from_iter(&dargs), |r| Ok((r.get::<_, i64>(0)?, (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))))?.collect::<Result<_, _>>()?;
            let first = since / 86400;
            let days: Vec<Value> = (first..first + days)
                .map(|d| {
                    let (count, size) = per_day.get(&d).copied().unwrap_or((0, 0));
                    json!({ "day": d * 86400, "count": count, "size": size })
                })
                .collect();
            let mut st = c.prepare(&format!("SELECT {} FROM items WHERE {filter} AND views > 0 ORDER BY views DESC LIMIT 5", Item::COLS))?;
            let top = st.query_map(rusqlite::params_from_iter(&args), Item::from_row)?.collect::<Result<Vec<_>, _>>()?;
            let users: i64 = if all { c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))? } else { 1 };
            Ok((totals, types, days, top, users))
        })
        .await?;
    let ((count, size, views), types, days, top, users) = data;
    let disk = if all { disk_usage(&app.store.tmp_dir()) } else { None };
    let s = app.settings();
    let quota = if all { None } else { user.quota_mb.or((s.default_quota_mb > 0).then_some(s.default_quota_mb as i64)).filter(|q| *q > 0).map(|q| q * 1024 * 1024) };
    Ok(Json(json!({
        "count": count, "size": size, "views": views, "users": users,
        "quota": quota, "types": types, "days": days,
        "top": top.iter().map(|i| ItemView::new(&app, i, i.user_id == uid)).collect::<Vec<_>>(),
        "disk": disk,
    })))
}

fn disk_usage(path: &std::path::Path) -> Option<Value> {
    let out = std::process::Command::new("df").arg("-Pk").arg(path).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?;
    let cols: Vec<&str> = line.split_whitespace().collect();
    let total: i64 = cols.get(1)?.parse().ok()?;
    let avail: i64 = cols.get(3)?.parse().ok()?;
    Some(json!({ "total": total * 1024, "free": avail * 1024 }))
}
