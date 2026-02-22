use crate::caching::external::ExternalCache;
use std::time::Duration;
use tokio::task::JoinHandle;

/// spawns a heartbeat task to extend a distributed lock.
///
/// runs until `extend_lock` returns false (lock expired or lost).
/// heartbeat interval is `lock_heartbeat_ms` from cache config.
pub(super) fn spawn_lock_heartbeat(
    cache: ExternalCache,
    lock_key: String,
    lock_token: String,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let interval = Duration::from_millis(cache.lock_heartbeat_ms());
        loop {
            tokio::time::sleep(interval).await;
            if !cache.extend_lock(&lock_key, &lock_token).await {
                break;
            }
        }
    })
}

/// spawns heartbeat for result computation lock.
///
/// thin wrapper around `spawn_lock_heartbeat` for semantic clarity.
pub(super) fn spawn_result_lock_heartbeat(
    cache: ExternalCache,
    lock_key: String,
    lock_token: String,
) -> JoinHandle<()> {
    spawn_lock_heartbeat(cache, lock_key, lock_token)
}
