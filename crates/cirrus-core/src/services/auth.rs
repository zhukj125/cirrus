use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::Method;
use serde_json::Value;

use crate::apple::api;
use crate::apple::http::{client as hc, headers as hh};
use crate::apple::wire::{
    ServiceErrorResponse, SigninChallengeResponse, SigninCompleteRequest, SigninInitRequest, SigninInitResponse,
};
use crate::crypto;
use crate::error::{AppError, AppResult};
use crate::models::session_keys::*;
use crate::models::{statuses, SrpAuthRequest, SrpInitResult, SrpResult};
use crate::session::{now_millis, SessionHandle};

pub async fn start_srp_auth(handle: &SessionHandle, request: &SrpAuthRequest) -> AppResult<SrpInitResult> {
    let mut session = handle.lock().await;
    log::info!("[SRP] starting authentication for appleId={}", mask(&request.apple_id));

    let bootstrap_url = format!("{}{}", session.auth_base(), api::auth::BOOTSTRAP);
    let bootstrap = session.request(Method::GET, &bootstrap_url, None, &[(hh::ACCEPT, hc::ACCEPT_HTML)]).await?;
    log::info!(
        "[SRP] session bootstrap: HTTP {} scnt={} authAttributes={}",
        bootstrap.status,
        session.session_data(SCNT).is_some(),
        session.session_data(AUTH_ATTRIBUTES).is_some()
    );
    let bootstrap_body = bootstrap.body.clone();
    if !bootstrap_body.is_empty() {
        session.set_session_data(BOOTSTRAP_BODY, bootstrap_body.clone());
        if let Some(push_token) = extract_push_token(&bootstrap_body) {
            let len = push_token.len();
            session.set_session_data(PUSH_TOKEN, push_token);
            log::info!("[SRP] extracted pushToken (len={len})");
        }
    }

    let private_key_a = crypto::random_private_key();
    let a_bytes = crypto::public_key_a(&private_key_a);
    session.set_private_key_a(private_key_a);
    session.set_a_bytes(a_bytes.clone());

    let payload = SigninInitRequest {
        a: BASE64.encode(&a_bytes),
        account_name: request.apple_id.clone(),
        protocols: vec![hc::PROTOCOL_S2K.to_string(), hc::PROTOCOL_S2K_FO.to_string()],
    };
    let payload = serde_json::to_value(&payload).map_err(AppError::from)?;
    let init_headers = session.oauth_headers();
    let init_headers: Vec<(&str, &str)> = init_headers.iter().map(|(name, value)| (*name, value.as_str())).collect();
    let url = format!("{}{}", session.auth_base(), api::auth::SIGNIN_INIT);
    let response = session.request(Method::POST, &url, Some(&payload), &init_headers).await?;

    if response.status != 200 {
        return Err(handle_error(&response));
    }

    let data: SigninInitResponse = serde_json::from_str(&response.body)
        .map_err(|error| AppError::auth(format!("Could not parse response (HTTP {}): {error}", response.status)))?;
    log::info!("[SRP] init complete: protocol={:?} iteration={:?}", data.protocol, data.iteration);
    let result = SrpInitResult {
        session_id: session.id().to_string(),
        protocol: data.protocol.clone(),
        iteration: data.iteration,
        salt: data.salt.clone(),
        b: data.b.clone(),
        c: data.c.clone(),
    };
    session.set_init_data(data);
    Ok(result)
}

