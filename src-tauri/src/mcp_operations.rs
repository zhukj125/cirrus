use std::path::PathBuf;

use std::collections::HashMap;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use cirrus_core::models::{MonthExactDto, NoteImage, PhotosListRequest};
use cirrus_core::services::{avatar, calendar, contacts, devices, drive, media, notes, photos};
use cirrus_core::SessionHandle;

use crate::account_store::{AccountStoreRepository, StoredAccount};
use crate::mcp::McpService;
use crate::state::AppState;

struct ResolvedAccount {
    account: StoredAccount,
    session_id: String,
    handle: std::sync::Arc<SessionHandle>,
}

pub async fn call_tool(
    tool: &str,
    arguments: &Value,
    state: &AppState,
    mcp: &McpService,
    app: &AppHandle,
) -> Result<Value, String> {
    if tool == "cirrus_list_accounts" {
        return serde_json::to_value(mcp.visible_accounts()?).map_err(|error| error.to_string());
    }

    let account_key = required_str(arguments, "accountKey")?;
    let resolved = resolve_account(account_key, state, mcp).await?;

    match tool {
        "cirrus_account_status" => account_status(&resolved).await,
        "cirrus_logout" => logout(resolved, state, app).await,
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
                photos::list(&state.sessions, &request.session_id, request.cursor, request.limit, request.start_rank)
                    .await,
            )?)
        }
        "cirrus_photos_total" => Ok(to_value(
            photos::library_total(&state.sessions, &resolved.session_id).await.map_err(|error| error.to_string())?,
        )?),
        "cirrus_photos_timeline" => Ok(to_value(
            photos::timeline(&state.sessions, &resolved.session_id, photos::DEFAULT_TZ_OFFSET_SECONDS)
                .await
                .map_err(|error| error.to_string())?,
        )?),
        "cirrus_photos_month_exact" => {
            let year = required_i64(arguments, "year")? as i32;
            let month = required_i64(arguments, "month")? as u32;
            let exact = photos::month_exact(
                &state.sessions,
                &resolved.session_id,
                year,
                month,
                photos::DEFAULT_TZ_OFFSET_SECONDS,
            )
            .await
            .map_err(|error| error.to_string())?;
            Ok(to_value(MonthExactDto::from(&exact))?)
        }
        "cirrus_download_photo" => download_photo(arguments, state, app, &resolved.account.key).await,
        "cirrus_cancel_download" => {
            let transfer_id = required_str(arguments, "transferId")?;
            let cancelled = state.download_transfers.cancel(transfer_id).await;
            Ok(json!({ "transferId": transfer_id, "cancelled": cancelled }))
        }
        "cirrus_list_download_tasks" => {
            Ok(to_value(state.download_tasks.load(&resolved.account.key).await.map_err(|error| error.to_string())?)?)
        }
        "cirrus_get_account_avatar" => {
            Ok(to_value(avatar::fetch(&resolved.handle).await.map_err(|error| error.to_string())?)?)
        }
        "cirrus_get_contact_photo" => {
            let url = required_str(arguments, "url")?;
            Ok(to_value(
                avatar::fetch_url(&state.sessions, &resolved.session_id, url)
                    .await
                    .map_err(|error| error.to_string())?,
            )?)
        }
        "cirrus_list_notes" => {
            let cursor = optional_str(arguments, "cursor").map(ToOwned::to_owned);
            Ok(to_value(notes::list(&state.sessions, &resolved.session_id, cursor).await)?)
        }
        "cirrus_get_note" => {
            let note_id = required_str(arguments, "noteId")?;
            let include_images = arguments.get("includeImages").and_then(Value::as_bool).unwrap_or(true);
            let original = arguments.get("original").and_then(Value::as_bool).unwrap_or(false);
            let detail = notes::detail(&state.sessions, &resolved.session_id, note_id).await;
            let mut result = to_value(detail)?;
            if include_images && result.get("status").and_then(Value::as_str) == Some("OK") {
                if let Some(note) = result.get("note").cloned() {
                    let attachments = note.get("attachments").and_then(Value::as_array).cloned().unwrap_or_default();
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
                            &state.sessions,
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
                notes::attachment_image(&state.sessions, &resolved.session_id, note_id, attachment_id, original)
                    .await
                    .map_err(|error| error.to_string())?,
            )?)
        }
        "cirrus_create_note" => {
            let title = required_str(arguments, "title")?;
            let body = optional_str(arguments, "body").unwrap_or("");
            let folder_id = optional_str(arguments, "folderId").map(ToOwned::to_owned);
            Ok(to_value(notes::create(&state.sessions, &resolved.session_id, folder_id, title, body).await)?)
        }
        "cirrus_update_note" => {
            let note_id = required_str(arguments, "noteId")?;
            let title = required_str(arguments, "title")?;
            let body = required_str(arguments, "body")?;
            Ok(to_value(notes::update(&state.sessions, &resolved.session_id, note_id, title, body).await)?)
        }
        "cirrus_delete_note" => {
            let note_id = required_str(arguments, "noteId")?;
            Ok(to_value(notes::delete(&state.sessions, &resolved.session_id, note_id).await)?)
        }
        "cirrus_list_drive_items" => {
            let folder_id = optional_str(arguments, "folderId").map(ToOwned::to_owned);
            Ok(to_value(drive::list(&state.sessions, &resolved.session_id, folder_id).await)?)
        }
        "cirrus_download_drive_file" => {
            let url = required_str(arguments, "url")?;
            let target_path = required_str(arguments, "targetPath")?;
            let target = PathBuf::from(target_path);
            if !target.is_absolute() {
                return Err("targetPath must be an absolute file path".to_string());
            }
            let downloaded =
                drive::download(&state.sessions, &resolved.session_id, url, optional_str(arguments, "name"), &target)
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
            Ok(to_value(contacts::list(&state.sessions, &resolved.session_id, locale).await)?)
        }
        "cirrus_load_calendar" => {
            let start_date = required_str(arguments, "startDate")?;
            let end_date = required_str(arguments, "endDate")?;
            let timezone = optional_str(arguments, "timezone").unwrap_or("Asia/Shanghai");
            let locale = optional_str(arguments, "locale").unwrap_or("zh-CN");
            Ok(to_value(
                calendar::load(&state.sessions, &resolved.session_id, start_date, end_date, timezone, locale).await,
            )?)
        }
        _ => Err(format!("Unknown Cirrus MCP tool: {tool}")),
    }
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
        let Some(end) = rest.find('"') else {
            break;
        };
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
async fn resolve_account(account_key: &str, state: &AppState, mcp: &McpService) -> Result<ResolvedAccount, String> {
    mcp.ensure_account_enabled(account_key)?;
    let snapshot = AccountStoreRepository::new(state.data_dir()).load().map_err(|error| error.to_string())?;
    let account = snapshot
        .accounts
        .into_iter()
        .find(|account| account.key == account_key)
        .ok_or_else(|| "Account was not found in Cirrus. Refresh the account list and try again.".to_string())?;
    let session_id = account
        .photos_session_id
        .clone()
        .or_else(|| account.devices_session_id.clone())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "This account is not signed in. Use cirrus_login first.".to_string())?;
    let handle = state.sessions.get(&session_id).await.map_err(|error| error.to_string())?;
    let verified = {
        let session = handle.lock().await;
        session.is_flag_set(cirrus_core::models::session_keys::PHOTOS_VERIFIED)
    };
    if !verified {
        return Err("This account session is not verified. Sign in to Cirrus again.".to_string());
    }
    Ok(ResolvedAccount { account, session_id, handle })
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

