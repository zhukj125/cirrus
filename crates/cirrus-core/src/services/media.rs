use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use reqwest::header::{CONTENT_TYPE, RANGE};
use reqwest::Url;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio::time::sleep;

use crate::apple::http::{client as hc, headers as hh};
use crate::error::{AppError, AppResult};

pub const ALLOWED_HOST_SUFFIXES: &[&str] =
    &[".icloud-content.com", ".icloud-content.com.cn", ".icloud.com", ".icloud.com.cn", ".apple-cloudkit.com"];

pub const DEFAULT_DOWNLOAD_NAME: &str = "icloud-media";

pub const DEFAULT_EXTENSION: &str = ".jpeg";

const PLACEHOLDER_FILE_NAME_PREFIX: &str = "unknown.";

#[derive(Debug, Clone)]
pub struct DownloadedMedia {
    pub bytes: Vec<u8>,

    pub file_name: String,

    pub content_type: String,
    pub size: usize,
}

#[derive(Debug, Clone)]
pub struct DownloadedFile {
    pub path: PathBuf,
    pub size: u64,
    pub file_name: String,
    pub content_type: String,
}

pub struct DownloadRateLimiter {
    state: Mutex<RateLimiterState>,
}

struct RateLimiterState {
    limit_bps: u64,
    tokens: f64,
    last_refill: Instant,
}

impl DownloadRateLimiter {
    pub fn new(limit_bps: u64) -> Self {
        Self { state: Mutex::new(RateLimiterState { limit_bps, tokens: 0.0, last_refill: Instant::now() }) }
    }

    pub async fn set_limit(&self, limit_bps: u64) {
        let mut state = self.state.lock().await;
        state.limit_bps = limit_bps;
        state.tokens = 0.0;
        state.last_refill = Instant::now();
    }

    pub async fn wait_for(&self, bytes: u64) {
        self.wait_for_cancellable(bytes, None).await;
    }

    pub async fn wait_for_cancellable(&self, bytes: u64, cancelled: Option<&AtomicBool>) -> bool {
        if bytes == 0 {
            return true;
        }
        loop {
            if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                return false;
            }
            let wait = {
                let mut state = self.state.lock().await;
                if state.limit_bps == 0 {
                    return true;
                }
                let now = Instant::now();
                let elapsed = now.duration_since(state.last_refill).as_secs_f64();
                let capacity = state.limit_bps as f64;
                state.tokens = (state.tokens + elapsed * state.limit_bps as f64).min(capacity);
                state.last_refill = now;

                let requested = bytes as f64;
                if state.tokens >= requested {
                    state.tokens -= requested;
                    return true;
                }
                let missing = requested - state.tokens;
                Duration::from_secs_f64(missing / state.limit_bps as f64)
            };
            sleep(wait.min(Duration::from_millis(50))).await;
        }
    }
}

pub async fn fetch_media(url: &str, name: Option<&str>) -> AppResult<DownloadedMedia> {
    fetch_media_with_progress(url, name, None, None, |_, _| {}).await
}

async fn next_response_chunk(
    response: &mut reqwest::Response,
    cancelled: Option<&AtomicBool>,
) -> AppResult<Option<Vec<u8>>> {
    let next = match cancelled {
        Some(flag) => {
            tokio::select! {
                chunk = response.chunk() => chunk,
                _ = async {
                    while !flag.load(Ordering::Relaxed) {
                        sleep(Duration::from_millis(20)).await;
                    }
                } => return Err(AppError::other("Download paused")),
            }
        }
        None => response.chunk().await,
    };
    next.map(|chunk| chunk.map(|bytes| bytes.to_vec()))
        .map_err(|error| AppError::other(format!("Download failed: {error}")))
}

