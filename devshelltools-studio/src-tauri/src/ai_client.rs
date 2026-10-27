use crate::ai_config::{self, AiConfig, AiProtocol, ChatMessage};
use crate::error::{DstError, DstResult};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

/// 用户点击「停止」时置位；流式读取循环会尽快中断。
static CHAT_CANCEL: AtomicBool = AtomicBool::new(false);

/// 请求取消当前 AI 对话（可在任意线程调用）。
pub fn cancel_chat() {
    CHAT_CANCEL.store(true, Ordering::SeqCst);
}

fn reset_chat_cancel() {
    CHAT_CANCEL.store(false, Ordering::SeqCst);
}

fn chat_cancelled() -> bool {
    CHAT_CANCEL.load(Ordering::SeqCst)
}

fn cancelled_error() -> DstError {
    DstError::Other("已停止生成".into())
}

/// 一次流式请求的输入
#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
}

/// 流式产生的事件
#[derive(Debug, Clone, Serialize)]
pub struct StreamEvent {
    pub kind: String, // "delta" | "done" | "error"
    pub content: String,
}

/// 构造一次完整 chat 请求并启动流式。
/// 返回一个 Vec<StreamEvent>（M3 阶段先收集后返回，UI 异步展示；
/// 真正逐 token 推送需 Tauri event，M3.5 优化）。
pub async fn chat_stream(
    config: &AiConfig,
    api_key: &str,
    messages: Vec<ChatMessage>,
) -> DstResult<Vec<StreamEvent>> {
    chat_stream_with_options(config, api_key, messages, false, None).await
}

/// `extra_system` 追加到固定 system prompt 之后（如工作区现有命令清单），
/// OpenAI / Anthropic 两协议统一走同一条 system 消息。
pub async fn chat_stream_with_options(
    config: &AiConfig,
    api_key: &str,
    messages: Vec<ChatMessage>,
    danger_mode: bool,
    extra_system: Option<&str>,
) -> DstResult<Vec<StreamEvent>> {
    reset_chat_cancel();
    let mut system_content = ai_config::system_prompt_with_options(danger_mode);
    if let Some(extra) = extra_system {
        let trimmed = extra.trim();
        if !trimmed.is_empty() {
            system_content.push_str("\n\n");
            system_content.push_str(trimmed);
        }
    }
    let mut full_messages = vec![ChatMessage {
        role: "system".into(),
        content: system_content,
    }];
    full_messages.extend(messages);

    match config.protocol {
        AiProtocol::Openai => chat_openai(config, api_key, full_messages).await,
        AiProtocol::Anthropic => chat_anthropic(config, api_key, full_messages).await,
    }
}

// ============ 端点归一化 ============

/// 纠正已知失效/错误的 OpenAI 兼容 Base URL。
/// 例如旧预设 `https://api.ollama.cloud/v1` 现已全局 503，应改用官方 `https://ollama.com/v1`。
fn normalize_openai_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let lower = trimmed.to_lowercase();
    if lower.contains("api.ollama.cloud") {
        return "https://ollama.com/v1".into();
    }
    trimmed.to_string()
}

// ============ 采样参数策略 ============

/// 部分模型对 temperature 有硬性限制：
/// - Kimi K2/K3 系列：固定采样，官方要求不传（传非 1 会 400）
/// - OpenAI o1/o3/o4 推理系列：不支持 temperature
/// - deepseek-reasoner 等：历史上曾拒绝该参数，省略更安全
/// 返回 None 表示请求体中省略该字段。
fn temperature_for_request(model: &str, configured: f64) -> Option<f64> {
    let m = model.to_lowercase();
    if m.starts_with("kimi-k") {
        return None;
    }
    if m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4-")
        || m == "o4"
    {
        return None;
    }
    if m.contains("reasoner") {
        return None;
    }
    Some(configured)
}

// ============ OpenAI 协议 ============

#[derive(Debug, Clone, Serialize)]
struct ThinkingParam {
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Serialize)]
struct OpenaiRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    max_tokens: u32,
    stream: bool,
    /// DeepSeek V4 默认 thinking=enabled，思考 token 计入 max_tokens，易 length 截断无正文
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingParam>,
}

/// DeepSeek（官方端点或模型名）：Studio 审阅场景默认关闭思考，保证稳定产出正文。
fn deepseek_disable_thinking(base_url: &str, model: &str) -> Option<ThinkingParam> {
    let u = base_url.to_lowercase();
    let m = model.to_lowercase();
    if u.contains("deepseek.com") || m.starts_with("deepseek") {
        Some(ThinkingParam {
            kind: "disabled".into(),
        })
    } else {
        None
    }
}

