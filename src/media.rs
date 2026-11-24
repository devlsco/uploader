use std::io::Cursor;
use std::path::Path;
use std::sync::OnceLock;

use bytes::Bytes;
use image::{DynamicImage, GenericImageView, ImageReader, Limits};
use img_parts::{ImageEXIF, jpeg::Jpeg, png::Png, webp::WebP};

const THUMB: u32 = 480;

const IMAGE_MEMORY: u64 = 160 * 1024 * 1024;

const FFMPEG_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

// no playlists (hls, concat) so ffmpeg can't read other files
const FFMPEG_FORMATS: &str = "mov,mp4,m4a,3gp,3g2,mj2,matroska,webm,ogg,mp3,wav,flac,aac,asf,aiff,w64,wv,ape,amr,caf,avi,flv,mpeg,mpegts";

pub fn sniff(head: &[u8], name: &str) -> String {
    if let Some(t) = infer::get(head) {
        let m = t.mime_type();
        return m.to_string();
    }
    if looks_like_text(head) {
        let guess = mime_guess::from_path(name).first();
        return match guess {
            Some(g) if g.type_() == "text" || is_text_app(g.essence_str()) => g.essence_str().to_string(),
            _ => "text/plain".to_string(),
        };
    }
    mime_guess::from_path(name).first().map(|g| g.essence_str().to_string()).unwrap_or_else(|| "application/octet-stream".into())
}

fn is_text_app(m: &str) -> bool {
    matches!(m, "application/json" | "application/javascript" | "application/xml" | "application/x-sh" | "application/toml" | "application/x-yaml" | "application/sql" | "image/svg+xml")
}

fn looks_like_text(head: &[u8]) -> bool {
    if head.is_empty() || head.contains(&0) {
        return head.is_empty();
    }
    match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none() && head.len() - e.valid_up_to() < 4,
    }
}

pub fn view_type(mime: &str) -> &'static str {
    if mime == "image/svg+xml" {
        return "image";
    }
    match mime.split('/').next().unwrap_or("") {
        "image" if is_web_image(mime) => "image",
        "video" if matches!(mime, "video/mp4" | "video/webm" | "video/ogg" | "video/quicktime") => "video",
        "audio" => "audio",
        "text" => "text",
        _ if mime == "application/pdf" => "pdf",
        _ if is_text_app(mime) => "text",
        _ => "other",
    }
}

fn is_web_image(mime: &str) -> bool {
    matches!(mime, "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/avif" | "image/bmp" | "image/x-icon" | "image/vnd.microsoft.icon" | "image/svg+xml")
}

pub fn extension(mime: &str, name: &str) -> String {
    let from_name = Path::new(name).extension().and_then(|e| e.to_str()).filter(|e| e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric()));
    let from_mime = match mime {
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/avif" => Some("avif"),
        "video/mp4" => Some("mp4"),
        "video/webm" => Some("webm"),
        "video/quicktime" => Some("mov"),
        "audio/mpeg" => Some("mp3"),
        "audio/ogg" => Some("ogg"),
        "audio/x-wav" | "audio/wav" => Some("wav"),
        "audio/x-flac" | "audio/flac" => Some("flac"),
        "application/pdf" => Some("pdf"),
        _ => None,
    };
    match (from_mime, from_name) {
        (Some(m), _) => m.to_string(),
        (None, Some(n)) => n.to_lowercase(),
        (None, None) if mime.starts_with("text/") => "txt".into(),
        _ => "bin".into(),
    }
}

fn orientation_exif(orientation: u16) -> Bytes {
    let mut b = Vec::with_capacity(26);
    b.extend_from_slice(b"II*\0");
    b.extend_from_slice(&8u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&0x0112u16.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&orientation.to_le_bytes());
    b.extend_from_slice(&[0, 0]);
    b.extend_from_slice(&0u32.to_le_bytes());
    Bytes::from(b)
}

