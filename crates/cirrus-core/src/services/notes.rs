use std::collections::HashMap;
use std::io::{Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use reqwest::Method;
use serde_json::{json, Value};

use crate::apple::http::headers as hh;
use crate::error::{AppError, AppResult};
use crate::models::{
    NoteAttachment, NoteDetail, NoteDetailResult, NoteFolder, NoteImage, NoteMutationResult, NoteSummary,
    NotesListResult,
};
use crate::services::{auth as auth_service, devices, media, note_content, note_paper};
use crate::session::SrpSession;
use crate::SessionManager;

const NOTES_ZONE: &str = "Notes";
const NOTES_QUERY_LIMIT: i64 = 100;
const NOTES_CLIENT_BUILD: &str = "2640Build18";
const NOTES_CLIENT_MASTERING: &str = "M2640Build18";
const NOTES_CKJS_BUILD: &str = "2310ProjectDev27";
const NOTES_CKJS_VERSION: &str = "2.6.4";
const DEFAULT_FOLDER: &str = "DefaultFolder-CloudKit";
const TRASH_FOLDER: &str = "TrashFolder-CloudKit";
const MAX_NOTE_IMAGE_BYTES: usize = 15 * 1024 * 1024;

const DESIRED_KEYS: &[&str] = &[
    "TitleEncrypted",
    "SnippetEncrypted",
    "FirstAttachmentUTIEncrypted",
    "FirstAttachmentThumbnail",
    "FirstAttachmentThumbnailOrientation",
    "CreationDate",
    "ModificationDate",
    "Deleted",
    "Folders",
    "Folder",
    "Attachments",
    "ParentFolder",
    "Note",
    "LastViewedModificationDate",
    "MinimumSupportedNotesVersion",
    "IsPinned",
];

pub async fn list(manager: &SessionManager, session_id: &str, cursor: Option<String>) -> NotesListResult {
    if session_id.trim().is_empty() {
        return NotesListResult::error("Missing session ID. Please sign in again.");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return NotesListResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    if let Err(error) = devices::account_login(&mut session, false).await {
        return NotesListResult::error(error.to_string());
    }
    let base = match notes_base(&session) {
        Some(base) => base,
        None => return NotesListResult::error("Notes is not available for this account"),
    };

    let folders = match query_folders(&mut session, &base).await {
        Ok(folders) => folders,
        Err(error) => {
            log::warn!("[NOTES] folder query failed: {error}");
            Vec::new()
        }
    };
    let (notes, next_cursor) = match query_notes(&mut session, &base, cursor).await {
        Ok(result) => result,
        Err(error) => {
            log::warn!("[NOTES] list query failed: {error}");
            return NotesListResult::error(error.to_string());
        }
    };
    NotesListResult::ok(folders, notes, next_cursor)
}

pub async fn detail(manager: &SessionManager, session_id: &str, note_id: &str) -> NoteDetailResult {
    if session_id.trim().is_empty() {
        return NoteDetailResult::error("Missing session ID. Please sign in again.");
    }
    if note_id.trim().is_empty() {
        return NoteDetailResult::error("Missing note ID");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return NoteDetailResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    if let Err(error) = devices::account_login(&mut session, false).await {
        return NoteDetailResult::error(error.to_string());
    }
    let base = match notes_base(&session) {
        Some(base) => base,
        None => return NoteDetailResult::error("Notes is not available for this account"),
    };
    let url = notes_url(&session, &base, "records/lookup");
    let body = json!({
        "records": [{ "recordName": note_id }],
        "zoneID": { "zoneName": NOTES_ZONE }
    });
    let response = match post_json(&mut session, &url, &body).await {
        Ok(response) => response,
        Err(error) => return NoteDetailResult::error(error.to_string()),
    };
    let Some(record) = response.get("records").and_then(Value::as_array).and_then(|records| records.first()) else {
        return NoteDetailResult::error("Note was not found");
    };
    if record.get("serverErrorCode").is_some() {
        return NoteDetailResult::error(format!(
            "Note could not be loaded: {}",
            record.get("reason").and_then(Value::as_str).unwrap_or("unknown CloudKit error")
        ));
    }
    let (mut note, embedded_attachment_ids) = match note_detail_from_record(record) {
        Ok(note) => note,
        Err(error) => return NoteDetailResult::error(error.to_string()),
    };
    let ordered_attachment_ids = match ordered_note_attachment_ids(record) {
        Ok(ids) => ids,
        Err(error) => return NoteDetailResult::error(error.to_string()),
    };
    let mut attachment_ids = attachment_references(record);
    attachment_ids.extend(embedded_attachment_ids);
    attachment_ids.sort();
    attachment_ids.dedup();
    log::info!("[NOTES] note={} attachmentRefs={}", note_id, attachment_ids.len());
    if !attachment_ids.is_empty() {
        match query_attachments(&mut session, &base, &attachment_ids).await {
            Ok(attachments) => {
                note.attachments = order_note_attachments(attachments, &ordered_attachment_ids);
            }
            Err(error) => log::warn!("[NOTES] attachment lookup failed: {error}"),
        }
    }
    if let Some(encoded) = record.pointer("/fields/TextDataEncrypted/value").and_then(Value::as_str) {
        match note_content::render_body_html(encoded, &note.title, &note.attachments) {
            Ok(html) => note.body_html = html,
            Err(error) => log::warn!("[NOTES] rich body render failed: {error}"),
        }
    }
    NoteDetailResult::ok(note)
}

pub async fn attachment_image(
    manager: &SessionManager,
    session_id: &str,
    note_id: &str,
    attachment_id: &str,
    original: bool,
) -> AppResult<NoteImage> {
    if session_id.trim().is_empty() {
        return Err(AppError::auth("Missing session ID. Please sign in again."));
    }
    if note_id.trim().is_empty() || attachment_id.trim().is_empty() {
        return Err(AppError::other("Missing note or attachment ID"));
    }
    let handle = manager.get(session_id).await?;
    let mut session = handle.lock().await;
    devices::account_login(&mut session, false).await?;
    let base = notes_base(&session).ok_or_else(|| AppError::other("Notes is not available for this account"))?;
    let record = lookup_note_record(&mut session, &base, note_id).await?;
    if !note_attachment_ids(&record)?.iter().any(|id| id == attachment_id) {
        return Err(AppError::other("Attachment does not belong to this note"));
    }
    let attachments = query_attachments(&mut session, &base, &[attachment_id.to_string()]).await?;
    let attachment = attachments
        .into_iter()
        .find(|attachment| attachment.id == attachment_id)
        .ok_or_else(|| AppError::other("Note attachment was not found"))?;
    let (url, source) = note_image_url(&attachment, original)
        .ok_or_else(|| AppError::other("This attachment does not have a downloadable image"))?;
    if let Some(rest) = url.strip_prefix("data:") {
        let (metadata, encoded) = rest.split_once(',').ok_or_else(|| AppError::other("Invalid image data URL"))?;
        let mime_type = metadata
            .split(';')
            .next()
            .filter(|value| value.starts_with("image/"))
            .ok_or_else(|| AppError::other("Attachment data is not an image"))?
            .to_string();
        let bytes =
            BASE64.decode(encoded).map_err(|error| AppError::other(format!("Could not decode note image: {error}")))?;
        if bytes.len() > MAX_NOTE_IMAGE_BYTES {
            return Err(AppError::other("Note image is too large"));
        }
        return Ok(note_image_result(&attachment, mime_type, bytes, source));
    }

    media::validate_media_url(&url)?;
    let fetched = media::fetch_media(&url, Some(&attachment.name)).await?;
    if fetched.bytes.len() > MAX_NOTE_IMAGE_BYTES {
        return Err(AppError::other("Note image is too large"));
    }
    let mime_type = image_mime_type(&attachment, &fetched.content_type)
        .ok_or_else(|| AppError::other("Attachment is not a supported image"))?;
    Ok(note_image_result(&attachment, mime_type, fetched.bytes, source))
}

fn note_image_url(attachment: &NoteAttachment, original: bool) -> Option<(String, &'static str)> {
    let candidates = if original {
        [
            (attachment.fallback_url.as_deref(), "original"),
            (attachment.download_url.as_deref(), "original"),
            (attachment.preview_url.as_deref(), "preview"),
            (attachment.dark_preview_url.as_deref(), "preview"),
            (attachment.render_url.as_deref(), "rendered"),
        ]
    } else {
        [
            (attachment.render_url.as_deref(), "rendered"),
            (attachment.preview_url.as_deref(), "preview"),
            (attachment.dark_preview_url.as_deref(), "preview"),
            (attachment.fallback_url.as_deref(), "preview"),
            (attachment.download_url.as_deref(), "original"),
        ]
    };
    candidates.into_iter().find_map(|(value, source)| {
        value.filter(|value| !value.trim().is_empty()).map(|value| (value.to_string(), source))
    })
}

fn image_mime_type(attachment: &NoteAttachment, content_type: &str) -> Option<String> {
    let content_type = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if content_type.starts_with("image/") {
        return Some(content_type);
    }
    let uti = attachment.uti.as_deref().unwrap_or("").to_ascii_lowercase();
    if uti.ends_with(".png") || uti == "public.png" {
        return Some("image/png".to_string());
    }
    if uti.ends_with(".jpeg") || uti.ends_with(".jpg") || uti == "public.jpeg" {
        return Some("image/jpeg".to_string());
    }
    if uti.ends_with(".gif") {
        return Some("image/gif".to_string());
    }
    if uti.ends_with(".webp") {
        return Some("image/webp".to_string());
    }
    if uti.ends_with(".heic") {
        return Some("image/heic".to_string());
    }
    if uti == "com.apple.paper" {
        return Some("image/svg+xml".to_string());
    }
    None
}

fn note_image_result(attachment: &NoteAttachment, mime_type: String, bytes: Vec<u8>, source: &str) -> NoteImage {
    NoteImage {
        attachment_id: attachment.id.clone(),
        name: attachment.name.clone(),
        mime_type: mime_type.clone(),
        byte_length: bytes.len(),
        data_url: format!("data:{mime_type};base64,{}", BASE64.encode(bytes)),
        source: source.to_string(),
    }
}
pub async fn create(
    manager: &SessionManager,
    session_id: &str,
    folder_id: Option<String>,
    title: &str,
    body: &str,
) -> NoteMutationResult {
    if session_id.trim().is_empty() {
        return NoteMutationResult::error("Missing session ID. Please sign in again.");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return NoteMutationResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    if let Err(error) = devices::account_login(&mut session, false).await {
        return NoteMutationResult::error(error.to_string());
    }
    let Some(base) = notes_base(&session) else {
        return NoteMutationResult::error("Notes is not available for this account");
    };
    let folder_id = folder_id.filter(|value| !value.trim().is_empty()).unwrap_or_else(|| DEFAULT_FOLDER.to_string());
    match create_with_session(&mut session, &base, &folder_id, title, body).await {
        Ok(note_id) => NoteMutationResult::ok(note_id),
        Err(error) => NoteMutationResult::error(error.to_string()),
    }
}

pub async fn update(
    manager: &SessionManager,
    session_id: &str,
    note_id: &str,
    title: &str,
    body: &str,
) -> NoteMutationResult {
    if session_id.trim().is_empty() {
        return NoteMutationResult::error("Missing session ID. Please sign in again.");
    }
    if note_id.trim().is_empty() {
        return NoteMutationResult::error("Missing note ID");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return NoteMutationResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    if let Err(error) = devices::account_login(&mut session, false).await {
        return NoteMutationResult::error(error.to_string());
    }
    let Some(base) = notes_base(&session) else {
        return NoteMutationResult::error("Notes is not available for this account");
    };
    match update_with_session(&mut session, &base, note_id, title, body).await {
        Ok(note_id) => NoteMutationResult::ok(note_id),
        Err(error) => NoteMutationResult::error(error.to_string()),
    }
}

pub async fn delete(manager: &SessionManager, session_id: &str, note_id: &str) -> NoteMutationResult {
    if session_id.trim().is_empty() {
        return NoteMutationResult::error("Missing session ID. Please sign in again.");
    }
    if note_id.trim().is_empty() {
        return NoteMutationResult::error("Missing note ID");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return NoteMutationResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    if let Err(error) = devices::account_login(&mut session, false).await {
        return NoteMutationResult::error(error.to_string());
    }
    let Some(base) = notes_base(&session) else {
        return NoteMutationResult::error("Notes is not available for this account");
    };
    match delete_with_session(&mut session, &base, note_id).await {
        Ok(note_id) => NoteMutationResult::ok(note_id),
        Err(error) => NoteMutationResult::error(error.to_string()),
    }
}

async fn create_with_session(
    session: &mut SrpSession,
    base: &str,
    folder_id: &str,
    title: &str,
    body: &str,
) -> AppResult<String> {
    let note_id = uuid::Uuid::new_v4().to_string();
    let operation = create_operation(&note_id, folder_id, title, body, now_millis())?;
    let response = post_json(session, &notes_url(session, base, "records/modify"), &modify_body(operation)).await?;
    let record = response_record(&response)?;
    Ok(record.get("recordName").and_then(Value::as_str).unwrap_or(&note_id).to_string())
}

async fn update_with_session(
    session: &mut SrpSession,
    base: &str,
    note_id: &str,
    title: &str,
    body: &str,
) -> AppResult<String> {
    let record = lookup_note_record(session, base, note_id).await?;
    let operation = update_operation(&record, title, body, now_millis())?;
    let response = post_json(session, &notes_url(session, base, "records/modify"), &modify_body(operation)).await?;
    let record = response_record(&response)?;
    Ok(record.get("recordName").and_then(Value::as_str).unwrap_or(note_id).to_string())
}

async fn delete_with_session(session: &mut SrpSession, base: &str, note_id: &str) -> AppResult<String> {
    let record = lookup_note_record(session, base, note_id).await?;
    if folder_id(&record).as_deref() == Some(TRASH_FOLDER) {
        return Ok(note_id.to_string());
    }
    let owner = zone_owner(&record).ok_or_else(|| AppError::other("Note owner is missing"))?.to_string();
    let operation = trash_operation(&record, now_millis())?;
    let response = post_json(session, &notes_url(session, base, "records/modify"), &modify_body(operation)).await?;
    let updated = response_record(&response)?;
    let owner = zone_owner(updated).unwrap_or(&owner).to_string();
    ensure_note_user_specific(session, base, &owner, note_id).await?;
    Ok(note_id.to_string())
}

fn create_operation(note_id: &str, folder_id: &str, title: &str, body: &str, now: i64) -> AppResult<Value> {
    let title = normalized_title(title);
    let text = note_text(&title, body);
    let snippet = note_snippet(body);
    let folder = folder_reference(folder_id, "VALIDATE");
    Ok(json!({
        "operationType": "create",
        "record": {
            "recordName": note_id,
            "fields": {
                "CreationDate": { "value": now },
                "Folders": { "value": [folder.clone()] },
                "Folder": { "value": folder },
                "ModificationDate": { "value": now },
                "TitleEncrypted": { "value": BASE64.encode(title.as_bytes()) },
                "SnippetEncrypted": { "value": BASE64.encode(snippet.as_bytes()) },
                "FirstAttachmentThumbnail": {},
                "FirstAttachmentUTIEncrypted": {},
                "TextDataAsset": {},
                "TextDataEncrypted": { "value": encode_note_document(&text)? }
            },
            "parent": { "recordName": folder_id },
            "createShortGUID": true,
            "recordType": "Note"
        }
    }))
}

fn update_operation(record: &Value, title: &str, body: &str, now: i64) -> AppResult<Value> {
    let ordered_attachment_ids = ordered_note_attachment_ids(record)?;
    let marker_count = body.matches('\u{fffc}').count();
    if marker_count != ordered_attachment_ids.len() {
        return Err(AppError::other("Inline attachment placeholders must remain unchanged when editing this note"));
    }
    let note_id = record
        .get("recordName")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::other("Note record is missing recordName"))?;
    let change_tag = record
        .get("recordChangeTag")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::other("Note record is missing recordChangeTag"))?;
    let folder_id = folder_id(record).unwrap_or_else(|| DEFAULT_FOLDER.to_string());
    let title = normalized_title(title);
    let text = note_text(&title, body);
    let snippet = note_snippet(body);
    let mut updated = json!({
        "recordName": note_id,
        "recordChangeTag": change_tag,
        "fields": {
            "ModificationDate": { "value": now },
            "TitleEncrypted": { "value": BASE64.encode(title.as_bytes()) },
            "SnippetEncrypted": { "value": BASE64.encode(snippet.as_bytes()) },
            "TextDataEncrypted": { "value": encode_note_document_with_attachments(&text, &ordered_attachment_ids)? }
        },
        "parent": { "recordName": folder_id },
        "recordType": "Note"
    });
    if let Some(short_guid) = record.get("shortGUID").and_then(Value::as_str) {
        updated["shortGUID"] = Value::String(short_guid.to_string());
    }
    Ok(json!({ "operationType": "update", "record": updated }))
}

fn trash_operation(record: &Value, now: i64) -> AppResult<Value> {
    let note_id = record
        .get("recordName")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::other("Note record is missing recordName"))?;
    let change_tag = record
        .get("recordChangeTag")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::other("Note record is missing recordChangeTag"))?;
    let mut fields = record.get("fields").and_then(Value::as_object).cloned().unwrap_or_default();
    for field in fields.values_mut() {
        if let Some(field) = field.as_object_mut() {
            field.remove("type");
        }
    }
    let trash = folder_reference(TRASH_FOLDER, "VALIDATE");
    fields.insert("Folder".to_string(), json!({ "value": trash.clone() }));
    fields.insert("Folders".to_string(), json!({ "value": [trash] }));
    fields.insert("FoldersModificationDate".to_string(), json!({ "value": now }));
    let mut updated = json!({
        "recordName": note_id,
        "recordChangeTag": change_tag,
        "fields": fields,
        "parent": { "recordName": TRASH_FOLDER },
        "recordType": "Note"
    });
    if let Some(short_guid) = record.get("shortGUID").and_then(Value::as_str) {
        updated["shortGUID"] = Value::String(short_guid.to_string());
    }
    Ok(json!({ "operationType": "update", "record": updated }))
}

async fn ensure_note_user_specific(session: &mut SrpSession, base: &str, owner: &str, note_id: &str) -> AppResult<()> {
    let record_name = format!("Note_UserSpecific::{owner}/Private::{NOTES_ZONE}::{owner}::{note_id}");
    let lookup = json!({
        "records": [{ "recordName": record_name }],
        "zoneID": { "zoneName": NOTES_ZONE }
    });
    let response = post_json(session, &notes_url(session, base, "records/lookup"), &lookup).await?;
    let existing = response
        .get("records")
        .and_then(Value::as_array)
        .and_then(|records| records.first())
        .filter(|record| record.get("serverErrorCode").is_none());
    let fields = json!({
        "Note": { "value": note_reference(note_id, "NONE") },
        "Folder": { "value": folder_reference(TRASH_FOLDER, "NONE") },
        "LastViewedModificationDate": { "value": 0 }
    });
    let operation = if let Some(existing) = existing {
        let change_tag = existing
            .get("recordChangeTag")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::other("Note user state is missing recordChangeTag"))?;
        json!({
            "operationType": "update",
            "record": {
                "recordName": record_name,
                "recordChangeTag": change_tag,
                "fields": fields,
                "recordType": "Note_UserSpecific"
            }
        })
    } else {
        json!({
            "operationType": "create",
            "record": {
                "recordName": record_name,
                "fields": fields,
                "recordType": "Note_UserSpecific"
            }
        })
    };
    let response = post_json(session, &notes_url(session, base, "records/modify"), &modify_body(operation)).await?;
    response_record(&response)?;
    Ok(())
}

async fn lookup_note_record(session: &mut SrpSession, base: &str, note_id: &str) -> AppResult<Value> {
    let body = json!({
        "records": [{ "recordName": note_id }],
        "zoneID": { "zoneName": NOTES_ZONE }
    });
    let response = post_json(session, &notes_url(session, base, "records/lookup"), &body).await?;
    response_record(&response).cloned()
}

fn response_record(response: &Value) -> AppResult<&Value> {
    if let Some(code) = response.get("serverErrorCode").and_then(Value::as_str) {
        return Err(AppError::other(format!(
            "CloudKit {}: {}",
            code,
            response.get("reason").and_then(Value::as_str).unwrap_or("Notes request failed")
        )));
    }
    let record = response
        .get("records")
        .and_then(Value::as_array)
        .and_then(|records| records.first())
        .ok_or_else(|| AppError::other("CloudKit did not return a note record"))?;
    if let Some(code) = record.get("serverErrorCode").and_then(Value::as_str) {
        return Err(AppError::other(format!(
            "CloudKit {}: {}",
            code,
            record.get("reason").and_then(Value::as_str).unwrap_or("Notes request failed")
        )));
    }
    Ok(record)
}

fn modify_body(operation: Value) -> Value {
    json!({ "operations": [operation], "zoneID": { "zoneName": NOTES_ZONE } })
}

fn folder_reference(folder_id: &str, action: &str) -> Value {
    json!({
        "recordName": folder_id,
        "action": action,
        "zoneID": { "zoneName": NOTES_ZONE }
    })
}

fn note_reference(note_id: &str, action: &str) -> Value {
    json!({
        "recordName": note_id,
        "action": action,
        "zoneID": { "zoneName": NOTES_ZONE }
    })
}

fn zone_owner(record: &Value) -> Option<&str> {
    record
        .pointer("/fields/Folder/value/zoneID/ownerRecordName")
        .and_then(Value::as_str)
        .or_else(|| record.pointer("/fields/Folders/value/0/zoneID/ownerRecordName").and_then(Value::as_str))
}

fn note_attachment_ids(record: &Value) -> AppResult<Vec<String>> {
    let mut ids = attachment_references(record);
    if let Some(encoded) = record.pointer("/fields/TextDataEncrypted/value").and_then(Value::as_str) {
        ids.extend(decode_note_document(encoded)?.1);
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn ordered_note_attachment_ids(record: &Value) -> AppResult<Vec<String>> {
    let Some(encoded) = record.pointer("/fields/TextDataEncrypted/value").and_then(Value::as_str) else {
        return Ok(Vec::new());
    };
    let bytes =
        BASE64.decode(encoded).map_err(|error| AppError::other(format!("Could not decode note body: {error}")))?;
    let raw = decompress_note_document(&bytes).or_else(|_| Ok::<Vec<u8>, AppError>(bytes.clone()))?;
    extract_note_attachment_ids_ordered(&raw)
}

fn order_note_attachments(mut attachments: Vec<NoteAttachment>, ordered_ids: &[String]) -> Vec<NoteAttachment> {
    let mut ordered = Vec::with_capacity(attachments.len());
    for id in ordered_ids {
        if let Some(index) = attachments.iter().position(|attachment| &attachment.id == id) {
            ordered.push(attachments.remove(index));
        }
    }
    ordered.extend(attachments);
    ordered
}

fn normalized_title(title: &str) -> String {
    let title = title.lines().next().unwrap_or("").trim();
    if title.is_empty() {
        "New Note".to_string()
    } else {
        title.to_string()
    }
}

fn strip_note_title(text: &str, title: &str) -> String {
    let mut lines = text.split('\n').peekable();
    while lines.peek().is_some_and(|line| line.trim().is_empty()) {
        lines.next();
    }
    if lines.peek().is_some_and(|line| line.trim() == title.trim()) {
        lines.next();
    }
    lines.collect::<Vec<_>>().join("\n").trim_end_matches('\n').to_string()
}
fn note_text(title: &str, body: &str) -> String {
    let body = body.replace("\r\n", "\n").replace('\r', "\n");
    format!("\n{title}\n{}", body.trim_end_matches('\n'))
}

fn note_snippet(body: &str) -> String {
    body.replace('\u{fffc}', "")
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .chars()
        .take(160)
        .collect()
}

fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(i64::MAX as u128) as i64
}

fn encode_note_document(text: &str) -> AppResult<String> {
    encode_note_document_with_attachments(text, &[])
}

fn encode_note_document_with_attachments(text: &str, attachment_ids: &[String]) -> AppResult<String> {
    let marker_count = text.matches('\u{fffc}').count();
    if marker_count != attachment_ids.len() {
        return Err(AppError::other("Inline attachment IDs do not match note placeholders"));
    }
    let mut topotext = protobuf_bytes(2, text.as_bytes());
    if attachment_ids.is_empty() {
        topotext.extend(protobuf_varint(5, 1));
    } else {
        for (index, segment) in text.split('\u{fffc}').enumerate() {
            if !segment.is_empty() {
                let run = protobuf_varint(1, segment.encode_utf16().count() as u64);
                topotext.extend(protobuf_bytes(5, &run));
            }
            if let Some(attachment_id) = attachment_ids.get(index) {
                let mut run = protobuf_varint(1, 1);
                run.extend(protobuf_bytes(12, &protobuf_bytes(1, attachment_id.as_bytes())));
                topotext.extend(protobuf_bytes(5, &run));
            }
        }
    }
    let mut version = protobuf_varint(1, 0);
    version.extend(protobuf_bytes(3, &topotext));
    let mut document = protobuf_varint(1, 0);
    document.extend(protobuf_bytes(2, &version));
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&document)?;
    let compressed = encoder.finish()?;
    Ok(BASE64.encode(compressed))
}

fn protobuf_bytes(field: u64, bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    put_protobuf_varint((field << 3) | 2, &mut output);
    put_protobuf_varint(bytes.len() as u64, &mut output);
    output.extend_from_slice(bytes);
    output
}

fn protobuf_varint(field: u64, value: u64) -> Vec<u8> {
    let mut output = Vec::new();
    put_protobuf_varint(field << 3, &mut output);
    put_protobuf_varint(value, &mut output);
    output
}

fn put_protobuf_varint(mut value: u64, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}
async fn query_folders(session: &mut SrpSession, base: &str) -> AppResult<Vec<NoteFolder>> {
    let url = notes_url(session, base, "records/query");
    let body = json!({
        "query": {
            "recordType": "SearchIndexes",
            "filterBy": [{
                "comparator": "EQUALS",
                "fieldName": "indexName",
                "fieldValue": { "value": "parentless", "type": "STRING" }
            }]
        },
        "zoneID": { "zoneName": NOTES_ZONE },
        "resultsLimit": 50
    });
    let response = post_json(session, &url, &body).await?;
    let mut folders: Vec<NoteFolder> = response
        .get("records")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|record| record.get("recordType").and_then(Value::as_str) == Some("Folder"))
        .filter_map(|record| {
            let id = record.get("recordName").and_then(Value::as_str)?.to_string();
            let name = encrypted_string(record, "TitleEncrypted").unwrap_or_else(|| "Notes".to_string());
            Some(NoteFolder { id, name })
        })
        .collect();
    folders.sort_by_key(|folder| folder.name.to_lowercase());
    Ok(folders)
}

