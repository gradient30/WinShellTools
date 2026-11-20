use crate::error::DstResult;
use crate::ps_render::quote_data;
use crate::ps_parser::{self, CategoryMeta, PsFunction};
use crate::workspace;
use std::sync::Mutex;
use std::time::SystemTime;

/// 一个分类的完整信息（元数据 + 函数列表 + 文件名）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CategoryInfo {
    pub file_name: String,   // 如 "Git.ps1"
    pub category: CategoryMeta,
    pub functions: Vec<PsFunction>,
}

struct CategoryCache {
    stamp: u64,
    data: Vec<CategoryInfo>,
    /// Help.ps1 等无分类块文件中的导出函数（如 dsh）
    extras: Vec<PsFunction>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct DiskCategoryCache {
    stamp: u64,
    categories: Vec<CategoryInfo>,
    #[serde(default)]
    extras: Vec<PsFunction>,
}

static CATEGORY_CACHE: Mutex<Option<CategoryCache>> = Mutex::new(None);

fn disk_cache_path() -> std::path::PathBuf {
    workspace::studio_dir().join("categories_cache.json")
}

fn public_stamp() -> u64 {
    let public = workspace::workspace_root().join("Public");
    if !public.exists() {
        return 0;
    }
    let mut max = 0u64;
    let mut count = 0u64;
    if let Ok(entries) = std::fs::read_dir(&public) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("ps1") {
                continue;
            }
            // Help.ps1 由 regenerate 每次重写，若计入 stamp 会导致分类缓存必失效
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case("Help.ps1"))
            {
                continue;
            }
            count += 1;
            if let Ok(m) = e.metadata().and_then(|m| m.modified()) {
                if let Ok(d) = m.duration_since(SystemTime::UNIX_EPOCH) {
                    max = max.max(d.as_secs());
                }
            }
        }
    }
    max.wrapping_mul(1_000_003).wrapping_add(count)
}

fn prime_category_cache_with_extras(data: &[CategoryInfo], extras: &[PsFunction]) {
    let stamp = public_stamp();
    let _ = save_disk_cache(stamp, data, extras);
    if let Ok(mut g) = CATEGORY_CACHE.lock() {
        *g = Some(CategoryCache {
            stamp,
            data: data.to_vec(),
            extras: extras.to_vec(),
        });
    }
}

/// 单次 PowerShell 批量解析 Public/*.ps1，拆成分类文件与非分类文件。
fn scan_public_once() -> DstResult<(Vec<CategoryInfo>, Vec<PsFunction>)> {
    let files = workspace::list_public_files()?;
    let root = workspace::workspace_root();
    let paths: Vec<std::path::PathBuf> = files
        .iter()
        .map(|f| root.join("Public").join(f))
        .collect();
    let parsed_batch = ps_parser::parse_public_batch(&paths)?;
    let mut cats = vec![];
    let mut extras = vec![];
    for (file_name, parsed) in parsed_batch {
        if let Some(cat) = parsed.category {
            cats.push(CategoryInfo {
                file_name,
                category: cat,
                functions: parsed.functions,
            });
        } else {
            extras.extend(parsed.functions);
        }
    }
    cats.sort_by(|a, b| a.category.name.cmp(&b.category.name));
    Ok((cats, extras))
}

fn load_disk_cache(stamp: u64) -> Option<(Vec<CategoryInfo>, Vec<PsFunction>)> {
    let path = disk_cache_path();
    let content = std::fs::read_to_string(path).ok()?;
    let cache: DiskCategoryCache = serde_json::from_str(&content).ok()?;
    if cache.stamp == stamp {
        Some((cache.categories, cache.extras))
    } else {
        None
    }
}

fn save_disk_cache(stamp: u64, data: &[CategoryInfo], extras: &[PsFunction]) -> DstResult<()> {
    let _ = std::fs::create_dir_all(workspace::studio_dir());
    let cache = DiskCategoryCache {
        stamp,
        categories: data.to_vec(),
        extras: extras.to_vec(),
    };
    let json = serde_json::to_string(&cache)?;
    std::fs::write(disk_cache_path(), json)?;
    Ok(())
}

