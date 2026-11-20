use crate::ai_config::{AiProtocol, ChatMessage};
use crate::error::{DstError, DstResult};
use std::net::IpAddr;

pub const MAX_OUTPUT_TOKENS: u32 = 32_768;

#[derive(Debug, Clone, Copy)]
pub struct ContextBudget {
    pub max_chars: usize,
    pub max_messages: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_chars: 60_000,
            max_messages: 40,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrimResult {
    pub messages: Vec<ChatMessage>,
    pub omitted_count: usize,
    pub approximated_chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointPolicy {
    Https,
    PrivateHttp,
}

pub fn trim_messages(messages: Vec<ChatMessage>, budget: ContextBudget) -> TrimResult {
    let (system, history): (Vec<_>, Vec<_>) = messages
        .into_iter()
        .partition(|message| message.role == "system");
    let mut selected = Vec::new();
    let reserved = 128usize;
    let mut used_chars: usize = system
        .iter()
        .map(|message| message.content.chars().count())
        .sum();

    for message in history.iter().rev() {
        if selected.len() >= budget.max_messages.saturating_sub(1) {
            break;
        }
        let message_chars = message.content.chars().count();
        if !selected.is_empty()
            && used_chars
                .saturating_add(message_chars)
                .saturating_add(reserved)
                > budget.max_chars
        {
            break;
        }
        used_chars = used_chars.saturating_add(message_chars);
        selected.push(message.clone());
    }
    selected.reverse();
    let omitted_count = history.len().saturating_sub(selected.len());

    let mut output = system;
    if omitted_count > 0 {
        let marker =
            format!("【上下文预算】已省略较早的 {omitted_count} 条消息；以下保留最近对话。");
        used_chars = used_chars.saturating_add(marker.chars().count());
        output.push(ChatMessage {
            role: "system".into(),
            content: marker,
        });
    }
    output.extend(selected);
    TrimResult {
        messages: output,
        omitted_count,
        approximated_chars: used_chars,
    }
}

pub fn validate_endpoint(base_url: &str, _protocol: AiProtocol) -> DstResult<EndpointPolicy> {
    let url = reqwest::Url::parse(base_url.trim())
        .map_err(|error| DstError::AiClient(format!("AI 端点 URL 无效：{error}")))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(DstError::AiClient(
            "AI 端点不得在 URL 中嵌入用户名或密码".into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| DstError::AiClient("AI 端点缺少主机名".into()))?;
    match url.scheme() {
        "https" => Ok(EndpointPolicy::Https),
        "http" if is_private_host(host) => Ok(EndpointPolicy::PrivateHttp),
        "http" => Err(DstError::AiClient(
            "公网 AI 端点必须使用 HTTPS；HTTP 仅允许本机或私有网络".into(),
        )),
        scheme => Err(DstError::AiClient(format!(
            "不支持的 AI 端点协议：{scheme}"
        ))),
    }
}

pub fn cap_output_tokens(tokens: u32) -> u32 {
    tokens.clamp(256, MAX_OUTPUT_TOKENS)
}

fn is_private_host(host: &str) -> bool {
    let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".local") || host.ends_with(".lan") {
        return true;
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) => {
            address.is_loopback()
                || address.is_private()
                || address.is_link_local()
                || address.is_unspecified()
        }
        Ok(IpAddr::V6(address)) => {
            address.is_loopback()
                || address.is_unspecified()
                || (address.segments()[0] & 0xfe00) == 0xfc00
                || (address.segments()[0] & 0xffc0) == 0xfe80
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_config::{AiProtocol, ChatMessage};

    #[test]
    fn newest_messages_are_preserved_when_context_is_trimmed() {
        let mut history = vec![ChatMessage {
            role: "system".into(),
            content: "system".into(),
        }];
        for index in 0..30 {
            history.push(ChatMessage {
                role: if index % 2 == 0 { "user" } else { "assistant" }.into(),
                content: format!("older-{index}-{}", "x".repeat(200)),
            });
        }
        history.push(ChatMessage {
            role: "user".into(),
            content: "newest".into(),
        });

        let result = trim_messages(
            history,
            ContextBudget {
                max_chars: 1200,
                max_messages: 8,
            },
        );
        assert!(result.omitted_count > 0);
        assert_eq!(result.messages.first().expect("system").role, "system");
        assert_eq!(result.messages.last().expect("newest").content, "newest");
    }

    #[test]
    fn public_plain_http_endpoint_is_rejected() {
        assert!(validate_endpoint("http://example.com/v1", AiProtocol::Openai).is_err());
        assert!(validate_endpoint("http://127.0.0.1:11434/v1", AiProtocol::Openai).is_ok());
        assert!(validate_endpoint("http://192.168.1.5:8000/v1", AiProtocol::Openai).is_ok());
        assert!(validate_endpoint("https://example.com/v1", AiProtocol::Openai).is_ok());
    }

    #[test]
    fn automatic_output_budget_never_exceeds_policy_cap() {
        assert!(cap_output_tokens(65_536) <= 32_768);
    }
}