pub async fn complete_srp_auth(handle: &SessionHandle, session_id: &str) -> AppResult<SrpResult> {
    let mut session = handle.lock().await;

    let init_data = session
        .init_data()
        .cloned()
        .ok_or_else(|| AppError::auth("Session is missing init data. Call startSrpAuth first."))?;

    let (Some(salt_b64), Some(b_b64), Some(server_session_id), Some(iterations), Some(protocol)) = (
        init_data.salt.clone(),
        init_data.b.clone(),
        init_data.c.clone(),
        init_data.iteration,
        init_data.protocol.clone(),
    ) else {
        return Err(AppError::auth("Init response is missing required fields (salt/b/c/iteration/protocol)"));
    };

    let salt = BASE64
        .decode(salt_b64.as_bytes())
        .map_err(|error| AppError::auth(format!("Could not decode salt base64: {error}")))?;
    let b_bytes = BASE64
        .decode(b_b64.as_bytes())
        .map_err(|error| AppError::auth(format!("Could not decode B base64: {error}")))?;

    let password = session
        .password()
        .ok_or_else(|| {
            AppError::auth(
                "Session is missing the password (authentication may already be complete). Please sign in again.",
            )
        })?
        .to_string();
    let derived = match protocol.as_str() {
        hc::PROTOCOL_S2K => crypto::derive_s2k(&password, &salt, iterations),
        hc::PROTOCOL_S2K_FO => crypto::derive_s2k_fo(&password, &salt, iterations),
        other => return Err(AppError::auth(format!("Unsupported protocol: {other}"))),
    };

    let a_bytes = session
        .a_bytes()
        .ok_or_else(|| AppError::auth("Session is missing client public key A. Call startSrpAuth first."))?
        .to_vec();
    let private_key_a = session
        .private_key_a()
        .cloned()
        .ok_or_else(|| AppError::auth("Session is missing client private key a. Call startSrpAuth first."))?;

    let x = crypto::compute_x(&salt, &derived);
    let u = crypto::compute_u(&a_bytes, &b_bytes);
    let k_bytes = crypto::compute_s(&private_key_a, &a_bytes, &b_bytes, &x, &u);
    let m1 = crypto::compute_m1(session.apple_id(), &salt, &a_bytes, &b_bytes, &k_bytes);
    let m2 = crypto::compute_m2(&a_bytes, &m1, &k_bytes);
    log::info!("[SRP] password derivation and M1/M2 complete (protocol={protocol}, iterations={iterations})");

    let payload = SigninCompleteRequest {
        account_name: session.apple_id().to_string(),
        c: server_session_id,
        m1: BASE64.encode(&m1),
        m2: BASE64.encode(&m2),
        remember_me: true,
        trust_tokens: Vec::new(),
    };
    let payload = serde_json::to_value(&payload).map_err(AppError::from)?;
    let complete_headers = session.auth_headers();
    let complete_headers: Vec<(&str, &str)> =
        complete_headers.iter().map(|(name, value)| (*name, value.as_str())).collect();
    let url = format!("{}{}?isRememberMeEnabled=true", session.auth_base(), api::auth::SIGNIN_COMPLETE);
    let response = session.request(Method::POST, &url, Some(&payload), &complete_headers).await?;

    session.clear_password();
    let _ = session_id;

    if response.status == 409 {
        let challenge: SigninChallengeResponse = serde_json::from_str(&response.body).unwrap_or_default();
        let data = response.json();
        session.set_session_data(CHALLENGE_BODY, data.to_string());
        log::info!(
            "[SRP] returning challenge: authType={:?} body={}",
            challenge.auth_type,
            truncate(&redact_tokens(&data.to_string()), 1600)
        );

        for (name, value) in response.headers.iter() {
            let lower = name.as_str().to_ascii_lowercase();
            if lower.contains("apple") || lower == "scnt" {
                log::info!("[SRP] 409 header: {} (len={})", name, value.to_str().map(str::len).unwrap_or(0));
            }
        }
        return Ok(SrpResult::challenge(
            session.id().to_string(),
            challenge.auth_type,
            session.session_data(SESSION_TOKEN).map(str::to_string),
            session.session_data(TRUST_TOKEN).map(str::to_string),
            session.session_data(SESSION_ID).map(str::to_string),
            data,
        ));
    }
    if response.status != 200 {
        return Err(handle_error(&response));
    }

    let data = response.json();
    let result = SrpResult::success(
        session.id().to_string(),
        session.session_data(SESSION_TOKEN).map(str::to_string),
        session.session_data(TRUST_TOKEN).map(str::to_string),
        session.session_data(SESSION_ID).map(str::to_string),
        data.get(api::json::SUCCESS).and_then(Value::as_bool),
        data,
    );
    log::info!("[SRP] authentication succeeded");
    Ok(result)
}

