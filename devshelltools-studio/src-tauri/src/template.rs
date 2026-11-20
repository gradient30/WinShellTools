use std::path::Path;

// 内嵌模板：每个文件单独 include_str!/include_bytes!，避免 include_dir 依赖。
// 路径相对于 src-tauri，即 $CARGO_MANIFEST_DIR/../templates/...

pub const PSD1: &str = include_str!("../../templates/DevShellTools.psd1");
pub const PSM1: &str = include_str!("../../templates/DevShellTools.psm1");
pub const INSTALL_PS1: &str = include_str!("../../templates/install.ps1");
pub const UNINSTALL_PS1: &str = include_str!("../../templates/uninstall.ps1");
pub const COMMON_PS1: &str = include_str!("../../templates/Private/Common.ps1");

pub const FILES_PS1: &str = include_str!("../../templates/Public/Files.ps1");
pub const POWERSHELL_PS1: &str = include_str!("../../templates/Public/PowerShell.ps1");
pub const PROXY_PS1: &str = include_str!("../../templates/Public/Proxy.ps1");
pub const GIT_PS1: &str = include_str!("../../templates/Public/Git.ps1");
pub const NETWORK_PS1: &str = include_str!("../../templates/Public/Network.ps1");
pub const HELP_PS1: &str = include_str!("../../templates/Public/Help.ps1");

pub const TEMPLATE_VERSION: &str = "1.0.5";

/// 把内嵌模板整体写入工作区根目录。覆盖已存在文件。
pub fn write_template_to(workspace_root: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(workspace_root)?;
    std::fs::create_dir_all(workspace_root.join("Private"))?;
    std::fs::create_dir_all(workspace_root.join("Public"))?;

    let pairs: &[(&str, &str)] = &[
        ("DevShellTools.psd1", PSD1),
        ("DevShellTools.psm1", PSM1),
        ("install.ps1", INSTALL_PS1),
        ("uninstall.ps1", UNINSTALL_PS1),
        ("Private/Common.ps1", COMMON_PS1),
        ("Public/Files.ps1", FILES_PS1),
        ("Public/PowerShell.ps1", POWERSHELL_PS1),
        ("Public/Proxy.ps1", PROXY_PS1),
        ("Public/Git.ps1", GIT_PS1),
        ("Public/Network.ps1", NETWORK_PS1),
        ("Public/Help.ps1", HELP_PS1),
    ];

    for (rel, content) in pairs {
        let path = workspace_root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
    }
    Ok(())
}

/// 返回模板里某个文件的文本内容（用于校验或预览，不写盘）。
pub fn read_template_file(rel: &str) -> Option<&'static str> {
    let pairs: &[(&str, &str)] = &[
        ("DevShellTools.psd1", PSD1),
        ("DevShellTools.psm1", PSM1),
        ("install.ps1", INSTALL_PS1),
        ("uninstall.ps1", UNINSTALL_PS1),
        ("Private/Common.ps1", COMMON_PS1),
        ("Public/Files.ps1", FILES_PS1),
        ("Public/PowerShell.ps1", POWERSHELL_PS1),
        ("Public/Proxy.ps1", PROXY_PS1),
        ("Public/Git.ps1", GIT_PS1),
        ("Public/Network.ps1", NETWORK_PS1),
        ("Public/Help.ps1", HELP_PS1),
    ];
    pairs.iter().find(|(r, _)| *r == rel).map(|(_, c)| *c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_has_core_files() {
        assert!(!PSD1.is_empty());
        assert!(!PSM1.is_empty());
        assert!(!COMMON_PS1.is_empty());
        assert!(!HELP_PS1.is_empty());
        assert!(!FILES_PS1.is_empty());
        assert!(!INSTALL_PS1.is_empty());
    }

    #[test]
    fn template_extract_roundtrip() {
        let tmp = tempfile_dir();
        write_template_to(&tmp).expect("extract");
        assert!(tmp.join("DevShellTools.psd1").exists());
        assert!(tmp.join("Public").join("Git.ps1").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "dst-studio-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
