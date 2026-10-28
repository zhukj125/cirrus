use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use num_bigint::BigUint;
use rand::RngCore;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::apple::api;
use crate::apple::http::{client as hc, headers as hh};
use crate::apple::wire::SigninInitResponse;
use crate::cookies::SharedCookieJar;
use crate::error::{AppError, AppResult};
use crate::models::session_keys::*;
use crate::persistence::SessionSnapshot;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: String,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponseBytes {
    pub status: u16,
    pub headers: HeaderMap,
    pub bytes: Vec<u8>,
}

impl HttpResponseBytes {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

fn insert_header(map: &mut HeaderMap, name: &str, value: &str) {
    if let (Ok(name), Ok(value)) = (HeaderName::from_bytes(name.as_bytes()), HeaderValue::from_str(value)) {
        map.insert(name, value);
    }
}

#[derive(Clone)]
pub struct SrpSession {
    id: String,
    apple_id: String,
    password: Option<String>,
    auth_base: String,
    setup_base: String,
    home_url: String,
    client_id: String,
    http: Client,

    cookies: Arc<SharedCookieJar>,
    session_data: HashMap<String, String>,

    private_key_a: Option<BigUint>,
    a_bytes: Option<Vec<u8>>,
    init_data: Option<SigninInitResponse>,

    dsid: Option<String>,
    find_me_url: Option<String>,
    photos_url: Option<String>,
    cloud_kit_url: Option<String>,
    contacts_url: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,

    server_context: Option<Value>,

    extras: HashMap<String, Value>,
}

impl SrpSession {
    pub fn new(apple_id: String, password: Option<String>, china_mainland: bool) -> AppResult<Self> {
        Self::with_endpoints(
            apple_id,
            password,
            if china_mainland { api::auth::BASE_CN } else { api::auth::BASE },
            if china_mainland { api::setup::BASE_CN } else { api::setup::BASE },
            if china_mainland { api::home::URL_CN } else { api::home::URL },
        )
    }

    pub fn with_endpoints(
        apple_id: String,
        password: Option<String>,
        auth_base: impl Into<String>,
        setup_base: impl Into<String>,
        home_url: impl Into<String>,
    ) -> AppResult<Self> {
        Self::build(
            apple_id,
            password,
            auth_base.into(),
            setup_base.into(),
            home_url.into(),
            Arc::new(SharedCookieJar::new()),
        )
    }

    fn build(
        apple_id: String,
        password: Option<String>,
        auth_base: String,
        setup_base: String,
        home_url: String,
        cookies: Arc<SharedCookieJar>,
    ) -> AppResult<Self> {
        let mut random = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut random);
        let client_id = format!("auth-{}", BASE64.encode(random));

        let http = Client::builder()
            .cookie_provider(Arc::clone(&cookies))
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(hc::CONNECT_TIMEOUT)
            .build()
            .map_err(AppError::from)?;

        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            apple_id,
            password,
            auth_base,
            setup_base,
            home_url,
            client_id,
            http,
            cookies,
            session_data: HashMap::new(),
            private_key_a: None,
            a_bytes: None,
            init_data: None,
            dsid: None,
            find_me_url: None,
            photos_url: None,
            cloud_kit_url: None,
            contacts_url: None,
            first_name: None,
            last_name: None,
            server_context: None,
            extras: HashMap::new(),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn apple_id(&self) -> &str {
        &self.apple_id
    }

    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    pub fn clear_password(&mut self) {
        self.password = None;
    }

    pub fn auth_base(&self) -> &str {
        &self.auth_base
    }

    pub fn setup_base(&self) -> &str {
        &self.setup_base
    }

    pub fn cookie_value_for(&self, url: &str, name: &str) -> Option<String> {
        url.parse().ok().and_then(|url| self.cookies.cookie_value(&url, name))
    }