fn read_orientation(exif: &[u8]) -> Option<u16> {
    let d = exif.strip_prefix(b"Exif\0\0").unwrap_or(exif);
    if d.len() < 8 {
        return None;
    }
    let le = match &d[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |i: usize| d.get(i..i + 2).map(|s| if le { u16::from_le_bytes([s[0], s[1]]) } else { u16::from_be_bytes([s[0], s[1]]) });
    let u32_at = |i: usize| d.get(i..i + 4).map(|s| if le { u32::from_le_bytes([s[0], s[1], s[2], s[3]]) } else { u32::from_be_bytes([s[0], s[1], s[2], s[3]]) });
    let ifd = u32_at(4)? as usize;
    let n = u16_at(ifd)? as usize;
    for i in 0..n.min(64) {
        let e = ifd + 2 + i * 12;
        if u16_at(e)? == 0x0112 {
            return u16_at(e + 8).filter(|o| (1..=8).contains(o));
        }
    }
    None
}

pub fn strip_exif(mime: &str, data: Bytes) -> Option<Bytes> {
    fn swap<T: ImageEXIF>(img: &mut T) -> bool {
        let Some(old) = img.exif() else { return false };
        let keep = read_orientation(&old).filter(|o| *o != 1).map(orientation_exif);
        img.set_exif(keep);
        true
    }
    let mut out = Vec::new();
    match mime {
        "image/jpeg" => {
            let mut j = Jpeg::from_bytes(data).ok()?;
            if !swap(&mut j) {
                return None;
            }
            j.encoder().write_to(&mut out).ok()?;
        }
        "image/png" => {
            let mut p = Png::from_bytes(data).ok()?;
            if !swap(&mut p) {
                return None;
            }
            p.encoder().write_to(&mut out).ok()?;
        }
        "image/webp" => {
            let mut w = WebP::from_bytes(data).ok()?;
            if !swap(&mut w) {
                return None;
            }
            w.encoder().write_to(&mut out).ok()?;
        }
        _ => return None,
    }
    Some(Bytes::from(out))
}

pub struct Thumb {
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration: Option<f64>,
    pub made: bool,
}

pub fn thumbnail(src: &Path, mime: &str, dest: &Path) -> anyhow::Result<Thumb> {
    std::fs::create_dir_all(dest.parent().expect("has parent"))?;
    match view_type(mime) {
        "image" if mime != "image/svg+xml" => image_thumb(src, mime == "image/jpeg", dest),
        "video" | "audio" => ffmpeg_thumb(src, mime, dest),
        _ => Ok(Thumb { width: None, height: None, duration: None, made: false }),
    }
}

fn image_thumb(src: &Path, jpeg: bool, dest: &Path) -> anyhow::Result<Thumb> {
    let mut reader = ImageReader::open(src)?.with_guessed_format()?;
    let mut limits = Limits::default();
    limits.max_alloc = Some(IMAGE_MEMORY);
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    reader.limits(limits);
    let img = reader.decode()?;
    let (mut w, mut h) = img.dimensions();
    let small = if w > THUMB || h > THUMB { img.thumbnail(THUMB, THUMB) } else { img };
    let small = if jpeg {
        let o = orientation(src);
        if (5..=8).contains(&o) {
            std::mem::swap(&mut w, &mut h);
        }
        orient(small, o)
    } else {
        small
    };
    write_jpeg(&small, dest)?;
    Ok(Thumb { width: Some(w as i64), height: Some(h as i64), duration: None, made: true })
}

fn orientation(src: &Path) -> u16 {
    use std::io::Read;
    let mut head = Vec::new();
    let Ok(f) = std::fs::File::open(src) else { return 1 };
    if f.take(256 * 1024).read_to_end(&mut head).is_err() {
        return 1;
    }
    jpeg_exif(&head).and_then(read_orientation).unwrap_or(1)
}

fn jpeg_exif(d: &[u8]) -> Option<&[u8]> {
    if d.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    while i + 4 <= d.len() {
        if d[i] != 0xFF {
            return None;
        }
        let marker = d[i + 1];
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
        let body = d.get(i + 4..i + 2 + len)?;
        if marker == 0xE1 && body.starts_with(b"Exif\0\0") {
            return Some(body);
        }
        i += 2 + len;
    }
    None
}

fn orient(img: DynamicImage, o: u16) -> DynamicImage {
    match o {
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.rotate90().fliph(),
        6 => img.rotate90(),
        7 => img.rotate270().fliph(),
        8 => img.rotate270(),
        _ => img,
    }
}

fn write_jpeg(img: &DynamicImage, dest: &Path) -> anyhow::Result<()> {
    let rgb = if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        let mut out = image::RgbImage::new(rgba.width(), rgba.height());
        for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
            let a = p[3] as u32;
            *o = image::Rgb([0, 1, 2].map(|i| ((p[i] as u32 * a + 255 * (255 - a)) / 255) as u8));
        }
        out
    } else {
        img.to_rgb8()
    };
    let mut buf = Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 80).encode_image(&rgb)?;
    let tmp = dest.with_extension("tmp");
    std::fs::write(&tmp, buf.into_inner())?;
    std::fs::rename(tmp, dest)?;
    Ok(())
}

