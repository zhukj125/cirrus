use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};

use cirrus_core::mcp_access::{load_account_access, load_mcp_settings, load_tool_access};
use cirrus_core::models::{CalendarEventInput, MonthExactDto, NoteImage, PhotosListRequest};
use cirrus_core::services::{avatar, calendar, contacts, devices, drive, media, notes, photos};
use cirrus_core::{
    database_path, now_millis, open_database, AccountStoreRepository, DownloadTaskRepository, DownloadTransferRegistry,
    McpCallHistoryEntry, McpHistoryRepository, SessionHandle, SessionManager, SessionVault, StoredAccount,
};

use crate::LIST_ACCOUNTS_TOOL;

struct ResolvedAccount {
    account: StoredAccount,
    session_id: String,
    handle: Arc<SessionHandle>,
}

pub(crate) struct LocalBackend {
    data_dir: PathBuf,
    sessions: SessionManager,
    download_tasks: DownloadTaskRepository,
    download_transfers: DownloadTransferRegistry,
    download_limiter: media::DownloadRateLimiter,
    history: McpHistoryRepository,
}

impl LocalBackend {
    pub(crate) fn open(data_dir: PathBuf) -> Result<Self, String> {
        let database = database_path(&data_dir);
        open_database(&database).map_err(|error| format!("Could not open the Cirrus database: {error}"))?;
        let vault = SessionVault::open_existing(database)
            .map_err(|error| format!("Could not unlock the Cirrus session vault: {error}"))?;
        let download_tasks = DownloadTaskRepository::new(Some(&data_dir));
        let download_limiter = media::DownloadRateLimiter::new(
            std::env::var("CIRRUS_MCP_DOWNLOAD_SPEED_LIMIT")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0),
        );
        let history = McpHistoryRepository::new(Some(&data_dir));
        Ok(Self {
            data_dir,
            sessions: SessionManager::new(None, Some(Arc::new(vault))),
            download_tasks,
            download_transfers: DownloadTransferRegistry::default(),
            download_limiter,
            history,
        })
    }

    pub(crate) fn visible_accounts(&self) -> Result<Value, String> {
        self.ensure_mcp_enabled()?;
        self.ensure_tool_enabled(LIST_ACCOUNTS_TOOL)?;
        let snapshot = AccountStoreRepository::new(Some(&self.data_dir)).load().map_err(|error| error.to_string())?;
        let account_access = load_account_access(&self.data_dir);
        let mut accounts = snapshot
            .accounts
            .into_iter()
            .filter(|account| !account_access.deny_all && !account_access.disabled_account_keys.contains(&account.key))
            .map(|account| {
                json!({
                    "accountKey": account.key,
                    "appleId": account.apple_id,
                    "firstName": account.first_name,
                    "lastName": account.last_name,
                    "signedIn": account.photos_session_id.is_some() || account.devices_session_id.is_some(),
                    "active": snapshot.active_account_key.as_deref() == Some(account.key.as_str()),
                    "lastUsedAt": account.last_used_at
                })
            })
            .collect::<Vec<_>>();
        accounts
            .sort_by_key(|account| std::cmp::Reverse(account.get("lastUsedAt").and_then(Value::as_i64).unwrap_or(0)));
        let count = accounts.len();
        Ok(json!({ "accounts": accounts, "count": count }))
    }

    pub(crate) async fn call_tool(&self, tool: &str, arguments: &Value) -> Result<Value, String> {
        self.ensure_mcp_enabled()?;
        self.ensure_tool_enabled(tool)?;
        if tool == LIST_ACCOUNTS_TOOL {
            return self.visible_accounts();
        }
        let account_key = required_str(arguments, "accountKey")?;
        let resolved = self.resolve_account(account_key).await?;

        match tool {
            "cirrus_account_status" => account_status(&resolved).await,
            "cirrus_logout" => self.logout(resolved).await,
            "cirrus_list_devices" => Ok(to_value(devices::list_devices(&resolved.handle).await)?),
            "cirrus_play_sound" => {
                let device_id = required_str(arguments, "deviceId")?;
                Ok(to_value(devices::play_sound(&resolved.handle, device_id).await)?)
            }
            "cirrus_list_photos" => {
                let cursor = optional_str(arguments, "cursor");
                let limit = optional_i64(arguments, "limit").map(|value| value.clamp(1, 200) as i32);
                let start_rank = optional_i64(arguments, "startRank");
                let request = PhotosListRequest {
                    session_id: resolved.session_id.clone(),
                    cursor: cursor.map(ToOwned::to_owned),
                    limit,
                    start_rank,
                };
                Ok(to_value(
                    photos::list(
                        &self.sessions,
                        &request.session_id,
                        request.cursor,
                        request.limit,
                        request.start_rank,
                    )
                    .await,
                )?)
            }
            "cirrus_delete_photos" => {
                let photo_ids = required_string_array(arguments, "photoIds")?;
                Ok(to_value(photos::delete(&self.sessions, &resolved.session_id, photo_ids).await)?)
            }
            "cirrus_photos_total" => Ok(to_value(
                photos::library_total(&self.sessions, &resolved.session_id).await.map_err(|error| error.to_string())?,
            )?),
            "cirrus_photos_timeline" => Ok(to_value(
                photos::timeline(&self.sessions, &resolved.session_id, photos::DEFAULT_TZ_OFFSET_SECONDS)
                    .await
                    .map_err(|error| error.to_string())?,
            )?),
            "cirrus_photos_month_exact" => {
                let year = required_i64(arguments, "year")? as i32;
                let month = required_i64(arguments, "month")? as u32;
                let exact = photos::month_exact(
                    &self.sessions,
                    &resolved.session_id,
                    year,
                    month,
                    photos::DEFAULT_TZ_OFFSET_SECONDS,
                )
                .await
                .map_err(|error| error.to_string())?;
                Ok(to_value(MonthExactDto::from(&exact))?)
            }
            "cirrus_download_photo" => self.download_photo(arguments, &resolved.account.key).await,
            "cirrus_cancel_download" => {
                let transfer_id = required_str(arguments, "transferId")?;
                let cancelled = self.download_transfers.cancel(transfer_id).await;
                Ok(json!({ "transferId": transfer_id, "cancelled": cancelled }))
            }
            "cirrus_list_download_tasks" => {
                Ok(to_value(self.download_tasks.load(&resolved.account.key).await.map_err(|error| error.to_string())?)?)
            }
            "cirrus_get_account_avatar" => {
                Ok(to_value(avatar::fetch(&resolved.handle).await.map_err(|error| error.to_string())?)?)
            }
            "cirrus_get_contact_photo" => {
                let url = required_str(arguments, "url")?;
                Ok(to_value(
                    avatar::fetch_url(&self.sessions, &resolved.session_id, url)
                        .await
                        .map_err(|error| error.to_string())?,
                )?)
            }
            "cirrus_list_notes" => {
                let cursor = optional_str(arguments, "cursor").map(ToOwned::to_owned);
                Ok(to_value(notes::list(&self.sessions, &resolved.session_id, cursor).await)?)
            }
            "cirrus_get_note" => {
                let note_id = required_str(arguments, "noteId")?;
                let include_images = arguments.get("includeImages").and_then(Value::as_bool).unwrap_or(true);
                let original = arguments.get("original").and_then(Value::as_bool).unwrap_or(false);
                let detail = notes::detail(&self.sessions, &resolved.session_id, note_id).await;
                let mut result = to_value(detail)?;
                if include_images && result.get("status").and_then(Value::as_str) == Some("OK") {
                    if let Some(note) = result.get("note").cloned() {
                        let attachments =
                            note.get("attachments").and_then(Value::as_array).cloned().unwrap_or_default();
                        let mut images = HashMap::new();
                        for attachment in &attachments {
                            let uti = attachment.get("uti").and_then(Value::as_str).unwrap_or_default();
                            if !note_attachment_is_image(uti) {
                                continue;
                            }
                            let Some(attachment_id) = attachment.get("id").and_then(Value::as_str) else {
                                continue;
                            };
                            if let Ok(image) = notes::attachment_image(
                                &self.sessions,
                                &resolved.session_id,
                                note_id,
                                attachment_id,
                                original,
                            )
                            .await
                            {
                                images.insert(attachment_id.to_string(), image);
                            }
                        }
                        let segments = note_content_segments(&note, &images);
                        if let Some(object) = result.as_object_mut() {
                            object.insert("contentSegments".to_string(), Value::Array(segments));
                        }
                    }
                }
                Ok(result)
            }
            "cirrus_get_note_image" => {
                let note_id = required_str(arguments, "noteId")?;
                let attachment_id = required_str(arguments, "attachmentId")?;
                let original = arguments.get("original").and_then(Value::as_bool).unwrap_or(false);
                Ok(to_value(
                    notes::attachment_image(&self.sessions, &resolved.session_id, note_id, attachment_id, original)
                        .await
                        .map_err(|error| error.to_string())?,
                )?)
            }
            "cirrus_create_note" => {
                let title = required_str(arguments, "title")?;
                let body = optional_str(arguments, "body").unwrap_or("");
                let folder_id = optional_str(arguments, "folderId").map(ToOwned::to_owned);
                Ok(to_value(notes::create(&self.sessions, &resolved.session_id, folder_id, title, body).await)?)
            }
            "cirrus_update_note" => {
                let note_id = required_str(arguments, "noteId")?;
                let title = required_str(arguments, "title")?;
                let body = required_str(arguments, "body")?;
                Ok(to_value(notes::update(&self.sessions, &resolved.session_id, note_id, title, body).await)?)
            }
            "cirrus_delete_note" => {
                let note_id = required_str(arguments, "noteId")?;
                Ok(to_value(notes::delete(&self.sessions, &resolved.session_id, note_id).await)?)
            }
            "cirrus_list_drive_items" => {
                let folder_id = optional_str(arguments, "folderId").map(ToOwned::to_owned);
                Ok(to_value(drive::list(&self.sessions, &resolved.session_id, folder_id).await)?)
            }
            "cirrus_download_drive_file" => {
                let url = required_str(arguments, "url")?;
                let target_path = required_str(arguments, "targetPath")?;
                let target = PathBuf::from(target_path);
                if !target.is_absolute() {
                    return Err("targetPath must be an absolute file path".to_string());
                }
                let downloaded = drive::download(
                    &self.sessions,
                    &resolved.session_id,
                    url,
                    optional_str(arguments, "name"),
                    &target,
                )
                .await
                .map_err(|error| error.to_string())?;
                Ok(json!({
                    "path": downloaded.path.to_string_lossy(),
                    "fileName": downloaded.file_name,
                    "sizeBytes": downloaded.size_bytes
                }))
            }
            "cirrus_list_contacts" => {
                let locale = optional_str(arguments, "locale").unwrap_or("zh-CN");
                Ok(to_value(contacts::list(&self.sessions, &resolved.session_id, locale).await)?)
            }
            "cirrus_load_calendar" => {
                let start_date = required_str(arguments, "startDate")?;
                let end_date = required_str(arguments, "endDate")?;
                let timezone = optional_str(arguments, "timezone").unwrap_or("Asia/Shanghai");
                let locale = optional_str(arguments, "locale").unwrap_or("zh-CN");
                Ok(to_value(
                    calendar::load(&self.sessions, &resolved.session_id, start_date, end_date, timezone, locale).await,
                )?)
            }
            "cirrus_save_calendar_event" => {
                let input = CalendarEventInput {
                    event_id: optional_str(arguments, "eventId").map(ToOwned::to_owned),
                    calendar_id: required_str(arguments, "calendarId")?.to_string(),
                    title: required_str(arguments, "title")?.to_string(),
                    start: required_str(arguments, "start")?.to_string(),
                    end: required_str(arguments, "end")?.to_string(),
                    all_day: arguments.get("allDay").and_then(Value::as_bool).unwrap_or(false),
                    location: optional_str(arguments, "location").unwrap_or("").to_string(),
                    notes: optional_str(arguments, "notes").unwrap_or("").to_string(),
                    url: optional_str(arguments, "url").unwrap_or("").to_string(),
                    timezone: optional_str(arguments, "timezone").map(ToOwned::to_owned),
                    timezone_offset: optional_str(arguments, "timezoneOffset").map(ToOwned::to_owned),
                    locale: optional_str(arguments, "locale").map(ToOwned::to_owned),
                };
                Ok(to_value(calendar::save_event(&self.sessions, &resolved.session_id, input).await)?)
            }
            "cirrus_delete_calendar_event" => {
                let event_id = required_str(arguments, "eventId")?;
                let calendar_id = required_str(arguments, "calendarId")?;
                let start = required_str(arguments, "start")?;
                let end = required_str(arguments, "end")?;
                Ok(to_value(
                    calendar::delete_event(
                        &self.sessions,
                        &resolved.session_id,
                        event_id,
                        calendar_id,
                        start,
                        end,
                        optional_str(arguments, "timezone"),
                        optional_str(arguments, "locale"),
                    )
                    .await,
                )?)
            }
            _ => Err(format!("Unknown Cirrus MCP tool: {tool}")),
        }
    }

    pub(crate) fn record_history(&self, entry: McpCallHistoryEntry) -> Result<(), String> {
        self.history.record(entry).map_err(|error| error.to_string())
    }

    fn ensure_mcp_enabled(&self) -> Result<(), String> {
        if load_mcp_settings(&self.data_dir).enabled {
            Ok(())
        } else {
            Err("MCP is disabled in Cirrus Settings.".to_string())
        }
    }

    fn ensure_tool_enabled(&self, tool: &str) -> Result<(), String> {
        let access = load_tool_access(&self.data_dir);
        if access.deny_all || access.disabled_tools.contains(tool) {
            Err(format!("MCP tool is disabled in Cirrus: {tool}"))
        } else {
            Ok(())
        }
    }

    fn account_enabled(&self, account_key: &str) -> bool {
        let access = load_account_access(&self.data_dir);
        !access.deny_all && !access.disabled_account_keys.contains(account_key)
    }

    async fn resolve_account(&self, account_key: &str) -> Result<ResolvedAccount, String> {
        if !self.account_enabled(account_key) {
            return Err("This iCloud account is disabled for MCP operations".to_string());
        }
        let snapshot = AccountStoreRepository::new(Some(&self.data_dir)).load().map_err(|error| error.to_string())?;
        let account =
            snapshot.accounts.into_iter().find(|account| account.key == account_key).ok_or_else(|| {
                "Account was not found in Cirrus. Refresh the account list and try again.".to_string()
            })?;
        let session_id = account
            .photos_session_id
            .clone()
            .or_else(|| account.devices_session_id.clone())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                "This account is not signed in. Sign in through the Cirrus desktop app first.".to_string()
            })?;
        let handle = self.sessions.get(&session_id).await.map_err(|error| {
            if !self.sessions.vault_enabled() {
                "The Cirrus session vault is unavailable. Open Cirrus and ensure the OS credential store is accessible."
                    .to_string()
            } else {
                error.to_string()
            }
        })?;
        let verified = {
            let session = handle.lock().await;
            session.is_flag_set(cirrus_core::models::session_keys::PHOTOS_VERIFIED)
        };
        if !verified {
            return Err("This account session is not verified. Sign in to Cirrus again.".to_string());
        }
        Ok(ResolvedAccount { account, session_id, handle })
    }

    async fn logout(&self, resolved: ResolvedAccount) -> Result<Value, String> {
        AccountStoreRepository::new(Some(&self.data_dir))
            .clear_sessions(&resolved.account.key)
            .map_err(|error| error.to_string())?;
        self.sessions.remove(&resolved.session_id).await;

        Ok(json!({ "accountKey": resolved.account.key, "signedOut": true }))
    }

    async fn download_photo(&self, arguments: &Value, account_key: &str) -> Result<Value, String> {
        let url = required_str(arguments, "url")?;
        let name = optional_str(arguments, "name");
        let target_path = required_str(arguments, "targetPath")?;
        let target = PathBuf::from(target_path);
        if !target.is_absolute() {
            return Err("targetPath must be an absolute file path".to_string());
        }
        let transfer_id = optional_str(arguments, "transferId")
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let cancelled = self
            .download_transfers
            .register(&transfer_id)
            .await
            .ok_or_else(|| "This transfer ID is already active".to_string())?;
        let result = media::fetch_media_to_file_with_progress(
            url,
            name,
            &target,
            optional_str(arguments, "partialKey"),
            Some(&self.download_limiter),
            Some(cancelled.as_ref()),
            |_, _| {},
        )
        .await;
        self.download_transfers.finish(&transfer_id).await;
        let downloaded = result.map_err(|error| error.to_string())?;
        let saved = persist_downloaded_file(&downloaded, &target)?;
        let file_name = saved
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| downloaded.file_name.clone());
        let photo_id = optional_str(arguments, "photoId").unwrap_or(&transfer_id).to_string();
        let directory = saved.parent().map(|value| value.to_string_lossy().to_string()).unwrap_or_default();
        let created_at = now_millis();
        let task = json!({
            "accountKey": account_key,
            "id": format!("mcp-{transfer_id}"),
            "title": format!("MCP · {file_name}"),
            "directory": directory,
            "status": "done",
            "source": {
                "kind": "snapshot",
                "items": [{
                    "id": photo_id,
                    "fileName": file_name,
                    "thumbUrl": url,
                    "originalUrl": url,
                    "sizeBytes": downloaded.size
                }],
                "pending": []
            },
            "nextIndex": 1,
            "done": 1,
            "failed": [],
            "failedItems": [],
            "completedFiles": [{ "name": file_name, "sizeBytes": downloaded.size }],
            "pausedItems": [],
            "pausedProgress": [],
            "inFlightItems": [],
            "inFlightProgress": [],
            "usedNames": [file_name],
            "concurrency": 1,
            "createdAt": created_at,
            "error": null
        });
        let persisted = self.download_tasks.append(account_key, task).await;
        if let Err(error) = &persisted {
            log::warn!("[MCP] download succeeded but task history could not be saved: {error}");
        }
        let result = json!({
            "transferId": transfer_id,
            "accountKey": account_key,
            "path": saved.to_string_lossy(),
            "sizeBytes": downloaded.size,
            "fileName": file_name,
            "persisted": persisted.is_ok()
        });

        Ok(result)
    }
}

