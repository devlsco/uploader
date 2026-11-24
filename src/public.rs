use std::borrow::Cow;

use axum::body::Body;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use rust_embed::RustEmbed;
use serde_json::Value;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::api::items::{find_public, is_unlocked, public_json};
use crate::auth::MaybeAuth;
use crate::db::{Item, Kind, User, delete_items};
use crate::error::AppError;
use crate::config::APP_NAME;
use crate::state::AppState;

#[derive(RustEmbed)]
#[folder = "web/out"]
#[allow_missing = true]
struct Assets;

#[derive(Clone, Copy)]
pub struct NoCompress;

const PAGE_CSP: &str = "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https://static-cdn.jtvnw.net; media-src 'self' blob:; frame-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";

pub async fn serve(State(app): State<AppState>, jar: CookieJar, req: Request) -> Response {
    let path = req.uri().path().to_string();
    let (mut parts, body) = req.into_parts();
    let user = if path.starts_with("/_next/") { None } else { MaybeAuth::from_request_parts(&mut parts, &app).await.map(|m| m.0).unwrap_or(None) };
    let req = Request::from_parts(parts, body);
    match route(&app, user, &jar, &path, req).await {
        Ok(r) => r,
        Err(AppError::Status(StatusCode::NOT_FOUND, _)) => not_found(),
        Err(e) => e.into_response(),
    }
}

async fn route(app: &AppState, user: Option<User>, jar: &CookieJar, path: &str, req: Request) -> Result<Response, AppError> {
    if path == "/health" {
        return Ok("ok".into_response());
    }
    let headers = req.headers().clone();
    if let Some(to) = gate(app, user.as_ref(), path, req.uri().query()) {
        let mut r = Redirect::to(&to).into_response();
        r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        return Ok(r);
    }
    if let Some(r) = asset(path, &headers) {
        return Ok(r);
    }
    let segs: Vec<&str> = path.trim_start_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        ["a", id] => album_page(app, user, jar, id).await,
        ["d", _] => Ok(page("delete.html", None, &headers).unwrap_or_else(not_found)),
        [id, "thumb"] => thumb(app, user, jar, id, req).await,
        [id, "download"] => raw(app, user, jar, id, Mode::Download, req).await,
        [id, "raw"] => raw(app, user, jar, id, Mode::Inline, req).await,
        [name] => match name.rsplit_once('.') {
            Some((id, ext)) if !id.is_empty() && ext.len() <= 8 => raw(app, user, jar, id, Mode::Inline, req).await,
            _ => item_page(app, user, jar, name, &headers).await,
        },
        _ => Err(AppError::not_found()),
    }
}

const APP_PAGES: &[&str] = &["/", "/files", "/albums", "/qr", "/stats", "/settings", "/admin"];
const AUTH_PAGES: &[&str] = &["/login", "/register"];

pub fn safe_next(n: &str) -> bool {
    n.len() < 512
        && n.starts_with('/')
        && !n.starts_with("//")
        && !n.starts_with("/api")
        && n.bytes().all(|b| b.is_ascii_graphic() && b != b'\\')
}

fn query_param<'a>(query: Option<&'a str>, key: &str) -> Option<&'a str> {
    query?.split('&').find_map(|p| p.strip_prefix(key)?.strip_prefix('='))
}

