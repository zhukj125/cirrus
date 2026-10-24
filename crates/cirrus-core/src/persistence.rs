use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use aes_gcm::aead::{rand_core::RngCore, Aead, OsRng};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const KEYRING_SERVICE: &str = "com.cirrus.session-vault.v1";
const KEYRING_USER: &str = "local-encryption-key";
const ENVELOPE_MAGIC: &[u8] = b"cirrus1";
const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub id: String,
    pub apple_id: String,

    #[serde(default)]
    pub china_mainland: bool,
    pub auth_base: String,
    pub setup_base: String,
    pub home_url: String,

    pub client_id: String,

    #[serde(default)]
    pub session_data: HashMap<String, String>,
    #[serde(default)]
    pub dsid: Option<String>,
    #[serde(default)]
    pub find_me_url: Option<String>,
    #[serde(default)]
    pub photos_url: Option<String>,
    #[serde(default)]
    pub cloud_kit_url: Option<String>,
    #[serde(default)]
    pub contacts_url: Option<String>,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,

    #[serde(default)]
    pub server_context: Option<serde_json::Value>,

    #[serde(default)]
    pub extras: HashMap<String, serde_json::Value>,

    #[serde(default)]
    pub cookies_json: String,

    #[serde(default)]
    pub last_access: i64,
}

struct SecretCodec {
    cipher: Aes256Gcm,
}

impl SecretCodec {
    fn load_or_create() -> AppResult<Self> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|error| AppError::other(format!("Could not open the OS credential store: {error}")))?;
        match entry.get_password() {
            Ok(encoded) => Self::from_encoded_key(&encoded),
            Err(keyring::Error::NoEntry) => {
                let key: [u8; 32] = rand::random();
                entry
                    .set_password(&URL_SAFE_NO_PAD.encode(key))
                    .map_err(|error| AppError::other(format!("Could not save the session key: {error}")))?;
                Ok(Self::from_key(key))
            }
            Err(error) => Err(AppError::other(format!("Could not read the session key: {error}"))),
        }
    }

    fn from_encoded_key(encoded: &str) -> AppResult<Self> {
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded.trim())
            .map_err(|_| AppError::other("The session key has an invalid format"))?;
        let key: [u8; 32] = bytes.try_into().map_err(|_| AppError::other("The session key has an invalid length"))?;
        Ok(Self::from_key(key))
    }

    fn from_key(key: [u8; 32]) -> Self {
        Self { cipher: Aes256Gcm::new_from_slice(&key).expect("AES-256 key length is fixed") }
    }

    fn encrypt(&self, plain: &[u8]) -> AppResult<Vec<u8>> {
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce), plain)
            .map_err(|_| AppError::other("Could not encrypt the session vault"))?;
        let mut envelope = Vec::with_capacity(ENVELOPE_MAGIC.len() + NONCE_LEN + ciphertext.len());
        envelope.extend_from_slice(ENVELOPE_MAGIC);
        envelope.extend_from_slice(&nonce);
        envelope.extend_from_slice(&ciphertext);
        Ok(envelope)
    }

    fn decrypt(&self, envelope: &[u8]) -> Option<Vec<u8>> {
        if envelope.len() <= ENVELOPE_MAGIC.len() + NONCE_LEN || !envelope.starts_with(ENVELOPE_MAGIC) {
            return None;
        }
        let nonce_start = ENVELOPE_MAGIC.len();
        let ciphertext_start = nonce_start + NONCE_LEN;
        self.cipher
            .decrypt(Nonce::from_slice(&envelope[nonce_start..ciphertext_start]), &envelope[ciphertext_start..])
            .ok()
    }
}

pub struct SessionVault {
    path: Option<PathBuf>,
    codec: Option<SecretCodec>,
    records: Mutex<HashMap<String, SessionSnapshot>>,
}

impl SessionVault {
    pub fn open(database: PathBuf) -> Self {
        let codec = match SecretCodec::load_or_create() {
            Ok(codec) => codec,
            Err(error) => {
                log::warn!("[VAULT] session persistence is unavailable: {error}");
                return Self { path: None, codec: None, records: Mutex::new(HashMap::new()) };
            }
        };
        let records = match load_records(&database, &codec) {
            Ok(records) => records,
            Err(error) => {
                log::warn!("[VAULT] session data is unavailable; starting empty: {error}");
                HashMap::new()
            }
        };
        log::info!("[VAULT] session vault ready: {} encrypted records", records.len());
        Self { path: Some(database), codec: Some(codec), records: Mutex::new(records) }
    }

    #[cfg(test)]
    pub(crate) fn open_with_key(database: PathBuf, key: [u8; 32]) -> Self {
        let codec = SecretCodec::from_key(key);
        let records = load_records(&database, &codec).unwrap_or_default();
        Self { path: Some(database), codec: Some(codec), records: Mutex::new(records) }
    }