    pub fn home_url(&self) -> &str {
        &self.home_url
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn private_key_a(&self) -> Option<&BigUint> {
        self.private_key_a.as_ref()
    }

    pub fn set_private_key_a(&mut self, value: BigUint) {
        self.private_key_a = Some(value);
    }

    pub fn a_bytes(&self) -> Option<&[u8]> {
        self.a_bytes.as_deref()
    }

    pub fn set_a_bytes(&mut self, value: Vec<u8>) {
        self.a_bytes = Some(value);
    }

    pub fn init_data(&self) -> Option<&SigninInitResponse> {
        self.init_data.as_ref()
    }

    pub fn set_init_data(&mut self, value: SigninInitResponse) {
        self.init_data = Some(value);
    }

    pub fn dsid(&self) -> Option<&str> {
        self.dsid.as_deref()
    }

    pub fn set_dsid(&mut self, value: Option<String>) {
        self.dsid = value;
    }

    pub fn find_me_url(&self) -> Option<&str> {
        self.find_me_url.as_deref()
    }

    pub fn set_find_me_url(&mut self, value: Option<String>) {
        self.find_me_url = value;
    }

    pub fn photos_url(&self) -> Option<&str> {
        self.photos_url.as_deref()
    }

    pub fn set_photos_url(&mut self, value: Option<String>) {
        self.photos_url = value;
    }

    pub fn cloud_kit_url(&self) -> Option<&str> {
        self.cloud_kit_url.as_deref()
    }

    pub fn set_cloud_kit_url(&mut self, value: Option<String>) {
        self.cloud_kit_url = value;
    }

    pub fn contacts_url(&self) -> Option<&str> {
        self.contacts_url.as_deref()
    }

    pub fn set_contacts_url(&mut self, value: Option<String>) {
        self.contacts_url = value;
    }

    pub fn first_name(&self) -> Option<&str> {
        self.first_name.as_deref()
    }

    pub fn set_first_name(&mut self, value: Option<String>) {
        self.first_name = value;
    }

    pub fn last_name(&self) -> Option<&str> {
        self.last_name.as_deref()
    }

    pub fn set_last_name(&mut self, value: Option<String>) {
        self.last_name = value;
    }

    pub fn server_context(&self) -> Option<&Value> {
        self.server_context.as_ref()
    }

    pub fn set_server_context(&mut self, value: Option<Value>) {
        self.server_context = value;
    }

    pub fn extra(&self, key: &str) -> Option<&Value> {
        self.extras.get(key)
    }

    pub fn set_extra(&mut self, key: &str, value: Value) {
        self.extras.insert(key.to_string(), value);
    }

    pub fn remove_extra(&mut self, key: &str) -> Option<Value> {
        self.extras.remove(key)
    }

    pub fn switch_to_china(&mut self) {
        self.auth_base = api::auth::BASE_CN.to_string();
        self.setup_base = api::setup::BASE_CN.to_string();
        self.home_url = api::home::URL_CN.to_string();
    }

    pub fn is_china_mainland(&self) -> bool {
        self.auth_base.contains(".cn")
    }

    pub fn session_data(&self, key: &str) -> Option<&str> {
        self.session_data.get(key).map(String::as_str)
    }

    pub fn set_session_data(&mut self, key: &str, value: impl Into<String>) {
        self.session_data.insert(key.to_string(), value.into());
    }

    pub fn remove_session_data(&mut self, key: &str) {
        self.session_data.remove(key);
    }

    pub fn is_flag_set(&self, key: &str) -> bool {
        self.session_data.get(key).map(String::as_str) == Some(FLAG_TRUE)
    }

    pub fn set_flag(&mut self, key: &str, value: bool) {
        if value {
            self.set_session_data(key, FLAG_TRUE);
        } else {
            self.session_data.remove(key);
        }
    }

    pub fn clear_cloud_session(&mut self) {
        self.dsid = None;
        self.find_me_url = None;
        self.photos_url = None;
        self.cloud_kit_url = None;
        self.contacts_url = None;
        self.first_name = None;
        self.last_name = None;
        self.server_context = None;
    }

    pub fn to_snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            id: self.id.clone(),
            apple_id: self.apple_id.clone(),
            china_mainland: self.is_china_mainland(),
            auth_base: self.auth_base.clone(),
            setup_base: self.setup_base.clone(),
            home_url: self.home_url.clone(),
            client_id: self.client_id.clone(),
            session_data: self.session_data.clone(),
            dsid: self.dsid.clone(),
            find_me_url: self.find_me_url.clone(),
            photos_url: self.photos_url.clone(),
            cloud_kit_url: self.cloud_kit_url.clone(),
            contacts_url: self.contacts_url.clone(),
            first_name: self.first_name.clone(),
            last_name: self.last_name.clone(),
            server_context: self.server_context.clone(),
            extras: self.extras.clone(),
            cookies_json: self.cookies.to_json(),
            last_access: 0,
        }
    }