fn gate(app: &AppState, user: Option<&User>, path: &str, query: Option<&str>) -> Option<String> {
    let path = path.strip_suffix(".html").unwrap_or(path);
    let query_code = query_param(query, "code").filter(|c| !c.is_empty() && c.len() <= 64 && c.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    let path = if path.len() > 1 { path.trim_end_matches('/') } else { path };
    let is_app = APP_PAGES.contains(&path);
    let is_auth = AUTH_PAGES.contains(&path);
    if path == "/setup" {
        if !app.has_users() {
            return None;
        }
        return Some(match user {
            Some(_) if query_param(query, "step") == Some("3") => return None,
            Some(_) => "/".into(),
            None => "/login".into(),
        });
    }
    if !app.has_users() && (is_app || is_auth) {
        return Some(match query_code {
            Some(c) => format!("/setup?code={c}"),
            None => "/setup".into(),
        });
    }
    match user {
        None if is_app => {
            let here = match query {
                Some(q) => format!("{path}?{q}"),
                None => path.to_string(),
            };
            Some(if path == "/" { "/login".into() } else { format!("/login?next={}", percent_encoding::utf8_percent_encode(&here, percent_encoding::NON_ALPHANUMERIC)) })
        }
        Some(u) if path == "/admin" && !u.is_admin() => Some("/".into()),
        Some(_) if is_auth => {
            let next = query_param(query, "next").map(|n| percent_encoding::percent_decode_str(n).decode_utf8_lossy().into_owned()).filter(|n| safe_next(n));
            Some(next.unwrap_or_else(|| "/".into()))
        }
        _ => None,
    }
}

fn not_found() -> Response {
    let mut r = page("404.html", None, &HeaderMap::new()).unwrap_or_else(|| "Not found".into_response());
    *r.status_mut() = StatusCode::NOT_FOUND;
    r
}

fn accepts(headers: &HeaderMap, enc: &str) -> bool {
    headers.get(header::ACCEPT_ENCODING).and_then(|v| v.to_str().ok()).is_some_and(|v| v.split(',').any(|e| e.trim().split(';').next() == Some(enc)))
}

fn asset(path: &str, headers: &HeaderMap) -> Option<Response> {
    let p = path.trim_start_matches('/');
    let candidates: Vec<Cow<str>> = if p.is_empty() {
        vec![Cow::Borrowed("index.html")]
    } else if p.ends_with(".html") || p.ends_with(".br") || p.ends_with(".gz") {
        return None;
    } else {
        vec![Cow::Borrowed(p), Cow::Owned(format!("{p}.html")), Cow::Owned(format!("{p}/index.html"))]
    };
    let name = candidates.into_iter().find(|c| Assets::get(c).is_some())?;
    if name.ends_with(".html") {
        return page(&name, None, headers);
    }
    file_response(&name, headers)
}

fn file_response(name: &str, headers: &HeaderMap) -> Option<Response> {
    let f = Assets::get(name)?;
    let etag = format!("\"{}\"", hex::encode(&f.metadata.sha256_hash()[..12]));
    let immutable = name.starts_with("_next/static/");
    let cache = if immutable { "public, max-age=31536000, immutable" } else { "public, max-age=3600" };
    if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
        return Some((StatusCode::NOT_MODIFIED, [(header::ETAG, etag), (header::CACHE_CONTROL, cache.to_string())]).into_response());
    }
    let mime = match mime_guess::from_path(name).first() {
        Some(m) => m.essence_str().to_string(),
        None => infer::get(&f.data).map(|t| t.mime_type().to_string()).unwrap_or_else(|| "application/octet-stream".into()),
    };
    let (data, enc) = ["br", "gzip"]
        .iter()
        .find_map(|enc| {
            let ext = if *enc == "br" { "br" } else { "gz" };
            if !accepts(headers, enc) {
                return None;
            }
            Assets::get(&format!("{name}.{ext}")).map(|c| (c.data, Some(*enc)))
        })
        .unwrap_or((f.data, None));
    let mut r = Response::new(Body::from(data.into_owned()));
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    h.insert(header::ETAG, HeaderValue::from_str(&etag).expect("hex"));
    h.insert(header::VARY, HeaderValue::from_static("Accept-Encoding"));
    if let Some(e) = enc {
        h.insert(header::CONTENT_ENCODING, HeaderValue::from_static(e));
    }
    r.extensions_mut().insert(NoCompress);
    Some(r)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

fn script_json(v: &Value) -> String {
    v.to_string().replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026").replace('\u{2028}', "\\u2028").replace('\u{2029}', "\\u2029")
}

pub struct Inject {
    pub title: String,
    pub meta: String,
    pub data: Value,
}

fn page(name: &str, inject: Option<Inject>, headers: &HeaderMap) -> Option<Response> {
    let f = Assets::get(name)?;
    let mut html = String::from_utf8_lossy(&f.data).into_owned();
    if let Some(inj) = inject {
        if let (Some(a), Some(b)) = (html.find("<title>"), html.find("</title>")) {
            html.replace_range(a + 7..b, &esc(&inj.title));
        }
        let extra = format!("{}<script id=\"__DATA__\" type=\"application/json\">{}</script>", inj.meta, script_json(&inj.data));
        match html.find("</head>") {
            Some(i) => html.insert_str(i, &extra),
            None => html.push_str(&extra),
        }
    } else if name != "404.html" {
        if let Some(r) = file_response(name, headers) {
            let mut r = r;
            r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            r.headers_mut().insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(PAGE_CSP));
            return Some(r);
        }
    }
    let mut r = Response::new(Body::from(html));
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache, private"));
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(PAGE_CSP));
    Some(r)
}

