use crate::ai_config::AiProtocol;
use serde::Serialize;

/// AI 提供商预设（端点来自各官方文档）。
/// 每个提供商一条，含 OpenAI 和 Anthropic 两种协议的端点（不支持则为空）。
#[derive(Debug, Clone, Serialize)]
pub struct AiPreset {
    pub id: String,
    pub name: String,
    /// OpenAI 兼容端点
    pub openai_base_url: String,
    /// Anthropic 兼容端点（不支持则为空）
    pub anthropic_base_url: String,
    /// OpenAI 协议默认模型
    pub openai_default_model: String,
    /// Anthropic 协议默认模型
    pub anthropic_default_model: String,
    /// 是否支持 Anthropic 协议
    pub supports_anthropic: bool,
}

/// 前端展示用的预设（一个提供商一条，不含协议字段）
#[derive(Debug, Clone, Serialize)]
pub struct AiPresetView {
    pub id: String,
    pub name: String,
    pub openai_base_url: String,
    pub anthropic_base_url: String,
    pub openai_default_model: String,
    pub anthropic_default_model: String,
    pub supports_anthropic: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiEndpointSuggestion {
    pub base_url: String,
    pub default_model: String,
    pub protocol: AiProtocol,
    pub note: String,
}

/// 全部预设（端点来自官方文档）
pub fn list_presets() -> Vec<AiPreset> {
    vec![
        AiPreset {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            openai_base_url: "https://api.deepseek.com/v1".into(),
            anthropic_base_url: "https://api.deepseek.com/anthropic".into(),
            openai_default_model: "deepseek-chat".into(),
            anthropic_default_model: "deepseek-chat".into(),
            supports_anthropic: true,
        },
        AiPreset {
            id: "glm".into(),
            name: "智谱 GLM".into(),
            openai_base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
            anthropic_base_url: String::new(),
            openai_default_model: "glm-4-flash".into(),
            anthropic_default_model: String::new(),
            supports_anthropic: false,
        },
        AiPreset {
            id: "kimi".into(),
            name: "Kimi（月之暗面）".into(),
            openai_base_url: "https://api.moonshot.cn/v1".into(),
            anthropic_base_url: String::new(),
            // K2/K3 系列固定采样参数，请求时不传 temperature
            openai_default_model: "kimi-k3".into(),
            anthropic_default_model: String::new(),
            supports_anthropic: false,
        },
        AiPreset {
            id: "qwen".into(),
            name: "通义千问（阿里百炼）".into(),
            openai_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
            anthropic_base_url: String::new(),
            openai_default_model: "qwen-plus".into(),
            anthropic_default_model: String::new(),
            supports_anthropic: false,
        },
        AiPreset {
            id: "qianfan".into(),
            name: "百度千帆".into(),
            // 官方 OpenAI 兼容：https://cloud.baidu.com/doc/qianfan/s/Hmh4suq26
            openai_base_url: "https://qianfan.baidubce.com/v2".into(),
            anthropic_base_url: String::new(),
            openai_default_model: "ernie-4.0-turbo-8k".into(),
            anthropic_default_model: String::new(),
            supports_anthropic: false,
        },
        AiPreset {
            id: "minimax".into(),
            name: "MiniMax".into(),
            openai_base_url: "https://api.minimaxi.com/v1".into(),
            anthropic_base_url: String::new(),
            openai_default_model: "MiniMax-Text-01".into(),
            anthropic_default_model: String::new(),
            supports_anthropic: false,
        },
        AiPreset {
            id: "opencode".into(),
            name: "OpenCode".into(),
            openai_base_url: "https://api.opencode.ai/v1".into(),
            anthropic_base_url: String::new(),
            openai_default_model: "gpt-4o-mini".into(),
            anthropic_default_model: String::new(),
            supports_anthropic: false,
        },
        AiPreset {
            id: "ollama-cloud".into(),
            name: "Ollama Cloud".into(),
            // 官方 Cloud OpenAI 兼容端点（旧域名 api.ollama.cloud 已 503）
            openai_base_url: "https://ollama.com/v1".into(),
            anthropic_base_url: String::new(),
            openai_default_model: "gpt-oss:120b".into(),
            anthropic_default_model: String::new(),
            supports_anthropic: false,
        },
        AiPreset {
            id: "hunyuan".into(),
            name: "腾讯混元".into(),
            openai_base_url: "https://api.hunyuan.cloud.tencent.com/v1".into(),
            anthropic_base_url: String::new(),
            openai_default_model: "hunyuan-turbos-latest".into(),
            anthropic_default_model: String::new(),
            supports_anthropic: false,
        },
        AiPreset {
            id: "mimo".into(),
            name: "小米 MiMo".into(),
            openai_base_url: "https://api.mimo.xiaomi.com/v1".into(),
            anthropic_base_url: String::new(),
            openai_default_model: "mimo-7b".into(),
            anthropic_default_model: String::new(),
            supports_anthropic: false,
        },
        AiPreset {
            id: "volcengine".into(),
            name: "字节火山引擎".into(),
            openai_base_url: "https://ark.cn-beijing.volces.com/api/v3".into(),
            anthropic_base_url: String::new(),
            openai_default_model: "doubao-pro-32k".into(),
            anthropic_default_model: String::new(),
            supports_anthropic: false,
        },
        AiPreset {
            id: "stepfun".into(),
            name: "星辰阶跃（StepFun）".into(),
            openai_base_url: "https://api.stepfun.com/v1".into(),
            anthropic_base_url: String::new(),
            openai_default_model: "step-1-flash".into(),
            anthropic_default_model: String::new(),
            supports_anthropic: false,
        },
    ]
}

/// 前端获取预设列表（一个提供商一条）
pub fn list_preset_views() -> Vec<AiPresetView> {
    list_presets()
        .into_iter()
        .map(|p| AiPresetView {
            id: p.id,
            name: p.name,
            openai_base_url: p.openai_base_url,
            anthropic_base_url: p.anthropic_base_url,
            openai_default_model: p.openai_default_model,
            anthropic_default_model: p.anthropic_default_model,
            supports_anthropic: p.supports_anthropic,
        })
        .collect()
}

/// 根据 base_url 检测所属提供商
fn detect_preset(base_url: &str) -> Option<AiPreset> {
    let u = base_url.to_lowercase();
    // 兼容已失效的 api.ollama.cloud 与本地 Ollama
    if u.contains("ollama.com") || u.contains("ollama.cloud") || u.contains(":11434") {
        return list_presets().into_iter().find(|p| p.id == "ollama-cloud");
    }
    list_presets().into_iter().find(|p| {
        let oai = p.openai_base_url.to_lowercase();
        let ant = p.anthropic_base_url.to_lowercase();
        (!oai.is_empty() && u.contains(&domain_of(&oai)))
            || (!ant.is_empty() && u.contains(&domain_of(&ant)))
    })
}

fn domain_of(url: &str) -> String {
    let no_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let no_trailing = no_scheme.trim_end_matches('/');
    no_trailing
        .split('/')
        .next()
        .unwrap_or(no_trailing)
        .to_string()
}

/// 切换协议时，根据当前 base_url 找到对应提供商，返回该协议下的端点。
pub fn suggest_endpoint(
    protocol: AiProtocol,
    current_base_url: Option<&str>,
) -> AiEndpointSuggestion {
    if let Some(url) = current_base_url.filter(|s| !s.trim().is_empty()) {
        if let Some(preset) = detect_preset(url) {
            return match protocol {
                AiProtocol::Openai => {
                    if !preset.openai_base_url.is_empty() {
                        AiEndpointSuggestion {
                            base_url: preset.openai_base_url.clone(),
                            default_model: preset.openai_default_model.clone(),
                            protocol,
                            note: format!("已切换为「{}」OpenAI 兼容端点", preset.name),
                        }
                    } else {
                        AiEndpointSuggestion {
                            base_url: "https://api.openai.com/v1".into(),
                            default_model: "gpt-4o-mini".into(),
                            protocol,
                            note: format!("「{}」不支持 OpenAI 兼容协议", preset.name),
                        }
                    }
                }
                AiProtocol::Anthropic => {
                    if !preset.anthropic_base_url.is_empty() {
                        AiEndpointSuggestion {
                            base_url: preset.anthropic_base_url.clone(),
                            default_model: preset.anthropic_default_model.clone(),
                            protocol,
                            note: format!("已切换为「{}」Anthropic 兼容端点", preset.name),
                        }
                    } else {
                        AiEndpointSuggestion {
                            base_url: "https://api.anthropic.com/v1".into(),
                            default_model: "claude-3-5-haiku-20241022".into(),
                            protocol,
                            note: format!("「{}」不支持 Anthropic 兼容协议", preset.name),
                        }
                    }
                }
            };
        }
    }

    match protocol {
        AiProtocol::Openai => AiEndpointSuggestion {
            base_url: "https://api.openai.com/v1".into(),
            default_model: "gpt-4o-mini".into(),
            protocol,
            note: "已填充 OpenAI 官方端点".into(),
        },
        AiProtocol::Anthropic => AiEndpointSuggestion {
            base_url: "https://api.anthropic.com/v1".into(),
            default_model: "claude-3-5-haiku-20241022".into(),
            protocol,
            note: "已填充 Anthropic 官方端点".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_protocol_switch() {
        let s = suggest_endpoint(AiProtocol::Anthropic, Some("https://api.deepseek.com/v1"));
        assert_eq!(s.base_url, "https://api.deepseek.com/anthropic");
    }

    #[test]
    fn deepseek_anthropic_to_openai() {
        let s = suggest_endpoint(
            AiProtocol::Openai,
            Some("https://api.deepseek.com/anthropic"),
        );
        assert_eq!(s.base_url, "https://api.deepseek.com/v1");
    }

    #[test]
    fn glm_no_anthropic_support() {
        let s = suggest_endpoint(
            AiProtocol::Anthropic,
            Some("https://open.bigmodel.cn/api/paas/v4"),
        );
        assert_eq!(s.base_url, "https://api.anthropic.com/v1");
        assert!(s.note.contains("不支持"));
    }

    #[test]
    fn all_presets_have_openai_url() {
        let presets = list_presets();
        assert_eq!(presets.len(), 12);
        for p in &presets {
            assert!(!p.openai_base_url.is_empty(), "{} 缺 OpenAI 端点", p.name);
        }
    }

    #[test]
    fn preset_views_one_per_provider() {
        let views = list_preset_views();
        assert_eq!(views.len(), 12);
        // DeepSeek 应该只有一条，且 supports_anthropic = true
        let ds = views.iter().find(|v| v.id == "deepseek").unwrap();
        assert!(ds.supports_anthropic);
        assert!(!ds.anthropic_base_url.is_empty());
    }

    #[test]
    fn qianfan_preset_endpoint() {
        let p = list_presets()
            .into_iter()
            .find(|x| x.id == "qianfan")
            .expect("缺少千帆预设");
        assert_eq!(p.openai_base_url, "https://qianfan.baidubce.com/v2");
        assert!(!p.openai_default_model.is_empty());
        assert!(!p.supports_anthropic);
    }

    #[test]
    fn ollama_cloud_uses_official_host() {
        let p = list_presets()
            .into_iter()
            .find(|x| x.id == "ollama-cloud")
            .expect("缺少 Ollama Cloud 预设");
        assert_eq!(p.openai_base_url, "https://ollama.com/v1");
        assert!(!p.openai_base_url.contains("api.ollama.cloud"));
        // 旧失效域名仍应识别为该预设
        let s = suggest_endpoint(AiProtocol::Openai, Some("https://api.ollama.cloud/v1"));
        assert_eq!(s.base_url, "https://ollama.com/v1");
    }
}
