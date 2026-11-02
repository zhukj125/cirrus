use reqwest::{Method, Url};
use serde::Deserialize;

use crate::apple::api;
use crate::apple::http::headers as hh;
use crate::error::{AppError, AppResult};
use crate::models::session_keys::FMIP_CLIENT_ID;
use crate::models::{statuses, StorageQuotaStatus, StorageUsageCategory, StorageUsageResult};
use crate::services::devices;
use crate::session::{SessionHandle, SrpSession};

const STORAGE_CLIENT_BUILD: &str = "2640Build24";

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawStorageUsage {
    storage_usage_by_media: Vec<RawStorageUsageCategory>,
    storage_usage_info: RawStorageUsageInfo,
    quota_status: RawQuotaStatus,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawStorageUsageCategory {
    media_key: String,
    display_label: String,
    display_color: String,
    usage_in_bytes: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawStorageUsageInfo {
    comp_storage_in_bytes: u64,
    used_storage_in_bytes: u64,
    total_storage_in_bytes: u64,
    commerce_storage_in_bytes: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct RawQuotaStatus {
    over_quota: bool,
    have_max_quota_tier: bool,
    #[serde(rename = "almost-full")]
    almost_full: bool,
    paid_quota: bool,
}

pub async fn get_usage(handle: &SessionHandle) -> StorageUsageResult {
    let mut session = handle.lock().await;
    if let Err(error) = devices::account_login(&mut session, false).await {
        log::warn!("[STORAGE] accountLogin failed: {error}");
        return StorageUsageResult::error(error.to_string());
    }

    match fetch_usage(&session).await {
        Ok(result) => {
            log::info!(
                "[STORAGE] loaded usage (used={} total={} categories={})",
                result.used_storage_in_bytes,
                result.total_storage_in_bytes,
                result.storage_usage_by_media.len()
            );
            handle.touch();
            result
        }
        Err(error) => {
            log::warn!("[STORAGE] storageUsageInfo failed: {error}");
            StorageUsageResult::error(error.to_string())
        }
    }
}

async fn fetch_usage(session: &SrpSession) -> AppResult<StorageUsageResult> {
    let url = storage_usage_url(session)?;
    let headers = [(hh::ACCEPT, "*/*")];
    let response = session.request_shared(Method::POST, url.as_str(), None, &headers).await?;
    if response.status != 200 {
        return Err(AppError::auth(format!(
            "storageUsageInfo failed: HTTP {} {}",
            response.status,
            short(&response.body, 300)
        )));
    }
    parse_storage_usage(&response.body, manage_url(session))
}

fn storage_usage_url(session: &SrpSession) -> AppResult<Url> {
    let dsid = session
        .dsid()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::auth("Session is missing a dsid; storage usage cannot be queried"))?;
    let client_id = session
        .session_data(FMIP_CLIENT_ID)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| session.client_id());

    let endpoint = format!("{}{}", session.setup_base().trim_end_matches('/'), api::setup::STORAGE_USAGE_INFO);
    let mut url =
        Url::parse(&endpoint).map_err(|error| AppError::other(format!("Invalid iCloud storage URL: {error}")))?;
    url.query_pairs_mut()
        .append_pair("clientBuildNumber", STORAGE_CLIENT_BUILD)
        .append_pair("clientMasteringNumber", STORAGE_CLIENT_BUILD)
        .append_pair("clientId", client_id)
        .append_pair("dsid", dsid);
    Ok(url)
}

fn manage_url(session: &SrpSession) -> String {
    format!("{}/storage/", session.home_url().trim_end_matches('/'))
}

fn parse_storage_usage(body: &str, manage_url: String) -> AppResult<StorageUsageResult> {
    let usage: RawStorageUsage = serde_json::from_str(body)
        .map_err(|error| AppError::auth(format!("Could not parse storageUsageInfo: {error}")))?;
    Ok(StorageUsageResult {
        status: statuses::OK.to_string(),
        message: None,
        storage_usage_by_media: usage
            .storage_usage_by_media
            .into_iter()
            .map(|category| StorageUsageCategory {
                media_key: category.media_key.clone(),
                display_label: if category.display_label.trim().is_empty() {
                    category.media_key
                } else {
                    category.display_label
                },
                display_color: category.display_color,
                usage_in_bytes: category.usage_in_bytes,
            })
            .collect(),
        used_storage_in_bytes: usage.storage_usage_info.used_storage_in_bytes,
        total_storage_in_bytes: usage.storage_usage_info.total_storage_in_bytes,
        commerce_storage_in_bytes: usage.storage_usage_info.commerce_storage_in_bytes,
        comp_storage_in_bytes: usage.storage_usage_info.comp_storage_in_bytes,
        quota_status: StorageQuotaStatus {
            over_quota: usage.quota_status.over_quota,
            have_max_quota_tier: usage.quota_status.have_max_quota_tier,
            almost_full: usage.quota_status.almost_full,
            paid_quota: usage.quota_status.paid_quota,
        },
        manage_url: Some(manage_url),
    })
}

fn short(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let short: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{short}…")
    } else {
        short
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const CAPTURED_STORAGE_RESPONSE: &str = r#"{
        "storageUsageByMedia": [
            {"mediaKey":"photos","displayLabel":"照片和视频","displayColor":"ffcc00","usageInBytes":53464975543},
            {"mediaKey":"backup","displayLabel":"备份","displayColor":"5856d6","usageInBytes":0},
            {"mediaKey":"docs","displayLabel":"文稿","displayColor":"ff9500","usageInBytes":140625035}
        ],
        "storageUsageInfo": {
            "compStorageInBytes":0,
            "usedStorageInBytes":53687091200,
            "totalStorageInBytes":53687091200,
            "commerceStorageInBytes":53687091200
        },
        "quotaStatus": {
            "overQuota":true,
            "haveMaxQuotaTier":false,
            "almost-full":true,
            "paidQuota":true
        }
    }"#;

    #[test]
    fn parses_the_captured_storage_response() {
        let result = parse_storage_usage(CAPTURED_STORAGE_RESPONSE, "https://www.icloud.com.cn/storage/".to_string())
            .expect("storage usage");

        assert_eq!(result.status, statuses::OK);
        assert_eq!(result.used_storage_in_bytes, 53_687_091_200);
        assert_eq!(result.total_storage_in_bytes, 53_687_091_200);
        assert_eq!(result.storage_usage_by_media.len(), 3);
        assert_eq!(result.storage_usage_by_media[0].media_key, "photos");
        assert_eq!(result.storage_usage_by_media[0].usage_in_bytes, 53_464_975_543);
        assert!(result.quota_status.over_quota);
        assert!(result.quota_status.almost_full);
        assert!(result.quota_status.paid_quota);
        assert_eq!(result.manage_url.as_deref(), Some("https://www.icloud.com.cn/storage/"));
    }

    #[test]
    fn builds_the_regional_storage_url_with_encoded_query_values() {
        let mut session = SrpSession::with_endpoints(
            "person@example.com".to_string(),
            None,
            "https://idmsa.apple.com.cn/appleauth/auth",
            "https://setup.icloud.com.cn/setup/ws/1",
            "https://www.icloud.com.cn",
        )
        .expect("session");
        session.set_dsid(Some("489338457".to_string()));
        session.set_session_data(FMIP_CLIENT_ID, "client/with space");

        let url = storage_usage_url(&session).expect("storage URL");
        assert_eq!(url.host_str(), Some("setup.icloud.com.cn"));
        assert_eq!(url.path(), "/setup/ws/1/storageUsageInfo");
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query.get("clientId").map(String::as_str), Some("client/with space"));
        assert_eq!(query.get("dsid").map(String::as_str), Some("489338457"));
        assert_eq!(query.get("clientBuildNumber").map(String::as_str), Some(STORAGE_CLIENT_BUILD));
    }
}
