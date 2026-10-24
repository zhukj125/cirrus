use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};
use crate::persistence::SessionVault;
use crate::session::{SessionHandle, SrpSession};

const DEFAULT_TTL_MINUTES: i64 = 720;

const PERSIST_INTERVAL: Duration = Duration::from_secs(60);

pub const MAX_SESSIONS: usize = 100;

pub struct SessionManager {
    ttl: Duration,
    sessions: Mutex<HashMap<String, Arc<SessionHandle>>>,

    vault: Option<Arc<SessionVault>>,
}

impl SessionManager {
    pub fn new(ttl_minutes: Option<i64>, vault: Option<Arc<SessionVault>>) -> Self {
        let ttl_minutes = ttl_minutes.unwrap_or(DEFAULT_TTL_MINUTES).max(1);
        let ttl = Duration::from_secs(ttl_minutes as u64 * 60);

        if let Some(vault) = vault.as_ref() {
            vault.prune(ttl);
        }
        Self { ttl, sessions: Mutex::new(HashMap::new()), vault }
    }

    pub fn ttl_minutes(&self) -> i64 {
        self.ttl.as_secs() as i64 / 60
    }

    pub fn vault_enabled(&self) -> bool {
        self.vault.as_ref().is_some_and(|vault| vault.is_enabled())
    }

    pub async fn create(
        &self,
        apple_id: String,
        password: String,
        china_mainland: bool,
    ) -> AppResult<Arc<SessionHandle>> {
        let session = SrpSession::new(apple_id, Some(password), china_mainland)?;
        let session_id = session.id().to_string();
        let handle = Arc::new(SessionHandle::new(session));

        {
            let mut sessions = self.sessions.lock().await;
            self.evict_if_needed(&mut sessions);
            sessions.insert(session_id, Arc::clone(&handle));
        }

        self.persist_or_warn(&handle).await;
        Ok(handle)
    }

    pub async fn get(&self, session_id: &str) -> AppResult<Arc<SessionHandle>> {
        if session_id.trim().is_empty() {
            return Err(AppError::auth("Missing session ID. Please sign in again."));
        }
        let Some(handle) = self.resolve(session_id).await else {
            return Err(AppError::auth("Session does not exist or has expired. Please sign in again."));
        };
        if handle.is_expired(self.ttl) {
            self.remove(session_id).await;
            return Err(AppError::auth("Session expired. Please sign in again."));
        }
        handle.touch();

        if handle.claim_persist(PERSIST_INTERVAL) {
            self.persist_or_warn(&handle).await;
        }
        Ok(handle)
    }

    pub async fn is_valid(&self, session_id: &str) -> bool {
        let Some(handle) = self.resolve(session_id).await else {
            return false;
        };
        if handle.is_expired(self.ttl) {
            self.remove(session_id).await;
            return false;
        }
        handle.touch();
        true
    }

    pub async fn find(&self, session_id: &str) -> Option<Arc<SessionHandle>> {
        if self.is_valid(session_id).await {
            let sessions = self.sessions.lock().await;
            sessions.get(session_id).cloned()
        } else {
            None
        }
    }

    pub async fn persist(&self, handle: &SessionHandle) -> AppResult<()> {
        let Some(vault) = self.vault.as_ref() else {
            handle.mark_persisted();
            return Ok(());
        };

        let mut snapshot = {
            let session = handle.lock().await;
            session.to_snapshot()
        };

        snapshot.last_access = handle.last_access();
        vault.save(&snapshot)?;
        handle.mark_persisted();
        Ok(())
    }

    pub async fn remove(&self, session_id: &str) {
        {
            let mut sessions = self.sessions.lock().await;
            sessions.remove(session_id);
        }

        if let Some(vault) = self.vault.as_ref() {
            vault.remove(session_id);
        }
    }

    pub async fn cleanup(&self) {
        let ttl = self.ttl;
        let expired: Vec<String> = {
            let mut sessions = self.sessions.lock().await;
            let expired: Vec<String> =
                sessions.iter().filter(|(_, handle)| handle.is_expired(ttl)).map(|(id, _)| id.clone()).collect();
            sessions.retain(|_, handle| !handle.is_expired(ttl));
            expired
        };

        let Some(vault) = self.vault.as_ref() else {
            return;
        };
        for session_id in expired {
            vault.remove(&session_id);
        }
    }

    async fn resolve(&self, session_id: &str) -> Option<Arc<SessionHandle>> {
        let existing = {
            let sessions = self.sessions.lock().await;
            sessions.get(session_id).cloned()
        };
        if existing.is_some() {
            return existing;
        }
        self.restore(session_id).await
    }

    async fn restore(&self, session_id: &str) -> Option<Arc<SessionHandle>> {
        let vault = self.vault.as_ref()?;
        let snapshot = vault.load(session_id)?;
        let session = match SrpSession::from_snapshot(&snapshot) {
            Ok(session) => session,
            Err(error) => {
                log::warn!("[SESSION] could not restore session from vault; treating as signed out: {error}");
                return None;
            }
        };

        log::info!(
            "[SESSION] restored session from vault (id length {}, apple_id length {})",
            snapshot.id.len(),
            snapshot.apple_id.len()
        );
        let handle = Arc::new(SessionHandle::with_last_access(session, snapshot.last_access));
        let mut sessions = self.sessions.lock().await;

        Some(sessions.entry(session_id.to_string()).or_insert(handle).clone())
    }

    async fn persist_or_warn(&self, handle: &SessionHandle) {
        if let Err(error) = self.persist(handle).await {
            log::warn!("[SESSION] failed to persist session; current session remains usable: {error}");
        }
    }

