use super::run_blocking;
use crate::error::{DstError, DstResult};
use crate::function_edit::{self, FunctionDraft, FunctionTestResult};
use crate::init_progress;
use crate::ps_parser;
use crate::runtime_sync;
use crate::safety;
use crate::sync::{self, CategoryInfo};
use crate::workspace;
use tauri::AppHandle;

#[tauri::command]
pub async fn workspace_status() -> DstResult<workspace::WorkspaceStatus> {
    run_blocking(|| {
        match workspace::recover_pending_transactions() {
            Ok(recovered) if !recovered.is_empty() => {
                log::warn!("启动时恢复了 {} 个未完成文件事务", recovered.len())
            }
            Ok(_) => {}
            Err(error) => log::error!("启动时恢复文件事务失败：{error}"),
        }
        workspace::status()
    })
    .await
}

#[tauri::command]
pub fn init_workspace(app: AppHandle) -> DstResult<String> {
    init_progress::init_with_progress(&app)
}

#[tauri::command]
pub async fn list_public_files() -> DstResult<Vec<String>> {
    run_blocking(workspace::list_public_files).await
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ListCategoriesResult {
    pub categories: Vec<CategoryInfo>,
    pub cached: bool,
}

#[tauri::command]
pub async fn list_categories() -> DstResult<ListCategoriesResult> {
    run_blocking(|| {
        let (mut categories, cached) = sync::scan_categories_cached_with_meta()?;
        for category in &mut categories {
            category.functions = sync::filter_public_functions(&category.functions);
        }
        Ok(ListCategoriesResult { categories, cached })
    })
    .await
}

#[tauri::command]
pub async fn read_category_file(file_name: String) -> DstResult<String> {
    run_blocking(move || workspace::read_file(&format!("Public/{file_name}"))).await
}

#[tauri::command]
pub async fn create_category(
    file_name: String,
    content: String,
    _message: String,
) -> DstResult<()> {
    run_blocking(move || {
        if !file_name.ends_with(".ps1") {
            return Err(DstError::Other("分类文件名必须以 .ps1 结尾".into()));
        }
        let rel = format!("Public/{file_name}");
        if workspace::read_file(&rel).is_ok() {
            return Err(DstError::Other(format!("分类文件已存在：{file_name}")));
        }
        let parsed = ps_parser::parse_ps1(&content)?;
        let report = safety::check_parsed(&parsed)?;
        if !report.ok {
            return Err(DstError::SafetyBlocked(report.violations.join("; ")));
        }
        workspace::write_file(&rel, &content)?;
        sync::regenerate_with_parsed(&file_name, Some(parsed))?;
        workspace::touch_last_sync()?;
        runtime_sync::request_sync();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn delete_category(file_name: String, _message: String) -> DstResult<()> {
    run_blocking(move || {
        workspace::delete_file(&format!("Public/{file_name}"))?;
        sync::regenerate_with_parsed(&file_name, None)?;
        workspace::touch_last_sync()?;
        runtime_sync::request_sync();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn update_category_file(
    file_name: String,
    content: String,
    acknowledge_danger: Option<bool>,
    _message: String,
) -> DstResult<()> {
    let acknowledge = acknowledge_danger.unwrap_or(false);
    run_blocking(move || {
        let rel = format!("Public/{file_name}");
        let parsed = ps_parser::parse_ps1(&content)?;
        let report = safety::check_parsed_with_options(&parsed, acknowledge)?;
        if !report.ok {
            return Err(DstError::SafetyBlocked(report.violations.join("; ")));
        }
        workspace::write_file(&rel, &content)?;
        sync::regenerate_with_parsed(&file_name, Some(parsed))?;
        workspace::touch_last_sync()?;
        runtime_sync::request_sync();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn sync_public(_message: String) -> DstResult<()> {
    run_blocking(move || {
        sync::regenerate_all()?;
        workspace::touch_last_sync()?;
        runtime_sync::request_sync();
        Ok(())
    })
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn upsert_function(
    file_name: String,
    name: String,
    synopsis: String,
    example: String,
    body: Option<String>,
    param_defaults: Option<std::collections::HashMap<String, String>>,
    acknowledge_danger: Option<bool>,
    _message: String,
) -> DstResult<()> {
    let acknowledge = acknowledge_danger.unwrap_or(false);
    run_blocking(move || {
        function_edit::upsert_function(
            &file_name,
            FunctionDraft {
                name,
                synopsis,
                example,
                body,
                param_defaults,
                extra_examples: vec![],
            },
            acknowledge,
        )
    })
    .await
}

#[tauri::command]
pub async fn delete_function(
    file_name: String,
    func_name: String,
    _message: String,
) -> DstResult<()> {
    run_blocking(move || function_edit::delete_function(&file_name, &func_name)).await
}

#[tauri::command]
pub async fn test_function(file_name: String, func_name: String) -> DstResult<FunctionTestResult> {
    run_blocking(move || function_edit::test_function(&file_name, &func_name)).await
}

#[tauri::command]
pub async fn consistency_check() -> DstResult<crate::consistency::ConsistencyReport> {
    run_blocking(crate::consistency::check).await
}

#[tauri::command]
pub async fn safety_check(code: String) -> DstResult<safety::SafetyReport> {
    run_blocking(move || safety::check(&code)).await
}

#[tauri::command]
pub async fn validate_ps_syntax(code: String) -> DstResult<()> {
    run_blocking(move || ps_parser::validate_syntax(&code)).await
}
