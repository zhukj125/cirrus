use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

const UPDATE_DOWNLOAD_PROGRESS_EVENT: &str = "app-update-progress";
const DOWNLOAD_CANCELED_ERROR: &str = "Update download canceled.";
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    pub current_version: String,
    pub version: String,
    pub body: Option<String>,
    pub date: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Default)]
struct ProgressGate {
    last_percentage: Option<u128>,
    emitted: bool,
}

impl ProgressGate {
    fn should_emit(&mut self, downloaded: u64, total: Option<u64>) -> bool {
        let percentage = total.filter(|total| *total > 0).map(|total| {
            let total = u128::from(total);
            (u128::from(downloaded) * 100 + total / 2) / total
        });
        if self.emitted && self.last_percentage == percentage {
            return false;
        }
        self.emitted = true;
        self.last_percentage = percentage;
        true
    }
}

struct DownloadCancellation {
    canceled: tokio::sync::watch::Sender<bool>,
}

impl Default for DownloadCancellation {
    fn default() -> Self {
        let (canceled, _) = tokio::sync::watch::channel(false);
        Self { canceled }
    }
}

impl DownloadCancellation {
    fn cancel(&self) {
        self.canceled.send_replace(true);
    }

    fn is_canceled(&self) -> bool {
        *self.canceled.borrow()
    }

    async fn canceled(&self) {
        let mut canceled = self.canceled.subscribe();
        if *canceled.borrow() {
            return;
        }
        while canceled.changed().await.is_ok() {
            if *canceled.borrow_and_update() {
                return;
            }
        }
    }
}

struct ReadyUpdate {
    update: Update,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct PendingUpdate {
    cancellation: Option<Arc<DownloadCancellation>>,
    ready: Option<ReadyUpdate>,
    installing: bool,
}

#[derive(Default)]
pub struct PendingUpdateState {
    pending: Mutex<PendingUpdate>,
}

impl PendingUpdateState {
    fn begin_download(&self) -> Result<Arc<DownloadCancellation>, String> {
        let mut pending = self.pending.lock().map_err(|_| "Update state is unavailable.".to_string())?;
        if pending.cancellation.is_some() || pending.ready.is_some() || pending.installing {
            return Err("An update is already downloading or ready to install.".to_string());
        }
        let cancellation = Arc::new(DownloadCancellation::default());
        pending.cancellation = Some(Arc::clone(&cancellation));
        Ok(cancellation)
    }

    fn finish_download(&self, cancellation: &Arc<DownloadCancellation>, ready: ReadyUpdate) -> Result<(), String> {
        let mut pending = self.pending.lock().map_err(|_| "Update state is unavailable.".to_string())?;
        let is_current = pending
            .cancellation
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, cancellation) && !cancellation.is_canceled());
        if !is_current {
            return Err(DOWNLOAD_CANCELED_ERROR.to_string());
        }
        pending.cancellation = None;
        pending.ready = Some(ready);
        Ok(())
    }

    fn finish_failed_download(&self, cancellation: &Arc<DownloadCancellation>) -> Result<(), String> {
        let mut pending = self.pending.lock().map_err(|_| "Update state is unavailable.".to_string())?;
        if pending.cancellation.as_ref().is_some_and(|current| Arc::ptr_eq(current, cancellation)) {
            pending.cancellation = None;
        }
        Ok(())
    }

    fn cancel_download(&self) {
        if let Ok(mut pending) = self.pending.lock() {
            if let Some(cancellation) = pending.cancellation.take() {
                cancellation.cancel();
            }
        }
    }

    fn take_ready(&self) -> Result<ReadyUpdate, String> {
        let mut pending = self.pending.lock().map_err(|_| "Update state is unavailable.".to_string())?;
        if pending.installing {
            return Err("The update is already being installed.".to_string());
        }
        let update = pending.ready.take().ok_or_else(|| "No downloaded update is ready to install.".to_string())?;
        pending.installing = true;
        Ok(update)
    }

    fn restore_ready(&self, ready: ReadyUpdate) -> Result<(), String> {
        let mut pending = self.pending.lock().map_err(|_| "Update state is unavailable.".to_string())?;
        pending.installing = false;
        pending.ready = Some(ready);
        Ok(())
    }

    fn finish_install(&self) -> Result<(), String> {
        let mut pending = self.pending.lock().map_err(|_| "Update state is unavailable.".to_string())?;
        pending.installing = false;
        Ok(())
    }
}

