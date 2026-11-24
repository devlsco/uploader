use std::collections::HashMap;
use std::time::Instant;

use anyhow::Context;
use axum::Json;
use axum::extract::{FromRequest, Multipart, Path, Query, Request, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Redirect, Response};
use bytes::Bytes;
use futures_util::StreamExt;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ItemView, parse_duration};
use crate::auth::{Auth, MaybeAuth, hash_password};
use crate::db::{Item, Kind, User, now};
use crate::error::{AppError, AppResult};
use crate::state::{AppState, Chunked, UploadSlot};
use crate::storage::{Finished, is_disk_full, is_too_large};
use crate::{ids, media, storage};

pub const CHUNK_SIZE: u64 = 32 * 1024 * 1024;
const MB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Options {
    pub expires: Option<String>,
    pub password: Option<String>,
    pub burn: bool,
    pub max_views: Option<i64>,
    pub album: Option<String>,
}

impl Options {
    fn set(&mut self, key: &str, v: &str) {
        let v = v.trim();
        match key {
            "expires" | "expiry" | "expire" => self.expires = Some(v.to_string()),
            "password" => self.password = Some(v.to_string()).filter(|p| !p.is_empty()),
            "burn" | "once" => self.burn = matches!(v, "1" | "true" | "yes" | "on"),
            "max_views" | "maxviews" | "views" => self.max_views = v.parse().ok().filter(|n| *n > 0),
            "album" => self.album = Some(v.to_string()).filter(|a| !a.is_empty()),
            _ => {}
        }
    }

    fn from_request(q: &HashMap<String, String>, h: &HeaderMap) -> Options {
        let mut o = Options::default();
        for (k, v) in h {
            if let (Some(k), Ok(v)) = (k.as_str().strip_prefix("x-"), v.to_str()) {
                o.set(&k.replace('-', "_"), v);
            }
        }
        for (k, v) in q {
            o.set(k, v);
        }
        o
    }
}

pub struct Resolved {
    pub expires_at: Option<i64>,
    pub password_hash: Option<String>,
    pub burn: bool,
    pub max_views: Option<i64>,
    pub album_id: Option<String>,
}

pub async fn resolve(app: &AppState, user: &User, o: &Options) -> AppResult<Resolved> {
    let expires_at = match &o.expires {
        Some(e) => parse_duration(e).map_err(AppError::bad)?.map(|s| now() + s),
        None => None,
    };
    let password_hash = match o.password.as_deref().filter(|p| !p.is_empty()) {
        Some(p) if p.len() > 256 => return Err(AppError::bad("Password is too long")),
        Some(p) => Some(hash_password(p.to_string()).await?),
        None => None,
    };
    if let Some(m) = o.max_views
        && !(1..=1_000_000).contains(&m)
    {
        return Err(AppError::bad("Max views must be between 1 and 1000000"));
    }
    let album_id = match &o.album {
        Some(a) => {
            let (a, uid) = (a.clone(), user.id);
            let found = app.db.read(move |c| Ok(c.query_row("SELECT id FROM albums WHERE (id = ?1 OR title = ?1 COLLATE NOCASE) AND user_id = ?2", rusqlite::params![a, uid], |r| r.get::<_, String>(0)).optional()?)).await?;
            Some(found.ok_or_else(|| AppError::bad("Album not found"))?)
        }
        None => None,
    };
    Ok(Resolved { expires_at, password_hash, burn: o.burn, max_views: o.max_views, album_id })
}

fn quota_bytes(app: &AppState, user: &User) -> Option<u64> {
    let s = app.settings();
    let q = match user.quota_mb {
        Some(q) => u64::try_from(q).unwrap_or(0),
        None => s.default_quota_mb,
    };
    (q > 0).then(|| q.saturating_mul(MB))
}

fn limit(app: &AppState, user: &User) -> AppResult<u64> {
    let mut max = app.settings().max_upload_mb.saturating_mul(MB);
    if let Some(q) = quota_bytes(app, user) {
        let left = q.saturating_sub(user.used_bytes.max(0) as u64);
        if left == 0 {
            return Err(AppError::too_large("Your storage is full"));
        }
        max = max.min(left);
    }
    match app.store.space_left() {
        Some(0) => return Err(disk_full()),
        Some(left) => max = max.min(left),
        None => {}
    }
    Ok(max)
}

