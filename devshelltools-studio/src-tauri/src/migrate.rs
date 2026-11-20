use crate::error::{DstError, DstResult};
use crate::install_mgr;
use crate::workspace;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn documents_dir() -> PathBuf {
    crate::workspace::my_documents_path_public().unwrap_or_else(|| {
        std::env::var("USERPROFILE")
            .map(|p| PathBuf::from(p).join("Documents"))
            .unwrap_or_else(|_| PathBuf::from("."))
    })
}

/// 旧 Studio 沙箱（真正的「旧版」工作区，迁移后应归档）。
fn old_sandbox_dir() -> PathBuf {
    documents_dir().join("DevShellTools")
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

/// 可从中回收 Public/*.ps1 的来源：旧沙箱 + 非工作区的模块目录。
fn merge_source_dirs() -> Vec<PathBuf> {
    let ws = workspace::workspace_root();
    let mut dirs = vec![
        old_sandbox_dir(),
        install_mgr::ps51_module_dir(),
        install_mgr::ps7_module_dir(),
    ];
    dirs.retain(|d| !same_path(d, &ws));
    dirs.dedup_by(|a, b| same_path(a, b));
    dirs
}

fn is_old_sandbox(path: &Path) -> bool {
    same_path(path, &old_sandbox_dir())
}

fn is_runtime_module_dir(path: &Path) -> bool {
    same_path(path, &install_mgr::ps51_module_dir())
        || same_path(path, &install_mgr::ps7_module_dir())
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    path.metadata().and_then(|m| m.modified()).ok()
}

/// 源比目标新，或目标不存在 → 应采用源文件（保留最新）。
fn source_is_newer_or_missing(src: &Path, dst: &Path) -> bool {
    if !dst.exists() {
        return true;
    }
    match (file_mtime(src), file_mtime(dst)) {
        (Some(s), Some(d)) => s > d,
        (Some(_), None) => true,
        _ => false,
    }
}

/// 该目录是否仍有「需要并入工作区」的内容，或属于应清理的旧沙箱。
fn dir_actionable(legacy: &Path, ws_public: &Path) -> bool {
    if !legacy.exists() || !legacy.join("DevShellTools.psd1").exists() {
        return false;
    }
    // 旧沙箱只要还在，就提示迁移（迁移后会归档）
    if is_old_sandbox(legacy) {
        return true;
    }
    // 运行时模块目录：仅当存在工作区没有的、或更新的 Public 脚本时才提示
    if !is_runtime_module_dir(legacy) {
        return true;
    }
    let legacy_public = legacy.join("Public");
    if !legacy_public.is_dir() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(&legacy_public) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("ps1") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // Help.ps1 由 Studio 重生成，不作为迁移依据
        if name.eq_ignore_ascii_case("Help.ps1") {
            continue;
        }
        let target = ws_public.join(name);
        if source_is_newer_or_missing(&path, &target) {
            return true;
        }
    }
    false
}

/// 检测仍需处理的旧版来源（用于 UI）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct MigrationCheck {
    pub has_legacy: bool,
    pub legacy_dirs: Vec<String>,
    pub workspace_initialized: bool,
}