/// 增量补丁用快照：忽略 stamp（刚写入的文件会令 mtime/stamp 变化）。
/// 无有效 extras 时返回 None，迫使全量扫描。
fn snapshot_for_patch() -> Option<(Vec<CategoryInfo>, Vec<PsFunction>)> {
    if let Ok(g) = CATEGORY_CACHE.lock() {
        if let Some(c) = g.as_ref() {
            if !c.extras.is_empty() {
                return Some((c.data.clone(), c.extras.clone()));
            }
        }
    }
    let path = disk_cache_path();
    let content = std::fs::read_to_string(path).ok()?;
    let cache: DiskCategoryCache = serde_json::from_str(&content).ok()?;
    if cache.extras.is_empty() {
        return None;
    }
    Some((cache.categories, cache.extras))
}

pub fn invalidate_category_cache() {
    if let Ok(mut g) = CATEGORY_CACHE.lock() {
        *g = None;
    }
    let _ = std::fs::remove_file(disk_cache_path());
}

/// 返回 (分类列表, 是否来自缓存)。
pub fn scan_categories_cached_with_meta() -> DstResult<(Vec<CategoryInfo>, bool)> {
    let stamp = public_stamp();
    if let Ok(g) = CATEGORY_CACHE.lock() {
        if let Some(c) = g.as_ref() {
            if c.stamp == stamp {
                return Ok((c.data.clone(), true));
            }
        }
    }
    if let Some((data, extras)) = load_disk_cache(stamp) {
        if let Ok(mut g) = CATEGORY_CACHE.lock() {
            *g = Some(CategoryCache {
                stamp,
                data: data.clone(),
                extras,
            });
        }
        return Ok((data, true));
    }
    let (cats, extras) = scan_public_once()?;
    let _ = save_disk_cache(stamp, &cats, &extras);
    if let Ok(mut g) = CATEGORY_CACHE.lock() {
        *g = Some(CategoryCache {
            stamp,
            data: cats.clone(),
            extras,
        });
    }
    Ok((cats, false))
}

/// 扫描工作区 Public/ 下所有 .ps1 文件，返回分类列表（内存 + 磁盘 mtime 缓存）。
pub fn scan_categories_cached() -> DstResult<Vec<CategoryInfo>> {
    scan_categories_cached_with_meta().map(|(data, _)| data)
}

/// 扫描工作区 Public/ 下所有 .ps1 文件，返回分类列表。
/// 无 @DST-Category 块的文件（如 Help.ps1）被跳过。
pub fn scan_categories() -> DstResult<Vec<CategoryInfo>> {
    Ok(scan_public_once()?.0)
}

/// 所有非分类文件中的函数（如 Help.ps1 的 dsh/Show-DstCategories 等）。
pub fn scan_extra_functions() -> DstResult<Vec<PsFunction>> {
    Ok(scan_public_once()?.1)
}

/// 全部应导出的函数名（分类文件函数 + 非分类文件函数）。
/// 约定：小写开头的函数为公共导出命令；大写开头（如 Assert-Git、Show-Dst*、Write-Dst*）为内部辅助函数，不导出。
pub fn all_export_names() -> DstResult<Vec<String>> {
    let mut names = vec![];
    for c in scan_categories()? {
        names.extend(c.functions.iter().filter(|f| is_exported(&f.name)).map(|f| f.name.clone()));
    }
    let extras = scan_extra_functions()?;
    names.extend(extras.iter().filter(|f| is_exported(&f.name)).map(|f| f.name.clone()));
    names.sort();
    names.dedup();
    Ok(names)
}

/// 判断函数名是否为公共导出命令：首字母小写为公共命令，首字母大写为内部辅助
///（如 `Assert-Git`、`Show-Dst*`、`Write-Dst*`）。
pub fn is_exported(name: &str) -> bool {
    name.chars()
        .next()
        .map(|c| c.is_ascii_lowercase())
        .unwrap_or(false)
}

/// 仅保留公共命令（供 UI / 帮助展示）。
pub fn filter_public_functions(functions: &[crate::ps_parser::PsFunction]) -> Vec<crate::ps_parser::PsFunction> {
    functions
        .iter()
        .filter(|f| is_exported(&f.name))
        .cloned()
        .collect()
}