fn human(bytes: i64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{bytes} B") } else { format!("{v:.1} {}", units[i]) }
}

fn meta(prop: &str, content: &str) -> String {
    format!("<meta property=\"{prop}\" content=\"{}\">", esc(content))
}

fn meta_name(name: &str, content: &str) -> String {
    format!("<meta name=\"{name}\" content=\"{}\">", esc(content))
}

async fn item_page(app: &AppState, user: Option<User>, jar: &CookieJar, id: &str, headers: &HeaderMap) -> Result<Response, AppError> {
    let item = find_public(app, id.to_string()).await?;
    let owner = user.as_ref().is_some_and(|u| u.id == item.user_id);
    let unlocked = is_unlocked(app, jar, &item) || owner;
    if item.kind == Kind::Link && unlocked && !item.burn {
        return link_redirect(app, item).await;
    }
    let data = public_json(app, &item, user.as_ref(), unlocked).await?;
    if data.get("content").is_some() && !owner {
        if item.max_views.is_some() {
            if !count_strict(app, &item).await? {
                return Err(AppError::not_found());
            }
        } else {
            app.count_view(&item.id);
        }
    }
    let mut m = String::new();
    m += &meta("og:site_name", APP_NAME);
    m += &meta("og:url", &app.link(&format!("/{}", item.id)));
    
    m += &meta_name("robots", "noindex");
    let title;
    if item.locked() {
        title = "Protected file".to_string();
        m += &meta("og:title", "🔒 Protected file");
        m += &meta("og:description", "Enter the password to open it");
    } else if item.burn {
        title = "One-time file".to_string();
        m += &meta("og:title", "🔥 One-time file");
        m += &meta("og:description", "Can only be opened once");
    } else if item.max_views.is_some() {
        title = item.name.clone();
        m += &meta("og:title", &item.name);
        m += &meta("og:description", "Can only be opened a few times");
        m += &meta("og:type", "website");
    } else {
        title = item.name.clone();
        let by = data["uploader"]["name"].as_str().map(|n| format!(" · by {n}")).unwrap_or_default();
        let desc = match item.kind {
            Kind::Paste => format!("Paste · {}{by}", item.language.as_deref().unwrap_or("text")),
            _ => format!("{}{by}", human(item.size)),
        };
        m += &meta("og:title", &item.name);
        m += &meta("og:description", &desc);
        let raw = data["raw_url"].as_str().unwrap_or_default().to_string();
        match item.view_type() {
            "image" => {
                m += &format!("<link rel=\"preload\" as=\"image\" href=\"{}\" fetchpriority=\"high\">", esc(&raw));
                m += &meta("og:type", "website");
                m += &meta("og:image", &raw);
                m += &meta("og:image:type", &item.mime);
                if let (Some(w), Some(h)) = (item.width, item.height) {
                    m += &meta("og:image:width", &w.to_string());
                    m += &meta("og:image:height", &h.to_string());
                }
                m += &meta_name("twitter:card", "summary_large_image");
                m += &meta_name("twitter:image", &raw);
            }
            "video" => {
                m += &meta("og:type", "video.other");
                m += &meta("og:video", &raw);
                m += &meta("og:video:secure_url", &raw);
                m += &meta("og:video:type", &item.mime);
                if let (Some(w), Some(h)) = (item.width, item.height) {
                    m += &meta("og:video:width", &w.to_string());
                    m += &meta("og:video:height", &h.to_string());
                }
                if item.has_thumb {
                    let thumb = app.link(&format!("/{}/thumb", item.id));
                    m += &format!("<link rel=\"preload\" as=\"image\" href=\"{}\">", esc(&thumb));
                    m += &meta("og:image", &thumb);
                }
            }
            "audio" => {
                m += &meta("og:type", "music.song");
                m += &meta("og:audio", &raw);
                m += &meta("og:audio:type", &item.mime);
                if item.has_thumb {
                    m += &meta("og:image", &app.link(&format!("/{}/thumb", item.id)));
                }
            }
            _ => {
                m += &meta("og:type", "website");
                m += &meta_name("twitter:card", "summary");
            }
        }
    }
    let inj = Inject { title: format!("{title} · {APP_NAME}"), meta: m, data };
    page("view.html", Some(inj), headers).ok_or_else(AppError::not_found)
}

