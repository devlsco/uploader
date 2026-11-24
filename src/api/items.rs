use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use rusqlite::types::Value as Sql;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

use super::{ItemView, double_option, parse_duration, uploader};
use crate::auth::{Auth, ClientIp, MaybeAuth, hash_password, sign, time_dur, verify, verify_password};
use crate::db::{Item, Kind, User, delete_items, now};
use crate::error::{AppError, AppResult};
use crate::state::{App, AppState};

const INLINE_TEXT: i64 = 512 * 1024;

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(rename = "type")]
    view_type: Option<String>,
    q: Option<String>,
    album: Option<String>,
    before: Option<String>,
    limit: Option<usize>,
}

const TEXT_MIMES: &str = "('application/json','application/javascript','application/xml','application/x-sh','application/toml','application/x-yaml','application/sql')";

pub async fn list(State(app): State<AppState>, Auth(user): Auth, Query(q): Query<ListQuery>) -> AppResult<Json<Value>> {
    let limit = q.limit.unwrap_or(60).clamp(1, 200);
    let mut sql = format!("SELECT {} FROM items WHERE user_id = ?", Item::COLS);
    let mut args: Vec<Sql> = vec![Sql::Integer(user.id)];
    let image = "(kind = 'file' AND (mime IN ('image/png','image/jpeg','image/gif','image/webp','image/avif','image/bmp','image/svg+xml','image/x-icon')))";
    let video = "(kind = 'file' AND mime IN ('video/mp4','video/webm','video/ogg','video/quicktime'))";
    let audio = "(kind = 'file' AND mime LIKE 'audio/%')";
    let pdf = "(kind = 'file' AND mime = 'application/pdf')";
    let text = format!("(kind = 'paste' OR (kind = 'file' AND (mime LIKE 'text/%' OR mime IN {TEXT_MIMES})))");
    match q.view_type.as_deref() {
        Some("image") => sql += &format!(" AND {image}"),
        Some("video") => sql += &format!(" AND {video}"),
        Some("audio") => sql += &format!(" AND {audio}"),
        Some("pdf") => sql += &format!(" AND {pdf}"),
        Some("text") => sql += &format!(" AND {text}"),
        Some("link") => sql += " AND kind = 'link'",
        Some("other") => sql += &format!(" AND kind = 'file' AND NOT {image} AND NOT {video} AND NOT {audio} AND NOT {pdf} AND NOT {text}"),
        _ => {}
    }
    if let Some(s) = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let esc = s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        sql += " AND (name LIKE ? ESCAPE '\\' OR id = ? OR target LIKE ? ESCAPE '\\')";
        args.push(Sql::Text(format!("%{esc}%")));
        args.push(Sql::Text(s.to_string()));
        args.push(Sql::Text(format!("%{esc}%")));
    }
    match q.album.as_deref() {
        Some("none") => sql += " AND album_id IS NULL",
        Some(a) if !a.is_empty() => {
            sql += " AND album_id = ?";
            args.push(Sql::Text(a.to_string()));
        }
        _ => {}
    }
    if let Some((t, id)) = q.before.as_deref().and_then(|b| b.split_once(':')).and_then(|(t, id)| Some((t.parse::<i64>().ok()?, id.to_string()))) {
        sql += " AND (created_at, id) < (?, ?)";
        args.push(Sql::Integer(t));
        args.push(Sql::Text(id));
    }
    sql += " AND (expires_at IS NULL OR expires_at > ?)";
    args.push(Sql::Integer(now()));
    sql += &format!(" ORDER BY created_at DESC, id DESC LIMIT {}", limit + 1);
    let mut items = app
        .db
        .read(move |c| {
            let mut st = c.prepare_cached(&sql)?;
            let rows = st.query_map(rusqlite::params_from_iter(args), Item::from_row)?.collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;
    let next = (items.len() > limit).then(|| {
        items.truncate(limit);
        items.last().map(|i| format!("{}:{}", i.created_at, i.id))
    });
    let views: Vec<ItemView> = items.iter().map(|i| ItemView::new(&app, i, true)).collect();
    Ok(Json(json!({ "items": views, "next": next.flatten() })))
}

async fn owned(app: &AppState, user: &User, id: String) -> AppResult<Item> {
    let item = app.db.read(move |c| Ok(Item::get(c, &id)?)).await?.ok_or_else(AppError::not_found)?;
    if item.user_id != user.id {
        let uid = item.user_id;
        let protected = app.db.read(move |c| Ok(User::by_id(c, uid)?.is_some_and(|u| u.owner))).await?;
        if !user.may_manage(uid, protected) {
            return Err(AppError::not_found());
        }
    }
    Ok(item)
}

pub async fn get_one(State(app): State<AppState>, Auth(user): Auth, Path(id): Path<String>) -> AppResult<Json<ItemView>> {
    let item = owned(&app, &user, id).await?;
    Ok(Json(ItemView::new(&app, &item, true)))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    expires: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    password: Option<Option<String>>,
    burn: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    max_views: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    album_id: Option<Option<String>>,
    target: Option<String>,
}

pub async fn update(State(app): State<AppState>, Auth(user): Auth, Path(id): Path<String>, Json(req): Json<UpdateReq>) -> AppResult<Json<ItemView>> {
    let mut item = owned(&app, &user, id).await?;
    if let Some(n) = req.name {
        let n = n.trim();
        if n.is_empty() || n.chars().count() > 200 {
            return Err(AppError::bad("Name must be 1-200 characters"));
        }
        item.name = n.to_string();
    }
    if let Some(e) = req.expires {
        item.expires_at = match e {
            Some(e) => parse_duration(&e).map_err(AppError::bad)?.map(|s| now() + s),
            None => None,
        };
    }
    if let Some(p) = req.password {
        item.password_hash = match p.filter(|p| !p.is_empty()) {
            Some(p) if p.len() > 256 => return Err(AppError::bad("Password is too long")),
            Some(p) => Some(hash_password(p).await?),
            None => None,
        };
    }
    if let Some(b) = req.burn {
        item.burn = b;
    }
    if let Some(m) = req.max_views {
        if m.is_some_and(|m| !(1..=1_000_000).contains(&m)) {
            return Err(AppError::bad("Max views must be between 1 and 1000000"));
        }
        item.max_views = m.map(|m| m + item.views);
    }
    if let Some(t) = req.target {
        if item.kind != Kind::Link {
            return Err(AppError::bad("Only links have a target"));
        }
        item.target = Some(super::upload::check_link(&t)?);
    }
    let album = req.album_id;
    let uid = item.user_id;
    let it = item.clone();
    app.db
        .write(move |c| {
            if let Some(Some(a)) = &album {
                let ok: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM albums WHERE id = ? AND user_id = ?)", rusqlite::params![a, uid], |r| r.get(0))?;
                anyhow::ensure!(ok, AlbumMissing);
            }
            c.execute(
                "UPDATE items SET name = ?2, expires_at = ?3, password_hash = ?4, burn = ?5, max_views = ?6, target = ?7, album_id = CASE WHEN ?8 THEN ?9 ELSE album_id END WHERE id = ?1",
                rusqlite::params![it.id, it.name, it.expires_at, it.password_hash, it.burn, it.max_views, it.target, album.is_some(), album.clone().flatten()],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| if e.is::<AlbumMissing>() { AppError::bad("Album not found") } else { AppError::Internal(e) })?;
    let id = item.id.clone();
    let item = app.db.read(move |c| Ok(Item::get(c, &id)?)).await?.ok_or_else(AppError::not_found)?;
    Ok(Json(ItemView::new(&app, &item, true)))
}

#[derive(Debug)]
struct AlbumMissing;
impl std::fmt::Display for AlbumMissing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("album missing")
    }
}
impl std::error::Error for AlbumMissing {}

