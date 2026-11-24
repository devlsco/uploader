use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, params};
use serde::Serialize;
use tokio::sync::Semaphore;

const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT,
    twitch_id TEXT UNIQUE,
    avatar_url TEXT,
    role TEXT NOT NULL DEFAULT 'user',
    quota_mb INTEGER,
    used_bytes INTEGER NOT NULL DEFAULT 0,
    disabled INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE TABLE sessions (
    token_hash BLOB PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
) WITHOUT ROWID;
CREATE INDEX sessions_user ON sessions(user_id);
CREATE TABLE api_keys (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_hash BLOB NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER
);
CREATE INDEX api_keys_user ON api_keys(user_id);
CREATE TABLE albums (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    public INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
) WITHOUT ROWID;
CREATE INDEX albums_user ON albums(user_id, created_at DESC);
CREATE TABLE items (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    mime TEXT NOT NULL DEFAULT '',
    size INTEGER NOT NULL DEFAULT 0,
    sha256 TEXT,
    width INTEGER,
    height INTEGER,
    duration REAL,
    language TEXT,
    target TEXT,
    has_thumb INTEGER NOT NULL DEFAULT 0,
    password_hash TEXT,
    expires_at INTEGER,
    burn INTEGER NOT NULL DEFAULT 0,
    max_views INTEGER,
    views INTEGER NOT NULL DEFAULT 0,
    album_id TEXT REFERENCES albums(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL
) WITHOUT ROWID;
CREATE INDEX items_user ON items(user_id, created_at DESC);
CREATE INDEX items_album ON items(album_id, created_at DESC) WHERE album_id IS NOT NULL;
CREATE INDEX items_expires ON items(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX items_sha ON items(sha256) WHERE sha256 IS NOT NULL;
CREATE TRIGGER items_used_add AFTER INSERT ON items BEGIN
    UPDATE users SET used_bytes = used_bytes + NEW.size WHERE id = NEW.user_id;
END;
CREATE TRIGGER items_used_del AFTER DELETE ON items BEGIN
    UPDATE users SET used_bytes = used_bytes - OLD.size WHERE id = OLD.user_id;
END;
CREATE TABLE invites (
    code TEXT PRIMARY KEY,
    created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    uses_left INTEGER NOT NULL DEFAULT 1,
    expires_at INTEGER,
    created_at INTEGER NOT NULL
) WITHOUT ROWID;
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;
"#, r#"
ALTER TABLE invites ADD COLUMN username TEXT;
ALTER TABLE invites ADD COLUMN role TEXT NOT NULL DEFAULT 'user';
"#, r#"
-- deletion links name the item and its creation time from now on; old links keep
-- working only for items made before this point
INSERT OR IGNORE INTO settings (key, value) VALUES ('delete_tokens_v2', CAST(strftime('%s', 'now') AS TEXT));
"#, r#"
-- the welcome tour shows once per account; people who already use the app skip it
ALTER TABLE users ADD COLUMN onboarded_at INTEGER;
UPDATE users SET onboarded_at = created_at;
-- the owner (the account from the setup) can't be changed or removed by other admins
ALTER TABLE users ADD COLUMN owner INTEGER NOT NULL DEFAULT 0;
UPDATE users SET owner = 1 WHERE id = (SELECT id FROM users WHERE role = 'admin' ORDER BY created_at, id LIMIT 1);
CREATE UNIQUE INDEX users_one_owner ON users(owner) WHERE owner = 1;
"#];

fn open(path: &Path, read_only: bool) -> anyhow::Result<Connection> {
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    let c = Connection::open_with_flags(path, flags).with_context(|| format!("cannot open {}", path.display()))?;
    c.busy_timeout(std::time::Duration::from_secs(5))?;
    c.execute_batch("PRAGMA foreign_keys = ON; PRAGMA cache_size = -8000; PRAGMA temp_store = MEMORY; PRAGMA mmap_size = 67108864;")?;
    if !read_only {
        c.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
    }
    Ok(c)
}

pub struct Db {
    writer: Mutex<Connection>,
    readers: Mutex<Vec<Connection>>,
    permits: Arc<Semaphore>,
}

struct Lease<'a> {
    db: &'a Db,
    conn: Option<Connection>,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if let Some(c) = self.conn.take() {
            self.db.readers.lock().unwrap_or_else(|e| e.into_inner()).push(c);
        }
    }
}

