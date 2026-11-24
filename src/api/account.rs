use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{Value, json};

use super::upload::CHUNK_SIZE;
use crate::auth::{self, Auth, ClientIp, MaybeAuth, SESSION_COOKIE, check_password, check_username, create_session, hash_password, random_token, sha256, sign, verify, verify_password};
use crate::config::Registration;
use crate::db::{User, now};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub async fn instance(State(app): State<AppState>, MaybeAuth(user): MaybeAuth) -> AppResult<Json<Value>> {
    let s = app.settings();
    let users = i64::from(app.has_users());
    Ok(Json(json!({
        "name": crate::config::APP_NAME,
        "url": app.cfg.url,
        "setup": users == 0,
        "registration": s.registration,
        "twitch": app.twitch().is_some(),
        "max_upload": s.max_upload_mb * 1024 * 1024,
        "chunk_size": CHUNK_SIZE,
        "version": env!("CARGO_PKG_VERSION"),
        "user": user,
    })))
}

#[derive(Deserialize)]
pub struct CodeReq {
    #[serde(default)]
    code: String,
}

fn setup_code_ok(app: &AppState, ip: std::net::IpAddr, code: &str) -> AppResult<()> {
    if !app.limiter.hit(&format!("setup:{ip}"), 10, Duration::from_secs(600)) {
        return Err(AppError::too_many());
    }
    if setup_code_matches(app, code) { Ok(()) } else { Err(AppError::forbidden("Wrong setup code. It is in the server log (docker compose logs uploader).")) }
}

fn setup_code_matches(app: &AppState, code: &str) -> bool {
    let code = code.trim();
    !code.is_empty() && app.setup_code.lock().unwrap_or_else(|e| e.into_inner()).as_deref().is_some_and(|c| auth::safe_compare(c, code))
}

pub async fn setup_check(State(app): State<AppState>, ClientIp(ip): ClientIp, Json(req): Json<CodeReq>) -> AppResult<StatusCode> {
    setup_code_ok(&app, ip, &req.code)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SetupReq {
    #[serde(default)]
    code: String,
    username: String,
    password: String,
}

pub async fn setup(State(app): State<AppState>, ClientIp(ip): ClientIp, jar: CookieJar, Json(req): Json<SetupReq>) -> AppResult<Response> {
    setup_code_ok(&app, ip, &req.code)?;
    let name = check_username(&req.username)?;
    check_password(&req.password)?;
    let hash = hash_password(req.password).await?;
    let id = app
        .db
        .write(move |c| {
            let users: i64 = c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
            if users > 0 {
                return Ok(None);
            }
            c.execute("INSERT INTO users (username, password_hash, role, owner, created_at) VALUES (?, ?, 'admin', 1, ?)", rusqlite::params![name, hash, now()])?;
            Ok(Some(c.last_insert_rowid()))
        })
        .await?
        .ok_or_else(|| AppError::conflict("Setup is already done"))?;
    *app.setup_code.lock().unwrap_or_else(|e| e.into_inner()) = None;
    app.set_has_users();
    let cookie = create_session(&app, id).await?;
    Ok((jar.add(cookie), StatusCode::NO_CONTENT).into_response())
}

#[derive(Deserialize)]
pub struct LoginReq {
    username: String,
    password: String,
}

pub async fn login(State(app): State<AppState>, ClientIp(ip): ClientIp, jar: CookieJar, Json(req): Json<LoginReq>) -> AppResult<Response> {
    let name = req.username.trim().to_string();
    let key = format!("login:{ip}:{}", name.to_lowercase());
    if !app.limiter.hit(&key, 10, Duration::from_secs(900)) || !app.limiter.hit(&format!("login:{ip}"), 30, Duration::from_secs(900)) {
        return Err(AppError::too_many());
    }
    let found = app
        .db
        .read(move |c| {
            Ok(c.query_row("SELECT id, password_hash, disabled, role FROM users WHERE username = ?", [&name], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, bool>(2)?, r.get::<_, String>(3)?)))
                .optional()?)
        })
        .await?;
    let found = found.map(|(id, h, d, _role)| (id, h, d));
    let (id, hash, disabled) = found.unwrap_or((0, None, false));
    let hash_or_dummy = match &hash {
        Some(h) => h.clone(),
        None => auth::dummy_hash().await,
    };
    let ok = verify_password(hash_or_dummy, req.password).await && hash.is_some();
    if !ok {
        return Err(AppError::Status(StatusCode::UNAUTHORIZED, "Wrong username or password".into()));
    }
    if disabled {
        return Err(AppError::forbidden("This account is disabled"));
    }
    app.limiter.clear(&key);
    let cookie = create_session(&app, id).await?;
    Ok((jar.add(cookie), StatusCode::NO_CONTENT).into_response())
}