pub async fn remove(app: &AppState, user: &User, ids: Vec<String>) -> AppResult<usize> {
    let (me, store) = (user.clone(), app.store.clone());
    let n = app
        .db
        .write(move |c| {
            let mine: Vec<String> = {
                let mut st = c.prepare_cached("SELECT i.user_id, u.owner FROM items i JOIN users u ON u.id = i.user_id WHERE i.id = ?")?;
                ids.into_iter().filter(|id| st.query_row([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?))).is_ok_and(|(o, owner)| me.may_manage(o, owner))).collect()
            };
            let n = mine.len();
            let unused = delete_items(c, &mine)?;
            store.remove(&unused);
            Ok(n)
        })
        .await?;
    Ok(n)
}

pub async fn delete_one(State(app): State<AppState>, Auth(user): Auth, Path(id): Path<String>) -> AppResult<StatusCode> {
    match remove(&app, &user, vec![id]).await? {
        0 => Err(AppError::not_found()),
        _ => Ok(StatusCode::NO_CONTENT),
    }
}

#[derive(Deserialize)]
pub struct IdsReq {
    ids: Vec<String>,
    #[serde(default)]
    album_id: Option<String>,
}

pub async fn delete_many(State(app): State<AppState>, Auth(user): Auth, Json(req): Json<IdsReq>) -> AppResult<Json<Value>> {
    if req.ids.len() > 1000 {
        return Err(AppError::bad("At most 1000 at once"));
    }
    let n = remove(&app, &user, req.ids).await?;
    Ok(Json(json!({ "deleted": n })))
}

