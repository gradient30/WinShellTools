use crate::ai_client;
use crate::ai_config::{self, AiConfig, AiProfile, ChatMessage};
use crate::chat_session::{self, ChatSession, SessionSummary};
use crate::consistency;
use crate::error::{DstError, DstResult};
use crate::export;
use crate::function_edit::{self, FunctionDraft, FunctionTestResult};
use crate::init_progress;
use crate::install_mgr;
use crate::logging;
use crate::migrate;
use crate::ps_parser;
use crate::safety;
use crate::sync::{self, CategoryInfo};
use crate::webview2;
use crate::workspace;
use tauri::AppHandle;

/// 将同步重活丢到阻塞线程池，避免卡住 UI / async runtime。
async fn run_blocking<T, F>(f: F) -> DstResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> DstResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| DstError::Other(format!("后台任务失败：{e}")))?
}

// ============ 工作区管理 ============

#[tauri::command]
pub fn workspace_status() -> DstResult<workspace::WorkspaceStatus> {
    workspace::status()
}

#[tauri::command]
pub fn init_workspace(app: AppHandle) -> DstResult<String> {
    init_progress::init_with_progress(&app)
}

// ============ 读取 ============

#[tauri::command]
pub fn list_public_files() -> DstResult<Vec<String>> {
    workspace::list_public_files()
}

