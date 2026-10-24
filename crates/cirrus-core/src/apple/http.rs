pub mod headers {
    pub const USER_AGENT: &str = "User-Agent";
    pub const ORIGIN: &str = "Origin";
    pub const REFERER: &str = "Referer";
    pub const ACCEPT: &str = "Accept";
    pub const CONTENT_TYPE: &str = "Content-Type";

    pub const SCNT: &str = "scnt";
    pub const ID_SESSION_ID: &str = "X-Apple-ID-Session-Id";
    pub const AUTH_ATTRIBUTES: &str = "X-Apple-Auth-Attributes";
    pub const SESSION_TOKEN: &str = "X-Apple-Session-Token";
    pub const TRUST_TOKEN: &str = "X-Apple-TwoSV-Trust-Token";
    pub const ACCOUNT_COUNTRY: &str = "X-Apple-ID-Account-Country";
    pub const RSCD: &str = "X-Apple-I-Rscd";
    pub const PUSH_TOKEN: &str = "X-Apple-Push-Token";
    pub const FRAME_ID: &str = "X-Apple-Frame-Id";

    pub const OAUTH_CLIENT_ID: &str = "X-Apple-OAuth-Client-Id";
    pub const OAUTH_CLIENT_TYPE: &str = "X-Apple-OAuth-Client-Type";
    pub const OAUTH_REDIRECT_URI: &str = "X-Apple-OAuth-Redirect-URI";
    pub const OAUTH_REQUIRE_GRANT_CODE: &str = "X-Apple-OAuth-Require-Grant-Code";
    pub const OAUTH_RESPONSE_MODE: &str = "X-Apple-OAuth-Response-Mode";
    pub const OAUTH_RESPONSE_TYPE: &str = "X-Apple-OAuth-Response-Type";
    pub const OAUTH_STATE: &str = "X-Apple-OAuth-State";
    pub const WIDGET_KEY: &str = "X-Apple-Widget-Key";

    pub const FD_CLIENT_INFO: &str = "X-Apple-I-FD-Client-Info";
    pub const DOMAIN_ID: &str = "X-Apple-Domain-Id";
    pub const LOCALE: &str = "X-Apple-Locale";
    pub const PRIVACY_CONSENT: &str = "X-Apple-Privacy-Consent";
    pub const PRIVACY_CONSENT_ACCEPTED: &str = "X-Apple-Privacy-Consent-Accepted";
}

pub mod client {
    use std::time::Duration;

    pub const OAUTH_CLIENT_ID: &str = "d39ba9916b7251055b22c7f910e2ea796ee65e98b2ddecea8f5dde8d9d1a815d";

    pub const ICLOUD_DOMAIN_ID: &str = "6";

    pub const LOCALE_ZH_CN: &str = "zh_CN";
    pub const OAUTH_CLIENT_TYPE_FIRST_PARTY: &str = "firstPartyAuth";
    pub const OAUTH_RESPONSE_MODE_WEB_MESSAGE: &str = "web_message";
    pub const OAUTH_RESPONSE_TYPE_CODE: &str = "code";
    pub const OAUTH_GRANT_CODE_REQUIRED: &str = "true";
    pub const PRIVACY_CONSENT_VALUE: &str = "true";

    pub const CONTENT_TYPE_JSON: &str = "application/json";
    pub const CONTENT_TYPE_TEXT_PLAIN: &str = "text/plain;charset=UTF-8";
    pub const ACCEPT_JSON: &str = "application/json";
    pub const ACCEPT_HTML: &str = "text/html";

    pub const URI_SCHEME_HTTPS: &str = "https://";

    pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
         (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36";

    pub const FD_CLIENT_INFO: &str = "{\"U\":\"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/151.0.0.0 Safari/537.36\",\"L\":\"zh-CN\",\"Z\":\"GMT+08:00\",\"V\":\"1.1\",\"F\":\"Fla44j1e3NlY5BNlY5BSmHACVZXnNAApukV22p1JkQxQeLaD.SAuXjodUW1BNmrk0ugN.xL4Fe.SpDI2tKEpSMw7GY5BNlYJNNlY5QB4bVNjMk.0Fm\"}";

    pub const CLIENT_BUILD_NUMBER: &str = "2638Build27";

    pub const FIND_MY_CLIENT_BUILD_NUMBER: &str = "2638Build19";

    pub const CONTACTS_CLIENT_BUILD_NUMBER: &str = "2638Build27";
    pub const CONTACTS_CLIENT_MASTERING_NUMBER: &str = "2638Build27";

    pub const DEFAULT_ACCOUNT_COUNTRY: &str = "CHN";

    pub const PROTOCOL_S2K: &str = "s2k";
    pub const PROTOCOL_S2K_FO: &str = "s2k_fo";

    pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
    pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
    pub const MEDIA_TIMEOUT: Duration = Duration::from_secs(300);
    pub const MEDIA_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
}