fn persist_downloaded_file(downloaded: &media::DownloadedFile, target: &Path) -> Result<PathBuf, String> {
    let destination = media::target_with_content_extension(target, &downloaded.content_type);
    media::persist_downloaded_file(&downloaded.path, &destination).map_err(|error| error.to_string())
}

async fn account_status(resolved: &ResolvedAccount) -> Result<Value, String> {
    let session = resolved.handle.lock().await;
    Ok(json!({
        "accountKey": resolved.account.key,
        "appleId": resolved.account.apple_id,
        "dsid": resolved.account.dsid,
        "firstName": session.first_name(),
        "lastName": session.last_name(),
        "signedIn": true,
        "verified": session.is_flag_set(cirrus_core::models::session_keys::PHOTOS_VERIFIED),
        "lastUsedAt": resolved.account.last_used_at
    }))
}

fn note_attachment_is_image(uti: &str) -> bool {
    let uti = uti.to_ascii_lowercase();
    uti == "com.apple.paper"
        || uti.starts_with("public.image")
        || uti.ends_with(".png")
        || uti.ends_with(".jpeg")
        || uti.ends_with(".jpg")
        || uti.ends_with(".heic")
        || uti.ends_with(".gif")
        || uti.ends_with(".webp")
        || uti.ends_with(".tiff")
}

