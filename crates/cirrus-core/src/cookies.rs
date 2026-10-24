use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use reqwest::cookie::CookieStore;
use reqwest::header::HeaderValue;
use reqwest::Url;

pub struct SharedCookieJar {
    inner: RwLock<cookie_store::CookieStore>,
}

impl Default for SharedCookieJar {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedCookieJar {
    pub fn new() -> Self {
        Self { inner: RwLock::new(cookie_store::CookieStore::default()) }
    }

    pub fn to_json(&self) -> String {
        let store = self.read();
        let cookies: Vec<&cookie_store::Cookie<'static>> = store.iter_unexpired().collect();
        match serde_json::to_string(&cookies) {
            Ok(json) => json,
            Err(error) => {
                log::warn!("[COOKIE] failed to serialize cookie jar; using an empty jar: {error}");
                "[]".to_string()
            }
        }
    }

    pub fn from_json(json: &str) -> Self {
        let store = Self::decode(json).unwrap_or_else(|| {
            log::warn!("[COOKIE] could not parse cookie snapshot; using an empty jar and requiring sign-in");
            cookie_store::CookieStore::default()
        });
        Self { inner: RwLock::new(store) }
    }

    fn decode(json: &str) -> Option<cookie_store::CookieStore> {
        let cookies: Vec<cookie_store::Cookie<'static>> = serde_json::from_str(json).ok()?;

        cookie_store::CookieStore::from_cookies(cookies.into_iter().map(Ok::<_, cookie_store::CookieError>), false).ok()
    }

    fn read(&self) -> RwLockReadGuard<'_, cookie_store::CookieStore> {
        self.inner.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn cookie_value(&self, url: &Url, name: &str) -> Option<String> {
        self.read()
            .get_request_values(url)
            .find(|(cookie_name, _)| cookie_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.to_string())
    }

