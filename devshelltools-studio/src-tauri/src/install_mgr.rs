use crate::error::{DstError, DstResult};
use crate::process_util::{output_hidden, ps_base_args};
use crate::workspace;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallStatus {
    pub workspace_ready: bool,
    pub ps51_module_present: bool,
    pub ps7_module_present: bool,
    pub profile_configured: bool,
    pub installed: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallResult {
    pub status: InstallStatus,
    pub message: String,
    pub verified: bool,
}

fn documents_dir() -> PathBuf {
    crate::workspace::my_documents_path_public()
        .unwrap_or_else(|| {
            std::env::var("USERPROFILE")
                .map(|p| PathBuf::from(p).join("Documents"))
                .unwrap_or_else(|_| PathBuf::from("."))
        })
}

pub fn ps51_module_dir() -> PathBuf {
    documents_dir().join("WindowsPowerShell").join("Modules").join("DevShellTools")
}

pub fn ps7_module_dir() -> PathBuf {
    documents_dir().join("PowerShell").join("Modules").join("DevShellTools")
}

fn profile_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut roots = vec![documents_dir()];
    if let Ok(up) = std::env::var("USERPROFILE") {
        let user_docs = PathBuf::from(up).join("Documents");
        if user_docs != documents_dir() {
            roots.push(user_docs);
        }
    }
    for docs in roots {
        paths.push(
            docs.join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        );
        paths.push(docs.join("WindowsPowerShell").join("profile.ps1"));
        paths.push(
            docs.join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        );
        paths.push(docs.join("PowerShell").join("profile.ps1"));
    }
    paths
}

fn profile_has_import(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    std::fs::read_to_string(path)
        .map(|c| {
            c.lines()
                .any(|l| l.trim().starts_with("Import-Module DevShellTools"))
        })
        .unwrap_or(false)
}

pub fn install_status() -> InstallStatus {
    // 工作区就绪：允许清单处于软卸载禁用态
    let workspace_ready = workspace::is_initialized() || workspace::has_module_manifest();
    // shell 可加载：活动 .psd1 与 .psm1 均在（仅留其一仍可能被自动加载）
    let ps51 = ps51_module_dir();
    let ps7 = ps7_module_dir();
    let ps51_module_present =
        ps51.join("DevShellTools.psd1").exists() && ps51.join("DevShellTools.psm1").exists();
    let ps7_module_present =
        ps7.join("DevShellTools.psd1").exists() && ps7.join("DevShellTools.psm1").exists();
    let profile_configured = profile_paths().iter().any(|p| profile_has_import(p));
    let installed =
        profile_configured && (ps51_module_present || ps7_module_present);
    InstallStatus {
        workspace_ready,
        ps51_module_present,
        ps7_module_present,
        profile_configured,
        installed,
    }
}

/// 每次安装前从内嵌模板覆盖写入 install.ps1，避免工作区残留旧版自毁脚本。
fn ensure_install_script() -> DstResult<PathBuf> {
    let ws = workspace::workspace_root();
    std::fs::create_dir_all(&ws)
        .map_err(|e| DstError::Other(format!("创建工作区失败：{e}")))?;
    let install_ps1 = ws.join("install.ps1");
    std::fs::write(&install_ps1, crate::template::INSTALL_PS1)
        .map_err(|e| DstError::Other(format!("写入 install.ps1 到工作区失败：{e}")))?;
    Ok(install_ps1)
}

/// 每次卸载前从内嵌模板覆盖写入 uninstall.ps1。
fn ensure_uninstall_script() -> DstResult<PathBuf> {
    let ws = workspace::workspace_root();
    std::fs::create_dir_all(&ws)
        .map_err(|e| DstError::Other(format!("创建工作区失败：{e}")))?;
    let uninstall_ps1 = ws.join("uninstall.ps1");
    std::fs::write(&uninstall_ps1, crate::template::UNINSTALL_PS1)
        .map_err(|e| DstError::Other(format!("写入 uninstall.ps1 到工作区失败：{e}")))?;
    Ok(uninstall_ps1)
}