    pub fn from_snapshot(snapshot: &SessionSnapshot) -> AppResult<Self> {
        let cookies = Arc::new(SharedCookieJar::from_json(&snapshot.cookies_json));
        let mut session = Self::build(
            snapshot.apple_id.clone(),
            None,
            snapshot.auth_base.clone(),
            snapshot.setup_base.clone(),
            snapshot.home_url.clone(),
            cookies,
        )?;

        session.id = snapshot.id.clone();
        session.client_id = snapshot.client_id.clone();
        session.session_data = snapshot.session_data.clone();
        session.dsid = snapshot.dsid.clone();
        session.find_me_url = snapshot.find_me_url.clone();
        session.photos_url = snapshot.photos_url.clone();
        session.cloud_kit_url = snapshot.cloud_kit_url.clone();
        session.contacts_url = snapshot.contacts_url.clone();
        session.first_name = snapshot.first_name.clone();
        session.last_name = snapshot.last_name.clone();
        session.server_context = snapshot.server_context.clone();
        session.extras = snapshot.extras.clone();
        Ok(session)
    }

    pub fn oauth_headers(&self) -> Vec<(&'static str, String)> {
        let mut headers = vec![
            (hh::OAUTH_CLIENT_ID, hc::OAUTH_CLIENT_ID.to_string()),
            (hh::OAUTH_CLIENT_TYPE, hc::OAUTH_CLIENT_TYPE_FIRST_PARTY.to_string()),
            (hh::OAUTH_REDIRECT_URI, api::home::URL.to_string()),
            (hh::OAUTH_REQUIRE_GRANT_CODE, hc::OAUTH_GRANT_CODE_REQUIRED.to_string()),
            (hh::OAUTH_RESPONSE_MODE, hc::OAUTH_RESPONSE_MODE_WEB_MESSAGE.to_string()),
            (hh::OAUTH_RESPONSE_TYPE, hc::OAUTH_RESPONSE_TYPE_CODE.to_string()),
            (hh::OAUTH_STATE, self.client_id.clone()),
            (hh::WIDGET_KEY, hc::OAUTH_CLIENT_ID.to_string()),
            (hh::DOMAIN_ID, hc::ICLOUD_DOMAIN_ID.to_string()),
            (hh::LOCALE, hc::LOCALE_ZH_CN.to_string()),
            (hh::FD_CLIENT_INFO, hc::FD_CLIENT_INFO.to_string()),
            (hh::PRIVACY_CONSENT, hc::PRIVACY_CONSENT_VALUE.to_string()),
            (hh::PRIVACY_CONSENT_ACCEPTED, hc::PRIVACY_CONSENT_VALUE.to_string()),
        ];
        if let Some(token) = self.session_data(TRUST_TOKEN).filter(|value| !value.trim().is_empty()) {
            headers.push((hh::TRUST_TOKEN, token.to_string()));
        }
        headers
    }

    pub fn auth_headers(&self) -> Vec<(&'static str, String)> {
        let mut headers = self.oauth_headers();
        let extra = [
            (hh::SCNT, SCNT),
            (hh::ID_SESSION_ID, SESSION_ID),
            (hh::AUTH_ATTRIBUTES, AUTH_ATTRIBUTES),
            (hh::PUSH_TOKEN, PUSH_TOKEN),
        ];
        for (header_name, session_key) in extra {
            if let Some(value) = self.session_data(session_key) {
                headers.push((header_name, value.to_string()));
            }
        }
        headers
    }

    fn auth_origin(&self) -> &str {
        match self.auth_base.find("/appleauth") {
            Some(index) if index > 0 => &self.auth_base[..index],
            _ => &self.home_url,
        }
    }

    fn save_response_headers(&mut self, response: &HttpResponse) {
        const HEADER_TO_KEY: [(&str, &str); 7] = [
            (hh::ACCOUNT_COUNTRY, ACCOUNT_COUNTRY),
            (hh::ID_SESSION_ID, SESSION_ID),
            (hh::SESSION_TOKEN, SESSION_TOKEN),
            (hh::TRUST_TOKEN, TRUST_TOKEN),
            (hh::AUTH_ATTRIBUTES, AUTH_ATTRIBUTES),
            (hh::SCNT, SCNT),
            (hh::RSCD, RSCD),
        ];
        for (header_name, session_key) in HEADER_TO_KEY {
            if let Some(value) = response.header(header_name) {
                self.session_data.insert(session_key.to_string(), value.to_string());
            }
        }
    }

    pub async fn request(
        &mut self,
        method: Method,
        url: &str,
        body: Option<&Value>,
        headers: &[(&str, &str)],
    ) -> AppResult<HttpResponse> {
        let result = self.send_request(method, url, body, headers).await?;
        self.save_response_headers(&result);
        Ok(result)
    }

    pub async fn request_shared(
        &self,
        method: Method,
        url: &str,
        body: Option<&Value>,
        headers: &[(&str, &str)],
    ) -> AppResult<HttpResponse> {
        self.send_request(method, url, body, headers).await
    }