    fn evict_if_needed(&self, sessions: &mut HashMap<String, Arc<SessionHandle>>) {
        if sessions.len() < MAX_SESSIONS {
            return;
        }
        let oldest = sessions.iter().min_by_key(|(_, handle)| handle.last_access()).map(|(id, _)| id.clone());
        if let Some(id) = oldest {
            sessions.remove(&id);
        }
    }
}

#[cfg(test)]
pub fn test_handle() -> Arc<SessionHandle> {
    let session =
        SrpSession::new("tester@example.com".to_string(), Some("secret".to_string()), false).expect("会话构造失败");
    Arc::new(SessionHandle::new(session))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_vault_path() -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("cirrus-manager-{}", uuid::Uuid::new_v4()));
        let file = dir.join("cirrus.db");
        (dir, file)
    }

    #[tokio::test]
    async fn works_without_vault() {
        let manager = SessionManager::new(None, None);
        assert!(!manager.vault_enabled());
        let handle = manager.create("tester@example.com".into(), "secret".into(), false).await.expect("建会话失败");
        let session_id = handle.lock().await.id().to_string();
        assert!(manager.get(&session_id).await.is_ok());
        assert!(manager.is_valid(&session_id).await);
        assert!(manager.find(&session_id).await.is_some());
        manager.remove(&session_id).await;
        assert!(manager.get(&session_id).await.is_err());

        manager.remove("no-such-session").await;
    }

    #[tokio::test]
    async fn restores_session_from_vault_after_restart() {
        let (dir, file) = temp_vault_path();
        let session_id = {
            let vault = Arc::new(SessionVault::open_with_key(file.clone(), [9u8; 32]));
            let manager = SessionManager::new(None, Some(Arc::clone(&vault)));
            assert!(manager.vault_enabled());
            let handle = manager.create("tester@example.com".into(), "secret".into(), true).await.expect("建会话失败");
            let session_id = {
                let mut session = handle.lock().await;
                session.set_session_data(crate::models::session_keys::SESSION_TOKEN, "SUPER-SECRET-TOKEN");
                session.set_dsid(Some("123456789".to_string()));
                session.id().to_string()
            };

            manager.persist(&handle).await.expect("落盘失败");
            session_id
        };

        let vault = Arc::new(SessionVault::open_with_key(file, [9u8; 32]));
        let manager = SessionManager::new(None, Some(vault));
        let handle = manager.get(&session_id).await.expect("重启后必须能从保险箱恢复会话");
        let session = handle.lock().await;
        assert_eq!(session.id(), session_id);
        assert_eq!(session.session_data(crate::models::session_keys::SESSION_TOKEN), Some("SUPER-SECRET-TOKEN"));
        assert_eq!(session.dsid(), Some("123456789"));
        assert!(session.is_china_mainland());

        assert!(session.password().is_none());
        drop(session);

        manager.remove(&session_id).await;
        let reopened =
            SessionManager::new(None, Some(Arc::new(SessionVault::open_with_key(dir.join("cirrus.db"), [9u8; 32]))));
        assert!(reopened.get(&session_id).await.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn expired_vault_record_is_pruned_on_start() {
        let (dir, file) = temp_vault_path();
        let session_id = {
            let vault = Arc::new(SessionVault::open_with_key(file.clone(), [9u8; 32]));
            let manager = SessionManager::new(Some(60), Some(Arc::clone(&vault)));
            let handle = manager.create("tester@example.com".into(), "secret".into(), false).await.expect("建会话失败");
            let mut snapshot = {
                let session = handle.lock().await;
                session.to_snapshot()
            };

            snapshot.last_access = crate::session::now_millis() - 10 * 60 * 60 * 1000;
            vault.save(&snapshot).expect("落盘失败");
            snapshot.id
        };

        let manager = SessionManager::new(Some(60), Some(Arc::new(SessionVault::open_with_key(file, [9u8; 32]))));
        assert!(manager.get(&session_id).await.is_err(), "过期会话不能被恢复");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn snapshot_roundtrip_keeps_session_state() {
        let handle = manager_with_session().await;
        let session_id = handle.lock().await.id().to_string();
        let snapshot = {
            let mut session = handle.lock().await;
            session.set_session_data(crate::models::session_keys::SCNT, "SCNT-VALUE");
            session.set_flag(crate::models::session_keys::PHOTOS_VERIFIED, true);
            session.set_first_name(Some("三".to_string()));
            session.set_last_name(Some("张".to_string()));
            session.set_extra("phones", serde_json::json!([1, 2]));
            session.to_snapshot()
        };

        let restored = SrpSession::from_snapshot(&snapshot).expect("恢复失败");
        assert_eq!(restored.id(), session_id);
        assert_eq!(restored.client_id(), snapshot.client_id);
        assert_eq!(restored.auth_base(), snapshot.auth_base);
        assert_eq!(restored.session_data(crate::models::session_keys::SCNT), Some("SCNT-VALUE"));
        assert!(restored.is_flag_set(crate::models::session_keys::PHOTOS_VERIFIED));
        assert_eq!(restored.first_name(), Some("三"));
        assert_eq!(restored.extra("phones"), Some(&serde_json::json!([1, 2])));

        assert!(restored.password().is_none());
        assert!(restored.private_key_a().is_none());
        assert!(restored.a_bytes().is_none());
        assert!(restored.init_data().is_none());
    }

    async fn manager_with_session() -> Arc<SessionHandle> {
        let manager = SessionManager::new(None, None);
        manager.create("tester@example.com".into(), "secret".into(), false).await.expect("建会话失败")
    }
}
