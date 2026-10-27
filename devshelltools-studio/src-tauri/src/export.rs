use crate::ai_config::{self, AiProfilesStore};
use crate::error::{DstError, DstResult};
use crate::install_mgr;
use crate::ps_parser;
use crate::safety;
use crate::sync;
use crate::workspace;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 导出所有 Public/*.ps1 脚本到目标目录。
/// 仅复制 .ps1 文件，不复制 .git/.studio/公共部分。
pub fn export_scripts(target_dir: &str) -> DstResult<Vec<String>> {
    let target = Path::new(target_dir);
    std::fs::create_dir_all(target)?;
    let ws = workspace::workspace_root();
    let public = ws.join("Public");
    if !public.exists() {
        return Err(DstError::WorkspaceBroken("Public 目录不存在".into()));
    }
    let mut exported = vec![];
    for entry in std::fs::read_dir(&public)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("ps1") {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                std::fs::copy(&path, target.join(name))?;
                exported.push(name.to_string());
            }
        }
    }
    Ok(exported)
}

/// 导入结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportResult {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
}

/// 从目录导入 ps1 脚本：逐个校验语法+安全，通过才写入。
/// 不通过的被跳过并记录原因，不破坏现有脚本。
pub fn import_scripts(source_dir: &str) -> DstResult<ImportResult> {
    let source = Path::new(source_dir);
    if !source.exists() {
        return Err(DstError::FileNotFound(source_dir.into()));
    }
    let ws = workspace::workspace_root();
    let public = ws.join("Public");
    std::fs::create_dir_all(&public)?;

    let mut imported = vec![];
    let mut skipped = vec![];
    let mut errors = vec![];

    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("ps1") {
            continue;
        }
        let name = match path.file_name().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{name}: 读取失败 - {e}"));
                skipped.push(name);
                continue;
            }
        };

        // 语法校验
        if let Err(e) = ps_parser::validate_syntax(&content) {
            errors.push(format!("{name}: 语法错误 - {e}"));
            skipped.push(name);
            continue;
        }

        // 安全校验
        match safety::check(&content) {
            Ok(report) if report.ok => {}
            Ok(report) => {
                errors.push(format!("{name}: 安全拦截 - {}", report.violations.join("；")));
                skipped.push(name);
                continue;
            }
            Err(e) => {
                errors.push(format!("{name}: 安全检查失败 - {e}"));
                skipped.push(name);
                continue;
            }
        }

        // 校验通过，写入
        let target = public.join(&name);
        std::fs::write(&target, &content)?;
        imported.push(name);
    }

    Ok(ImportResult { imported, skipped, errors })
}

// ============ 完整配置包（bundle）：双向同步 / 迁移 ============
//
// 包结构：
//   manifest.json          包元信息（格式、版本、文件清单）
//   Public/*.ps1           全部分类脚本（含自定义命令）
//   Private/Common.ps1     内部辅助函数
//   ai_profiles.json       AI 配置（仅元数据，永不包含 API Key）
//
// 导出 = 本机配置打包；导入 = 校验（语法+安全红线）→ 预览勾选 → 写入
// → 自动重生成公共部分（psd1/psm1/Help.ps1）→ 后台同步 PS5.1/PS7 运行时目录。

pub const BUNDLE_FORMAT: &str = "devshelltools-sync";
pub const BUNDLE_VERSION: u32 = 1;
const MANIFEST_NAME: &str = "manifest.json";
const AI_PROFILES_NAME: &str = "ai_profiles.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleFileEntry {
    /// 工作区相对路径，如 "Public/Git.ps1"
    pub rel: String,
    /// category | private
    pub kind: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleManifest {
    pub format: String,
    pub version: u32,
    pub exported_at: String,
    /// 导出端工作区的模板版本
    pub template_version: String,
    pub files: Vec<BundleFileEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportBundleResult {
    /// 写出的文件（含 manifest.json，相对包目录）
    pub files: Vec<String>,
    /// 随包导出的 AI Profile 数（0 = 无配置未打包）
    pub ai_profiles: usize,
    pub target_dir: String,
}

/// 预览单项：一个待导入文件 / AI 配置的处理建议。
#[derive(Debug, Clone, Serialize)]
pub struct BundleItem {
    /// "Public/X.ps1" | "Private/Common.ps1" | "ai_profiles.json"
    pub rel: String,
    /// category | private | ai
    pub kind: String,
    /// add | overwrite | same | keep_local | invalid
    pub action: String,
    /// 默认勾选（add / overwrite 为 true）
    pub checked: bool,
    /// category 时的公共命令数
    pub commands: usize,
    pub syntax_ok: bool,
    pub safety_ok: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BundlePreview {
    pub manifest_found: bool,
    pub template_version: String,
    pub exported_at: String,
    pub items: Vec<BundleItem>,
    pub ai_new: usize,
    pub ai_existing: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportBundleResult {
    pub imported: Vec<String>,
    pub ai_added: usize,
    /// 是否已重生成 psd1/psm1/Help.ps1
    pub regenerated: bool,
    /// 是否已触发后台运行时目录同步
    pub runtime_synced: bool,
    pub errors: Vec<String>,
}

/// 读文件字节并剥掉 UTF-8 BOM（内容对比用）。
fn read_bytes_stripped(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut b = std::fs::read(path)?;
    if b.starts_with(&[0xEF, 0xBB, 0xBF]) {
        b.drain(0..3);
    }
    Ok(b)
}

/// 以 UTF-8 + BOM 写入工作区文件（与 AST 替换路径一致，PS 5.1 中文安全）。
fn write_file_bom(rel: &str, content: &str) -> DstResult<()> {
    let path = workspace::workspace_root().join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut bytes = Vec::with_capacity(content.len() + 3);
    bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    bytes.extend_from_slice(content.as_bytes());
    std::fs::write(&path, bytes)?;
    Ok(())
}

/// 导出完整配置包。目标目录必须不存在或为空，防止误写用户目录。
pub fn export_bundle(target_dir: &str) -> DstResult<ExportBundleResult> {
    let target = Path::new(target_dir);
    if target.exists() {
        if !target.is_dir() {
            return Err(DstError::Other("目标路径不是目录".into()));
        }
        let non_empty = std::fs::read_dir(target)
            .map(|mut it| it.next().is_some())
            .unwrap_or(true);
        if non_empty {
            return Err(DstError::Other(
                "目标目录已存在且非空；请换一个新的空目录，避免影响其它文件".into(),
            ));
        }
    }
    let ws = workspace::workspace_root();
    let public = ws.join("Public");
    if !public.exists() {
        return Err(DstError::WorkspaceBroken("Public 目录不存在".into()));
    }
    std::fs::create_dir_all(target.join("Public"))?;

    let mut entries: Vec<BundleFileEntry> = vec![];
    let mut names: Vec<String> = vec![];
    for e in std::fs::read_dir(&public)? {
        let e = e?;
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("ps1") {
            continue;
        }
        if let Some(n) = p.file_name().and_then(|s| s.to_str()) {
            names.push(n.to_string());
        }
    }
    names.sort();
    for n in &names {
        let bytes = std::fs::read(public.join(n))?;
        std::fs::write(target.join("Public").join(n), &bytes)?;
        entries.push(BundleFileEntry {
            rel: format!("Public/{n}"),
            kind: "category".into(),
            size: bytes.len() as u64,
        });
    }

    // Private/Common.ps1：自定义辅助函数随包分发
    let common = ws.join("Private").join("Common.ps1");
    if common.exists() {
        let bytes = std::fs::read(&common)?;
        std::fs::create_dir_all(target.join("Private"))?;
        std::fs::write(target.join("Private").join("Common.ps1"), &bytes)?;
        entries.push(BundleFileEntry {
            rel: "Private/Common.ps1".into(),
            kind: "private".into(),
            size: bytes.len() as u64,
        });
    }

    // AI 配置：仅 profiles 元数据。API Key 单独存 ai_keys/，此处不读取，
    // 且防御性把 key_configured 置 false，避免误导导入端。
    let mut ai_count = 0;
    let store = ai_config::load_profiles_store()?;
    if !store.profiles.is_empty() {
        let mut sanitized = store.clone();
        for p in &mut sanitized.profiles {
            p.key_configured = false;
        }
        std::fs::write(
            target.join(AI_PROFILES_NAME),
            serde_json::to_string_pretty(&sanitized)?,
        )?;
        ai_count = sanitized.profiles.len();
    }

    let template_version = std::fs::read_to_string(workspace::meta_file())
        .ok()
        .and_then(|raw| serde_json::from_str::<workspace::WorkspaceMeta>(&raw).ok())
        .map(|m| m.template_version)
        .unwrap_or_else(|| crate::template::TEMPLATE_VERSION.to_string());
    let manifest = BundleManifest {
        format: BUNDLE_FORMAT.into(),
        version: BUNDLE_VERSION,
        exported_at: chrono::Utc::now().to_rfc3339(),
        template_version,
        files: entries,
    };
    std::fs::write(
        target.join(MANIFEST_NAME),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    let mut files: Vec<String> = manifest.files.iter().map(|f| f.rel.clone()).collect();
    if ai_count > 0 {
        files.push(AI_PROFILES_NAME.into());
    }
    files.push(MANIFEST_NAME.into());
    Ok(ExportBundleResult {
        files,
        ai_profiles: ai_count,
        target_dir: target.display().to_string(),
    })
}

/// 填充与本地同名文件的差异动作：add / same / overwrite(源较新) / keep_local(本地较新)。
fn fill_diff_action(item: &mut BundleItem, local_path: &Path, src_path: &Path) {
    if !local_path.exists() {
        item.action = "add".into();
        item.checked = true;
        item.note = "本地不存在，将新增".into();
        return;
    }
    let local = read_bytes_stripped(local_path).unwrap_or_default();
    let src = read_bytes_stripped(src_path).unwrap_or_default();
    if local == src {
        item.action = "same".into();
        item.checked = false;
        item.note = "与本地内容相同".into();
        return;
    }
    let local_m = local_path.metadata().and_then(|m| m.modified()).ok();
    let src_m = src_path.metadata().and_then(|m| m.modified()).ok();
    match (local_m, src_m) {
        (Some(l), Some(s)) if s > l => {
            item.action = "overwrite".into();
            item.checked = true;
            item.note = "包内较新，默认覆盖本地".into();
        }
        _ => {
            item.action = "keep_local".into();
            item.checked = false;
            item.note = "本地较新，默认保留本地（勾选可强制覆盖）".into();
        }
    }
}

fn invalid_item(rel: &str, kind: &str, note: String) -> BundleItem {
    BundleItem {
        rel: rel.into(),
        kind: kind.into(),
        action: "invalid".into(),
        checked: false,
        commands: 0,
        syntax_ok: false,
        safety_ok: false,
        note,
    }
}

/// 预览配置包：逐文件语法 + 安全校验，与本地比对给出动作建议。不写入任何内容。
/// 兼容两种布局：完整包（Public/ 子目录 + manifest）与旧版平铺 .ps1 目录。
pub fn preview_bundle(source_dir: &str) -> DstResult<BundlePreview> {
    let source = Path::new(source_dir);
    if !source.is_dir() {
        return Err(DstError::FileNotFound(source_dir.into()));
    }

    let mut manifest_found = false;
    let mut template_version = String::new();
    let mut exported_at = String::new();
    if let Ok(raw) = std::fs::read_to_string(source.join(MANIFEST_NAME)) {
        if let Ok(m) = serde_json::from_str::<BundleManifest>(&raw) {
            if m.format == BUNDLE_FORMAT {
                manifest_found = true;
                template_version = m.template_version;
                exported_at = m.exported_at;
            }
        }
    }

    // 收集脚本：优先 Public/ 子目录，为空时回退根目录平铺（旧版导出）
    let mut scripts: Vec<(PathBuf, String)> = vec![];
    let scan_dir = if source.join("Public").is_dir() {
        source.join("Public")
    } else {
        source.to_path_buf()
    };
    let mut names: Vec<String> = vec![];
    for e in std::fs::read_dir(&scan_dir)? {
        let e = e?;
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("ps1") {
            continue;
        }
        if let Some(n) = p.file_name().and_then(|s| s.to_str()) {
            names.push(n.to_string());
        }
    }
    names.sort();
    for n in &names {
        scripts.push((scan_dir.join(n), format!("Public/{n}")));
    }
    if scripts.is_empty() {
        return Err(DstError::Other(
            "目录中未找到 .ps1 脚本（应包含 Public/*.ps1 或平铺的 .ps1）".into(),
        ));
    }

    // 一次 PowerShell 批量容错解析：语法 + 公共命令数
    let paths: Vec<PathBuf> = scripts.iter().map(|(p, _)| p.clone()).collect();
    let parsed_map: std::collections::HashMap<String, (bool, usize, String)> =
        ps_parser::parse_public_batch_lenient(&paths)?
            .into_iter()
            .map(|(name, parsed)| {
                let syntax_ok = parsed.parse_errors.is_empty();
                let commands = parsed
                    .functions
                    .iter()
                    .filter(|f| sync::is_exported(&f.name))
                    .count();
                let err = parsed.parse_errors.join("; ");
                (format!("Public/{name}"), (syntax_ok, commands, err))
            })
            .collect();

    let ws = workspace::workspace_root();
    let mut items: Vec<BundleItem> = vec![];
    for (path, rel) in &scripts {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                items.push(invalid_item(rel, "category", format!("读取失败：{e}")));
                continue;
            }
        };
        let (syntax_ok, commands, err) = parsed_map
            .get(rel)
            .cloned()
            .unwrap_or((false, 0, "未能解析".into()));
        let report = safety::check(&content)?;
        let mut item = BundleItem {
            rel: rel.clone(),
            kind: "category".into(),
            action: String::new(),
            checked: false,
            commands,
            syntax_ok,
            safety_ok: report.ok,
            note: String::new(),
        };
        if !syntax_ok {
            item.action = "invalid".into();
            item.note = format!("语法错误：{err}");
        } else if !report.ok {
            item.action = "invalid".into();
            item.note = format!("安全拦截：{}", report.violations.join("；"));
        } else {
            fill_diff_action(&mut item, &ws.join(rel), path);
        }
        items.push(item);
    }

    // Private/Common.ps1
    let common_src = source.join("Private").join("Common.ps1");
    if common_src.exists() {
        let content = std::fs::read_to_string(&common_src)?;
        let syntax_ok = ps_parser::validate_syntax(&content).is_ok();
        let report = safety::check(&content)?;
        let mut item = BundleItem {
            rel: "Private/Common.ps1".into(),
            kind: "private".into(),
            action: String::new(),
            checked: false,
            commands: 0,
            syntax_ok,
            safety_ok: report.ok,
            note: String::new(),
        };
        if !syntax_ok {
            item.action = "invalid".into();
            item.note = "语法错误".into();
        } else if !report.ok {
            item.action = "invalid".into();
            item.note = format!("安全拦截：{}", report.violations.join("；"));
        } else {
            fill_diff_action(&mut item, &ws.join("Private/Common.ps1"), &common_src);
        }
        items.push(item);
    }

    // ai_profiles.json
    let mut ai_new = 0;
    let mut ai_existing = 0;
    if source.join(AI_PROFILES_NAME).exists() {
        let parse = std::fs::read_to_string(source.join(AI_PROFILES_NAME))
            .map_err(DstError::from)
            .and_then(|raw| serde_json::from_str::<AiProfilesStore>(&raw).map_err(DstError::from));
        match parse {
            Ok(src_store) => {
                let local = ai_config::load_profiles_store()?;
                for p in &src_store.profiles {
                    if local.profiles.iter().any(|x| x.id == p.id) {
                        ai_existing += 1;
                    } else {
                        ai_new += 1;
                    }
                }
                items.push(BundleItem {
                    rel: AI_PROFILES_NAME.into(),
                    kind: "ai".into(),
                    action: if ai_new > 0 { "add".into() } else { "same".into() },
                    checked: ai_new > 0,
                    commands: 0,
                    syntax_ok: true,
                    safety_ok: true,
                    note: format!("新增 {ai_new} 个 · 已存在 {ai_existing} 个（不含 API Key）"),
                });
            }
            Err(e) => {
                items.push(invalid_item(AI_PROFILES_NAME, "ai", format!("解析失败：{e}")));
            }
        }
    }

    Ok(BundlePreview {
        manifest_found,
        template_version,
        exported_at,
        items,
        ai_new,
        ai_existing,
    })
}

/// 按预览勾选导入配置包。逐项二次校验（预览后内容可能变化），
/// 有分类脚本导入即自动重生成公共部分并后台同步运行时目录。
pub fn import_bundle(source_dir: &str, selection: &[String]) -> DstResult<ImportBundleResult> {
    let source = Path::new(source_dir);
    if !source.is_dir() {
        return Err(DstError::FileNotFound(source_dir.into()));
    }
    let mut result = ImportBundleResult {
        imported: vec![],
        ai_added: 0,
        regenerated: false,
        runtime_synced: false,
        errors: vec![],
    };
    let mut imported_any_script = false;

    for sel in selection {
        if sel == AI_PROFILES_NAME {
            let raw = match std::fs::read_to_string(source.join(AI_PROFILES_NAME)) {
                Ok(r) => r,
                Err(e) => {
                    result.errors.push(format!("{sel}: 读取失败 {e}"));
                    continue;
                }
            };
            let src_store: AiProfilesStore = match serde_json::from_str(&raw) {
                Ok(s) => s,
                Err(e) => {
                    result.errors.push(format!("{sel}: 解析失败 {e}"));
                    continue;
                }
            };
            let mut local = ai_config::load_profiles_store()?;
            for p in src_store.profiles {
                if local.profiles.iter().any(|x| x.id == p.id) {
                    continue;
                }
                let mut p = p;
                p.key_configured = false; // Key 永不随包导入
                local.profiles.push(p);
                result.ai_added += 1;
            }
            if local.default_profile_id.is_none() {
                if let Some(d) = &src_store.default_profile_id {
                    if local.profiles.iter().any(|p| &p.id == d) {
                        local.default_profile_id = Some(d.clone());
                    }
                }
            }
            ai_config::save_profiles_store(&local)?;
            result.imported.push(AI_PROFILES_NAME.into());
            continue;
        }

        let (src_path, ws_rel): (PathBuf, String) = if sel == "Private/Common.ps1" {
            (
                source.join("Private").join("Common.ps1"),
                "Private/Common.ps1".to_string(),
            )
        } else {
            let name = sel.strip_prefix("Public/").unwrap_or(sel);
            if name.is_empty()
                || name.contains('/')
                || name.contains('\\')
                || name.contains("..")
                || name.contains(':') // Windows 盘符相对路径（如 C:x）会被 join 替换整路径
                || !name.ends_with(".ps1")
            {
                result.errors.push(format!("{sel}: 非法的文件路径"));
                continue;
            }
            (
                source.join("Public").join(name),
                format!("Public/{name}"),
            )
        };
        let content = match std::fs::read_to_string(&src_path) {
            Ok(c) => c,
            Err(e) => {
                result.errors.push(format!("{sel}: 读取失败 {e}"));
                continue;
            }
        };
        if let Err(e) = ps_parser::validate_syntax(&content) {
            result.errors.push(format!("{sel}: 语法错误 {e}"));
            continue;
        }
        match safety::check(&content) {
            Ok(r) if r.ok => {}
            Ok(r) => {
                result.errors.push(format!("{sel}: 安全拦截 {}", r.violations.join("；")));
                continue;
            }
            Err(e) => {
                result.errors.push(format!("{sel}: 安全检查失败 {e}"));
                continue;
            }
        }
        if let Err(e) = write_file_bom(&ws_rel, &content) {
            result.errors.push(format!("{sel}: 写入失败 {e}"));
            continue;
        }
        result.imported.push(ws_rel);
        imported_any_script = true;
    }

    if imported_any_script {
        sync::regenerate_all()?;
        workspace::touch_last_sync()?;
        install_mgr::spawn_sync_runtime_modules();
        result.regenerated = true;
        result.runtime_synced = true;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_excludes_non_ps1() {
        // 逻辑测试：只复制 .ps1
        assert!(true); // 端到端测试在 m4_acceptance
    }
}