fn slot(app: &AppState, user: &User) -> AppResult<UploadSlot> {
    UploadSlot::take(app, user.id).ok_or_else(|| AppError::Status(axum::http::StatusCode::TOO_MANY_REQUESTS, "Too many uploads at once, wait for the others to finish".into()))
}

fn disk_full() -> AppError {
    AppError::Status(axum::http::StatusCode::INSUFFICIENT_STORAGE, "The server is out of space".into())
}

fn write_error(e: anyhow::Error) -> AppError {
    if is_too_large(&e) {
        AppError::too_large("File is too large or your storage is full")
    } else if is_disk_full(&e) {
        disk_full()
    } else {
        AppError::Internal(e)
    }
}

pub fn check_link(url: &str) -> AppResult<String> {
    let url = url.trim();
    if url.len() > 4096 {
        return Err(AppError::bad("That link is too long"));
    }
    let parsed = reqwest::Url::parse(url).map_err(|_| AppError::bad("That is not a valid link"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none_or(str::is_empty) {
        return Err(AppError::bad("Links must start with http:// or https://"));
    }
    Ok(parsed.to_string())
}

fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let n: String = base.chars().filter(|c| !c.is_control()).take(200).collect();
    let n = n.trim();
    if n.is_empty() { "upload".into() } else { n.to_string() }
}

fn header_name(h: &HeaderMap) -> Option<String> {
    if let Some(v) = h.get("x-filename").and_then(|v| v.to_str().ok()) {
        return Some(percent_encoding::percent_decode_str(v).decode_utf8_lossy().into_owned());
    }
    let cd = h.get(header::CONTENT_DISPOSITION)?.to_str().ok()?;
    let start = cd.find("filename=")? + 9;
    Some(cd[start..].trim_matches(|c| c == '"' || c == ';' || c == ' ').to_string())
}

pub async fn upload(State(app): State<AppState>, Auth(user): Auth, Query(q): Query<HashMap<String, String>>, req: Request) -> AppResult<Json<Value>> {
    let headers = req.headers().clone();
    let mut opts = Options::from_request(&q, &headers);
    let max = limit(&app, &user)?;
    let _slot = slot(&app, &user)?;
    if let Some(len) = headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok())
        && len > max + 64 * 1024
    {
        return Err(AppError::too_large("File is too large or your storage is full"));
    }
    let ct = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let (fin, name) = if ct.starts_with("multipart/form-data") {
        let mut mp = Multipart::from_request(req, &app).await.map_err(|e| AppError::bad(e.body_text()))?;
        loop {
            let Some(mut field) = mp.next_field().await.map_err(|e| AppError::bad(e.body_text()))? else {
                return Err(AppError::bad("No file in the request"));
            };
            let file_name = field.file_name().map(str::to_string);
            let field_name = field.name().unwrap_or("").to_string();
            if file_name.is_none() && !matches!(field_name.as_str(), "file" | "files" | "files[]" | "attachment" | "image" | "") {
                if let Ok(v) = field.text().await {
                    opts.set(&field_name, &v);
                }
                continue;
            }
            let mut inc = app.store.incoming(max).await.map_err(write_error)?;
            while let Some(chunk) = field.chunk().await.map_err(|e| AppError::bad(e.body_text())).transpose() {
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        inc.discard().await;
                        return Err(e);
                    }
                };
                if let Err(e) = inc.write(&chunk).await {
                    inc.discard().await;
                    return Err(write_error(e));
                }
            }
            let name = file_name.or_else(|| header_name(&headers)).or_else(|| q.get("name").cloned()).unwrap_or_else(|| "upload".into());
            break (inc.finish().await?, name);
        }
    } else {
        let mut inc = app.store.incoming(max).await.map_err(write_error)?;
        let mut body = req.into_body().into_data_stream();
        while let Some(chunk) = body.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(_) => {
                    inc.discard().await;
                    return Err(AppError::bad("Upload was interrupted"));
                }
            };
            if let Err(e) = inc.write(&chunk).await {
                inc.discard().await;
                return Err(write_error(e));
            }
        }
        let name = header_name(&headers).or_else(|| q.get("name").cloned()).unwrap_or_else(|| "upload".into());
        (inc.finish().await?, name)
    };
    if fin.size == 0 {
        let _ = tokio::fs::remove_file(&fin.path).await;
        return Err(AppError::bad("The file is empty"));
    }
    let r = match resolve(&app, &user, &opts).await {
        Ok(r) => r,
        Err(e) => {
            let _ = tokio::fs::remove_file(&fin.path).await;
            return Err(e);
        }
    };
    let item = finalize(&app, &user, fin, &name, Kind::File, None, r).await?;
    Ok(Json(upload_response(&app, &item)))
}

