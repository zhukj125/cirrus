use std::path::PathBuf;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use cirrus_core::models::{MonthExactDto, PhotosListRequest};
use cirrus_core::services::{avatar, calendar, contacts, devices, media, notes, photos};
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
            Ok(to_value(notes::detail(&state.sessions, &resolved.session_id, note_id).await)?)
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