async fn link_redirect(app: &AppState, item: Item) -> Result<Response, AppError> {
    let target = item.target.clone().unwrap_or_else(|| "/".into());
    if item.max_views.is_some() {
        if !count_strict(app, &item).await? {
            return Err(AppError::not_found());
        }
    } else {
        app.count_view(&item.id);
    }
    let mut r = Redirect::temporary(&target).into_response();
    r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    r.headers_mut().insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    Ok(r)
}

async fn count_strict(app: &AppState, item: &Item) -> Result<bool, AppError> {
    let (id, store) = (item.id.clone(), app.store.clone());
    let ok = app
        .db
        .write(move |c| {
            let row: Option<(i64, Option<i64>, bool)> = rusqlite::OptionalExtension::optional(c.query_row(
                "UPDATE items SET views = views + 1 WHERE id = ? AND (max_views IS NULL OR views < max_views) RETURNING views, max_views, burn",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            ))?;
            let Some((views, max, burn)) = row else { return Ok(false) };
            if burn || max.is_some_and(|m| views >= m) {
                let unused = delete_items(c, std::slice::from_ref(&id))?;
                store.remove(&unused);
            }
            Ok(true)
        })
        .await?;
    Ok(ok)
}

async fn album_page(app: &AppState, user: Option<User>, jar: &CookieJar, id: &str) -> Result<Response, AppError> {
    let data = crate::api::albums::public_json(app, id.to_string(), user.as_ref(), jar).await?;
    let title = data["title"].as_str().unwrap_or("Album").to_string();
    let count = data["items"].as_array().map(|a| a.len()).unwrap_or(0);
    let mut m = String::new();
    m += &meta("og:site_name", APP_NAME);
    m += &meta("og:title", &title);
    let desc = data["description"].as_str().filter(|d| !d.is_empty()).map(str::to_string).unwrap_or_else(|| format!("{count} files"));
    m += &meta("og:description", &desc);
    
    m += &meta_name("robots", "noindex");
    if let Some(cover) = data["items"].as_array().and_then(|a| a.iter().find_map(|i| i["thumb_url"].as_str())) {
        m += &meta("og:image", cover);
        m += &meta_name("twitter:card", "summary_large_image");
    }
    let inj = Inject { title: format!("{title} · {APP_NAME}"), meta: m, data };
    page("album.html", Some(inj), &HeaderMap::new()).ok_or_else(AppError::not_found)
}

#[cfg(test)]
mod tests {
    use super::safe_next;