pub fn upload_response(app: &AppState, item: &Item) -> Value {
    let v = ItemView::new(app, item, true);
    let mut out = serde_json::to_value(&v).unwrap_or_default();
    out["link"] = json!(v.url);
    out["direct_link"] = json!(v.raw_url);
    out["deletion_link"] = json!(v.deletion_url);
    out["share_link"] = json!(if v.view_type == "image" { &v.raw_url } else { &v.url });
    out["thumbnail_link"] = json!(app.link(&format!("/{}/thumb", item.id)));
    out
}

pub async fn finalize(app: &AppState, user: &User, fin: Finished, name: &str, kind: Kind, language: Option<String>, r: Resolved) -> AppResult<Item> {
    let tmp = fin.path.clone();
    let res = finalize_inner(app, user, fin, name, kind, language, r).await;
    if res.is_err() {
        let _ = tokio::fs::remove_file(&tmp).await;
    }
    res
}

async fn finalize_inner(app: &AppState, user: &User, mut fin: Finished, name: &str, kind: Kind, language: Option<String>, r: Resolved) -> AppResult<Item> {
    let name = clean_name(name);
    let mime = if kind == Kind::Paste { "text/plain".to_string() } else { media::sniff(&fin.head, &name) };
    if kind == Kind::File && matches!(mime.as_str(), "image/jpeg" | "image/png" | "image/webp") && fin.size <= 100 * MB {
        let (path, m) = (fin.path.clone(), mime.clone());
        let _permit = app.heavy.acquire().await?;
        let stripped = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<(String, u64)>> {
            let data = Bytes::from(std::fs::read(&path)?);
            match media::strip_exif(&m, data) {
                Some(out) => {
                    std::fs::write(&path, &out)?;
                    Ok(Some(storage::hash_file(&path)?))
                }
                None => Ok(None),
            }
        })
        .await??;
        if let Some((sha, size)) = stripped {
            fin.sha = sha;
            fin.size = size;
        }
    }
    let sha = fin.sha.clone();
    let known = app
        .db
        .read(move |c| {
            Ok(c.query_row("SELECT width, height, duration, has_thumb FROM items WHERE sha256 = ? ORDER BY has_thumb DESC LIMIT 1", [&sha], |r| {
                Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?, r.get::<_, Option<f64>>(2)?, r.get::<_, bool>(3)?))
            })
            .optional()?)
        })
        .await?;
    let (width, height, duration, has_thumb) = known.unwrap_or((None, None, None, false));
    let mut item = Item {
        id: String::new(),
        user_id: user.id,
        kind,
        name,
        mime,
        size: fin.size as i64,
        sha256: Some(fin.sha.clone()),
        width,
        height,
        duration,
        language,
        target: None,
        has_thumb,
        password_hash: r.password_hash,
        expires_at: r.expires_at,
        burn: r.burn,
        max_views: r.max_views,
        views: 0,
        album_id: r.album_id,
        created_at: now(),
    };
    let (store, tmp, id_len, quota) = (app.store.clone(), fin.path.clone(), crate::config::ID_LENGTH, quota_bytes(app, user));
    let item = app
        .db
        .write(move |c| {
            // check again under the lock, parallel uploads
            if let Some(q) = quota {
                let used: i64 = c.query_row("SELECT used_bytes FROM users WHERE id = ?", [item.user_id], |r| r.get(0))?;
                if used.max(0) as u64 + item.size.max(0) as u64 > q {
                    return Ok(None);
                }
            }
            store.commit(&tmp, item.sha256.as_deref().unwrap_or_default()).context("store file")?;
            insert_with_new_id(c, &mut item, id_len)?;
            Ok(Some(item))
        })
        .await?
        .ok_or_else(|| AppError::too_large("Your storage is full"))?;
    if known.is_none() && matches!(item.view_type(), "image" | "video" | "audio") && item.mime != "image/svg+xml" {
        spawn_thumbnail(app.clone(), fin.sha, item.mime.clone());
    }
    Ok(item)
}