pub async fn fetch_media_with_progress<F>(
    url: &str,
    name: Option<&str>,
    limiter: Option<&DownloadRateLimiter>,
    cancelled: Option<&AtomicBool>,
    mut on_progress: F,
) -> AppResult<DownloadedMedia>
where
    F: FnMut(u64, Option<u64>),
{
    let parsed = validate_media_url(url)?;

    let client = reqwest::Client::builder()
        .connect_timeout(hc::MEDIA_CONNECT_TIMEOUT)
        .timeout(hc::MEDIA_TIMEOUT)
        .redirect(reqwest::redirect::Policy::default())
        .build()
        .map_err(|error| AppError::other(format!("Download failed: {error}")))?;

    let mut response = client
        .get(parsed)
        .header(hh::USER_AGENT, hc::USER_AGENT)
        .send()
        .await
        .map_err(|error| AppError::other(format!("Download failed: {error}")))?;

    let status = response.status().as_u16();
    if status != 200 {
        let host = response.url().host_str().unwrap_or("").to_string();
        log::warn!("[MEDIA] upstream returned HTTP {status}: {host}");
        return Err(AppError::other(format!("Upstream returned HTTP {status}")));
    }

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let total_bytes = response.content_length();

    let mut bytes = Vec::new();
    let mut downloaded = 0u64;
    let mut last_emit = Instant::now();
    on_progress(0, total_bytes);
    while let Some(chunk) = next_response_chunk(&mut response, cancelled).await? {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(AppError::other("Download paused"));
        }
        if let Some(limiter) = limiter {
            if !limiter.wait_for_cancellable(chunk.len() as u64, cancelled).await {
                return Err(AppError::other("Download paused"));
            }
        }
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(AppError::other("Download paused"));
        }
        bytes.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() >= Duration::from_millis(100) {
            on_progress(downloaded, total_bytes);
            last_emit = Instant::now();
        }
    }
    on_progress(downloaded, total_bytes.or(Some(downloaded)));

    Ok(DownloadedMedia { size: bytes.len(), bytes, file_name: guess_file_name(url, name), content_type })
}

pub fn partial_file_path(target_path: &Path, url: &str) -> PathBuf {
    let file_name = target_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_DOWNLOAD_NAME.to_string());
    let digest = format!("{:x}", Sha256::digest(url.as_bytes()));
    target_path.with_file_name(format!(".{file_name}.cirrus-{}.part", &digest[..12]))
}

pub async fn fetch_media_to_file_with_progress<F>(
    url: &str,
    name: Option<&str>,
    target_path: &Path,
    limiter: Option<&DownloadRateLimiter>,
    cancelled: Option<&AtomicBool>,
    mut on_progress: F,
) -> AppResult<DownloadedFile>
where
    F: FnMut(u64, Option<u64>),
{
    let parsed = validate_media_url(url)?;
    if let Some(parent) = target_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let part_path = partial_file_path(target_path, url);
    let existing = std::fs::metadata(&part_path).map(|metadata| metadata.len()).unwrap_or(0);
    let client = reqwest::Client::builder()
        .connect_timeout(hc::MEDIA_CONNECT_TIMEOUT)
        .timeout(hc::MEDIA_TIMEOUT)
        .redirect(reqwest::redirect::Policy::default())
        .build()
        .map_err(|error| AppError::other(format!("Download failed: {error}")))?;

    let mut request = client.get(parsed).header(hh::USER_AGENT, hc::USER_AGENT);
    if existing > 0 {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }
    let mut response = request.send().await.map_err(|error| AppError::other(format!("Download failed: {error}")))?;

    let status = response.status().as_u16();
    let (start, total_bytes) = match status {
        200 => (0, response.content_length()),
        206 if existing > 0 => (existing, response.content_length().map(|length| existing.saturating_add(length))),
        _ => {
            let host = response.url().host_str().unwrap_or("").to_string();
            log::warn!("[MEDIA] upstream returned HTTP {status}: {host}");
            return Err(AppError::other(format!("Upstream returned HTTP {status}")));
        }
    };

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let mut file =
        std::fs::OpenOptions::new().create(true).write(true).append(start > 0).truncate(start == 0).open(&part_path)?;
    let mut downloaded = start;
    let mut last_emit = Instant::now();
    on_progress(downloaded, total_bytes);
    while let Some(chunk) = next_response_chunk(&mut response, cancelled).await? {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            let _ = file.flush();
            return Err(AppError::other("Download paused"));
        }
        if let Some(limiter) = limiter {
            if !limiter.wait_for_cancellable(chunk.len() as u64, cancelled).await {
                return Err(AppError::other("Download paused"));
            }
        }
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            let _ = file.flush();
            return Err(AppError::other("Download paused"));
        }
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() >= Duration::from_millis(100) {
            file.flush()?;
            on_progress(downloaded, total_bytes);
            last_emit = Instant::now();
        }
    }
    file.flush()?;
    on_progress(downloaded, total_bytes.or(Some(downloaded)));

    Ok(DownloadedFile { path: part_path, size: downloaded, file_name: guess_file_name(url, name), content_type })
}
pub async fn save_media_bytes(bytes: &[u8], target_path: &Path) -> AppResult<String> {
    if let Some(parent) = target_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(target_path, bytes)?;

    let absolute =
        if target_path.is_absolute() { target_path.to_path_buf() } else { std::env::current_dir()?.join(target_path) };
    Ok(absolute.to_string_lossy().to_string())
}

