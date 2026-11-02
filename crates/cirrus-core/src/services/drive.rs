use std::path::{Path, PathBuf};

use reqwest::{Method, Url};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{DriveItem, DriveListing};
use crate::services::{devices, media};
use crate::session::SrpSession;
use crate::SessionManager;

const DRIVE_ROOT: &str = "root";
const DRIVE_PAGE_LIMIT: &str = "100";
const DOCWS_CLIENT_BUILD: &str = "2640Build24";
const DOCWS_CLIENT_MASTERING: &str = "2640Build24";

pub struct DownloadedDriveFile {
    pub path: PathBuf,
    pub file_name: String,
    pub size_bytes: u64,
}

pub async fn list(manager: &SessionManager, session_id: &str, folder_id: Option<String>) -> DriveListing {
    let folder_id = folder_id.filter(|value| !value.trim().is_empty()).unwrap_or_else(|| DRIVE_ROOT.to_string());
    if session_id.trim().is_empty() {
        return DriveListing::error(folder_id, "Missing session ID. Please sign in again.");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return DriveListing::error(folder_id, error.to_string()),
    };
    let mut session = handle.lock().await;
    if let Err(error) = devices::account_login(&mut session, false).await {
        return DriveListing::error(folder_id, error.to_string());
    }
    match list_with_session(&session, &folder_id).await {
        Ok(items) => DriveListing::ok(folder_id, items, None),
        Err(error) => DriveListing::error(folder_id, error.to_string()),
    }
}

async fn list_with_session(session: &SrpSession, folder_id: &str) -> AppResult<Vec<DriveItem>> {
    let base = session.docws_url().ok_or_else(|| AppError::other("iCloud Drive is not available for this account"))?;
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for kind in ["folders", "files"] {
        let url = enumerate_url(base, session, kind, folder_id)?;
        match session.request_shared(Method::GET, url.as_str(), None, &[]).await {
            Ok(response) if response.is_success() => {
                let value = response.json();
                if let Some(values) = value.get("drive_item").and_then(Value::as_array) {
                    items.extend(values.iter().filter_map(drive_item_from_value));
                }
            }
            Ok(response) => errors.push(format!("{} returned HTTP {}", kind, response.status)),
            Err(error) => errors.push(error.to_string()),
        }
    }
    if items.is_empty() && !errors.is_empty() {
        return Err(AppError::other(errors.join("; ")));
    }
    items.sort_by(|left, right| {
        right.is_folder().cmp(&left.is_folder()).then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    items.dedup_by(|left, right| left.drivewsid == right.drivewsid);
    Ok(items)
}

fn enumerate_url(base: &str, session: &SrpSession, kind: &str, folder_id: &str) -> AppResult<Url> {
    let mut url = Url::parse(base).map_err(|error| AppError::other(format!("Invalid iCloud Drive URL: {error}")))?;
    {
        let mut segments = url.path_segments_mut().map_err(|_| AppError::other("Invalid iCloud Drive service URL"))?;
        segments.pop_if_empty();
        segments.push("v1").push("enumerate").push(kind).push(folder_id);
    }
    let dsid = session.dsid().ok_or_else(|| AppError::other("Missing iCloud account DSID"))?;
    url.query_pairs_mut()
        .append_pair("limit", DRIVE_PAGE_LIMIT)
        .append_pair("unified_format", "true")
        .append_pair("clientBuildNumber", DOCWS_CLIENT_BUILD)
        .append_pair("clientMasteringNumber", DOCWS_CLIENT_MASTERING)
        .append_pair("clientId", session.client_id())
        .append_pair("dsid", dsid);
    Ok(url)
}

fn drive_item_from_value(value: &Value) -> Option<DriveItem> {
    let drivewsid = text(value, "drivewsid").or_else(|| text(value, "docwsid"))?;
    let id = text(value, "item_id").unwrap_or_else(|| drivewsid.clone());
    let kind = match text(value, "type").unwrap_or_default().to_ascii_uppercase().as_str() {
        "FOLDER" => "folder",
        "APP_LIBRARY" => "app-library",
        "FILE" => "file",
        _ => return None,
    };
    let name = text(value, "name").unwrap_or_else(|| id.clone());
    Some(DriveItem {
        id,
        drivewsid,
        parent_id: text(value, "parentId"),
        name,
        kind: kind.to_string(),
        zone: text(value, "zone"),
        extension: text(value, "extension"),
        size_bytes: integer(value, "size"),
        date_modified: text(value, "dateModified"),
        download_url: value.get("urls").and_then(|urls| text(urls, "url_download")),
        child_count: integer(value, "directChildrenCount").or_else(|| integer(value, "fileCount")),
        favorite: value.get("favorite_order").is_some(),
    })
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).map(str::to_string)
}

fn integer(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|value| value.as_u64().or_else(|| value.as_str().and_then(|value| value.parse().ok())))
}