    fn write(&self) -> RwLockWriteGuard<'_, cookie_store::CookieStore> {
        self.inner.write().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl CookieStore for SharedCookieJar {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let mut store = self.write();
        for header in cookie_headers {
            let Ok(raw) = header.to_str() else {
                log::debug!("[COOKIE] skipping non-UTF-8 Set-Cookie header");
                continue;
            };

            let Ok(cookie) = cookie::Cookie::parse(raw) else {
                log::debug!("[COOKIE] skipping unparsable Set-Cookie header");
                continue;
            };
            if let Err(error) = store.insert_raw(&cookie, url) {
                log::debug!("[COOKIE] rejected cookie: {error}");
            }
        }
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        let store = self.read();
        let joined =
            store.get_request_values(url).map(|(name, value)| format!("{name}={value}")).collect::<Vec<_>>().join("; ");
        if joined.is_empty() {
            return None;
        }

        HeaderValue::from_str(&joined).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_cookie(jar: &SharedCookieJar, url: &str, raw: &str) {
        let url: Url = url.parse().expect("测试 URL 应当合法");
        let header = HeaderValue::from_str(raw).expect("测试 Set-Cookie 应当合法");
        let mut headers = std::iter::once(&header);
        jar.set_cookies(&mut headers, &url);
    }

    fn cookie_header(jar: &SharedCookieJar, url: &str) -> Option<String> {
        let url: Url = url.parse().expect("测试 URL 应当合法");
        jar.cookies(&url).map(|value| value.to_str().expect("Cookie 头应当是可见字符").to_string())
    }

    #[test]
    fn cookies_only_match_their_own_domain() {
        let jar = SharedCookieJar::new();
        set_cookie(&jar, "https://idmsa.apple.com/", "session_id=APPLE-ONLY; Path=/; Secure");
        set_cookie(&jar, "https://example.com/", "foo=BAR; Path=/; Secure");

        assert_eq!(
            cookie_header(&jar, "https://idmsa.apple.com/appleauth/auth").as_deref(),
            Some("session_id=APPLE-ONLY")
        );
        assert_eq!(cookie_header(&jar, "https://example.com/index.html").as_deref(), Some("foo=BAR"));

        assert_eq!(cookie_header(&jar, "https://evil.example.net/"), None);
        assert_eq!(cookie_header(&jar, "https://icloud.com/"), None);
    }

    #[test]
    fn json_roundtrip_keeps_behavior_including_session_cookies() {
        let jar = SharedCookieJar::new();

        set_cookie(&jar, "https://idmsa.apple.com/", "myacinfo=WEB-DUMMY; Path=/; Secure");
        set_cookie(&jar, "https://example.com/", "lang=zh-CN; Path=/; Secure");

        let json = jar.to_json();
        assert!(json.contains("myacinfo"), "会话级 cookie 必须进快照");

        let restored = SharedCookieJar::from_json(&json);
        assert_eq!(
            cookie_header(&restored, "https://idmsa.apple.com/appleauth/auth"),
            cookie_header(&jar, "https://idmsa.apple.com/appleauth/auth")
        );
        assert_eq!(cookie_header(&restored, "https://example.com/"), cookie_header(&jar, "https://example.com/"));
        assert_eq!(cookie_header(&restored, "https://evil.example.net/"), None);
    }

    #[test]
    fn broken_snapshot_degrades_to_empty_jar() {
        for json in ["not json", "", "{}", "null"] {
            let jar = SharedCookieJar::from_json(json);
            assert_eq!(cookie_header(&jar, "https://idmsa.apple.com/"), None, "输入 {json:?}");
        }
    }

    #[test]
    fn invalid_set_cookie_is_skipped_without_failing() {
        let jar = SharedCookieJar::new();

        set_cookie(&jar, "https://idmsa.apple.com/", "this is not a cookie");
        set_cookie(&jar, "https://idmsa.apple.com/", "session_id=OK; Path=/; Secure");
        assert_eq!(cookie_header(&jar, "https://idmsa.apple.com/appleauth/auth").as_deref(), Some("session_id=OK"));
    }

    #[test]
    fn parses_realistic_apple_set_cookie_headers() {
        let jar = SharedCookieJar::new();
        let url = "https://idmsa.apple.com/appleauth/auth/signin";
        set_cookie(&jar, url, "myacinfo=WEB-DUMMY-VALUE; Path=/; Domain=.apple.com; Secure; HttpOnly");
        set_cookie(&jar, url, r#"X-APPLE-WEBAUTH-USER="v=1:s=1:d=1234567890"; Path=/; Domain=.apple.com; Secure"#);

        set_cookie(&jar, url, "other=1; Path=/; Domain=.example.com; Secure");

        let header = cookie_header(&jar, url).expect("应当有 cookie");
        assert!(header.contains("myacinfo=WEB-DUMMY-VALUE"), "实际: {header}");
        assert!(header.contains("X-APPLE-WEBAUTH-USER="), "实际: {header}");
        assert!(!header.contains("other=1"), "域名不匹配的 cookie 不该被带上: {header}");

        assert!(cookie_header(&jar, "https://www.apple.com/").is_some());
        assert_eq!(cookie_header(&jar, "https://setup.icloud.com/setup/ws/1"), None);
    }

    #[tokio::test]
    async fn reqwest_uses_this_jar_end_to_end() {
        use std::io::{Read, Write};
        use std::sync::Arc;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("绑定本地端口失败");
        let address = listener.local_addr().expect("取本地地址失败");
        let server = std::thread::spawn(move || {
            for _ in 0..2 {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut buffer = [0_u8; 4096];
                let read = stream.read(&mut buffer).unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let cookie_line = request
                    .lines()
                    .find(|line| line.to_ascii_lowercase().starts_with("cookie:"))
                    .unwrap_or("Cookie: (none)")
                    .to_string();

                let set_cookie =
                    if cookie_line == "Cookie: (none)" { "Set-Cookie: myacinfo=WEB-DUMMY; Path=/\r\n" } else { "" };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n{}",
                    cookie_line.len(),
                    set_cookie,
                    cookie_line
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        let jar = Arc::new(SharedCookieJar::new());
        let client =
            reqwest::Client::builder().cookie_provider(Arc::clone(&jar)).build().expect("构造 reqwest Client 失败");
        let url = format!("http://{address}/");

        let first = client.get(&url).send().await.expect("第一次请求失败").text().await.expect("读取响应失败");
        assert_eq!(first, "Cookie: (none)");
        let second = client.get(&url).send().await.expect("第二次请求失败").text().await.expect("读取响应失败");
        assert!(second.to_ascii_lowercase().contains("myacinfo=web-dummy"), "reqwest 没有回传 Cookie: {second}");

        let stored: Url = url.parse().expect("测试 URL 非法");
        assert!(jar.cookies(&stored).is_some());
        assert!(jar.to_json().contains("myacinfo"));
        server.join().expect("mock 服务线程 panic");
    }
}