pub fn validate_media_url(url: &str) -> AppResult<Url> {
    if url.trim().is_empty() {
        return Err(AppError::other("Missing url parameter"));
    }
    let parsed = Url::parse(url).map_err(|_| AppError::other("Invalid URL"))?;
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    let allowed = parsed.scheme().eq_ignore_ascii_case("https")
        && ALLOWED_HOST_SUFFIXES.iter().any(|suffix| host.ends_with(suffix));
    if !allowed {
        return Err(AppError::other(format!("Proxying this host is not allowed: {host}")));
    }
    Ok(parsed)
}

pub fn guess_file_name(url: &str, name: Option<&str>) -> String {
    let extension = extension_from_url(url);
    let candidate = name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| sanitize_file_name(last_path_segment(value)))
        .filter(|value| !value.is_empty() && value != "." && value != "..")
        .filter(|value| !value.to_ascii_lowercase().starts_with(PLACEHOLDER_FILE_NAME_PREFIX));

    let fallback_extension = || extension.clone().unwrap_or_else(|| DEFAULT_EXTENSION.to_string());
    match candidate {
        Some(value) if has_extension(&value) => value,
        Some(value) => format!("{value}{}", fallback_extension()),
        None => format!("{DEFAULT_DOWNLOAD_NAME}{}", fallback_extension()),
    }
}

fn extension_from_url(url: &str) -> Option<String> {
    let without_query = match url.find(['?', '#']) {
        Some(index) => &url[..index],
        None => url,
    };
    let segment = without_query.rsplit('/').find(|part| !part.is_empty())?;
    let dot = segment.rfind('.')?;

    if dot == 0 {
        return None;
    }
    let extension = &segment[dot..];

    if extension.len() <= 1 || extension.len() > 10 {
        return None;
    }
    if !extension[1..].chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return None;
    }
    Some(extension.to_ascii_lowercase())
}

fn has_extension(name: &str) -> bool {
    match name.rfind('.') {
        Some(index) => {
            index > 0 && index + 1 < name.len() && name[index + 1..].chars().all(|ch| ch.is_ascii_alphanumeric())
        }
        None => false,
    }
}

fn last_path_segment(value: &str) -> &str {
    value.rsplit(['/', '\\']).next().unwrap_or(value)
}

