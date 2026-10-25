use std::collections::{BTreeMap, BTreeSet, HashMap};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use futures_util::{stream, StreamExt};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::apple::api;
use crate::apple::http::{client as hc, headers as hh};
use crate::error::{AppError, AppResult};
use crate::models::session_keys::*;
use crate::models::{
    statuses, AssetBucketDto, AssetDistributionDto, LibraryTotal, MonthEntryDto, MonthExactDto, PhoneOption, PhotoItem,
    Photos2faOptions, PhotosLoginResult, PhotosPage, PhotosTimelineDto, SrpAuthRequest, TimelineDto, YearEntryDto,
};
use crate::services::auth as auth_service;
use crate::services::devices;
use crate::session::{now_millis, SrpSession};
use crate::session_manager::SessionManager;

use self::cloud_kit_schema as ck;
use self::cloud_kit_schema::distribution as pbd;
use self::cloud_kit_schema::query as ckq;

pub mod photos_api {

    pub const SECURITY_CODE: &str = "securityCode";

    pub const CODE: &str = "code";

    pub const PHONE_NUMBER_ID: &str = "phoneNumberId";

    pub const DEVICE_CODE_LEN: usize = 6;

    pub const PHONE_CODE_MIN_LEN: usize = 4;
    pub const PHONE_CODE_MAX_LEN: usize = 8;

    pub const MAX_PAGE_SIZE: i32 = 200;
    pub const DEFAULT_PAGE_SIZE: i32 = 60;

    pub const RECORDS_PER_PHOTO: i32 = 2;

    pub const PHOTO_REQUEST_CONCURRENCY: usize = 4;

    pub const ACCESS_PROBE_INTERVAL_MS: i64 = 5_000;

    pub const TRUNCATE_DEFAULT: usize = 200;
    pub const TRUNCATE_ERROR: usize = 300;
    pub const TRUNCATE_LOG: usize = 400;
    pub const TRUNCATE_AUTH_PROBE: usize = 1_600;

    pub mod status_fields {
        pub const STATE: &str = "state";
        pub const AUTHENTICATED: &str = "authenticated";
        pub const ACTIVATED: &str = "activated";
    }
}

pub mod cloud_kit_schema {

    pub const TIMELINE_RECORD_TYPES: &[&str] =
        &["CPLAssetAndMasterByAssetDate", "CPLAssetByAssetDateWithoutHiddenOrDeleted"];

    pub const RECORD_TYPE_MASTER: &str = "CPLMaster";
    pub const RECORD_TYPE_ASSET: &str = "CPLAsset";

    pub const RECORD_NAME_LIBRARY_INFO: &str = "PrimarySync-0000-LI";
    pub const RECORD_TYPE_LIBRARY_INFO: &str = "CPLLibraryInfo";

    pub const RECORD_NAME_ASSET_DISTRIBUTION: &str = "PrimarySync-0000-CI-CPLAssetByAssetDateWithoutHiddenOrDeleted";

    pub const FIELD_ASSET_DISTRIBUTION: &str = "cplAssetDistribution";

    pub const FIELD_PHOTOS_COUNT: &str = "photosCount";
    pub const FIELD_VIDEOS_COUNT: &str = "videosCount";

    pub const ZONES: &str = "zones";

    pub const FIELD_OWNER_RECORD_NAME: &str = "ownerRecordName";

    pub const FIELD_RECORD_TYPE: &str = "recordType";
    pub const FIELD_FIELDS: &str = "fields";

    pub const FIELD_RECORD_ID: &str = "recordId";

    pub const FIELD_ASSET_DATE: &str = "assetDate";
    pub const FIELD_MASTER_REF: &str = "masterRef";
    pub const FIELD_FILENAME: &str = "filename";
    pub const FIELD_ITEM_NAME: &str = "itemName";
    pub const FIELD_DURATION: &str = "duration";
    pub const FIELD_DURATION_ENC: &str = "durationEnc";
    pub const FIELD_VIDEO_COMPLEMENT_DURATION: &str = "vidComplDurValue";
    pub const FIELD_RECORD_NAME: &str = "recordName";

    pub const WRAPPER_VALUE: &str = "value";
    pub const ASSET_DOWNLOAD_URL: &str = "downloadURL";
    pub const ASSET_LEGACY_URL: &str = "url";
    pub const FILE_SIZE: &str = "size";

    pub const THUMB_FIELDS: &[&str] = &["resJPEGThumbRes", "resJpegSmallThumb", "resJPEGMedRes"];

    pub const VIDEO_SIZE_FIELDS: &[(&str, &str)] = &[
        ("resVidComplWidth", "resVidComplHeight"),
        ("resVidMedWidth", "resVidMedHeight"),
        ("resVidSmallWidth", "resVidSmallHeight"),
        ("resVidFullWidth", "resVidFullHeight"),
        ("resVidHDRMedWidth", "resVidHDRMedHeight"),
        ("resOriginalWidth", "resOriginalHeight"),
    ];
    pub const THUMB_SIZE_FIELDS: &[(&str, &str)] = &[
        ("resJPEGThumbWidth", "resJPEGThumbHeight"),
        ("resJPEGMedWidth", "resJPEGMedHeight"),
        ("resOriginalWidth", "resOriginalHeight"),
    ];

    pub const DISPLAY_FIELDS: &[&str] = &["resJPEGMedRes", "resJPEGFullRes", "resJpegMediumThumb", "resOriginalRes"];

    pub const ORIGINAL_FIELDS: &[&str] = &["resOriginalRes", "resJPEGFullRes"];

    pub const ORIGINAL_RES: &str = "resOriginalRes";

    pub const VIDEO_FIELDS: &[&str] = &[
        "resVidComplRes",
        "resVidFullRes",
        "resVidMedRes",
        "resVidHDRMedRes",
        "resVidSmallRes",
        "resVidSmallThumb",
        "resVidMedThumb",
        "resJPEGThumbWidth",
        "resJPEGThumbHeight",
        "resJPEGMedWidth",
        "resJPEGMedHeight",
        "resOriginalWidth",
        "resOriginalHeight",
        "resVidComplWidth",
        "resVidComplHeight",
        "resVidMedWidth",
        "resVidMedHeight",
        "resVidSmallWidth",
        "resVidSmallHeight",
        "resVidFullWidth",
        "resVidFullHeight",
        "resVidHDRMedWidth",
        "resVidHDRMedHeight",
    ];

    pub const DESIRED_KEYS: &[&str] = &[
        "filename",
        "itemName",
        "assetDate",
        "addedDate",
        "masterRef",
        "duration",
        "durationEnc",
        "vidComplDurValue",
        "videoFrameRate",
        "itemType",
        "itemTypeEnc",
        "codec",
        "resJPEGThumbRes",
        "resJPEGMedRes",
        "resJPEGFullRes",
        "resOriginalRes",
        "resJpegSmallThumb",
        "resJpegMediumThumb",
        "resVidComplRes",
        "resVidFullRes",
        "resVidMedRes",
        "resVidHDRMedRes",
        "resVidSmallRes",
        "resVidSmallThumb",
        "resVidMedThumb",
    ];

    pub const FILE_TYPE_PLACEHOLDER: &str = "${f}";

    pub const IMAGE_FILE_TYPE: &str = "unknown.jpeg";

    pub const VIDEO_FILE_TYPE: &str = "unknown.mp4";

    pub const RECORDS: &str = "records";
    pub const CONTINUATION_MARKER: &str = "continuationMarker";

    pub mod query {
        pub const QUERY: &str = "query";
        pub const RECORD_TYPE: &str = "recordType";
        pub const FILTER_BY: &str = "filterBy";
        pub const SORT_BY: &str = "sortBy";
        pub const FIELD_NAME: &str = "fieldName";
        pub const COMPARATOR: &str = "comparator";
        pub const FIELD_VALUE: &str = "fieldValue";
        pub const VALUE_TYPE: &str = "type";
        pub const VALUE: &str = "value";
        pub const ASCENDING: &str = "ascending";
        pub const ZONE_ID: &str = "zoneID";
        pub const ZONE_NAME: &str = "zoneName";

        pub const ZONE_TYPE: &str = "zoneType";

        pub const ZONE_TYPE_REGULAR_CUSTOM: &str = "REGULAR_CUSTOM_ZONE";
        pub const DESIRED_KEYS: &str = "desiredKeys";
        pub const RESULTS_LIMIT: &str = "resultsLimit";
        pub const FILTER_START_DATE: &str = "startDate";
        pub const TYPE_TIMESTAMP: &str = "TIMESTAMP";
        pub const COMPARATOR_GREATER_THAN: &str = "GREATER_THAN";

        pub const TYPE_INT64: &str = "INT64";
    }

    pub mod rank {

        pub const RECORD_TYPE: &str = "CPLAssetAndMasterByAssetDateWithoutHiddenOrDeleted";

        pub const FIELD_DIRECTION: &str = "direction";
        pub const DIRECTION_DESCENDING: &str = "DESCENDING";

        pub const FIELD_START_RANK: &str = "startRank";

        pub const TYPE_STRING: &str = "STRING";
        pub const COMPARATOR_EQUALS: &str = "EQUALS";
    }

    /// Protobuf field IDs for CPLAssetDistribution.
    pub mod distribution {

        pub const FIELD_BUCKET_WIDTH: u64 = 2;

        pub const FIELD_BUCKET: u64 = 3;

        pub const FIELD_BUCKET_START: u64 = 4;

        pub const FIELD_BUCKET_COUNT: u64 = 5;

        pub const WIRE_VARINT: u64 = 0;
        pub const WIRE_FIXED64: u64 = 1;
        pub const WIRE_LENGTH_DELIMITED: u64 = 2;
        pub const WIRE_FIXED32: u64 = 5;
    }
}

const VERIFY_FALLBACK_MESSAGE: &str = "Verification code is incorrect or expired";

const WAIT_2FA_MESSAGE: &str = "Waiting for two-factor authentication";

pub async fn login(
    manager: &SessionManager,
    apple_id: &str,
    password: &str,
    china_mainland: bool,
) -> PhotosLoginResult {
    if apple_id.trim().is_empty() || password.trim().is_empty() {
        return PhotosLoginResult::error("Enter your Apple ID and password");
    }

    let request = SrpAuthRequest { apple_id: apple_id.to_string(), password: password.to_string(), china_mainland };
    let auth = match auth_service::authenticate(manager, &request).await {
        Ok(result) => result,
        Err(error) => {
            log::warn!("[PHOTOS] sign-in failed: {error}");
            return PhotosLoginResult::error(error.to_string());
        }
    };

    let Some(session_id) = auth.session_id.clone() else {
        return PhotosLoginResult::error("Authentication did not return a session ID");
    };

    match auth.status.as_str() {
        statuses::TWO_FA_REQUIRED => {
            let handle = match manager.get(&session_id).await {
                Ok(handle) => handle,
                Err(error) => return PhotosLoginResult::error(error.to_string()),
            };
            let mut session = handle.lock().await;

            let _ = request_device_code_locked(&mut session).await;
            PhotosLoginResult::need_2fa(session_id, auth.auth_type.clone())
        }
        statuses::SUCCESS => {
            let handle = match manager.get(&session_id).await {
                Ok(handle) => handle,
                Err(error) => return PhotosLoginResult::error(error.to_string()),
            };
            let mut session = handle.lock().await;
            if let Err(error) = devices::account_login(&mut session, false).await {
                log::warn!("[PHOTOS] sign-in failed: {error}");
                return PhotosLoginResult::error(error.to_string());
            }
            session.set_session_data(PHOTOS_VERIFIED, FLAG_TRUE);
            ok_result(&session)
        }
        _ => PhotosLoginResult::error(auth.message.clone().unwrap_or_else(|| "Authentication failed".to_string())),
    }
}

