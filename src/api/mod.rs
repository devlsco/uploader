pub mod account;
pub mod admin;
pub mod albums;
pub mod items;
pub mod stats;
pub mod upload;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, get, patch, post, put};
use serde::{Deserialize, Deserializer, Serialize};

use crate::db::{Item, Kind, User};
use crate::media;
use crate::state::{App, AppState};

pub fn router() -> Router<AppState> {
    let uploads = Router::new()
        .route("/upload", post(upload::upload))

        .route("/uploads/{id}", put(upload::chunk))
        .route("/share", post(upload::share))
        .layer(DefaultBodyLimit::disable());
    Router::new()
        .merge(uploads)
        .route("/instance", get(account::instance))
        .route("/setup", post(account::setup))
        .route("/setup/check", post(account::setup_check))
        .route("/auth/login", post(account::login))
        .route("/auth/register", post(account::register))
        .route("/auth/logout", post(account::logout))
        .route("/auth/twitch", get(account::twitch_start))
        .route("/auth/twitch/callback", get(account::twitch_callback))
        .route("/me", get(account::me).patch(account::update_me))
        .route("/me/twitch", delete(account::unlink_twitch))
        .route("/me/sessions", delete(account::logout_everywhere))
        .route("/keys", get(account::keys).post(account::create_key))
        .route("/keys/{id}", delete(account::delete_key))
        .route("/uploads", post(upload::start_chunked))
        .route("/uploads/{id}/complete", post(upload::complete_chunked))
        .route("/paste", post(upload::paste).layer(DefaultBodyLimit::max(16 << 20)))
        .route("/links", post(upload::link))
        .route("/items", get(items::list))
        .route("/items/delete", post(items::delete_many))
        .route("/items/album", post(items::move_many))
        .route("/items/{id}", get(items::get_one).patch(items::update).delete(items::delete_one))
        .route("/albums", get(albums::list).post(albums::create))
        .route("/albums/{id}", patch(albums::update).delete(albums::remove))
        .route("/stats", get(stats::stats))
        .route("/public/items/{id}", get(items::public_get))
        .route("/public/items/{id}/unlock", post(items::unlock))
        .route("/public/items/{id}/delete", post(items::delete_with_token))
        .route("/public/albums/{id}", get(albums::public_get))
        .route("/admin/users", get(admin::users))
        .route("/invites/{code}", get(account::invite_info))
        .route("/admin/users/{id}", patch(admin::update_user).delete(admin::delete_user))
        .route("/admin/users/{id}/password", post(admin::reset_password))
        .route("/admin/invites", get(admin::invites).post(admin::create_invite))
        .route("/admin/invites/{code}", delete(admin::delete_invite))
        .route("/admin/settings", get(admin::get_settings).put(admin::put_settings))
        .route("/admin/version", get(admin::version))
        .route("/admin/update", post(admin::update))
        .fallback(|| async { crate::error::AppError::not_found() })
}

#[derive(Debug, Serialize)]
pub struct ItemView {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub mime: String,
    #[serde(rename = "type")]
    pub view_type: &'static str,
    pub size: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration: Option<f64>,
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub thumb: bool,
    pub locked: bool,
    pub expires_at: Option<i64>,
    pub burn: bool,
    pub max_views: Option<i64>,
    pub views: i64,
    pub album_id: Option<String>,
    pub created_at: i64,
    pub url: String,
    pub raw_url: String,
    pub thumb_url: Option<String>,
    pub download_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deletion_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploader: Option<Uploader>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Uploader {
    pub name: String,
    pub avatar_url: Option<String>,
}

impl ItemView {
    pub fn new(app: &App, i: &Item, owner: bool) -> ItemView {
        let ext = media::extension(&i.mime, &i.name);
        let thumb = i.has_thumb && (owner || (!i.burn && i.max_views.is_none()));
        ItemView {
            id: i.id.clone(),
            kind: i.kind,
            name: i.name.clone(),
            mime: i.mime.clone(),
            view_type: i.view_type(),
            size: i.size,
            width: i.width,
            height: i.height,
            duration: i.duration,
            language: i.language.clone(),
            target: if owner { i.target.clone() } else { None },
            thumb,
            locked: i.locked(),
            expires_at: i.expires_at,
            burn: i.burn,
            max_views: i.max_views,
            views: i.views,
            album_id: if owner { i.album_id.clone() } else { None },
            created_at: i.created_at,
            url: app.link(&format!("/{}", i.id)),
            raw_url: if i.kind == Kind::Link { app.link(&format!("/{}", i.id)) } else { app.link(&format!("/{}.{ext}", i.id)) },
            thumb_url: thumb.then(|| app.link(&format!("/{}/thumb", i.id))),
            download_url: app.link(&format!("/{}/download", i.id)),
            deletion_url: owner.then(|| app.link(&format!("/d/{}?t={}", i.id, crate::auth::deletion_token(app, &i.id, i.created_at)))),
            uploader: None,
        }
    }
}

pub fn uploader(u: &User) -> Uploader {
    Uploader { name: u.username.clone(), avatar_url: u.avatar_url.clone() }
}

pub fn parse_duration(s: &str) -> Result<Option<i64>, String> {
    let s = s.trim().to_lowercase();
    if s.is_empty() || s == "never" || s == "0" {
        return Ok(None);
    }
    let (num, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: i64 = num.parse().map_err(|_| "Expiry must look like 1h, 7d or never".to_string())?;
    let mul = match unit.trim() {
        "" | "s" => 1,
        "m" | "min" => 60,
        "h" => 3600,
        "d" => 86400,
        "w" => 7 * 86400,
        "mo" => 30 * 86400,
        "y" => 365 * 86400,
        _ => return Err("Expiry must look like 1h, 7d or never".into()),
    };
    let secs = n.checked_mul(mul).filter(|v| *v > 0 && *v <= 10 * 365 * 86400).ok_or("Expiry is out of range")?;
    Ok(Some(secs))
}

pub fn double_option<'de, T: Deserialize<'de>, D: Deserializer<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("never"), Ok(None));
        assert_eq!(parse_duration("1h"), Ok(Some(3600)));
        assert_eq!(parse_duration("7d"), Ok(Some(7 * 86400)));
        assert_eq!(parse_duration("90"), Ok(Some(90)));
        assert!(parse_duration("5x").is_err());
        assert!(parse_duration("999999y").is_err());
    }
}
