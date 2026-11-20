use crate::error::DstResult;
use crate::process_runner::{run as run_process, ProcessSpec};
use std::path::PathBuf;
use std::time::Duration;

/// WebView2 Runtime 检测结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct Webview2Status {
    pub installed: bool,
    pub version: String,
    pub needs_guidance: bool,
}

/// 检测 WebView2 Runtime 是否已安装。
/// Win11 自带，Win10 可能需要安装 Evergreen Runtime。
pub fn check_webview2() -> DstResult<Webview2Status> {
    // 方法1：检查注册表 HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\
    // {F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}
    // 方法2：检查 CoreWebView2Environment 的默认安装路径
    // 方法3：检查常见安装路径

    // 检查注册表（通过 reg query）
    let version = check_registry_version()
        .or_else(|| check_installed_path())
        .unwrap_or_default();

    Ok(Webview2Status {
        installed: !version.is_empty(),
        version,
        needs_guidance: false, // 前端根据 installed 决定是否显示引导
    })
}

/// 通过 reg query 检查注册表中的 WebView2 版本
fn check_registry_version() -> Option<String> {
    let output = run_process(ProcessSpec::new(
        "reg",
        [
            "query",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
            "/v",
            "pv",
        ],
        Duration::from_secs(5),
        64 * 1024,
    ))
    .ok()?;

    if !output.status.success() {
        // 尝试 32 位注册表
        let output = run_process(ProcessSpec::new(
            "reg",
            [
                "query",
                r"HKLM\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
                "/v",
                "pv",
            ],
            Duration::from_secs(5),
            64 * 1024,
        ))
        .ok()?;
        if !output.status.success() {
            return None;
        }
        return parse_reg_pv(&String::from_utf8_lossy(&output.stdout));
    }
    parse_reg_pv(&String::from_utf8_lossy(&output.stdout))
}

fn parse_reg_pv(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.contains("pv") && trimmed.contains("REG_SZ") {
            // 格式: "    pv    REG_SZ    120.0.2651.174"
            if let Some(idx) = trimmed.rfind("REG_SZ") {
                let ver = trimmed[idx + 6..].trim();
                if !ver.is_empty() {
                    return Some(ver.to_string());
                }
            }
        }
    }
    None
}

/// 检查常见安装路径
fn check_installed_path() -> Option<String> {
    let paths = [
        r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application",
        r"C:\Program Files\Microsoft\EdgeWebView\Application",
    ];
    for base in &paths {
        let base = PathBuf::from(base);
        if !base.exists() {
            continue;
        }
        // 版本子目录
        if let Ok(entries) = std::fs::read_dir(&base) {
            for e in entries.flatten() {
                if e.path().is_dir() {
                    let name = e.file_name().to_string_lossy().to_string();
                    // 版本号格式如 120.0.2651.174
                    if name.contains('.') && name.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                        return Some(name);
                    }
                }
            }
        }
    }
    None
}

/// Evergreen 下载 URL（前端打开浏览器）
pub const WEBVIEW2_DOWNLOAD_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reg_pv_extracts_version() {
        let stdout = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}\r\n    pv    REG_SZ    120.0.2651.174\r\n";
        let v = parse_reg_pv(stdout);
        assert_eq!(v.as_deref(), Some("120.0.2651.174"));
    }

    #[test]
    fn parse_reg_pv_empty_returns_none() {
        let v = parse_reg_pv("no relevant line");
        assert!(v.is_none());
    }

    #[test]
    fn download_url_is_https() {
        assert!(WEBVIEW2_DOWNLOAD_URL.starts_with("https://"));
    }
}