#[derive(Deserialize)]
pub struct RegisterReq {
    username: String,
    password: String,
    #[serde(default)]
    invite: Option<String>,
}

pub struct Grant {
    pub username: Option<String>,
    pub role: String,
}

fn find_invite(c: &rusqlite::Connection, code: &str) -> rusqlite::Result<Option<(Option<String>, String)>> {
    c.query_row("SELECT username, role FROM invites WHERE code = ? AND uses_left > 0 AND (expires_at IS NULL OR expires_at > ?)", rusqlite::params![code, now()], |r| Ok((r.get(0)?, r.get(1)?))).optional()
}

fn may_register(c: &rusqlite::Connection, mode: Registration, invite: Option<&str>) -> anyhow::Result<Result<Grant, &'static str>> {
    if let Some(code) = invite.filter(|c| !c.is_empty()) {
        if let Some((username, role)) = find_invite(c, code)? {
            c.execute("UPDATE invites SET uses_left = uses_left - 1 WHERE code = ?", [code])?;
            c.execute("DELETE FROM invites WHERE uses_left <= 0", [])?;
            return Ok(Ok(Grant { username, role }));
        }
        if mode != Registration::Open {
            return Ok(Err("This invite link is not valid any more"));
        }
    }
    Ok(match mode {
        Registration::Open => Ok(Grant { username: None, role: "user".into() }),
        Registration::Invite => Err("You need an invite link to sign up"),
        Registration::Closed => Err("Sign up is closed"),
    })
}

pub async fn invite_info(State(app): State<AppState>, Path(code): Path<String>) -> AppResult<Json<Value>> {
    let found = app.db.read(move |c| Ok(find_invite(c, &code)?)).await?;
    Ok(Json(match found {
        Some((username, role)) => json!({ "valid": true, "username": username, "admin": role == "admin" }),
        None => json!({ "valid": false }),
    }))
}

pub async fn register(State(app): State<AppState>, ClientIp(ip): ClientIp, jar: CookieJar, Json(req): Json<RegisterReq>) -> AppResult<Response> {
    if !app.limiter.hit(&format!("register:{ip}"), 5, Duration::from_secs(3600)) {
        return Err(AppError::too_many());
    }
    let name = check_username(&req.username)?;
    check_password(&req.password)?;
    let hash = hash_password(req.password).await?;
    let mode = app.settings().registration;
    let res = app
        .db
        .write(move |c| {
            let tx = c.transaction()?;
            let taken: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM users WHERE username = ?)", [&name], |r| r.get(0))?;
            if taken {
                return Ok(Err("That username is taken"));
            }
            let grant = match may_register(&tx, mode, req.invite.as_deref())? {
                Ok(g) => g,
                Err(e) => return Ok(Err(e)),
            };
            if grant.username.as_deref().is_some_and(|u| !u.eq_ignore_ascii_case(&name)) {
                return Ok(Err("This invite is for another username"));
            }
            tx.execute("INSERT INTO users (username, password_hash, role, created_at) VALUES (?, ?, ?, ?)", rusqlite::params![name, hash, grant.role, now()])?;
            let id = tx.last_insert_rowid();
            tx.commit()?;
            Ok(Ok(id))
        })
        .await?;
    let id = res.map_err(AppError::forbidden)?;
    let cookie = create_session(&app, id).await?;
    Ok((jar.add(cookie), StatusCode::NO_CONTENT).into_response())
}

pub async fn logout(State(app): State<AppState>, jar: CookieJar) -> AppResult<Response> {
    if let Some(c) = jar.get(SESSION_COOKIE) {
        let hash = sha256(c.value());
        app.db.write(move |c| Ok(c.execute("DELETE FROM sessions WHERE token_hash = ?", [hash])?)).await?;
    }
    let gone = auth::session_cookie(&app, String::new(), 0);
    Ok((jar.remove(gone), StatusCode::NO_CONTENT).into_response())
}

