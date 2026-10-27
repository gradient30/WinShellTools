//! PowerShell 7（pwsh）运行时检测。
//! Studio 的解析、语法校验、命令自测优先使用 pwsh 7.x（`ps_parser::which_powershell`），
//! 未安装时回退 Windows PowerShell 5.1；本模块供设置页展示状态并引导下载。

use crate::process_util::{output_hidden, ps_base_args};
use std::sync::OnceLock;

/// PowerShell 7 运行时状态
#[derive(Debug, Clone, serde::Serialize)]
pub struct PwshStatus {
    /// 是否找到 pwsh（PowerShell 7.x）
    pub pwsh_found: bool,
    /// pwsh 版本号（如 7.4.6）；未找到为空
    pub version: String,
}

/// 官方下载入口（aka.ms 短链，直达 PowerShell Releases）
pub const PWSH_DOWNLOAD_URL: &str = "https://aka.ms/powershell";

static CACHE: OnceLock<PwshStatus> = OnceLock::new();

/// 检测 pwsh 7.x。结果进程内缓存——安装/卸载后需重启 Studio 重新检测。
pub fn check_pwsh() -> PwshStatus {
    CACHE
        .get_or_init(|| {
            let mut cmd = std::process::Command::new("pwsh");
            for arg in ps_base_args("pwsh") {
                cmd.arg(arg);
            }
            cmd.args(["-Command", "$PSVersionTable.PSVersion.ToString()"]);
            match output_hidden(cmd) {
                Ok(o) if o.status.success() => {
                    let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if v.is_empty() {
                        PwshStatus { pwsh_found: false, version: String::new() }
                    } else {
                        PwshStatus { pwsh_found: true, version: v }
                    }
                }
                _ => PwshStatus { pwsh_found: false, version: String::new() },
            }
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_url_is_official_shortlink() {
        assert!(PWSH_DOWNLOAD_URL.starts_with("https://aka.ms/"));
    }

    #[test]
    fn check_pwsh_no_panic() {
        let s = check_pwsh();
        assert!(s.pwsh_found || s.version.is_empty());
    }
}