pub async fn authenticate(
    manager: &crate::session_manager::SessionManager,
    request: &SrpAuthRequest,
) -> AppResult<SrpResult> {
    let handle = manager.create(request.apple_id.clone(), request.password.clone(), request.china_mainland).await?;
    let session_id = handle.lock().await.id().to_string();
    let result = async {
        let init = start_srp_auth(&handle, request).await?;
        complete_srp_auth(&handle, &init.session_id).await
    }
    .await;
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            manager.remove(&session_id).await;
            return Err(error);
        }
    };
    handle.touch();

    if result.status == statuses::CHALLENGE {
        return Ok(SrpResult {
            status: statuses::TWO_FA_REQUIRED.to_string(),
            message: Some("Two-factor authentication (hsa2) is enabled. Device queries do not require 2FA and can continue directly.".to_string()),
            session_id: Some(session_id),
            auth_type: result.auth_type,
            session_token: result.session_token,
            trust_token: result.trust_token,
            apple_session_id: result.apple_session_id,
            success: None,
            details: result.details,
        });
    }
    Ok(result)
}

pub fn handle_error(response: &crate::session::HttpResponse) -> AppError {
    let mut message = response.body.clone();
    let mut error_code = String::new();
    if let Ok(parsed) = serde_json::from_str::<ServiceErrorResponse>(&response.body) {
        if let Some(text) = parsed.message() {
            message = text.to_string();
        }
        if let Some(code) = parsed.code() {
            error_code = code.to_string();
        }
    }
    let code_part = if error_code.is_empty() { String::new() } else { format!(" ({error_code})") };
    AppError::auth(format!("HTTP {}{code_part}: {message}", response.status))
}

pub fn extract_push_token(html: &str) -> Option<String> {
    let pattern = regex::Regex::new(r#""pushToken"\s*:\s*"([^"]+)""#).ok()?;
    pattern.captures(html).and_then(|captures| captures.get(1)).map(|matched| matched.as_str().to_string())
}

fn mask(apple_id: &str) -> String {
    if apple_id.chars().count() < 4 {
        return "***".to_string();
    }
    let prefix: String = apple_id.chars().take(3).collect();
    format!("{prefix}***")
}

pub fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() > limit {
        let truncated: String = value.chars().take(limit).collect();
        format!("{truncated}…")
    } else {
        value.to_string()
    }
}

pub fn redact_tokens(text: &str) -> String {
    const PATTERN: &str = r#"(?i)"(session_?token|trust_?token|dsWebAuthToken|password)"\s*:\s*"[^"]*""#;
    match regex::Regex::new(PATTERN) {
        Ok(pattern) => pattern.replace_all(text, r#""$1":"***""#).to_string(),

        Err(_) => "<omitted>".to_string(),
    }
}

pub fn timestamp_millis() -> i64 {
    now_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_push_token_from_bootstrap_html() {
        let html = r#"<script>var x = {"pushToken":"abc123","other":1};</script>"#;
        assert_eq!(extract_push_token(html).as_deref(), Some("abc123"));
        assert_eq!(extract_push_token("no token here"), None);
    }

    #[test]
    fn handle_error_prefers_apple_error_message() {
        let response = crate::session::HttpResponse {
            status: 401,
            headers: reqwest::header::HeaderMap::new(),
            body: r#"{"errorCode":"AUTHENTICATION_FAILED","errorMessage":"密码错误"}"#.to_string(),
        };
        let error = handle_error(&response);
        assert!(error.to_string().contains("AUTHENTICATION_FAILED"));
        assert!(error.to_string().contains("密码错误"));
    }

    #[test]
    fn mask_hides_most_of_the_apple_id() {
        assert_eq!(mask("user@example.com"), "use***");
        assert_eq!(mask("ab"), "***");
    }

    #[test]
    fn redact_tokens_hides_credentials_but_keeps_shape() {
        let body = r#"{"authType":"hsa2","sessionToken":"ABC123","trustToken":"XYZ","trustedDeviceCount":3}"#;
        let redacted = redact_tokens(body);
        assert!(!redacted.contains("ABC123"));
        assert!(!redacted.contains("XYZ"));

        assert!(redacted.contains(r#""sessionToken":"***""#));
        assert!(redacted.contains(r#""trustToken":"***""#));
        assert!(redacted.contains(r#""authType":"hsa2""#));
        assert!(redacted.contains(r#""trustedDeviceCount":3"#));
    }
}