fn attachment_ids_in_html(body_html: &str) -> Vec<String> {
    const NEEDLE: &str = "data-note-attachment-id=\"";
    let mut ids = Vec::new();
    let mut rest = body_html;
    while let Some(start) = rest.find(NEEDLE) {
        rest = &rest[start + NEEDLE.len()..];
        let Some(end) = rest.find('"') else { break };
        let id = rest[..end].trim();
        if !id.is_empty() {
            ids.push(id.to_string());
        }
        rest = &rest[end + 1..];
    }
    ids
}

fn note_content_segments(note: &Value, images: &HashMap<String, NoteImage>) -> Vec<Value> {
    let body = note.get("body").and_then(Value::as_str).unwrap_or_default();
    let body_html = note.get("bodyHtml").and_then(Value::as_str).unwrap_or_default();
    let attachments = note.get("attachments").and_then(Value::as_array).cloned().unwrap_or_default();
    let ordered_ids = attachment_ids_in_html(body_html);
    let fallback_ids = attachments
        .iter()
        .filter(|attachment| {
            note_attachment_is_image(attachment.get("uti").and_then(Value::as_str).unwrap_or_default())
        })
        .filter_map(|attachment| attachment.get("id").and_then(Value::as_str).map(str::to_string))
        .collect::<Vec<_>>();
    let mut segments = Vec::new();
    let mut cursor = 0;
    let mut marker_index = 0;
    for (offset, character) in body.char_indices() {
        if character != '\u{fffc}' {
            continue;
        }
        push_note_text_segment(&mut segments, &body[cursor..offset]);
        let attachment_id =
            ordered_ids.get(marker_index).or_else(|| fallback_ids.get(marker_index)).map(String::as_str);
        if let Some(attachment_id) = attachment_id {
            let attachment = attachments
                .iter()
                .find(|attachment| attachment.get("id").and_then(Value::as_str) == Some(attachment_id));
            if let Some(attachment) = attachment {
                let name = attachment.get("name").and_then(Value::as_str).unwrap_or("Attachment");
                if note_attachment_is_image(attachment.get("uti").and_then(Value::as_str).unwrap_or_default()) {
                    if let Some(image) = images.get(attachment_id) {
                        segments.push(json!({
                            "type": "image",
                            "attachmentId": image.attachment_id,
                            "name": image.name,
                            "mimeType": image.mime_type,
                            "byteLength": image.byte_length,
                            "source": image.source,
                            "dataUrl": image.data_url
                        }));
                    } else {
                        push_note_text_segment(&mut segments, &format!("[Image unavailable: {name}]"));
                    }
                } else {
                    push_note_text_segment(&mut segments, &format!("[Attachment: {name}]"));
                }
            }
        }
        cursor = offset + character.len_utf8();
        marker_index += 1;
    }
    push_note_text_segment(&mut segments, &body[cursor..]);
    segments
}