pub async fn move_many(State(app): State<AppState>, Auth(user): Auth, Json(req): Json<IdsReq>) -> AppResult<Json<Value>> {
    let uid = user.id;
    let n = app
        .db
        .write(move |c| {
            if let Some(a) = &req.album_id {
                let ok: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM albums WHERE id = ? AND user_id = ?)", rusqlite::params![a, uid], |r| r.get(0))?;
                if !ok {
                    return Ok(None);
                }
            }
            let tx = c.transaction()?;
            let mut n = 0;
            {
                let mut st = tx.prepare_cached("UPDATE items SET album_id = ? WHERE id = ? AND user_id = ?")?;
                for id in req.ids.iter().take(1000) {
                    n += st.execute(rusqlite::params![req.album_id, id, uid])?;
                }
            }
            tx.commit()?;
            Ok(Some(n))
        })
        .await?
        .ok_or_else(|| AppError::bad("Album not found"))?;
    Ok(Json(json!({ "moved": n })))
}

pub fn unlock_cookie_name(id: &str) -> String {
    format!("up_unlock_{id}")
}

fn unlock_msg(item: &Item, exp: i64) -> String {
    let h = item.password_hash.as_deref().unwrap_or("");
    format!("unlock:{}:{exp}:{}", item.id, &h[h.len().saturating_sub(16)..])
}

pub fn is_unlocked(app: &App, jar: &CookieJar, item: &Item) -> bool {
    if !item.locked() {
        return true;
    }
    let Some(c) = jar.get(&unlock_cookie_name(&item.id)) else { return false };
    let Some((exp, sig)) = c.value().split_once('.') else { return false };
    let Ok(exp) = exp.parse::<i64>() else { return false };
    exp > now() && verify(app, &unlock_msg(item, exp), sig)
}

pub async fn public_json(app: &AppState, item: &Item, viewer: Option<&User>, unlocked: bool) -> anyhow::Result<Value> {
    let uid = item.user_id;
    let up_user = app.db.read(move |c| Ok(User::by_id(c, uid)?)).await?;
    let owner = viewer.is_some_and(|u| u.may_manage(item.user_id, up_user.as_ref().is_some_and(|x| x.owner)));
    let up = up_user.map(|u| uploader(&u));
    if item.locked() && !unlocked {
        return Ok(json!({
            "id": item.id, "kind": item.kind, "name": item.name, "type": item.view_type(), "locked": true, "unlocked": false,
            "created_at": item.created_at, "uploader": up, "owner": owner,
        }));
    }
    let mut v = ItemView::new(app, item, owner);
    v.uploader = up;
    let mut out = serde_json::to_value(&v)?;
    out["owner"] = json!(owner);
    out["unlocked"] = json!(true);
    if v.view_type == "text" && item.size <= INLINE_TEXT && !item.burn
        && let Some(sha) = &item.sha256
    {
        let bytes = tokio::fs::read(app.store.file(sha)).await.unwrap_or_default();
        out["content"] = json!(String::from_utf8_lossy(&bytes));
    }
    Ok(out)
}

