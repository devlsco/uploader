use std::net::{IpAddr, SocketAddr};

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, header};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use hmac::{Hmac, Mac};
use rand::RngCore;
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};

use crate::config::TrustProxy;
use crate::db::{User, now};
use crate::error::AppError;
use crate::state::{App, AppState};

pub const SESSION_COOKIE: &str = "up_session";
const SESSION_DAYS: i64 = 30;

pub fn random_token(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut b);
    B64.encode(b)
}

pub fn sha256(s: &str) -> Vec<u8> {
    Sha256::digest(s.as_bytes()).to_vec()
}

pub async fn hash_password(pw: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
        Argon2::default().hash_password(pw.as_bytes(), &salt).map(|h| h.to_string()).map_err(|e| anyhow::anyhow!("hash: {e}"))
    })
    .await?
}

pub async fn verify_password(hash: String, pw: String) -> bool {
    tokio::task::spawn_blocking(move || PasswordHash::new(&hash).is_ok_and(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())).await.unwrap_or(false)
}

pub async fn dummy_hash() -> String {
    static DUMMY: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    DUMMY.get_or_init(|| async { hash_password(random_token(16)).await.unwrap_or_default() }).await.clone()
}

pub fn check_password(pw: &str) -> Result<(), AppError> {
    if pw.chars().count() < 8 {
        return Err(AppError::bad("Password needs at least 8 characters"));
    }
    if pw.len() > 256 {
        return Err(AppError::bad("Password is too long"));
    }
    Ok(())
}

pub fn check_username(name: &str) -> Result<String, AppError> {
    let n = name.trim();
    if !(2..=32).contains(&n.chars().count()) || !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
        return Err(AppError::bad("Username: 2-32 letters, numbers, _ - ."));
    }
    Ok(n.to_string())
}

pub fn sign(app: &App, msg: &str) -> String {
    let mut m = Hmac::<Sha256>::new_from_slice(&app.cfg.secret).expect("any key size");
    m.update(msg.as_bytes());
    B64.encode(&m.finalize().into_bytes()[..18])
}

pub fn verify(app: &App, msg: &str, sig: &str) -> bool {
    safe_compare(&sign(app, msg), sig)
}

pub fn safe_compare(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub fn deletion_token(app: &App, id: &str, created_at: i64) -> String {
    sign(app, &format!("delete:{id}:{created_at}"))
}

pub fn legacy_deletion_token(app: &App, id: &str) -> String {
    sign(app, &format!("delete:{id}"))
}

pub async fn create_session(app: &AppState, user_id: i64) -> anyhow::Result<Cookie<'static>> {
    let token = random_token(32);
    let hash = sha256(&token);
    let t = now();
    app.db
        .write(move |c| {
            c.execute("INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)", rusqlite::params![hash, user_id, t, t + SESSION_DAYS * 86400])?;
            Ok(())
        })
        .await?;
    Ok(session_cookie(app, token, SESSION_DAYS * 86400))
}

pub fn session_cookie(app: &App, value: String, max_age: i64) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, value))
        .path("/")
        .http_only(true)
        .secure(app.cfg.secure_cookies())
        .same_site(SameSite::Lax)
        .max_age(time_dur(max_age))
        .build()
}

pub fn time_dur(secs: i64) -> time::Duration {
    time::Duration::seconds(secs)
}

fn api_key(h: &HeaderMap) -> Option<String> {
    let v = h.get(header::AUTHORIZATION).or_else(|| h.get("x-api-key"))?.to_str().ok()?.trim();
    let v = v.strip_prefix("Bearer ").or_else(|| v.strip_prefix("bearer ")).unwrap_or(v).trim();
    (v.starts_with("upk_") && v.len() < 200).then(|| v.to_string())
}

fn same_origin(app: &App, parts: &Parts) -> bool {
    if matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return true;
    }
    let h = &parts.headers;
    if h.get("sec-fetch-site").and_then(|v| v.to_str().ok()).is_some_and(|v| v == "same-origin" || v == "none") {
        return true;
    }
    match h.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(o) => o == app.cfg.url,
        None => true,
    }
}