#[tauri::command]
pub async fn app_update_check(app: AppHandle) -> Result<Option<AppUpdateInfo>, String> {
    let updater = app.updater().map_err(|error| format!("Could not initialize the updater: {error}"))?;
    let update = updater.check().await.map_err(|error| format!("Could not check for updates: {error}"))?;
    Ok(update.map(|update| AppUpdateInfo {
        current_version: update.current_version,
        version: update.version,
        body: update.body,
        date: update.date.map(|date| date.to_string()),
    }))
}

#[tauri::command]
pub async fn app_update_download(app: AppHandle, state: State<'_, PendingUpdateState>) -> Result<(), String> {
    let updater = app.updater().map_err(|error| format!("Could not initialize the updater: {error}"))?;
    let update = updater
        .check()
        .await
        .map_err(|error| format!("Could not check for updates: {error}"))?
        .ok_or_else(|| "No update is available.".to_string())?;
    let cancellation = state.begin_download()?;

    let downloaded = Arc::new(AtomicU64::new(0));
    let chunk_downloaded = Arc::clone(&downloaded);
    let finish_downloaded = Arc::clone(&downloaded);
    let chunk_app = app.clone();
    let finish_app = app.clone();
    let mut progress_gate = ProgressGate::default();
    let download_update = update.clone();

    let download = async move {
        let result = download_update
            .download(
                move |chunk_len, total| {
                    let current = chunk_downloaded
                        .fetch_add(chunk_len as u64, Ordering::Relaxed)
                        .saturating_add(chunk_len as u64);
                    if progress_gate.should_emit(current, total) {
                        let _ = chunk_app
                            .emit(UPDATE_DOWNLOAD_PROGRESS_EVENT, AppUpdateProgress { downloaded: current, total });
                    }
                },
                move || {
                    let current = finish_downloaded.load(Ordering::Relaxed);
                    let _ = finish_app.emit(
                        UPDATE_DOWNLOAD_PROGRESS_EVENT,
                        AppUpdateProgress { downloaded: current, total: Some(current) },
                    );
                },
            )
            .await;
        result.map_err(|error| format!("Could not download the update: {error}"))
    };

    let result = tokio::select! {
        _ = cancellation.canceled() => Err(DOWNLOAD_CANCELED_ERROR.to_string()),
        result = tokio::time::timeout(DOWNLOAD_TIMEOUT, download) => {
            result.map_err(|_| "The update download timed out.".to_string())?
        }
    };

    match result {
        Ok(bytes) => state.finish_download(&cancellation, ReadyUpdate { update, bytes }),
        Err(error) => {
            let was_canceled = cancellation.is_canceled();
            state.finish_failed_download(&cancellation)?;
            if was_canceled {
                Err(DOWNLOAD_CANCELED_ERROR.to_string())
            } else {
                Err(error)
            }
        }
    }
}

#[tauri::command]
pub fn app_update_cancel_download(state: State<'_, PendingUpdateState>) {
    state.cancel_download();
}

#[tauri::command]
pub fn app_update_install(state: State<'_, PendingUpdateState>) -> Result<(), String> {
    let ready = state.take_ready()?;
    if let Err(error) = ready.update.install(&ready.bytes) {
        state.restore_ready(ready)?;
        return Err(format!("Could not install the update: {error}"));
    }
    state.finish_install()?;
    Ok(())
}

#[tauri::command]
pub fn app_update_restart(app: AppHandle) {
    app.request_restart();
}

#[cfg(test)]
mod tests {
    use super::{AppUpdateInfo, AppUpdateProgress, ProgressGate};

    #[test]
    fn progress_gate_emits_only_when_visible_percentage_changes() {
        let mut gate = ProgressGate::default();
        assert!(gate.should_emit(0, Some(100)));
        assert!(!gate.should_emit(0, Some(100)));
        assert!(gate.should_emit(1, Some(100)));
        assert!(!gate.should_emit(1, Some(100)));
    }

    #[test]
    fn progress_gate_emits_the_first_unknown_length_chunk() {
        let mut gate = ProgressGate::default();
        assert!(gate.should_emit(128, None));
        assert!(!gate.should_emit(256, None));
    }
    #[test]
    fn update_contract_uses_frontend_friendly_field_names() {
        let info = AppUpdateInfo {
            current_version: "1.4.3".to_string(),
            version: "1.5.0".to_string(),
            body: Some("Changes".to_string()),
            date: None,
        };
        let value = serde_json::to_value(info).expect("serialize update info");
        assert_eq!(value["currentVersion"], "1.4.3");
        assert_eq!(value["version"], "1.5.0");

        let progress = AppUpdateProgress { downloaded: 12, total: Some(24) };
        let value = serde_json::to_value(progress).expect("serialize update progress");
        assert_eq!(value["downloaded"], 12);
        assert_eq!(value["total"], 24);
    }
}