async fn query_notes(
    session: &mut SrpSession,
    base: &str,
    cursor: Option<String>,
) -> AppResult<(Vec<NoteSummary>, Option<String>)> {
    let url = notes_url(session, base, "records/query");
    let mut body = json!({
        "query": {
            "recordType": "SearchIndexes",
            "filterBy": [{
                "comparator": "EQUALS",
                "fieldName": "indexName",
                "fieldValue": { "value": "recents", "type": "STRING" }
            }],
            "sortBy": [{ "fieldName": "modTime", "ascending": false }]
        },
        "zoneID": { "zoneName": NOTES_ZONE },
        "desiredKeys": DESIRED_KEYS,
        "resultsLimit": NOTES_QUERY_LIMIT
    });
    if let Some(cursor) = cursor.filter(|value| !value.trim().is_empty()) {
        body["continuationMarker"] = Value::String(cursor);
    }
    let response = post_json(session, &url, &body).await?;
    let notes = response
        .get("records")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(note_summary_from_record)
        .filter(|note| !note.deleted && note.folder_id.as_deref() != Some(TRASH_FOLDER))
        .collect();
    let next_cursor = response
        .get("continuationMarker")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string);
    Ok((notes, next_cursor))
}

fn note_summary_from_record(record: &Value) -> Option<NoteSummary> {
    let record_type = record.get("recordType").and_then(Value::as_str)?;
    if record_type != "Note" && record_type != "Note_UserSpecific" {
        return None;
    }
    let id = record.get("recordName").and_then(Value::as_str)?.to_string();
    Some(NoteSummary {
        id,
        title: encrypted_string(record, "TitleEncrypted").unwrap_or_else(|| "Untitled".to_string()),
        snippet: encrypted_string(record, "SnippetEncrypted").unwrap_or_default(),
        folder_id: folder_id(record),
        created_at: timestamp_field(record, "CreationDate"),
        modified_at: timestamp_field(record, "ModificationDate")
            .or_else(|| timestamp_field(record, "LastViewedModificationDate")),
        pinned: bool_field(record, "IsPinned"),
        deleted: bool_field(record, "Deleted") || folder_id(record).as_deref() == Some(TRASH_FOLDER),
        first_attachment_uti: encrypted_string(record, "FirstAttachmentUTIEncrypted"),
    })
}