async fn user_from_parts(parts: &mut Parts, app: &AppState) -> Result<Option<User>, AppError> {
    if let Some(key) = api_key(&parts.headers) {
        let hash = sha256(&key);
        let found = app
            .db
            .read(move |c| {
                let row: Option<(i64, i64, Option<i64>)> = c.query_row("SELECT id, user_id, last_used_at FROM api_keys WHERE key_hash = ?", [&hash], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
                Ok(match row {
                    Some((kid, uid, last)) => User::by_id(c, uid)?.map(|u| (kid, last, u)),
                    None => None,
                })
            })
            .await?;
        let Some((kid, last, user)) = found else { return Err(AppError::Status(axum::http::StatusCode::UNAUTHORIZED, "Invalid API key".into())) };
        let t = now();
        if last.is_none_or(|l| t - l > 300) {
            let db = app.db.clone();
            tokio::spawn(async move {
                let _ = db.write(move |c| Ok(c.execute("UPDATE api_keys SET last_used_at = ? WHERE id = ?", [t, kid])?)).await;
            });
        }
        return active(user).map(Some);
    }
    let jar = CookieJar::from_headers(&parts.headers);
    let Some(token) = jar.get(SESSION_COOKIE).map(|c| c.value().to_string()) else { return Ok(None) };
    if !same_origin(app, parts) {
        return Err(AppError::forbidden("Request from another site"));
    }
    let hash = sha256(&token);
    let t = now();
    let user = app
        .db
        .read(move |c| {
            let uid: Option<i64> = c.query_row("SELECT user_id FROM sessions WHERE token_hash = ? AND expires_at > ?", rusqlite::params![hash, t], |r| r.get(0)).optional()?;
            Ok(match uid {
                Some(uid) => User::by_id(c, uid)?,
                None => None,
            })
        })
        .await?;
    match user {
        Some(u) => active(u).map(Some),
        None => Ok(None),
    }
}

fn active(u: User) -> Result<User, AppError> {
    if u.disabled { Err(AppError::forbidden("This account is disabled")) } else { Ok(u) }
}

pub struct Auth(pub User);

pub struct MaybeAuth(pub Option<User>);

pub struct Admin(pub User);

impl FromRequestParts<AppState> for Auth {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, app: &AppState) -> Result<Self, Self::Rejection> {
        user_from_parts(parts, app).await?.map(Auth).ok_or_else(AppError::unauthorized)
    }
}

impl FromRequestParts<AppState> for MaybeAuth {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, app: &AppState) -> Result<Self, Self::Rejection> {
        Ok(MaybeAuth(user_from_parts(parts, app).await.unwrap_or(None)))
    }
}

impl FromRequestParts<AppState> for Admin {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, app: &AppState) -> Result<Self, Self::Rejection> {
        let Auth(u) = Auth::from_request_parts(parts, app).await?;
        if !u.is_admin() {
            return Err(AppError::forbidden("Admins only"));
        }
        Ok(Admin(u))
    }
}

pub struct ClientIp(pub IpAddr);

static PROXY_HINT: std::sync::Once = std::sync::Once::new();

fn from_proxy(peer: IpAddr) -> bool {
    match peer {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback() || v4.is_private()) || (v6.segments()[0] & 0xfe00) == 0xfc00,
    }
}

pub fn client_ip(mode: TrustProxy, peer: IpAddr, h: &HeaderMap) -> IpAddr {
    let header_ip = || -> Option<IpAddr> {
        match mode {
            TrustProxy::Off => None,
            TrustProxy::Cloudflare => h.get("cf-connecting-ip")?.to_str().ok()?.trim().parse().ok(),
            TrustProxy::Forwarded => h.get_all("x-forwarded-for").iter().last()?.to_str().ok()?.rsplit(',').next()?.trim().parse().ok(),
        }
    };
    let ip = if from_proxy(peer) { header_ip().unwrap_or(peer) } else { peer };
    normalize(ip)
}

fn normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let s = v6.segments();
                IpAddr::V6(std::net::Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
            }
        },
        v4 => v4,
    }
}

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, app: &AppState) -> Result<Self, Self::Rejection> {
        let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip()).unwrap_or(IpAddr::from([127, 0, 0, 1]));
        if app.cfg.trust_proxy == TrustProxy::Off && peer.is_loopback() && (parts.headers.contains_key("x-forwarded-for") || parts.headers.contains_key("cf-connecting-ip")) {
            PROXY_HINT.call_once(|| {
                tracing::warn!("Requests come through a proxy, but UPLOADER_TRUST_PROXY is not set: all visitors share one address and login limits hit everyone. Set UPLOADER_TRUST_PROXY=true (or cloudflare).");
            });
        }
        Ok(ClientIp(client_ip(app.cfg.trust_proxy, peer, &parts.headers)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(*k, v.parse().unwrap());
        }
        h
    }

    #[test]
    fn client_ip_from_proxy() {
        let local: IpAddr = "127.0.0.1".parse().unwrap();
        let public: IpAddr = "203.0.113.9".parse().unwrap();
        let h = headers(&[("x-forwarded-for", "1.1.1.1, 198.51.100.7"), ("cf-connecting-ip", "1.1.1.1")]);
        assert_eq!(client_ip(TrustProxy::Forwarded, local, &h), "198.51.100.7".parse::<IpAddr>().unwrap());
        assert_eq!(client_ip(TrustProxy::Cloudflare, local, &h), "1.1.1.1".parse::<IpAddr>().unwrap());
        assert_eq!(client_ip(TrustProxy::Forwarded, public, &h), public);
        assert_eq!(client_ip(TrustProxy::Off, local, &h), local);
        assert_eq!(client_ip(TrustProxy::Forwarded, local, &headers(&[("x-forwarded-for", "nonsense")])), local);
    }

    #[test]
    fn ipv6_counts_per_network() {
        let a = client_ip(TrustProxy::Off, "2001:db8:1:2:aaaa::1".parse().unwrap(), &HeaderMap::new());
        let b = client_ip(TrustProxy::Off, "2001:db8:1:2:bbbb::2".parse().unwrap(), &HeaderMap::new());
        assert_eq!(a, b);
    }

    #[test]
    fn signatures() {
        assert!(safe_compare("abc", "abc"));
        assert!(!safe_compare("abc", "abd"));
        assert!(!safe_compare("abc", "ab"));
    }
}
