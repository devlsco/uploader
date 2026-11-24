use std::path::{Path, PathBuf};

use rand::Rng;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct Storage {
    root: PathBuf,
    min_free: u64,
}

const FREE_CHECK_EVERY: u64 = 16 * 1024 * 1024;

pub const SNIFF: usize = 8192;

impl Storage {
    pub fn new(root: &Path, min_free: u64) -> anyhow::Result<Storage> {
        for d in ["files", "thumbs", "tmp"] {
            std::fs::create_dir_all(root.join(d))?;
        }
        Ok(Storage { root: root.to_path_buf(), min_free })
    }

    pub fn space_left(&self) -> Option<u64> {
        free_bytes(&self.root).map(|f| f.saturating_sub(self.min_free))
    }

    pub async fn incoming(&self, limit: u64) -> anyhow::Result<Incoming> {
        if self.space_left() == Some(0) {
            return Err(anyhow::Error::new(DiskFullMarker));
        }
        Incoming::create(self.tmp(), limit, self.clone()).await
    }

    pub async fn resume(&self, path: PathBuf, hasher: Sha256, size: u64, head: Vec<u8>, limit: u64) -> std::io::Result<Incoming> {
        let file = tokio::fs::OpenOptions::new().append(true).open(&path).await?;
        Ok(Incoming { path, file: tokio::io::BufWriter::with_capacity(256 * 1024, file), hasher, size, head, limit, store: self.clone(), next_check: size + FREE_CHECK_EVERY })
    }

    pub fn file(&self, sha: &str) -> PathBuf {
        self.root.join("files").join(&sha[..2]).join(sha)
    }

    pub fn thumb(&self, sha: &str) -> PathBuf {
        self.root.join("thumbs").join(&sha[..2]).join(format!("{sha}.jpg"))
    }

    pub fn tmp(&self) -> PathBuf {
        let n: u64 = rand::thread_rng().r#gen();
        self.root.join("tmp").join(format!("{n:016x}"))
    }

    pub fn files_dir(&self) -> PathBuf {
        self.root.join("files")
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn commit(&self, tmp: &Path, sha: &str) -> std::io::Result<()> {
        let dest = self.file(sha);
        if dest.exists() {
            let _ = std::fs::remove_file(tmp);
            return Ok(());
        }
        std::fs::create_dir_all(dest.parent().expect("has parent"))?;
        std::fs::rename(tmp, &dest)
    }

    pub fn remove(&self, shas: &[String]) {
        for sha in shas {
            let _ = std::fs::remove_file(self.file(sha));
            let _ = std::fs::remove_file(self.thumb(sha));
        }
    }
}

pub struct Incoming {
    pub path: PathBuf,
    file: tokio::io::BufWriter<tokio::fs::File>,
    hasher: Sha256,
    pub size: u64,
    pub head: Vec<u8>,
    limit: u64,
    store: Storage,
    next_check: u64,
}

pub struct Finished {
    pub path: PathBuf,
    pub sha: String,
    pub size: u64,
    pub head: Vec<u8>,
}

impl Incoming {
    async fn create(path: PathBuf, limit: u64, store: Storage) -> anyhow::Result<Incoming> {
        let file = tokio::fs::File::create(&path).await?;
        Ok(Incoming { path, file: tokio::io::BufWriter::with_capacity(256 * 1024, file), hasher: Sha256::new(), size: 0, head: Vec::new(), limit, store, next_check: FREE_CHECK_EVERY })
    }

    pub async fn write(&mut self, chunk: &[u8]) -> anyhow::Result<()> {
        if self.size + chunk.len() as u64 > self.limit {
            return Err(anyhow::Error::new(TooLargeMarker));
        }
        if self.size + chunk.len() as u64 >= self.next_check {
            self.next_check = self.size + FREE_CHECK_EVERY;
            if self.store.space_left() == Some(0) {
                return Err(anyhow::Error::new(DiskFullMarker));
            }
        }
        if self.head.len() < SNIFF {
            let n = (SNIFF - self.head.len()).min(chunk.len());
            self.head.extend_from_slice(&chunk[..n]);
        }
        self.hasher.update(chunk);
        self.size += chunk.len() as u64;
        self.file.write_all(chunk).await?;
        Ok(())
    }

    pub async fn pause(mut self) -> std::io::Result<(PathBuf, Sha256, u64, Vec<u8>)> {
        self.file.flush().await?;
        Ok((self.path, self.hasher, self.size, self.head))
    }

    pub async fn finish(mut self) -> std::io::Result<Finished> {
        self.file.flush().await?;
        self.file.get_mut().sync_data().await.ok();
        Ok(Finished { path: self.path, sha: hex::encode(self.hasher.finalize()), size: self.size, head: self.head })
    }

    pub async fn discard(self) {
        let path = self.path.clone();
        drop(self);
        let _ = tokio::fs::remove_file(path).await;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TooLargeMarker;

impl std::fmt::Display for TooLargeMarker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("file too large")
    }
}

impl std::error::Error for TooLargeMarker {}

pub fn is_too_large(e: &anyhow::Error) -> bool {
    e.downcast_ref::<TooLargeMarker>().is_some()
}

#[derive(Debug, Clone, Copy)]
pub struct DiskFullMarker;

impl std::fmt::Display for DiskFullMarker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("disk full")
    }
}

impl std::error::Error for DiskFullMarker {}

pub fn is_disk_full(e: &anyhow::Error) -> bool {
    e.downcast_ref::<DiskFullMarker>().is_some() || e.downcast_ref::<std::io::Error>().is_some_and(|io| io.raw_os_error() == Some(28))
}

#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
pub fn free_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs only writes into the struct we pass; the path is a valid C string.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    Some((st.f_bavail as u64).saturating_mul(st.f_frsize as u64))
}

#[cfg(not(unix))]
pub fn free_bytes(_path: &Path) -> Option<u64> {
    None
}

pub fn hash_file(path: &Path) -> std::io::Result<(String, u64)> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let n = std::io::copy(&mut f, &mut h)?;
    Ok((hex::encode(h.finalize()), n))
}
