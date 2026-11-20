use crate::error::DstResult;
use crate::sync;
use crate::workspace;

/// 判断函数名是否应导出：首字母小写为公共命令，首字母大写为内部辅助。
fn is_exported_name(name: &str) -> bool {
    name.chars()
        .next()
        .map(|c| c.is_lowercase())
        .unwrap_or(false)
}

/// 一致性校验结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ConsistencyReport {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// 实际从 Public/*.ps1 扫描到的所有函数名
    pub actual_functions: Vec<String>,
    /// .psd1 声明的 FunctionsToExport
    pub psd1_exports: Vec<String>,
    /// .psm1 声明的 exports
    pub psm1_exports: Vec<String>,
    /// Help.ps1 中出现过的命令名
    pub help_commands: Vec<String>,
}

/// 执行三方一致性校验：
/// 1. Public/*.ps1 实际函数名（AST 扫描）
/// 2. .psd1 FunctionsToExport
/// 3. .psm1 exports
/// 4. Help.ps1 HelpData 中的命令
///
/// 四者应完全一致（除 dsh/Show-* 等非分类函数）。
pub fn check() -> DstResult<ConsistencyReport> {
    let mut errors = vec![];
    let mut warnings = vec![];

    // 1. 扫描实际函数（只取应导出的：小写开头）
    // 优先走缓存，避免与 list_categories 并发时重复启动 powershell。
    let cats = sync::scan_categories_cached()?;
    let extras = sync::scan_extra_functions()?;
    let actual: Vec<String> = cats
        .iter()
        .flat_map(|c| {
            c.functions
                .iter()
                .filter(|f| is_exported_name(&f.name))
                .map(|f| f.name.clone())
        })
        .chain(
            extras
                .iter()
                .filter(|f| is_exported_name(&f.name))
                .map(|f| f.name.clone()),
        )
        .collect::<Vec<_>>()
        .into_iter()
        .collect();
    let mut actual_sorted = actual.clone();
    actual_sorted.sort();
    actual_sorted.dedup();

    // 2. 解析 .psd1 的 FunctionsToExport
    let psd1 = workspace::read_module_manifest()?;
    let psd1_exports = extract_ps_string_array(&psd1, "FunctionsToExport");
    let mut psd1_sorted = psd1_exports.clone();
    psd1_sorted.sort();
    psd1_sorted.dedup();

    // 3. 解析 .psm1 的 exports
    let psm1 = workspace::read_file("DevShellTools.psm1")?;
    let psm1_exports = extract_ps_string_array(&psm1, "exports");
    let mut psm1_sorted = psm1_exports.clone();
    psm1_sorted.sort();
    psm1_sorted.dedup();

    // 4. 解析 Help.ps1 的 HelpData 命令名
    let help = workspace::read_file("Public/Help.ps1")?;
    let help_commands = extract_help_data_commands(&help);
    let mut help_sorted = help_commands.clone();
    help_sorted.sort();
    help_sorted.dedup();

    // 比较：实际 vs psd1
    if actual_sorted != psd1_sorted {
        let missing_in_psd1: Vec<_> = actual_sorted
            .iter()
            .filter(|n| !psd1_sorted.contains(n))
            .collect();
        let extra_in_psd1: Vec<_> = psd1_sorted
            .iter()
            .filter(|n| !actual_sorted.contains(n))
            .collect();
        if !missing_in_psd1.is_empty() {
            errors.push(format!(
                "psd1 缺少导出：{}",
                missing_in_psd1
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !extra_in_psd1.is_empty() {
            errors.push(format!(
                "psd1 多余导出：{}",
                extra_in_psd1
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    // 比较：实际 vs psm1
    if actual_sorted != psm1_sorted {
        let missing_in_psm1: Vec<_> = actual_sorted
            .iter()
            .filter(|n| !psm1_sorted.contains(n))
            .collect();
        let extra_in_psm1: Vec<_> = psm1_sorted
            .iter()
            .filter(|n| !actual_sorted.contains(n))
            .collect();
        if !missing_in_psm1.is_empty() {
            errors.push(format!(
                "psm1 缺少导出：{}",
                missing_in_psm1
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !extra_in_psm1.is_empty() {
            errors.push(format!(
                "psm1 多余导出：{}",
                extra_in_psm1
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    // 比较：实际 vs help（help 只含公共命令；内部辅助如 Assert-Git 不要求进帮助）
    let category_functions: Vec<String> = cats
        .iter()
        .flat_map(|c| {
            c.functions
                .iter()
                .filter(|f| is_exported_name(&f.name))
                .map(|f| f.name.clone())
        })
        .collect();
    for f in &category_functions {
        if !help_commands.contains(f) {
            warnings.push(format!("Help.ps1 缺少命令说明：{f}"));
        }
    }

    // 检查每个公共命令是否有 synopsis（内部辅助函数不要求）
    for c in &cats {
        for f in c.functions.iter().filter(|f| is_exported_name(&f.name)) {
            if f.synopsis.is_empty() {
                warnings.push(format!(
                    "函数 {}.{} 缺少 .SYNOPSIS",
                    c.category.name, f.name
                ));
            }
        }
    }

    let ok = errors.is_empty();
    Ok(ConsistencyReport {
        ok,
        errors,
        warnings,
        actual_functions: actual_sorted,
        psd1_exports: psd1_sorted,
        psm1_exports: psm1_sorted,
        help_commands: help_sorted,
    })
}

/// 从 .psd1/.psm1 文本中提取 `var_name = @(...)` 里的字符串（支持单引号和双引号）。
fn extract_ps_string_array(src: &str, var_name: &str) -> Vec<String> {
    let mut out = vec![];
    let pattern = format!("{var_name} = @(");
    if let Some(start) = src.find(&pattern) {
        let after_start = start + pattern.len();
        if let Some(end) = src[after_start..].find(')') {
            let body = &src[after_start..after_start + end];
            let mut chars = body.chars().peekable();
            while let Some(&c) = chars.peek() {
                if c == '\'' || c == '"' {
                    let quote = c;
                    chars.next();
                    let mut s = String::new();
                    while let Some(&c2) = chars.peek() {
                        if c2 == quote {
                            chars.next();
                            // PS 转义：单引号内用 ''，双引号内用 `"（反引号）
                            if chars.peek() == Some(&quote) {
                                s.push(quote);
                                chars.next();
                                continue;
                            }
                            break;
                        }
                        s.push(c2);
                        chars.next();
                    }
                    let trimmed = s.trim().to_string();
                    if !trimmed.is_empty() {
                        out.push(trimmed);
                    }
                } else {
                    chars.next();
                }
            }
        }
    }
    out
}

/// 从 Help.ps1 的 $DstHelpData 中提取所有命令名（每条 `@("cmd","...","...")` 的第一个字段）。
fn extract_help_data_commands(src: &str) -> Vec<String> {
    let mut out = vec![];
    // 匹配 `@("`...`"` 的模式（命令名字段）
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("@(\"") {
            if let Some(end) = rest.find("\",") {
                let name = &rest[..end];
                let unescaped = name.replace("`\"", "\"");
                out.push(unescaped);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_string_array_works() {
        let src = "FunctionsToExport = @(\n  'lt',\n  'gg',\n  'dsh'\n)";
        let v = extract_ps_string_array(src, "FunctionsToExport");
        assert_eq!(v, vec!["lt", "gg", "dsh"]);
    }

    #[test]
    fn extract_string_array_empty() {
        let src = "FunctionsToExport = @()";
        let v = extract_ps_string_array(src, "FunctionsToExport");
        assert!(v.is_empty());
    }

    #[test]
    fn extract_help_commands_works() {
        let src = r#"
files = @(
    @("lt","最近修改项目","lt -10"),
    @("sz","统计大小","sz .")
)
"#;
        let v = extract_help_data_commands(src);
        assert_eq!(v, vec!["lt", "sz"]);
    }
}