fn sanitize_file_name(file_name: &str) -> String {
    file_name
        .chars()
        .map(|ch| match ch {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\r' | '\n' => '_',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_only_apple_https_hosts() {
        assert!(validate_media_url("https://cvws.icloud-content.com.cn/B/AAA/unknown.jpeg").is_ok());
        assert!(validate_media_url("https://cvws.icloud-content.com/B/AAA/unknown.jpeg").is_ok());
        assert!(validate_media_url("https://p232-ckdatabasews.icloud.com/x").is_ok());
        assert!(validate_media_url("https://cdn.apple-cloudkit.com/x").is_ok());

        assert!(validate_media_url("http://cvws.icloud-content.com/B/AAA").is_err());
        assert!(validate_media_url("https://icloud.com/x").is_err());
        assert!(validate_media_url("https://evil-icloud.com/x").is_err());

        assert!(validate_media_url("https://p01-apple-cloudkit.com/x").is_err());
        assert!(validate_media_url("https://example.com/x").is_err());
        assert!(validate_media_url("https://icloud.com.evil.net/x").is_err());
        assert!(validate_media_url("").is_err());
        assert!(validate_media_url("not a url").is_err());
    }

    #[test]
    fn guesses_safe_file_names() {
        assert_eq!(
            guess_file_name("https://cvws.icloud-content.com.cn/B/AAA/${f}?o=1", Some("IMG_0001.HEIC")),
            "IMG_0001.HEIC"
        );

        assert_eq!(guess_file_name("https://cvws.icloud-content.com/B/AAA/${f}", Some("../../evil.jpg")), "evil.jpg");
        assert_eq!(guess_file_name("https://cvws.icloud-content.com/B/AAA/${f}", Some(r"..\..\evil.jpg")), "evil.jpg");

        assert_eq!(guess_file_name("https://cvws.icloud-content.com/B/AAA/${f}", Some("a:b*c?.jpg")), "a_b_c_.jpg");

        assert_eq!(guess_file_name("https://cvws.icloud-content.com/B/AAA/unknown.jpeg", Some("photo")), "photo.jpeg");

        assert_eq!(guess_file_name("https://cvws.icloud-content.com/B/AAA/unknown.jpeg", None), "icloud-media.jpeg");
        assert_eq!(
            guess_file_name("https://cvws.icloud-content.com/B/AAA/unknown.mp4", Some("unknown.mp4")),
            "icloud-media.mp4"
        );

        assert_eq!(guess_file_name("", None), "icloud-media.jpeg");
        assert_eq!(guess_file_name("https://cvws.icloud.com/B/AAA/", Some("   ")), "icloud-media.jpeg");
    }

    #[test]
    fn extracts_extension_from_last_segment_only() {
        assert_eq!(extension_from_url("https://a.com/x/y.MOV").as_deref(), Some(".mov"));
        assert_eq!(extension_from_url("https://a.com/x/${f}?o=1"), None);
        assert_eq!(extension_from_url("https://a.com/x/name.jpeg#frag").as_deref(), Some(".jpeg"));
        assert_eq!(extension_from_url("https://a.com/x/noext").as_deref(), None);
        assert_eq!(extension_from_url("https://a.com/.hidden").as_deref(), None);
    }

    #[tokio::test]
    async fn shared_rate_limiter_delays_only_when_limited() {
        let unlimited = DownloadRateLimiter::new(0);
        let started = Instant::now();
        unlimited.wait_for(1_000_000).await;
        assert!(started.elapsed() < Duration::from_millis(50));

        let limited = DownloadRateLimiter::new(1_000_000);
        let started = Instant::now();
        limited.wait_for(100_000).await;
        assert!(started.elapsed() >= Duration::from_millis(70));
        limited.set_limit(0).await;
        limited.wait_for(10_000_000).await;
    }

    #[test]
    fn partial_file_paths_are_stable_and_url_specific() {
        let target = Path::new("C:/downloads/photo.jpeg");
        let first = partial_file_path(target, "https://cvws.icloud-content.com/B/AAA/photo.jpeg?one=1");
        let same = partial_file_path(target, "https://cvws.icloud-content.com/B/AAA/photo.jpeg?one=1");
        let other = partial_file_path(target, "https://cvws.icloud-content.com/B/BBB/photo.jpeg?one=1");
        assert_eq!(first, same);
        assert_ne!(first, other);
        assert!(first.to_string_lossy().ends_with(".part"));
    }

    #[tokio::test]
    async fn saves_bytes_and_creates_parent_directory() {
        let dir = std::env::temp_dir().join(format!("cirrus-test-{}", uuid::Uuid::new_v4()));
        let target = dir.join("nested").join("photo.jpeg");
        let saved = save_media_bytes(b"hello", &target).await.expect("写盘失败");
        assert_eq!(std::fs::read(&target).expect("读回失败"), b"hello");
        assert!(Path::new(&saved).is_absolute());
        assert!(saved.ends_with("photo.jpeg"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