pub fn check_migration() -> MigrationCheck {
    let ws_public = workspace::workspace_root().join("Public");
    let legacy: Vec<String> = merge_source_dirs()
        .into_iter()
        .filter(|d| dir_actionable(d, &ws_public))
        .map(|d| {
            if is_old_sandbox(&d) {
                format!("旧 Studio 沙箱：{}", d.display())
            } else if same_path(&d, &install_mgr::ps7_module_dir()) {
                format!("PowerShell 7 模块目录（含尚未并入的命令）：{}", d.display())
            } else if same_path(&d, &install_mgr::ps51_module_dir()) {
                format!(
                    "Windows PowerShell 5.1 模块目录（含尚未并入的命令）：{}",
                    d.display()
                )
            } else {
                d.display().to_string()
            }
        })
        .collect();
    MigrationCheck {
        has_legacy: !legacy.is_empty(),
        legacy_dirs: legacy,
        workspace_initialized: workspace::is_initialized(),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MigrateResult {
    /// 实际写入/更新到工作区的文件名
    pub migrated_files: Vec<String>,
    /// 已归档的旧沙箱路径
    pub archived_dirs: Vec<String>,
    pub message: String,
}

fn archive_old_sandbox() -> DstResult<Option<String>> {
    let sandbox = old_sandbox_dir();
    if !sandbox.exists() {
        return Ok(None);
    }
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = documents_dir().join(format!("DevShellTools.migrated-{ts}"));
    // 若目标偶发存在则加后缀
    let dest = if dest.exists() {
        documents_dir().join(format!(
            "DevShellTools.migrated-{ts}-{}",
            std::process::id()
        ))
    } else {
        dest
    };
    std::fs::rename(&sandbox, &dest).map_err(|e| {
        DstError::Other(format!(
            "归档旧沙箱失败：{} → {}：{e}",
            sandbox.display(),
            dest.display()
        ))
    })?;
    Ok(Some(dest.display().to_string()))
}

/// 执行迁移：把可回收目录中较新的 Public/*.ps1 并入工作区，重生成公共部分，
/// 同步到 PS7 运行时目录，并归档旧 Studio 沙箱。
pub fn migrate_from_legacy() -> DstResult<MigrateResult> {
    let check = check_migration();
    if !check.has_legacy {
        return Err(DstError::Other("未检测到需要迁移的旧版内容".into()));
    }
    if !workspace::is_initialized() {
        workspace::init_from_template()?;
    }

    let ws_root = workspace::workspace_root();
    let ws_public = ws_root.join("Public");
    std::fs::create_dir_all(&ws_public)?;

    let mut migrated = vec![];
    let sources: Vec<PathBuf> = merge_source_dirs()
        .into_iter()
        .filter(|d| d.exists() && d.join("Public").is_dir())
        .collect();

    for legacy_dir in &sources {
        let legacy_public = legacy_dir.join("Public");
        let Ok(entries) = std::fs::read_dir(&legacy_public) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("ps1") {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if name.eq_ignore_ascii_case("Help.ps1") {
                continue;
            }
            let target = ws_public.join(name);
            if !source_is_newer_or_missing(&path, &target) {
                continue;
            }
            std::fs::copy(&path, &target)
                .map_err(|e| DstError::Other(format!("复制 {name} 失败：{e}")))?;
            if !migrated.iter().any(|n| n == name) {
                migrated.push(name.to_string());
            }
        }
    }

    migrated.sort();
    crate::sync::invalidate_category_cache();
    crate::sync::regenerate_all()?;
    workspace::touch_last_sync()?;

    let mut archived_dirs = vec![];
    if let Some(archived) = archive_old_sandbox()? {
        archived_dirs.push(archived);
    }

    // 同步运行时目录；失败须暴露给用户，禁止写「已对齐」假成功
    let sync_result = install_mgr::sync_runtime_modules();

    let message = {
        let mut parts = vec![format!(
            "已将 {} 个较新/缺失的脚本并入当前工作区",
            migrated.len()
        )];
        if !archived_dirs.is_empty() {
            parts.push(format!("已归档旧沙箱 → {}", archived_dirs.join("、")));
        }
        match &sync_result {
            Ok(note) if !note.is_empty() => {
                parts.push(note.clone());
                parts.push("PowerShell 7 / 5.1 模块目录已按当前工作区对齐，不再视为旧版。".into());
            }
            Ok(_) => {
                parts.push("PowerShell 7 / 5.1 模块目录已按当前工作区对齐，不再视为旧版。".into());
            }
            Err(e) => {
                parts.push(format!(
                    "工作区已更新，但同步到 PowerShell 模块目录失败：{e}（可稍后在工具箱重试或手动「同步公共部分」）"
                ));
            }
        }
        parts.join("。")
    };

    // 工作区合并/归档已成功时仍返回 Ok，但 message 标明同步失败，供 UI 用 info/warn 展示
    Ok(MigrateResult {
        migrated_files: migrated,
        archived_dirs,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_sandbox_path_under_documents() {
        let p = old_sandbox_dir();
        let s = p.to_string_lossy();
        assert!(
            s.ends_with("DevShellTools")
                || s.ends_with("DevShellTools\\")
                || s.ends_with("DevShellTools/"),
            "{s}"
        );
        assert!(
            s.contains("Documents") || s.contains("documents"),
            "应在 Documents 下：{s}"
        );
    }

    #[test]
    fn source_newer_logic() {
        let dir = std::env::temp_dir().join(format!(
            "dst-mtime-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let a = dir.join("a.ps1");
        let b = dir.join("b.ps1");
        std::fs::write(&a, "a").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&b, "b").unwrap();
        assert!(source_is_newer_or_missing(&b, &a));
        assert!(!source_is_newer_or_missing(&a, &b));
        assert!(source_is_newer_or_missing(&a, &dir.join("missing.ps1")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_check_returns_struct() {
        let check = check_migration();
        // 标签文案可能变长，条数仍有限
        assert!(check.legacy_dirs.len() <= 3);
    }

    #[test]
    fn merge_sources_exclude_workspace() {
        let ws = workspace::workspace_root();
        for d in merge_source_dirs() {
            assert!(!same_path(&d, &ws), "来源不应包含工作区本身");
        }
    }
}
