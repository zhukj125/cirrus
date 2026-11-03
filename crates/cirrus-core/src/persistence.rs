use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use aes_gcm::aead::{rand_core::RngCore, Aead, OsRng};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rusqlite::{OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

const KEYRING_SERVICE: &str = "com.cirrus.session-vault.v1";
const KEYRING_USER: &str = "local-encryption-key";
const ENVELOPE_MAGIC: &[u8] = b"cirrus1";
const NONCE_LEN: usize = 12;
const TRUSTED_BROWSER_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub id: String,
    pub apple_id: String,
    #[serde(default)]
    pub revision: i64,

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
    pub docws_url: Option<String>,
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

/// Verify that an existing vault key can be read by this executable without
/// creating a new key. On platforms with an interactive credential store this
/// is the command users run once to grant background access.
pub fn verify_vault_key_access() -> AppResult<()> {
    SecretCodec::load_existing().map(|_| ())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrustedBrowserRecord {
    apple_id: String,
    token: String,
    updated_at: i64,
}
struct KeyInitLock {
    path: PathBuf,
}

impl KeyInitLock {
    fn acquire(database: &Path) -> AppResult<Self> {
        let parent = database.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let path = parent.join(".cirrus-vault-key-init.lock");
        for _ in 0..50 {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if std::fs::metadata(&path)
                        .ok()
                        .and_then(|metadata| metadata.modified().ok())
                        .and_then(|modified| modified.elapsed().ok())
                        .is_some_and(|age| age > Duration::from_secs(30))
                    {
                        let _ = std::fs::remove_file(&path);
                        continue;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(error) => {
                    return Err(AppError::other(format!("Could not lock session vault key initialization: {error}")));
                }
            }
        }
        Err(AppError::other("Timed out while initializing the Cirrus session vault key"))
    }
}

impl Drop for KeyInitLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn database_has_encrypted_records(database: &Path) -> AppResult<bool> {
    let connection = crate::database::open_database(database)?;
    let sessions: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM session_secrets)", [], |row| row.get(0))?;
    let trusted: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM trusted_browsers)", [], |row| row.get(0))?;
    Ok(sessions || trusted)
}

struct SecretCodec {
    cipher: Aes256Gcm,
}

impl SecretCodec {
    fn load_or_create(database: &Path) -> AppResult<Self> {
        if let Some(codec) = Self::load_from_environment()? {
            return Ok(codec);
        }
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|error| AppError::other(format!("Could not open the OS credential store: {error}")))?;
        match entry.get_password() {
            Ok(encoded) => return Self::from_encoded_key(&encoded),
            Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(AppError::other(format!("Could not read the session key: {error}"))),
        }

        let _guard = KeyInitLock::acquire(database)?;
        // Another process may have created the key while this process waited.
        match entry.get_password() {
            Ok(encoded) => return Self::from_encoded_key(&encoded),
            Err(keyring::Error::NoEntry) => {}
            Err(error) => return Err(AppError::other(format!("Could not read the session key: {error}"))),
        }
        if database_has_encrypted_records(database)? {
            return Err(AppError::other(
                "The Cirrus session vault key is missing while encrypted records exist. Open Cirrus to restore the OS credential entry.",
            ));
        }
        let key: [u8; 32] = rand::random();
        entry
            .set_password(&URL_SAFE_NO_PAD.encode(key))
            .map_err(|error| AppError::other(format!("Could not save the session key: {error}")))?;
        Ok(Self::from_key(key))
    }

    fn load_existing() -> AppResult<Self> {
        if let Some(codec) = Self::load_from_environment()? {
            return Ok(codec);
        }
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|error| AppError::other(format!("Could not open the OS credential store: {error}")))?;
        match entry.get_password() {
            Ok(encoded) => Self::from_encoded_key(&encoded),
            Err(keyring::Error::NoEntry) => Err(AppError::other(
                "The Cirrus session vault key is unavailable. Open Cirrus once to initialize it, or provide CIRRUS_SESSION_KEY_FILE.",
            )),
            Err(error) => Err(AppError::other(format!(
                "Could not read the Cirrus session vault key: {error}. Open Cirrus and ensure the OS credential store is accessible."
            ))),
        }
    }

    fn load_from_environment() -> AppResult<Option<Self>> {
        if let Some(path) = std::env::var_os("CIRRUS_SESSION_KEY_FILE").filter(|value| !value.is_empty()) {
            let path = PathBuf::from(path);
            let encoded = std::fs::read_to_string(&path).map_err(|error| {
                AppError::other(format!("Could not read CIRRUS_SESSION_KEY_FILE at {}: {error}", path.display()))
            })?;
            return Self::from_encoded_key(encoded.trim()).map(Some);
        }
        if let Some(encoded) = std::env::var("CIRRUS_SESSION_KEY").ok().filter(|value| !value.trim().is_empty()) {
            return Self::from_encoded_key(encoded.trim()).map(Some);
        }
        Ok(None)
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
    trusted_browsers: Mutex<HashMap<String, TrustedBrowserRecord>>,
}