fn has_ffmpeg() -> bool {
    static FOUND: OnceLock<bool> = OnceLock::new();
    *FOUND.get_or_init(|| {
        let ok = std::process::Command::new("ffprobe").arg("-version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success());
        if !ok {
            tracing::info!("ffmpeg not found: videos and audio get no thumbnails");
        }
        ok
    })
}

fn ffmpeg_thumb(src: &Path, mime: &str, dest: &Path) -> anyhow::Result<Thumb> {
    let mut t = Thumb { width: None, height: None, duration: None, made: false };
    if !has_ffmpeg() {
        return Ok(t);
    }
    let input = ffmpeg_input(src)?;
    let mut probe = std::process::Command::new("ffprobe");
    probe
        .args(["-v", "error", "-protocol_whitelist", "file", "-format_whitelist", FFMPEG_FORMATS, "-select_streams", "v:0", "-show_entries", "stream=width,height:format=duration", "-of", "json"])
        .arg(&input);
    let probe = run_limited(probe)?;
    if probe.status.success() {
        let v: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap_or_default();
        t.duration = v["format"]["duration"].as_str().and_then(|d| d.parse().ok());
        if mime.starts_with("video/") {
            t.width = v["streams"][0]["width"].as_i64();
            t.height = v["streams"][0]["height"].as_i64();
        }
    }
    let tmp = dest.with_extension("tmp.jpg");
    let seek = if mime.starts_with("video/") && t.duration.is_some_and(|d| d > 2.0) { "1" } else { "0" };
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args(["-nostdin", "-v", "error", "-y", "-protocol_whitelist", "file", "-format_whitelist", FFMPEG_FORMATS, "-ss", seek, "-i"])
        .arg(&input)
        .args(["-an", "-sn", "-dn", "-frames:v", "1", "-vf", &format!("scale='min({THUMB},iw)':'min({THUMB},ih)':force_original_aspect_ratio=decrease"), "-q:v", "4", "-f", "image2"])
        .arg(&tmp);
    let out = run_limited(cmd)?;
    if out.status.success() && tmp.exists() {
        std::fs::rename(&tmp, dest)?;
        t.made = true;
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(t)
}

fn ffmpeg_input(src: &Path) -> anyhow::Result<std::ffi::OsString> {
    let mut s = std::ffi::OsString::from("file:");
    s.push(src.as_os_str());
    Ok(s)
}

fn run_limited(mut cmd: std::process::Command) -> anyhow::Result<std::process::Output> {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut stdout).take(1024 * 1024).read_to_end(&mut buf);
        buf
    });
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if started.elapsed() > FFMPEG_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("ffmpeg took too long");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let stdout = reader.join().unwrap_or_default();
    Ok(std::process::Output { status, stdout, stderr: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exif_from_jpeg_head() {
        let exif = orientation_exif(6);
        let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE1];
        jpg.extend_from_slice(&((exif.len() + 8) as u16).to_be_bytes());
        jpg.extend_from_slice(b"Exif\0\0");
        jpg.extend_from_slice(&exif);
        jpg.extend_from_slice(&[0xFF, 0xDA, 0, 2]);
        assert_eq!(jpeg_exif(&jpg).and_then(read_orientation), Some(6));
        assert_eq!(jpeg_exif(b"not a jpeg"), None);
    }

    #[test]
    fn detects_types() {
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n0000", "x"), "image/png");
        assert_eq!(sniff(b"fn main() {}", "main.rs"), "text/x-rust");
        assert_eq!(sniff(b"hello world", "notes"), "text/plain");
        assert_eq!(sniff(b"{\"a\":1}", "a.json"), "application/json");
        assert_eq!(sniff(&[0, 1, 2, 3, 0xff], "blob"), "application/octet-stream");
        assert_eq!(sniff("grüß".as_bytes().split_at(5).0, "a.txt"), "text/plain");
    }

    #[test]
    fn view_types() {
        assert_eq!(view_type("image/png"), "image");
        assert_eq!(view_type("image/tiff"), "other");
        assert_eq!(view_type("video/mp4"), "video");
        assert_eq!(view_type("application/pdf"), "pdf");
        assert_eq!(view_type("application/json"), "text");
        assert_eq!(view_type("application/zip"), "other");
    }

    #[test]
    fn extensions() {
        assert_eq!(extension("image/jpeg", "photo.jpeg"), "jpg");
        assert_eq!(extension("text/plain", "main.rs"), "rs");
        assert_eq!(extension("text/plain", "notes"), "txt");
        assert_eq!(extension("application/octet-stream", "x"), "bin");
    }

    #[test]
    fn orientation_round_trip() {
        assert_eq!(read_orientation(&orientation_exif(6)), Some(6));
        assert_eq!(read_orientation(b"nonsense"), None);
    }
}
