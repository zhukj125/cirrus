use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cirrus_core::services::media::DownloadRateLimiter;
use cirrus_core::{database_path, now_millis, SessionManager, SessionVault, MAX_SESSIONS};

use crate::download_tasks::DownloadTaskRepository;
use crate::download_transfers::DownloadTransferRegistry;

pub struct AppState {
    pub sessions: SessionManager,

    pub download_tasks: DownloadTaskRepository,

    pub download_limiter: DownloadRateLimiter,

    pub download_transfers: DownloadTransferRegistry,

    data_dir: Option<PathBuf>,

    pub started_at: AtomicI64,
    tray_available: AtomicBool,
}

impl AppState {
    pub fn new(data_dir: &Path) -> Self {
        let vault = SessionVault::open(database_path(data_dir));
        log::info!(
            "[VAULT] session vault: {}",
            if vault.is_enabled() { "enabled (OS credential store + AES-256-GCM)" } else { "disabled for this run" }
        );
        Self {
            sessions: SessionManager::new(None, Some(Arc::new(vault))),
            download_tasks: DownloadTaskRepository::new(Some(data_dir)),
            download_limiter: DownloadRateLimiter::new(0),
            download_transfers: DownloadTransferRegistry::default(),
            data_dir: Some(data_dir.to_path_buf()),
            started_at: AtomicI64::new(now_millis()),
            tray_available: AtomicBool::new(false),
        }
    }

    pub fn without_persistence() -> Self {
        log::warn!(
            "[VAULT] no app data directory available; persistence disabled and sign-in will be required after restart"
        );
        Self {
            sessions: SessionManager::new(None, None),
            download_tasks: DownloadTaskRepository::new(None),
            download_limiter: DownloadRateLimiter::new(0),
            download_transfers: DownloadTransferRegistry::default(),
            data_dir: None,
            started_at: AtomicI64::new(now_millis()),
            tray_available: AtomicBool::new(false),
        }
    }

    pub fn data_dir(&self) -> Option<&Path> {
        self.data_dir.as_deref()
    }

    pub fn set_tray_available(&self, available: bool) {
        self.tray_available.store(available, Ordering::Relaxed);
    }

    pub fn tray_available(&self) -> bool {
        self.tray_available.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub ttl_minutes: i64,
    pub session_limit: usize,
    pub china_mainland_default: bool,
    pub version: String,

    pub vault_enabled: bool,

    pub download_task_persistence_enabled: bool,
}

impl AppState {
    pub fn runtime_info(&self) -> RuntimeInfo {
        RuntimeInfo {
            ttl_minutes: self.sessions.ttl_minutes(),
            session_limit: MAX_SESSIONS,
            china_mainland_default: false,
            version: env!("CARGO_PKG_VERSION").to_string(),
            vault_enabled: self.sessions.vault_enabled(),
            download_task_persistence_enabled: self.data_dir.is_some(),
        }
    }
}

pub fn spawn_cleanup_task(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        loop {
            ticker.tick().await;
            state.sessions.cleanup().await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_info_reports_download_persistence() {
        let dir = std::env::temp_dir().join(format!("cirrus-state-test-{}", std::process::id()));
        let state = AppState::new(&dir);
        assert!(state.runtime_info().download_task_persistence_enabled);

        let no_persistence = AppState::without_persistence();
        assert!(!no_persistence.runtime_info().download_task_persistence_enabled);
    }

    #[test]
    fn tray_availability_defaults_off_and_can_be_enabled() {
        let state = AppState::without_persistence();
        assert!(!state.tray_available());
        state.set_tray_available(true);
        assert!(state.tray_available());
    }
}
