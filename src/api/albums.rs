use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum_extra::extract::CookieJar;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{ItemView, uploader};
use crate::auth::{Auth, MaybeAuth};
use crate::db::{Item, User, now};
use crate::error::{AppError, AppResult};
use crate::ids;
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct Album {
    id: String,
    title: String,
    description: String,
    public: bool,
    created_at: i64,
    count: i64,
    size: i64,
    covers: Vec<String>,
    url: String,
}

pub async fn list(State(app): State<AppState>, Auth(user): Auth) -> AppResult<Json<Vec<Album>>> {
    let uid = user.id;
    let rows = app
        .db
        .read(move |c| {
            let mut st = c.prepare_cached(
                "SELECT a.id, a.title, a.description, a.public, a.created_at,
                        (SELECT COUNT(*) FROM items i WHERE i.album_id = a.id),
                        (SELECT COALESCE(SUM(size), 0) FROM items i WHERE i.album_id = a.id),
                        (SELECT group_concat(id) FROM (SELECT id FROM items i WHERE i.album_id = a.id AND has_thumb = 1 AND password_hash IS NULL ORDER BY created_at DESC LIMIT 4))
                 FROM albums a WHERE a.user_id = ? ORDER BY a.created_at DESC",
            )?;
            let rows = st
                .query_map([uid], |r| {
                    Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get::<_, Option<String>>(7)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, title, description, public, created_at, count, size, covers)| Album {
                url: app.link(&format!("/a/{id}")),
                id,
                title,
                description,
                public,
                created_at,
                count,
                size,
                covers: covers.map(|c| c.split(',').map(str::to_string).collect()).unwrap_or_default(),
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct AlbumReq {
    title: Option<String>,
    description: Option<String>,
    public: Option<bool>,
}

fn clean_title(t: &str) -> AppResult<String> {
    let t = t.trim();
    if t.is_empty() || t.chars().count() > 80 {
        return Err(AppError::bad("Title must be 1-80 characters"));
    }
    Ok(t.to_string())
}

pub async fn create(State(app): State<AppState>, Auth(user): Auth, Json(req): Json<AlbumReq>) -> AppResult<Json<Value>> {
    let title = clean_title(req.title.as_deref().unwrap_or(""))?;
    let description: String = req.description.unwrap_or_default().trim().chars().take(500).collect();
    let public = req.public.unwrap_or(true);
    let (uid, len) = (user.id, 8);
    let id = app
        .db
        .write(move |c| {
            for _ in 0..10 {
                let id = ids::new_id(len);
                let n = c.execute(
                    "INSERT OR IGNORE INTO albums (id, user_id, title, description, public, created_at) VALUES (?, ?, ?, ?, ?, ?)",
                    rusqlite::params![id, uid, title, description, public, now()],
                )?;
                if n == 1 {
                    return Ok(id);
                }
            }
            anyhow::bail!("no free album id")
        })
        .await?;
    Ok(Json(json!({ "id": id, "url": app.link(&format!("/a/{id}")) })))
}

pub async fn update(State(app): State<AppState>, Auth(user): Auth, Path(id): Path<String>, Json(req): Json<AlbumReq>) -> AppResult<StatusCode> {
    let title = req.title.as_deref().map(clean_title).transpose()?;
    let description = req.description.map(|d| d.trim().chars().take(500).collect::<String>());
    let uid = user.id;
    let n = app
        .db
        .write(move |c| {
            Ok(c.execute(
                "UPDATE albums SET title = COALESCE(?, title), description = COALESCE(?, description), public = COALESCE(?, public) WHERE id = ? AND user_id = ?",
                rusqlite::params![title, description, req.public, id, uid],
            )?)
        })
        .await?;
    if n == 0 { Err(AppError::not_found()) } else { Ok(StatusCode::NO_CONTENT) }
}

pub async fn remove(State(app): State<AppState>, Auth(user): Auth, Path(id): Path<String>) -> AppResult<StatusCode> {
    let uid = user.id;
    let n = app.db.write(move |c| Ok(c.execute("DELETE FROM albums WHERE id = ? AND user_id = ?", rusqlite::params![id, uid])?)).await?;
    if n == 0 { Err(AppError::not_found()) } else { Ok(StatusCode::NO_CONTENT) }
}

pub async fn public_json(app: &AppState, id: String, viewer: Option<&User>, _jar: &CookieJar) -> AppResult<Value> {
    if !ids::valid_lookup(&id) {
        return Err(AppError::not_found());
    }
    let t = now();
    let found = app
        .db
        .read(move |c| {
            let a: Option<(String, String, String, bool, i64, i64)> = c
                .query_row("SELECT id, title, description, public, created_at, user_id FROM albums WHERE id = ?", [&id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
                .optional()?;
            let Some(a) = a else { return Ok(None) };
            let mut st = c.prepare_cached(&format!("SELECT {} FROM items WHERE album_id = ? AND (expires_at IS NULL OR expires_at > ?) ORDER BY created_at DESC LIMIT 1000", Item::COLS))?;
            let items = st.query_map(rusqlite::params![a.0, t], Item::from_row)?.collect::<Result<Vec<_>, _>>()?;
            let owner = User::by_id(c, a.5)?;
            Ok(Some((a, items, owner)))
        })
        .await?;
    let Some(((id, title, description, public, created_at, owner_id), items, owner)) = found else { return Err(AppError::not_found()) };
    let is_owner = viewer.is_some_and(|u| u.may_manage(owner_id, owner.as_ref().is_some_and(|o| o.owner)));
    if !public && !is_owner {
        return Err(AppError::not_found());
    }
    let items: Vec<Value> = items
        .iter()
        .filter(|i| !i.burn && i.max_views.is_none())
        .map(|i| {
            let mut v = ItemView::new(app, i, false);
            if i.locked() {
                v.thumb_url = None;
                v.thumb = false;
            }
            serde_json::to_value(v).unwrap_or_default()
        })
        .collect();
    Ok(json!({
        "id": id, "title": title, "description": description, "public": public, "created_at": created_at,
        "items": items, "owner": is_owner, "uploader": owner.as_ref().map(uploader),
        "url": app.link(&format!("/a/{id}")),
    }))
}

pub async fn public_get(State(app): State<AppState>, MaybeAuth(user): MaybeAuth, jar: CookieJar, Path(id): Path<String>) -> AppResult<Json<Value>> {
    Ok(Json(public_json(&app, id, user.as_ref(), &jar).await?))
}