fn export_names_from(cats: &[CategoryInfo], extras: &[PsFunction]) -> Vec<String> {
    let mut v: Vec<String> = cats
        .iter()
        .flat_map(|c| {
            c.functions
                .iter()
                .filter(|f| is_exported(&f.name))
                .map(|f| f.name.clone())
        })
        .chain(
            extras
                .iter()
                .filter(|f| is_exported(&f.name))
                .map(|f| f.name.clone()),
        )
        .collect();
    v.sort();
    v.dedup();
    v
}

fn write_generated_files(cats: &[CategoryInfo], extras: &[PsFunction]) -> DstResult<()> {
    let all_names = export_names_from(cats, extras);
    let psd1 = regenerate_psd1(&all_names)?;
    let psm1 = regenerate_psm1(&all_names)?;
    let help = regenerate_help_ps1(cats, extras)?;
    workspace::write_module_manifest(&psd1)?;
    workspace::write_module_loader(&psm1)?;
    workspace::write_file("Public/Help.ps1", &help)?;
    prime_category_cache_with_extras(cats, extras);
    Ok(())
}

/// 重生成所有公共部分文件。原子操作：全部成功才写盘，失败回滚不写。
/// 只启动一次 PowerShell 批量解析；完成后回填分类缓存（避免紧接着 list_categories 再扫一遍）。
pub fn regenerate_all() -> DstResult<()> {
    let (cats, extras) = scan_public_once()?;
    write_generated_files(&cats, &extras)
}

/// 用已解析结果补丁缓存并重写公共部分（零额外 PowerShell）。
/// `parsed=None` 表示文件已删除。
pub fn regenerate_with_parsed(
    file_name: &str,
    parsed: Option<ps_parser::ParsedPsFile>,
) -> DstResult<()> {
    let file_name = file_name
        .trim()
        .trim_start_matches("Public/")
        .trim_start_matches("Public\\");
    let Some((mut cats, extras)) = snapshot_for_patch() else {
        return regenerate_all();
    };

    match parsed {
        Some(p) => {
            if let Some(cat) = p.category {
                let info = CategoryInfo {
                    file_name: file_name.to_string(),
                    category: cat,
                    functions: p.functions,
                };
                if let Some(slot) = cats
                    .iter_mut()
                    .find(|c| c.file_name.eq_ignore_ascii_case(file_name))
                {
                    *slot = info;
                } else {
                    cats.push(info);
                    cats.sort_by(|a, b| a.category.name.cmp(&b.category.name));
                }
            } else {
                cats.retain(|c| !c.file_name.eq_ignore_ascii_case(file_name));
            }
        }
        None => {
            cats.retain(|c| !c.file_name.eq_ignore_ascii_case(file_name));
        }
    }

    write_generated_files(&cats, &extras)
}

/// 单文件变更后的增量再生：只解析该 .ps1，其余沿用缓存。
pub fn regenerate_after_file_change(file_name: &str) -> DstResult<()> {
    let file_name = file_name
        .trim()
        .trim_start_matches("Public/")
        .trim_start_matches("Public\\");
    let path = workspace::workspace_root().join("Public").join(file_name);
    if !path.exists() {
        return regenerate_with_parsed(file_name, None);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| {
        crate::error::DstError::Other(format!("读取 {file_name} 失败：{e}"))
    })?;
    let parsed = ps_parser::parse_ps1(&content)?;
    regenerate_with_parsed(file_name, Some(parsed))
}

const PSD1_TEMPLATE: &str = include_str!("../../templates/DevShellTools.psd1");
const PSM1_TEMPLATE: &str = include_str!("../../templates/DevShellTools.psm1");
const HELP_TEMPLATE: &str = include_str!("../../templates/Public/Help.ps1");

