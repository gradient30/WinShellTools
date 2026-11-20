use crate::error::DstResult;
use crate::install_mgr;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeSyncState {
    Idle,
    Queued,
    Syncing,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSyncStatus {
    pub state: RuntimeSyncState,
    pub generation: u64,
    pub message: String,
    pub last_error: Option<String>,
    pub updated_at: String,
}

impl RuntimeSyncStatus {
    fn initial() -> Self {
        Self {
            state: RuntimeSyncState::Idle,
            generation: 0,
            message: "尚未同步".into(),
            last_error: None,
            updated_at: now(),
        }
    }
}

type Executor = Arc<dyn Fn() -> DstResult<String> + Send + Sync>;

pub struct RuntimeSyncController {
    sender: mpsc::SyncSender<()>,
    generation: Arc<AtomicU64>,
    status: Arc<Mutex<RuntimeSyncStatus>>,
}

impl RuntimeSyncController {
    fn start() -> Self {
        Self::start_with_executor(Arc::new(install_mgr::sync_runtime_modules))
    }

    fn start_with_executor(executor: Executor) -> Self {
        let (sender, receiver) = mpsc::sync_channel(1);
        let generation = Arc::new(AtomicU64::new(0));
        let status = Arc::new(Mutex::new(RuntimeSyncStatus::initial()));
        let worker_generation = Arc::clone(&generation);
        let worker_status = Arc::clone(&status);
        std::thread::spawn(move || {
            run_worker(receiver, worker_generation, worker_status, executor);
        });
        Self {
            sender,
            generation,
            status,
        }
    }

    fn request(&self) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        update_status(
            &self.status,
            RuntimeSyncStatus {
                state: RuntimeSyncState::Queued,
                generation,
                message: "等待后台同步".into(),
                last_error: None,
                updated_at: now(),
            },
        );
        let _ = self.sender.try_send(());
        generation
    }

    fn status(&self) -> RuntimeSyncStatus {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static CONTROLLER: OnceLock<RuntimeSyncController> = OnceLock::new();

pub fn initialize(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
    let _ = controller();
}

pub fn request_sync() -> u64 {
    controller().request()
}

pub fn sync_status() -> RuntimeSyncStatus {
    controller().status()
}

pub fn retry_sync() -> u64 {
    request_sync()
}

fn controller() -> &'static RuntimeSyncController {
    CONTROLLER.get_or_init(RuntimeSyncController::start)
}

fn run_worker(
    receiver: mpsc::Receiver<()>,
    generation: Arc<AtomicU64>,
    status: Arc<Mutex<RuntimeSyncStatus>>,
    executor: Executor,
) {
    let mut wait_for_signal = true;
    loop {
        if wait_for_signal && receiver.recv().is_err() {
            break;
        }
        let current_generation = drain_latest_generation(&receiver, &generation);
        update_status(
            &status,
            RuntimeSyncStatus {
                state: RuntimeSyncState::Syncing,
                generation: current_generation,
                message: "正在同步模块目录".into(),
                last_error: None,
                updated_at: now(),
            },
        );
        let result = executor();
        if generation.load(Ordering::SeqCst) > current_generation {
            wait_for_signal = false;
            continue;
        }
        match result {
            Ok(message) => update_status(
                &status,
                RuntimeSyncStatus {
                    state: RuntimeSyncState::Succeeded,
                    generation: current_generation,
                    message,
                    last_error: None,
                    updated_at: now(),
                },
            ),
            Err(error) => update_status(
                &status,
                RuntimeSyncStatus {
                    state: RuntimeSyncState::Failed,
                    generation: current_generation,
                    message: "模块目录同步失败；编辑内容已安全保存".into(),
                    last_error: Some(error.to_string()),
                    updated_at: now(),
                },
            ),
        }
        wait_for_signal = true;
    }
}

fn drain_latest_generation(receiver: &mpsc::Receiver<()>, generation: &AtomicU64) -> u64 {
    while receiver.try_recv().is_ok() {}
    generation.load(Ordering::SeqCst)
}

fn update_status(slot: &Mutex<RuntimeSyncStatus>, status: RuntimeSyncStatus) {
    let snapshot = {
        let mut current = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *current = status;
        current.clone()
    };
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit("runtime-sync-status", snapshot);
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{mpsc, Arc};

    #[test]
    fn many_requests_coalesce_to_latest_generation() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let current = AtomicU64::new(0);
        for generation in 1..=100 {
            current.store(generation, Ordering::SeqCst);
            let _ = sender.try_send(());
        }
        assert_eq!(drain_latest_generation(&receiver, &current), 100);
    }

    #[test]
    fn failed_sync_reaches_failed_status_without_panicking() {
        let controller = RuntimeSyncController::start_with_executor(Arc::new(|| {
            Err(crate::error::DstError::Other("sync failed".into()))
        }));
        let generation = controller.request();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let status = controller.status();
            if status.state == RuntimeSyncState::Failed {
                assert_eq!(status.generation, generation);
                assert!(status.last_error.is_some());
                break;
            }
            assert!(std::time::Instant::now() < deadline, "sync did not finish");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