fn note_detail_from_record(record: &Value) -> AppResult<(NoteDetail, Vec<String>)> {
    if record.get("recordType").and_then(Value::as_str) == Some("PasswordProtectedNote") {
        return Err(AppError::auth("Password-protected notes are not supported yet"));
    }
    let id = record
        .get("recordName")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::other("Note record is missing recordName"))?
        .to_string();
    let title = encrypted_string(record, "TitleEncrypted").unwrap_or_else(|| "Untitled".to_string());
    let encoded_body = record
        .pointer("/fields/TextDataEncrypted/value")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::other("Note body is not available"))?;
    let (body, embedded_attachment_ids) = decode_note_document(encoded_body)?;
    let body = strip_note_title(&body, &title);
    Ok((
        NoteDetail {
            id,
            title,
            body,
            body_html: String::new(),
            folder_id: folder_id(record),
            created_at: timestamp_field(record, "CreationDate"),
            modified_at: timestamp_field(record, "ModificationDate"),
            attachments: Vec::new(),
        },
        embedded_attachment_ids,
    ))
}

async fn query_attachments(
    session: &mut SrpSession,
    base: &str,
    attachment_ids: &[String],
) -> AppResult<Vec<NoteAttachment>> {
    let url = notes_url(session, base, "records/lookup");
    let body = json!({
        "records": attachment_ids.iter().map(|id| json!({ "recordName": id })).collect::<Vec<_>>(),
        "zoneID": { "zoneName": NOTES_ZONE }
    });
    let response = post_json(session, &url, &body).await?;
    let records: Vec<&Value> = response
        .get("records")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|record| record.get("serverErrorCode").is_none())
        .collect();

    let mut media_ids = records
        .iter()
        .filter_map(|record| record.pointer("/fields/Media/value/recordName").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<Vec<_>>();
    media_ids.sort();
    media_ids.dedup();

    let mut media_records = HashMap::new();
    if !media_ids.is_empty() {
        let media_body = json!({
            "records": media_ids.iter().map(|id| json!({ "recordName": id })).collect::<Vec<_>>(),
            "zoneID": { "zoneName": NOTES_ZONE }
        });
        let media_response = post_json(session, &url, &media_body).await?;
        for record in media_response
            .get("records")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|record| record.get("serverErrorCode").is_none())
        {
            if let Some(id) = record.get("recordName").and_then(Value::as_str) {
                media_records.insert(id.to_string(), record.clone());
            }
        }
    }

    let mut attachments = Vec::new();
    for record in records {
        let Some(mut attachment) = note_attachment_from_record(record) else {
            continue;
        };

        if let Some(media_id) = record.pointer("/fields/Media/value/recordName").and_then(Value::as_str) {
            if let Some(media) = media_records.get(media_id) {
                if let Some(original_url) = media
                    .pointer("/fields/Asset/value")
                    .and_then(find_download_url)
                    .or_else(|| find_download_url(media))
                {
                    attachment.download_url = Some(original_url);
                }
                if attachment.name == "Attachment" {
                    if let Some(filename) = encrypted_string(media, "FilenameEncrypted") {
                        attachment.name = filename;
                    }
                }
            }
        }

        if attachment.uti.as_deref() == Some("com.apple.notes.table") {
            if let Some(encoded) = record.pointer("/fields/MergeableDataEncrypted/value").and_then(Value::as_str) {
                match note_content::parse_table(encoded) {
                    Ok(rows) => attachment.table = Some(rows),
                    Err(error) => log::warn!("[NOTES] table parse failed for {}: {error}", attachment.id),
                }
            }
        }
        if attachment.uti.as_deref() == Some("com.apple.paper") {
            if let Some(database_url) = record.pointer("/fields/PaperDatabase/value").and_then(find_download_url) {
                match session.request_bytes(Method::GET, &database_url, None, &[]).await {
                    Ok(download) if download.is_success() => {
                        match note_paper::render_svg(
                            &download.bytes,
                            attachment.width.unwrap_or(768.0),
                            attachment.height.unwrap_or(852.0),
                        ) {
                            Ok(svg) => {
                                attachment.render_url =
                                    Some(format!("data:image/svg+xml;base64,{}", BASE64.encode(svg)));
                            }
                            Err(error) => log::warn!("[NOTES] paper render failed for {}: {error}", attachment.id),
                        }
                    }
                    Ok(download) => log::warn!(
                        "[NOTES] paper database download failed for {}: HTTP {}",
                        attachment.id,
                        download.status
                    ),
                    Err(error) => log::warn!("[NOTES] paper database request failed for {}: {error}", attachment.id),
                }
            }
        }

        attachments.push(attachment);
    }
    Ok(attachments)
}
fn attachment_references(record: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    if let Some(fields) = record.get("fields").and_then(Value::as_object) {
        for (key, field) in fields {
            if key.to_ascii_lowercase().contains("attachment") {
                collect_record_names(field, &mut ids);
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

fn collect_record_names(value: &Value, ids: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(name) = map.get("recordName").and_then(Value::as_str).filter(|name| !name.is_empty()) {
                ids.push(name.to_string());
            }
            if let Some(value) = map.get("value") {
                collect_record_names(value, ids);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_record_names(value, ids);
            }
        }
        _ => {}
    }
}

fn note_attachment_from_record(record: &Value) -> Option<NoteAttachment> {
    let id = record.get("recordName").and_then(Value::as_str)?.to_string();
    let (preview_url, dark_preview_url, fallback_url) = attachment_preview_urls(record);
    let download_url = preview_url.clone().or_else(|| dark_preview_url.clone()).or_else(|| find_download_url(record));
    let name = ["NameEncrypted", "FilenameEncrypted", "FileNameEncrypted", "TitleEncrypted"]
        .into_iter()
        .find_map(|key| encrypted_string(record, key))
        .unwrap_or_else(|| "Attachment".to_string());
    let uti =
        ["UTIEncrypted", "TypeUTIEncrypted", "UtiEncrypted"].into_iter().find_map(|key| encrypted_string(record, key));
    Some(NoteAttachment {
        id,
        name,
        uti,
        preview_url,
        dark_preview_url,
        fallback_url,
        download_url,
        render_url: None,
        table: None,
        file_size: number_field(record, "FileSize").map(|value| value.max(0.0) as u64),
        width: number_field(record, "Width").or_else(|| number_field(record, "PreviewWidth")),
        height: number_field(record, "Height").or_else(|| number_field(record, "PreviewHeight")),
    })
}

fn attachment_preview_urls(record: &Value) -> (Option<String>, Option<String>, Option<String>) {
    let images = record.pointer("/fields/PreviewImages/value").and_then(Value::as_array);
    let appearances = record.pointer("/fields/PreviewAppearances/value").and_then(Value::as_array);
    let mut light = None;
    let mut dark = None;
    let mut fallback = record.pointer("/fields/FallbackImage/value").and_then(find_download_url);
    if let Some(images) = images {
        for (index, image) in images.iter().enumerate() {
            let Some(url) = find_download_url(image) else {
                continue;
            };
            let appearance = appearances
                .and_then(|values| values.get(index))
                .and_then(Value::as_i64)
                .unwrap_or(if index == 1 { 1 } else { 0 });
            if appearance == 1 && dark.is_none() {
                dark = Some(url.clone());
            }
            if appearance == 0 && light.is_none() {
                light = Some(url.clone());
            }
            if light.is_none() {
                light = Some(url);
            }
        }
    }
    if light.is_none() {
        light = fallback.clone();
    }
    if light.is_none() {
        light = record.pointer("/fields/PreviewImage/value").and_then(find_download_url);
    }
    (light, dark, fallback.take())
}
fn find_download_url(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(url) = map.get("downloadURL").and_then(Value::as_str).filter(|url| !url.is_empty()) {
                return Some(url.to_string());
            }
            map.values().find_map(find_download_url)
        }
        Value::Array(values) => values.iter().find_map(find_download_url),
        _ => None,
    }
}