pub async fn logout_everywhere(State(app): State<AppState>, Auth(user): Auth, jar: CookieJar) -> AppResult<Response> {
    let uid = user.id;
    app.db.write(move |c| Ok(c.execute("DELETE FROM sessions WHERE user_id = ?", [uid])?)).await?;
    let gone = auth::session_cookie(&app, String::new(), 0);
    Ok((jar.remove(gone), StatusCode::NO_CONTENT).into_response())
}

pub async fn me(Auth(user): Auth) -> Json<User> {
    Json(user)
}

#[derive(Deserialize)]
pub struct UpdateMe {
    username: Option<String>,
    onboarded: Option<bool>,
    current_password: Option<String>,
    new_password: Option<String>,
}

pub async fn update_me(State(app): State<AppState>, Auth(user): Auth, jar: CookieJar, Json(req): Json<UpdateMe>) -> AppResult<Json<User>> {
    let uid = user.id;
    if let Some(pw) = req.new_password {
        check_password(&pw)?;
        let old: Option<String> = app.db.read(move |c| Ok(c.query_row("SELECT password_hash FROM users WHERE id = ?", [uid], |r| r.get(0))?)).await?;
        if let Some(old) = old
            && !verify_password(old, req.current_password.unwrap_or_default()).await
        {
            return Err(AppError::forbidden("Current password is wrong"));
        }
        let hash = hash_password(pw).await?;
        let current = jar.get(SESSION_COOKIE).map(|c| sha256(c.value())).unwrap_or_default();
        app.db
            .write(move |c| {
                let tx = c.transaction()?;
                tx.execute("UPDATE users SET password_hash = ? WHERE id = ?", rusqlite::params![hash, uid])?;
                tx.execute("DELETE FROM sessions WHERE user_id = ? AND token_hash != ?", rusqlite::params![uid, current])?;
                tx.commit()?;
                Ok(())
            })
            .await?;
    }
    if let Some(name) = req.username {
        let name = check_username(&name)?;
        let ok = app
            .db
            .write(move |c| match c.execute("UPDATE users SET username = ? WHERE id = ?", rusqlite::params![name, uid]) {
                Ok(_) => Ok(true),
                Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => Ok(false),
                Err(e) => Err(e.into()),
            })
            .await?;
        if !ok {
            return Err(AppError::conflict("That username is taken"));
        }
    }
    if let Some(done) = req.onboarded {
        let at = done.then(now);
        app.db.write(move |c| Ok(c.execute("UPDATE users SET onboarded_at = ? WHERE id = ?", rusqlite::params![at, uid])?)).await?;
    }
    let user = app.db.read(move |c| Ok(User::by_id(c, uid)?)).await?.ok_or_else(AppError::not_found)?;
    Ok(Json(user))
}

