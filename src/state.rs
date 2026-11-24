use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use sha2::Sha256;
use tokio::sync::Semaphore;

use crate::config::{Config, Settings};
use crate::db::Db;
use crate::storage::Storage;

pub type AppState = Arc<App>;

pub struct App {
    pub cfg: Config,
    pub db: Arc<Db>,
    pub store: Storage,
    settings: RwLock<Arc<Settings>>,
    pub http: reqwest::Client,
    pub views: Mutex<HashMap<String, i64>>,
    pub heavy: Semaphore,
    pub uploads: Arc<Mutex<HashMap<i64, u32>>>,
    pub limiter: Limiter,
    pub chunks: Mutex<HashMap<String, Chunked>>,
    pub setup_code: Mutex<Option<String>>,
    pub latest: Mutex<Option<(Instant, Option<String>)>>,
    pub shutdown: tokio::sync::Notify,
    pub has_users: std::sync::atomic::AtomicBool,
}

pub struct Chunked {
    pub user_id: i64,
    pub _slot: UploadSlot,
    pub name: String,
    pub total: u64,
    pub opts: crate::api::upload::Options,
    pub path: std::path::PathBuf,
    pub hasher: Sha256,
    pub size: u64,
    pub head: Vec<u8>,
    pub touched: Instant,
}

impl App {
    pub fn new(cfg: Config, db: Arc<Db>, store: Storage, settings: Settings) -> App {
        App {
            cfg,
            db,
            store,
            settings: RwLock::new(Arc::new(settings)),
            http: reqwest::Client::builder().user_agent(concat!("uploader/", env!("CARGO_PKG_VERSION"))).timeout(Duration::from_secs(20)).build().expect("http client"),
            views: Mutex::new(HashMap::new()),
            heavy: Semaphore::new(1),
            uploads: Arc::default(),
            limiter: Limiter::default(),
            chunks: Mutex::new(HashMap::new()),
            setup_code: Mutex::new(None),
            latest: Mutex::new(None),
            shutdown: tokio::sync::Notify::new(),
            has_users: std::sync::atomic::AtomicBool::new(true),
        }
    }

    pub fn settings(&self) -> Arc<Settings> {
        self.settings.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_settings(&self, s: Settings) {
        *self.settings.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(s);
    }

    pub fn twitch(&self) -> Option<crate::config::Twitch> {
        let s = self.settings();
        if !s.twitch_client_id.is_empty() && !s.twitch_client_secret.is_empty() {
            return Some(crate::config::Twitch { client_id: s.twitch_client_id.clone(), client_secret: s.twitch_client_secret.clone() });
        }
        self.cfg.twitch.clone()
    }

    pub fn count_view(&self, id: &str) {
        let mut v = self.views.lock().unwrap_or_else(|e| e.into_inner());
        *v.entry(id.to_string()).or_insert(0) += 1;
    }

    pub fn has_users(&self) -> bool {
        self.has_users.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_has_users(&self) {
        self.has_users.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn link(&self, path: &str) -> String {
        format!("{}{}", self.cfg.url, path)
    }
}

pub const MAX_PARALLEL_UPLOADS: u32 = 4;

pub struct UploadSlot {
    map: Arc<Mutex<HashMap<i64, u32>>>,
    user_id: i64,
}

impl UploadSlot {
    pub fn take(app: &App, user_id: i64) -> Option<UploadSlot> {
        let mut m = app.uploads.lock().unwrap_or_else(|e| e.into_inner());
        let n = m.entry(user_id).or_insert(0);
        if *n >= MAX_PARALLEL_UPLOADS {
            return None;
        }
        *n += 1;
        Some(UploadSlot { map: app.uploads.clone(), user_id })
    }
}

impl Drop for UploadSlot {
    fn drop(&mut self) {
        let mut m = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = m.get_mut(&self.user_id) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                m.remove(&self.user_id);
            }
        }
    }
}

#[derive(Default)]
pub struct Limiter {
    hits: Mutex<HashMap<String, (u32, Instant)>>,
}

impl Limiter {
    pub fn hit(&self, key: &str, max: u32, window: Duration) -> bool {
        let mut m = self.hits.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if m.len() > 10_000 {
            m.retain(|_, (_, t)| now.duration_since(*t) < window);
        }
        let e = m.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(e.1) >= window {
            *e = (0, now);
        }
        e.0 += 1;
        e.0 <= max
    }

    pub fn clear(&self, key: &str) {
        self.hits.lock().unwrap_or_else(|e| e.into_inner()).remove(key);
    }
}