    pub fn is_enabled(&self) -> bool {
        self.path.is_some() && self.codec.is_some()
    }

    pub fn save(&self, snapshot: &SessionSnapshot) -> AppResult<()> {
        if !self.is_enabled() {
            return Ok(());
        }
        let mut records = self.lock();
        records.insert(snapshot.id.clone(), snapshot.clone());
        self.flush(&records)
    }

    pub fn load(&self, session_id: &str) -> Option<SessionSnapshot> {
        if !self.is_enabled() {
            return None;
        }
        self.lock().get(session_id).cloned()
    }

    pub fn remove(&self, session_id: &str) {
        if !self.is_enabled() {
            return;
        }
        let mut records = self.lock();
        if records.remove(session_id).is_none() {
            return;
        }
        if let Err(error) = self.flush(&records) {
            log::warn!("[VAULT] failed to persist after deleting a session: {error}");
        }
    }

    pub fn prune(&self, ttl: Duration) {
        if !self.is_enabled() {
            return;
        }
        let ttl_ms = ttl.as_millis() as i64;
        let now = crate::session::now_millis();
        let mut records = self.lock();
        let before = records.len();
        records.retain(|_, snapshot| now.saturating_sub(snapshot.last_access) <= ttl_ms);
        if records.len() == before {
            return;
        }
        log::info!("[VAULT] pruned {} expired session records", before - records.len());
        if let Err(error) = self.flush(&records) {
            log::warn!("[VAULT] failed to persist after pruning expired sessions: {error}");
        }
    }

    fn flush(&self, records: &HashMap<String, SessionSnapshot>) -> AppResult<()> {
        let Some(path) = self.path.as_ref() else {
            return Ok(());
        };
        let Some(codec) = self.codec.as_ref() else {
            return Ok(());
        };
        write_records(path, codec, records)
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, SessionSnapshot>> {
        self.records.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn load_records(database: &Path, codec: &SecretCodec) -> AppResult<HashMap<String, SessionSnapshot>> {
    let connection = crate::database::open_database(database)?;
    let mut statement = connection.prepare("SELECT session_id, encrypted FROM session_secrets")?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)))?;
    let mut records = HashMap::new();
    for row in rows {
        let (id, envelope) = row?;
        let Some(plain) = codec.decrypt(&envelope) else {
            log::warn!("[VAULT] skipped an unreadable session record");
            continue;
        };
        match serde_json::from_slice::<SessionSnapshot>(&plain) {
            Ok(snapshot) => {
                records.insert(id, snapshot);
            }
            Err(error) => log::warn!("[VAULT] skipped a malformed session record: {error}"),
        }
    }
    Ok(records)
}