pub async fn unlink_twitch(State(app): State<AppState>, Auth(user): Auth) -> AppResult<StatusCode> {
    if !user.has_password {
        return Err(AppError::bad("Set a password first, otherwise you can't log in any more"));
    }
    let uid = user.id;
    app.db.write(move |c| Ok(c.execute("UPDATE users SET twitch_id = NULL, avatar_url = NULL WHERE id = ?", [uid])?)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct TwitchStart {
    #[serde(default)]
    invite: Option<String>,
    #[serde(default)]
    setup: Option<String>,
    #[serde(default)]
    next: Option<String>,
}

const STATE_COOKIE: &str = "up_oauth";
const NEXT_COOKIE: &str = "up_next";

pub async fn twitch_start(State(app): State<AppState>, MaybeAuth(user): MaybeAuth, jar: CookieJar, Query(q): Query<TwitchStart>) -> AppResult<Response> {
    let Some(tw) = app.twitch() else { return Err(AppError::not_found()) };
    let nonce = random_token(16);
    let mode = if user.is_some() { "link" } else { "login" };
    let extra = q.invite.or(q.setup.map(|s| format!("setup:{s}"))).unwrap_or_default();
    let payload = format!("{nonce}|{mode}|{extra}");
    let value = format!("{payload}|{}", sign(&app, &payload));
    let cookie = Cookie::build((STATE_COOKIE, value)).path("/api/auth/twitch").http_only(true).secure(app.cfg.secure_cookies()).same_site(SameSite::Lax).max_age(auth::time_dur(600)).build();
    let next = Cookie::build((NEXT_COOKIE, q.next.filter(|n| crate::public::safe_next(n)).unwrap_or_default()))
        .path("/api/auth/twitch")
        .http_only(true)
        .secure(app.cfg.secure_cookies())
        .same_site(SameSite::Lax)
        .max_age(auth::time_dur(600))
        .build();
    let jar = jar.add(next);
    let url = format!(
        "https://id.twitch.tv/oauth2/authorize?client_id={}&redirect_uri={}&response_type=code&scope=&state={}",
        tw.client_id,
        percent_encoding::utf8_percent_encode(&app.link("/api/auth/twitch/callback"), percent_encoding::NON_ALPHANUMERIC),
        nonce
    );
    Ok((jar.add(cookie), Redirect::to(&url)).into_response())
}

#[derive(Deserialize)]
pub struct Callback {
    code: Option<String>,
    state: Option<String>,
}

fn back(path: &str, error: &str) -> Response {
    Redirect::to(&format!("{path}?error={}", percent_encoding::utf8_percent_encode(error, percent_encoding::NON_ALPHANUMERIC))).into_response()
}

pub async fn twitch_callback(State(app): State<AppState>, MaybeAuth(user): MaybeAuth, jar: CookieJar, Query(q): Query<Callback>) -> AppResult<Response> {
    let Some(tw) = app.twitch() else { return Err(AppError::not_found()) };
    let cookie = jar.get(STATE_COOKIE).map(|c| c.value().to_string()).unwrap_or_default();
    let next = jar.get(NEXT_COOKIE).map(|c| c.value().to_string()).filter(|n| crate::public::safe_next(n)).unwrap_or_else(|| "/".into());
    let jar = jar.remove(Cookie::build(STATE_COOKIE).path("/api/auth/twitch").build()).remove(Cookie::build(NEXT_COOKIE).path("/api/auth/twitch").build());
    let parts: Vec<&str> = cookie.rsplitn(2, '|').collect();
    let (Some(sig), Some(payload)) = (parts.first(), parts.get(1)) else { return Ok((jar, back("/login", "Login expired, try again")).into_response()) };
    let mut p = payload.splitn(3, '|');
    let (nonce, mode, extra) = (p.next().unwrap_or(""), p.next().unwrap_or(""), p.next().unwrap_or(""));
    if !verify(&app, payload, sig) || q.state.as_deref() != Some(nonce) {
        return Ok((jar, back("/login", "Login expired, try again")).into_response());
    }
    let Some(code) = q.code else { return Ok((jar, back("/login", "Twitch login was cancelled")).into_response()) };

    let token: Value = app
        .http
        .post("https://id.twitch.tv/oauth2/token")
        .form(&[("client_id", tw.client_id.as_str()), ("client_secret", tw.client_secret.as_str()), ("code", code.as_str()), ("grant_type", "authorization_code"), ("redirect_uri", &app.link("/api/auth/twitch/callback"))])
        .send()
        .await?
        .json()
        .await?;
    let Some(access) = token["access_token"].as_str() else { return Ok((jar, back("/login", "Twitch login failed")).into_response()) };
    let me: Value = app.http.get("https://api.twitch.tv/helix/users").bearer_auth(access).header("Client-Id", &tw.client_id).send().await?.json().await?;
    let tu = &me["data"][0];
    let (Some(tid), Some(login)) = (tu["id"].as_str().map(str::to_string), tu["login"].as_str().map(str::to_string)) else {
        return Ok((jar, back("/login", "Twitch login failed")).into_response());
    };
    let avatar = tu["profile_image_url"].as_str().map(str::to_string);

    if mode == "link" {
        let Some(u) = user else { return Ok((jar, back("/login", "Log in first")).into_response()) };
        let (uid, tid2, av) = (u.id, tid.clone(), avatar.clone());
        let ok = app
            .db
            .write(move |c| match c.execute("UPDATE users SET twitch_id = ?, avatar_url = ? WHERE id = ?", rusqlite::params![tid2, av, uid]) {
                Ok(_) => Ok(true),
                Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => Ok(false),
                Err(e) => Err(e.into()),
            })
            .await?;
        let to = if next == "/" { "/settings".to_string() } else { next };
        return Ok((jar, if ok { Redirect::to(&to).into_response() } else { back("/settings", "This Twitch account belongs to another user") }).into_response());
    }

    let setup_ok = extra.strip_prefix("setup:").is_some_and(|c| setup_code_matches(&app, c));
    let invite = (!extra.starts_with("setup:") && !extra.is_empty()).then(|| extra.to_string());
    let mode_setting = app.settings().registration;
    let res = app
        .db
        .write(move |c| {
            let tx = c.transaction()?;
            let found: Option<(i64, bool)> = tx.query_row("SELECT id, disabled FROM users WHERE twitch_id = ?", [&tid], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
            if let Some((id, disabled)) = found {
                if disabled {
                    return Ok(Err("This account is disabled"));
                }
                tx.execute("UPDATE users SET avatar_url = ? WHERE id = ?", rusqlite::params![avatar, id])?;
                tx.commit()?;
                return Ok(Ok(id));
            }
            let users: i64 = tx.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
            let (role, wanted) = if users == 0 {
                if !setup_ok {
                    return Ok(Err("Open the setup page first"));
                }
                ("admin".to_string(), None)
            } else {
                match may_register(&tx, mode_setting, invite.as_deref())? {
                    Ok(g) => (g.role, g.username),
                    Err(e) => return Ok(Err(e)),
                }
            };
            let base = wanted.unwrap_or_else(|| login.clone());
            let mut name = base.clone();
            let mut n = 1;
            while tx.query_row("SELECT EXISTS(SELECT 1 FROM users WHERE username = ?)", [&name], |r| r.get::<_, bool>(0))? {
                n += 1;
                name = format!("{base}{n}");
            }
            tx.execute("INSERT INTO users (username, twitch_id, avatar_url, role, owner, created_at) VALUES (?, ?, ?, ?, ?, ?)", rusqlite::params![name, tid, avatar, role, users == 0, now()])?;
            let id = tx.last_insert_rowid();
            tx.commit()?;
            Ok(Ok(id))
        })
        .await?;
    match res {
        Ok(id) => {
            if setup_ok {
                *app.setup_code.lock().unwrap_or_else(|e| e.into_inner()) = None;
                app.set_has_users();
            }
            let cookie = create_session(&app, id).await?;
            Ok((jar.add(cookie), Redirect::to(if setup_ok { "/setup?step=3" } else { &next })).into_response())
        }
        Err(e) => Ok((jar, back("/login", e)).into_response()),
    }
}

pub async fn keys(State(app): State<AppState>, Auth(user): Auth) -> AppResult<Json<Value>> {
    let uid = user.id;
    let rows = app
        .db
        .read(move |c| {
            let mut st = c.prepare_cached("SELECT id, name, prefix, created_at, last_used_at FROM api_keys WHERE user_id = ? ORDER BY created_at DESC")?;
            let rows = st
                .query_map([uid], |r| Ok(json!({ "id": r.get::<_, i64>(0)?, "name": r.get::<_, String>(1)?, "prefix": r.get::<_, String>(2)?, "created_at": r.get::<_, i64>(3)?, "last_used_at": r.get::<_, Option<i64>>(4)? })))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;
    Ok(Json(json!(rows)))
}

#[derive(Deserialize)]
pub struct KeyReq {
    name: String,
}

pub async fn create_key(State(app): State<AppState>, Auth(user): Auth, Json(req): Json<KeyReq>) -> AppResult<Json<Value>> {
    let name: String = req.name.trim().chars().take(40).collect();
    if name.is_empty() {
        return Err(AppError::bad("Give the key a name"));
    }
    let key = format!("upk_{}", random_token(24));
    let (hash, prefix, uid) = (sha256(&key), key[..10].to_string(), user.id);
    let n = name.clone();
    let id = app
        .db
        .write(move |c| {
            let count: i64 = c.query_row("SELECT COUNT(*) FROM api_keys WHERE user_id = ?", [uid], |r| r.get(0))?;
            if count >= 50 {
                return Ok(None);
            }
            c.execute("INSERT INTO api_keys (user_id, name, key_hash, prefix, created_at) VALUES (?, ?, ?, ?, ?)", rusqlite::params![uid, n, hash, prefix, now()])?;
            Ok(Some(c.last_insert_rowid()))
        })
        .await?
        .ok_or_else(|| AppError::bad("At most 50 keys"))?;
    Ok(Json(json!({ "id": id, "name": name, "key": key, "upload_url": app.link("/api/upload") })))
}

pub async fn delete_key(State(app): State<AppState>, Auth(user): Auth, Path(id): Path<i64>) -> AppResult<StatusCode> {
    let uid = user.id;
    let n = app.db.write(move |c| Ok(c.execute("DELETE FROM api_keys WHERE id = ? AND user_id = ?", [id, uid])?)).await?;
    if n == 0 { Err(AppError::not_found()) } else { Ok(StatusCode::NO_CONTENT) }
}