fn number_field(record: &Value, key: &str) -> Option<f64> {
    let value = record.pointer(&format!("/fields/{key}/value"))?;
    value.as_f64().or_else(|| value.as_i64().map(|number| number as f64))
}
fn notes_url(session: &SrpSession, base: &str, endpoint: &str) -> String {
    let client_id = session.client_id();
    let dsid = session.dsid().unwrap_or("null");
    format!(
        "{base}/database/1/com.apple.notes/production/private/{endpoint}?ckjsBuildVersion={NOTES_CKJS_BUILD}&ckjsVersion={NOTES_CKJS_VERSION}&clientId={client_id}&clientBuildNumber={NOTES_CLIENT_BUILD}&clientMasteringNumber={NOTES_CLIENT_MASTERING}&dsid={dsid}"
    )
}

fn notes_base(session: &SrpSession) -> Option<String> {
    if let Some(url) = session.cloud_kit_url().filter(|url| !url.trim().is_empty()) {
        return Some(url.trim_end_matches('/').to_string());
    }
    if session.auth_base().contains(".cn") {
        Some("https://ckdatabasews.icloud.com.cn".to_string())
    } else {
        Some("https://ckdatabasews.icloud.com".to_string())
    }
}

async fn post_json(session: &mut SrpSession, url: &str, body: &Value) -> AppResult<Value> {
    let headers = [(hh::CONTENT_TYPE, "text/plain")];
    let response = session.request(Method::POST, url, Some(body), &headers).await?;
    if response.status != 200 {
        return Err(AppError::auth(format!(
            "Notes request failed: HTTP {} {}",
            response.status,
            auth_service::truncate(&response.body, 500)
        )));
    }
    serde_json::from_str(&response.body).map_err(AppError::from)
}