fn write_records(database: &Path, codec: &SecretCodec, records: &HashMap<String, SessionSnapshot>) -> AppResult<()> {
    let mut connection = crate::database::open_database(database)?;
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM session_secrets", [])?;
    for (id, snapshot) in records {
        let plain = serde_json::to_vec(snapshot)?;
        let envelope = codec.encrypt(&plain)?;
        transaction.execute(
            "INSERT INTO session_secrets (session_id, encrypted, last_access) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, envelope, snapshot.last_access],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: [u8; 32] = [7u8; 32];

    fn temp_vault() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("cirrus-vault-{}", uuid::Uuid::new_v4()));
        let file = dir.join("nested").join("cirrus.db");
        (dir, file)
    }

    fn snapshot(id: &str, last_access: i64) -> SessionSnapshot {
        let mut session_data = HashMap::new();
        session_data.insert("session_token".to_string(), "SUPER-SECRET-TOKEN-12345".to_string());
        session_data.insert("scnt".to_string(), "SUPER-SECRET-SCNT-67890".to_string());
        let mut extras = HashMap::new();
        extras.insert("phones".to_string(), serde_json::json!([{"id": 1, "label": "iPhone"}]));
        SessionSnapshot {
            id: id.to_string(),
            apple_id: "tester@example.com".to_string(),
            china_mainland: true,
            auth_base: "https://idmsa.apple.com.cn/appleauth/auth".to_string(),
            setup_base: "https://setup.icloud.com.cn/setup/ws/1".to_string(),
            home_url: "https://www.icloud.com.cn".to_string(),
            client_id: "auth-test-client".to_string(),
            session_data,
            dsid: Some("123456789".to_string()),
            find_me_url: Some("https://fmipmobile.icloud.com/fmipservice/device".to_string()),
            photos_url: Some("https://p01-ckdatabasews.icloud.com".to_string()),
            cloud_kit_url: Some("https://p01-ckdatabasews.icloud.com".to_string()),
            contacts_url: Some("https://p01-contactsws.icloud.com".to_string()),
            first_name: Some("三".to_string()),
            last_name: Some("张".to_string()),
            server_context: Some(serde_json::json!({"authToken": "server-context-token"})),
            extras,
            cookies_json: r#"[{"raw_cookie":"myacinfo=WEB-DUMMY"}]"#.to_string(),
            last_access,
        }
    }

    #[test]
    fn encrypted_envelope_roundtrips_and_rejects_tampering() {
        let codec = SecretCodec::from_key(TEST_KEY);
        let plain = b"sensitive session data";
        let mut envelope = codec.encrypt(plain).expect("encrypt");
        assert_ne!(envelope, plain);
        assert_eq!(codec.decrypt(&envelope).as_deref(), Some(plain.as_slice()));

        let last = envelope.len() - 1;
        envelope[last] ^= 0x01;
        assert!(codec.decrypt(&envelope).is_none());
    }

    #[test]
    fn save_then_open_roundtrips_all_fields() {
        let (dir, file) = temp_vault();
        let vault = SessionVault::open_with_key(file.clone(), TEST_KEY);
        assert!(vault.is_enabled());
        vault.save(&snapshot("session-1", 1_700_000_000_000)).expect("save session");

        let reopened = SessionVault::open_with_key(file, TEST_KEY);
        let loaded = reopened.load("session-1").expect("load session");
        let expected = snapshot("session-1", 1_700_000_000_000);
        assert_eq!(loaded.id, expected.id);
        assert_eq!(loaded.apple_id, expected.apple_id);
        assert_eq!(loaded.china_mainland, expected.china_mainland);
        assert_eq!(loaded.auth_base, expected.auth_base);
        assert_eq!(loaded.setup_base, expected.setup_base);
        assert_eq!(loaded.home_url, expected.home_url);
        assert_eq!(loaded.client_id, expected.client_id);
        assert_eq!(loaded.session_data, expected.session_data);
        assert_eq!(loaded.dsid, expected.dsid);
        assert_eq!(loaded.find_me_url, expected.find_me_url);
        assert_eq!(loaded.photos_url, expected.photos_url);
        assert_eq!(loaded.cloud_kit_url, expected.cloud_kit_url);
        assert_eq!(loaded.contacts_url, expected.contacts_url);
        assert_eq!(loaded.first_name, expected.first_name);
        assert_eq!(loaded.last_name, expected.last_name);
        assert_eq!(loaded.server_context, expected.server_context);
        assert_eq!(loaded.extras, expected.extras);
        assert_eq!(loaded.cookies_json, expected.cookies_json);
        assert_eq!(loaded.last_access, expected.last_access);
        assert!(reopened.load("missing").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn vault_file_never_contains_plaintext_secrets() {
        let (dir, file) = temp_vault();
        let vault = SessionVault::open_with_key(file.clone(), TEST_KEY);
        vault.save(&snapshot("session-secret", 1_700_000_000_000)).expect("save session");

        let raw = std::fs::read(&file).expect("read database");
        let text = String::from_utf8_lossy(&raw);
        for secret in
            ["SUPER-SECRET-TOKEN-12345", "SUPER-SECRET-SCNT-67890", "server-context-token", "tester@example.com"]
        {
            assert!(!text.contains(secret), "plaintext found in database: {secret}");
        }
        assert!(!text.contains("session_token"));
        assert!(!text.contains("\"sessionData\""));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_drops_expired_records() {
        let (dir, file) = temp_vault();
        let vault = SessionVault::open_with_key(file.clone(), TEST_KEY);
        let now = crate::session::now_millis();
        vault.save(&snapshot("fresh", now)).expect("save fresh");
        vault.save(&snapshot("stale", now - 10 * 60 * 1000)).expect("save stale");
        vault.prune(Duration::from_secs(60));
        assert!(vault.load("stale").is_none());
        assert!(vault.load("fresh").is_some());

        let reopened = SessionVault::open_with_key(file, TEST_KEY);
        assert!(reopened.load("stale").is_none());
        assert!(reopened.load("fresh").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_corrupt_vault_degrades_to_empty() {
        let (dir, file) = temp_vault();
        let vault = SessionVault::open_with_key(file.clone(), TEST_KEY);
        assert!(vault.load("anything").is_none());

        std::fs::create_dir_all(file.parent().expect("parent directory")).expect("create directory");
        std::fs::write(&file, b"not a SQLite database").expect("write corrupt database");
        let broken = SessionVault::open_with_key(file, TEST_KEY);
        assert!(broken.load("anything").is_none());
        broken.remove("anything");
        broken.prune(Duration::from_secs(60));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