pub async fn download(
    manager: &SessionManager,
    session_id: &str,
    url: &str,
    name: Option<&str>,
    target_path: &Path,
) -> AppResult<DownloadedDriveFile> {
    if session_id.trim().is_empty() {
        return Err(AppError::auth("Missing session ID. Please sign in again."));
    }
    let parsed = media::validate_media_url(url)?;
    let handle = manager.get(session_id).await?;
    let mut session = handle.lock().await;
    devices::account_login(&mut session, false).await?;
    let docws_url =
        session.docws_url().ok_or_else(|| AppError::other("iCloud Drive is not available for this account"))?;
    let parsed = drive_download_url(docws_url, parsed.as_str())?;
    let response = session.request_bytes(Method::GET, parsed.as_str(), None, &[]).await?;
    if !response.is_success() {
        return Err(AppError::other(format!("iCloud Drive download returned HTTP {}", response.status)));
    }
    let content_type =
        response.header("content-type").map(str::to_string).unwrap_or_else(|| "application/octet-stream".to_string());
    let destination = media::target_with_content_extension(target_path, &content_type);
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(&destination, &response.bytes)?;
    let file_name = destination
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .or_else(|| name.map(str::to_string))
        .unwrap_or_else(|| "iCloud Drive file".to_string());
    Ok(DownloadedDriveFile { path: destination, file_name, size_bytes: response.bytes.len() as u64 })
}

fn drive_download_url(docws_url: &str, url: &str) -> AppResult<Url> {
    let mut download = media::validate_media_url(url)?;
    let base = Url::parse(docws_url).map_err(|error| AppError::other(format!("Invalid iCloud Drive URL: {error}")))?;
    let download_host = download.host_str().unwrap_or("").to_ascii_lowercase();
    let docws_host = base.host_str().unwrap_or("").to_ascii_lowercase();

    // Apple can return an `icloud.com` download URL for a mainland account whose
    // authenticated session and cookies belong to `icloud.com.cn`. Use the
    // regional docws origin that accountLogin selected so the download works.
    if download_host != docws_host && download_host.contains("-docws.") && docws_host.contains("-docws.") {
        download.set_scheme(base.scheme()).map_err(|_| AppError::other("Invalid iCloud Drive download scheme"))?;
        download.set_host(Some(&docws_host)).map_err(|_| AppError::other("Invalid iCloud Drive download host"))?;
        download.set_port(base.port()).map_err(|_| AppError::other("Invalid iCloud Drive download port"))?;
    }

    Ok(download)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_folder_and_file_items() {
        let folder = drive_item_from_value(&json!({
            "drivewsid": "FOLDER::com.apple.CloudDocs::ABC",
            "item_id": "item-folder",
            "parentId": "FOLDER::com.apple.CloudDocs::root",
            "type": "FOLDER",
            "name": "Downloads",
            "zone": "com.apple.CloudDocs",
            "dateModified": "2026-08-08T01:16:02Z",
            "directChildrenCount": "3",
            "favorite_order": "1"
        }))
        .expect("folder");
        assert!(folder.is_folder());
        assert_eq!(folder.name, "Downloads");
        assert_eq!(folder.child_count, Some(3));
        assert!(folder.favorite);

        let file = drive_item_from_value(&json!({
            "drivewsid": "FILE::com.apple.CloudDocs::DEF",
            "item_id": "item-file",
            "type": "FILE",
            "name": "archive",
            "extension": "rar",
            "size": "305877",
            "urls": { "url_download": "https://p232-docws.icloud.com/v1/download/item-file" }
        }))
        .expect("file");
        assert!(!file.is_folder());
        assert_eq!(file.size_bytes, Some(305877));
        assert_eq!(file.extension.as_deref(), Some("rar"));
        assert!(file.download_url.is_some());
    }

    #[test]
    fn skips_unknown_item_types() {
        assert!(drive_item_from_value(&json!({ "drivewsid": "X", "type": "UNKNOWN", "name": "x" })).is_none());
    }

    #[test]
    fn regionalizes_drive_download_host() {
        let url = drive_download_url(
            "https://p232-docws.icloud.com.cn:443",
            "https://p232-docws.icloud.com/v1/download/item-id?foo=bar",
        )
        .expect("download URL");
        assert_eq!(url.as_str(), "https://p232-docws.icloud.com.cn/v1/download/item-id?foo=bar");
    }

    #[test]
    fn keeps_matching_docws_origin() {
        let url =
            drive_download_url("https://p232-docws.icloud.com", "https://p232-docws.icloud.com/v1/download/item-id")
                .expect("download URL");
        assert_eq!(url.as_str(), "https://p232-docws.icloud.com/v1/download/item-id");
    }

    #[test]
    fn does_not_rewrite_non_docws_urls() {
        let url = drive_download_url("https://p232-docws.icloud.com.cn", "https://cvws.icloud-content.com.cn/asset")
            .expect("download URL");
        assert_eq!(url.as_str(), "https://cvws.icloud-content.com.cn/asset");
    }
}