fn encrypted_string(record: &Value, key: &str) -> Option<String> {
    let value = record.pointer(&format!("/fields/{key}/value"))?.as_str()?;
    if value.is_empty() {
        return None;
    }
    let bytes = BASE64.decode(value).unwrap_or_else(|_| value.as_bytes().to_vec());
    String::from_utf8(bytes).ok().map(|value| value.trim_matches('\0').to_string())
}

fn timestamp_field(record: &Value, key: &str) -> Option<i64> {
    record.pointer(&format!("/fields/{key}/value")).and_then(Value::as_i64)
}

fn bool_field(record: &Value, key: &str) -> bool {
    let Some(value) = record.pointer(&format!("/fields/{key}/value")) else {
        return false;
    };
    value.as_bool().unwrap_or_else(|| value.as_i64().unwrap_or(0) != 0)
}

fn folder_id(record: &Value) -> Option<String> {
    record
        .pointer("/fields/Folder/value/recordName")
        .and_then(Value::as_str)
        .or_else(|| record.pointer("/parent/recordName").and_then(Value::as_str))
        .map(str::to_string)
}

fn decode_note_document(encoded: &str) -> AppResult<(String, Vec<String>)> {
    let bytes =
        BASE64.decode(encoded).map_err(|error| AppError::other(format!("Could not decode note body: {error}")))?;
    let raw = decompress_note_document(&bytes).or_else(|_| Ok::<Vec<u8>, AppError>(bytes.clone()))?;
    if let Some(text) = extract_note_text(&raw)? {
        return Ok((text, extract_note_attachment_ids(&raw)?));
    }
    let text =
        String::from_utf8(raw).map_err(|error| AppError::other(format!("Could not decode note text: {error}")))?;
    Ok((text, Vec::new()))
}