pub async fn find_public(app: &AppState, id: String) -> AppResult<Item> {
    if !crate::ids::valid_lookup(&id) {
        return Err(AppError::not_found());
    }
    let item = app.db.read(move |c| Ok(Item::get(c, &id)?)).await?.ok_or_else(AppError::not_found)?;
    if item.expired(now()) {
        return Err(AppError::not_found());
    }
    Ok(item)
}

pub async fn public_get(State(app): State<AppState>, MaybeAuth(user): MaybeAuth, jar: CookieJar, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let item = find_public(&app, id).await?;
    let unlocked = is_unlocked(&app, &jar, &item) || user.as_ref().is_some_and(|u| u.id == item.user_id);
    Ok(Json(public_json(&app, &item, user.as_ref(), unlocked).await?))
}

#[derive(Deserialize)]
pub struct UnlockReq {
    password: String,
}

pub async fn unlock(State(app): State<AppState>, ClientIp(ip): ClientIp, MaybeAuth(user): MaybeAuth, jar: CookieJar, Path(id): Path<String>, Json(req): Json<UnlockReq>) -> AppResult<Response> {
    let item = find_public(&app, id).await?;
    let Some(hash) = item.password_hash.clone() else {
        return Ok(Json(public_json(&app, &item, user.as_ref(), true).await?).into_response());
    };
    if !app.limiter.hit(&format!("unlock:{ip}:{}", item.id), 10, Duration::from_secs(600)) {
        return Err(AppError::too_many());
    }
    if !verify_password(hash, req.password).await {
        return Err(AppError::forbidden("Wrong password"));
    }
    let exp = now() + 86400;
    let value = format!("{exp}.{}", sign(&app, &unlock_msg(&item, exp)));
    let cookie = Cookie::build((unlock_cookie_name(&item.id), value)).path("/").http_only(true).secure(app.cfg.secure_cookies()).same_site(SameSite::Lax).max_age(time_dur(86400)).build();
    let body = public_json(&app, &item, user.as_ref(), true).await?;
    Ok((jar.add(cookie), Json(body)).into_response())
}

#[derive(Deserialize)]
pub struct TokenReq {
    t: String,
}

pub async fn delete_with_token(State(app): State<AppState>, ClientIp(ip): ClientIp, Path(id): Path<String>, Json(req): Json<TokenReq>) -> AppResult<Json<Value>> {
    if !crate::ids::valid_lookup(&id) || req.t.len() > 64 {
        return Err(AppError::forbidden("This deletion link is not valid"));
    }
    if !app.limiter.hit(&format!("delete:{ip}"), 120, Duration::from_secs(600)) {
        return Err(AppError::too_many());
    }
    let (id2, app2) = (id.clone(), app.clone());
    let found = app
        .db
        .read(move |c| {
            let created: Option<i64> = rusqlite::OptionalExtension::optional(c.query_row("SELECT created_at FROM items WHERE id = ?", [&id2], |r| r.get(0)))?;
            let legacy_until: i64 = rusqlite::OptionalExtension::optional(c.query_row("SELECT value FROM settings WHERE key = 'delete_tokens_v2'", [], |r| r.get::<_, String>(0)))?
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            Ok(created.map(|t| (t, legacy_until)))
        })
        .await?;
    let Some((created, legacy_until)) = found else { return Err(AppError::Status(StatusCode::GONE, "Already deleted".into())) };
    let ok = crate::auth::safe_compare(&crate::auth::deletion_token(&app2, &id, created), &req.t)
        || (created < legacy_until && crate::auth::safe_compare(&crate::auth::legacy_deletion_token(&app2, &id), &req.t));
    if !ok {
        return Err(AppError::forbidden("This deletion link is not valid"));
    }
    let store = app.store.clone();
    let deleted = app
        .db
        .write(move |c| {
            let same: bool = c.query_row("SELECT EXISTS(SELECT 1 FROM items WHERE id = ? AND created_at = ?)", rusqlite::params![id, created], |r| r.get(0))?;
            if same {
                let unused = delete_items(c, std::slice::from_ref(&id))?;
                store.remove(&unused);
            }
            Ok(same)
        })
        .await?;
    if !deleted {
        return Err(AppError::Status(StatusCode::GONE, "Already deleted".into()));
    }
    Ok(Json(json!({ "deleted": true })))
}