fn insert_with_new_id(c: &rusqlite::Connection, item: &mut Item, len: usize) -> anyhow::Result<()> {
    for attempt in 0..20 {
        item.id = ids::new_id(len + attempt / 5);
        match item.insert(c) {
            Ok(_) => return Ok(()),
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => continue,
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("no free id")
}

pub fn spawn_thumbnail(app: AppState, sha: String, mime: String) {
    tokio::spawn(async move {
        let Ok(_permit) = app.heavy.acquire().await else { return };
        let (src, dest) = (app.store.file(&sha), app.store.thumb(&sha));
        let res = tokio::task::spawn_blocking(move || media::thumbnail(&src, &mime, &dest)).await;
        match res {
            Ok(Ok(t)) => {
                let _ = app
                    .db
                    .write(move |c| {
                        c.execute(
                            "UPDATE items SET width = COALESCE(?1, width), height = COALESCE(?2, height), duration = COALESCE(?3, duration), has_thumb = ?4 WHERE sha256 = ?5",
                            rusqlite::params![t.width, t.height, t.duration, t.made, sha],
                        )?;
                        Ok(())
                    })
                    .await;
            }
            Ok(Err(e)) => tracing::debug!("thumbnail for {sha}: {e:#}"),
            Err(e) => tracing::warn!("thumbnail task: {e}"),
        }
    });
}

#[derive(Deserialize)]
pub struct StartReq {
    name: String,
    size: u64,
    #[serde(flatten)]
    opts: Options,
}

pub async fn start_chunked(State(app): State<AppState>, Auth(user): Auth, Json(req): Json<StartReq>) -> AppResult<Json<Value>> {
    let max = limit(&app, &user)?;
    if req.size == 0 {
        return Err(AppError::bad("The file is empty"));
    }
    let slot = slot(&app, &user)?;
    if req.size > max {
        return Err(AppError::too_large("File is too large or your storage is full"));
    }
    resolve(&app, &user, &Options { password: None, ..req.opts.clone() }).await?;
    let path = app.store.tmp();
    tokio::fs::File::create(&path).await?;
    let id = crate::auth::random_token(16);
    let state = Chunked { user_id: user.id, _slot: slot, name: req.name, total: req.size, opts: req.opts, path, hasher: Default::default(), size: 0, head: Vec::new(), touched: Instant::now() };
    app.chunks.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), state);
    Ok(Json(json!({ "id": id, "chunk_size": CHUNK_SIZE })))
}

fn take(app: &AppState, id: &str, user: &User) -> AppResult<Chunked> {
    let mut m = app.chunks.lock().unwrap_or_else(|e| e.into_inner());
    match m.get(id) {
        Some(c) if c.user_id == user.id => Ok(m.remove(id).expect("exists")),
        _ => Err(AppError::bad("Upload not found or busy, start again")),
    }
}

#[derive(Deserialize)]
pub struct ChunkQuery {
    offset: u64,
}