pub async fn request_device_code(manager: &SessionManager, session_id: &str) -> PhotosLoginResult {
    if session_id.trim().is_empty() {
        return PhotosLoginResult::error("Missing session ID. Please sign in again.");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return PhotosLoginResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    request_device_code_locked(&mut session).await
}

async fn request_device_code_locked(session: &mut SrpSession) -> PhotosLoginResult {
    let url = format!("{}{}", session.auth_base(), api::auth::VERIFY_TRUSTED_DEVICE);
    let owned = device_headers(session);
    let refs = header_refs(&owned);
    let response = match session.request(Method::PUT, &url, None, &refs).await {
        Ok(response) => response,
        Err(error) => {
            log::warn!("[PHOTOS] failed to request device verification code: {error}");
            return PhotosLoginResult::error(error.to_string());
        }
    };
    log::info!(
        "[PHOTOS] requesting device verification code: HTTP {} body={}",
        response.status,
        auth_service::truncate(&response.body, photos_api::TRUNCATE_LOG)
    );
    if response.status == 200 || response.status == 202 {
        return PhotosLoginResult {
            status: statuses::CODE_SENT.to_string(),
            message: Some("Verification code sent to your trusted device".to_string()),
            session_id: Some(session.id().to_string()),
            auth_type: None,
            first_name: None,
            last_name: None,
        };
    }
    PhotosLoginResult::error(format!(
        "verification code delivery failed: HTTP {} {}",
        response.status,
        auth_service::truncate(&response.body, photos_api::TRUNCATE_ERROR)
    ))
}

pub async fn verify_2fa(manager: &SessionManager, session_id: &str, code: &str) -> PhotosLoginResult {
    if session_id.trim().is_empty() {
        return PhotosLoginResult::error("Missing session ID. Please sign in again.");
    }
    let trimmed = strip_whitespace(code);
    if !is_device_code(&trimmed) {
        return PhotosLoginResult::error("Enter the 6-digit verification code");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return PhotosLoginResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    verify_device_code(&mut session, &trimmed).await
}

async fn verify_device_code(session: &mut SrpSession, code: &str) -> PhotosLoginResult {
    let headers = device_headers(session);
    let body = json_object(vec![(
        photos_api::SECURITY_CODE,
        json_object(vec![(photos_api::CODE, Value::String(code.to_string()))]),
    )]);
    let url = format!("{}{}", session.auth_base(), api::auth::VERIFY_TRUSTED_DEVICE);
    let refs = header_refs(&headers);
    let response = match session.request(Method::POST, &url, Some(&body), &refs).await {
        Ok(response) => response,
        Err(error) => return PhotosLoginResult::error(error.to_string()),
    };
    log::info!(
        "[PHOTOS] POST /verify/trusteddevice/securitycode: HTTP {} body={}",
        response.status,
        auth_service::truncate(&response.body, photos_api::TRUNCATE_LOG)
    );
    if response.status != 204 && response.status != 200 {
        return PhotosLoginResult::error(format!(
            "verification failed: HTTP {} {}",
            response.status,
            extract_verify_error(&response.body)
        ));
    }
    activate(session, &headers).await
}

async fn activate(session: &mut SrpSession, headers: &[(&'static str, String)]) -> PhotosLoginResult {
    let url = format!("{}{}", session.auth_base(), api::auth::TRUST);
    let refs = header_refs(headers);
    let trust = match session.request(Method::GET, &url, None, &refs).await {
        Ok(response) => response,
        Err(error) => {
            log::warn!("[PHOTOS] service activation failed: {error}");
            return PhotosLoginResult::error(format!("Verification succeeded, but service activation failed: {error}"));
        }
    };
    log::info!("[PHOTOS] GET /2sv/trust: HTTP {}", trust.status);

    if let Err(error) = devices::account_login(session, true).await {
        log::warn!("[PHOTOS] service activation failed: {error}");
        return PhotosLoginResult::error(format!("Verification succeeded, but service activation failed: {error}"));
    }
    session.set_session_data(PHOTOS_VERIFIED, FLAG_TRUE);
    log::info!("[PHOTOS] 2FA verification succeeded (dsid={:?}, photos={:?})", session.dsid(), session.photos_url());
    ok_result(session)
}

fn device_headers(session: &SrpSession) -> Vec<(&'static str, String)> {
    let mut headers = session.auth_headers();
    headers.push((hh::DOMAIN_ID, hc::ICLOUD_DOMAIN_ID.to_string()));
    headers.push((hh::LOCALE, hc::LOCALE_ZH_CN.to_string()));
    headers
}

pub async fn status(manager: &SessionManager, session_id: &str) -> Value {
    let mut result = Map::new();
    let Ok(handle) = manager.get(session_id).await else {
        result.insert(photos_api::status_fields::STATE.to_string(), Value::String(statuses::PENDING.to_string()));
        result.insert(photos_api::status_fields::AUTHENTICATED.to_string(), Value::Bool(false));
        return Value::Object(result);
    };
    let session = handle.lock().await;
    let activated = is_verified(&session);
    result.insert(
        photos_api::status_fields::STATE.to_string(),
        Value::String(if activated { statuses::AUTHENTICATED.to_string() } else { statuses::PENDING.to_string() }),
    );
    result.insert(photos_api::status_fields::AUTHENTICATED.to_string(), Value::Bool(activated));
    result.insert(photos_api::status_fields::ACTIVATED.to_string(), Value::Bool(activated));
    if activated {
        result.insert(api::json::FIRST_NAME.to_string(), option_string(session.first_name()));
        result.insert(api::json::LAST_NAME.to_string(), option_string(session.last_name()));
    }
    Value::Object(result)
}

fn is_verified(session: &SrpSession) -> bool {
    session.is_flag_set(PHOTOS_VERIFIED)
}

pub async fn check_approval(manager: &SessionManager, session_id: &str) -> PhotosLoginResult {
    if session_id.trim().is_empty() {
        return PhotosLoginResult::error("Missing session ID. Please sign in again.");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return PhotosLoginResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;
    if is_verified(&session) {
        return ok_result(&session);
    }
    let now = now_millis();

    let last_probe = session.session_data(PHOTOS_ACCESS_PROBE_AT).and_then(|value| value.parse::<i64>().ok());
    if let Some(last) = last_probe {
        if now - last < photos_api::ACCESS_PROBE_INTERVAL_MS {
            return PhotosLoginResult::error(WAIT_2FA_MESSAGE);
        }
    }
    session.set_session_data(PHOTOS_ACCESS_PROBE_AT, now.to_string());
    match photos_accessible(&mut session).await {
        Ok(true) => {
            session.set_session_data(PHOTOS_VERIFIED, FLAG_TRUE);
            log::info!("[PHOTOS] session is authorized; CloudKit Photos API is available");
            ok_result(&session)
        }
        Ok(false) => PhotosLoginResult::error(WAIT_2FA_MESSAGE),
        Err(error) => PhotosLoginResult::error(error.to_string()),
    }
}

async fn photos_accessible(session: &mut SrpSession) -> AppResult<bool> {
    let Some(base) = cloud_kit_base(session) else {
        return Ok(false);
    };
    let mut url = format!("{base}{}{}", api::cloud_kit::ZONES_LIST, api::cloud_kit::QUERY_PARAMS);
    if let Some(dsid) = session.dsid() {
        url.push_str(api::cloud_kit::PARAM_DSID);
        url.push_str(dsid);
    }
    let response = session.request(Method::GET, &url, None, &[]).await?;
    log::info!("[PHOTOS] authorization probe zones/list: HTTP {}", response.status);
    Ok(response.status == 200)
}

pub async fn options_2fa(manager: &SessionManager, session_id: &str) -> Photos2faOptions {
    if session_id.trim().is_empty() {
        return Photos2faOptions::error("Missing session ID. Please sign in again.");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return Photos2faOptions::error(error.to_string()),
    };
    let mut session = handle.lock().await;

    let url = format!("{}{}", session.auth_base(), api::auth::BOOTSTRAP);
    let owned = session.auth_headers();
    let refs = header_refs(&owned);
    let auth_response = match session.request(Method::GET, &url, None, &refs).await {
        Ok(response) => response,
        Err(error) => return Photos2faOptions::error(error.to_string()),
    };
    log::info!(
        "[PHOTOS] /auth probe: HTTP {} body={}",
        auth_response.status,
        auth_service::truncate(&auth_response.body, photos_api::TRUNCATE_AUTH_PROBE)
    );
    let mut data = if auth_response.status == 200 { parse_probe(&auth_response.body) } else { None };

    if data.as_ref().map(|probe| probe.phones().is_empty()).unwrap_or(true) {
        let url = format!("{}{}", session.auth_base(), api::auth::VERIFY_PHONE);
        let owned = session.auth_headers();
        let refs = header_refs(&owned);
        let phone_response = match session.request(Method::GET, &url, None, &refs).await {
            Ok(response) => response,
            Err(error) => return Photos2faOptions::error(error.to_string()),
        };
        log::info!(
            "[PHOTOS] /verify/phone probe: HTTP {} body={}",
            phone_response.status,
            auth_service::truncate(&phone_response.body, photos_api::TRUNCATE_DEFAULT)
        );
        if phone_response.status == 200 {
            data = parse_probe(&phone_response.body);
        }
    }

    let Some(data) = data else {
        return Photos2faOptions {
            status: statuses::OK.to_string(),
            message: Some(
                "Could not detect two-factor authentication methods (no valid response from the auth service)"
                    .to_string(),
            ),
            trusted_device: true,
            trusted_device_count: -1,
            phones: Vec::new(),
        };
    };

    let mut phones = Vec::new();
    for phone in data.phones() {
        let id = phone.effective_id();

        if id > 0 {
            if let Some(label) = phone.label() {
                phones.push(PhoneOption { id, label });
            }
        }
    }
    let device_count = data.device_count().unwrap_or(-1);
    log::info!("[PHOTOS] 2FA probe: authType={:?} devices={} phones={}", data.auth_type(), device_count, phones.len());
    if device_count < 0 && phones.is_empty() {
        let raw = serde_json::to_string(&data).unwrap_or_default();
        return Photos2faOptions {
            status: statuses::OK.to_string(),
            message: Some(format!(
                "Authentication response is missing fields: {}",
                auth_service::truncate(&raw, photos_api::TRUNCATE_DEFAULT)
            )),
            trusted_device: true,
            trusted_device_count: device_count,
            phones,
        };
    }
    Photos2faOptions::ok(device_count, phones)
}

pub async fn send_phone_code(manager: &SessionManager, session_id: &str, phone_number_id: i64) -> PhotosLoginResult {
    if session_id.trim().is_empty() {
        return PhotosLoginResult::error("Missing session ID or phone number");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return PhotosLoginResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;

    let url = format!(
        "{}{}{}{}",
        session.auth_base(),
        api::auth::VERIFY_PHONE_CODE,
        api::auth::PARAM_PHONE_NUMBER_ID,
        phone_number_id
    );
    let owned = session.auth_headers();
    let refs = header_refs(&owned);
    let mut response = match session.request(Method::PUT, &url, None, &refs).await {
        Ok(response) => response,
        Err(error) => return PhotosLoginResult::error(error.to_string()),
    };

    if response.status == 404 || response.status == 405 {
        let url = format!("{}{}", session.auth_base(), api::auth::VERIFY_PHONE);
        let body = json_object(vec![(photos_api::PHONE_NUMBER_ID, json!(phone_number_id))]);
        let owned = session.auth_headers();
        let refs = header_refs(&owned);
        response = match session.request(Method::PUT, &url, Some(&body), &refs).await {
            Ok(response) => response,
            Err(error) => return PhotosLoginResult::error(error.to_string()),
        };
    }

    if response.status != 200 && response.status != 204 {
        let detail = extract_verify_error(&response.body);
        return PhotosLoginResult::error(format!("Could not send SMS: HTTP {} {}", response.status, detail));
    }
    log::info!("[PHOTOS] SMS verification code sent (phoneNumberId={phone_number_id})");
    PhotosLoginResult {
        status: statuses::OK.to_string(),
        message: Some("SMS verification code sent".to_string()),
        session_id: Some(session.id().to_string()),
        auth_type: None,
        first_name: None,
        last_name: None,
    }
}

pub async fn verify_phone_2fa(
    manager: &SessionManager,
    session_id: &str,
    phone_number_id: i64,
    code: &str,
) -> PhotosLoginResult {
    if session_id.trim().is_empty() {
        return PhotosLoginResult::error("Missing session ID or phone number");
    }
    let trimmed = strip_whitespace(code);
    if !is_phone_code(&trimmed) {
        return PhotosLoginResult::error("Enter the verification code from the SMS message");
    }
    let handle = match manager.get(session_id).await {
        Ok(handle) => handle,
        Err(error) => return PhotosLoginResult::error(error.to_string()),
    };
    let mut session = handle.lock().await;

    let mut headers = session.auth_headers();
    headers.push((hh::CONTENT_TYPE, hc::CONTENT_TYPE_TEXT_PLAIN.to_string()));
    let body =
        json_object(vec![(photos_api::SECURITY_CODE, json_object(vec![(photos_api::CODE, Value::String(trimmed))]))]);
    let url = format!(
        "{}{}{}{}",
        session.auth_base(),
        api::auth::VERIFY_PHONE_CODE,
        api::auth::PARAM_PHONE_NUMBER_ID,
        phone_number_id
    );
    let refs = header_refs(&headers);
    let response = match session.request(Method::POST, &url, Some(&body), &refs).await {
        Ok(response) => response,
        Err(error) => return PhotosLoginResult::error(error.to_string()),
    };
    if response.status != 204 && response.status != 200 {
        return PhotosLoginResult::error(extract_verify_error(&response.body));
    }

    if let Err(error) = devices::account_login(&mut session, false).await {
        return PhotosLoginResult::error(error.to_string());
    }
    session.set_session_data(PHOTOS_VERIFIED, FLAG_TRUE);
    log::info!(
        "[PHOTOS] SMS 2FA verification succeeded (dsid={:?}, photos={:?})",
        session.dsid(),
        session.photos_url()
    );
    ok_result(&session)
}

pub async fn list(
    manager: &SessionManager,
    session_id: &str,
    cursor: Option<String>,
    limit: Option<i32>,
    start_rank: Option<i64>,
) -> PhotosPage {
    if session_id.trim().is_empty() {
        return PhotosPage::error("Missing session ID. Please sign in again.");
    }
    let page_size = limit.unwrap_or(photos_api::DEFAULT_PAGE_SIZE).clamp(1, photos_api::MAX_PAGE_SIZE);
    let rank = start_rank.filter(|value| *value >= 0);

    let context = match prepare_photo_read(manager, session_id, rank.is_some()).await {
        Ok(context) => context,
        Err(error) => {
            log::warn!("[PHOTOS] query failed: {error}");
            return PhotosPage::error(error.to_string());
        }
    };

    if let Some(rank) = rank {
        return rank_page_context(&context, rank, page_size).await;
    }

    let mut last_error = None;
    for record_type in ck::TIMELINE_RECORD_TYPES {
        let body = build_query_body(record_type, cursor.as_deref(), page_size);

        let headers = [(hh::CONTENT_TYPE, hc::CONTENT_TYPE_TEXT_PLAIN)];
        let response =
            match context.session.request_shared(Method::POST, &context.query_url, Some(&body), &headers).await {
                Ok(response) => response,
                Err(error) => {
                    log::warn!("[PHOTOS] query failed: {error}");
                    return PhotosPage::error(error.to_string());
                }
            };
        log::info!("[PHOTOS] records/query ({}) HTTP {} length={}", record_type, response.status, response.body.len());
        if response.status == 200 {
            return parse_records(&response.body);
        }
        last_error = Some(format!(
            "HTTP {} {}",
            response.status,
            auth_service::truncate(&response.body, photos_api::TRUNCATE_ERROR)
        ));
        log::warn!("[PHOTOS] recordType={} query failed: {}", record_type, last_error.as_deref().unwrap_or(""));
    }
    PhotosPage::error(format!("Photo query failed: {}", last_error.unwrap_or_default()))
}

struct PhotoReadContext {
    session: SrpSession,
    query_url: String,
    zone_owner: Option<String>,
}

async fn prepare_photo_read(
    manager: &SessionManager,
    session_id: &str,
    need_zone_owner: bool,
) -> AppResult<PhotoReadContext> {
    let handle = manager.get(session_id).await?;
    let mut session = handle.lock().await;
    if !is_verified(&session) {
        return Err(AppError::auth(
            "Two-factor authentication is not complete. Enter the code shown on your trusted device.",
        ));
    }
    devices::account_login(&mut session, false).await?;
    let query_url = build_records_query_url(&session)?;
    let zone_owner = if need_zone_owner { zone_owner_cached(&mut session).await } else { None };
    let mut snapshot = session.clone();

    snapshot.clear_password();
    Ok(PhotoReadContext { session: snapshot, query_url, zone_owner })
}

pub async fn library_total(manager: &SessionManager, session_id: &str) -> AppResult<LibraryTotal> {
    if session_id.trim().is_empty() {
        return Err(AppError::auth("Missing session ID. Please sign in again."));
    }
    let context = prepare_photo_read(manager, session_id, true).await?;
    let body = lookup_record_context(&context, ck::RECORD_NAME_LIBRARY_INFO).await.inspect_err(|error| {
        log::warn!("[PHOTOS] library total query failed: {error}");
    })?;
    parse_library_total(&body)
}

async fn lookup_record_context(context: &PhotoReadContext, record_name: &str) -> AppResult<String> {
    let url = build_lookup_url(&context.session)?;
    let body = build_lookup_body(record_name, context.zone_owner.as_deref());

    let headers = [(hh::CONTENT_TYPE, hc::CONTENT_TYPE_TEXT_PLAIN)];
    let response = context.session.request_shared(Method::POST, &url, Some(&body), &headers).await?;
    log::info!("[PHOTOS] records/lookup ({record_name}) HTTP {}", response.status);
    if response.status != 200 {
        return Err(AppError::auth(format!(
            "index record query failed: HTTP {} {}",
            response.status,
            auth_service::truncate(&response.body, photos_api::TRUNCATE_ERROR)
        )));
    }
    Ok(response.body)
}

fn build_lookup_url(session: &SrpSession) -> AppResult<String> {
    let Some(base) = cloud_kit_base(session) else {
        return Err(AppError::auth(
            "iCloud Photos is not enabled for this account or no CloudKit service URL was returned",
        ));
    };
    let dsid = session.dsid().unwrap_or("null");
    Ok(format!(
        "{base}{lookup}{params}{param_dsid}{dsid}",
        lookup = api::cloud_kit::RECORDS_LOOKUP,
        params = api::cloud_kit::QUERY_PARAMS,
        param_dsid = api::cloud_kit::PARAM_DSID,
    ))
}

async fn ensure_zone_owner(session: &mut SrpSession) -> Option<String> {
    let base = cloud_kit_base(session)?;
    let mut url = format!("{base}{}{}", api::cloud_kit::ZONES_LIST, api::cloud_kit::QUERY_PARAMS);
    if let Some(dsid) = session.dsid() {
        url.push_str(api::cloud_kit::PARAM_DSID);
        url.push_str(dsid);
    }
    let response = match session.request(Method::GET, &url, None, &[]).await {
        Ok(response) => response,
        Err(error) => {
            log::warn!("[PHOTOS] zones/list probe failed; continuing without zone owner: {error}");
            return None;
        }
    };
    if response.status != 200 {
        log::warn!("[PHOTOS] zones/list probe returned HTTP {}; continuing without zone owner", response.status);
        return None;
    }
    let owner = zone_owner(&response.body)?;
    session.set_session_data(PHOTOS_ZONE_OWNER, owner.clone());
    Some(owner)
}

fn zone_owner(body: &str) -> Option<String> {
    let root: Value = serde_json::from_str(body).ok()?;
    for zone in field(&root, ck::ZONES).as_array()? {
        let zone_id = field(zone, ckq::ZONE_ID);
        if field(zone_id, ckq::ZONE_NAME).as_str() != Some(api::cloud_kit::ZONE) {
            continue;
        }
        if let Some(owner) = non_blank(as_text(field(zone_id, ck::FIELD_OWNER_RECORD_NAME))) {
            return Some(owner);
        }
    }
    None
}

fn build_lookup_body(record_name: &str, zone_owner: Option<&str>) -> Value {
    let record = json_object(vec![(ck::FIELD_RECORD_NAME, Value::String(record_name.to_string()))]);
    json_object(vec![(ck::RECORDS, Value::Array(vec![record])), (ckq::ZONE_ID, build_zone_id(zone_owner))])
}

fn build_zone_id(zone_owner: Option<&str>) -> Value {
    let mut entries = vec![
        (ckq::ZONE_NAME, Value::String(api::cloud_kit::ZONE.to_string())),
        (ckq::ZONE_TYPE, Value::String(ckq::ZONE_TYPE_REGULAR_CUSTOM.to_string())),
    ];
    if let Some(owner) = zone_owner.map(str::trim).filter(|value| !value.is_empty()) {
        entries.push((ck::FIELD_OWNER_RECORD_NAME, Value::String(owner.to_string())));
    }
    json_object(entries)
}

fn parse_library_total(body: &str) -> AppResult<LibraryTotal> {
    if let Some(message) = cloud_kit_zone_error(body) {
        return Err(AppError::auth(message));
    }
    let root: Value = serde_json::from_str(body)
        .map_err(|error| AppError::auth(format!("Library count response is not valid JSON: {error}")))?;
    let Some(records) = field(&root, ck::RECORDS).as_array() else {
        return Err(AppError::auth(format!(
            "library total response is missing records: {}",
            auth_service::truncate(body, photos_api::TRUNCATE_DEFAULT)
        )));
    };
    let record = records.iter().find(|record| {
        record_name(record).as_deref() == Some(ck::RECORD_NAME_LIBRARY_INFO)
            || field(record, ck::FIELD_RECORD_TYPE).as_str() == Some(ck::RECORD_TYPE_LIBRARY_INFO)
    });
    let Some(record) = record else {
        return Err(AppError::auth(format!(
            "library total response has no {} record: {}",
            ck::RECORD_TYPE_LIBRARY_INFO,
            auth_service::truncate(body, photos_api::TRUNCATE_DEFAULT)
        )));
    };
    let fields = field(record, ck::FIELD_FIELDS);
    let photos = long_value(fields, ck::FIELD_PHOTOS_COUNT);
    let videos = long_value(fields, ck::FIELD_VIDEOS_COUNT);
    Ok(LibraryTotal { photos, videos, assets: photos + videos })
}

pub const DEFAULT_TZ_OFFSET_SECONDS: i32 = 28_800;

const MS_PER_DAY: i64 = 86_400_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetDistribution {
    pub bucket_width_ms: i64,

    pub total: i64,

    pub buckets: Vec<AssetBucket>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetBucket {
    pub start_ms: i64,

    pub count: i64,

    pub start_rank: i64,

    pub second_month_rank: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeline {
    pub years: Vec<YearEntry>,

    pub total: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YearEntry {
    pub year: i32,

    pub count: i64,

    pub months: Vec<MonthEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthEntry {
    pub year: i32,

    pub month: u32,

    pub count: i64,

    pub start_rank: i64,
}

pub async fn asset_distribution(manager: &SessionManager, session_id: &str) -> AppResult<AssetDistribution> {
    if session_id.trim().is_empty() {
        return Err(AppError::auth("Missing session ID. Please sign in again."));
    }
    let context = prepare_photo_read(manager, session_id, true).await?;
    let parsed = asset_distribution_context(&context).await?;

    if let Some(hint) = parsed.library_total_hint {
        if hint != parsed.distribution.total {
            log::warn!(
                "[PHOTOS] timeline histogram and index count differ (histogram {} / index {}); using histogram",
                parsed.distribution.total,
                hint
            );
        }
    }
    Ok(parsed.distribution)
}

async fn asset_distribution_context(context: &PhotoReadContext) -> AppResult<ParsedDistribution> {
    let body = lookup_record_context(context, ck::RECORD_NAME_ASSET_DISTRIBUTION).await?;
    parse_asset_distribution(&body)
}

pub async fn timeline(
    manager: &SessionManager,
    session_id: &str,
    tz_offset_seconds: i32,
) -> AppResult<PhotosTimelineDto> {
    let distribution = asset_distribution(manager, session_id).await?;
    let timeline = timeline_from_distribution(&distribution, tz_offset_seconds);
    Ok(PhotosTimelineDto { distribution: (&distribution).into(), timeline: (&timeline).into() })
}

struct ParsedDistribution {
    distribution: AssetDistribution,

    library_total_hint: Option<i64>,
}

fn parse_asset_distribution(body: &str) -> AppResult<ParsedDistribution> {
    if let Some(message) = cloud_kit_zone_error(body) {
        return Err(AppError::other(message));
    }
    let root: Value = serde_json::from_str(body)
        .map_err(|error| AppError::other(format!("Timeline response is not valid JSON: {error}")))?;
    let Some(records) = field(&root, ck::RECORDS).as_array() else {
        return Err(AppError::other(format!(
            "timeline response is missing records: {}",
            auth_service::truncate(body, photos_api::TRUNCATE_DEFAULT)
        )));
    };

    let record = records
        .iter()
        .find(|record| record_name(record).as_deref() == Some(ck::RECORD_NAME_ASSET_DISTRIBUTION))
        .or_else(|| {
            records.iter().find(|record| text(field(record, ck::FIELD_FIELDS), ck::FIELD_ASSET_DISTRIBUTION).is_some())
        });
    let Some(record) = record else {
        return Err(AppError::other(format!(
            "timeline response has no {} record: {}",
            ck::RECORD_NAME_ASSET_DISTRIBUTION,
            auth_service::truncate(body, photos_api::TRUNCATE_DEFAULT)
        )));
    };
    let fields = field(record, ck::FIELD_FIELDS);
    let encoded = non_blank(text(fields, ck::FIELD_ASSET_DISTRIBUTION)).ok_or_else(|| {
        AppError::other(format!("Timeline response is missing the {} field", ck::FIELD_ASSET_DISTRIBUTION))
    })?;

    let bytes = BASE64
        .decode(strip_whitespace(&encoded).as_bytes())
        .map_err(|error| AppError::other(format!("Could not decode the timeline base64 payload: {error}")))?;
    let distribution = parse_asset_distribution_bytes(&bytes)?;

    let counts_present =
        !field(fields, ck::FIELD_PHOTOS_COUNT).is_null() || !field(fields, ck::FIELD_VIDEOS_COUNT).is_null();
    let library_total_hint =
        counts_present.then(|| long_value(fields, ck::FIELD_PHOTOS_COUNT) + long_value(fields, ck::FIELD_VIDEOS_COUNT));
    Ok(ParsedDistribution { distribution, library_total_hint })
}

fn parse_asset_distribution_bytes(bytes: &[u8]) -> AppResult<AssetDistribution> {
    let mut reader = ProtoReader::new(bytes);
    let mut bucket_width_ms = 0i64;
    let mut buckets: Vec<AssetBucket> = Vec::new();
    while !reader.is_done() {
        let tag = reader.varint()?;
        let field_number = tag >> 3;
        let wire_type = tag & 0b111;
        match (field_number, wire_type) {
            (pbd::FIELD_BUCKET_WIDTH, pbd::WIRE_VARINT) => {
                bucket_width_ms = as_i64(reader.varint()?, "bucket width")?;
            }
            (pbd::FIELD_BUCKET, pbd::WIRE_LENGTH_DELIMITED) => {
                let length = reader.varint()?;
                let body = reader.take(length)?;
                buckets.push(parse_bucket_bytes(body)?);
            }
            _ => reader.skip(wire_type)?,
        }
    }

    buckets.sort_by_key(|bucket| bucket.start_ms);

    let mut total = 0i64;
    for bucket in &buckets {
        total =
            total.checked_add(bucket.count).ok_or_else(|| AppError::other("Timeline photo count overflowed i64"))?;
    }

    let mut older_count = 0i64;

    if total > 0 {
        for bucket in &mut buckets {
            bucket.start_rank = total - 1 - older_count;

            bucket.second_month_rank = (bucket.count > 0).then(|| total - older_count - bucket.count);

            older_count = older_count
                .checked_add(bucket.count)
                .ok_or_else(|| AppError::other("Timeline photo count overflowed i64"))?;
        }
    }
    Ok(AssetDistribution { bucket_width_ms, total, buckets })
}

fn parse_bucket_bytes(bytes: &[u8]) -> AppResult<AssetBucket> {
    let mut reader = ProtoReader::new(bytes);
    let mut start_ms = 0i64;
    let mut count = 0i64;
    while !reader.is_done() {
        let tag = reader.varint()?;
        let field_number = tag >> 3;
        let wire_type = tag & 0b111;
        match (field_number, wire_type) {
            (pbd::FIELD_BUCKET_START, pbd::WIRE_VARINT) => start_ms = as_i64(reader.varint()?, "bucket start time")?,
            (pbd::FIELD_BUCKET_COUNT, pbd::WIRE_VARINT) => count = as_i64(reader.varint()?, "bucket count")?,
            _ => reader.skip(wire_type)?,
        }
    }

    Ok(AssetBucket { start_ms, count, start_rank: 0, second_month_rank: None })
}

fn as_i64(value: u64, what: &str) -> AppResult<i64> {
    i64::try_from(value).map_err(|_| AppError::other(format!("Timeline {what} is outside the i64 range")))
}

struct ProtoReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> ProtoReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn is_done(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn varint(&mut self) -> AppResult<u64> {
        let mut value: u64 = 0;
        let mut shift: u32 = 0;
        loop {
            if shift > 63 {
                return Err(AppError::other("protobuf: varint exceeds 10 bytes (malformed data)"));
            }
            let byte = *self
                .bytes
                .get(self.pos)
                .ok_or_else(|| AppError::other("protobuf: varint is truncated at the end of the data"))?;
            self.pos += 1;
            value |= u64::from(byte & 0x7F) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
        }
    }

    fn take(&mut self, length: u64) -> AppResult<&'a [u8]> {
        let length = usize::try_from(length)
            .map_err(|_| AppError::other("protobuf: length field exceeds the addressable range"))?;
        let end = self.pos.checked_add(length).ok_or_else(|| AppError::other("protobuf: length field overflow"))?;
        if end > self.bytes.len() {
            return Err(AppError::other(format!(
                "protobuf: expected {length} bytes but only {} remain (truncated data)",
                self.bytes.len() - self.pos
            )));
        }
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn skip(&mut self, wire_type: u64) -> AppResult<()> {
        match wire_type {
            pbd::WIRE_VARINT => {
                self.varint()?;
            }
            pbd::WIRE_FIXED64 => {
                self.take(8)?;
            }
            pbd::WIRE_LENGTH_DELIMITED => {
                let length = self.varint()?;
                self.take(length)?;
            }
            pbd::WIRE_FIXED32 => {
                self.take(4)?;
            }
            other => {
                return Err(AppError::other(format!(
                    "protobuf: wire type {other} cannot be skipped safely (groups are deprecated)"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct MonthAnchor {
    year: i32,
    month: u32,

    rank: i64,

    second_month: Option<(i32, u32, i64)>,
}

impl MonthAnchor {
    fn key(&self) -> (i32, u32) {
        (self.year, self.month)
    }

    fn second_key(&self) -> Option<(i32, u32)> {
        self.second_month.map(|(year, month, _)| (year, month))
    }
}

pub fn timeline_from_distribution(dist: &AssetDistribution, tz_offset_seconds: i32) -> Timeline {
    let tz_offset_ms = i64::from(tz_offset_seconds) * 1_000;

    let mut counts: BTreeMap<(i32, u32), i64> = BTreeMap::new();

    let mut anchors: Vec<MonthAnchor> = Vec::new();
    for bucket in &dist.buckets {
        let Some(second_rank) = bucket.second_month_rank else { continue };
        let start_month = local_year_month(bucket.start_ms, tz_offset_ms);
        *counts.entry(start_month).or_insert(0) += bucket.count;

        let second_month = (dist.bucket_width_ms > 0)
            .then(|| local_year_month(bucket.start_ms.saturating_add(dist.bucket_width_ms - 1), tz_offset_ms))
            .filter(|month| *month != start_month)
            .map(|(year, month)| (year, month, second_rank));

        anchors.push(MonthAnchor { year: start_month.0, month: start_month.1, rank: bucket.start_rank, second_month });
    }

    let mut kept: Vec<MonthAnchor> = Vec::with_capacity(anchors.len());
    for (index, anchor) in anchors.iter().enumerate() {
        let bridged = anchor.second_key().is_some_and(|second| {
            let previous_same_month = index
                .checked_sub(1)
                .and_then(|previous| anchors.get(previous))
                .is_some_and(|previous| previous.key() == anchor.key());
            let next_is_second_month = anchors.get(index + 1).is_some_and(|next| next.key() == second);
            previous_same_month && next_is_second_month
        });
        if !bridged {
            kept.push(*anchor);
        }
    }

    let mut seen: BTreeSet<(i32, u32)> = BTreeSet::new();
    let mut months: Vec<MonthEntry> = Vec::new();
    let month_count = |key: (i32, u32)| counts.get(&key).copied().unwrap_or(0);
    for anchor in kept {
        if seen.insert(anchor.key()) {
            months.push(MonthEntry {
                year: anchor.year,
                month: anchor.month,
                count: month_count(anchor.key()),
                start_rank: anchor.rank,
            });
        }
        if let Some((year, month, rank)) = anchor.second_month {
            if seen.insert((year, month)) {
                months.push(MonthEntry { year, month, count: month_count((year, month)), start_rank: rank });
            }
        }
    }

    months.sort_by_key(|month| (month.year, month.month));

    let mut years: Vec<YearEntry> = Vec::new();
    let mut total = 0i64;
    for month_entry in months {
        total += month_entry.count;
        match years.last_mut() {
            Some(last) if last.year == month_entry.year => {
                last.count += month_entry.count;
                last.months.push(month_entry);
            }
            _ => {
                let year = month_entry.year;
                let count = month_entry.count;
                years.push(YearEntry { year, count, months: vec![month_entry] });
            }
        }
    }
    Timeline { years, total }
}

fn local_year_month(start_ms: i64, tz_offset_ms: i64) -> (i32, u32) {
    let days = start_ms.saturating_add(tz_offset_ms).div_euclid(MS_PER_DAY);
    let (year, month, _day) = civil_from_days(days);
    (year, month)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthExact {
    pub year: i32,

    pub month: u32,

    pub count: i64,

    pub newest_rank: i64,

    pub oldest_rank: i64,
}

const MAX_MONTH_EXACT_PAGES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
struct BoundaryBucket {
    start_rank: i64,

    count: i64,

    requests: Vec<(i64, i32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MonthPlan {
    year: i32,
    month: u32,

    month_start_ms: i64,

    month_end_ms: i64,

    inside_count: i64,

    inside_newest_rank: Option<i64>,

    inside_oldest_rank: Option<i64>,

    boundaries: Vec<BoundaryBucket>,
}

impl MonthPlan {
    fn pages(&self) -> usize {
        self.boundaries.iter().map(|bucket| bucket.requests.len()).sum()
    }
}

pub async fn month_exact(
    manager: &SessionManager,
    session_id: &str,
    year: i32,
    month: u32,
    tz_offset_seconds: i32,
) -> AppResult<MonthExact> {
    if session_id.trim().is_empty() {
        return Err(AppError::auth("Missing session ID. Please sign in again."));
    }

    let context = prepare_photo_read(manager, session_id, true).await?;
    let parsed = asset_distribution_context(&context).await?;
    let plan = plan_month(&parsed.distribution, tz_offset_seconds, year, month)?;

    let mut requests = Vec::with_capacity(plan.pages());
    for (boundary_index, boundary) in plan.boundaries.iter().enumerate() {
        for (page_index, (rank, limit)) in boundary.requests.iter().copied().enumerate() {
            requests.push((boundary_index, page_index, rank, limit));
        }
    }
    let tasks = requests.into_iter().map(|(boundary_index, page_index, rank, limit)| {
        let context = &context;
        async move { (boundary_index, page_index, rank_page_dates_context(context, rank, limit).await) }
    });
    let results: Vec<_> = stream::iter(tasks).buffer_unordered(photos_api::PHOTO_REQUEST_CONCURRENCY).collect().await;

    let mut slots: Vec<Vec<Option<Vec<i64>>>> =
        plan.boundaries.iter().map(|boundary| (0..boundary.requests.len()).map(|_| None).collect()).collect();
    for (boundary_index, page_index, dates) in results {
        slots[boundary_index][page_index] = Some(dates?);
    }
    let fetched: Vec<Vec<i64>> =
        slots.into_iter().map(|pages| pages.into_iter().flatten().flatten().collect()).collect();

    let pages = plan.pages();
    let exact = finish_month(&plan, &fetched)?;
    log::info!(
        "[PHOTOS] exact monthly count: {} {} = {} photos ({} concurrent boundary pages)",
        exact.year,
        exact.month,
        exact.count,
        pages
    );
    Ok(exact)
}

fn month_window(year: i32, month: u32, tz_offset_ms: i64) -> AppResult<(i64, i64)> {
    if !(1..=12).contains(&month) {
        return Err(AppError::other(format!("Invalid month: {month} (expected 1-12)")));
    }

    if !(1000..=9999).contains(&year) {
        return Err(AppError::other(format!("Invalid year: {year}")));
    }
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let start_days = days_from_civil(year, month, 1);
    let end_days = days_from_civil(next_year, next_month, 1);
    Ok((start_days * MS_PER_DAY - tz_offset_ms, end_days * MS_PER_DAY - tz_offset_ms))
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

fn pages_for(count: i64) -> usize {
    if count <= 0 {
        return 0;
    }
    let per_page = i64::from(photos_api::DEFAULT_PAGE_SIZE);

    (count as u64).div_ceil(per_page as u64) as usize
}

fn boundary_page_plan(start_rank: i64, count: i64) -> Vec<(i64, i32)> {
    let per_page = i64::from(photos_api::DEFAULT_PAGE_SIZE);
    let mut requests = Vec::with_capacity(pages_for(count));
    let mut fetched = 0i64;
    while fetched < count {
        let limit = (count - fetched).min(per_page) as i32;
        requests.push((start_rank - fetched, limit));
        fetched += i64::from(limit);
    }
    requests
}

fn plan_month(dist: &AssetDistribution, tz_offset_seconds: i32, year: i32, month: u32) -> AppResult<MonthPlan> {
    let tz_offset_ms = i64::from(tz_offset_seconds) * 1_000;
    let (month_start_ms, month_end_ms) = month_window(year, month, tz_offset_ms)?;

    if dist.bucket_width_ms <= 0 {
        return Err(AppError::other("Timeline is missing its bucket width; exact monthly count failed"));
    }
    let width = dist.bucket_width_ms;
    let target = (year, month);

    let mut plan = MonthPlan {
        year,
        month,
        month_start_ms,
        month_end_ms,
        inside_count: 0,
        inside_newest_rank: None,
        inside_oldest_rank: None,
        boundaries: Vec::new(),
    };

    for bucket in &dist.buckets {
        if bucket.count <= 0 {
            continue;
        }
        let start_ms = bucket.start_ms;

        let end_ms = start_ms.saturating_add(width - 1);

        if end_ms < month_start_ms || start_ms >= month_end_ms {
            continue;
        }
        let start_month = local_year_month(start_ms, tz_offset_ms);
        let end_month = local_year_month(end_ms, tz_offset_ms);
        if start_month == target && end_month == target {
            plan.inside_count = plan
                .inside_count
                .checked_add(bucket.count)
                .ok_or_else(|| AppError::other("Exact monthly count overflowed i64"))?;

            let newest = bucket.second_month_rank.unwrap_or(bucket.start_rank);
            plan.inside_newest_rank = Some(plan.inside_newest_rank.map_or(newest, |current| current.min(newest)));
            plan.inside_oldest_rank =
                Some(plan.inside_oldest_rank.map_or(bucket.start_rank, |current| current.max(bucket.start_rank)));
            continue;
        }

        plan.boundaries.push(BoundaryBucket {
            start_rank: bucket.start_rank,
            count: bucket.count,
            requests: Vec::new(),
        });
    }

    let pages: usize = plan.boundaries.iter().map(|bucket| pages_for(bucket.count)).sum();
    if pages > MAX_MONTH_EXACT_PAGES {
        return Err(AppError::other(format!(
            "exact monthly count needs {pages} pages, exceeding the limit of {MAX_MONTH_EXACT_PAGES} (invalid histogram data)"
        )));
    }
    for boundary in &mut plan.boundaries {
        boundary.requests = boundary_page_plan(boundary.start_rank, boundary.count);
    }
    Ok(plan)
}

fn finish_month(plan: &MonthPlan, fetched: &[Vec<i64>]) -> AppResult<MonthExact> {
    if fetched.len() != plan.boundaries.len() {
        return Err(AppError::other("Boundary-week data did not match the plan"));
    }
    let mut count = plan.inside_count;
    let mut newest = plan.inside_newest_rank;
    let mut oldest = plan.inside_oldest_rank;
    for (boundary, dates) in plan.boundaries.iter().zip(fetched) {
        if dates.len() as i64 != boundary.count {
            return Err(AppError::other(format!(
                "boundary-week histogram count ({}) does not match fetched count ({})",
                boundary.count,
                dates.len()
            )));
        }
        for (index, taken_at) in dates.iter().enumerate() {
            let rank = boundary.start_rank - index as i64;

            if *taken_at < plan.month_start_ms || *taken_at >= plan.month_end_ms {
                continue;
            }
            count = count.checked_add(1).ok_or_else(|| AppError::other("Exact monthly count overflowed i64"))?;
            newest = Some(newest.map_or(rank, |current| current.min(rank)));
            oldest = Some(oldest.map_or(rank, |current| current.max(rank)));
        }
    }
    Ok(MonthExact {
        year: plan.year,
        month: plan.month,
        count,

        newest_rank: newest.unwrap_or(0),
        oldest_rank: oldest.unwrap_or(0),
    })
}

async fn rank_page_dates_context(context: &PhotoReadContext, start_rank: i64, page_size: i32) -> AppResult<Vec<i64>> {
    let page_size = page_size.clamp(1, photos_api::MAX_PAGE_SIZE);
    let page = rank_page_context(context, start_rank, page_size).await;
    if page.status != statuses::OK {
        return Err(AppError::other(page.message.unwrap_or_else(|| "Could not load photos by rank".to_string())));
    }
    let mut dates = Vec::with_capacity(page.photos.len());
    for photo in &page.photos {
        let taken_at = photo
            .taken_at
            .ok_or_else(|| AppError::other("Photo is missing its capture date; month cannot be determined"))?;
        dates.push(taken_at);
    }
    Ok(dates)
}

impl From<&MonthExact> for MonthExactDto {
    fn from(exact: &MonthExact) -> Self {
        Self {
            year: exact.year,
            month: exact.month,
            count: exact.count,
            newest_rank: exact.newest_rank,
            oldest_rank: exact.oldest_rank,
        }
    }
}

impl From<&AssetDistribution> for AssetDistributionDto {
    fn from(dist: &AssetDistribution) -> Self {
        Self {
            bucket_width_ms: dist.bucket_width_ms,
            total: dist.total,
            buckets: dist.buckets.iter().map(AssetBucketDto::from).collect(),
        }
    }
}

impl From<&AssetBucket> for AssetBucketDto {
    fn from(bucket: &AssetBucket) -> Self {
        Self {
            start_ms: bucket.start_ms,
            count: bucket.count,
            start_rank: bucket.start_rank,
            second_month_rank: bucket.second_month_rank,
        }
    }
}

impl From<&Timeline> for TimelineDto {
    fn from(timeline: &Timeline) -> Self {
        Self { total: timeline.total, years: timeline.years.iter().map(YearEntryDto::from).collect() }
    }
}

impl From<&YearEntry> for YearEntryDto {
    fn from(year: &YearEntry) -> Self {
        Self { year: year.year, count: year.count, months: year.months.iter().map(MonthEntryDto::from).collect() }
    }
}

impl From<&MonthEntry> for MonthEntryDto {
    fn from(month: &MonthEntry) -> Self {
        Self { year: month.year, month: month.month, count: month.count, start_rank: month.start_rank }
    }
}

fn rank_filter(field_name: &str, value_type: &str, value: Value) -> Value {
    json_object(vec![
        (ckq::FIELD_NAME, Value::String(field_name.to_string())),
        (ckq::COMPARATOR, Value::String(ck::rank::COMPARATOR_EQUALS.to_string())),
        (
            ckq::FIELD_VALUE,
            json_object(vec![(ckq::VALUE_TYPE, Value::String(value_type.to_string())), (ckq::VALUE, value)]),
        ),
    ])
}

fn build_rank_page_body(start_rank: i64, results_limit: i32, zone_owner: Option<&str>) -> Value {
    let query = json_object(vec![
        (ckq::RECORD_TYPE, Value::String(ck::rank::RECORD_TYPE.to_string())),
        (
            ckq::FILTER_BY,
            Value::Array(vec![
                rank_filter(
                    ck::rank::FIELD_DIRECTION,
                    ck::rank::TYPE_STRING,
                    Value::String(ck::rank::DIRECTION_DESCENDING.to_string()),
                ),
                rank_filter(ck::rank::FIELD_START_RANK, ckq::TYPE_INT64, json!(start_rank)),
            ]),
        ),
    ]);
    json_object(vec![
        (ckq::QUERY, query),
        (ckq::ZONE_ID, build_zone_id(zone_owner)),
        (ckq::RESULTS_LIMIT, json!(results_limit)),
    ])
}

async fn zone_owner_cached(session: &mut SrpSession) -> Option<String> {
    match session.session_data(PHOTOS_ZONE_OWNER) {
        Some(owner) if !owner.trim().is_empty() => Some(owner.to_string()),
        _ => ensure_zone_owner(session).await,
    }
}

async fn rank_page_context(context: &PhotoReadContext, start_rank: i64, page_size: i32) -> PhotosPage {
    let body =
        build_rank_page_body(start_rank, page_size * photos_api::RECORDS_PER_PHOTO, context.zone_owner.as_deref());

    let headers = [(hh::CONTENT_TYPE, hc::CONTENT_TYPE_TEXT_PLAIN)];
    let response = match context.session.request_shared(Method::POST, &context.query_url, Some(&body), &headers).await {
        Ok(response) => response,
        Err(error) => return PhotosPage::error(error.to_string()),
    };
    log::info!("[PHOTOS] rank page: startRank={} limit={} -> HTTP {}", start_rank, page_size, response.status);
    if response.status != 200 {
        return PhotosPage::error(format!("Could not load photos by rank: HTTP {}", response.status));
    }
    parse_records(&response.body)
}

fn cloud_kit_base(session: &SrpSession) -> Option<String> {
    if let Some(url) = session.cloud_kit_url() {
        if !url.trim().is_empty() {
            return Some(url.trim_end_matches('/').to_string());
        }
    }
    if session.auth_base().contains(".cn") {
        Some(api::cloud_kit::BASE_CN.to_string())
    } else {
        Some(api::cloud_kit::BASE.to_string())
    }
}

fn build_records_query_url(session: &SrpSession) -> AppResult<String> {
    let Some(base) = cloud_kit_base(session) else {
        return Err(AppError::auth(
            "iCloud Photos is not enabled for this account or no CloudKit service URL was returned",
        ));
    };

    let dsid = session.dsid().unwrap_or("null");
    Ok(format!(
        "{base}{query}{params}{param_dsid}{dsid}",
        query = api::cloud_kit::RECORDS_QUERY,
        params = api::cloud_kit::QUERY_PARAMS,
        param_dsid = api::cloud_kit::PARAM_DSID,
    ))
}

fn date_filter(field_name: &str, comparator: &str, value: i64) -> Value {
    json_object(vec![
        (ckq::FIELD_NAME, Value::String(field_name.to_string())),
        (ckq::COMPARATOR, Value::String(comparator.to_string())),
        (
            ckq::FIELD_VALUE,
            json_object(vec![
                (ckq::VALUE_TYPE, Value::String(ckq::TYPE_TIMESTAMP.to_string())),
                (ckq::VALUE, json!(value)),
            ]),
        ),
    ])
}

fn web_client_baseline_filter() -> Value {
    date_filter(ckq::FILTER_START_DATE, ckq::COMPARATOR_GREATER_THAN, 0)
}

/// Builds a CloudKit query body.
fn build_query_body(record_type: &str, cursor: Option<&str>, page_size: i32) -> Value {
    let filters = vec![web_client_baseline_filter()];
    let sort = json_object(vec![
        (ckq::FIELD_NAME, Value::String(ck::FIELD_ASSET_DATE.to_string())),
        (ckq::ASCENDING, Value::Bool(false)),
    ]);
    let query = json_object(vec![
        (ckq::RECORD_TYPE, Value::String(record_type.to_string())),
        (ckq::FILTER_BY, Value::Array(filters)),
        (ckq::SORT_BY, Value::Array(vec![sort])),
    ]);
    let desired_keys = Value::Array(ck::DESIRED_KEYS.iter().map(|key| Value::String((*key).to_string())).collect());

    let mut body = json_object(vec![
        (ckq::QUERY, query),
        (ckq::ZONE_ID, json_object(vec![(ckq::ZONE_NAME, Value::String(api::cloud_kit::ZONE.to_string()))])),
        (ckq::DESIRED_KEYS, desired_keys),
        (
            ckq::RESULTS_LIMIT,
            json!(std::cmp::min(
                page_size * photos_api::RECORDS_PER_PHOTO,
                photos_api::MAX_PAGE_SIZE * photos_api::RECORDS_PER_PHOTO
            )),
        ),
    ]);

    if let Some(cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) {
        if let Value::Object(map) = &mut body {
            map.insert(ck::CONTINUATION_MARKER.to_string(), Value::String(cursor.to_string()));
        }
    }
    body
}

fn cloud_kit_zone_error(body: &str) -> Option<String> {
    if body.contains("ZONE_NOT_FOUND") || body.contains("Zone does not exist") {
        Some(
            "iCloud Photos is not enabled for this Apple account, or the library has not been created. Enable Sync this iPhone/iPad in Settings > Apple Account > iCloud > Photos, then sign in again."
                .to_string(),
        )
    } else {
        None
    }
}

fn parse_records(body: &str) -> PhotosPage {
    let root: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    parse_records_value(&root, body)
}

fn parse_records_value(root: &Value, raw: &str) -> PhotosPage {
    if let Some(message) = cloud_kit_zone_error(raw) {
        return PhotosPage::error(message);
    }
    let records = field(root, ck::RECORDS);
    let Value::Array(records) = records else {
        return PhotosPage::error(format!(
            "photo response is missing records: {}",
            auth_service::truncate(raw, photos_api::TRUNCATE_DEFAULT)
        ));
    };

    let mut masters: HashMap<String, &Value> = HashMap::new();
    for record in records {
        if field(record, ck::FIELD_RECORD_TYPE).as_str() != Some(ck::RECORD_TYPE_MASTER) {
            continue;
        }
        let Some(name) = record_name(record) else {
            continue;
        };
        let fields = field(record, ck::FIELD_FIELDS);

        let short_name = last_segment(&name);
        let has_prefix = short_name != name;
        masters.insert(name, fields);
        if has_prefix {
            masters.insert(short_name, fields);
        }
    }

    let mut photos: Vec<PhotoItem> = Vec::new();
    let mut master_field_names: BTreeSet<String> = BTreeSet::new();
    let mut asset_field_names: BTreeSet<String> = BTreeSet::new();
    let mut video_count = 0i64;
    let mut live_photo_count = 0i64;
    let mut playable_video_count = 0i64;
    let mut playable_live_count = 0i64;

    for record in records {
        if field(record, ck::FIELD_RECORD_TYPE).as_str() != Some(ck::RECORD_TYPE_ASSET) {
            continue;
        }
        let fields = field(record, ck::FIELD_FIELDS);
        let master_id = master_ref_name(fields);
        let mut master = master_id.as_deref().and_then(|name| masters.get(name).copied());
        if master.is_none() {
            if let Some(name) = master_id.as_deref() {
                let short_name = last_segment(name);
                master = masters.get(short_name.as_str()).copied();
            }
        }

        let master = master.unwrap_or(fields);

        if let Value::Object(map) = master {
            master_field_names.extend(map.keys().cloned());
        }
        if let Value::Object(map) = fields {
            asset_field_names.extend(map.keys().cloned());
        }

        let file_name = text(master, ck::FIELD_FILENAME)
            .or_else(|| text(fields, ck::FIELD_FILENAME))
            .or_else(|| text(master, ck::FIELD_ITEM_NAME))
            .or_else(|| text(fields, ck::FIELD_ITEM_NAME));

        let duration = number_value(fields, master, &[ck::FIELD_DURATION, ck::FIELD_DURATION_ENC]);
        let live_photo = number_value(fields, master, &[ck::FIELD_VIDEO_COMPLEMENT_DURATION]) > 0.0;
        let is_video = !live_photo && duration > 0.0;

        let thumb_url = first_url(master, fields, ck::THUMB_FIELDS, ck::IMAGE_FILE_TYPE);
        let display_url = if is_video {
            thumb_url.clone()
        } else {
            first_url(master, fields, ck::DISPLAY_FIELDS, ck::IMAGE_FILE_TYPE)
        };
        let original_url = if is_video {
            first_non_null(&[
                first_url(master, fields, &[ck::ORIGINAL_RES], ck::VIDEO_FILE_TYPE),
                first_url(master, fields, ck::VIDEO_FIELDS, ck::VIDEO_FILE_TYPE),
            ])
        } else {
            first_url(master, fields, ck::ORIGINAL_FIELDS, ck::IMAGE_FILE_TYPE)
        };

        let video_url = if is_video || live_photo {
            first_non_null(&[
                first_url(master, fields, ck::VIDEO_FIELDS, ck::VIDEO_FILE_TYPE),
                if is_video { original_url.clone() } else { None },
            ])
        } else {
            None
        };

        if is_video {
            video_count += 1;
            if video_url.is_some() {
                playable_video_count += 1;
            }
        }
        if live_photo {
            live_photo_count += 1;
            if video_url.is_some() {
                playable_live_count += 1;
            }
        }

        let taken_at = long_value(fields, ck::FIELD_ASSET_DATE);
        let (width, height) = first_size(
            master,
            fields,
            if is_video || live_photo { ck::VIDEO_SIZE_FIELDS } else { ck::THUMB_SIZE_FIELDS },
        )
        .map_or((None, None), |(width, height)| (Some(width), Some(height)));
        let size_bytes = if is_video {
            first_file_size(master, fields, &[ck::ORIGINAL_RES])
                .or_else(|| first_file_size(master, fields, ck::VIDEO_FIELDS))
        } else {
            first_file_size(master, fields, ck::ORIGINAL_FIELDS)
        }
        .or_else(|| first_file_size(master, fields, ck::DISPLAY_FIELDS))
        .or_else(|| first_file_size(master, fields, ck::THUMB_FIELDS));
        photos.push(PhotoItem {
            id: record_name(record).unwrap_or_default(),
            file_name,
            taken_at: Some(taken_at),
            is_video: Some(is_video),

            thumb_url: thumb_url.unwrap_or_default(),
            display_url,
            original_url,
            video_url,
            is_live_photo: Some(live_photo),
            width,
            height,
            size_bytes,
        });
    }

    if !master_field_names.is_empty() {
        log::debug!(
            "[PHOTOS] page fields: Master={:?} Asset={:?} photos={} videos={}(playable {}) live={}(paired video {})",
            master_field_names,
            asset_field_names,
            photos.len() as i64 - video_count - live_photo_count,
            video_count,
            playable_video_count,
            live_photo_count,
            playable_live_count
        );
    }

    let next_cursor = field(root, ck::CONTINUATION_MARKER).as_str().map(str::to_string);
    log::debug!(
        "[PHOTOS] parsed {} photos (nextCursor={})",
        photos.len(),
        if next_cursor.is_none() { "none" } else { "yes" }
    );
    PhotosPage { status: statuses::OK.to_string(), message: None, photos, next_cursor }
}

fn first_file_size(master: &Value, asset: &Value, candidates: &[&str]) -> Option<i64> {
    for key in candidates {
        if let Some(size) = asset_size(field(master, key)) {
            return Some(size);
        }
        if !std::ptr::eq(master, asset) {
            if let Some(size) = asset_size(field(asset, key)) {
                return Some(size);
            }
        }
    }
    None
}

fn asset_size(asset_field: &Value) -> Option<i64> {
    let value = field(asset_field, ck::WRAPPER_VALUE);
    as_long(field(value, ck::FILE_SIZE)).filter(|size| *size > 0)
}

fn first_url(master: &Value, asset: &Value, candidates: &[&str], substitution: &str) -> Option<String> {
    let same = std::ptr::eq(master, asset);
    for key in candidates {
        if let Some(url) = asset_url(field(master, key), substitution) {
            return Some(url);
        }
        if !same {
            if let Some(url) = asset_url(field(asset, key), substitution) {
                return Some(url);
            }
        }
    }
    None
}

fn first_size(master: &Value, asset: &Value, candidates: &[(&str, &str)]) -> Option<(i64, i64)> {
    for (width_key, height_key) in candidates {
        let mut width = positive_long(master, width_key);
        let mut height = positive_long(master, height_key);
        if (width.is_none() || height.is_none()) && !std::ptr::eq(master, asset) {
            width = width.or_else(|| positive_long(asset, width_key));
            height = height.or_else(|| positive_long(asset, height_key));
        }
        if let (Some(width), Some(height)) = (width, height) {
            return Some((width, height));
        }
    }
    None
}

fn positive_long(fields: &Value, key: &str) -> Option<i64> {
    let value = long_value(fields, key);
    (value > 0).then_some(value)
}

fn first_non_null(values: &[Option<String>]) -> Option<String> {
    values.iter().flatten().find(|value| !value.trim().is_empty()).cloned()
}

fn record_name(record: &Value) -> Option<String> {
    if let Some(name) = non_blank(as_text(field(record, ck::FIELD_RECORD_NAME))) {
        return Some(name);
    }
    let id = field(record, ck::FIELD_RECORD_ID);
    if let Some(text) = id.as_str() {
        return Some(text.to_string());
    }
    non_blank(as_text(field(id, ck::FIELD_RECORD_NAME)))
}

fn master_ref_name(fields: &Value) -> Option<String> {
    let reference = field(field(fields, ck::FIELD_MASTER_REF), ck::WRAPPER_VALUE);
    if reference.is_object() {
        if let Some(name) = non_blank(as_text(field(reference, ck::FIELD_RECORD_NAME))) {
            return Some(name);
        }
    }
    reference.as_str().map(str::to_string)
}

fn last_segment(reference: &str) -> String {
    match reference.rfind('/') {
        Some(index) => reference[index + 1..].to_string(),
        None => reference.to_string(),
    }
}

fn text(fields: &Value, key: &str) -> Option<String> {
    let node = field(fields, key);
    let value = field(node, ck::WRAPPER_VALUE);
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    node.as_str().map(str::to_string)
}

fn long_value(fields: &Value, key: &str) -> i64 {
    let node = field(fields, key);
    let value = field(node, ck::WRAPPER_VALUE);
    as_long(value).or_else(|| as_long(node)).unwrap_or(0)
}

fn number_value(primary: &Value, secondary: &Value, keys: &[&str]) -> f64 {
    let same = std::ptr::eq(primary, secondary);
    for key in keys {
        let mut value = double_value(primary, key);
        if value == 0.0 && !same {
            value = double_value(secondary, key);
        }
        if value != 0.0 {
            return value;
        }
    }
    0.0
}

fn double_value(fields: &Value, key: &str) -> f64 {
    let node = field(fields, key);
    let value = field(node, ck::WRAPPER_VALUE);
    if let Some(number) = value.as_f64() {
        return number;
    }
    node.as_f64().unwrap_or(0.0)
}

fn asset_url(asset_field: &Value, substitution: &str) -> Option<String> {
    let value = field(asset_field, ck::WRAPPER_VALUE);
    let url = field(value, ck::ASSET_DOWNLOAD_URL);
    let url = if url.is_string() { url } else { field(value, ck::ASSET_LEGACY_URL) };
    url.as_str().map(|text| text.replace(ck::FILE_TYPE_PLACEHOLDER, substitution))
}

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    static NULL: Value = Value::Null;
    value.get(key).unwrap_or(&NULL)
}

fn as_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Array(_) | Value::Object(_) => Some(String::new()),
        Value::Null => None,
    }
}

fn as_long(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().or_else(|| number.as_f64().map(|f| f as i64)),
        _ => None,
    }
}

fn non_blank(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}

fn option_string(value: Option<&str>) -> Value {
    match value {
        Some(text) => Value::String(text.to_string()),
        None => Value::Null,
    }
}

fn json_object(entries: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        map.insert(key.to_string(), value);
    }
    Value::Object(map)
}

fn header_refs<'a>(headers: &'a [(&'static str, String)]) -> Vec<(&'a str, &'a str)> {
    headers.iter().map(|(name, value)| (*name, value.as_str())).collect()
}

fn ok_result(session: &SrpSession) -> PhotosLoginResult {
    PhotosLoginResult::ok(
        session.id().to_string(),
        session.first_name().map(str::to_string),
        session.last_name().map(str::to_string),
    )
}

fn strip_whitespace(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn is_device_code(code: &str) -> bool {
    code.len() == photos_api::DEVICE_CODE_LEN && code.bytes().all(|byte| byte.is_ascii_digit())
}

fn is_phone_code(code: &str) -> bool {
    (photos_api::PHONE_CODE_MIN_LEN..=photos_api::PHONE_CODE_MAX_LEN).contains(&code.len())
        && code.bytes().all(|byte| byte.is_ascii_digit())
}

fn extract_verify_error(body: &str) -> String {
    if body.trim().is_empty() {
        return VERIFY_FALLBACK_MESSAGE.to_string();
    }
    let Ok(data) = serde_json::from_str::<VerifyErrorResponse>(body) else {
        return VERIFY_FALLBACK_MESSAGE.to_string();
    };
    if let Some(message) = data.first_message() {
        if !message.trim().is_empty() {
            return format!("verification code error: {message}");
        }
    }
    match data.reason.filter(|reason| !reason.trim().is_empty()) {
        Some(reason) => format!("{VERIFY_FALLBACK_MESSAGE} ({reason})"),
        None => VERIFY_FALLBACK_MESSAGE.to_string(),
    }
}

fn parse_probe(body: &str) -> Option<PhotosAuthProbeResponse> {
    match serde_json::from_str::<PhotosAuthProbeResponse>(body) {
        Ok(probe) => Some(probe),
        Err(error) => {
            log::warn!("[PHOTOS] response parse failed (PhotosAuthProbeResponse): {error}");
            None
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotosAuthProbeResponse {
    #[serde(default)]
    pub trusted_device_count: Option<i32>,
    #[serde(default)]
    pub trusted_phone_numbers: Option<Vec<TrustedPhone>>,
    #[serde(default)]
    pub phone_number_verification: Option<PhoneNumberVerification>,
    #[serde(default, alias = "authType")]
    pub authentication_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneNumberVerification {
    #[serde(default)]
    pub trusted_device_count: Option<i32>,
    #[serde(default)]
    pub trusted_phone_numbers: Option<Vec<TrustedPhone>>,
    #[serde(default, alias = "authType")]
    pub authentication_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedPhone {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub phone_number_id: Option<i64>,
    #[serde(default)]
    pub number_with_dial_code: Option<String>,
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default)]
    pub obfuscated_number: Option<String>,
}

impl TrustedPhone {
    pub fn effective_id(&self) -> i64 {
        self.id.or(self.phone_number_id).unwrap_or(0)
    }

    pub fn label(&self) -> Option<String> {
        non_blank(self.number_with_dial_code.clone())
            .or_else(|| non_blank(self.number.clone()))
            .or_else(|| self.obfuscated_number.clone())
    }
}

impl PhotosAuthProbeResponse {
    pub fn phones(&self) -> &[TrustedPhone] {
        if let Some(phones) = self.trusted_phone_numbers.as_deref() {
            if !phones.is_empty() {
                return phones;
            }
        }
        if let Some(nested) = self.phone_number_verification.as_ref() {
            if let Some(phones) = nested.trusted_phone_numbers.as_deref() {
                return phones;
            }
        }
        &[]
    }

    pub fn device_count(&self) -> Option<i32> {
        self.trusted_device_count
            .or_else(|| self.phone_number_verification.as_ref().and_then(|nested| nested.trusted_device_count))
    }

    pub fn auth_type(&self) -> Option<&str> {
        self.authentication_type.as_deref().or_else(|| {
            self.phone_number_verification.as_ref().and_then(|nested| nested.authentication_type.as_deref())
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VerifyErrorResponse {
    #[serde(default, rename = "service_errors", alias = "serviceErrors")]
    pub service_errors: Option<Vec<VerifyError>>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VerifyError {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

impl VerifyErrorResponse {
    pub fn first_message(&self) -> Option<&str> {
        self.service_errors.as_deref().and_then(|errors| errors.first()).and_then(|error| error.message.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_body() -> String {
        json!({
            "records": [
                {
                    "recordName": "MASTER-PHOTO",
                    "recordType": "CPLMaster",
                    "fields": {
                        "filename": {"value": "IMG_0001.HEIC", "type": "STRING"},
                        "resJPEGThumbRes": {
                            "value": {"downloadURL": "https://cvws.icloud-content.com.cn/B/AAA/${f}?o=1", "size": 83985},
                            "type": "ASSETID"
                        },
                        "resJPEGMedRes": {
                            "value": {"downloadURL": "https://cvws.icloud-content.com.cn/B/BBB/${f}?o=1"},
                            "type": "ASSETID"
                        },
                        "resJPEGThumbWidth": {"value": 600},
                        "resJPEGThumbHeight": {"value": 800},
                        "resOriginalRes": {
                            "value": {"url": "https://cvws.icloud-content.com.cn/B/CCC/${f}?o=1"},
                            "type": "ASSETID"
                        }
                    }
                },
                {
                    "recordName": "ASSET-PHOTO",
                    "recordType": "CPLAsset",
                    "fields": {
                        "assetDate": {"value": 1711339140000i64, "type": "TIMESTAMP"},
                        "duration": {"value": 0},
                        "masterRef": {
                            "value": {"recordName": "MASTER-PHOTO", "action": "DELETE_SELF"},
                            "type": "REFERENCE"
                        }
                    }
                },
                {
                    "recordName": "MASTER-VIDEO",
                    "recordType": "CPLMaster",
                    "fields": {
                        "resJPEGThumbRes": {
                            "value": {"downloadURL": "https://cvws.icloud-content.com.cn/B/DDD/${f}"}
                        },
                        "resOriginalRes": {
                            "value": {"downloadURL": "https://cvws.icloud-content.com.cn/B/EEE/${f}"}
                        },
                        "resVidMedRes": {
                            "value": {"downloadURL": "https://cvws.icloud-content.com.cn/B/FFF/${f}"}
                        },
                        "resVidMedWidth": {"value": 1080},
                        "resVidMedHeight": {"value": 1920}
                    }
                },
                {
                    "recordName": "ASSET-VIDEO",
                    "recordType": "CPLAsset",
                    "fields": {
                        "assetDate": {"value": 1711339200000i64},
                        "duration": {"value": 3.5},
                        "masterRef": {"value": {"recordName": "Master/MASTER-VIDEO"}}
                    }
                },
                {
                    "recordName": "MASTER-LIVE",
                    "recordType": "CPLMaster",
                    "fields": {
                        "resJPEGThumbRes": {
                            "value": {"downloadURL": "https://cvws.icloud-content.com.cn/B/GGG/${f}"}
                        },
                        "resVidComplRes": {
                            "value": {"downloadURL": "https://cvws.icloud-content.com.cn/B/HHH/${f}"}
                        },
                        "resVidComplWidth": {"value": 1440},
                        "resVidComplHeight": {"value": 1080}
                    }
                },
                {
                    "recordId": {"recordName": "ASSET-LIVE"},
                    "recordType": "CPLAsset",
                    "fields": {
                        "assetDate": {"value": 1711339300000i64},
                        "vidComplDurValue": {"value": 2.0},
                        "masterRef": {"value": {"recordName": "MASTER-LIVE"}}
                    }
                }
            ],
            "continuationMarker": "EgYZ",
            "syncToken": "token"
        })
        .to_string()
    }

    #[test]
    fn parses_photo_video_and_live_photo() {
        let page = parse_records(&sample_body());
        assert_eq!(page.status, statuses::OK);
        assert_eq!(page.photos.len(), 3);
        assert_eq!(page.next_cursor.as_deref(), Some("EgYZ"));

        let photo = &page.photos[0];
        assert_eq!(photo.id, "ASSET-PHOTO");
        assert_eq!(photo.file_name.as_deref(), Some("IMG_0001.HEIC"));
        assert_eq!(photo.taken_at, Some(1_711_339_140_000));
        assert_eq!(photo.is_video, Some(false));
        assert_eq!(photo.is_live_photo, Some(false));
        assert_eq!(photo.thumb_url, "https://cvws.icloud-content.com.cn/B/AAA/unknown.jpeg?o=1");
        assert_eq!(photo.display_url.as_deref(), Some("https://cvws.icloud-content.com.cn/B/BBB/unknown.jpeg?o=1"));

        assert_eq!(photo.original_url.as_deref(), Some("https://cvws.icloud-content.com.cn/B/CCC/unknown.jpeg?o=1"));
        assert_eq!(photo.video_url, None);
        assert_eq!((photo.width, photo.height), (Some(600), Some(800)));
        assert_eq!(photo.size_bytes, Some(83_985));

        let video = &page.photos[1];
        assert_eq!(video.id, "ASSET-VIDEO");
        assert_eq!(video.is_video, Some(true));
        assert_eq!(video.is_live_photo, Some(false));
        assert_eq!(video.display_url.as_deref(), Some(video.thumb_url.as_str()));
        assert_eq!(video.original_url.as_deref(), Some("https://cvws.icloud-content.com.cn/B/EEE/unknown.mp4"));
        assert_eq!(video.video_url.as_deref(), Some("https://cvws.icloud-content.com.cn/B/FFF/unknown.mp4"));
        assert_eq!((video.width, video.height), (Some(1080), Some(1920)));

        let live = &page.photos[2];
        assert_eq!(live.id, "ASSET-LIVE");
        assert_eq!(live.is_video, Some(false));
        assert_eq!(live.is_live_photo, Some(true));
        assert_eq!(live.video_url.as_deref(), Some("https://cvws.icloud-content.com.cn/B/HHH/unknown.mp4"));
        assert_eq!((live.width, live.height), (Some(1440), Some(1080)));
    }

    #[test]
    fn zone_not_found_is_a_friendly_account_message() {
        let body = r#"{"serverErrorCode":"ZONE_NOT_FOUND","reason":"Zone does not exist","error":"ZONE_NOT_FOUND"}"#;
        let page = parse_records(body);
        assert_eq!(page.status, statuses::ERROR);
        assert!(page.message.unwrap_or_default().contains("iCloud Photos is not enabled"));
    }

    #[test]
    fn missing_records_is_an_error_page() {
        let page = parse_records(r#"{"foo":1}"#);
        assert_eq!(page.status, statuses::ERROR);
        assert!(page.photos.is_empty());
        assert!(page.message.unwrap_or_default().contains("photo response is missing records"));
        assert_eq!(parse_records("not json").status, statuses::ERROR);
    }

    #[test]
    fn unpaired_asset_falls_back_to_its_own_fields() {
        let body = json!({
            "records": [{
                "recordName": "ASSET-ONLY",
                "recordType": "CPLAsset",
                "fields": {
                    "assetDate": {"value": 1},
                    "resJPEGThumbRes": {
                        "value": {"downloadURL": "https://cvws.icloud-content.com.cn/B/X/${f}"}
                    }
                }
            }]
        })
        .to_string();
        let page = parse_records(&body);
        assert_eq!(page.photos.len(), 1);
        assert_eq!(page.photos[0].file_name, None);
        assert_eq!(page.photos[0].thumb_url, "https://cvws.icloud-content.com.cn/B/X/unknown.jpeg");
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn master_record_with_prefix_is_indexed_by_short_name() {
        let body = json!({
            "records": [
                {
                    "recordName": "Master/M-PREFIX",
                    "recordType": "CPLMaster",
                    "fields": {
                        "resJPEGThumbRes": {
                            "value": {"downloadURL": "https://cvws.icloud-content.com/B/P/${f}"}
                        }
                    }
                },
                {
                    "recordName": "ASSET-PREFIX",
                    "recordType": "CPLAsset",
                    "fields": {
                        "assetDate": {"value": 42},
                        "masterRef": {"value": {"recordName": "M-PREFIX"}}
                    }
                }
            ]
        })
        .to_string();
        let page = parse_records(&body);
        assert_eq!(page.photos.len(), 1);
        assert_eq!(page.photos[0].thumb_url, "https://cvws.icloud-content.com/B/P/unknown.jpeg");
    }

    fn filter_by(body: &Value) -> &Vec<Value> {
        body[ckq::QUERY][ckq::FILTER_BY].as_array().expect("filterBy 必须是 query 下的数组")
    }

    #[test]
    fn build_query_body_matches_web_client_shape() {
        let body = build_query_body("CPLAssetAndMasterByAssetDate", None, 60);
        assert_eq!(body[ckq::QUERY][ckq::RECORD_TYPE], Value::String("CPLAssetAndMasterByAssetDate".to_string()));
        assert_eq!(body[ckq::QUERY][ckq::FILTER_BY][0][ckq::FIELD_NAME], Value::String("startDate".to_string()));
        assert_eq!(body[ckq::QUERY][ckq::FILTER_BY][0][ckq::FIELD_VALUE][ckq::VALUE_TYPE], json!("TIMESTAMP"));
        assert_eq!(body[ckq::QUERY][ckq::FILTER_BY][0][ckq::FIELD_VALUE][ckq::VALUE], json!(0));
        assert_eq!(body[ckq::QUERY][ckq::SORT_BY][0][ckq::ASCENDING], json!(false));
        assert_eq!(body[ckq::ZONE_ID][ckq::ZONE_NAME], json!("PrimarySync"));
        assert_eq!(body[ckq::DESIRED_KEYS].as_array().map(Vec::len), Some(ck::DESIRED_KEYS.len()));

        assert_eq!(body[ckq::RESULTS_LIMIT], json!(120));
        assert_eq!(body.get(ck::CONTINUATION_MARKER), None);

        let body = build_query_body("CPLAssetAndMasterByAssetDate", Some("EgYZ"), 200);
        assert_eq!(body[ck::CONTINUATION_MARKER], json!("EgYZ"));

        assert_eq!(body[ckq::RESULTS_LIMIT], json!(400));
        let body = build_query_body("CPLAssetAndMasterByAssetDate", Some("   "), 60);
        assert_eq!(body.get(ck::CONTINUATION_MARKER), None);
    }

    #[test]
    fn query_body_carries_no_date_filter() {
        let body = build_query_body("CPLAssetAndMasterByAssetDate", None, 60);
        let filters = filter_by(&body);
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0][ckq::FIELD_NAME], json!("startDate"));
        assert_eq!(filters[0][ckq::COMPARATOR], json!("GREATER_THAN"));
        assert_eq!(filters[0][ckq::FIELD_VALUE][ckq::VALUE], json!(0));
    }

    #[test]
    fn query_body_keeps_pagination_and_other_fields() {
        let body = build_query_body("CPLAssetAndMasterByAssetDate", Some("EgYZ"), 60);
        assert_eq!(body[ckq::QUERY][ckq::RECORD_TYPE], json!("CPLAssetAndMasterByAssetDate"));
        assert_eq!(body[ckq::QUERY][ckq::SORT_BY][0][ckq::FIELD_NAME], json!("assetDate"));
        assert_eq!(body[ckq::QUERY][ckq::SORT_BY][0][ckq::ASCENDING], json!(false));
        assert_eq!(body[ckq::ZONE_ID][ckq::ZONE_NAME], json!("PrimarySync"));
        assert_eq!(body[ckq::DESIRED_KEYS].as_array().map(Vec::len), Some(ck::DESIRED_KEYS.len()));
        assert_eq!(body[ckq::RESULTS_LIMIT], json!(120));
        assert_eq!(body[ck::CONTINUATION_MARKER], json!("EgYZ"));
    }

    #[test]
    fn verify_error_extraction_handles_both_wire_shapes() {
        assert_eq!(
            extract_verify_error(r#"{"service_errors":[{"code":"-21669","message":"验证码不正确"}]}"#),
            "verification code error: 验证码不正确"
        );
        assert_eq!(
            extract_verify_error(r#"{"serviceErrors":[{"message":"bad code"}]}"#),
            "verification code error: bad code"
        );
        assert_eq!(
            extract_verify_error(r#"{"reason":"INVALID_CODE"}"#),
            "Verification code is incorrect or expired (INVALID_CODE)"
        );
        assert_eq!(extract_verify_error("{}"), "Verification code is incorrect or expired");
        assert_eq!(extract_verify_error(""), "Verification code is incorrect or expired");
        assert_eq!(extract_verify_error("<html>"), "Verification code is incorrect or expired");
    }

    #[test]
    fn validates_verification_codes() {
        assert!(is_device_code("123456"));
        assert!(!is_device_code("12345"));
        assert!(!is_device_code("1234567"));
        assert!(!is_device_code("12345a"));
        assert!(is_phone_code("1234"));
        assert!(is_phone_code("12345678"));
        assert!(!is_phone_code("123"));
        assert!(!is_phone_code("123456789"));
        assert_eq!(strip_whitespace(" 12 34 56 "), "123456");
    }

    #[test]
    fn probe_response_reads_nested_phone_shape() {
        let probe = parse_probe(
            r#"{"phoneNumberVerification":{"trustedDeviceCount":2,
                "trustedPhoneNumbers":[{"id":7,"numberWithDialCode":"+86 138****"},
                                       {"phoneNumberId":9,"obfuscatedNumber":"+86 139****"}],
                "authType":"hsa2"}}"#,
        )
        .expect("探测响应应能解析");
        assert_eq!(probe.device_count(), Some(2));
        assert_eq!(probe.auth_type(), Some("hsa2"));
        let phones = probe.phones();
        assert_eq!(phones.len(), 2);
        assert_eq!(phones[0].effective_id(), 7);
        assert_eq!(phones[0].label().as_deref(), Some("+86 138****"));
        assert_eq!(phones[1].effective_id(), 9);
        assert_eq!(probe.auth_type(), Some("hsa2"));
    }

    #[test]
    fn probe_response_prefers_root_phone_list() {
        let probe = parse_probe(
            r#"{"trustedDeviceCount":0,"trustedPhoneNumbers":[{"id":1,"number":"+86"}],
                "phoneNumberVerification":{"trustedDeviceCount":5,"trustedPhoneNumbers":[]},
                "authenticationType":"hsa2"}"#,
        )
        .expect("探测响应应能解析");

        assert_eq!(probe.device_count(), Some(0));
        assert_eq!(probe.phones().len(), 1);
        assert_eq!(probe.auth_type(), Some("hsa2"));
        assert!(parse_probe("oops").is_none());
    }

    #[test]
    fn device_headers_carry_icloud_domain_and_locale() {
        let session =
            SrpSession::new("tester@example.com".to_string(), Some("secret".to_string()), false).expect("会话构造失败");
        let headers = device_headers(&session);
        assert!(headers.iter().any(|(name, value)| *name == hh::DOMAIN_ID && value == hc::ICLOUD_DOMAIN_ID));
        assert!(headers.iter().any(|(name, value)| *name == hh::LOCALE && value == hc::LOCALE_ZH_CN));
    }

    #[test]
    fn cloud_kit_base_falls_back_to_region_host() {
        let session =
            SrpSession::new("tester@example.com".to_string(), Some("secret".to_string()), true).expect("会话构造失败");
        assert_eq!(cloud_kit_base(&session).as_deref(), Some(api::cloud_kit::BASE_CN));
        assert!(session.auth_base().contains(".cn"));
    }

    fn library_info_body() -> String {
        json!({
            "records": [{
                "recordName": "PrimarySync-0000-LI",
                "recordType": "CPLLibraryInfo",
                "fields": {
                    "photosCount": {"value": 1234, "type": "INT64"},
                    "videosCount": {"value": 321, "type": "INT64"},

                    "irisCount": {"value": 99, "type": "INT64"},
                    "albumsCount": {"value": 7, "type": "INT64"}
                }
            }],
            "syncToken": "token"
        })
        .to_string()
    }

    #[test]
    fn parses_library_total_without_double_counting_iris() {
        let total = parse_library_total(&library_info_body()).expect("图库总数应能解析");
        assert_eq!(total.photos, 1234);
        assert_eq!(total.videos, 321);

        assert_eq!(total.assets, 1555);
    }

    #[test]
    fn missing_video_count_counts_as_zero() {
        let body = json!({
            "records": [{
                "recordName": "PrimarySync-0000-LI",
                "recordType": "CPLLibraryInfo",
                "fields": {"photosCount": {"value": 42}}
            }]
        })
        .to_string();
        let total = parse_library_total(&body).expect("缺 videosCount 也应按 0 计");
        assert_eq!(total.photos, 42);
        assert_eq!(total.videos, 0);
        assert_eq!(total.assets, 42);
    }

    #[test]
    fn unparseable_library_total_is_an_error() {
        assert!(parse_library_total(r#"{"records":[]}"#).is_err());
        assert!(parse_library_total(r#"{"foo":1}"#).is_err());
        assert!(parse_library_total(r#"{"records":[{"recordName":"ASSET-1","recordType":"CPLAsset"}]}"#).is_err());
        assert!(parse_library_total("not json").is_err());
        assert!(parse_library_total("").is_err());
    }

    #[test]
    fn lookup_body_targets_the_library_info_record() {
        let body = build_lookup_body(ck::RECORD_NAME_LIBRARY_INFO, Some("_owner0001"));
        assert_eq!(body[ck::RECORDS][0][ck::FIELD_RECORD_NAME], json!("PrimarySync-0000-LI"));
        assert_eq!(body[ckq::ZONE_ID][ckq::ZONE_NAME], json!("PrimarySync"));
        assert_eq!(body[ckq::ZONE_ID][ckq::ZONE_TYPE], json!("REGULAR_CUSTOM_ZONE"));
        assert_eq!(body[ckq::ZONE_ID][ck::FIELD_OWNER_RECORD_NAME], json!("_owner0001"));

        assert_eq!(body.get(ckq::DESIRED_KEYS), None);
    }

    #[test]
    fn lookup_body_omits_unknown_zone_owner() {
        let body = build_lookup_body(ck::RECORD_NAME_LIBRARY_INFO, None);
        assert_eq!(body[ckq::ZONE_ID].get(ck::FIELD_OWNER_RECORD_NAME), None);
        let blank = build_lookup_body(ck::RECORD_NAME_LIBRARY_INFO, Some("  "));
        assert_eq!(blank[ckq::ZONE_ID].get(ck::FIELD_OWNER_RECORD_NAME), None);
        assert_eq!(blank[ckq::ZONE_ID][ckq::ZONE_NAME], json!("PrimarySync"));
    }

    #[test]
    fn lookup_body_can_target_the_asset_distribution_record() {
        let body = build_lookup_body(ck::RECORD_NAME_ASSET_DISTRIBUTION, None);
        assert_eq!(
            body[ck::RECORDS][0][ck::FIELD_RECORD_NAME],
            json!("PrimarySync-0000-CI-CPLAssetByAssetDateWithoutHiddenOrDeleted")
        );
        assert_eq!(body[ckq::ZONE_ID][ckq::ZONE_NAME], json!("PrimarySync"));
    }

    #[test]
    fn zone_owner_is_read_from_zones_list() {
        let body = json!({
            "zones": [
                {"zoneID": {"zoneName": "Other", "ownerRecordName": "_notmine"}},
                {"zoneID": {"zoneName": "PrimarySync", "ownerRecordName": "_owner0001", "zoneType": "REGULAR_CUSTOM_ZONE"}}
            ]
        })
        .to_string();
        assert_eq!(zone_owner(&body).as_deref(), Some("_owner0001"));

        assert_eq!(zone_owner(r#"{"zones":[{"zoneID":{"zoneName":"PrimarySync"}}]}"#), None);
        assert_eq!(zone_owner(r#"{"zones":[]}"#), None);
        assert_eq!(zone_owner("oops"), None);
    }

    #[test]
    fn rank_page_url_stays_on_the_plain_records_query_endpoint() {
        let session =
            SrpSession::new("tester@example.com".to_string(), Some("secret".to_string()), false).expect("会话构造失败");
        let url = build_records_query_url(&session).expect("应能拼出 URL");
        assert!(url.starts_with(api::cloud_kit::BASE));
        assert!(url.contains(api::cloud_kit::RECORDS_QUERY));
        assert!(url.contains(api::cloud_kit::QUERY_PARAMS));
        assert!(url.contains(api::cloud_kit::PARAM_DSID));
        assert!(!url.contains("/internal"));
        assert!(!url.contains("parallel"));
    }

    #[test]
    fn rank_page_body_matches_the_ordered_window_shape() {
        let body = build_rank_page_body(10, 8, Some("_owner0001"));
        assert_eq!(body[ckq::QUERY][ckq::RECORD_TYPE], json!("CPLAssetAndMasterByAssetDateWithoutHiddenOrDeleted"));
        let filters = filter_by(&body);
        assert_eq!(filters.len(), 2);
        assert_eq!(filters[0][ckq::FIELD_NAME], json!("direction"));
        assert_eq!(filters[0][ckq::COMPARATOR], json!("EQUALS"));
        assert_eq!(filters[0][ckq::FIELD_VALUE][ckq::VALUE], json!("DESCENDING"));
        assert_eq!(filters[0][ckq::FIELD_VALUE][ckq::VALUE_TYPE], json!("STRING"));
        assert_eq!(filters[1][ckq::FIELD_NAME], json!("startRank"));
        assert_eq!(filters[1][ckq::COMPARATOR], json!("EQUALS"));
        assert_eq!(filters[1][ckq::FIELD_VALUE][ckq::VALUE], json!(10));
        assert_eq!(filters[1][ckq::FIELD_VALUE][ckq::VALUE_TYPE], json!("INT64"));

        assert_eq!(body[ckq::ZONE_ID][ckq::ZONE_NAME], json!("PrimarySync"));
        assert_eq!(body[ckq::ZONE_ID][ckq::ZONE_TYPE], json!("REGULAR_CUSTOM_ZONE"));
        assert_eq!(body[ckq::ZONE_ID][ck::FIELD_OWNER_RECORD_NAME], json!("_owner0001"));
        assert_eq!(body[ckq::QUERY].get(ckq::ZONE_ID), None);
        assert_eq!(body[ckq::QUERY].get(ckq::RESULTS_LIMIT), None);
        assert_eq!(body[ckq::RESULTS_LIMIT], json!(8));

        assert_eq!(body[ckq::RESULTS_LIMIT].as_i64().map(|limit| limit % 2), Some(0));

        assert_eq!(body.get(ck::CONTINUATION_MARKER), None);

        let page = build_rank_page_body(0, 60 * photos_api::RECORDS_PER_PHOTO, None);
        assert_eq!(page[ckq::RESULTS_LIMIT], json!(120));

        let small = build_rank_page_body(95, photos_api::RECORDS_PER_PHOTO, Some("_owner0001"));
        assert_eq!(small[ckq::RESULTS_LIMIT], json!(2));
        assert_eq!(small[ckq::QUERY][ckq::FILTER_BY][1][ckq::FIELD_VALUE][ckq::VALUE], json!(95));
        assert_eq!(small[ckq::QUERY][ckq::FILTER_BY][1][ckq::FIELD_VALUE][ckq::VALUE_TYPE], json!("INT64"));

        assert_eq!(page[ckq::ZONE_ID].get(ck::FIELD_OWNER_RECORD_NAME), None);
    }

    #[test]
    fn parses_a_page_from_the_rank_query_response() {
        let body = json!({
            "records": [
                {"recordName": "M-9", "recordType": "CPLMaster", "fields": {
                    "filename": {"value": "IMG_0009.HEIC"},
                    "resJPEGThumbRes": {"value": {"downloadURL": "https://cvws.icloud-content.com/B/Z/${f}"}}
                }},
                {"recordName": "A-9", "recordType": "CPLAsset", "fields": {
                    "assetDate": {"value": 1_710_253_920_000i64},
                    "masterRef": {"value": {"recordName": "M-9"}}
                }}
            ],
            "continuationMarker": "EgYZ",
            "syncToken": "token"
        })
        .to_string();
        let page = parse_records(&body);
        assert_eq!(page.status, statuses::OK);
        assert_eq!(page.photos.len(), 1);
        assert_eq!(page.photos[0].id, "A-9");
        assert_eq!(page.photos[0].file_name.as_deref(), Some("IMG_0009.HEIC"));
        assert_eq!(page.photos[0].thumb_url, "https://cvws.icloud-content.com/B/Z/unknown.jpeg");
        assert_eq!(page.next_cursor.as_deref(), Some("EgYZ"));

        let empty = parse_records(r#"{"records":[]}"#);
        assert_eq!(empty.status, statuses::OK);
        assert!(empty.photos.is_empty());

        assert_eq!(parse_records("oops").status, statuses::ERROR);
        assert_eq!(parse_records(r#"{"batch":[{"records":[]}]}"#).status, statuses::ERROR);
    }

    use crate::models::{PhotosMonthExactRequest, PhotosTimelineRequest};

    fn put_varint(mut value: u64, out: &mut Vec<u8>) {
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                return;
            }
        }
    }

    fn put_varint_field(field_number: u64, value: u64, out: &mut Vec<u8>) {
        put_varint((field_number << 3) | pbd::WIRE_VARINT, out);
        put_varint(value, out);
    }

    fn put_bytes_field(field_number: u64, bytes: &[u8], out: &mut Vec<u8>) {
        put_varint((field_number << 3) | pbd::WIRE_LENGTH_DELIMITED, out);
        put_varint(bytes.len() as u64, out);
        out.extend_from_slice(bytes);
    }

    fn bucket_bytes(start_ms: i64, count: i64) -> Vec<u8> {
        let mut out = Vec::new();
        put_varint_field(pbd::FIELD_BUCKET_START, start_ms as u64, &mut out);
        put_varint_field(pbd::FIELD_BUCKET_COUNT, count as u64, &mut out);
        out
    }

    fn distribution_bytes(bucket_width_ms: i64, buckets: &[(i64, i64)]) -> Vec<u8> {
        let mut out = Vec::new();
        put_varint_field(pbd::FIELD_BUCKET_WIDTH, bucket_width_ms as u64, &mut out);
        for (start_ms, count) in buckets {
            put_bytes_field(pbd::FIELD_BUCKET, &bucket_bytes(*start_ms, *count), &mut out);
        }
        out
    }

    fn distribution_body(bucket_width_ms: i64, buckets: &[(i64, i64)]) -> String {
        let encoded = BASE64.encode(distribution_bytes(bucket_width_ms, buckets));
        json!({
            "records": [{
                "recordName": "PrimarySync-0000-CI-CPLAssetByAssetDateWithoutHiddenOrDeleted",
                "recordType": "CPLCollectionIndex",
                "fields": {"cplAssetDistribution": {"value": encoded, "type": "BYTES"}}
            }],
            "syncToken": "token"
        })
        .to_string()
    }

    #[test]
    fn parses_distribution_buckets_with_sizes_and_prefix_ranks() {
        let body = distribution_body(604_800_000, &[(1_704_326_400_000, 5), (1_704_931_200_000, 7)]);
        let parsed = parse_asset_distribution(&body).expect("应能解析");
        assert_eq!(parsed.distribution.bucket_width_ms, 604_800_000);
        assert_eq!(parsed.distribution.total, 12);
        assert_eq!(
            parsed.distribution.buckets,
            vec![
                AssetBucket { start_ms: 1_704_326_400_000, count: 5, start_rank: 11, second_month_rank: Some(7) },
                AssetBucket { start_ms: 1_704_931_200_000, count: 7, start_rank: 6, second_month_rank: Some(0) },
            ]
        );
    }

    #[test]
    fn prefix_ranks_match_the_client_accumulation() {
        let dist = parse_asset_distribution_bytes(&distribution_bytes(
            86_400_000,
            &[(1_000, 3), (2_000, 0), (3_000, 4), (4_000, 1), (5_000, 2)],
        ))
        .expect("应能解析");
        assert_eq!(dist.total, 10);
        assert_eq!(
            dist.buckets
                .iter()
                .map(|bucket| (bucket.start_ms, bucket.count, bucket.start_rank, bucket.second_month_rank))
                .collect::<Vec<_>>(),
            vec![
                (1_000, 3, 9, Some(7)),
                (2_000, 0, 6, None),
                (3_000, 4, 6, Some(3)),
                (4_000, 1, 2, Some(2)),
                (5_000, 2, 1, Some(0)),
            ]
        );

        assert_eq!(dist.buckets.last().and_then(|bucket| bucket.second_month_rank), Some(0));
    }

    #[test]
    fn unsorted_buckets_are_sorted_before_the_prefix_sum() {
        let dist =
            parse_asset_distribution_bytes(&distribution_bytes(86_400_000, &[(3_000, 4), (1_000, 3), (2_000, 1)]))
                .expect("应能解析");
        assert_eq!(dist.buckets.iter().map(|bucket| bucket.start_ms).collect::<Vec<_>>(), vec![1_000, 2_000, 3_000]);
        assert_eq!(dist.buckets.iter().map(|bucket| bucket.start_rank).collect::<Vec<_>>(), vec![7, 4, 3]);
        assert_eq!(
            dist.buckets.iter().filter_map(|bucket| bucket.second_month_rank).collect::<Vec<_>>(),
            vec![5, 4, 0]
        );
        assert_eq!(dist.total, 8);

        assert!(dist.buckets.windows(2).all(|pair| pair[0].start_rank >= pair[1].start_rank));
    }

    #[test]
    fn unknown_fields_are_skipped_by_wire_type() {
        let mut bytes = Vec::new();
        put_varint_field(1, 99, &mut bytes);
        put_varint_field(pbd::FIELD_BUCKET_WIDTH, 86_400_000, &mut bytes);
        put_bytes_field(9, b"old-buckets-payload", &mut bytes);

        let mut bucket = bucket_bytes(1_000, 3);
        put_varint_field(6, 42, &mut bucket);
        put_bytes_field(7, b"x", &mut bucket);
        put_bytes_field(pbd::FIELD_BUCKET, &bucket, &mut bytes);

        bytes.push(((10u64 << 3) | pbd::WIRE_FIXED32) as u8);
        bytes.extend_from_slice(&[1, 2, 3, 4]);
        bytes.push(((11u64 << 3) | pbd::WIRE_FIXED64) as u8);
        bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        put_bytes_field(pbd::FIELD_BUCKET, &bucket_bytes(2_000, 4), &mut bytes);

        let dist = parse_asset_distribution_bytes(&bytes).expect("未知字段应被跳过");
        assert_eq!(dist.bucket_width_ms, 86_400_000);
        assert_eq!(dist.total, 7);

        assert_eq!(
            dist.buckets
                .iter()
                .map(|bucket| (bucket.start_ms, bucket.count, bucket.start_rank, bucket.second_month_rank))
                .collect::<Vec<_>>(),
            vec![(1_000, 3, 6, Some(4)), (2_000, 4, 3, Some(0))]
        );
    }

    #[test]
    fn malformed_protobuf_is_an_error_not_a_panic() {
        let mut truncated = distribution_bytes(86_400_000, &[(1_000, 3)]);
        truncated.push(0x80);
        assert!(parse_asset_distribution_bytes(&truncated).is_err());

        let mut nested = Vec::new();
        put_varint_field(pbd::FIELD_BUCKET_WIDTH, 86_400_000, &mut nested);
        put_bytes_field(pbd::FIELD_BUCKET, &[(pbd::FIELD_BUCKET_START << 3) as u8], &mut nested);
        assert!(parse_asset_distribution_bytes(&nested).is_err());

        let mut liar = vec![((pbd::FIELD_BUCKET << 3) | pbd::WIRE_LENGTH_DELIMITED) as u8];
        put_varint(999, &mut liar);
        liar.push(1);
        assert!(parse_asset_distribution_bytes(&liar).is_err());

        let mut too_long = vec![((pbd::FIELD_BUCKET_WIDTH << 3) | pbd::WIRE_VARINT) as u8];
        too_long.extend(std::iter::repeat_n(0xFF, 11));
        assert!(parse_asset_distribution_bytes(&too_long).is_err());

        assert!(parse_asset_distribution_bytes(&[((pbd::FIELD_BUCKET << 3) | 3) as u8]).is_err());
    }

    #[test]
    fn empty_distribution_is_zero_not_an_error() {
        let mut only_width = Vec::new();
        put_varint_field(pbd::FIELD_BUCKET_WIDTH, 604_800_000, &mut only_width);
        let dist = parse_asset_distribution_bytes(&only_width).expect("空直方图是合法的");
        assert_eq!(dist.bucket_width_ms, 604_800_000);
        assert_eq!(dist.total, 0);
        assert!(dist.buckets.is_empty());

        let blank = parse_asset_distribution_bytes(&[]).expect("空字节流是合法的空直方图");
        assert_eq!(blank.bucket_width_ms, 0);
        assert_eq!(blank.total, 0);
        assert!(blank.buckets.is_empty());

        let timeline = timeline_from_distribution(&blank, DEFAULT_TZ_OFFSET_SECONDS);
        assert_eq!(timeline.total, 0);
        assert!(timeline.years.is_empty());
    }

    #[test]
    fn prefix_ranks_stay_exact_over_many_buckets() {
        let buckets: Vec<(i64, i64)> = (0..177i64).map(|index| (1_000_000 + index * 604_800_000, index + 1)).collect();
        let dist = parse_asset_distribution_bytes(&distribution_bytes(604_800_000, &buckets)).expect("应能解析");
        assert_eq!(dist.buckets.len(), 177);
        let total = (1..=177i64).sum::<i64>();
        assert_eq!(dist.total, total);
        for (index, bucket) in dist.buckets.iter().enumerate() {
            let index = index as i64;

            assert_eq!(bucket.count, index + 1, "第 {index} 个桶的张数");
            let older: i64 = (1..=index).sum();
            assert_eq!(bucket.start_rank, total - 1 - older, "第 {index} 个桶的主锚点");
            assert_eq!(bucket.second_month_rank, Some(total - older - (index + 1)), "第 {index} 个桶的第二锚点");
        }

        assert_eq!(dist.buckets.first().map(|bucket| bucket.start_rank), Some(total - 1));
        assert_eq!(dist.buckets.first().and_then(|bucket| bucket.second_month_rank), Some(total - 1));
        let newest = dist.buckets.last().expect("至少yes桶");
        assert_eq!(newest.count, 177);
        assert_eq!(newest.start_rank, 176);
        assert_eq!(newest.second_month_rank, Some(0));
    }

    #[test]
    fn start_rank_matches_the_real_capture_anchors() {
        let buckets: [(i64, i64); 10] = [
            (1_531_987_200_000, 1),
            (1_533_196_800_000, 1_200), // 2018-08-02
            (1_698_912_000_000, 2_000), // 2023-11-02
            (1_701_936_000_000, 2_500), // 2023-12-07
            (1_704_355_200_000, 2_845), // 2024-01-04
            (1_706_774_400_000, 1),
            (1_709_193_600_000, 5),
            (1_709_798_400_000, 11),
            (1_710_403_200_000, 6), // 2024-03-14
            (1_711_008_000_000, 73),
        ];
        let dist = parse_asset_distribution_bytes(&distribution_bytes(604_800_000, &buckets)).expect("应能解析");
        assert_eq!(dist.total, 8_642, "总张数必须是抓包里的 8642");

        let newest = dist.buckets.last().expect("至少yes桶");
        assert_eq!(newest.start_ms, 1_711_008_000_000, "最新那桶 = 含 2024-03-25 的那一周");
        assert_eq!(newest.second_month_rank, Some(0), "名次 0 = 桶内最新那张");
        assert_eq!(newest.start_rank, 72, "主锚点 = 桶内最早那张的名次");
        assert_ne!(newest.start_rank, 0, "旧口径的「最新桶 = 0」必须不再成立");

        let oldest = dist.buckets.first().expect("至少yes桶");
        assert_eq!(oldest.start_ms, 1_531_987_200_000, "最老那桶 = 含 2018-07-21 的那一周");
        assert_eq!(oldest.count, 1, "最老那桶只yes 1 张: older=0 ⇒ 主锚点 = 8642-1-0");
        assert_eq!(oldest.start_rank, 8_641);
        assert_eq!(oldest.start_rank, dist.total - 1);
        assert_eq!(oldest.second_month_rank, Some(8_641), "只yes 1 张时两个锚点重合");

        let march_week =
            dist.buckets.iter().find(|bucket| bucket.start_ms == 1_709_798_400_000).expect("应yes 2024-03-07 那一周");
        assert_eq!(march_week.start_rank, 89);
        assert_eq!(march_week.count, 11);

        let february_week =
            dist.buckets.iter().find(|bucket| bucket.start_ms == 1_706_774_400_000).expect("应yes 2024-02-01 那一周");
        assert_eq!(february_week.start_rank, 95);
        assert_eq!(february_week.count, 1);

        let observed = [0i64, 89, 95, 8_641];
        let on_primary = dist.buckets.iter().filter(|bucket| observed.contains(&bucket.start_rank)).count();
        let on_second = dist
            .buckets
            .iter()
            .filter_map(|bucket| bucket.second_month_rank)
            .filter(|rank| observed.contains(rank))
            .count();
        assert_eq!(on_primary, 3, "89/95/8641 是桶的主锚点");
        assert_eq!(on_second, 3, "0 是最新那桶的第二锚点（8641/95 恰好与主锚点重合）");

        for bucket in &dist.buckets {
            let second = bucket.second_month_rank.expect("非空桶都yes第二锚点");
            assert_eq!(second, bucket.start_rank - bucket.count + 1, "第二锚点 = 主锚点 - count + 1");
            assert!(bucket.start_rank >= 0 && bucket.start_rank < dist.total);
            assert!(second >= 0);
        }
        assert!(dist.buckets.windows(2).all(|pair| pair[0].start_rank > pair[1].start_rank));
        assert!(dist.buckets.windows(2).all(|pair| pair[0].second_month_rank > pair[1].second_month_rank));

        let timeline = timeline_from_distribution(&dist, DEFAULT_TZ_OFFSET_SECONDS);
        assert_eq!(timeline.total, 8_642);
        assert_eq!(timeline.years.iter().map(|year| year.year).collect::<Vec<_>>(), vec![2018, 2023, 2024]);
        let months: Vec<(i32, u32, i64, i64)> = timeline
            .years
            .iter()
            .flat_map(|year| year.months.iter().map(|month| (month.year, month.month, month.count, month.start_rank)))
            .collect();
        assert_eq!(
            months,
            vec![
                (2018, 7, 1, 8_641),
                (2018, 8, 1_200, 8_640),
                (2023, 11, 2_000, 7_440),
                (2023, 12, 2_500, 5_440),
                (2024, 1, 2_845, 2_940),
                (2024, 2, 6, 95),
                (2024, 3, 90, 89),
            ]
        );

        assert_eq!(months.iter().map(|(_, _, count, _)| count).sum::<i64>(), 8_642);
    }

    #[test]
    fn cross_month_week_anchors_both_months_from_one_bucket() {
        let january_week = 1_706_140_800_000;
        let february_week = 1_706_745_600_000;
        let dist =
            parse_asset_distribution_bytes(&distribution_bytes(604_800_000, &[(january_week, 10), (february_week, 6)]))
                .expect("应能解析");

        let crossing = dist.buckets.first().expect("至少yes桶");
        assert_eq!(crossing.start_rank, 15);
        assert_eq!(crossing.second_month_rank, Some(6));

        let timeline = timeline_from_distribution(&dist, DEFAULT_TZ_OFFSET_SECONDS);
        assert_eq!(timeline.total, 16);
        assert_eq!(timeline.years.len(), 1);
        let year = &timeline.years[0];
        assert_eq!(year.year, 2024);
        assert_eq!(year.count, 16);
        assert_eq!(year.months.len(), 2);
        assert_eq!(year.months[0], MonthEntry { year: 2024, month: 1, count: 10, start_rank: 15 });
        assert_eq!(year.months[1], MonthEntry { year: 2024, month: 2, count: 6, start_rank: 6 });
    }

    #[test]
    fn cross_month_bridge_is_dropped_when_both_neighbours_anchor_their_months() {
        let dist = parse_asset_distribution_bytes(&distribution_bytes(
            604_800_000,
            &[(1_706_774_400_000, 1), (1_709_193_600_000, 5), (1_709_798_400_000, 3)],
        ))
        .expect("应能解析");
        assert_eq!(dist.total, 9);

        let bridge = dist.buckets.get(1).expect("应yes 2/29 那一周");
        assert_eq!(bridge.start_rank, 7);
        assert_eq!(bridge.second_month_rank, Some(3));

        let timeline = timeline_from_distribution(&dist, DEFAULT_TZ_OFFSET_SECONDS);
        let months: Vec<(i32, u32, i64, i64)> = timeline
            .years
            .iter()
            .flat_map(|year| year.months.iter().map(|month| (month.year, month.month, month.count, month.start_rank)))
            .collect();

        assert_eq!(months, vec![(2024, 2, 6, 8), (2024, 3, 3, 2)]);
    }

    #[test]
    fn second_month_anchor_keeps_a_month_that_no_bucket_starts_in() {
        let dist = parse_asset_distribution_bytes(&distribution_bytes(
            604_800_000,
            &[(1_706_140_800_000, 10), (1_709_798_400_000, 4)],
        ))
        .expect("应能解析");
        assert_eq!(dist.total, 14);

        let timeline = timeline_from_distribution(&dist, DEFAULT_TZ_OFFSET_SECONDS);
        let months: Vec<(i32, u32, i64, i64)> = timeline
            .years
            .iter()
            .flat_map(|year| year.months.iter().map(|month| (month.year, month.month, month.count, month.start_rank)))
            .collect();

        assert_eq!(months, vec![(2024, 1, 10, 13), (2024, 2, 0, 4), (2024, 3, 4, 3)]);
        assert_eq!(timeline.total, 14);
    }

    #[test]
    fn empty_buckets_never_anchor_a_month() {
        let dist = parse_asset_distribution_bytes(&distribution_bytes(
            86_400_000,
            &[(1_706_659_200_000, 2), (1_706_745_600_000, 0), (1_709_251_200_000, 3)],
        ))
        .expect("应能解析");
        assert_eq!(dist.total, 5);
        assert_eq!(dist.buckets.get(1).and_then(|bucket| bucket.second_month_rank), None, "空桶没yes第二锚点");

        let timeline = timeline_from_distribution(&dist, 0);
        let months: Vec<(i32, u32, i64, i64)> = timeline
            .years
            .iter()
            .flat_map(|year| year.months.iter().map(|month| (month.year, month.month, month.count, month.start_rank)))
            .collect();

        assert_eq!(months, vec![(2024, 1, 2, 4), (2024, 3, 3, 2)]);
        assert_eq!(timeline.total, 5);
    }

    #[test]
    fn timezone_offset_decides_which_month_a_bucket_falls_into() {
        let late_january = 1_706_731_200_000;
        let dist =
            parse_asset_distribution_bytes(&distribution_bytes(3_600_000, &[(late_january, 4)])).expect("应能解析");

        let utc = timeline_from_distribution(&dist, 0);
        assert_eq!(utc.years[0].months[0], MonthEntry { year: 2024, month: 1, count: 4, start_rank: 3 });
        let china = timeline_from_distribution(&dist, DEFAULT_TZ_OFFSET_SECONDS);
        assert_eq!(china.years[0].months[0], MonthEntry { year: 2024, month: 2, count: 4, start_rank: 3 });
        assert_eq!(china.total, 4);
    }

    #[test]
    fn timeline_groups_buckets_into_ascending_years_and_months() {
        let dist = parse_asset_distribution_bytes(&distribution_bytes(
            604_800_000,
            &[(1_703_721_600_000, 2), (1_704_326_400_000, 3)],
        ))
        .expect("应能解析");
        let timeline = timeline_from_distribution(&dist, DEFAULT_TZ_OFFSET_SECONDS);
        assert_eq!(timeline.years.iter().map(|year| year.year).collect::<Vec<_>>(), vec![2023, 2024]);
        assert_eq!(timeline.years[0].count, 2);
        assert_eq!(timeline.years[1].count, 3);
        assert_eq!(timeline.years[0].months[0], MonthEntry { year: 2023, month: 12, count: 2, start_rank: 4 });
        assert_eq!(timeline.years[1].months[0], MonthEntry { year: 2024, month: 1, count: 3, start_rank: 3 });
    }

    #[test]
    fn unparseable_distribution_is_an_error() {
        assert!(parse_asset_distribution(r#"{"records":[]}"#).is_err());
        assert!(parse_asset_distribution(r#"{"records":[{"recordName":"A-1","recordType":"CPLAsset"}]}"#).is_err());
        assert!(parse_asset_distribution("not json").is_err());
        assert!(parse_asset_distribution("").is_err());

        let bad_base64 = json!({
            "records": [{
                "recordName": "PrimarySync-0000-CI-CPLAssetByAssetDateWithoutHiddenOrDeleted",
                "fields": {"cplAssetDistribution": {"value": "!!! not base64 !!!", "type": "BYTES"}}
            }]
        })
        .to_string();
        assert!(parse_asset_distribution(&bad_base64).is_err());

        let broken_protobuf = json!({
            "records": [{
                "recordName": "PrimarySync-0000-CI-CPLAssetByAssetDateWithoutHiddenOrDeleted",
                "fields": {"cplAssetDistribution": {"value": BASE64.encode([0x80u8]), "type": "BYTES"}}
            }]
        })
        .to_string();
        assert!(parse_asset_distribution(&broken_protobuf).is_err());
    }

    #[test]
    fn cross_checks_the_index_count_when_present() {
        let encoded = BASE64.encode(distribution_bytes(604_800_000, &[(1_000, 3), (2_000, 4)]));
        let body = json!({
            "records": [{
                "recordName": "PrimarySync-0000-CI-CPLAssetByAssetDateWithoutHiddenOrDeleted",
                "fields": {
                    "cplAssetDistribution": {"value": encoded, "type": "BYTES"},
                    "photosCount": {"value": 7, "type": "INT64"}
                }
            }]
        })
        .to_string();
        let parsed = parse_asset_distribution(&body).expect("应能解析");
        assert_eq!(parsed.distribution.total, 7);
        assert_eq!(parsed.library_total_hint, Some(7));

        let mismatched = json!({
            "records": [{
                "recordName": "PrimarySync-0000-CI-CPLAssetByAssetDateWithoutHiddenOrDeleted",
                "fields": {
                    "cplAssetDistribution": {"value": BASE64.encode(distribution_bytes(604_800_000, &[(1_000, 3)])), "type": "BYTES"},
                    "photosCount": {"value": 999, "type": "INT64"},
                    "videosCount": {"value": 1, "type": "INT64"}
                }
            }]
        })
        .to_string();
        let parsed = parse_asset_distribution(&mismatched).expect("计数不一致不该让解析失败");
        assert_eq!(parsed.distribution.total, 3);
        assert_eq!(parsed.library_total_hint, Some(1_000));

        assert_eq!(
            parse_asset_distribution(&distribution_body(604_800_000, &[(1_000, 3)]))
                .expect("应能解析")
                .library_total_hint,
            None
        );
    }

    #[test]
    fn timeline_dto_serializes_as_camel_case() {
        let dist = parse_asset_distribution_bytes(&distribution_bytes(604_800_000, &[(1_000, 3)])).expect("应能解析");
        let timeline = timeline_from_distribution(&dist, DEFAULT_TZ_OFFSET_SECONDS);
        let dto = PhotosTimelineDto { distribution: (&dist).into(), timeline: (&timeline).into() };
        let value = serde_json::to_value(&dto).expect("应能序列化");
        assert_eq!(value["distribution"]["bucketWidthMs"], json!(604_800_000));
        assert_eq!(value["distribution"]["total"], json!(3));
        assert_eq!(value["distribution"]["buckets"][0]["startMs"], json!(1_000));
        assert_eq!(value["distribution"]["buckets"][0]["count"], json!(3));
        assert_eq!(value["distribution"]["buckets"][0]["startRank"], json!(2));
        assert_eq!(value["distribution"]["buckets"][0]["secondMonthRank"], json!(0));
        assert_eq!(value["timeline"]["total"], json!(3));
        assert_eq!(value["timeline"]["years"][0]["months"][0]["startRank"], json!(2));
    }

    #[test]
    fn timeline_request_only_needs_the_session_id() {
        let request: PhotosTimelineRequest = serde_json::from_str(r#"{"sessionId":"s-1"}"#).expect("应能反序列化");
        assert_eq!(request.session_id, "s-1");
    }

    fn local_ms(year: i32, month: u32, day: u32, hour: u32) -> i64 {
        days_from_civil(year, month, day) * MS_PER_DAY - i64::from(DEFAULT_TZ_OFFSET_SECONDS) * 1_000
            + i64::from(hour) * 3_600_000
    }

    fn march_2024_buckets() -> [(i64, i64); 5] {
        [
            (1_709_164_800_000, 5),
            (1_709_769_600_000, 11),
            (1_710_374_400_000, 6),
            (1_710_979_200_000, 73),
            (1_711_584_000_000, 6),
        ]
    }

    fn march_2024_distribution() -> AssetDistribution {
        parse_asset_distribution_bytes(&distribution_bytes(604_800_000, &march_2024_buckets())).expect("应能解析")
    }

    fn dist_with_boundary_count(count: i64) -> AssetDistribution {
        parse_asset_distribution_bytes(&distribution_bytes(604_800_000, &[(1_709_164_800_000, count)]))
            .expect("应能解析")
    }

    #[test]
    fn month_plan_adds_inside_weeks_directly_and_fetches_only_the_two_boundaries() {
        let dist = march_2024_distribution();
        assert_eq!(dist.total, 101);
        let plan = plan_month(&dist, DEFAULT_TZ_OFFSET_SECONDS, 2024, 3).expect("应能排出计划");

        assert_eq!(plan.inside_count, 90);

        assert_eq!(plan.inside_newest_rank, Some(6));
        assert_eq!(plan.inside_oldest_rank, Some(95));

        assert_eq!(plan.boundaries.len(), 2, "3 月只yes月初、月末两个跨月周");
        assert_eq!(plan.boundaries[0].start_rank, 100);
        assert_eq!(plan.boundaries[0].count, 5);
        assert_eq!(plan.boundaries[1].start_rank, 5);
        assert_eq!(plan.boundaries[1].count, 6);

        assert_eq!(plan.boundaries[0].requests, vec![(100, 5)]);
        assert_eq!(plan.boundaries[1].requests, vec![(5, 6)]);
        assert_eq!(plan.pages(), 2);

        assert_eq!(plan.month_start_ms, local_ms(2024, 3, 1, 0));
        assert_eq!(plan.month_end_ms, local_ms(2024, 4, 1, 0));
    }

    #[test]
    fn boundary_weeks_are_classified_by_their_own_asset_date() {
        let plan = plan_month(&march_2024_distribution(), DEFAULT_TZ_OFFSET_SECONDS, 2024, 3).expect("应能排出计划");
        let fetched = vec![
            vec![
                local_ms(2024, 2, 29, 21),
                local_ms(2024, 3, 1, 8),
                local_ms(2024, 3, 5, 20),
                local_ms(2024, 3, 6, 9),
                local_ms(2024, 3, 7, 12),
            ],
            vec![
                local_ms(2024, 3, 28, 9),
                local_ms(2024, 3, 29, 6),
                local_ms(2024, 3, 30, 7),
                local_ms(2024, 3, 31, 23),
                local_ms(2024, 4, 2, 18),
                local_ms(2024, 4, 3, 10),
            ],
        ];
        let exact = finish_month(&plan, &fetched).expect("应能算出精确值");
        assert_eq!(exact, MonthExact { year: 2024, month: 3, count: 98, newest_rank: 2, oldest_rank: 99 });

        assert_ne!(exact.count, 95);
    }

    #[test]
    fn month_window_edges_are_left_closed_right_open() {
        let plan = plan_month(&dist_with_boundary_count(4), DEFAULT_TZ_OFFSET_SECONDS, 2024, 3).expect("应能排出计划");
        let fetched = vec![vec![
            local_ms(2024, 2, 28, 12),
            local_ms(2024, 2, 29, 23),
            local_ms(2024, 3, 1, 0),
            local_ms(2024, 3, 7, 7),
        ]];
        let exact = finish_month(&plan, &fetched).expect("应能算出精确值");
        assert_eq!(exact.count, 2);

        assert_eq!(exact.newest_rank, 0);
        assert_eq!(exact.oldest_rank, 1);

        let ending = parse_asset_distribution_bytes(&distribution_bytes(604_800_000, &[(1_711_584_000_000, 3)]))
            .expect("应能解析");
        let plan = plan_month(&ending, DEFAULT_TZ_OFFSET_SECONDS, 2024, 3).expect("应能排出计划");
        assert_eq!(plan.inside_count, 0);
        let fetched = vec![vec![local_ms(2024, 3, 31, 10), local_ms(2024, 4, 1, 0), local_ms(2024, 4, 1, 1)]];
        let exact = finish_month(&plan, &fetched).expect("应能算出精确值");
        assert_eq!(exact.count, 1);
        assert_eq!(exact.newest_rank, 2);
        assert_eq!(exact.oldest_rank, 2);
    }

    #[test]
    fn whole_month_inside_one_bucket_is_fetched_exactly_once() {
        let dist = parse_asset_distribution_bytes(&distribution_bytes(4_000_000_000, &[(1_708_387_200_000, 12)]))
            .expect("应能解析");
        let plan = plan_month(&dist, DEFAULT_TZ_OFFSET_SECONDS, 2024, 3).expect("应能排出计划");
        assert_eq!(plan.inside_count, 0, "没yes任何整块落在 3 月内的桶");
        assert_eq!(plan.boundaries.len(), 1, "同一个桶只算一个边界（不必取两遍）");
        assert_eq!(plan.pages(), 1);

        let mut dates = vec![
            local_ms(2024, 2, 24, 10),
            local_ms(2024, 2, 25, 10),
            local_ms(2024, 2, 26, 10),
            local_ms(2024, 2, 27, 10),
            local_ms(2024, 2, 28, 10),
        ];
        dates.extend((8..=14).map(|day| local_ms(2024, 3, day, 10)));
        let exact = finish_month(&plan, &[dates]).expect("应能算出精确值");

        assert_eq!(exact.count, 7);

        assert_eq!(exact.newest_rank, 0);
        assert_eq!(exact.oldest_rank, 6);
    }

    #[test]
    fn month_borders_cross_the_year_boundary() {
        let tz = i64::from(DEFAULT_TZ_OFFSET_SECONDS) * 1_000;
        let (dec_start, dec_end) = month_window(2023, 12, tz).expect("应能算月窗口");
        let (jan_start, jan_end) = month_window(2024, 1, tz).expect("应能算月窗口");
        assert_eq!(dec_start, 1_701_360_000_000);
        assert_eq!(dec_end, 1_704_038_400_000);
        assert_eq!(jan_start, dec_end, "12 月的上界等于 1 月的下界");
        assert_eq!(jan_end, 1_706_716_800_000);

        let crossing = 1_703_721_600_000;
        let inside_january = 1_704_326_400_000;
        let dist =
            parse_asset_distribution_bytes(&distribution_bytes(604_800_000, &[(crossing, 4), (inside_january, 3)]))
                .expect("应能解析");
        assert_eq!(dist.total, 7);

        let fetched = vec![vec![
            local_ms(2023, 12, 30, 10),
            local_ms(2023, 12, 31, 10),
            local_ms(2024, 1, 1, 10),
            local_ms(2024, 1, 2, 10),
        ]];

        let december = plan_month(&dist, DEFAULT_TZ_OFFSET_SECONDS, 2023, 12).expect("应能排出计划");
        assert_eq!(december.inside_count, 0);
        assert_eq!(december.boundaries.len(), 1);
        assert_eq!(december.boundaries[0].start_rank, 6);
        let exact = finish_month(&december, &fetched).expect("应能算出精确值");

        assert_eq!(exact, MonthExact { year: 2023, month: 12, count: 2, newest_rank: 5, oldest_rank: 6 });

        let january = plan_month(&dist, DEFAULT_TZ_OFFSET_SECONDS, 2024, 1).expect("应能排出计划");
        assert_eq!(january.inside_count, 3);
        assert_eq!(january.boundaries.len(), 1);
        let exact = finish_month(&january, &fetched).expect("应能算出精确值");
        assert_eq!(exact.count, 5, "3（内部整周）+ 2（跨年周里落在 1 月的那两张）");

        assert_eq!(exact.newest_rank, 0);
        assert_eq!(exact.oldest_rank, 4);

        let (dec, jan) = (finish_month(&december, &fetched).unwrap(), finish_month(&january, &fetched).unwrap());
        assert_eq!(dec.count + jan.count, dist.total);
    }

    #[test]
    fn month_exact_is_an_error_when_the_page_budget_is_exceeded() {
        let at_limit = plan_month(&dist_with_boundary_count(1200), DEFAULT_TZ_OFFSET_SECONDS, 2024, 3)
            .expect("1200 张 = 20 页，正好在上限内");
        assert_eq!(at_limit.pages(), 20);

        let over = plan_month(&dist_with_boundary_count(1201), DEFAULT_TZ_OFFSET_SECONDS, 2024, 3);
        assert!(over.is_err(), "1201 张需要 21 页，必须 Err 而不是截断");

        let two_big = parse_asset_distribution_bytes(&distribution_bytes(
            604_800_000,
            &[(1_709_164_800_000, 700), (1_711_584_000_000, 700)],
        ))
        .expect("应能解析");
        assert!(plan_month(&two_big, DEFAULT_TZ_OFFSET_SECONDS, 2024, 3).is_err());

        let busiest =
            plan_month(&dist_with_boundary_count(243), DEFAULT_TZ_OFFSET_SECONDS, 2024, 3).expect("243 张不该超上限");
        assert_eq!(busiest.boundaries[0].requests, vec![(242, 60), (182, 60), (122, 60), (62, 60), (2, 3)]);
        assert_eq!(busiest.pages(), 5);
    }

    #[test]
    fn inconsistent_boundary_fetch_is_an_error() {
        let plan = plan_month(&dist_with_boundary_count(5), DEFAULT_TZ_OFFSET_SECONDS, 2024, 3).expect("应能排出计划");
        assert!(finish_month(&plan, &[vec![local_ms(2024, 3, 2, 1)]]).is_err(), "只取回 1 张但直方图说 5 张");
        assert!(finish_month(&plan, &[]).is_err(), "少给了一个边界周的数据");

        let no_width =
            parse_asset_distribution_bytes(&distribution_bytes(0, &[(1_709_164_800_000, 5)])).expect("应能解析");
        assert!(plan_month(&no_width, DEFAULT_TZ_OFFSET_SECONDS, 2024, 3).is_err());
    }

    #[test]
    fn invalid_month_or_year_is_an_error() {
        let dist = dist_with_boundary_count(5);
        assert!(plan_month(&dist, DEFAULT_TZ_OFFSET_SECONDS, 2024, 0).is_err());
        assert!(plan_month(&dist, DEFAULT_TZ_OFFSET_SECONDS, 2024, 13).is_err());
        assert!(plan_month(&dist, DEFAULT_TZ_OFFSET_SECONDS, 999, 3).is_err());
        assert!(month_window(2024, 13, 0).is_err());
    }

    #[test]
    fn days_from_civil_inverts_civil_from_days() {
        for (year, month, day) in
            [(1970, 1, 1), (2000, 2, 29), (2024, 3, 1), (2024, 12, 31), (2023, 3, 1), (9999, 12, 31)]
        {
            let days = days_from_civil(year, month, day);
            assert_eq!(civil_from_days(days), (year, month, day), "{year}-{month}-{day} 来回一趟必须一样");
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2024, 3, 1), 19_783);
    }

    #[test]
    fn month_exact_dto_serializes_as_camel_case() {
        let exact = MonthExact { year: 2024, month: 3, count: 98, newest_rank: 3, oldest_rank: 100 };
        let value = serde_json::to_value(MonthExactDto::from(&exact)).expect("应能序列化");
        assert_eq!(value["year"], json!(2024));
        assert_eq!(value["month"], json!(3));
        assert_eq!(value["count"], json!(98));
        assert_eq!(value["newestRank"], json!(3));
        assert_eq!(value["oldestRank"], json!(100));

        let request: PhotosMonthExactRequest =
            serde_json::from_str(r#"{"sessionId":"s-1","year":2024,"month":3}"#).expect("应能反序列化");
        assert_eq!(request.session_id, "s-1");
        assert_eq!(request.year, 2024);
        assert_eq!(request.month, 3);
    }
}
