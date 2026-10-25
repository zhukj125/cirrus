use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;

enum TransferState {
    PendingCancel,
    Active(Arc<AtomicBool>),
}

#[derive(Default)]
pub struct DownloadTransferRegistry {
    transfers: Mutex<HashMap<String, TransferState>>,
}

impl DownloadTransferRegistry {
    pub async fn register(&self, transfer_id: &str) -> Arc<AtomicBool> {
        let mut transfers = self.transfers.lock().await;
        let cancelled = matches!(transfers.get(transfer_id), Some(TransferState::PendingCancel));
        let flag = Arc::new(AtomicBool::new(cancelled));
        transfers.insert(transfer_id.to_string(), TransferState::Active(Arc::clone(&flag)));
        flag
    }

    pub async fn cancel(&self, transfer_id: &str) -> bool {
        let mut transfers = self.transfers.lock().await;
        match transfers.get_mut(transfer_id) {
            Some(TransferState::Active(flag)) => flag.store(true, Ordering::Relaxed),
            Some(TransferState::PendingCancel) => {}
            None => {
                const MAX_PENDING_CANCELS: usize = 1024;
                if transfers.len() >= MAX_PENDING_CANCELS {
                    let removable = transfers
                        .iter()
                        .find_map(|(id, state)| matches!(state, TransferState::PendingCancel).then(|| id.clone()));
                    if let Some(id) = removable {
                        transfers.remove(&id);
                    }
                }
                transfers.insert(transfer_id.to_string(), TransferState::PendingCancel);
            }
        }
        true
    }

    pub async fn finish(&self, transfer_id: &str) {
        self.transfers.lock().await.remove(transfer_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancel_sets_only_the_target_transfer_flag() {
        let registry = DownloadTransferRegistry::default();
        let first = registry.register("t1").await;
        let second = registry.register("t2").await;

        assert!(registry.cancel("t1").await);
        assert!(first.load(Ordering::Relaxed));
        assert!(!second.load(Ordering::Relaxed));

        assert!(registry.cancel("pre-register").await);
        let pre_cancelled = registry.register("pre-register").await;
        assert!(pre_cancelled.load(Ordering::Relaxed));

        registry.finish("t1").await;
        assert!(registry.cancel("t1").await);
    }
}