/// 安装：执行工作区的 install.ps1（$PSScriptRoot 指向工作区，不会误复制 temp 文件）。
pub fn install_module() -> DstResult<InstallResult> {
    let install_ps1 = ensure_install_script()?;
    let exe = "powershell";
    let mut cmd = Command::new(exe);
    for arg in ps_base_args(exe) {
        cmd.arg(arg);
    }
    cmd.arg("-File").arg(&install_ps1);
    let output = output_hidden(cmd)
        .map_err(|e| DstError::Other(format!("启动 install.ps1 失败：{e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DstError::Other(format!(
            "install.ps1 执行失败：{}",
            stderr.trim()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let status = install_status();
    let verified = status.installed;
    let msg_lines: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains("[成功]") || l.contains("[备份]") || l.contains("[兼容]"))
        .collect();
    let message = if msg_lines.is_empty() {
        "安装完成。".into()
    } else {
        format!("安装完成。\n{}", msg_lines.join("\n"))
    };
    Ok(InstallResult {
        status,
        message,
        verified,
    })
}

/// 后台同步到 PS7 等运行时目录，不阻塞编辑返回（IDE 手感）。
pub fn spawn_sync_runtime_modules() {
    std::thread::spawn(|| {
        match sync_runtime_modules() {
            Ok(msg) => log::info!("后台模块同步：{msg}"),
            Err(e) => log::warn!("后台模块同步失败：{e}"),
        }
    });
}

/// 将工作区模块文件同步到其它 PowerShell 模块目录（通常为 PS7）。
/// 工作区本身即 PS5.1 目录，写入后该 shell 已是最新；PS7 需复制，否则只能靠重装。
pub fn sync_runtime_modules() -> DstResult<String> {
    let src = workspace::workspace_root();
    if !src.join("DevShellTools.psd1").exists() && !src.join(workspace::PSD1_DISABLED_NAME).exists() {
        return Err(DstError::Other("工作区未初始化，无法同步模块".into()));
    }
    let mut notes = Vec::new();
    for target in [ps51_module_dir(), ps7_module_dir()] {
        if paths_same(&src, &target) {
            notes.push(format!("已是最新：{}", target.display()));
            continue;
        }
        copy_module_tree(&src, &target)?;
        notes.push(format!("已同步：{}", target.display()));
    }
    notes.push("已打开的 PowerShell 请执行：Import-Module DevShellTools -Force".into());
    Ok(notes.join("；"))
}

fn paths_same(a: &Path, b: &Path) -> bool {
    let fa = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let fb = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    fa == fb
}

fn copy_module_tree(src: &Path, dst: &Path) -> DstResult<()> {
    std::fs::create_dir_all(dst).map_err(|e| DstError::Other(format!("创建模块目录失败：{e}")))?;
    for name in ["DevShellTools.psd1", "DevShellTools.psm1", "Private", "Public"] {
        let from = src.join(name);
        if !from.exists() {
            // 软卸载时可能是 *.dst-disabled
            if name == "DevShellTools.psd1" {
                let disabled = src.join(workspace::PSD1_DISABLED_NAME);
                if disabled.exists() {
                    continue; // 禁用态不同步活动清单
                }
            }
            if name == "DevShellTools.psm1" {
                let disabled = src.join(workspace::PSM1_DISABLED_NAME);
                if disabled.exists() {
                    continue;
                }
            }
            continue;
        }
        let to = dst.join(name);
        // 增量复制：未变更文件跳过，避免每次全量 wipe+copy
        copy_path(&from, &to)?;
    }
    // 清理目标 Public 中源已删除的 .ps1（防止残留旧分类）
    sync_delete_stale_ps1(&src.join("Public"), &dst.join("Public"))?;
    Ok(())
}

fn sync_delete_stale_ps1(src_public: &Path, dst_public: &Path) -> DstResult<()> {
    if !dst_public.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dst_public)
        .map_err(|e| DstError::Other(format!("读取目标 Public 失败：{e}")))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("ps1") {
            continue;
        }
        let name = entry.file_name();
        if !src_public.join(&name).exists() {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}

fn copy_path(from: &Path, to: &Path) -> DstResult<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)
            .map_err(|e| DstError::Other(format!("创建目录失败 {}: {e}", to.display())))?;
        for entry in std::fs::read_dir(from)
            .map_err(|e| DstError::Other(format!("读取目录失败 {}: {e}", from.display())))?
        {
            let entry = entry.map_err(|e| DstError::Other(format!("读取目录项失败：{e}")))?;
            let name = entry.file_name();
            copy_path(&entry.path(), &to.join(name))?;
        }
    } else {
        if files_identical(from, to) {
            return Ok(());
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DstError::Other(format!("创建父目录失败：{e}")))?;
        }
        std::fs::copy(from, to).map_err(|e| {
            DstError::Other(format!("复制失败 {} → {}：{e}", from.display(), to.display()))
        })?;
    }
    Ok(())
}

/// 内容级比较：函数级删除/编辑不改变文件名，且 mtime 在跨目录复制、
/// git checkout 等场景下不可靠（曾导致 PS7 副本残留已删除的函数）。
/// 逐字节比较（模块文件都很小）确保 PS5.1/PS7 副本与工作区完全一致。
fn files_identical(from: &Path, to: &Path) -> bool {
    let (Ok(a), Ok(b)) = (std::fs::read(from), std::fs::read(to)) else {
        return false;
    };
    a == b
}

/// 卸载：执行工作区的 uninstall.ps1（软卸载：保留 Studio 工作区，禁用 shell 自动加载）。
pub fn uninstall_module() -> DstResult<InstallResult> {
    let uninstall_ps1 = ensure_uninstall_script()?;
    let exe = "powershell";
    let mut cmd = Command::new(exe);
    for arg in ps_base_args(exe) {
        cmd.arg(arg);
    }
    cmd.arg("-File").arg(&uninstall_ps1);
    let output = output_hidden(cmd)
        .map_err(|e| DstError::Other(format!("启动 uninstall.ps1 失败：{e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DstError::Other(format!(
            "uninstall.ps1 执行失败：{}",
            stderr.trim()
        )));
    }
    let status = install_status();
    let verified = !status.installed && !workspace::is_shell_enabled();
    Ok(InstallResult {
        status,
        message: "卸载完成：已清理 Profile 并禁用模块自动加载。新开 PowerShell 中 dsh 应不可用；Studio 工作区仍可编辑。".into(),
        verified,
    })
}
