use super::run_blocking;
use crate::error::DstResult;
use crate::export;
use crate::install_mgr;
use crate::logging;
use crate::migrate;
use crate::pwsh;
use crate::runtime_sync;
use crate::sync;
use crate::webview2;

#[tauri::command]
pub async fn install_status() -> install_mgr::InstallStatus {
    run_blocking(|| Ok(install_mgr::install_status()))
        .await
        .unwrap_or(install_mgr::InstallStatus {
            workspace_ready: false,
            ps51_module_present: false,
            ps7_module_present: false,
            profile_configured: false,
            installed: false,
        })
}

#[tauri::command]
pub async fn install_module() -> DstResult<install_mgr::InstallResult> {
    run_blocking(install_mgr::install_module).await
}

#[tauri::command]
pub fn runtime_sync_status() -> runtime_sync::RuntimeSyncStatus {
    runtime_sync::sync_status()
}

#[tauri::command]
pub fn retry_runtime_sync() -> u64 {
    runtime_sync::retry_sync()
}

#[tauri::command]
pub async fn uninstall_module() -> DstResult<install_mgr::InstallResult> {
    run_blocking(install_mgr::uninstall_module).await
}

#[tauri::command]
pub async fn check_migration() -> migrate::MigrationCheck {
    run_blocking(|| Ok(migrate::check_migration()))
        .await
        .unwrap_or(migrate::MigrationCheck {
            has_legacy: false,
            legacy_dirs: vec![],
            workspace_initialized: false,
        })
}

#[tauri::command]
pub async fn migrate_legacy() -> DstResult<migrate::MigrateResult> {
    run_blocking(move || {
        let result = migrate::migrate_from_legacy()?;
        logging::log(
            logging::LogLevel::Info,
            "migrate",
            &format!(
                "迁移 {} 个文件；归档 {} 处",
                result.migrated_files.len(),
                result.archived_dirs.len()
            ),
        );
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn export_workspace(target_dir: String) -> DstResult<Vec<String>> {
    run_blocking(move || {
        let files = export::export_scripts(&target_dir)?;
        logging::log(
            logging::LogLevel::Info,
            "export",
            &format!("导出 {} 个脚本到 {target_dir}", files.len()),
        );
        Ok(files)
    })
    .await
}

#[tauri::command]
pub async fn import_workspace(source_dir: String) -> DstResult<export::ImportResult> {
    run_blocking(move || {
        let result = export::import_scripts(&source_dir)?;
        sync::invalidate_category_cache();
        sync::regenerate_all()?;
        logging::log(
            logging::LogLevel::Info,
            "import",
            &format!("导入 {} 个脚本", result.imported.len()),
        );
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn export_bundle(target_dir: String) -> DstResult<export::ExportBundleResult> {
    run_blocking(move || {
        let result = export::export_bundle(&target_dir)?;
        logging::log(
            logging::LogLevel::Info,
            "bundle",
            &format!(
                "导出配置包：{} 个文件到 {}",
                result.files.len(),
                result.target_dir
            ),
        );
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn preview_bundle(source_dir: String) -> DstResult<export::BundlePreview> {
    run_blocking(move || export::preview_bundle(&source_dir)).await
}

#[tauri::command]
pub async fn import_bundle(
    source_dir: String,
    selection: Vec<String>,
) -> DstResult<export::ImportBundleResult> {
    run_blocking(move || {
        let result = export::import_bundle(&source_dir, &selection)?;
        logging::log(
            logging::LogLevel::Info,
            "bundle",
            &format!("导入配置包：{} 项", result.imported.len()),
        );
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn list_logs() -> Vec<String> {
    run_blocking(|| Ok(logging::list_log_files()))
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn read_log(name: String) -> DstResult<String> {
    run_blocking(move || logging::read_log_file(&name)).await
}

#[tauri::command]
pub async fn webview2_status() -> DstResult<webview2::Webview2Status> {
    static CACHE: std::sync::OnceLock<webview2::Webview2Status> = std::sync::OnceLock::new();
    run_blocking(|| {
        let status = CACHE
            .get_or_init(|| {
                webview2::check_webview2().unwrap_or(webview2::Webview2Status {
                    installed: false,
                    version: String::new(),
                    needs_guidance: false,
                })
            })
            .clone();
        Ok(status)
    })
    .await
}

#[tauri::command]
pub fn webview2_download_url() -> String {
    webview2::WEBVIEW2_DOWNLOAD_URL.to_string()
}

#[tauri::command]
pub async fn pwsh_status() -> DstResult<pwsh::PwshStatus> {
    run_blocking(|| Ok(pwsh::check_pwsh())).await
}

#[tauri::command]
pub fn pwsh_download_url() -> String {
    pwsh::PWSH_DOWNLOAD_URL.to_string()
}