impl SessionVault {
    pub fn open(database: PathBuf) -> Self {
        let codec = match SecretCodec::load_or_create(&database) {
            Ok(codec) => codec,
            Err(error) => {
                log::warn!("[VAULT] session persistence is unavailable: {error}");
                return Self {
                    path: None,
                    codec: None,
                    records: Mutex::new(HashMap::new()),
                    trusted_browsers: Mutex::new(HashMap::new()),
                };
            }
        };
        let records = match load_records(&database, &codec) {
            Ok(records) => records,
            Err(error) => {
                log::warn!("[VAULT] session data is unavailable; starting empty: {error}");
                HashMap::new()
            }
        };
        let trusted_browsers = match load_trusted_browsers(&database, &codec) {
            Ok(records) => records,
            Err(error) => {
                log::warn!("[VAULT] trusted-browser data is unavailable; starting empty: {error}");
                HashMap::new()
            }
        };
        log::info!(
            "[VAULT] session vault ready: {} encrypted sessions, {} trusted browsers",
            records.len(),
            trusted_browsers.len()
        );
        Self {
            path: Some(database),
            codec: Some(codec),
            records: Mutex::new(records),
            trusted_browsers: Mutex::new(trusted_browsers),
        }
    }

    pub fn open_existing(database: PathBuf) -> AppResult<Self> {
        let codec = SecretCodec::load_existing()?;
        let records = load_records(&database, &codec)?;
        let trusted_browsers = load_trusted_browsers(&database, &codec)?;
        Ok(Self {
            path: Some(database),
            codec: Some(codec),
            records: Mutex::new(records),
            trusted_browsers: Mutex::new(trusted_browsers),
        })
    }

