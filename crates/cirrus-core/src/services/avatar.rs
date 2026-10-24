use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::Method;
use reqwest::Url;

use crate::apple::http::{client as hc, headers as hh};
use crate::apple::wire::MeCardResponse;
use crate::apple::{api, http::headers};
use crate::error::{AppError, AppResult};
use crate::models::AccountAvatar;
use crate::services::devices;
use crate::session::SessionHandle;

const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024;
const DEFAULT_IMAGE_MIME: &str = "image/jpeg";

fn is_apple_avatar_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    matches!(url.scheme(), "https")
        && url.host_str().is_some_and(|host| {
            host == "icloud.com"
                || host.ends_with(".icloud.com")
                || host == "icloud.com.cn"
                || host.ends_with(".icloud.com.cn")
        })
}

pub async fn fetch(handle: &SessionHandle) -> AppResult<AccountAvatar> {
    log::info!("[AVATAR] loading account avatar");

    let (mut session, contacts_url, dsid, client_id) = {
        let mut shared = handle.lock().await;
        if shared.contacts_url().is_none() {
            devices::account_login(&mut shared, false).await?;
        }
        let contacts_url = shared
            .contacts_url()
            .ok_or_else(|| AppError::other("accountLogin did not return a contacts service URL"))?
            .trim_end_matches('/')
            .to_string();
        let dsid = shared.dsid().ok_or_else(|| AppError::auth("Missing DSID; avatar cannot be loaded"))?.to_string();
        let client_id = shared.client_id().to_string();
        let mut snapshot = shared.clone();
        snapshot.clear_password();
        (snapshot, contacts_url, dsid, client_id)
    };
    let mecard_url = format!(
        "{contacts_url}{}?clientBuildNumber={}&clientMasteringNumber={}&clientId={client_id}&dsid={dsid}",
        api::contacts::MECARD,
        hc::CONTACTS_CLIENT_BUILD_NUMBER,
        hc::CONTACTS_CLIENT_MASTERING_NUMBER,
    );
    let response = session.request(Method::GET, &mecard_url, None, &[]).await?;
    if !response.is_success() {
        return Err(AppError::other(format!("Could not load contact card: HTTP {}", response.status)));
    }
    let card: MeCardResponse = serde_json::from_str(&response.body)
        .map_err(|error| AppError::other(format!("Could not parse contact card: {error}")))?;
    let Some(photo_url) = card.photo_url() else {
        return Ok(AccountAvatar { data_url: String::new() });
    };
    if !is_apple_avatar_url(photo_url) {
        return Err(AppError::other("Avatar URL is not on an Apple domain"));
    }

    let image = session.request_bytes(Method::GET, photo_url, None, &[(hh::ACCEPT, "image/*")]).await?;
    if !image.is_success() {
        return Err(AppError::other(format!("Could not download avatar: HTTP {}", image.status)));
    }
    if image.bytes.len() > MAX_AVATAR_BYTES {
        return Err(AppError::other("Avatar file is too large"));
    }
    let mime = image
        .header(headers::CONTENT_TYPE)
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| value.starts_with("image/"))
        .unwrap_or(DEFAULT_IMAGE_MIME);
    log::info!("[AVATAR] avatar loaded ({} bytes)", image.bytes.len());
    let data_url = format!("data:{mime};base64,{}", BASE64.encode(&image.bytes));
    Ok(AccountAvatar { data_url })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_apple_https_avatar_urls_are_allowed() {
        assert!(is_apple_avatar_url("https://gateway.icloud.com.cn/contacts/1/ck/card/abc"));
        assert!(is_apple_avatar_url("https://p01-contactsws.icloud.com/co/x"));
        assert!(!is_apple_avatar_url("http://gateway.icloud.com.cn/contacts/1"));
        assert!(!is_apple_avatar_url("https://evil.example.com/avatar.jpg"));
    }

    #[test]
    fn mecard_photo_url_is_extracted() {
        let card: MeCardResponse = serde_json::from_str(
            r#"{"contacts":[{"photo":{"url":"https://gateway.icloud.com.cn/contacts/1/ck/card/abc"}}]}"#,
        )
        .expect("解析失败");
        assert_eq!(card.photo_url(), Some("https://gateway.icloud.com.cn/contacts/1/ck/card/abc"));
    }
}