/// 网关因不认识 thinking 字段而 4xx 时才去掉该字段重试（避免 401/429 误去掉后重新打开思考）。
fn is_thinking_param_rejected(err: &DstError) -> bool {
    let s = err.to_string().to_lowercase();
    let is_client_err = s.contains(" 400")
        || s.contains(" 422")
        || s.contains("返回 400")
        || s.contains("返回 422");
    is_client_err
        && (s.contains("thinking")
            || s.contains("unknown field")
            || s.contains("unexpected")
            || s.contains("unrecognized"))
}

/// 审阅类长输出需要更大预算；DeepSeek 抬底，避免截断。
fn effective_max_tokens(base_url: &str, model: &str, configured: u32) -> u32 {
    let deepseek = base_url.to_lowercase().contains("deepseek.com")
        || model.to_lowercase().starts_with("deepseek");
    let floor = if deepseek { 16384 } else { 2048 };
    configured.max(floor)
}

#[derive(Deserialize)]
struct OpenaiStreamChunk {
    #[serde(default)]
    choices: Vec<OpenaiStreamChoice>,
}

#[derive(Deserialize)]
struct OpenaiStreamChoice {
    delta: Option<OpenaiStreamDelta>,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenaiStreamDelta {
    content: Option<String>,
    /// DeepSeek / 部分思考模型：思考过程走此字段，正文仍在 content
    reasoning_content: Option<String>,
}

async fn chat_openai(
    config: &AiConfig,
    api_key: &str,
    messages: Vec<ChatMessage>,
) -> DstResult<Vec<StreamEvent>> {
    let base = normalize_openai_base_url(&config.base_url);
    // DeepSeek V4 默认 thinking=enabled：Studio 审阅默认关闭，避免 reasoning 占满 max_tokens
    let mut thinking = deepseek_disable_thinking(&base, &config.model);
    let mut events = match chat_openai_once(
        config,
        api_key,
        &base,
        messages.clone(),
        thinking.clone(),
    )
    .await
    {
        Ok(e) => e,
        Err(e) if thinking.is_some() && is_thinking_param_rejected(&e) => {
            log::warn!("网关拒绝 thinking 字段，去掉后重试：{e}");
            thinking = None;
            chat_openai_once(config, api_key, &base, messages.clone(), None).await?
        }
        Err(e) => return Err(e),
    };

    if openai_needs_thinking_retry(&events) {
        if thinking.as_ref().map(|t| t.kind.as_str()) != Some("disabled") {
            thinking = Some(ThinkingParam {
                kind: "disabled".into(),
            });
            events = chat_openai_once(
                config,
                api_key,
                &base,
                messages.clone(),
                thinking.clone(),
            )
            .await?;
        }
        if openai_needs_thinking_retry(&events) {
            // 仍被 length 截断：抬高输出预算再试（并继续关闭思考）
            let mut boosted = config.clone();
            boosted.max_tokens = config.max_tokens.max(16384).saturating_mul(2).min(65536);
            events = chat_openai_once(
                &boosted,
                api_key,
                &base,
                messages,
                Some(ThinkingParam {
                    kind: "disabled".into(),
                }),
            )
            .await?;
        }
    }

    let saw_reasoning = events.iter().any(|e| e.kind == "reasoning");
    finalize_openai_events(events, saw_reasoning)
}

fn openai_needs_thinking_retry(events: &[StreamEvent]) -> bool {
    let content_empty = !events
        .iter()
        .any(|e| e.kind == "delta" && !e.content.trim().is_empty());
    let saw_reasoning = events.iter().any(|e| e.kind == "reasoning");
    let finish_length = events
        .iter()
        .rev()
        .find(|e| e.kind == "done")
        .map(|e| e.content == "length")
        .unwrap_or(false);
    content_empty && (saw_reasoning || finish_length)
}

async fn chat_openai_once(
    config: &AiConfig,
    api_key: &str,
    base: &str,
    messages: Vec<ChatMessage>,
    thinking: Option<ThinkingParam>,
) -> DstResult<Vec<StreamEvent>> {
    if chat_cancelled() {
        return Err(cancelled_error());
    }
    let url = format!("{base}/chat/completions");
    // 强制关闭思考时始终带上字段（含重试路径），避免 V4 默认 enabled
    let body = OpenaiRequest {
        model: config.model.clone(),
        messages,
        temperature: temperature_for_request(&config.model, config.temperature),
        max_tokens: effective_max_tokens(base, &config.model, config.max_tokens),
        stream: true,
        thinking,
    };
    if body.thinking.as_ref().map(|t| t.kind.as_str()) == Some("disabled") {
        log::info!(
            "DeepSeek/兼容：请求已带 thinking=disabled，max_tokens={}",
            body.max_tokens
        );
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(DstError::Http)?;

    if chat_cancelled() {
        return Err(cancelled_error());
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(DstError::Other(format!("OpenAI 返回 {status}: {text}")));
    }

    let mut events = vec![];
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut finish_reason = String::new();

    while let Some(chunk_result) = stream.next().await {
        if chat_cancelled() {
            return Err(cancelled_error());
        }
        let chunk = chunk_result.map_err(DstError::Http)?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        while let Some(pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=pos).collect();
            let line = line.trim().trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let Some(data) = line
                .strip_prefix("data: ")
                .or_else(|| line.strip_prefix("data:"))
            else {
                continue;
            };
            let data = data.trim();
            if data == "[DONE]" {
                events.push(StreamEvent {
                    kind: "done".into(),
                    content: finish_reason.clone(),
                });
                return Ok(events);
            }
            if let Ok(chunk) = serde_json::from_str::<OpenaiStreamChunk>(data) {
                if let Some(choice) = chunk.choices.first() {
                    if let Some(fr) = choice.finish_reason.as_ref() {
                        if !fr.is_empty() {
                            finish_reason = fr.clone();
                        }
                    }
                    if let Some(delta) = choice.delta.as_ref() {
                        if let Some(r) = delta.reasoning_content.as_ref() {
                            if !r.is_empty() {
                                events.push(StreamEvent {
                                    kind: "reasoning".into(),
                                    content: r.clone(),
                                });
                            }
                        }
                        if let Some(content) = delta.content.as_ref() {
                            if !content.is_empty() {
                                events.push(StreamEvent {
                                    kind: "delta".into(),
                                    content: content.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    if chat_cancelled() {
        return Err(cancelled_error());
    }
    events.push(StreamEvent {
        kind: "done".into(),
        content: finish_reason,
    });
    Ok(events)
}

/// 思考模型若把 token 耗在 reasoning 上，正文可能为空——直接报明确错误，避免前端“假中断”。
fn finalize_openai_events(
    events: Vec<StreamEvent>,
    saw_reasoning: bool,
) -> DstResult<Vec<StreamEvent>> {
    let content_len: usize = events
        .iter()
        .filter(|e| e.kind == "delta")
        .map(|e| e.content.len())
        .sum();
    if content_len == 0 && saw_reasoning {
        let preview: String = events
            .iter()
            .filter(|e| e.kind == "reasoning")
            .map(|e| e.content.as_str())
            .collect::<String>()
            .chars()
            .take(120)
            .collect();
        let finish = events
            .iter()
            .rev()
            .find(|e| e.kind == "done")
            .map(|e| e.content.as_str())
            .unwrap_or("");
        return Err(DstError::Other(format!(
            "模型只返回了思考过程、未产出正文（常见原因：思考模式占满 max_tokens）。\
DeepSeek 请确认已使用官方端点（会自动关闭思考）；或将 max_tokens 调到 16384+。\
finish_reason={finish}；思考预览：{preview}…"
        )));
    }
    Ok(events)
}

// ============ Anthropic 协议 ============

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<ChatMessage>,
    system: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    max_tokens: u32,
    stream: bool,
    /// DeepSeek Anthropic 兼容端同样支持 thinking 开关
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingParam>,
}

#[derive(Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<AnthropicDelta>,
}

#[derive(Deserialize)]
struct AnthropicDelta {
    #[serde(rename = "type")]
    #[allow(dead_code)]
    delta_type: Option<String>,
    text: Option<String>,
}

async fn chat_anthropic(
    config: &AiConfig,
    api_key: &str,
    messages: Vec<ChatMessage>,
) -> DstResult<Vec<StreamEvent>> {
    if chat_cancelled() {
        return Err(cancelled_error());
    }
    let url = format!("{}/messages", config.base_url.trim_end_matches('/'));

    // Anthropic 的 system 是顶层字段，不在 messages 里
    let system = messages
        .iter()
        .find(|m| m.role == "system")
        .map(|m| m.content.clone())
        .unwrap_or_default();
    let user_messages: Vec<ChatMessage> = messages
        .into_iter()
        .filter(|m| m.role != "system")
        .collect();

    let thinking = deepseek_disable_thinking(&config.base_url, &config.model);
    let body = AnthropicRequest {
        model: config.model.clone(),
        messages: user_messages,
        system,
        temperature: temperature_for_request(&config.model, config.temperature),
        max_tokens: effective_max_tokens(&config.base_url, &config.model, config.max_tokens),
        stream: true,
        thinking,
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(DstError::Http)?;

    if chat_cancelled() {
        return Err(cancelled_error());
    }

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(DstError::Other(format!("Anthropic 返回 {status}: {text}")));
    }

    let mut events = vec![];
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk_result) = stream.next().await {
        if chat_cancelled() {
            return Err(cancelled_error());
        }
        let chunk = chunk_result.map_err(DstError::Http)?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        while let Some(pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=pos).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(data) = line.strip_prefix("data: ") {
                if let Ok(event) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                    match event.event_type.as_str() {
                        "content_block_delta" => {
                            if let Some(delta) = event.delta {
                                if let Some(text) = delta.text {
                                    events.push(StreamEvent {
                                        kind: "delta".into(),
                                        content: text,
                                    });
                                }
                            }
                        }
                        "message_stop" => {
                            events.push(StreamEvent {
                                kind: "done".into(),
                                content: String::new(),
                            });
                            return Ok(events);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    events.push(StreamEvent {
        kind: "done".into(),
        content: String::new(),
    });
    Ok(events)
}

#[derive(Deserialize)]
struct OpenaiModelsResponse {
    data: Vec<OpenaiModelItem>,
}

#[derive(Deserialize)]
struct OpenaiModelItem {
    id: String,
}

/// 拉取可用模型列表。
/// OpenAI 和 Anthropic 协议都走 GET /v1/models 真实拉取（DeepSeek 等 OpenAI 兼容服务也支持此端点）。
/// Anthropic 官方 API 也可用 /v1/models（需 x-api-key 头）。
/// Ollama（含 Cloud）在 OpenAI `/models` 失败时回退原生 `/api/tags`。
pub async fn list_models(config: &AiConfig, api_key: &str) -> DstResult<Vec<String>> {
    let base = match config.protocol {
        AiProtocol::Openai => normalize_openai_base_url(&config.base_url),
        AiProtocol::Anthropic => config.base_url.trim().trim_end_matches('/').to_string(),
    };
    let url = format!("{base}/models");
    let client = reqwest::Client::new();
    let req = match config.protocol {
        AiProtocol::Openai => client.get(&url).bearer_auth(api_key),
        AiProtocol::Anthropic => client
            .get(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01"),
    };
    let resp = req.send().await.map_err(DstError::Http)?;

    if resp.status().is_success() {
        let parsed: OpenaiModelsResponse = resp.json().await.map_err(DstError::Http)?;
        let mut ids: Vec<String> = parsed.data.into_iter().map(|m| m.id).collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return Err(DstError::Other("模型列表为空".into()));
        }
        return Ok(ids);
    }

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    // Ollama Cloud / 本地：回退原生 tags 接口
    if matches!(config.protocol, AiProtocol::Openai) && is_ollama_base(&base) {
        if let Ok(ids) = list_ollama_tags(&client, &base, api_key).await {
            return Ok(ids);
        }
    }

    Err(DstError::Other(format!("拉取模型失败 {status}：{text}")))
}

fn is_ollama_base(base_url: &str) -> bool {
    let u = base_url.to_lowercase();
    u.contains("ollama.com") || u.contains("ollama.cloud") || u.contains(":11434")
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaTagItem>,
}

#[derive(Deserialize)]
struct OllamaTagItem {
    name: String,
}

async fn list_ollama_tags(
    client: &reqwest::Client,
    openai_base: &str,
    api_key: &str,
) -> DstResult<Vec<String>> {
    let tags_url = if let Some(root) = openai_base
        .trim_end_matches('/')
        .strip_suffix("/v1")
    {
        format!("{root}/api/tags")
    } else {
        format!("{}/api/tags", openai_base.trim_end_matches('/'))
    };
    let mut req = client.get(&tags_url);
    if !api_key.trim().is_empty() {
        req = req.bearer_auth(api_key);
    }
    let resp = req.send().await.map_err(DstError::Http)?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(DstError::Other(format!(
            "Ollama /api/tags 失败 {status}：{text}"
        )));
    }
    let parsed: OllamaTagsResponse = resp.json().await.map_err(DstError::Http)?;
    let mut ids: Vec<String> = parsed.models.into_iter().map(|m| m.name).collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Err(DstError::Other("模型列表为空".into()));
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_event_serialize() {
        let e = StreamEvent {
            kind: "delta".into(),
            content: "hi".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("delta"));
        assert!(json.contains("hi"));
    }

    #[test]
    fn kimi_k_models_omit_temperature() {
        assert_eq!(temperature_for_request("kimi-k3", 0.7), None);
        assert_eq!(temperature_for_request("kimi-k2.6", 0.7), None);
        assert_eq!(temperature_for_request("Kimi-K2.5", 1.0), None);
    }

    #[test]
    fn normal_models_keep_temperature() {
        assert_eq!(temperature_for_request("deepseek-chat", 0.7), Some(0.7));
        assert_eq!(temperature_for_request("glm-4-flash", 0.5), Some(0.5));
        assert_eq!(temperature_for_request("qwen-plus", 0.7), Some(0.7));
        assert_eq!(
            temperature_for_request("ernie-4.0-turbo-8k", 0.7),
            Some(0.7)
        );
        assert_eq!(
            temperature_for_request("moonshot-v1-8k", 0.3),
            Some(0.3)
        );
    }

    #[test]
    fn reasoning_models_omit_temperature() {
        assert_eq!(temperature_for_request("o1-mini", 0.7), None);
        assert_eq!(temperature_for_request("o3-mini", 0.7), None);
        assert_eq!(temperature_for_request("deepseek-reasoner", 0.7), None);
    }

    #[test]
    fn openai_request_omits_temperature_when_none() {
        let body = OpenaiRequest {
            model: "kimi-k3".into(),
            messages: vec![],
            temperature: None,
            max_tokens: 1024,
            stream: true,
            thinking: None,
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(!json.contains("temperature"));
        assert!(!json.contains("thinking"));
    }

    #[test]
    fn deepseek_request_disables_thinking() {
        let t = deepseek_disable_thinking("https://api.deepseek.com/v1", "deepseek-chat").unwrap();
        assert_eq!(t.kind, "disabled");
        assert!(
            deepseek_disable_thinking("https://api.moonshot.cn/v1", "moonshot-v1-8k").is_none()
        );
        assert!(
            deepseek_disable_thinking("https://proxy.example/v1", "deepseek-chat").is_some()
        );
        let body = OpenaiRequest {
            model: "deepseek-chat".into(),
            messages: vec![],
            temperature: Some(0.7),
            max_tokens: 8192,
            stream: true,
            thinking: Some(t),
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"thinking\""));
        assert!(json.contains("disabled"));
    }

    #[test]
    fn thinking_reject_detects_400() {
        let e = DstError::Other(
            "OpenAI 返回 400: {\"error\":{\"message\":\"unknown field thinking\"}}".into(),
        );
        assert!(is_thinking_param_rejected(&e));
        let e2 = DstError::Other("OpenAI 返回 401: invalid api key".into());
        assert!(!is_thinking_param_rejected(&e2));
    }

    #[test]
    fn deepseek_max_tokens_floor() {
        assert_eq!(
            effective_max_tokens("https://api.deepseek.com/v1", "deepseek-chat", 2048),
            16384
        );
        assert_eq!(
            effective_max_tokens("https://api.moonshot.cn/v1", "moonshot-v1-8k", 2048),
            2048
        );
        assert_eq!(
            effective_max_tokens("https://proxy.example/v1", "deepseek-v4-flash", 4096),
            16384
        );
    }

    #[test]
    fn needs_retry_on_reasoning_length() {
        let events = vec![
            StreamEvent {
                kind: "reasoning".into(),
                content: "分析中".into(),
            },
            StreamEvent {
                kind: "done".into(),
                content: "length".into(),
            },
        ];
        assert!(openai_needs_thinking_retry(&events));
    }

    #[test]
    fn normalize_dead_ollama_cloud_host() {
        assert_eq!(
            normalize_openai_base_url("https://api.ollama.cloud/v1"),
            "https://ollama.com/v1"
        );
        assert_eq!(
            normalize_openai_base_url("https://ollama.com/v1/"),
            "https://ollama.com/v1"
        );
        assert_eq!(
            normalize_openai_base_url("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/v1"
        );
    }

    #[test]
    fn reasoning_only_stream_errors_clearly() {
        let events = vec![
            StreamEvent {
                kind: "reasoning".into(),
                content: "先分析命令安全性".into(),
            },
            StreamEvent {
                kind: "done".into(),
                content: "length".into(),
            },
        ];
        let err = finalize_openai_events(events, true).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("思考过程"), "{msg}");
        assert!(msg.contains("max_tokens"), "{msg}");
    }

    #[test]
    fn content_after_reasoning_ok() {
        let events = vec![
            StreamEvent {
                kind: "reasoning".into(),
                content: "think".into(),
            },
            StreamEvent {
                kind: "delta".into(),
                content: "1. 问题检查".into(),
            },
            StreamEvent {
                kind: "done".into(),
                content: "stop".into(),
            },
        ];
        assert!(finalize_openai_events(events, true).is_ok());
    }
}