#[tauri::command]
pub fn read_workspace_file(rel: String) -> DstResult<String> {
    workspace::read_file(&rel)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ListCategoriesResult {
    pub categories: Vec<CategoryInfo>,
    pub cached: bool,
}

#[tauri::command]
pub fn list_categories() -> DstResult<ListCategoriesResult> {
    let (mut categories, cached) = sync::scan_categories_cached_with_meta()?;
    // UI 只展示公共命令；Assert-Git 等内部辅助函数保留在源码中，但不出现在命令列表
    for c in &mut categories {
        c.functions = sync::filter_public_functions(&c.functions);
    }
    Ok(ListCategoriesResult { categories, cached })
}

#[tauri::command]
pub fn read_category_file(file_name: String) -> DstResult<String> {
    let rel = format!("Public/{file_name}");
    workspace::read_file(&rel)
}

// ============ 写入（安全检查 + 重生成，无 git）============

#[tauri::command]
pub fn write_workspace_file(rel: String, content: String, _message: String) -> DstResult<()> {
    workspace::write_file(&rel, &content)?;
    workspace::touch_last_sync()?;
    Ok(())
}

#[tauri::command]
pub fn delete_workspace_file(rel: String, _message: String) -> DstResult<()> {
    workspace::delete_file(&rel)?;
    workspace::touch_last_sync()?;
    Ok(())
}

#[tauri::command]
pub async fn create_category(file_name: String, content: String, _message: String) -> DstResult<()> {
    run_blocking(move || {
        if !file_name.ends_with(".ps1") {
            return Err(DstError::Other("分类文件名必须以 .ps1 结尾".into()));
        }
        let rel = format!("Public/{file_name}");
        if workspace::read_file(&rel).is_ok() {
            return Err(DstError::Other(format!("分类文件已存在：{file_name}")));
        }
        let report = safety::check(&content)?;
        if !report.ok {
            return Err(DstError::SafetyBlocked(report.violations.join("; ")));
        }
        let parsed = ps_parser::parse_ps1(&content)?;
        workspace::write_file(&rel, &content)?;
        sync::regenerate_with_parsed(&file_name, Some(parsed))?;
        workspace::touch_last_sync()?;
        install_mgr::spawn_sync_runtime_modules();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn delete_category(file_name: String, _message: String) -> DstResult<()> {
    run_blocking(move || {
        let rel = format!("Public/{file_name}");
        workspace::delete_file(&rel)?;
        sync::regenerate_with_parsed(&file_name, None)?;
        workspace::touch_last_sync()?;
        install_mgr::spawn_sync_runtime_modules();
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
    let ack = acknowledge_danger.unwrap_or(false);
    run_blocking(move || {
        let rel = format!("Public/{file_name}");
        let report = safety::check_with_options(&content, ack)?;
        if !report.ok {
            return Err(DstError::SafetyBlocked(report.violations.join("; ")));
        }
        let parsed = ps_parser::parse_ps1(&content)?;
        workspace::write_file(&rel, &content)?;
        sync::regenerate_with_parsed(&file_name, Some(parsed))?;
        workspace::touch_last_sync()?;
        install_mgr::spawn_sync_runtime_modules();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn sync_public(_message: String) -> DstResult<()> {
    run_blocking(move || {
        // 手动「同步公共部分」仍全量扫描，保证与磁盘完全一致
        sync::regenerate_all()?;
        workspace::touch_last_sync()?;
        install_mgr::spawn_sync_runtime_modules();
        Ok(())
    })
    .await
}

// ============ 函数级 CRUD ============

#[tauri::command]
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
    let ack = acknowledge_danger.unwrap_or(false);
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
            ack,
        )
    })
    .await
}

#[tauri::command]
pub async fn delete_function(file_name: String, func_name: String, _message: String) -> DstResult<()> {
    run_blocking(move || function_edit::delete_function(&file_name, &func_name)).await
}

#[tauri::command]
pub async fn test_function(file_name: String, func_name: String) -> DstResult<FunctionTestResult> {
    run_blocking(move || function_edit::test_function(&file_name, &func_name)).await
}

/// AI 插入结果：命令名 + 首个新命令的隔离自测（example 试跑）结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApplyAiResult {
    /// 插入的公共命令名（小写函数；大写辅助函数随写但不计入）
    pub names: Vec<String>,
    /// None = 无可自测的命令；ok=false 时命令已插入但 example 试跑失败，提示用户检查
    pub self_test: Option<FunctionTestResult>,
}

#[tauri::command]
pub async fn apply_ai_code(
    file_name: String,
    code: String,
    _message: String,
    danger_mode: Option<bool>,
) -> DstResult<ApplyAiResult> {
    let danger = danger_mode.unwrap_or(false);
    run_blocking(move || {
        let names = function_edit::apply_code_to_category_with_options(&file_name, &code, danger)?;
        // 插入后自动验收：隔离进程试跑首个新命令的 example，保证用户拿到即可用
        let self_test = match names.first() {
            Some(first) => function_edit::test_function(&file_name, first).ok(),
            None => None,
        };
        Ok(ApplyAiResult { names, self_test })
    })
    .await
}

// ============ 安装 / 卸载 ============

#[tauri::command]
pub fn install_status() -> install_mgr::InstallStatus {
    install_mgr::install_status()
}

#[tauri::command]
pub async fn install_module() -> DstResult<install_mgr::InstallResult> {
    run_blocking(install_mgr::install_module).await
}

#[tauri::command]
pub async fn uninstall_module() -> DstResult<install_mgr::InstallResult> {
    run_blocking(install_mgr::uninstall_module).await
}

// ============ 校验 ============

#[tauri::command]
pub async fn consistency_check() -> DstResult<consistency::ConsistencyReport> {
    run_blocking(consistency::check).await
}

#[tauri::command]
pub fn safety_check(code: String) -> DstResult<safety::SafetyReport> {
    safety::check(&code)
}

#[tauri::command]
pub fn validate_ps_syntax(code: String) -> DstResult<()> {
    ps_parser::validate_syntax(&code)
}

// ============ AI 配置 ============

#[tauri::command]
pub fn get_ai_config() -> DstResult<AiConfig> { ai_config::load_config() }

#[tauri::command]
pub fn save_ai_config(config: AiConfig) -> DstResult<()> { ai_config::save_config(&config) }

#[tauri::command]
pub fn save_ai_key(key: String) -> DstResult<()> { ai_config::save_key(&key) }

#[tauri::command]
pub fn get_ai_key_status() -> DstResult<AiKeyStatus> {
    match ai_config::load_key() {
        Ok(k) => {
            let masked = if k.len() > 8 { format!("{}...{}", &k[..4], &k[k.len()-4..]) } else { "****".into() };
            Ok(AiKeyStatus { configured: true, masked })
        }
        Err(_) => Ok(AiKeyStatus { configured: false, masked: String::new() }),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AiKeyStatus { pub configured: bool, pub masked: String }

#[tauri::command]
pub fn ai_ready() -> bool { ai_config::is_configured() }

#[tauri::command]
pub fn list_ai_profiles() -> DstResult<Vec<AiProfile>> { ai_config::list_profiles() }

#[derive(Debug, Clone, serde::Serialize)]
pub struct AiProfilesMeta { pub profiles: Vec<AiProfile>, pub default_profile_id: Option<String> }

#[tauri::command]
pub fn get_ai_profiles_meta() -> DstResult<AiProfilesMeta> {
    let store = ai_config::load_profiles_store()?;
    Ok(AiProfilesMeta { profiles: store.profiles, default_profile_id: store.default_profile_id })
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SaveAiProfileInput { pub profile: AiProfile, pub key: Option<String> }

#[tauri::command]
pub fn save_ai_profile(input: SaveAiProfileInput) -> DstResult<AiProfile> {
    ai_config::save_profile(input.profile, input.key.as_deref())
}

#[tauri::command]
pub fn delete_ai_profile(id: String) -> DstResult<()> { ai_config::delete_profile(&id) }

#[tauri::command]
pub fn set_default_ai_profile(id: String) -> DstResult<()> {
    let mut store = ai_config::load_profiles_store()?;
    if !store.profiles.iter().any(|p| p.id == id) {
        return Err(DstError::Other(format!("Profile 不存在：{id}")));
    }
    store.default_profile_id = Some(id);
    ai_config::save_profiles_store(&store)
}

#[tauri::command]
pub async fn test_ai_profile(id: String) -> DstResult<String> {
    let config = ai_config::load_config_for_profile(Some(&id))?;
    let api_key = ai_config::load_key_for_profile(&id)?;
    let messages = vec![ChatMessage { role: "user".into(), content: "回复 OK".into() }];
    let events = ai_client::chat_stream(&config, &api_key, messages).await?;
    let full: String = events.iter().filter(|e| e.kind == "delta").map(|e| e.content.as_str()).collect();
    if full.trim().is_empty() { return Err(DstError::Other("模型返回为空".into())); }
    Ok(full.chars().take(120).collect())
}

#[tauri::command]
pub fn list_ai_presets() -> Vec<crate::ai_presets::AiPresetView> { crate::ai_presets::list_preset_views() }

#[tauri::command]
pub fn suggest_ai_endpoint(protocol: ai_config::AiProtocol, current_base_url: Option<String>) -> crate::ai_presets::AiEndpointSuggestion {
    crate::ai_presets::suggest_endpoint(protocol, current_base_url.as_deref())
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FetchModelsInput { pub protocol: ai_config::AiProtocol, pub base_url: String, pub key: String }

#[tauri::command]
pub async fn fetch_ai_models(id: String) -> DstResult<Vec<String>> {
    let config = ai_config::load_config_for_profile(Some(&id))?;
    let api_key = ai_config::load_key_for_profile(&id)?;
    ai_client::list_models(&config, &api_key).await
}

#[tauri::command]
pub async fn fetch_ai_models_preview(input: FetchModelsInput) -> DstResult<Vec<String>> {
    if input.key.trim().is_empty() { return Err(DstError::Other("请先填写 API Key".into())); }
    let config = AiConfig { protocol: input.protocol, base_url: input.base_url, model: String::new(), temperature: 0.7, max_tokens: 8192 };
    ai_client::list_models(&config, input.key.trim()).await
}

// ============ AI 对话 ============

async fn ai_chat_inner(
    messages: Vec<ChatMessage>,
    profile_id: Option<String>,
    danger_mode: bool,
    workspace_context: Option<String>,
) -> DstResult<String> {
    let config = ai_config::load_config_for_profile(profile_id.as_deref())?;
    let id = profile_id
        .clone()
        .or_else(|| ai_config::load_profiles_store().ok()?.default_profile_id)
        .ok_or_else(|| DstError::Other("未选择 AI Profile".into()))?;
    let api_key = ai_config::load_key_for_profile(&id)?;
    let events = ai_client::chat_stream_with_options(
        &config,
        &api_key,
        messages,
        danger_mode,
        workspace_context.as_deref(),
    )
    .await?;
    let full: String = events
        .iter()
        .filter(|e| e.kind == "delta")
        .map(|e| e.content.as_str())
        .collect();
    if full.trim().is_empty() {
        return Err(DstError::Other(
            "模型返回空正文。若使用 DeepSeek 思考模式，请增大 max_tokens（≥8192）或关闭思考。".into(),
        ));
    }
    Ok(full)
}

#[tauri::command]
pub async fn ai_chat(
    messages: Vec<ChatMessage>,
    profile_id: Option<String>,
    danger_mode: Option<bool>,
    workspace_context: Option<String>,
) -> DstResult<String> {
    ai_chat_inner(messages, profile_id, danger_mode.unwrap_or(false), workspace_context).await
}

/// 停止当前进行中的 AI 流式请求（前端「停止」按钮）。
#[tauri::command]
pub fn ai_cancel_chat() {
    ai_client::cancel_chat();
}

#[tauri::command]
pub async fn ai_chat_with_validation(
    messages: Vec<ChatMessage>,
    profile_id: Option<String>,
    danger_mode: Option<bool>,
    workspace_context: Option<String>,
) -> DstResult<AiChatResult> {
    let danger = danger_mode.unwrap_or(false);
    let reply = ai_chat_inner(messages, profile_id, danger, workspace_context).await?;
    let code_blocks = ai_config::extract_code_blocks(&reply);
    run_blocking(move || {
        // 现有公共命令清单（走缓存，命中零成本）：用于重名提示
        let existing: Vec<String> = sync::scan_categories_cached()
            .unwrap_or_default()
            .iter()
            .flat_map(|c| {
                c.functions
                    .iter()
                    .filter(|f| sync::is_exported(&f.name))
                    .map(|f| f.name.clone())
            })
            .collect();
        let mut validated = vec![];
        for code in &code_blocks {
            // 一次 parse：同时得到语法结果、函数元数据与分类，避免 validate×2 + parse
            let mut exported: Vec<String> = vec![];
            let mut helpers: Vec<String> = vec![];
            let mut functions: Vec<String> = vec![];
            let mut category: Option<String> = None;
            let mut warnings: Vec<String> = vec![];
            let (syntax_ok, syntax_err) = match ps_parser::parse_ps1(code) {
                Ok(p) if p.parse_errors.is_empty() => {
                    category = p.category.as_ref().map(|c| c.name.clone());
                    for f in &p.functions {
                        functions.push(f.name.clone());
                        let is_public = f.name.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false)
                            && f.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
                        if is_public {
                            exported.push(f.name.clone());
                        } else if f.name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
                            helpers.push(f.name.clone());
                        } else {
                            // 小写开头但含非法字符（如连字符）——按名字规范提示
                            exported.push(f.name.clone());
                            warnings.push(format!("{}：命令名应仅含小写字母和数字", f.name));
                        }
                        if f.synopsis.trim().is_empty() {
                            warnings.push(format!("{} 缺少 .SYNOPSIS", f.name));
                        }
                        if f.first_example.trim().is_empty() {
                            warnings.push(format!("{} 缺少 .EXAMPLE", f.name));
                        }
                    }
                    (true, String::new())
                }
                Ok(p) => (false, p.parse_errors.join("; ")),
                Err(e) => (false, e.to_string()),
            };
            if syntax_ok && exported.is_empty() {
                warnings.push("代码块无公共命令（小写函数名），无法插入分类".into());
            }
            for n in &exported {
                if existing.iter().any(|e| e.eq_ignore_ascii_case(n)) {
                    warnings.push(format!("{n} 与已有命令重名，插入将覆盖同名函数"));
                }
            }
            let safety_report = safety::check_with_options(code, danger).unwrap_or(
                safety::SafetyReport {
                    ok: false,
                    violations: vec!["安全检查内部错误".into()],
                },
            );
            validated.push(ValidatedCodeBlock {
                code: code.clone(),
                syntax_ok,
                syntax_err,
                safety_ok: safety_report.ok,
                safety_violations: safety_report.violations,
                functions,
                exported_functions: exported,
                helper_functions: helpers,
                quality_warnings: warnings,
                category,
            });
        }
        Ok(AiChatResult {
            reply,
            code_blocks: validated,
        })
    })
    .await
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidatedCodeBlock {
    pub code: String,
    pub syntax_ok: bool,
    pub syntax_err: String,
    pub safety_ok: bool,
    pub safety_violations: Vec<String>,
    /// 块内全部函数（兼容保留）
    pub functions: Vec<String>,
    /// 公共命令（小写函数名）——用户在终端真正可敲的命令
    pub exported_functions: Vec<String>,
    /// 内部辅助函数（大写开头，随命令一并写入但不作为命令汇报）
    pub helper_functions: Vec<String>,
    /// 质量验收警告：缺 SYNOPSIS/EXAMPLE、命名不规范、无公共命令、与已有命令重名
    pub quality_warnings: Vec<String>,
    pub category: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AiChatResult { pub reply: String, pub code_blocks: Vec<ValidatedCodeBlock> }

// ============ AI 会话持久化 /resume ============

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResumeListResult {
    pub summaries: Vec<SessionSummary>,
    pub list_text: String,
}

#[tauri::command]
pub fn list_chat_sessions() -> DstResult<ResumeListResult> {
    let summaries = chat_session::list_sessions()?;
    let list_text = chat_session::format_resume_list(&summaries);
    Ok(ResumeListResult {
        summaries,
        list_text,
    })
}

#[tauri::command]
pub fn load_chat_session(id: String) -> DstResult<ChatSession> {
    let sess = chat_session::load_session(&id)?;
    chat_session::set_active_id(Some(&id))?;
    Ok(sess)
}

#[tauri::command]
pub fn save_chat_session(session: ChatSession) -> DstResult<ChatSession> {
    chat_session::save_session(session)
}

#[tauri::command]
pub fn new_chat_session(profile_id: String) -> DstResult<ChatSession> {
    chat_session::new_session(&profile_id)
}

#[tauri::command]
pub fn load_or_create_chat_session(profile_id: String) -> DstResult<ChatSession> {
    chat_session::load_or_create_active(&profile_id)
}

#[tauri::command]
pub fn set_active_chat_session(id: String) -> DstResult<()> {
    chat_session::set_active_id(Some(&id))
}

// ============ M4：迁移 / 导出导入 / 日志 / WebView2 ============

#[tauri::command]
pub fn check_migration() -> migrate::MigrationCheck { migrate::check_migration() }

#[tauri::command]
pub fn migrate_legacy() -> DstResult<migrate::MigrateResult> {
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
}

/// 导出所有 Public/*.ps1 脚本到目标目录。
#[tauri::command]
pub fn export_workspace(target_dir: String) -> DstResult<Vec<String>> {
    let files = export::export_scripts(&target_dir)?;
    logging::log(logging::LogLevel::Info, "export", &format!("导出 {} 个脚本到 {target_dir}", files.len()));
    Ok(files)
}

/// 从目录导入 ps1 脚本：逐个校验语法+安全，通过才写入，不破坏现有。
#[tauri::command]
pub fn import_workspace(source_dir: String) -> DstResult<export::ImportResult> {
    let result = export::import_scripts(&source_dir)?;
    sync::invalidate_category_cache();
    sync::regenerate_all()?;
    logging::log(logging::LogLevel::Info, "import", &format!("导入 {} 个脚本", result.imported.len()));
    Ok(result)
}

// ============ 完整配置包（双向同步 / 迁移） ============

/// 导出完整配置包（Public + Private/Common + AI 配置，不含 API Key）。
#[tauri::command]
pub async fn export_bundle(target_dir: String) -> DstResult<export::ExportBundleResult> {
    run_blocking(move || {
        let result = export::export_bundle(&target_dir)?;
        logging::log(
            logging::LogLevel::Info,
            "bundle",
            &format!("导出配置包：{} 个文件到 {}", result.files.len(), result.target_dir),
        );
        Ok(result)
    })
    .await
}

/// 预览配置包：逐文件校验并给出动作建议，不写入。
#[tauri::command]
pub async fn preview_bundle(source_dir: String) -> DstResult<export::BundlePreview> {
    run_blocking(move || export::preview_bundle(&source_dir)).await
}

/// 按勾选导入配置包；写入后自动重生成公共部分并后台同步运行时目录。
#[tauri::command]
pub async fn import_bundle(
    source_dir: String,
    selection: Vec<String>,
) -> DstResult<export::ImportBundleResult> {
    run_blocking(move || {
        let result = export::import_bundle(&source_dir, &selection)?;
        // regenerate_all 已按导入后的状态回填分类缓存，无需再 invalidate
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
pub fn list_logs() -> Vec<String> { logging::list_log_files() }

#[tauri::command]
pub fn read_log(name: String) -> DstResult<String> { Ok(logging::read_log_file(&name)) }

#[tauri::command]
pub fn webview2_status() -> DstResult<webview2::Webview2Status> { webview2::check_webview2() }

#[tauri::command]
pub fn webview2_download_url() -> String { webview2::WEBVIEW2_DOWNLOAD_URL.to_string() }