    #[test]
    fn next_stays_on_this_site() {
        assert!(safe_next("/files?type=image"));
        assert!(safe_next("/abc123"));
        for bad in ["//evil.com", "/\\evil.com", "/\t/evil.com", "/\n/evil.com", "/ /evil.com", "/%09x\\y", "https://evil.com", "/api/me", "evil.com", "/ä"] {
            assert!(!safe_next(bad), "{bad:?}");
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Mode {
    Inline,
    Download,
}

fn safe_mime(mime: &str) -> Cow<'_, str> {
    match mime {
        "text/html" | "application/xhtml+xml" | "text/xml" | "application/xml" => Cow::Borrowed("text/plain; charset=utf-8"),
        m if m.starts_with("text/") || m == "application/json" || m == "application/javascript" => Cow::Owned(format!("{m}; charset=utf-8")),
        m if m.is_empty() => Cow::Borrowed("application/octet-stream"),
        m => Cow::Borrowed(m),
    }
}

fn disposition(mode: Mode, name: &str) -> String {
    let kind = if mode == Mode::Download { "attachment" } else { "inline" };
    let ascii: String = name.chars().map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' }).collect();
    let enc = percent_encoding::utf8_percent_encode(name, percent_encoding::NON_ALPHANUMERIC);
    format!("{kind}; filename=\"{ascii}\"; filename*=UTF-8''{enc}")
}

async fn raw(app: &AppState, user: Option<User>, jar: &CookieJar, id: &str, mode: Mode, mut req: Request) -> Result<Response, AppError> {
    let item = find_public(app, id.to_string()).await?;
    if item.kind == Kind::Link {
        return item_page(app, user, jar, id, &req.headers().clone()).await;
    }
    let owner = user.as_ref().is_some_and(|u| u.id == item.user_id);
    if !is_unlocked(app, jar, &item) && !owner {
        return Ok(Redirect::to(&format!("/{}", item.id)).into_response());
    }
    let Some(sha) = item.sha256.clone() else { return Err(AppError::not_found()) };
    let strict = item.burn || item.max_views.is_some();
    let reveal = req.uri().query().is_some_and(|q| q.split('&').any(|p| p == "reveal=1"));
    if item.burn && !reveal && !owner {
        return Ok(Redirect::to(&format!("/{}", item.id)).into_response());
    }
    let etag = format!("\"{}\"", &sha[..24]);
    let cacheable = !item.locked() && !strict && item.expires_at.is_none();
    if !strict && req.headers().get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()) {
        let mut r = StatusCode::NOT_MODIFIED.into_response();
        r.headers_mut().insert(header::ETAG, HeaderValue::from_str(&etag).expect("hex"));
        return Ok(r);
    }
    let range_start_zero = match req.headers().get(header::RANGE).and_then(|v| v.to_str().ok()) {
        None => true,
        Some(r) => r.starts_with("bytes=0-"),
    };
    if strict {
        req.headers_mut().remove(header::RANGE);
    }
    let mime = safe_mime(&item.mime).into_owned();
    let path = app.store.file(&sha);
    let mut res = ServeFile::new_with_mime(&path, &mime.parse().unwrap_or(mime_guess::mime::APPLICATION_OCTET_STREAM)).oneshot(req).await.map_err(|e| AppError::Internal(e.into()))?;
    if res.status() == StatusCode::NOT_FOUND {
        return Err(AppError::not_found());
    }
    if strict && !owner {
        if !count_strict(app, &item).await? {
            return Err(AppError::not_found());
        }
    } else if range_start_zero && !owner && res.status().is_success() {
        app.count_view(&item.id);
    }
    let h = res.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(if cacheable { "public, max-age=604800" } else { "private, no-store" }));
    h.insert(header::ETAG, HeaderValue::from_str(&etag).expect("hex"));
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    let csp = if item.mime == "application/pdf" {
        "default-src 'none'; style-src 'unsafe-inline'; img-src data:; frame-ancestors 'self'"
    } else {
        "default-src 'none'; img-src 'self' data:; media-src 'self'; style-src 'unsafe-inline'; sandbox; frame-ancestors 'self'"
    };
    h.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(csp));
    if let Ok(v) = HeaderValue::from_str(&disposition(mode, &item.name)) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    let (parts, body) = res.into_parts();
    let mut r = Response::from_parts(parts, Body::new(body));
    r.extensions_mut().insert(NoCompress);
    Ok(r)
}

async fn thumb(app: &AppState, user: Option<User>, jar: &CookieJar, id: &str, req: Request) -> Result<Response, AppError> {
    let item = find_public(app, id.to_string()).await?;
    let owner = user.as_ref().is_some_and(|u| u.id == item.user_id);
    let admin = user.as_ref().is_some_and(User::is_admin);
    let strict = item.burn || item.max_views.is_some();
    if !item.has_thumb || (!is_unlocked(app, jar, &item) && !owner) || (strict && !owner && !admin) {
        return Err(AppError::not_found());
    }
    let Some(sha) = item.sha256.clone() else { return Err(AppError::not_found()) };
    let res = ServeFile::new_with_mime(app.store.thumb(&sha), &mime_guess::mime::IMAGE_JPEG).oneshot(req).await.map_err(|e| AppError::Internal(e.into()))?;
    let (mut parts, body) = res.into_parts();
    let cacheable = !item.locked() && item.expires_at.is_none();
    parts.headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(if cacheable { "public, max-age=604800" } else { "private, no-store" }));
    parts.headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    let mut r = Response::from_parts(parts, Body::new(body));
    r.extensions_mut().insert(NoCompress);
    Ok(r)
}
