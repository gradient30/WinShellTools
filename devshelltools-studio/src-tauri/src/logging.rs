use crate::error::DstResult;
use crate::workspace;
use crate::workspace_path::validate_component;
use std::path::PathBuf;

/// 日志级别
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// 日志目录：.studio/logs/
fn log_dir() -> PathBuf {
    workspace::studio_dir().join("logs")
}

/// 当天日志文件：.studio/logs/YYYY-MM-DD.log
fn log_file() -> PathBuf {
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    log_dir().join(format!("{date}.log"))
}

/// 写一条日志。API key 等敏感信息会被脱敏。
pub fn log(level: LogLevel, category: &str, message: &str) {
    let dir = log_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let ts = chrono::Utc::now().to_rfc3339();
    let level_str = match level {
        LogLevel::Info => "INFO",
        LogLevel::Warn => "WARN",
        LogLevel::Error => "ERROR",
    };
    let sanitized = sanitize(message);
    let line = format!("[{ts}] [{level_str}] [{category}] {sanitized}\n");
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file())
        .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
}

/// 脱敏：把 sk-xxxx、Bearer xxxx 等 API key 模式替换为 ****
pub fn sanitize(msg: &str) -> String {
    let mut s = msg.to_string();
    // sk- 开头的 OpenAI key
    let _re_patterns: &[&str] = &[
        r"(sk-[a-zA-Z0-9]{6})[a-zA-Z0-9]+",
        r"(Bearer\s+)[a-zA-Z0-9\-_]{8,}",
        r#"(api_key["\s:=]+)["a-zA-Z0-9\-_]{8,}"#,
    ];
    // M4 避免引入 regex crate，用字符串匹配近似脱敏
    // 近似脱敏：把长 hex/base64 串替换
    if s.contains("sk-") {
        s = replace_long_after_prefix(&s, "sk-", 6);
    }
    if s.contains("Bearer ") {
        s = replace_long_after_prefix(&s, "Bearer ", 0);
    }
    s
}

fn replace_long_after_prefix(s: &str, prefix: &str, keep: usize) -> String {
    if let Some(idx) = s.find(prefix) {
        let after = &s[idx + prefix.len()..];
        let after_trimmed = after.trim_start();
        let end = after_trimmed
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
            .unwrap_or(after_trimmed.len());
        if end > keep + 4 {
            let (keep_part, _) = after_trimmed.split_at(keep + 4);
            let replaced = format!("{prefix}{keep_part}...****");
            return format!("{}{}{}", &s[..idx], replaced, &after_trimmed[end..]);
        }
    }
    s.to_string()
}

/// 读取当天日志（给前端展示）。
pub fn read_today_log() -> String {
    let path = log_file();
    if !path.exists() {
        return String::new();
    }
    std::fs::read_to_string(path).unwrap_or_default()
}

/// 列出所有日志文件名。
pub fn list_log_files() -> Vec<String> {
    let dir = log_dir();
    if !dir.exists() {
        return vec![];
    }
    let mut files = vec![];
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            if let Some(name) = e.file_name().to_str() {
                files.push(name.to_string());
            }
        }
    }
    files.sort();
    files.reverse(); // 最新在前
    files
}

/// 读取指定日志文件。
pub fn read_log_file(name: &str) -> DstResult<String> {
    validate_component(name, "日志文件")?;
    let path = log_dir().join(name);
    if !path.exists() {
        return Ok(String::new());
    }
    Ok(std::fs::read_to_string(path).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_sk_key() {
        let s = "api_key=sk-abc1234567890xyz";
        let sanitized = sanitize(s);
        assert!(sanitized.contains("****"));
        assert!(!sanitized.contains("7890xyz"));
    }

    #[test]
    fn sanitize_bearer() {
        let s = "Authorization: Bearer abcdef12345678";
        let sanitized = sanitize(s);
        assert!(sanitized.contains("****"));
        assert!(!sanitized.contains("abcdef12345678"));
    }

    #[test]
    fn sanitize_plain_text_unchanged() {
        let s = "普通日志消息";
        let sanitized = sanitize(s);
        assert_eq!(sanitized, s);
    }

    #[test]
    fn read_log_rejects_path_traversal() {
        assert!(read_log_file("../outside.log").is_err());
    }
}