impl Db {
    pub fn open(path: &Path) -> anyhow::Result<Arc<Db>> {
        let mut writer = open(path, false)?;
        migrate(&mut writer)?;
        let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(2, 8);
        let readers = (0..n).map(|_| open(path, true)).collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Arc::new(Db { writer: Mutex::new(writer), readers: Mutex::new(readers), permits: Arc::new(Semaphore::new(n)) }))
    }

    pub async fn read<T, F>(self: &Arc<Self>, f: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> anyhow::Result<T> + Send + 'static,
    {
        let permit = self.permits.clone().acquire_owned().await?;
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let conn = db.readers.lock().unwrap_or_else(|e| e.into_inner()).pop().context("no free reader")?;
            let lease = Lease { db: &db, conn: Some(conn) };
            f(lease.conn.as_ref().expect("leased"))
        })
        .await?
    }

    pub async fn write<T, F>(self: &Arc<Self>, f: F) -> anyhow::Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> anyhow::Result<T> + Send + 'static,
    {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut c = db.writer.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut c)
        })
        .await?
    }
}

fn migrate(c: &mut Connection) -> anyhow::Result<()> {
    let version: usize = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = c.transaction()?;
        tx.execute_batch(sql).with_context(|| format!("migration {}", i + 1))?;
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
    }
    Ok(())
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub role: String,
    pub avatar_url: Option<String>,
    pub twitch: bool,
    pub has_password: bool,
    pub quota_mb: Option<i64>,
    pub used_bytes: i64,
    pub disabled: bool,
    pub created_at: i64,
    pub onboarded: bool,
    pub owner: bool,
}

impl User {
    pub const COLS: &'static str = "id, username, role, avatar_url, twitch_id IS NOT NULL, password_hash IS NOT NULL, quota_mb, used_bytes, disabled, created_at, onboarded_at IS NOT NULL, owner";
    pub const NCOLS: usize = 12;

    pub fn from_row(r: &Row) -> rusqlite::Result<User> {
        Ok(User {
            id: r.get(0)?,
            username: r.get(1)?,
            role: r.get(2)?,
            avatar_url: r.get(3)?,
            twitch: r.get(4)?,
            has_password: r.get(5)?,
            quota_mb: r.get(6)?,
            used_bytes: r.get(7)?,
            disabled: r.get(8)?,
            created_at: r.get(9)?,
            onboarded: r.get(10)?,
            owner: r.get(11)?,
        })
    }

    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    pub fn may_manage(&self, target_id: i64, target_is_owner: bool) -> bool {
        self.id == target_id || (self.is_admin() && !target_is_owner)
    }

    pub fn by_id(c: &Connection, id: i64) -> rusqlite::Result<Option<User>> {
        c.query_row(&format!("SELECT {} FROM users WHERE id = ?", User::COLS), [id], User::from_row).optional()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    File,
    Paste,
    Link,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::File => "file",
            Kind::Paste => "paste",
            Kind::Link => "link",
        }
    }
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "file" => Some(Kind::File),
            "paste" => Some(Kind::Paste),
            "link" => Some(Kind::Link),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Item {
    pub id: String,
    pub user_id: i64,
    pub kind: Kind,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub sha256: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration: Option<f64>,
    pub language: Option<String>,
    pub target: Option<String>,
    pub has_thumb: bool,
    pub password_hash: Option<String>,
    pub expires_at: Option<i64>,
    pub burn: bool,
    pub max_views: Option<i64>,
    pub views: i64,
    pub album_id: Option<String>,
    pub created_at: i64,
}

impl Item {
    pub const COLS: &'static str = "id, user_id, kind, name, mime, size, sha256, width, height, duration, language, target, has_thumb, password_hash, expires_at, burn, max_views, views, album_id, created_at";

    pub fn from_row(r: &Row) -> rusqlite::Result<Item> {
        let kind: String = r.get(2)?;
        Ok(Item {
            id: r.get(0)?,
            user_id: r.get(1)?,
            kind: Kind::parse(&kind).unwrap_or(Kind::File),
            name: r.get(3)?,
            mime: r.get(4)?,
            size: r.get(5)?,
            sha256: r.get(6)?,
            width: r.get(7)?,
            height: r.get(8)?,
            duration: r.get(9)?,
            language: r.get(10)?,
            target: r.get(11)?,
            has_thumb: r.get(12)?,
            password_hash: r.get(13)?,
            expires_at: r.get(14)?,
            burn: r.get(15)?,
            max_views: r.get(16)?,
            views: r.get(17)?,
            album_id: r.get(18)?,
            created_at: r.get(19)?,
        })
    }

