use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;

#[derive(Default)]
pub struct DownloadTransferRegistry {
    flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl DownloadTransferRegistry {
    pub async fn register(&self, transfer_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.flags.lock().await.insert(transfer_id.to_string(), Arc::clone(&flag));
        flag
    }

    pub async fn cancel(&self, transfer_id: &str) -> bool {
        let flags = self.flags.lock().await;
        match flags.get(transfer_id) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    pub async fn finish(&self, transfer_id: &str) {
        self.flags.lock().await.remove(transfer_id);
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
        assert!(!registry.cancel("missing").await);

        registry.finish("t1").await;
        assert!(!registry.cancel("t1").await);
    }
}
