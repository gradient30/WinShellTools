use crate::error::{DstError, DstResult};
use crate::workspace;
use serde::{Deserialize, Serialize};

/// AI 提供商协议
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AiProtocol {
    Openai,
    Anthropic,
}

/// AI 配置（不含 api_key，key 单独存）— 兼容旧 API，等同单个 Profile 的字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    pub protocol: AiProtocol,
    pub base_url: String,
    pub model: String,
    /// 0-2 的 temperature，默认 0.7
    #[serde(default = "default_temp")]
    pub temperature: f64,
    /// 最大输出 token，默认 2048
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

/// 多 Profile 存储项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProfile {
    pub id: String,
    pub name: String,
    pub protocol: AiProtocol,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_temp")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default)]
    pub key_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AiProfilesStore {
    #[serde(default)]
    pub profiles: Vec<AiProfile>,
    pub default_profile_id: Option<String>,
}

impl From<&AiProfile> for AiConfig {
    fn from(p: &AiProfile) -> Self {
        Self {
            protocol: p.protocol,
            base_url: p.base_url.clone(),
            model: p.model.clone(),
            temperature: p.temperature,
            max_tokens: p.max_tokens,
        }
    }
}

impl AiProfile {
    pub fn from_config(id: String, name: String, cfg: &AiConfig) -> Self {
        Self {
            id,
            name,
            protocol: cfg.protocol,
            base_url: cfg.base_url.clone(),
            model: cfg.model.clone(),
            temperature: cfg.temperature,
            max_tokens: cfg.max_tokens,
            key_configured: false,
        }
    }
}

fn default_temp() -> f64 {
    0.7
}
fn default_max_tokens() -> u32 {
    // 审阅类长提示 + 思考模型需要更大输出预算，2048 易被 reasoning 占满导致正文为空
    8192
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            protocol: AiProtocol::Openai,
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            temperature: 0.7,
            max_tokens: 8192,
        }
    }
}

/// 多 Profile 配置文件：.studio/ai_profiles.json
fn profiles_file() -> std::path::PathBuf {
    workspace::studio_dir().join("ai_profiles.json")
}

fn keys_dir() -> std::path::PathBuf {
    workspace::studio_dir().join("ai_keys")
}

/// 配置文件路径：工作区 .studio/ai_config.json（旧版，迁移后保留只读）
fn config_file() -> std::path::PathBuf {
    workspace::studio_dir().join("ai_config.json")
}

/// 凭证文件路径：工作区 .studio/ai_key.txt（旧版）
fn key_file() -> std::path::PathBuf {
    workspace::studio_dir().join("ai_key.txt")
}

fn key_file_for(id: &str) -> std::path::PathBuf {
    keys_dir().join(format!("{id}.txt"))
}

fn migrate_legacy_if_needed() -> DstResult<()> {
    if profiles_file().exists() {
        return Ok(());
    }
    let mut store = AiProfilesStore::default();
    if config_file().exists() {
        if let Ok(raw) = std::fs::read_to_string(config_file()) {
            if let Ok(cfg) = serde_json::from_str::<AiConfig>(&raw) {
                let id = uuid_simple();
                let mut profile = AiProfile::from_config(id.clone(), "默认配置".into(), &cfg);
                if key_file().exists() {
                    if let Ok(key) = std::fs::read_to_string(key_file()) {
                        save_key_for_profile(&id, key.trim())?;
                        profile.key_configured = true;
                    }
                }
                store.profiles.push(profile);
                store.default_profile_id = Some(id);
            }
        }
    }
    if store.profiles.is_empty() {
        let id = uuid_simple();
        let profile = AiProfile::from_config(id.clone(), "默认配置".into(), &AiConfig::default());
        store.profiles.push(profile);
        store.default_profile_id = Some(id);
    }
    save_profiles_store(&store)
}