    #[cfg(test)]
    pub(crate) fn open_with_key(database: PathBuf, key: [u8; 32]) -> Self {
        let codec = SecretCodec::from_key(key);
        let records = load_records(&database, &codec).unwrap_or_default();
        let trusted_browsers = load_trusted_browsers(&database, &codec).unwrap_or_default();
        Self {
            path: Some(database),
            codec: Some(codec),
            records: Mutex::new(records),
            trusted_browsers: Mutex::new(trusted_browsers),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.path.is_some() && self.codec.is_some()
    }

    pub fn save(&self, snapshot: &SessionSnapshot) -> AppResult<i64> {
        if !self.is_enabled() {
            return Ok(snapshot.revision);
        }
        let Some(path) = self.path.as_ref() else {
            return Ok(snapshot.revision);
        };
        let Some(codec) = self.codec.as_ref() else {
            return Ok(snapshot.revision);
        };
        let revision = save_record(path, codec, snapshot)?;
        let mut stored = snapshot.clone();
        stored.revision = revision;
        self.lock().insert(stored.id.clone(), stored);
        Ok(revision)
    }

    pub fn load(&self, session_id: &str) -> Option<SessionSnapshot> {
        if !self.is_enabled() {
            return None;
        }
        if let (Some(path), Some(codec)) = (self.path.as_ref(), self.codec.as_ref()) {
            match load_record(path, codec, session_id) {
                Ok(Some(snapshot)) => {
                    self.lock().insert(session_id.to_string(), snapshot.clone());
                    return Some(snapshot);
                }
                Ok(None) => {
                    self.lock().remove(session_id);
                    return None;
                }
                Err(error) => {
                    log::warn!("[VAULT] could not refresh session record from the shared database: {error}");
                }
            }
        }
        self.lock().get(session_id).cloned()
    }

    pub fn remove(&self, session_id: &str) {
        if !self.is_enabled() {
            return;
        }
        self.lock().remove(session_id);
        let Some(path) = self.path.as_ref() else {
            return;
        };
        if let Err(error) = delete_record(path, session_id) {
            log::warn!("[VAULT] failed to persist after deleting a session: {error}");
        }
    }

    pub fn save_trusted_browser(&self, apple_id: &str, token: &str) -> AppResult<()> {
        if !self.is_enabled() {
            return Ok(());
        }
        let key = trusted_browser_key(apple_id);
        let token = token.trim();
        if key.is_empty() || token.is_empty() {
            return Ok(());
        }
        let record = TrustedBrowserRecord {
            apple_id: apple_id.trim().to_string(),
            token: token.to_string(),
            updated_at: crate::session::now_millis(),
        };
        self.trusted_lock().insert(key.clone(), record.clone());
        let Some(path) = self.path.as_ref() else {
            return Ok(());
        };
        let Some(codec) = self.codec.as_ref() else {
            return Ok(());
        };
        save_trusted_browser_record(path, codec, &key, &record)
    }

    pub fn load_trusted_browser(&self, apple_id: &str) -> Option<String> {
        if !self.is_enabled() {
            return None;
        }
        let key = trusted_browser_key(apple_id);
        if let (Some(path), Some(codec)) = (self.path.as_ref(), self.codec.as_ref()) {
            match load_trusted_browser_record(path, codec, &key) {
                Ok(Some(record)) => {
                    self.trusted_lock().insert(key.clone(), record.clone());
                    if crate::session::now_millis().saturating_sub(record.updated_at) <= TRUSTED_BROWSER_TTL_MS {
                        return Some(record.token);
                    }
                    return None;
                }
                Ok(None) => {
                    self.trusted_lock().remove(&key);
                    return None;
                }
                Err(error) => {
                    log::warn!("[VAULT] could not refresh trusted-browser record: {error}");
                }
            }
        }
        let records = self.trusted_lock();
        let record = records.get(&key)?;
        if crate::session::now_millis().saturating_sub(record.updated_at) > TRUSTED_BROWSER_TTL_MS {
            return None;
        }
        Some(record.token.clone())
    }

    pub fn remove_trusted_browser(&self, apple_id: &str) {
        if !self.is_enabled() {
            return;
        }
        let key = trusted_browser_key(apple_id);
        self.trusted_lock().remove(&key);
        if let Some(path) = self.path.as_ref() {
            if let Err(error) = delete_trusted_browser_record(path, &key) {
                log::warn!("[VAULT] failed to persist after removing a trusted browser: {error}");
            }
        }
    }
    pub fn prune(&self, ttl: Duration) {
        if !self.is_enabled() {
            return;
        }
        let ttl_ms = ttl.as_millis() as i64;
        let now = crate::session::now_millis();
        let sessions_changed = {
            let mut records = self.lock();
            let before = records.len();
            let expired = records
                .iter()
                .filter(|(_, snapshot)| now.saturating_sub(snapshot.last_access) > ttl_ms)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            records.retain(|_, snapshot| now.saturating_sub(snapshot.last_access) <= ttl_ms);
            if records.len() == before {
                false
            } else {
                log::info!("[VAULT] pruned {} expired session records", before - records.len());
                if let Some(path) = self.path.as_ref() {
                    if let Err(error) = delete_records(path, &expired) {
                        log::warn!("[VAULT] failed to persist after pruning expired sessions: {error}");
                    }
                }
                true
            }
        };

        let trusted_changed = {
            let mut records = self.trusted_lock();
            let before = records.len();
            let expired = records
                .iter()
                .filter(|(_, record)| now.saturating_sub(record.updated_at) > TRUSTED_BROWSER_TTL_MS)
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            records.retain(|_, record| now.saturating_sub(record.updated_at) <= TRUSTED_BROWSER_TTL_MS);
            if records.len() == before {
                false
            } else {
                log::info!("[VAULT] pruned {} expired trusted browsers", before - records.len());
                if let Some(path) = self.path.as_ref() {
                    if let Err(error) = delete_trusted_browser_records(path, &expired) {
                        log::warn!("[VAULT] failed to persist after pruning trusted browsers: {error}");
                    }
                }
                true
            }
        };

        let _ = (sessions_changed, trusted_changed);
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, SessionSnapshot>> {
        self.records.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn trusted_lock(&self) -> MutexGuard<'_, HashMap<String, TrustedBrowserRecord>> {
        self.trusted_browsers.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn trusted_browser_key(apple_id: &str) -> String {
    let normalized = apple_id.trim().to_ascii_lowercase();
    format!("{:x}", Sha256::digest(normalized.as_bytes()))
}

fn load_trusted_browsers(database: &Path, codec: &SecretCodec) -> AppResult<HashMap<String, TrustedBrowserRecord>> {
    let connection = crate::database::open_database(database)?;
    let mut statement = connection.prepare("SELECT apple_id_key, encrypted FROM trusted_browsers")?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)))?;
    let mut records = HashMap::new();
    for row in rows {
        let (key, envelope) = row?;
        let plain = codec
            .decrypt(&envelope)
            .ok_or_else(|| AppError::other("The session vault key cannot decrypt a trusted-browser record"))?;
        let record = serde_json::from_slice::<TrustedBrowserRecord>(&plain)
            .map_err(|error| AppError::other(format!("Stored trusted-browser record is invalid: {error}")))?;
        records.insert(key, record);
    }
    Ok(records)
}

fn load_trusted_browser_record(
    database: &Path,
    codec: &SecretCodec,
    key: &str,
) -> AppResult<Option<TrustedBrowserRecord>> {
    let connection = crate::database::open_database(database)?;
    let envelope = connection
        .query_row("SELECT encrypted FROM trusted_browsers WHERE apple_id_key = ?1", [key], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .optional()?;
    let Some(envelope) = envelope else {
        return Ok(None);
    };
    let Some(plain) = codec.decrypt(&envelope) else {
        return Err(AppError::other("Could not decrypt a trusted-browser record"));
    };
    serde_json::from_slice::<TrustedBrowserRecord>(&plain)
        .map(Some)
        .map_err(|error| AppError::other(format!("Stored trusted-browser record is invalid: {error}")))
}

fn save_trusted_browser_record(
    database: &Path,
    codec: &SecretCodec,
    key: &str,
    record: &TrustedBrowserRecord,
) -> AppResult<()> {
    let plain = serde_json::to_vec(record)?;
    let envelope = codec.encrypt(&plain)?;
    let mut connection = crate::database::open_database(database)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(
        "INSERT INTO trusted_browsers (apple_id_key, encrypted, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(apple_id_key) DO UPDATE SET encrypted = excluded.encrypted, updated_at = excluded.updated_at",
        rusqlite::params![key, envelope, record.updated_at],
    )?;
    transaction.commit()?;
    Ok(())
}

fn delete_trusted_browser_record(database: &Path, key: &str) -> AppResult<()> {
    let connection = crate::database::open_database(database)?;
    connection.execute("DELETE FROM trusted_browsers WHERE apple_id_key = ?1", [key])?;
    Ok(())
}

fn delete_trusted_browser_records(database: &Path, keys: &[String]) -> AppResult<()> {
    if keys.is_empty() {
        return Ok(());
    }
    let mut connection = crate::database::open_database(database)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for key in keys {
        transaction.execute("DELETE FROM trusted_browsers WHERE apple_id_key = ?1", [key])?;
    }
    transaction.commit()?;
    Ok(())
}
fn load_records(database: &Path, codec: &SecretCodec) -> AppResult<HashMap<String, SessionSnapshot>> {
    let connection = crate::database::open_database(database)?;
    let mut statement = connection.prepare("SELECT session_id, encrypted FROM session_secrets")?;
    let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)))?;
    let mut records = HashMap::new();
    for row in rows {
        let (id, envelope) = row?;
        let plain = codec
            .decrypt(&envelope)
            .ok_or_else(|| AppError::other("The session vault key cannot decrypt a stored session record"))?;
        let snapshot = serde_json::from_slice::<SessionSnapshot>(&plain)
            .map_err(|error| AppError::other(format!("Stored session record is invalid: {error}")))?;
        records.insert(id, snapshot);
    }
    Ok(records)
}

