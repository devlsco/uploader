use anyhow::{Context, bail};
use base64::Engine;
use serde_json::Value;
use sha2::{Digest, Sha256};

// ed25519 public key, releases are signed in CI
const RELEASE_KEY: &str = "98mb+aXicV/HG06Dq5NeuznLy9Zf1yxzPT6J3//uecU=";

fn verify_signature(binary: &[u8], sig: &[u8], key_b64: &str) -> anyhow::Result<()> {
    let key = base64::engine::general_purpose::STANDARD.decode(key_b64.trim()).context("the update key is not valid base64")?;
    let sig = if sig.len() == 64 { sig.to_vec() } else { base64::engine::general_purpose::STANDARD.decode(String::from_utf8_lossy(sig).trim()).context("signature is not readable")? };
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key).verify(binary, &sig).map_err(|_| anyhow::anyhow!("the signature does not match, not updating"))
}

fn github(http: &reqwest::Client, url: &str, accept: &'static str) -> reqwest::RequestBuilder {
    // only one Accept header, otherwise github sends json instead of the file
    let mut r = http.get(url).header(reqwest::header::ACCEPT, accept);
    if let Ok(t) = std::env::var("GITHUB_TOKEN")
        && !t.is_empty()
    {
        r = r.bearer_auth(t);
    }
    r
}

async fn latest_release(http: &reqwest::Client, repo: &str) -> anyhow::Result<Value> {
    let res = github(http, &format!("https://api.github.com/repos/{repo}/releases/latest"), "application/vnd.github+json").send().await?;
    if !res.status().is_success() {
        bail!("GitHub answered {} (private repositories need GITHUB_TOKEN)", res.status());
    }
    Ok(res.json().await?)
}

pub async fn latest_version(http: &reqwest::Client, repo: &str) -> anyhow::Result<String> {
    let r = latest_release(http, repo).await?;
    Ok(r["tag_name"].as_str().context("no tag")?.trim_start_matches('v').to_string())
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| v.split(['.', '-']).take(3).map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parse(latest) > parse(current)
}

pub fn in_docker() -> bool {
    std::path::Path::new("/.dockerenv").exists()
}

pub async fn install(http: &reqwest::Client, repo: &str) -> anyhow::Result<Option<String>> {
    let current = env!("CARGO_PKG_VERSION");
    let release = latest_release(http, repo).await?;
    let tag = release["tag_name"].as_str().context("no tag")?.trim_start_matches('v').to_string();
    if !is_newer(&tag, current) {
        return Ok(None);
    }
    let exe = std::env::current_exe()?;
    let new = exe.with_extension("new");
    std::fs::write(&new, b"").map_err(|_| anyhow::anyhow!("No write access to {}. Run `sudo {} update` instead.", exe.display(), exe.display()))?;
    let res = replace(http, &release, &new, &exe).await;
    if res.is_err() {
        let _ = std::fs::remove_file(&new);
    }
    res?;
    Ok(Some(tag))
}

async fn replace(http: &reqwest::Client, release: &Value, new: &std::path::Path, exe: &std::path::Path) -> anyhow::Result<()> {
    let name = format!("uploader-linux-{}", std::env::consts::ARCH);
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let find = |n: &str| assets.iter().find(|a| a["name"].as_str() == Some(n)).cloned();
    let asset = find(&name).with_context(|| format!("release has no {name}"))?;
    let sum = find(&format!("{name}.sha256")).context("release has no checksum")?;
    let sig = find(&format!("{name}.sig")).context("release is not signed, not updating")?;
    let download = |a: &Value| {
        let url = a["url"].as_str().unwrap_or_default().to_string();
        github(http, &url, "application/octet-stream").timeout(std::time::Duration::from_secs(300)).send()
    };
    let bytes = download(&asset).await?.error_for_status()?.bytes().await?;
    let text = download(&sum).await?.error_for_status()?.text().await?;
    if text.split_whitespace().next().unwrap_or_default() != hex::encode(Sha256::digest(&bytes)) {
        bail!("checksum does not match, not updating");
    }
    let sig = download(&sig).await?.error_for_status()?.bytes().await?;
    let key = std::env::var("UPLOADER_UPDATE_KEY").ok().filter(|k| !k.trim().is_empty()).unwrap_or_else(|| RELEASE_KEY.to_string());
    verify_signature(&bytes, &sig, &key)?;
    std::fs::write(new, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(new, exe)?;
    Ok(())
}

pub async fn run(repo: &str, check_only: bool) -> anyhow::Result<()> {
    let http = reqwest::Client::builder().user_agent(concat!("uploader/", env!("CARGO_PKG_VERSION"))).build()?;
    let current = env!("CARGO_PKG_VERSION");
    let latest = latest_version(&http, repo).await?;
    if !is_newer(&latest, current) {
        println!("Up to date (v{current})");
        return Ok(());
    }
    println!("New version: v{latest} (running v{current})");
    if check_only {
        return Ok(());
    }
    if in_docker() {
        println!("Running in Docker: update with `docker compose pull && docker compose up -d`");
        return Ok(());
    }
    if let Some(v) = install(&http, repo).await? {
        println!("Updated to v{v}. Restart the service, e.g. `sudo systemctl restart uploader`.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn asset_request_has_one_accept_header() {
        let req = super::github(&reqwest::Client::new(), "https://api.github.com/repos/o/r/releases/assets/1", "application/octet-stream").build().unwrap();
        let accept: Vec<_> = req.headers().get_all(reqwest::header::ACCEPT).iter().collect();
        assert_eq!(accept, ["application/octet-stream"]);
    }

    #[test]
    fn signatures() {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let key = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, pair.public_key().as_ref());
        let sig = pair.sign(b"binary");
        assert!(super::verify_signature(b"binary", sig.as_ref(), &key).is_ok());
        assert!(super::verify_signature(b"changed", sig.as_ref(), &key).is_err());
        assert!(super::verify_signature(b"binary", sig.as_ref(), super::RELEASE_KEY).is_err());
    }

    #[test]
    fn release_key_matches_ci_signatures() {
        let sig = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, "5sm3PmYGGusMWCc7v+dBqiZ7la0BhC8emk2npMMbt/IOdqkt2rbxLnHgSZzFE1wqewy6O3fadjxDfUOurUUgCA==").unwrap();
        assert!(super::verify_signature(b"uploader release key", &sig, super::RELEASE_KEY).is_ok());
    }

    #[test]
    fn versions() {
        assert!(super::is_newer("1.10.0", "1.9.3"));
        assert!(!super::is_newer("1.0.0", "1.0.0"));
        assert!(super::is_newer("0.2.0", "0.1.9"));
    }
}