/// 通用占位符替换：把 `@DST-AUTOGENERATED@ tag ... @DST-AUTOGENERATED-END@` 之间内容替换为 new_body。
/// tag 用于定位具体的占位段（如 "FunctionsToExport"、"exports"）。
fn replace_block(src: &str, tag: &str, new_body: &str) -> DstResult<String> {
    let start_marker = format!("# @DST-AUTOGENERATED@ {tag}");
    let end_marker = "# @DST-AUTOGENERATED-END@";
    let start_idx = src
        .find(&start_marker)
        .ok_or_else(|| crate::error::DstError::Other(format!(
            "占位符开始标记未找到：{start_marker}"
        )))?;
    // 找开始标记之后的第一个换行
    let line_end = src[start_idx..].find('\n').map(|i| start_idx + i + 1).unwrap_or(src.len());
    let end_idx = src[line_end..]
        .find(end_marker)
        .map(|i| line_end + i)
        .ok_or_else(|| crate::error::DstError::Other(format!(
            "占位符结束标记未找到：{end_marker}"
        )))?;
    let mut out = String::with_capacity(src.len() + new_body.len());
    out.push_str(&src[..line_end]);
    out.push_str(new_body);
    out.push_str(&src[end_idx..]);
    Ok(out)
}

fn regenerate_psd1(export_names: &[String]) -> DstResult<String> {
    let mut body = String::from("    FunctionsToExport = @(\n");
    // 分组：每行最多 4 个，引号包裹
    for chunk in export_names.chunks(4) {
        let line: Vec<String> = chunk.iter().map(|n| format!("'{n}'")).collect();
        body.push_str("        ");
        body.push_str(&line.join(","));
        body.push_str(",\n");
    }
    // 移除最后多余的逗号+换行
    if body.ends_with(",\n") {
        body.truncate(body.len() - 2);
        body.push('\n');
    }
    body.push_str("    )\n");
    replace_block(PSD1_TEMPLATE, "FunctionsToExport", &body)
}

fn regenerate_psm1(export_names: &[String]) -> DstResult<String> {
    let mut body = String::from("$exports = @(\n");
    for chunk in export_names.chunks(4) {
        let line: Vec<String> = chunk.iter().map(|n| format!("\"{n}\"")).collect();
        body.push_str("    ");
        body.push_str(&line.join(","));
        body.push_str(",\n");
    }
    if body.ends_with(",\n") {
        body.truncate(body.len() - 2);
        body.push('\n');
    }
    body.push_str(")\n");
    replace_block(PSM1_TEMPLATE, "exports", &body)
}

fn regenerate_help_ps1(cats: &[CategoryInfo], _extras: &[PsFunction]) -> DstResult<String> {
    let mut out = HELP_TEMPLATE.to_string();

    // 1. CategoryMeta
    out = replace_block(&out, "CategoryMeta", &gen_category_meta(cats))?;
    // 2. HelpData
    out = replace_block(&out, "HelpData", &gen_help_data(cats))?;
    // 3. CategoryValidateSet (Show-DstCategoryCommands 的参数)
    out = replace_block(&out, "CategoryValidateSet", &gen_validate_set(cats, false))?;
    // 4. ActionValidateSet (dsh 的 $Action)
    out = replace_block(&out, "ActionValidateSet", &gen_validate_set(cats, true))?;
    // 5. CategoryArgValidateSet (dsh 的 $Category)
    out = replace_block(
        &out,
        "CategoryArgValidateSet",
        &gen_validate_set_with_all(cats),
    )?;

    Ok(out)
}

fn gen_category_meta(cats: &[CategoryInfo]) -> String {
    let mut out = String::from("$script:DstCategoryMeta = [ordered]@{\n");
    for (i, c) in cats.iter().enumerate() {
        let aliases = if c.category.aliases.is_empty() {
            "@()".to_string()
        } else {
            let parts: Vec<String> = c
                .category
                .aliases
                .iter()
                .map(|a| quote_data(a))
                .collect();
            format!("@({})", parts.join(","))
        };
        let category_name = quote_data(&c.category.name);
        let title = quote_data(&c.category.title);
        let description = quote_data(&c.category.description);
        let example = quote_data(&format!("dsh {}", c.category.name));
        // 编号与 Keys 顺序一致；菜单/帮助必须按 编号 解析，禁止硬编码 1=files 等映射
        out.push_str(&format!(
            "    {} = [PSCustomObject]@{{\n        编号 = {}\n        分类 = {}\n        说明 = {}\n        示例 = {}\n        别名 = {}\n    }}\n",
            category_name,
            i + 1,
            title,
            description,
            example,
            aliases
        ));
    }
    out.push_str("}\n");
    out
}