fn load_record(database: &Path, codec: &SecretCodec, session_id: &str) -> AppResult<Option<SessionSnapshot>> {
    let connection = crate::database::open_database(database)?;
    let envelope = connection
        .query_row("SELECT encrypted FROM session_secrets WHERE session_id = ?1", [session_id], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .optional()?;
    let Some(envelope) = envelope else {
        return Ok(None);
    };
    let Some(plain) = codec.decrypt(&envelope) else {
        return Err(AppError::other("Could not decrypt the stored session record"));
    };
    serde_json::from_slice::<SessionSnapshot>(&plain)
        .map(Some)
        .map_err(|error| AppError::other(format!("Stored session record is invalid: {error}")))
}

fn save_record(database: &Path, codec: &SecretCodec, snapshot: &SessionSnapshot) -> AppResult<i64> {
    let mut connection = crate::database::open_database(database)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existing = transaction
        .query_row("SELECT encrypted FROM session_secrets WHERE session_id = ?1", [&snapshot.id], |row| {
            row.get::<_, Vec<u8>>(0)
        })
        .optional()?;
    let current_revision = match existing {
        Some(envelope) => {
            let plain = codec
                .decrypt(&envelope)
                .ok_or_else(|| AppError::other("Could not decrypt the stored session record"))?;
            serde_json::from_slice::<SessionSnapshot>(&plain)
                .map_err(|error| AppError::other(format!("Stored session record is invalid: {error}")))?
                .revision
        }
        None => 0,
    };
    if snapshot.revision != current_revision {
        return Err(AppError::other(format!(
            "SESSION_VAULT_CONFLICT: expected revision {}, current revision {}",
            snapshot.revision, current_revision
        )));
    }
    let next_revision = current_revision.saturating_add(1);
    let mut stored = snapshot.clone();
    stored.revision = next_revision;
    let plain = serde_json::to_vec(&stored)?;
    let envelope = codec.encrypt(&plain)?;
    transaction.execute(
        "INSERT INTO session_secrets (session_id, encrypted, last_access) VALUES (?1, ?2, ?3) \
         ON CONFLICT(session_id) DO UPDATE SET encrypted = excluded.encrypted, last_access = excluded.last_access",
        rusqlite::params![snapshot.id, envelope, snapshot.last_access],
    )?;
    transaction.commit()?;
    Ok(next_revision)
}

fn delete_record(database: &Path, session_id: &str) -> AppResult<()> {
    let connection = crate::database::open_database(database)?;
    connection.execute("DELETE FROM session_secrets WHERE session_id = ?1", [session_id])?;
    Ok(())
}

fn delete_records(database: &Path, session_ids: &[String]) -> AppResult<()> {
    if session_ids.is_empty() {
        return Ok(());
    }
    let mut connection = crate::database::open_database(database)?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for session_id in session_ids {
        transaction.execute("DELETE FROM session_secrets WHERE session_id = ?1", [session_id])?;
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
            revision: 0,
            china_mainland: true,
            auth_base: "https://idmsa.apple.com.cn/appleauth/auth".to_string(),
            setup_base: "https://setup.icloud.com.cn/setup/ws/1".to_string(),
            home_url: "https://www.icloud.com.cn".to_string(),
            client_id: "auth-test-client".to_string(),
            session_data,
            dsid: Some("123456789".to_string()),
            find_me_url: Some("https://fmipmobile.icloud.com/fmipservice/device".to_string()),
            photos_url: Some("https://p01-ckdatabasews.icloud.com".to_string()),
            docws_url: Some("https://p01-docws.icloud.com".to_string()),
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
    fn shared_vault_instances_observe_latest_records_without_overwriting_each_other() {
        let (dir, file) = temp_vault();
        let first = SessionVault::open_with_key(file.clone(), TEST_KEY);
        let second = SessionVault::open_with_key(file, TEST_KEY);

        let mut session_a = snapshot("session-a", 1_700_000_000_000);
        session_a.session_data.insert("token".to_string(), "A1".to_string());
        first.save(&session_a).expect("save A");

        let observed_a = second.load("session-a").expect("second sees A");
        assert_eq!(observed_a.session_data.get("token").map(String::as_str), Some("A1"));

        let mut session_b = snapshot("session-b", 1_700_000_000_100);
        session_b.session_data.insert("token".to_string(), "B1".to_string());
        second.save(&session_b).expect("save B");
        assert!(first.load("session-b").is_some(), "first sees B");

        let mut session_a = first.load("session-a").expect("reload A");
        session_a.session_data.insert("token".to_string(), "A2".to_string());
        first.save(&session_a).expect("update A");
        let observed_a = second.load("session-a").expect("second sees updated A");
        assert_eq!(observed_a.session_data.get("token").map(String::as_str), Some("A2"));

        first.remove("session-a");
        assert!(second.load("session-a").is_none());
        assert!(second.load("session-b").is_some(), "removing A must not remove B");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_same_session_save_is_rejected() {
        let (dir, file) = temp_vault();
        let first = SessionVault::open_with_key(file.clone(), TEST_KEY);
        let second = SessionVault::open_with_key(file, TEST_KEY);
        let mut stale = snapshot("shared-session", 1_700_000_000_000);
        stale.session_data.insert("token".to_string(), "OLD".to_string());
        first.save(&stale).expect("save old");

        let mut latest = second.load("shared-session").expect("load latest");
        latest.session_data.insert("token".to_string(), "NEW".to_string());
        second.save(&latest).expect("save new");

        stale.session_data.insert("token".to_string(), "STALE".to_string());
        let error = first.save(&stale).expect_err("stale same-session save must be rejected");
        assert!(error.to_string().contains("SESSION_VAULT_CONFLICT"));
        assert_eq!(
            second.load("shared-session").expect("load final").session_data.get("token").map(String::as_str),
            Some("NEW")
        );
        let _ = std::fs::remove_dir_all(dir);
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
        assert_eq!(loaded.docws_url, expected.docws_url);
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
    fn trusted_browser_token_roundtrips_and_can_be_removed() {
        let (dir, file) = temp_vault();
        let vault = SessionVault::open_with_key(file.clone(), TEST_KEY);
        vault.save_trusted_browser("Tester@Example.com", "TRUST-TOKEN-123").expect("save trusted browser");

        let reopened = SessionVault::open_with_key(file, TEST_KEY);
        assert_eq!(reopened.load_trusted_browser("tester@example.com").as_deref(), Some("TRUST-TOKEN-123"));

        reopened.remove_trusted_browser("TESTER@example.com");
        assert!(reopened.load_trusted_browser("tester@example.com").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn wrong_vault_key_is_reported_instead_of_silently_skipping_records() {
        let (dir, file) = temp_vault();
        let vault = SessionVault::open_with_key(file.clone(), TEST_KEY);
        vault.save(&snapshot("session-1", 1_700_000_000_000)).expect("save session");
        let wrong_codec = SecretCodec::from_key([8u8; 32]);
        let error = load_records(&file, &wrong_codec).expect_err("wrong key must fail");
        assert!(error.to_string().contains("cannot decrypt"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn shared_vault_trusted_browsers_do_not_overwrite_each_other() {
        let (dir, file) = temp_vault();
        let first = SessionVault::open_with_key(file.clone(), TEST_KEY);
        let second = SessionVault::open_with_key(file, TEST_KEY);
        first.save_trusted_browser("first@example.com", "FIRST").expect("save first");
        second.save_trusted_browser("second@example.com", "SECOND").expect("save second");
        assert_eq!(first.load_trusted_browser("second@example.com").as_deref(), Some("SECOND"));
        assert_eq!(second.load_trusted_browser("first@example.com").as_deref(), Some("FIRST"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn vault_file_never_contains_plaintext_secrets() {
        let (dir, file) = temp_vault();
        let vault = SessionVault::open_with_key(file.clone(), TEST_KEY);
        vault.save(&snapshot("session-secret", 1_700_000_000_000)).expect("save session");
        vault.save_trusted_browser("tester@example.com", "TRUSTED-BROWSER-SECRET").expect("save trusted browser");

        let raw = std::fs::read(&file).expect("read database");
        let text = String::from_utf8_lossy(&raw);
        for secret in [
            "SUPER-SECRET-TOKEN-12345",
            "SUPER-SECRET-SCNT-67890",
            "server-context-token",
            "TRUSTED-BROWSER-SECRET",
            "tester@example.com",
        ] {
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