async fn logout(resolved: ResolvedAccount, state: &AppState, app: &AppHandle) -> Result<Value, String> {
    let mut snapshot = AccountStoreRepository::new(state.data_dir()).load().map_err(|error| error.to_string())?;
    for account in &mut snapshot.accounts {
        if account.key == resolved.account.key {
            account.photos_session_id = None;
            account.devices_session_id = None;
            account.last_known_status = "expired".to_string();
        }
    }
    AccountStoreRepository::new(state.data_dir()).save(&snapshot).map_err(|error| error.to_string())?;
    state.sessions.remove(&resolved.session_id).await;
    let _ = app.emit("mcp-accounts-changed", json!({ "accountKey": resolved.account.key, "reason": "logout" }));
    Ok(json!({ "accountKey": resolved.account.key, "signedOut": true }))
}

async fn download_photo(
    arguments: &Value,
    state: &AppState,
    app: &AppHandle,
    account_key: &str,
) -> Result<Value, String> {
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
    let cancelled = state.download_transfers.register(&transfer_id).await;
    let event_transfer_id = transfer_id.clone();
    let event_app = app.clone();
    let event_name = name.unwrap_or("iCloud media").to_string();
    let result = media::fetch_media_to_file_with_progress(
        url,
        name,
        &target,
        optional_str(arguments, "partialKey"),
        Some(&state.download_limiter),
        Some(cancelled.as_ref()),
        move |downloaded_bytes, total_bytes| {
            let _ = event_app.emit(
                "download-progress",
                json!({
                    "transferId": event_transfer_id,
                    "fileName": event_name,
                    "downloadedBytes": downloaded_bytes,
                    "totalBytes": total_bytes
                }),
            );
        },
    )
    .await;
    state.download_transfers.finish(&transfer_id).await;
    let downloaded = result.map_err(|error| error.to_string())?;
    let destination = media::target_with_content_extension(&target, &downloaded.content_type);
    let saved = media::persist_downloaded_file(&downloaded.path, &destination).map_err(|error| error.to_string())?;
    let file_name = saved
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| downloaded.file_name.clone());
    let photo_id = optional_str(arguments, "photoId").unwrap_or(&transfer_id).to_string();
    let directory = saved.parent().map(|value| value.to_string_lossy().to_string()).unwrap_or_default();
    let created_at = cirrus_core::now_millis();
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
    let persisted = state.download_tasks.append(account_key, task).await;
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
    let _ = app.emit("mcp-download-completed", result.clone());
    Ok(result)
}

fn required_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str, String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Missing required argument: {key}"))
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

    #[test]
    fn note_content_segments_preserve_image_positions() {
        let note = json!({
            "body": "before\u{fffc}middle\u{fffc}after",
            "bodyHtml": "<figure data-note-attachment-id=\"image-1\"></figure><figure data-note-attachment-id=\"image-2\"></figure>",
            "attachments": [
                { "id": "image-1", "name": "one.png", "uti": "public.png" },
                { "id": "image-2", "name": "two.png", "uti": "public.png" }
            ]
        });
        let mut images = HashMap::new();
        for id in ["image-1", "image-2"] {
            images.insert(
                id.to_string(),
                NoteImage {
                    attachment_id: id.to_string(),
                    name: format!("{id}.png"),
                    mime_type: "image/png".to_string(),
                    byte_length: 3,
                    data_url: "data:image/png;base64,YWJj".to_string(),
                    source: "preview".to_string(),
                },
            );
        }
        let segments = note_content_segments(&note, &images);
        let kinds =
            segments.iter().filter_map(|segment| segment.get("type").and_then(Value::as_str)).collect::<Vec<_>>();
        assert_eq!(kinds, vec!["text", "image", "text", "image", "text"]);
        assert_eq!(segments[1]["attachmentId"], "image-1");
        assert_eq!(segments[3]["attachmentId"], "image-2");
    }
}