pub async fn chunk(State(app): State<AppState>, Auth(user): Auth, Path(id): Path<String>, Query(q): Query<ChunkQuery>, req: Request) -> AppResult<Json<Value>> {
    let mut st = take(&app, &id, &user)?;
    if q.offset != st.size {
        let size = st.size;
        put_back(&app, id, st);
        return Err(AppError::conflict(format!("Expected offset {size}")));
    }
    let mut inc = app.store.resume(st.path.clone(), std::mem::take(&mut st.hasher), st.size, std::mem::take(&mut st.head), st.total).await?;
    let mut body = req.into_body().into_data_stream();
    let mut written = 0u64;
    while let Some(c) = body.next().await {
        let ok = match c {
            Ok(c) => {
                written += c.len() as u64;
                if written > CHUNK_SIZE + MB { Err(AppError::too_large("Part is too large")) } else { inc.write(&c).await.map_err(write_error) }
            }
            Err(_) => Err(AppError::bad("Upload was interrupted")),
        };
        if let Err(e) = ok {
            inc.discard().await;
            return Err(e);
        }
    }
    let (path, hasher, size, head) = inc.pause().await?;
    st.path = path;
    st.hasher = hasher;
    st.size = size;
    st.head = head;
    st.touched = Instant::now();
    put_back(&app, id, st);
    Ok(Json(json!({ "size": size })))
}

fn put_back(app: &AppState, id: String, st: Chunked) {
    app.chunks.lock().unwrap_or_else(|e| e.into_inner()).insert(id, st);
}

pub async fn complete_chunked(State(app): State<AppState>, Auth(user): Auth, Path(id): Path<String>) -> AppResult<Json<Value>> {
    let st = take(&app, &id, &user)?;
    if st.size != st.total {
        let msg = format!("Only {} of {} bytes arrived", st.size, st.total);
        put_back(&app, id, st);
        return Err(AppError::bad(msg));
    }
    let inc = app.store.resume(st.path.clone(), st.hasher, st.size, st.head, st.total).await?;
    let fin = inc.finish().await?;
    let r = match resolve(&app, &user, &st.opts).await {
        Ok(r) => r,
        Err(e) => {
            let _ = tokio::fs::remove_file(&fin.path).await;
            return Err(e);
        }
    };
    let item = finalize(&app, &user, fin, &st.name, Kind::File, None, r).await?;
    Ok(Json(upload_response(&app, &item)))
}

#[derive(Deserialize)]
pub struct PasteReq {
    content: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(flatten)]
    opts: Options,
}

pub async fn paste(State(app): State<AppState>, Auth(user): Auth, Json(req): Json<PasteReq>) -> AppResult<Json<Value>> {
    let _slot = slot(&app, &user)?;
    let item = create_paste(&app, &user, &req.content, req.title, req.language, &req.opts).await?;
    Ok(Json(upload_response(&app, &item)))
}

pub async fn create_paste(app: &AppState, user: &User, content: &str, title: Option<String>, language: Option<String>, opts: &Options) -> AppResult<Item> {
    if content.trim().is_empty() {
        return Err(AppError::bad("The paste is empty"));
    }
    let language = language.map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty() && l != "auto");
    if let Some(l) = &language
        && (l.len() > 24 || !l.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '#' | '-' | '_')))
    {
        return Err(AppError::bad("Unknown language"));
    }
    let max = limit(app, user)?;
    let r = resolve(app, user, opts).await?;
    let mut inc = app.store.incoming(max).await.map_err(write_error)?;
    if let Err(e) = inc.write(content.as_bytes()).await {
        inc.discard().await;
        return Err(write_error(e));
    }
    let fin = inc.finish().await?;
    let title = title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| "Paste".into());
    finalize(app, user, fin, &title, Kind::Paste, language, r).await
}

#[derive(Deserialize)]
pub struct LinkReq {
    url: String,
    #[serde(default)]
    slug: Option<String>,
    #[serde(flatten)]
    opts: Options,
}

pub async fn link(State(app): State<AppState>, Auth(user): Auth, Json(req): Json<LinkReq>) -> AppResult<Json<Value>> {
    let item = create_link(&app, &user, &req.url, req.slug, &req.opts).await?;
    Ok(Json(upload_response(&app, &item)))
}