    async fn send_request(
        &self,
        method: Method,
        url: &str,
        body: Option<&Value>,
        headers: &[(&str, &str)],
    ) -> AppResult<HttpResponse> {
        let is_auth_request = url.starts_with(self.auth_base.as_str());
        let origin = if is_auth_request { self.auth_origin().to_string() } else { self.home_url.clone() };
        let referer = format!("{origin}/");

        let mut map = HeaderMap::new();
        insert_header(&mut map, hh::USER_AGENT, hc::USER_AGENT);
        insert_header(&mut map, hh::ORIGIN, &origin);
        insert_header(&mut map, hh::REFERER, &referer);
        insert_header(&mut map, hh::ACCEPT, hc::ACCEPT_JSON);
        insert_header(&mut map, hh::CONTENT_TYPE, hc::CONTENT_TYPE_JSON);
        for (name, value) in headers {
            insert_header(&mut map, name, value);
        }

        let mut builder = self.http.request(method, url).timeout(hc::REQUEST_TIMEOUT).headers(map);
        if let Some(body) = body {
            builder = builder.body(serde_json::to_vec(body).map_err(AppError::from)?);
        }

        let response = builder.send().await.map_err(|source| AppError::Http { url: url.to_string(), source })?;
        let status = response.status().as_u16();
        let response_headers = response.headers().clone();
        let body_text = response.text().await.map_err(|source| AppError::Http { url: url.to_string(), source })?;
        Ok(HttpResponse { status, headers: response_headers, body: body_text })
    }

    pub async fn request_bytes(
        &mut self,
        method: Method,
        url: &str,
        body: Option<&Value>,
        headers: &[(&str, &str)],
    ) -> AppResult<HttpResponseBytes> {
        let is_auth_request = url.starts_with(self.auth_base.as_str());
        let origin = if is_auth_request { self.auth_origin().to_string() } else { self.home_url.clone() };
        let referer = format!("{origin}/");

        let mut map = HeaderMap::new();
        insert_header(&mut map, hh::USER_AGENT, hc::USER_AGENT);
        insert_header(&mut map, hh::ORIGIN, &origin);
        insert_header(&mut map, hh::REFERER, &referer);
        insert_header(&mut map, hh::ACCEPT, hc::ACCEPT_JSON);
        insert_header(&mut map, hh::CONTENT_TYPE, hc::CONTENT_TYPE_JSON);
        for (name, value) in headers {
            insert_header(&mut map, name, value);
        }

        let mut builder = self.http.request(method, url).timeout(hc::REQUEST_TIMEOUT).headers(map);
        if let Some(body) = body {
            builder = builder.body(serde_json::to_vec(body).map_err(AppError::from)?);
        }

        let response = builder.send().await.map_err(|source| AppError::Http { url: url.to_string(), source })?;
        let status = response.status().as_u16();
        let response_headers = response.headers().clone();
        let bytes = response.bytes().await.map_err(|source| AppError::Http { url: url.to_string(), source })?.to_vec();
        Ok(HttpResponseBytes { status, headers: response_headers, bytes })
    }
}

pub struct SessionHandle {
    inner: Mutex<SrpSession>,
    last_access: AtomicI64,

    last_persisted: AtomicI64,
}

impl SessionHandle {
    pub fn new(session: SrpSession) -> Self {
        Self {
            inner: Mutex::new(session),
            last_access: AtomicI64::new(now_millis()),
            last_persisted: AtomicI64::new(0),
        }
    }

    pub fn with_last_access(session: SrpSession, last_access: i64) -> Self {
        Self { inner: Mutex::new(session), last_access: AtomicI64::new(last_access), last_persisted: AtomicI64::new(0) }
    }

    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, SrpSession> {
        self.inner.lock().await
    }

    pub fn last_access(&self) -> i64 {
        self.last_access.load(Ordering::Relaxed)
    }

    pub fn touch(&self) {
        self.last_access.store(now_millis(), Ordering::Relaxed);
    }

    pub fn claim_persist(&self, min_interval: Duration) -> bool {
        let now = now_millis();
        let last = self.last_persisted.load(Ordering::Relaxed);
        if last != 0 && now.saturating_sub(last) < min_interval.as_millis() as i64 {
            return false;
        }
        self.last_persisted.compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed).is_ok()
    }

    pub fn mark_persisted(&self) {
        self.last_persisted.store(now_millis(), Ordering::Relaxed);
    }

    pub fn is_expired(&self, ttl: Duration) -> bool {
        now_millis() - self.last_access() > ttl.as_millis() as i64
    }
}

pub fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}