fn gen_help_data(cats: &[CategoryInfo]) -> String {
    let mut out = String::from("$script:DstHelpData = @{\n");
    for c in cats {
        out.push_str(&format!("    {} = @(\n", quote_data(&c.category.name)));
        let exported: Vec<_> = c
            .functions
            .iter()
            .filter(|f| is_exported(&f.name))
            .collect();
        for f in &exported {
            // 命令展示：若是 "set/test/show" 这种带空格的（lpr 子命令），保留；否则用函数名
            let cmd_display = &f.name;
            let synopsis = if f.synopsis.is_empty() {
                "(无说明)"
            } else {
                &f.synopsis
            };
            let example = if f.first_example.is_empty() {
                &f.name
            } else {
                &f.first_example
            };
            // 每条前加一元逗号 `,@(...)`，且条目之间不加分隔逗号：
            // - 无一元逗号：单元素分类（如仅 lpr）会被拍平成字符串 → $entry[0] 变单字符
            // - 一元逗号 + 行尾分隔逗号：会多包一层 → $entry[1] 越界
            out.push_str(&format!(
                "        ,@({},{},{})\n",
                quote_data(cmd_display),
                quote_data(synopsis),
                quote_data(example)
            ));
        }
        out.push_str("    )\n");
    }
    out.push_str("}\n");
    out
}

fn gen_validate_set(cats: &[CategoryInfo], for_action: bool) -> String {
    let mut names: Vec<String> = cats.iter().map(|c| c.category.name.clone()).collect();
    if for_action {
        let mut base = vec!["menu".into(), "list".into(), "help".into(), "version".into()];
        base.append(&mut names);
        names = base;
    }
    let quoted: Vec<String> = names.iter().map(|n| quote_data(n)).collect();
    format!("[ValidateSet({})]\n", quoted.join(","))
}