fn push_note_text_segment(segments: &mut Vec<Value>, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    segments.push(json!({ "type": "text", "text": text.trim_end() }));
}

fn required_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Missing required argument: {key}"))
}

fn required_string_array(arguments: &Value, key: &str) -> Result<Vec<String>, String> {
    let values = arguments
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Missing required array argument: {key}"))?;
    let mut result = Vec::new();
    for value in values {
        let Some(value) = value.as_str().map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        if !result.iter().any(|existing| existing == value) {
            result.push(value.to_string());
        }
    }
    if result.is_empty() {
        return Err(format!("{key} must contain at least one ID"));
    }
    Ok(result)
}

fn optional_str<'a>(arguments: &'a Value, key: &str) -> Option<&'a str> {
    arguments.get(key).and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty())
}

fn optional_i64(arguments: &Value, key: &str) -> Option<i64> {
    arguments.get(key).and_then(Value::as_i64)
}

fn required_i64(arguments: &Value, key: &str) -> Result<i64, String> {
    optional_i64(arguments, key).ok_or_else(|| format!("Missing required numeric argument: {key}"))
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cirrus_core::mcp_access::{save_account_access, save_tool_access};

    fn ensure_test_vault_key() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            std::env::set_var("CIRRUS_SESSION_KEY", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        });
    }

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cirrus-local-mcp-{name}-{}", uuid::Uuid::new_v4()))
    }

    fn account(key: &str, signed_in: bool) -> StoredAccount {
        StoredAccount {
            key: key.to_string(),
            apple_id: "person@example.com".to_string(),
            dsid: Some("123".to_string()),
            first_name: Some("三".to_string()),
            last_name: Some("张".to_string()),
            photos_session_id: signed_in.then(|| "session-a".to_string()),
            devices_session_id: signed_in.then(|| "session-a".to_string()),
            last_used_at: 10,
            last_known_status: "valid".to_string(),
        }
    }

    #[test]
    fn lists_accounts_from_the_local_vault() {
        ensure_test_vault_key();
        let dir = temp_dir("accounts");
        AccountStoreRepository::new(Some(&dir))
            .save(&cirrus_core::AccountStoreSnapshot {
                accounts: vec![account("dsid:123", true), account("dsid:456", false)],
                active_account_key: Some("dsid:123".to_string()),
                avatars: Default::default(),
                revision: 0,
            })
            .expect("seed accounts");
        let backend = LocalBackend::open(dir.clone()).expect("open backend");
        let result = backend.visible_accounts().expect("visible accounts");
        assert_eq!(result["count"], 2);
        assert_eq!(result["accounts"][0]["accountKey"], "dsid:123");
        assert_eq!(result["accounts"][0]["signedIn"], true);
        assert_eq!(result["accounts"][0]["active"], true);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn download_finalization_moves_the_part_file_to_the_typed_target() {
        let dir = temp_dir("download");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let part = dir.join(".photo.cirrus-part");
        std::fs::write(&part, b"image-bytes").expect("write part");
        let target = dir.join("photo.download");
        let downloaded = media::DownloadedFile {
            path: part.clone(),
            size: 11,
            file_name: "photo".to_string(),
            content_type: "image/png".to_string(),
        };

        let saved = persist_downloaded_file(&downloaded, &target).expect("persist download");
        assert_eq!(saved.extension().and_then(|value| value.to_str()), Some("png"));
        assert_eq!(std::fs::read(&saved).expect("read saved"), b"image-bytes");
        assert!(!part.exists(), "temporary part file must be moved");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn enforces_tool_and_account_access_from_the_shared_database() {
        ensure_test_vault_key();
        let dir = temp_dir("access");
        AccountStoreRepository::new(Some(&dir))
            .save(&cirrus_core::AccountStoreSnapshot {
                accounts: vec![account("dsid:123", true)],
                active_account_key: Some("dsid:123".to_string()),
                avatars: Default::default(),
                revision: 0,
            })
            .expect("seed accounts");
        let backend = LocalBackend::open(dir.clone()).expect("open backend");

        let mut account_access = load_account_access(&dir);
        account_access.disabled_account_keys.insert("dsid:123".to_string());
        save_account_access(&dir, &account_access).expect("disable account");
        let result = backend.visible_accounts().expect("visible accounts");
        assert_eq!(result["count"], 0);

        let mut tool_access = load_tool_access(&dir);
        tool_access.disabled_tools.insert(LIST_ACCOUNTS_TOOL.to_string());
        save_tool_access(&dir, &tool_access).expect("disable tool");
        let error = backend.call_tool(LIST_ACCOUNTS_TOOL, &json!({})).await.expect_err("disabled tool");
        assert!(error.contains("disabled"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
