use super::run_blocking;
use crate::ai_client::{self, AiStreamEvent};
use crate::ai_config::{self, AiConfig, AiProfile, ChatMessage};
use crate::ai_types::{AiChatResult, ValidatedCodeBlock};
use crate::error::{DstError, DstResult};
use crate::function_edit;
use crate::ps_parser;
use crate::safety;
use crate::sync;
use tauri::ipc::Channel;

/// AI 插入结果：本次写入的公共命令名。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApplyAiResult {
    pub names: Vec<String>,
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
        Ok(ApplyAiResult { names })
    })
    .await
}

#[tauri::command]
pub fn get_ai_config() -> DstResult<AiConfig> {
    ai_config::load_config()
}

#[tauri::command]
pub fn save_ai_config(config: AiConfig) -> DstResult<()> {
    ai_config::save_config(&config)
}

#[tauri::command]
pub fn save_ai_key(key: String) -> DstResult<()> {
    ai_config::save_key(&key)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AiKeyStatus {
    pub configured: bool,
    pub masked: String,
}

#[tauri::command]
pub fn get_ai_key_status() -> DstResult<AiKeyStatus> {
    match ai_config::load_key() {
        Ok(key) => Ok(AiKeyStatus {
            configured: true,
            masked: crate::key_store::mask_secret_for_display(&key),
        }),
        Err(_) => Ok(AiKeyStatus {
            configured: false,
            masked: String::new(),
        }),
    }
}

#[tauri::command]
pub fn ai_ready() -> bool {
    ai_config::is_configured()
}

#[tauri::command]
pub fn list_ai_profiles() -> DstResult<Vec<AiProfile>> {
    ai_config::list_profiles()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AiProfilesMeta {
    pub profiles: Vec<AiProfile>,
    pub default_profile_id: Option<String>,
}

#[tauri::command]
pub fn get_ai_profiles_meta() -> DstResult<AiProfilesMeta> {
    let store = ai_config::load_profiles_store()?;
    Ok(AiProfilesMeta {
        profiles: store.profiles,
        default_profile_id: store.default_profile_id,
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SaveAiProfileInput {
    pub profile: AiProfile,
    pub key: Option<String>,
}

#[tauri::command]
pub fn save_ai_profile(input: SaveAiProfileInput) -> DstResult<AiProfile> {
    ai_config::save_profile(input.profile, input.key.as_deref())
}

#[tauri::command]
pub fn delete_ai_profile(id: String) -> DstResult<()> {
    ai_config::delete_profile(&id)
}

#[tauri::command]
pub fn set_default_ai_profile(id: String) -> DstResult<()> {
    let mut store = ai_config::load_profiles_store()?;
    if !store.profiles.iter().any(|profile| profile.id == id) {
        return Err(DstError::Other(format!("Profile 不存在：{id}")));
    }
    store.default_profile_id = Some(id);
    ai_config::save_profiles_store(&store)
}

#[tauri::command]
pub async fn test_ai_profile(id: String) -> DstResult<String> {
    let config = ai_config::load_config_for_profile(Some(&id))?;
    let api_key = ai_config::load_key_for_profile(&id)?;
    let messages = vec![ChatMessage {
        role: "user".into(),
        content: "回复 OK".into(),
    }];
    let events = ai_client::chat_stream(&config, &api_key, messages).await?;
    let full: String = events
        .iter()
        .filter(|event| event.kind == "delta")
        .map(|event| event.content.as_str())
        .collect();
    if full.trim().is_empty() {
        return Err(DstError::Other("模型返回为空".into()));
    }
    Ok(full.chars().take(120).collect())
}

#[tauri::command]
pub fn list_ai_presets() -> Vec<crate::ai_presets::AiPresetView> {
    crate::ai_presets::list_preset_views()
}

#[tauri::command]
pub fn suggest_ai_endpoint(
    protocol: ai_config::AiProtocol,
    current_base_url: Option<String>,
) -> crate::ai_presets::AiEndpointSuggestion {
    crate::ai_presets::suggest_endpoint(protocol, current_base_url.as_deref())
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FetchModelsInput {
    pub protocol: ai_config::AiProtocol,
    pub base_url: String,
    pub key: String,
}

#[tauri::command]
pub async fn fetch_ai_models(id: String) -> DstResult<Vec<String>> {
    let config = ai_config::load_config_for_profile(Some(&id))?;
    let api_key = ai_config::load_key_for_profile(&id)?;
    ai_client::list_models(&config, &api_key).await
}

#[tauri::command]
pub async fn fetch_ai_models_preview(input: FetchModelsInput) -> DstResult<Vec<String>> {
    if input.key.trim().is_empty() {
        return Err(DstError::Other("请先填写 API Key".into()));
    }
    let config = AiConfig {
        protocol: input.protocol,
        base_url: input.base_url,
        model: String::new(),
        temperature: 0.7,
        max_tokens: 8192,
    };
    ai_client::list_models(&config, input.key.trim()).await
}

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
    collect_reply(events)
}

async fn ai_chat_inner_stream(
    messages: Vec<ChatMessage>,
    profile_id: Option<String>,
    danger_mode: bool,
    workspace_context: Option<String>,
    on_event: &Channel<AiStreamEvent>,
) -> DstResult<String> {
    let config = ai_config::load_config_for_profile(profile_id.as_deref())?;
    let id = profile_id
        .clone()
        .or_else(|| ai_config::load_profiles_store().ok()?.default_profile_id)
        .ok_or_else(|| DstError::Other("未选择 AI Profile".into()))?;
    let api_key = ai_config::load_key_for_profile(&id)?;
    let events = ai_client::chat_stream_to_channel(
        &config,
        &api_key,
        messages,
        danger_mode,
        workspace_context.as_deref(),
        on_event,
    )
    .await?;
    collect_reply(events)
}

fn collect_reply(events: Vec<ai_client::StreamEvent>) -> DstResult<String> {
    let full: String = events
        .iter()
        .filter(|event| event.kind == "delta")
        .map(|event| event.content.as_str())
        .collect();
    if full.trim().is_empty() {
        return Err(DstError::Other(
            "模型返回空正文。若使用 DeepSeek 思考模式，请增大 max_tokens（≥8192）或关闭思考。"
                .into(),
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
    ai_chat_inner(
        messages,
        profile_id,
        danger_mode.unwrap_or(false),
        workspace_context,
    )
    .await
}

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
    on_event: Channel<AiStreamEvent>,
) -> DstResult<AiChatResult> {
    let danger = danger_mode.unwrap_or(false);
    let reply = match ai_chat_inner_stream(
        messages,
        profile_id,
        danger,
        workspace_context,
        &on_event,
    )
    .await
    {
        Ok(reply) => reply,
        Err(error) => {
            let _ = on_event.send(AiStreamEvent::Error {
                message: error.to_string(),
            });
            return Err(error);
        }
    };
    let code_blocks = ai_config::extract_code_blocks(&reply);
    let result = run_blocking(move || validate_code_blocks(reply, code_blocks, danger)).await;
    match result {
        Ok(result) => {
            let _ = on_event.send(AiStreamEvent::Validation {
                result: result.clone(),
            });
            let _ = on_event.send(AiStreamEvent::Done);
            Ok(result)
        }
        Err(error) => {
            let _ = on_event.send(AiStreamEvent::Error {
                message: error.to_string(),
            });
            Err(error)
        }
    }
}

fn validate_code_blocks(
    reply: String,
    code_blocks: Vec<String>,
    danger: bool,
) -> DstResult<AiChatResult> {
    let existing: Vec<String> = sync::scan_categories_cached()
        .unwrap_or_default()
        .iter()
        .flat_map(|category| {
            category
                .functions
                .iter()
                .filter(|function| sync::is_exported(&function.name))
                .map(|function| function.name.clone())
        })
        .collect();
    let mut validated = vec![];
    for code in &code_blocks {
        let mut exported = vec![];
        let mut helpers = vec![];
        let mut functions = vec![];
        let mut category = None;
        let mut first_example = None;
        let mut warnings = vec![];
        let parsed_result = ps_parser::parse_ps1(code);
        let (syntax_ok, syntax_err) = match &parsed_result {
            Ok(parsed) if parsed.parse_errors.is_empty() => {
                category = parsed.category.as_ref().map(|value| value.name.clone());
                for function in &parsed.functions {
                    functions.push(function.name.clone());
                    let is_public = function
                        .name
                        .chars()
                        .next()
                        .map(|value| value.is_ascii_lowercase())
                        .unwrap_or(false)
                        && function
                            .name
                            .chars()
                            .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit());
                    if is_public {
                        exported.push(function.name.clone());
                        if first_example.is_none() && !function.first_example.trim().is_empty() {
                            first_example = Some(function.first_example.clone());
                        }
                    } else if function
                        .name
                        .chars()
                        .next()
                        .map(|value| value.is_ascii_uppercase())
                        .unwrap_or(false)
                    {
                        helpers.push(function.name.clone());
                    } else {
                        exported.push(function.name.clone());
                        warnings.push(format!("{}：命令名应仅含小写字母和数字", function.name));
                    }
                    if function.synopsis.trim().is_empty() {
                        warnings.push(format!("{} 缺少 .SYNOPSIS", function.name));
                    }
                    if function.first_example.trim().is_empty() {
                        warnings.push(format!("{} 缺少 .EXAMPLE", function.name));
                    }
                }
                (true, String::new())
            }
            Ok(parsed) => (false, parsed.parse_errors.join("; ")),
            Err(error) => (false, error.to_string()),
        };
        if syntax_ok && exported.is_empty() {
            warnings.push("代码块无公共命令（小写函数名），无法插入分类".into());
        }
        for name in &exported {
            if existing
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(name))
            {
                warnings.push(format!("{name} 与已有命令重名，插入将覆盖同名函数"));
            }
        }
        let safety_report = match &parsed_result {
            Ok(parsed) if parsed.parse_errors.is_empty() => {
                safety::check_parsed_with_options(parsed, danger).unwrap_or(safety::SafetyReport {
                    ok: false,
                    violations: vec!["安全检查内部错误".into()],
                })
            }
            Ok(parsed) => safety::SafetyReport {
                ok: false,
                violations: parsed.parse_errors.clone(),
            },
            Err(error) => safety::SafetyReport {
                ok: false,
                violations: vec![error.to_string()],
            },
        };
        validated.push(ValidatedCodeBlock {
            code: code.clone(),
            syntax_ok,
            syntax_err,
            safety_ok: safety_report.ok,
            safety_violations: safety_report.violations,
            functions,
            exported_functions: exported,
            helper_functions: helpers,
            first_example,
            quality_warnings: warnings,
            category,
        });
    }
    Ok(AiChatResult {
        reply,
        code_blocks: validated,
    })
}