fn gen_validate_set_with_all(cats: &[CategoryInfo]) -> String {
    let mut names: Vec<String> = cats.iter().map(|c| c.category.name.clone()).collect();
    let mut base = vec!["all".into()];
    base.append(&mut names);
    names = base;
    let quoted: Vec<String> = names.iter().map(|n| quote_data(n)).collect();
    format!("[ValidateSet({})]\n", quoted.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regenerate_from_template_matches_baseline() {
        // 用内嵌模板作为工作区源，扫描后重生成应产出与模板占位符段等价的内容。
        // 这里只验证不 panic 且产出含预期关键字。
        let files = vec![
            ("Files.ps1", include_str!("../../templates/Public/Files.ps1")),
            ("Git.ps1", include_str!("../../templates/Public/Git.ps1")),
            ("Help.ps1", include_str!("../../templates/Public/Help.ps1")),
            ("Network.ps1", include_str!("../../templates/Public/Network.ps1")),
            ("PowerShell.ps1", include_str!("../../templates/Public/PowerShell.ps1")),
            ("Proxy.ps1", include_str!("../../templates/Public/Proxy.ps1")),
        ];
        let mut cats = vec![];
        let mut extras = vec![];
        for (name, code) in &files {
            let parsed = ps_parser::parse_ps1(code).expect("parse");
            if let Some(cat) = parsed.category {
                cats.push(CategoryInfo {
                    file_name: name.to_string(),
                    category: cat,
                    functions: parsed.functions,
                });
            } else {
                extras.extend(parsed.functions);
            }
        }
        cats.sort_by(|a, b| a.category.name.cmp(&b.category.name));

        let psd1 = regenerate_psd1(&all_names_from(&cats, &extras)).unwrap();
        assert!(psd1.contains("'lt'"));
        assert!(psd1.contains("'gg'"));
        assert!(psd1.contains("'dsh'"));
        assert!(psd1.contains("FunctionsToExport = @("));

        let psm1 = regenerate_psm1(&all_names_from(&cats, &extras)).unwrap();
        assert!(psm1.contains("\"lt\""));
        assert!(psm1.contains("\"dsh\""));

        let help = regenerate_help_ps1(&cats, &extras).unwrap();
        assert!(help.contains("'files' = [PSCustomObject]"));
        assert!(help.contains("'git' = [PSCustomObject]"));
        assert!(help.contains("[ValidateSet('menu','list','help','version','files','git','network','powershell','proxy')]"));
        assert!(help.contains("$script:DstHelpData = @{"));
        assert!(
            help.contains("别名 = "),
            "CategoryMeta 应含别名字段，供菜单按别名解析"
        );
        assert!(
            help.contains("Resolve-DstCategoryKey"),
            "Help.ps1 须含动态分类解析，禁止硬编码编号映射"
        );
        assert!(
            !help.contains("\"1\" { Show-DstCategoryCommands"),
            "禁止硬编码菜单编号（如 1=files），新增分类后会与 CategoryMeta 编号错位"
        );
        // 回归：HelpData 须用一元逗号包裹嵌套数组，避免 PS 拍平导致 dsh <分类> 错位
        assert!(
            help.contains(",@('lt',") && help.contains(",@('lpr',"),
            "HelpData 条目应形如 ,@('name',...)，实际 lt 行: {}",
            help.lines().find(|l| l.contains("@('lt'")).unwrap_or("(no lt line)")
        );
        // gg 函数应出现在 git 分类的帮助数据中（synopsis 来自 AST，含"图形化"关键字）
        assert!(help.contains("@('gg','显示图形化精简提交历史，默认显示 20 条。','gg')"));
        // 内部辅助函数不应进入帮助数据
        assert!(!help.contains("Assert-Git"));
    }

    fn all_names_from(cats: &[CategoryInfo], extras: &[PsFunction]) -> Vec<String> {
        let mut v: Vec<String> = cats
            .iter()
            .flat_map(|c| c.functions.iter().filter(|f| is_exported(&f.name)).map(|f| f.name.clone()))
            .chain(extras.iter().filter(|f| is_exported(&f.name)).map(|f| f.name.clone()))
            .collect();
        v.sort();
        v.dedup();
        v
    }

    #[test]
    fn replace_block_basic() {
        let src = "before\n# @DST-AUTOGENERATED@ foo\nOLD\n# @DST-AUTOGENERATED-END@\nafter";
        let out = replace_block(src, "foo", "NEW\n").expect("replace");
        assert!(out.contains("NEW"));
        assert!(!out.contains("OLD"));
        assert!(out.contains("after"));
    }

    #[test]
    fn replace_block_returns_error_when_marker_is_missing() {
        assert!(replace_block("function a {}", "HelpData", "x").is_err());
    }

    #[test]
    fn metadata_injection_is_rendered_as_inert_data() {
        let cats = vec![CategoryInfo {
            file_name: "Evil.ps1".into(),
            category: CategoryMeta {
                name: "evil".into(),
                title: "$(Write-Output DST_INJECTED)".into(),
                description: "x'; Remove-Item owned".into(),
                aliases: vec!["a'; b".into(), "`whoami`".into()],
            },
            functions: vec![],
        }];

        let help = regenerate_help_ps1(&cats, &[]).expect("help");
        assert!(help.contains("分类 = '$(Write-Output DST_INJECTED)'"));
        assert!(!help_ast_has_command_containing(&help, "DST_INJECTED"));
    }

    fn help_ast_has_command_containing(help: &str, needle: &str) -> bool {
        let tmp = std::env::temp_dir().join(format!(
            "dst-help-ast-{}-{}.ps1",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::write(&tmp, help).expect("write help fixture");
        let probe = tmp.with_extension("probe.ps1");
        let probe_source = r#"param([string]$HelpPath, [string]$Needle)
$tokens = $null
$errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    $HelpPath,
    [ref]$tokens,
    [ref]$errors
)
$commands = @($ast.FindAll(
    { param($node) $node -is [System.Management.Automation.Language.CommandAst] },
    $true
))
if (@($commands | Where-Object { $_.Extent.Text.Contains($Needle) }).Count -gt 0) {
    exit 1
}
"#;
        std::fs::write(&probe, probe_source).expect("write parser probe");
        let status = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&probe)
            .arg(&tmp)
            .arg(needle)
            .status()
            .expect("powershell");
        let _ = std::fs::remove_file(tmp);
        let _ = std::fs::remove_file(probe);
        !status.success()
    }
}
