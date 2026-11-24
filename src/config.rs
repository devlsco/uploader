use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct Config {
    pub url: String,
    pub bind: SocketAddr,
    pub data_dir: PathBuf,
    pub twitch: Option<Twitch>,
    pub trust_proxy: TrustProxy,
    pub min_free_bytes: u64,
    pub secret: [u8; 32],
    pub repo: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustProxy {
    Off,
    Forwarded,
    Cloudflare,
}

impl TrustProxy {
    pub fn parse(v: Option<&str>) -> TrustProxy {
        match v.map(str::to_ascii_lowercase).as_deref() {
            None | Some("0" | "false" | "no" | "off") => TrustProxy::Off,
            Some("1" | "true" | "yes" | "on" | "forwarded") => TrustProxy::Forwarded,
            Some("cloudflare" | "cf") => TrustProxy::Cloudflare,
            Some(other) => {
                tracing::warn!("UPLOADER_TRUST_PROXY={other:?} is not known (use true or cloudflare), proxy headers are ignored");
                TrustProxy::Off
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Twitch {
    pub client_id: String,
    pub client_secret: String,
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Config> {
        let bind: SocketAddr = env("UPLOADER_BIND").unwrap_or_else(|| "0.0.0.0:3000".into()).parse().context("UPLOADER_BIND must look like 0.0.0.0:3000")?;
        let url = env("UPLOADER_URL").unwrap_or_else(|| format!("http://localhost:{}", bind.port())).trim_end_matches('/').to_string();
        anyhow::ensure!(url.starts_with("http://") || url.starts_with("https://"), "UPLOADER_URL must start with http:// or https://");
        let data_dir = PathBuf::from(env("UPLOADER_DATA").unwrap_or_else(|| "data".into()));
        std::fs::create_dir_all(&data_dir).with_context(|| format!("cannot create data directory {}", data_dir.display()))?;
        let twitch = match (env("TWITCH_CLIENT_ID"), env("TWITCH_CLIENT_SECRET")) {
            (Some(client_id), Some(client_secret)) => Some(Twitch { client_id, client_secret }),
            _ => None,
        };
        let secret = match env("UPLOADER_SECRET") {
            Some(s) => Sha256::digest(s.as_bytes()).into(),
            None => load_or_create_secret(&data_dir)?,
        };
        Ok(Config {
            url,
            bind,
            data_dir,
            twitch,
            trust_proxy: TrustProxy::parse(env("UPLOADER_TRUST_PROXY").as_deref()),
            min_free_bytes: env("UPLOADER_MIN_FREE_MB")
                .and_then(|v| {
                    let n = v.parse::<u64>().ok();
                    if n.is_none() {
                        tracing::warn!("UPLOADER_MIN_FREE_MB={v:?} is not a number, using 1024");
                    }
                    n
                })
                .unwrap_or(1024)
                .saturating_mul(1024 * 1024),
            secret,
            repo: env("UPLOADER_REPO").unwrap_or_else(|| "devlsco/uploader".into()),
        })
    }

    pub fn secure_cookies(&self) -> bool {
        self.url.starts_with("https://")
    }
}

fn load_or_create_secret(dir: &std::path::Path) -> anyhow::Result<[u8; 32]> {
    let path = dir.join("secret");
    if let Ok(b) = std::fs::read(&path)
        && b.len() == 32
    {
        let mut s = [0u8; 32];
        s.copy_from_slice(&b);
        return Ok(s);
    }
    let mut s = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut s);
    std::fs::write(&path, s).context("cannot write data/secret")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::TrustProxy;

    #[test]
    fn trust_proxy_values() {
        assert_eq!(TrustProxy::parse(None), TrustProxy::Off);
        assert_eq!(TrustProxy::parse(Some("true")), TrustProxy::Forwarded);
        assert_eq!(TrustProxy::parse(Some("Cloudflare")), TrustProxy::Cloudflare);
        assert_eq!(TrustProxy::parse(Some("maybe")), TrustProxy::Off);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Registration {
    Closed,
    Invite,
    Open,
}

pub const APP_NAME: &str = "Uploader";
pub const ID_LENGTH: usize = 6;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub registration: Registration,
    pub max_upload_mb: u64,
    pub default_quota_mb: u64,
    pub twitch_client_id: String,
    pub twitch_client_secret: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            registration: Registration::Closed,
            max_upload_mb: 1024,
            default_quota_mb: 0,
            twitch_client_id: String::new(),
            twitch_client_secret: String::new(),
        }
    }
}

impl Settings {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.max_upload_mb == 0 {
            return Err("Max upload size must be at least 1 MB".into());
        }
        self.twitch_client_id = self.twitch_client_id.trim().to_string();
        self.twitch_client_secret = self.twitch_client_secret.trim().to_string();
        let ok = |s: &str| s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric());
        if !ok(&self.twitch_client_id) || !ok(&self.twitch_client_secret) {
            return Err("Twitch client ID and secret are letters and numbers only".into());
        }
        Ok(())
    }
}
