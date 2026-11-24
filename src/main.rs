mod api;
mod auth;
mod config;
mod db;
mod error;
mod ids;
mod media;
mod public;
mod state;
mod storage;
mod tasks;
mod update;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::http::{HeaderValue, Response, header};
use clap::{Parser, Subcommand};
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::Predicate;

use crate::config::Config;
use crate::db::Db;
use crate::state::{App, AppState};
use crate::storage::Storage;

#[derive(Parser)]
#[command(name = "uploader", version, about = "Fast self-hosted file, paste and link host")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start the server (default)
    Serve,
    /// Remove expired items and files no item uses
    Clean {
        /// only show what would be removed
        #[arg(long)]
        dry_run: bool,
    },
    /// Give a user a new random password
    ResetPassword { username: String },
    /// Exit 0 when the server answers (Docker health check)
    Health,
    /// Download and install the newest version
    Update {
        /// only check for a new version
        #[arg(long)]
        check: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "uploader=info,tower_http=warn".into()))
        .with_target(false)
        .init();
    let cli = Cli::parse();
    match cli.cmd.unwrap_or(Cmd::Serve) {
        Cmd::Serve => serve().await,
        Cmd::Clean { dry_run } => tasks::clean_command(&open()?, dry_run).await,
        Cmd::ResetPassword { username } => reset_password(&open()?, username).await,
        Cmd::Update { check } => update::run(&Config::from_env()?.repo, check).await,
        Cmd::Health => {
            let port = std::env::var("UPLOADER_BIND").ok().and_then(|b| b.rsplit(':').next().map(str::to_string)).unwrap_or_else(|| "3000".into());
            let ok = reqwest::Client::new().get(format!("http://127.0.0.1:{port}/health")).timeout(std::time::Duration::from_secs(3)).send().await.is_ok_and(|r| r.status().is_success());
            std::process::exit(if ok { 0 } else { 1 })
        }
    }
}

fn open() -> anyhow::Result<AppState> {
    let cfg = Config::from_env()?;
    let db = Db::open(&cfg.data_dir.join("uploader.db"))?;
    let store = Storage::new(&cfg.data_dir, cfg.min_free_bytes)?;
    let settings = {
        let c = rusqlite::Connection::open(cfg.data_dir.join("uploader.db"))?;
        api::admin::load_settings(&c)
    };
    Ok(Arc::new(App::new(cfg, db, store, settings)))
}

async fn reset_password(app: &AppState, username: String) -> anyhow::Result<()> {
    let pw = auth::random_token(9);
    let hash = auth::hash_password(pw.clone()).await?;
    let n = app
        .db
        .write(move |c| {
            let n = c.execute("UPDATE users SET password_hash = ? WHERE username = ?", rusqlite::params![hash, username])?;
            c.execute("DELETE FROM sessions WHERE user_id = (SELECT id FROM users WHERE username = ?)", [&username])?;
            Ok(n)
        })
        .await?;
    anyhow::ensure!(n == 1, "no such user");
    println!("New password: {pw}");
    Ok(())
}

#[derive(Clone, Copy)]
struct Compressible;

impl Predicate for Compressible {
    fn should_compress<B>(&self, res: &Response<B>) -> bool
    where
        B: axum::body::HttpBody,
    {
        if res.extensions().get::<public::NoCompress>().is_some() || res.headers().contains_key(header::CONTENT_ENCODING) {
            return false;
        }
        let ct = res.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
        let big = res.headers().get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()).is_none_or(|n| n > 512);
        big && (ct.starts_with("text/") || ct.starts_with("application/json"))
    }
}

async fn serve() -> anyhow::Result<()> {
    let app = open()?;
    let users: i64 = app.db.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?)).await?;
    app.has_users.store(users > 0, std::sync::atomic::Ordering::Relaxed);
    if users == 0 {
        let code = auth::random_token(12);
        tracing::info!("No account yet. Open {}/setup?code={code} to create the admin account (setup code: {code})", app.cfg.url);
        *app.setup_code.lock().unwrap_or_else(|e| e.into_inner()) = Some(code);
    }
    tasks::spawn(app.clone());

    let router = Router::new()
        .nest("/api", api::router())
        .fallback(public::serve)
        .layer(CompressionLayer::new().br(true).gzip(true).compress_when(Compressible))
        .layer(axum::middleware::from_fn_with_state(app.clone(), headers))
        .with_state(app.clone());

    let listener = tokio::net::TcpListener::bind(app.cfg.bind).await?;
    tracing::info!("uploader v{} on http://{} ({})", env!("CARGO_PKG_VERSION"), app.cfg.bind, app.cfg.url);
    axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).with_graceful_shutdown(shutdown(app.clone())).await?;
    tasks::flush_views(&app).await?;
    Ok(())
}

async fn headers(axum::extract::State(app): axum::extract::State<AppState>, req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let started = std::time::Instant::now();
    let mut res = next.run(req).await;
    let took = started.elapsed().as_secs_f64() * 1000.0;
    let h = res.headers_mut();
    h.entry(header::X_CONTENT_TYPE_OPTIONS).or_insert(HeaderValue::from_static("nosniff"));
    h.entry(header::REFERRER_POLICY).or_insert(HeaderValue::from_static("strict-origin-when-cross-origin"));
    h.insert(header::SERVER, HeaderValue::from_static(concat!("uploader/", env!("CARGO_PKG_VERSION"))));
    h.insert("permissions-policy", HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=(), usb=(), interest-cohort=()"));
    h.insert("cross-origin-opener-policy", HeaderValue::from_static("same-origin"));
    if app.cfg.secure_cookies() {
        h.insert(header::STRICT_TRANSPORT_SECURITY, HeaderValue::from_static("max-age=31536000"));
    }
    if let Ok(v) = HeaderValue::from_str(&format!("app;desc=\"uploader\";dur={took:.2}")) {
        h.insert("server-timing", v);
    }
    res
}

async fn shutdown(app: AppState) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
        _ = app.shutdown.notified() => tracing::info!("restarting after the update"),
    }
}
