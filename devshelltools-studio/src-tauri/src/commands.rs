use crate::error::{DstError, DstResult};

mod ai;
mod sessions;
mod tools;
mod workspace;

pub use ai::*;
pub use sessions::*;
pub use tools::*;
pub use workspace::*;

/// 将同步重活丢到阻塞线程池，避免卡住 UI / async runtime。
pub(super) async fn run_blocking<T, F>(function: F) -> DstResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> DstResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(function)
        .await
        .map_err(|error| DstError::Other(format!("后台任务失败：{error}")))?
}