fn uuid_simple() -> String {
    format!(
        "{:x}{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

pub fn load_profiles_store() -> DstResult<AiProfilesStore> {
    migrate_legacy_if_needed()?;
    let path = profiles_file();
    if !path.exists() {
        return Ok(AiProfilesStore::default());
    }
    let raw = std::fs::read_to_string(&path)?;
    let mut store: AiProfilesStore = serde_json::from_str(&raw)?;
    let mut bumped = false;
    for p in &mut store.profiles {
        p.key_configured = key_file_for(&p.id).exists();
        // 旧默认 2048/8192 在 DeepSeek 思考模式下易导致“只思考无正文”；抬到 16384
        if p.max_tokens > 0 && p.max_tokens < 16384 {
            let is_deepseek = p.base_url.to_lowercase().contains("deepseek")
                || p.model.to_lowercase().contains("deepseek");
            let new_cap = if is_deepseek { 16384 } else if p.max_tokens <= 2048 { 8192 } else { p.max_tokens };
            if new_cap > p.max_tokens {
                p.max_tokens = new_cap;
                bumped = true;
            }
        }
    }
    if bumped {
        let _ = save_profiles_store(&store);
    }
    Ok(store)
}

pub fn save_profiles_store(store: &AiProfilesStore) -> DstResult<()> {
    if let Some(parent) = profiles_file().parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(profiles_file(), serde_json::to_string_pretty(store)?)?;
    Ok(())
}

pub fn list_profiles() -> DstResult<Vec<AiProfile>> {
    Ok(load_profiles_store()?.profiles)
}

pub fn get_profile(id: &str) -> DstResult<AiProfile> {
    let store = load_profiles_store()?;
    store
        .profiles
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or_else(|| DstError::Other(format!("Profile 不存在：{id}")))
}

pub fn save_profile(profile: AiProfile, key: Option<&str>) -> DstResult<AiProfile> {
    let mut store = load_profiles_store()?;
    if let Some(k) = key {
        if !k.trim().is_empty() {
            save_key_for_profile(&profile.id, k.trim())?;
        }
    }
    let mut p = profile;
    p.key_configured = key_file_for(&p.id).exists();
    if let Some(existing) = store.profiles.iter_mut().find(|x| x.id == p.id) {
        *existing = p.clone();
    } else {
        store.profiles.push(p.clone());
        if store.default_profile_id.is_none() {
            store.default_profile_id = Some(p.id.clone());
        }
    }
    save_profiles_store(&store)?;
    Ok(p)
}

pub fn delete_profile(id: &str) -> DstResult<()> {
    let mut store = load_profiles_store()?;
    store.profiles.retain(|p| p.id != id);
    let _ = std::fs::remove_file(key_file_for(id));
    if store.default_profile_id.as_deref() == Some(id) {
        store.default_profile_id = store.profiles.first().map(|p| p.id.clone());
    }
    save_profiles_store(&store)
}

pub fn load_config_for_profile(id: Option<&str>) -> DstResult<AiConfig> {
    let store = load_profiles_store()?;
    let pid = id
        .map(|s| s.to_string())
        .or_else(|| store.default_profile_id.clone())
        .ok_or_else(|| DstError::Other("未配置 AI Profile".into()))?;
    let profile = get_profile(&pid)?;
    Ok(AiConfig::from(&profile))
}

pub fn load_key_for_profile(id: &str) -> DstResult<String> {
    let path = key_file_for(id);
    if !path.exists() {
        return Err(DstError::Other("未配置 API Key".into()));
    }
    let key = std::fs::read_to_string(&path)?.trim().to_string();
    if key.is_empty() {
        return Err(DstError::Other("API Key 为空".into()));
    }
    Ok(key)
}

pub fn save_key_for_profile(id: &str, key: &str) -> DstResult<()> {
    std::fs::create_dir_all(keys_dir())?;
    std::fs::write(key_file_for(id), key.trim())?;
    Ok(())
}

/// 读取 AI 配置（默认 Profile）。
pub fn load_config() -> DstResult<AiConfig> {
    load_config_for_profile(None)
}

/// 保存 AI 配置到默认 Profile。
pub fn save_config(cfg: &AiConfig) -> DstResult<()> {
    let mut store = load_profiles_store()?;
    let id = store
        .default_profile_id
        .clone()
        .unwrap_or_else(uuid_simple);
    let name = store
        .profiles
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "默认配置".into());
    let mut profile = AiProfile::from_config(id.clone(), name, cfg);
    profile.key_configured = key_file_for(&id).exists();
    if let Some(existing) = store.profiles.iter_mut().find(|p| p.id == id) {
        profile.key_configured = existing.key_configured || profile.key_configured;
        *existing = profile;
    } else {
        store.profiles.push(profile);
    }
    store.default_profile_id = Some(id);
    save_profiles_store(&store)
}

/// 读取 API key（默认 Profile）。
pub fn load_key() -> DstResult<String> {
    let store = load_profiles_store()?;
    let id = store
        .default_profile_id
        .ok_or_else(|| DstError::Other("未配置 AI Profile".into()))?;
    load_key_for_profile(&id)
}

/// 保存 API key（默认 Profile）。
pub fn save_key(key: &str) -> DstResult<()> {
    let store = load_profiles_store()?;
    let id = store
        .default_profile_id
        .ok_or_else(|| DstError::Other("未配置 AI Profile".into()))?;
    save_key_for_profile(&id, key)
}

/// 是否已配置（任一 Profile 有 key）。
pub fn is_configured() -> bool {
    load_profiles_store()
        .map(|s| {
            s.profiles
                .iter()
                .any(|p| key_file_for(&p.id).exists())
        })
        .unwrap_or(false)
}

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// 固化的 System Prompt。`danger_mode` 由 AI 会话 `/danger` 激活。
pub fn system_prompt() -> String {
    system_prompt_with_options(false)
}

pub fn system_prompt_with_options(danger_mode: bool) -> String {
    if danger_mode {
        return r#"你是 DevShellTools Studio 的 PowerShell 命令生成助手。【危险模式已激活】用户已通过 /danger 授权本会话最高权限。

本会话允许生成默认红线内的破坏性操作（须在说明中醒目标注风险）：
- git push --force / --force-with-lease
- git reset --hard
- git clean -fd / -f（真实删除）
- 其它高风险 git/文件操作（仍应尽量加确认参数或明确警告）

仍须遵守：
1. 命名即命令：公共命令是小写函数名（仅小写字母/数字）；大写开头（Assert-Xxx 等）是内部辅助函数，不得当作用户命令介绍；辅助函数与所服务的公共命令放在同一代码块
2. 每个函数必须包含 .SYNOPSIS 和至少一个 .EXAMPLE
3. 在回复开头用一行简短警告：已处于危险模式，操作可能不可逆

输出格式：
- PowerShell 代码放在 ```powershell 代码块中
- 代码块外可加简短说明
"#
        .to_string();
    }

    r#"你是 DevShellTools Studio 的 PowerShell 命令生成助手。你的任务是根据用户需求生成 PowerShell 5.1 兼容的快捷命令函数。

【命名即命令】用户在终端输入的命令 = 小写开头的函数名：
1. 每个公共命令必须是 function <小写名> { ... }，名称仅用小写字母和数字（如 dps、gcmsg）
2. 大写开头（Assert-Xxx、Write-Xxx）是内部辅助函数，用户不能直接调用；说明文字中禁止把它当命令介绍
3. 若公共命令需要辅助函数，两者放在同一代码块（会一起写入分类文件）
4. 依赖外部工具时在命令开头用 Assert-DstCommand "xxx" 校验（如 git、docker、curl.exe）
5. 引用已有命令时只使用【当前工作区命令清单】中列出的真实名称，不得杜撰

严格安全规则（违反则拒绝生成）：
1. 禁止生成 git push --force / --force-with-lease
2. 禁止生成 git reset --hard
3. 禁止生成 git clean -fd / -f（真实删除），只允许 -nd / -ndx 预览
4. Stop-Process 必须配合 -Confirm 或在 SupportsShouldProcess 函数内
5. 禁止 [Environment]::SetEnvironmentVariable(..., "User")，只允许 "Process"
6. 禁止 Remove-Item -Recurse -Force（危险删除）
7. 每个函数（含辅助函数）必须包含 .SYNOPSIS 和至少一个 .EXAMPLE 注释型帮助
8. 若用户需要上述破坏性能力，提示其在本会话输入 /danger 激活危险模式（输入 /safe 可关闭）

输出格式：
- 把生成的 PowerShell 代码放在 ```powershell 代码块中
- 代码块外可加简短说明
- 代码必须包含 @DST-Category 元数据块（如果是新分类）；追加到现有分类时不要重复该块
- 直接给出最终代码，不要输出草稿或多个备选版本
- 每个函数用标准 PowerShell 注释帮助块

示例输出：
```powershell
<#!
@DST-Category
Name: docker
Title: Docker
Description: 容器管理快捷命令
Aliases: 容器
@DST-Category-End
#>

function dps {
<#
.SYNOPSIS
列出运行中容器。
.EXAMPLE
dps
#>
    [CmdletBinding()] param()
    Assert-DstCommand "docker"
    & docker ps
}
```
"#
    .to_string()
}

/// 从 AI 响应文本中提取 ```powershell 代码块
pub fn extract_code_blocks(text: &str) -> Vec<String> {
    let mut blocks = vec![];
    let mut in_block = false;
    let mut current = String::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```powershell") || line.trim_start().starts_with("```ps1") {
            in_block = true;
            current.clear();
            continue;
        }
        if in_block && line.trim() == "```" {
            in_block = false;
            if !current.is_empty() {
                blocks.push(current.clone());
            }
            continue;
        }
        if in_block {
            current.push_str(line);
            current.push('\n');
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_single_block() {
        let text = "这是结果：\n```powershell\nfunction foo { }\n```\n完成";
        let blocks = extract_code_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].contains("function foo"));
    }

    #[test]
    fn extract_multiple_blocks() {
        let text = "```powershell\nfunction a {}\n```\n中间\n```powershell\nfunction b {}\n```";
        let blocks = extract_code_blocks(text);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn no_block() {
        let text = "没有代码块的回答";
        let blocks = extract_code_blocks(text);
        assert!(blocks.is_empty());
    }

    #[test]
    fn default_config() {
        let cfg = AiConfig::default();
        assert_eq!(cfg.protocol, AiProtocol::Openai);
        assert!(!cfg.base_url.is_empty());
        assert!(!cfg.model.is_empty());
    }

    #[test]
    fn system_prompt_has_safety_rules() {
        let p = system_prompt();
        assert!(p.contains("禁止"));
        assert!(p.contains("--force"));
        assert!(p.contains("--hard"));
        assert!(p.contains(".SYNOPSIS"));
        assert!(p.contains("/danger"));
    }

    #[test]
    fn danger_system_prompt_allows_hard_reset() {
        let p = system_prompt_with_options(true);
        assert!(p.contains("危险模式"));
        assert!(p.contains("reset --hard"));
        assert!(!p.contains("违反则拒绝生成"));
    }
}