pub async fn create_link(app: &AppState, user: &User, url: &str, slug: Option<String>, opts: &Options) -> AppResult<Item> {
    let url = check_link(url)?;
    let parsed = reqwest::Url::parse(&url).map_err(|_| AppError::bad("That is not a valid link"))?;
    let slug = slug.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if let Some(s) = &slug
        && !ids::valid_custom(s)
    {
        return Err(AppError::bad("Custom name: 3-48 letters, numbers, - and _"));
    }
    let r = resolve(app, user, opts).await?;
    let mut item = Item {
        id: slug.clone().unwrap_or_default(),
        user_id: user.id,
        kind: Kind::Link,
        name: parsed.host_str().unwrap_or("link").trim_start_matches("www.").to_string(),
        mime: String::new(),
        size: 0,
        sha256: None,
        width: None,
        height: None,
        duration: None,
        language: None,
        target: Some(url),
        has_thumb: false,
        password_hash: r.password_hash,
        expires_at: r.expires_at,
        burn: r.burn,
        max_views: r.max_views,
        views: 0,
        album_id: None,
        created_at: now(),
    };
    let id_len = crate::config::ID_LENGTH;
    app.db
        .write(move |c| {
            if slug.is_some() {
                match item.insert(c) {
                    Ok(_) => Ok(Ok(item)),
                    Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => Ok(Err(())),
                    Err(e) => Err(e.into()),
                }
            } else {
                insert_with_new_id(c, &mut item, id_len)?;
                Ok(Ok(item))
            }
        })
        .await?
        .map_err(|_| AppError::conflict("That name is taken"))
}

pub async fn share(State(app): State<AppState>, MaybeAuth(user): MaybeAuth, req: Request) -> Response {
    let Some(user) = user else { return Redirect::to("/login").into_response() };
    match share_inner(&app, &user, req).await {
        Ok(to) => Redirect::to(&to).into_response(),
        Err(AppError::Status(_, msg)) => Redirect::to(&format!("/?error={}", percent_encoding::utf8_percent_encode(&msg, percent_encoding::NON_ALPHANUMERIC))).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn share_inner(app: &AppState, user: &User, req: Request) -> AppResult<String> {
    let max = limit(app, user)?;
    let _slot = slot(app, user)?;
    let mut mp = Multipart::from_request(req, app).await.map_err(|e| AppError::bad(e.body_text()))?;
    let (mut title, mut text, mut url) = (None, None, None);
    let mut items = Vec::new();
    while let Some(mut field) = mp.next_field().await.map_err(|e| AppError::bad(e.body_text()))? {
        let name = field.name().unwrap_or("").to_string();
        match field.file_name().map(str::to_string) {
            Some(file) if !file.is_empty() => {
                let mut inc = app.store.incoming(max).await.map_err(write_error)?;
                while let Some(chunk) = field.chunk().await.map_err(|e| AppError::bad(e.body_text())).transpose() {
                    let res = match chunk {
                        Ok(c) => inc.write(&c).await.map_err(write_error),
                        Err(e) => Err(e),
                    };
                    if let Err(e) = res {
                        inc.discard().await;
                        return Err(e);
                    }
                }
                let fin = inc.finish().await?;
                if fin.size == 0 {
                    let _ = tokio::fs::remove_file(&fin.path).await;
                    continue;
                }
                let r = resolve(app, user, &Options::default()).await?;
                items.push(finalize(app, user, fin, &file, Kind::File, None, r).await?);
            }
            Some(_) => {}
            None => {
                let v = field.text().await.unwrap_or_default();
                match name.as_str() {
                    "title" => title = Some(v),
                    "text" => text = Some(v),
                    "url" => url = Some(v),
                    _ => {}
                }
            }
        }
    }
    if items.is_empty() {
        let is_url = |s: &String| !s.contains(char::is_whitespace) && (s.starts_with("http://") || s.starts_with("https://"));
        let text = text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        let link = url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).or_else(|| text.clone().filter(is_url));
        if let Some(l) = link {
            items.push(create_link(app, user, &l, None, &Options::default()).await?);
        } else if let Some(t) = text {
            items.push(create_paste(app, user, &t, title, None, &Options::default()).await?);
        }
    }
    Ok(match items.len() {
        0 => return Err(AppError::bad("Nothing to share")),
        1 => format!("/{}", items[0].id),
        _ => "/files".into(),
    })
}
