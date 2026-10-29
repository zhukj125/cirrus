use std::collections::HashMap;
use std::io::Read;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use reqwest::Method;
use serde_json::{json, Value};

use crate::apple::http::headers as hh;
use crate::error::{AppError, AppResult};
use crate::models::{NoteAttachment, NoteDetail, NoteDetailResult, NoteFolder, NoteSummary, NotesListResult};
use crate::services::{auth as auth_service, devices, note_content, note_paper};
use crate::session::SrpSession;
use crate::SessionManager;

const NOTES_ZONE: &str = "Notes";
const NOTES_QUERY_LIMIT: i64 = 100;
const NOTES_CLIENT_BUILD: &str = "2640Build18";
const NOTES_CLIENT_MASTERING: &str = "M2640Build18";
const NOTES_CKJS_BUILD: &str = "2310ProjectDev27";
const NOTES_CKJS_VERSION: &str = "2.6.4";

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
    let mut attachment_ids = attachment_references(record);
    attachment_ids.extend(embedded_attachment_ids);
    attachment_ids.sort();
    attachment_ids.dedup();
    log::info!("[NOTES] note={} attachmentRefs={}", note_id, attachment_ids.len());
    if !attachment_ids.is_empty() {
        match query_attachments(&mut session, &base, &attachment_ids).await {
            Ok(attachments) => note.attachments = attachments,
            Err(error) => log::warn!("[NOTES] attachment lookup failed: {error}"),
        }
    }
    if let Some(encoded) = record.pointer("/fields/TextDataEncrypted/value").and_then(Value::as_str) {
        match note_content::render_body_html(encoded, &note.attachments) {
            Ok(html) => note.body_html = html,
            Err(error) => log::warn!("[NOTES] rich body render failed: {error}"),
        }
    }
    NoteDetailResult::ok(note)
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
        .filter(|note| !note.deleted)
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
        deleted: bool_field(record, "Deleted"),
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
    ids.sort();
    ids.dedup();
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
}