fn decompress_note_document(bytes: &[u8]) -> AppResult<Vec<u8>> {
    let mut output = Vec::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        GzDecoder::new(bytes).read_to_end(&mut output)?;
        return Ok(output);
    }
    if ZlibDecoder::new(bytes).read_to_end(&mut output).is_ok() && !output.is_empty() {
        return Ok(output);
    }
    output.clear();
    DeflateDecoder::new(bytes).read_to_end(&mut output)?;
    Ok(output)
}

fn extract_note_text(document: &[u8]) -> AppResult<Option<String>> {
    let Some(version) = last_length_delimited(document, 2)? else {
        return Ok(None);
    };
    let Some(topotext) = last_length_delimited(version, 3)? else {
        return Ok(None);
    };
    Ok(last_string(topotext, 2)?.map(|value| value.trim_matches('\0').to_string()))
}

fn extract_note_attachment_ids(document: &[u8]) -> AppResult<Vec<String>> {
    let mut ids = extract_note_attachment_ids_ordered(document)?;
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn extract_note_attachment_ids_ordered(document: &[u8]) -> AppResult<Vec<String>> {
    let Some(version) = last_length_delimited(document, 2)? else {
        return Ok(Vec::new());
    };
    let Some(topotext) = last_length_delimited(version, 3)? else {
        return Ok(Vec::new());
    };
    let mut ids = Vec::new();
    for run in length_delimited_values(topotext, 5)? {
        let Some(info) = last_length_delimited(run, 12)? else {
            continue;
        };
        if let Some(identifier) = last_string(info, 1)? {
            let identifier = identifier.trim();
            if !identifier.is_empty() {
                ids.push(identifier.to_string());
            }
        }
    }
    Ok(ids)
}

fn length_delimited_values(data: &[u8], wanted_field: u64) -> AppResult<Vec<&[u8]>> {
    let mut reader = ProtoReader { data, position: 0 };
    let mut values = Vec::new();
    while !reader.done() {
        let tag = reader.varint()?;
        let field = tag >> 3;
        let wire = tag & 7;
        if field == wanted_field && wire == 2 {
            values.push(reader.bytes()?);
        } else {
            reader.skip(wire)?;
        }
    }
    Ok(values)
}
fn last_length_delimited(data: &[u8], wanted_field: u64) -> AppResult<Option<&[u8]>> {
    let mut reader = ProtoReader { data, position: 0 };
    let mut found = None;
    while !reader.done() {
        let tag = reader.varint()?;
        let field = tag >> 3;
        let wire = tag & 7;
        if field == wanted_field && wire == 2 {
            found = Some(reader.bytes()?);
        } else {
            reader.skip(wire)?;
        }
    }
    Ok(found)
}

fn last_string(data: &[u8], wanted_field: u64) -> AppResult<Option<String>> {
    let Some(bytes) = last_length_delimited(data, wanted_field)? else {
        return Ok(None);
    };
    Ok(Some(String::from_utf8_lossy(bytes).to_string()))
}

struct ProtoReader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> ProtoReader<'a> {
    fn done(&self) -> bool {
        self.position >= self.data.len()
    }

    fn varint(&mut self) -> AppResult<u64> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let byte = *self.data.get(self.position).ok_or_else(|| AppError::other("protobuf: truncated varint"))?;
            self.position += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(AppError::other("protobuf: varint is too long"))
    }

    fn bytes(&mut self) -> AppResult<&'a [u8]> {
        let length = self.varint()? as usize;
        let end = self
            .position
            .checked_add(length)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| AppError::other("protobuf: truncated length-delimited field"))?;
        let value = &self.data[self.position..end];
        self.position = end;
        Ok(value)
    }

    fn skip(&mut self, wire_type: u64) -> AppResult<()> {
        match wire_type {
            0 => {
                self.varint()?;
            }
            1 => self.advance(8)?,
            2 => {
                let length = self.varint()? as usize;
                self.advance(length)?;
            }
            5 => self.advance(4)?,
            other => return Err(AppError::other(format!("protobuf: unsupported wire type {other}"))),
        }
        Ok(())
    }

    fn advance(&mut self, amount: usize) -> AppResult<()> {
        self.position = self
            .position
            .checked_add(amount)
            .filter(|position| *position <= self.data.len())
            .ok_or_else(|| AppError::other("protobuf: truncated field"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::{GzEncoder, ZlibEncoder};
    use flate2::Compression;
    use std::io::Write;

    fn field_varint(field: u64, value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        put_varint(field << 3, &mut out);
        put_varint(value, &mut out);
        out
    }

    fn field_bytes(field: u64, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        put_varint((field << 3) | 2, &mut out);
        put_varint(bytes.len() as u64, &mut out);
        out.extend_from_slice(bytes);
        out
    }

    fn put_varint(mut value: u64, out: &mut Vec<u8>) {
        while value >= 0x80 {
            out.push((value as u8) | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    fn note_record_with_inline_attachments(text: &str, attachment_ids: &[String]) -> Value {
        let encoded = encode_note_document_with_attachments(text, attachment_ids).expect("encode inline note");
        json!({
            "recordName": "note-1",
            "recordChangeTag": "tag-1",
            "recordType": "Note",
            "fields": {
                "Folder": { "value": { "recordName": DEFAULT_FOLDER } },
                "TextDataEncrypted": { "value": encoded }
            }
        })
    }

    fn encoded_note(text: &str, gzip: bool) -> String {
        let mut topotext = field_bytes(2, text.as_bytes());
        topotext.extend(field_varint(5, 1));
        let mut version = field_varint(1, 0);
        version.extend(field_bytes(3, &topotext));
        let mut document = field_varint(1, 0);
        document.extend(field_bytes(2, &version));
        let mut compressed = Vec::new();
        if gzip {
            let mut encoder = GzEncoder::new(&mut compressed, Compression::default());
            encoder.write_all(&document).expect("gzip note");
            encoder.finish().expect("finish gzip");
        } else {
            let mut encoder = ZlibEncoder::new(&mut compressed, Compression::default());
            encoder.write_all(&document).expect("zlib note");
            encoder.finish().expect("finish zlib");
        }
        BASE64.encode(compressed)
    }

    #[test]
    fn extracts_attachment_identifiers_from_attribute_runs() {
        let mut info = field_bytes(1, b"attachment-1");
        info.extend(field_bytes(2, b"public.png"));
        let mut run = field_varint(1, 1);
        run.extend(field_bytes(12, &info));
        let mut topotext = field_bytes(2, "图片\u{FFFC}".as_bytes());
        topotext.extend(field_bytes(5, &run));
        let mut version = field_varint(1, 0);
        version.extend(field_bytes(3, &topotext));
        let mut document = field_varint(1, 0);
        document.extend(field_bytes(2, &version));
        let mut compressed = Vec::new();
        let mut encoder = GzEncoder::new(&mut compressed, Compression::default());
        encoder.write_all(&document).expect("gzip note with attachment");
        encoder.finish().expect("finish gzip");
        let encoded = BASE64.encode(compressed);

        let (body, attachments) = decode_note_document(&encoded).expect("decode note");
        assert_eq!(body, "图片\u{FFFC}");
        assert_eq!(attachments, vec!["attachment-1"]);
    }
    #[test]
    fn decodes_gzip_and_zlib_note_documents() {
        assert_eq!(decode_note_document(&encoded_note("第一行\n正文内容", true)).expect("gzip").0, "第一行\n正文内容");
        assert_eq!(decode_note_document(&encoded_note("plain body", false)).expect("zlib").0, "plain body");
    }

    #[test]
    fn parses_attachment_preview_metadata() {
        let uti = BASE64.encode("com.apple.paper");
        let record = json!({
            "recordName": "attachment-1",
            "fields": {
                "UTIEncrypted": { "value": uti },
                "PreviewImages": {
                    "value": [
                        { "downloadURL": "https://cvws.icloud-content.com.cn/light.png", "size": 1234 },
                        { "downloadURL": "https://cvws.icloud-content.com.cn/dark.png", "size": 1200 }
                    ]
                },
                "PreviewAppearances": { "value": [0, 1] },
                "FallbackImage": { "value": { "downloadURL": "https://cvws.icloud-content.com.cn/fallback.png" } },
                "Width": { "value": 768.0 },
                "Height": { "value": 221.0 }
            }
        });
        let attachment = note_attachment_from_record(&record).expect("attachment");
        assert_eq!(attachment.preview_url.as_deref(), Some("https://cvws.icloud-content.com.cn/light.png"));
        assert_eq!(attachment.dark_preview_url.as_deref(), Some("https://cvws.icloud-content.com.cn/dark.png"));
        assert_eq!(attachment.fallback_url.as_deref(), Some("https://cvws.icloud-content.com.cn/fallback.png"));
        assert_eq!(attachment.uti.as_deref(), Some("com.apple.paper"));
        assert_eq!(attachment.width, Some(768.0));
        assert_eq!(attachment.height, Some(221.0));
    }
    #[test]
    fn parses_note_summary_and_folder() {
        let title = BASE64.encode("购物清单");
        let snippet = BASE64.encode("牛奶、鸡蛋");
        let record = json!({
            "recordName": "note-1",
            "recordType": "Note",
            "fields": {
                "TitleEncrypted": { "value": title },
                "SnippetEncrypted": { "value": snippet },
                "ModificationDate": { "value": 1700000000000i64 },
                "Folder": { "value": { "recordName": "folder-1" } },
                "IsPinned": { "value": 1 }
            }
        });
        let note = note_summary_from_record(&record).expect("note summary");
        assert_eq!(note.title, "购物清单");
        assert_eq!(note.snippet, "牛奶、鸡蛋");
        assert_eq!(note.folder_id.as_deref(), Some("folder-1"));
        assert!(note.pinned);
    }
    #[test]
    fn encodes_plain_text_notes() {
        let encoded = encode_note_document("\n标题\n正文\n第二行").expect("encode note");
        let (text, attachments) = decode_note_document(&encoded).expect("decode note");
        assert_eq!(text, "\n标题\n正文\n第二行");
        assert!(attachments.is_empty());
    }

    #[test]
    fn builds_create_operation_for_a_folder() {
        let operation = create_operation("note-1", "folder-1", "标题", "正文", 123).expect("create operation");
        assert_eq!(operation["operationType"], "create");
        assert_eq!(operation["record"]["fields"]["Folder"]["value"]["recordName"], "folder-1");
        assert_eq!(operation["record"]["parent"]["recordName"], "folder-1");
        assert_eq!(encrypted_string(&operation["record"], "TitleEncrypted").as_deref(), Some("标题"));
        let encoded = operation["record"]["fields"]["TextDataEncrypted"]["value"].as_str().expect("encoded body");
        assert_eq!(decode_note_document(encoded).expect("decode body").0, "\n标题\n正文");
    }

    #[test]
    fn update_operation_keeps_cloudkit_change_tag() {
        let encoded = encode_note_document("\n旧标题\n旧正文").expect("encode note");
        let record = json!({
            "recordName": "note-1",
            "recordChangeTag": "tag-1",
            "shortGUID": "short-1",
            "recordType": "Note",
            "fields": {
                "Folder": { "value": { "recordName": "DefaultFolder-CloudKit" } },
                "TextDataEncrypted": { "value": encoded }
            }
        });
        let operation = update_operation(&record, "新标题", "新正文", 456).expect("update operation");
        assert_eq!(operation["record"]["recordChangeTag"], "tag-1");
        assert_eq!(operation["record"]["shortGUID"], "short-1");
        assert_eq!(operation["record"]["fields"]["ModificationDate"]["value"], 456);
    }

    #[test]
    fn update_operation_preserves_inline_attachments_and_edited_text() {
        let record =
            note_record_with_inline_attachments("\n旧标题\n图片前\u{fffc}图片后", &["attachment-1".to_string()]);
        let operation = update_operation(&record, "新标题", "修改后的图片前\u{fffc}修改后的图片后", 456)
            .expect("update inline note");
        let encoded = operation["record"]["fields"]["TextDataEncrypted"]["value"].as_str().expect("encoded body");
        let (body, attachments) = decode_note_document(encoded).expect("decode updated note");
        assert_eq!(body, "\n新标题\n修改后的图片前\u{fffc}修改后的图片后");
        assert_eq!(attachments, vec!["attachment-1"]);
    }

    #[test]
    fn update_operation_rejects_removed_inline_attachments() {
        let record = note_record_with_inline_attachments("\n标题\n图片前\u{fffc}图片后", &["attachment-1".to_string()]);
        let error = update_operation(&record, "标题", "没有图片占位符", 456)
            .expect_err("removing an inline attachment should fail");
        assert!(error.to_string().contains("placeholders"));
    }

    #[test]
    fn update_operation_preserves_multiple_inline_attachment_order() {
        let attachment_ids = vec!["attachment-z".to_string(), "attachment-a".to_string()];
        let record = note_record_with_inline_attachments("\n标题\na\u{fffc}b\u{fffc}c", &attachment_ids);
        let operation =
            update_operation(&record, "标题", "前😀\u{fffc}中🙂\u{fffc}后", 456).expect("update inline note");
        let encoded = operation["record"]["fields"]["TextDataEncrypted"]["value"].as_str().expect("encoded body");
        let bytes = BASE64.decode(encoded).expect("base64 note");
        let raw = decompress_note_document(&bytes).expect("compressed note");
        assert_eq!(extract_note_attachment_ids_ordered(&raw).expect("ordered ids"), attachment_ids);

        let attachments = vec![image_attachment_with_id("attachment-z"), image_attachment_with_id("attachment-a")];
        let html = note_content::render_body_html(encoded, "标题", &attachments).expect("render inline note");
        let first = html.find("data-note-attachment-id=\"attachment-z\"").expect("first image");
        let second = html.find("data-note-attachment-id=\"attachment-a\"").expect("second image");
        assert!(first < second);
        assert!(html.contains("前😀"));
        assert!(html.contains("中🙂"));
        assert!(html.contains("后"));
    }

    #[test]
    fn order_note_attachments_follows_inline_document_order() {
        let attachments = vec![image_attachment_with_id("attachment-a"), image_attachment_with_id("attachment-z")];
        let ordered = order_note_attachments(attachments, &["attachment-z".to_string(), "attachment-a".to_string()]);
        assert_eq!(
            ordered.iter().map(|attachment| attachment.id.as_str()).collect::<Vec<_>>(),
            vec!["attachment-z", "attachment-a"]
        );
    }

    #[test]
    fn trash_operation_moves_the_note_to_trash() {
        let record = json!({
            "recordName": "note-1",
            "recordChangeTag": "tag-1",
            "recordType": "Note",
            "fields": {
                "Folder": { "value": { "recordName": "DefaultFolder-CloudKit" } },
                "Folders": { "value": [{ "recordName": "DefaultFolder-CloudKit" }] },
                "TitleEncrypted": { "value": "dGl0bGU=", "type": "ENCRYPTED_BYTES" }
            }
        });
        let operation = trash_operation(&record, 789).expect("trash operation");
        assert_eq!(operation["record"]["fields"]["Folder"]["value"]["recordName"], TRASH_FOLDER);
        assert_eq!(operation["record"]["fields"]["Folders"]["value"][0]["recordName"], TRASH_FOLDER);
        assert_eq!(operation["record"]["parent"]["recordName"], TRASH_FOLDER);
        assert_eq!(operation["record"]["fields"]["TitleEncrypted"]["value"], "dGl0bGU=");
    }

    #[test]
    fn strips_the_first_title_line_from_detail_body() {
        assert_eq!(strip_note_title("\n标题\n正文\n第二行\n", "标题"), "正文\n第二行");
        assert_eq!(strip_note_title("正文", "标题"), "正文");
    }
    fn image_attachment_with_id(id: &str) -> NoteAttachment {
        let mut attachment = image_attachment();
        attachment.id = id.to_string();
        attachment.name = format!("{id}.png");
        attachment.download_url = Some(format!("https://cvws.icloud-content.com.cn/{id}.png"));
        attachment
    }

    fn image_attachment() -> NoteAttachment {
        NoteAttachment {
            id: "attachment-1".to_string(),
            name: "photo.png".to_string(),
            uti: Some("public.png".to_string()),
            preview_url: Some("https://cvws.icloud-content.com.cn/preview.png".to_string()),
            dark_preview_url: None,
            fallback_url: None,
            download_url: Some("https://cvws.icloud-content.com.cn/original.png".to_string()),
            render_url: None,
            table: None,
            file_size: Some(1024),
            width: Some(100.0),
            height: Some(200.0),
        }
    }

    #[test]
    fn note_image_prefers_preview_but_can_request_original() {
        let attachment = image_attachment();
        assert_eq!(note_image_url(&attachment, false).expect("preview").1, "preview");
        assert_eq!(note_image_url(&attachment, true).expect("original").1, "original");
        assert_eq!(image_mime_type(&attachment, "application/octet-stream").as_deref(), Some("image/png"));
    }
}