    pub fn get(c: &Connection, id: &str) -> rusqlite::Result<Option<Item>> {
        c.query_row(&format!("SELECT {} FROM items WHERE id = ?", Item::COLS), [id], Item::from_row).optional()
    }

    pub fn insert(&self, c: &Connection) -> rusqlite::Result<usize> {
        c.execute(
            "INSERT INTO items (id, user_id, kind, name, mime, size, sha256, width, height, duration, language, target, has_thumb, password_hash, expires_at, burn, max_views, views, album_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                self.id,
                self.user_id,
                self.kind.as_str(),
                self.name,
                self.mime,
                self.size,
                self.sha256,
                self.width,
                self.height,
                self.duration,
                self.language,
                self.target,
                self.has_thumb,
                self.password_hash,
                self.expires_at,
                self.burn,
                self.max_views,
                self.views,
                self.album_id,
                self.created_at
            ],
        )
    }

    pub fn expired(&self, now: i64) -> bool {
        self.expires_at.is_some_and(|e| e <= now) || self.max_views.is_some_and(|m| self.views >= m)
    }

    pub fn locked(&self) -> bool {
        self.password_hash.is_some()
    }

    pub fn view_type(&self) -> &'static str {
        match self.kind {
            Kind::Paste => "text",
            Kind::Link => "link",
            Kind::File => crate::media::view_type(&self.mime),
        }
    }
}

pub fn delete_items(c: &mut Connection, ids: &[String]) -> rusqlite::Result<Vec<String>> {
    let tx = c.transaction()?;
    let mut shas = Vec::new();
    {
        let mut sel = tx.prepare_cached("DELETE FROM items WHERE id = ? RETURNING sha256")?;
        for id in ids {
            if let Some(Some(sha)) = sel.query_row([id], |r| r.get::<_, Option<String>>(0)).optional()? {
                shas.push(sha);
            }
        }
        shas.sort();
        shas.dedup();
        let mut used = tx.prepare_cached("SELECT 1 FROM items WHERE sha256 = ? LIMIT 1")?;
        let mut unused = Vec::new();
        for sha in shas {
            if !used.exists([&sha])? {
                unused.push(sha);
            }
        }
        shas = unused;
    }
    tx.commit()?;
    Ok(shas)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_and_onboarding() {
        let dir = std::env::temp_dir().join(format!("uploader-test-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        {
            let mut c = open(&path, false).unwrap();
            migrate(&mut c).unwrap();
            let version: usize = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
            assert_eq!(version, MIGRATIONS.len());
            c.execute("INSERT INTO users (username, role, created_at) VALUES ('new', 'user', 1)", []).unwrap();
            let u = User::by_id(&c, c.last_insert_rowid()).unwrap().unwrap();
            assert!(!u.onboarded);
            c.execute("UPDATE users SET onboarded_at = 2 WHERE id = ?", [u.id]).unwrap();
            assert!(User::by_id(&c, u.id).unwrap().unwrap().onboarded);
            c.execute("UPDATE users SET owner = 1 WHERE id = ?", [u.id]).unwrap();
            c.execute("INSERT INTO users (username, role, created_at) VALUES ('second', 'admin', 3)", []).unwrap();
            assert!(c.execute("UPDATE users SET owner = 1 WHERE id = ?", [c.last_insert_rowid()]).is_err());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_the_owner_manages_the_owner() {
        let user = |id, role: &str, owner| User {
            id,
            username: String::new(),
            role: role.into(),
            avatar_url: None,
            twitch: false,
            has_password: true,
            quota_mb: None,
            used_bytes: 0,
            disabled: false,
            created_at: 0,
            onboarded: true,
            owner,
        };
        let (owner, admin, member) = (user(1, "admin", true), user(2, "admin", false), user(3, "user", false));
        assert!(owner.may_manage(2, false) && owner.may_manage(3, false) && owner.may_manage(1, true));
        assert!(admin.may_manage(3, false) && admin.may_manage(2, false));
        assert!(!admin.may_manage(1, true), "admins can't touch the owner");
        assert!(!member.may_manage(2, false) && member.may_manage(3, false));
    }
}
