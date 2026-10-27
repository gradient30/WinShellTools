use crate::error::{DstError, DstResult};
use crate::process_util::{output_hidden, ps_base_args};
use crate::{ps_parser, safety, sync, workspace};
use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FunctionDraft {
    pub name: String,
    pub synopsis: String,
    pub example: String,
    pub body: Option<String>,
    /// 参数名 → 新默认值源码（如 "10"）；仅更新已有默认值的参数
    #[serde(default)]
    pub param_defaults: Option<std::collections::HashMap<String, String>>,
    /// 除首条外的 .EXAMPLE（编辑时从原帮助保留，避免静默丢失）
    #[serde(default)]
    pub extra_examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionTestResult {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

fn ps_exe() -> DstResult<&'static str> {
    let mut cmd = Command::new("pwsh");
    cmd.arg("--version");
    if crate::process_util::output_hidden_ref(&mut cmd).is_ok() {
        Ok("pwsh")
    } else {
        Ok("powershell")
    }
}

fn run_ps_script(script: &str) -> DstResult<String> {
    let exe = ps_exe()?;
    let mut cmd = Command::new(exe);
    for arg in ps_base_args(exe) {
        cmd.arg(arg);
    }
    cmd.args(["-Command", script]);
    let output = output_hidden(cmd).map_err(|e| DstError::Other(format!("启动 PowerShell 失败：{e}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        return Err(DstError::Other(format!(
            "PowerShell 失败：{}\n{}",
            err.trim(),
            out.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn default_body(name: &str) -> String {
    format!(
        r#"    [CmdletBinding()] param()
    Write-DstInfo "TODO: 实现 {name}""#
    )
}

/// 提取函数体开头 `<# ... #>` 注释帮助的内部文本（不含定界符）。
pub fn extract_leading_help_inner(body: &str) -> Option<String> {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len()
        && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\r' || bytes[i] == b'\n')
    {
        i += 1;
    }
    if i + 2 > bytes.len() || &bytes[i..i + 2] != b"<#" {
        return None;
    }
    let rel = body[i..].find("#>")?;
    let inner = &body[i + 2..i + rel];
    Some(inner.to_string())
}

/// 去掉函数体开头的注释帮助块，避免 upsert 时重复包裹 `<# ... #>`。
pub fn strip_leading_comment_help(body: &str) -> String {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len()
        && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\r' || bytes[i] == b'\n')
    {
        i += 1;
    }
    if i + 2 <= bytes.len() && &bytes[i..i + 2] == b"<#" {
        if let Some(rel) = body[i..].find("#>") {
            let after = i + rel + 2;
            let rest = body[after..].trim_start_matches(['\r', '\n']);
            return rest.to_string();
        }
    }
    body.to_string()
}

/// 从注释帮助中按 `.EXAMPLE` 分段提取示例首行（与 ps_parser 一致）。
pub fn examples_from_help_inner(help_inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in help_inner.lines() {
        let t = line.trim();
        if t.eq_ignore_ascii_case(".EXAMPLE") {
            if let Some(ex) = current.take() {
                if !ex.is_empty() {
                    out.push(ex);
                }
            }
            current = Some(String::new());
            continue;
        }
        if let Some(ref mut buf) = current {
            if buf.is_empty() {
                if !t.is_empty()
                    && !t.starts_with('.')
                {
                    *buf = t.to_string();
                }
            }
        }
    }
    if let Some(ex) = current {
        if !ex.is_empty() {
            out.push(ex);
        }
    }
    out
}

fn format_example_help_lines(first: &str, extras: &[String]) -> String {
    let mut s = format!(".EXAMPLE\n{}", first.trim());
    for ex in extras {
        let t = ex.trim();
        if t.is_empty() {
            continue;
        }
        s.push_str(&format!("\n.EXAMPLE\n{t}"));
    }
    s
}

/// 生成完整函数块文本。
pub fn build_function_block(draft: &FunctionDraft) -> String {
    let raw_body = draft
        .body
        .clone()
        .unwrap_or_else(|| default_body(&draft.name));
    let body = strip_leading_comment_help(&raw_body);
    let example = draft.example.trim();
    let example_help = format_example_help_lines(example, &draft.extra_examples);
    format!(
        r#"function {name} {{
<#
.SYNOPSIS
{synopsis}
{example_help}
#>
{body}
}}
"#,
        name = draft.name,
        synopsis = draft.synopsis,
        example_help = example_help,
        body = body.trim_end()
    )
}

fn assert_safety_ok(report: crate::safety::SafetyReport) -> DstResult<()> {
    if !report.ok {
        return Err(DstError::SafetyBlocked(report.violations.join("; ")));
    }
    Ok(())
}

fn validate_public_fn_name(name: &str) -> DstResult<()> {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        Some(c) if c.is_ascii_uppercase() => {
            return Err(DstError::Other(format!(
                "「{name}」是内部辅助函数（大写开头），不能在命令列表中编辑/删除/测试；请只管理小写公共命令"
            )));
        }
        _ => return Err(DstError::Other("函数名须以小写字母开头".into())),
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
        return Err(DstError::Other("函数名仅允许小写字母与数字".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_function_block, examples_from_help_inner, extract_leading_help_inner,
        patch_param_default, strip_leading_comment_help, FunctionDraft,
    };

    #[test]
    fn patch_int_default() {
        let body = r#"    param(
        [int]$Count = 20
    )
    Assert-Git"#;
        let out = patch_param_default(body, "Count", "10");
        assert!(out.contains("$Count = 10"), "{out}");
        assert!(!out.contains("$Count = 20"));
    }

    #[test]
    fn strip_help_keeps_param() {
        let body = r#"<#
.SYNOPSIS
显示历史
.EXAMPLE
gg
#>
    [CmdletBinding()]
    param(
        [int]$Count = 20
    )
    Assert-Git"#;
        let out = strip_leading_comment_help(body);
        assert!(!out.contains(".SYNOPSIS"), "{out}");
        assert!(out.contains("$Count = 20"), "{out}");
    }

    #[test]
    fn examples_from_help_keeps_all() {
        let help = r#"
.SYNOPSIS
显示历史
.EXAMPLE
gg
.EXAMPLE
gg 5
"#;
        let ex = examples_from_help_inner(help);
        assert_eq!(ex, vec!["gg".to_string(), "gg 5".to_string()]);
    }

    #[test]
    fn build_preserves_extra_examples() {
        let draft = FunctionDraft {
            name: "gg".into(),
            synopsis: "显示历史".into(),
            example: "gg".into(),
            body: Some(
                r#"    [CmdletBinding()]
    param([int]$Count = 10)
    Assert-Git"#
                    .into(),
            ),
            param_defaults: None,
            extra_examples: vec!["gg 5".into()],
        };
        let block = build_function_block(&draft);
        assert!(block.contains(".EXAMPLE\ngg\n.EXAMPLE\ngg 5"), "{block}");
        assert!(block.contains("$Count = 10"), "{block}");
        assert!(extract_leading_help_inner(&format!("<#\n.SYNOPSIS\nx\n#>\nbody")).is_some());
    }

    #[test]
    fn acknowledge_danger_allows_hard_reset_in_block() {
        let draft = FunctionDraft {
            name: "glf".into(),
            synopsis: "危险".into(),
            example: "glf".into(),
            body: Some("    & git reset --hard origin/main".into()),
            param_defaults: None,
            extra_examples: vec![],
        };
        let block = build_function_block(&draft);
        let blocked = crate::safety::check(&block).unwrap();
        assert!(!blocked.ok);
        let ok = crate::safety::check_with_options(&block, true).unwrap();
        assert!(ok.ok);
    }
}

fn category_rel(file_name: &str) -> DstResult<String> {
    let name = file_name
        .trim()
        .trim_start_matches("Public/")
        .trim_start_matches("Public\\");
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err(DstError::Other("分类文件名非法".into()));
    }
    if !name.ends_with(".ps1") {
        return Err(DstError::Other("文件名必须以 .ps1 结尾".into()));
    }
    Ok(format!("Public/{name}"))
}

fn write_temp_ps1(content: &str) -> DstResult<std::path::PathBuf> {
    let mut tmp = std::env::temp_dir();
    tmp.push(format!(
        "dst-fn-{}-{}.ps1",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&[0xEF, 0xBB, 0xBF])?;
        f.write_all(content.as_bytes())?;
    }
    Ok(tmp)
}

/// 从现有函数提取函数体（ScriptBlock 内部，不含外层 function 包装）。
fn extract_existing_body(file_path: &std::path::Path, fn_name: &str) -> DstResult<Option<String>> {
    let path_escaped = file_path.to_string_lossy().replace('\'', "''");
    let name_escaped = fn_name.replace('\'', "''");
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
$raw = Get-Content -LiteralPath '{path_escaped}' -Raw -Encoding UTF8
$errors = $null; $tokens = $null
$ast = [System.Management.Automation.Language.Parser]::ParseInput($raw, [ref]$tokens, [ref]$errors)
$fnAsts = $ast.FindAll({{ param($n,$d) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq '{name_escaped}' }}, $true)
if (-not $fnAsts -or $fnAsts.Count -eq 0) {{ '' ; exit 0 }}
$inner = $fnAsts[0].Body.Extent.Text.Trim()
if ($inner.StartsWith('{{') -and $inner.EndsWith('}}')) {{
    $inner = $inner.Substring(1, $inner.Length - 2).TrimEnd()
    if ($inner.StartsWith("`r`n")) {{ $inner = $inner.Substring(2) }}
    elseif ($inner.StartsWith("`n")) {{ $inner = $inner.Substring(1) }}
}}
$inner
"#
    );
    let out = run_ps_script(&script)?;
    let body = out.trim_end().to_string();
    if body.is_empty() {
        Ok(None)
    } else {
        Ok(Some(body))
    }
}

/// 替换函数体中 `$Name = <旧值>` 的默认值（仅首处匹配）。
pub fn patch_param_default(body: &str, name: &str, new_val: &str) -> String {
    let marker = format!("${name}");
    let bytes = body.as_bytes();
    let marker_bytes = marker.as_bytes();
    let mut i = 0;
    while i + marker_bytes.len() <= bytes.len() {
        if &bytes[i..i + marker_bytes.len()] == marker_bytes {
            let after_name = i + marker_bytes.len();
            let mut j = after_name;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'=' {
                j += 1;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                    j += 1;
                }
                let val_start = j;
                while j < bytes.len() {
                    let c = bytes[j];
                    if c == b',' || c == b')' || c == b'\r' || c == b'\n' {
                        break;
                    }
                    j += 1;
                }
                if val_start < j {
                    let mut out = String::with_capacity(body.len());
                    out.push_str(&body[..val_start]);
                    out.push_str(new_val.trim());
                    out.push_str(&body[j..]);
                    return out;
                }
            }
        }
        i += 1;
    }
    body.to_string()
}

/// 插入或更新分类文件中的函数，并重生成公共部分。
pub fn upsert_function(
    file_name: &str,
    mut draft: FunctionDraft,
    acknowledge_danger: bool,
) -> DstResult<()> {
    let rel = category_rel(file_name)?;
    validate_public_fn_name(&draft.name)?;
    let file_path = workspace::workspace_root().join(&rel);
    if !file_path.exists() {
        return Err(DstError::FileNotFound(rel));
    }

    // 编辑已有命令且未显式传 body：保留原函数体 / 额外 EXAMPLE，并应用参数默认值补丁
    if draft.body.is_none() {
        if let Some(mut body) = extract_existing_body(&file_path, &draft.name)? {
            if draft.extra_examples.is_empty() {
                if let Some(help) = extract_leading_help_inner(&body) {
                    let all = examples_from_help_inner(&help);
                    if all.len() > 1 {
                        draft.extra_examples = all.into_iter().skip(1).collect();
                    }
                }
            }
            if let Some(defaults) = draft.param_defaults.take() {
                for (pname, pval) in defaults {
                    if pval.trim().is_empty() {
                        continue;
                    }
                    body = patch_param_default(&body, &pname, &pval);
                }
            }
            draft.body = Some(body);
        }
    } else if let Some(defaults) = draft.param_defaults.take() {
        if let Some(body) = draft.body.as_mut() {
            for (pname, pval) in defaults {
                if pval.trim().is_empty() {
                    continue;
                }
                *body = patch_param_default(body, &pname, &pval);
            }
        }
    }

    let block = build_function_block(&draft);
    assert_safety_ok(safety::check_with_options(&block, acknowledge_danger)?)?;

    let block_tmp = write_temp_ps1(&block)?;
    let path_escaped = file_path.to_string_lossy().replace('\'', "''");
    let block_path_escaped = block_tmp.to_string_lossy().replace('\'', "''");
    let name_escaped = draft.name.replace('\'', "''");

    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
$path = '{path_escaped}'
$raw = Get-Content -LiteralPath $path -Raw -Encoding UTF8
$newBlock = Get-Content -LiteralPath '{block_path_escaped}' -Raw -Encoding UTF8
$errors = $null; $tokens = $null
$ast = [System.Management.Automation.Language.Parser]::ParseInput($raw, [ref]$tokens, [ref]$errors)
if ($errors -and $errors.Count -gt 0) {{ throw ($errors | ForEach-Object {{ $_.Message }}) -join '; ' }}
$fnName = '{name_escaped}'
$fnAsts = $ast.FindAll({{ param($n,$d) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $fnName }}, $true)
if ($fnAsts -and $fnAsts.Count -gt 0) {{
    $start = $fnAsts[0].Extent.StartOffset
    $end = $fnAsts[0].Extent.EndOffset
    $raw = $raw.Substring(0, $start) + $newBlock + $raw.Substring($end)
}} else {{
    if (-not $raw.EndsWith("`n")) {{ $raw += "`n" }}
    $raw += "`n" + $newBlock
}}
$enc = New-Object System.Text.UTF8Encoding $true
[System.IO.File]::WriteAllText($path, $raw, $enc)
"#,
    );

    run_ps_script(&script)?;
    let _ = std::fs::remove_file(&block_tmp);

    // 仅校验本次写入的函数块；同文件其它命令（如 AI /danger 插入的 glf）不应阻断 gg 等正常编辑。
    let content = workspace::read_file(&rel)?;
    let parsed = ps_parser::parse_ps1(&content)?;
    sync::regenerate_with_parsed(file_name, Some(parsed))?;
    workspace::touch_last_sync()?;
    crate::install_mgr::spawn_sync_runtime_modules();
    Ok(())
}

/// 从分类文件删除函数。
pub fn delete_function(file_name: &str, func_name: &str) -> DstResult<()> {
    let rel = category_rel(file_name)?;
    validate_public_fn_name(func_name)?;
    let file_path = workspace::workspace_root().join(&rel);
    if !file_path.exists() {
        return Err(DstError::FileNotFound(rel));
    }
    let path_escaped = file_path.to_string_lossy().replace('\'', "''");
    let name_escaped = func_name.replace('\'', "''");
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
$path = '{path_escaped}'
$raw = Get-Content -LiteralPath $path -Raw -Encoding UTF8
$errors = $null; $tokens = $null
$ast = [System.Management.Automation.Language.Parser]::ParseInput($raw, [ref]$tokens, [ref]$errors)
$fnAsts = $ast.FindAll({{ param($n,$d) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq '{name_escaped}' }}, $true)
if (-not $fnAsts -or $fnAsts.Count -eq 0) {{ throw "函数不存在：{name_escaped}" }}
$start = $fnAsts[0].Extent.StartOffset
$end = $fnAsts[0].Extent.EndOffset
$raw = $raw.Substring(0, $start) + $raw.Substring($end)
$raw = ($raw -replace "(\r?\n){{3,}}", "`n`n").TrimEnd() + "`n"
$enc = New-Object System.Text.UTF8Encoding $true
[System.IO.File]::WriteAllText($path, $raw, $enc)
"#,
    );
    run_ps_script(&script)?;
    let content = workspace::read_file(&rel)?;
    let parsed = ps_parser::parse_ps1(&content)?;
    sync::regenerate_with_parsed(file_name, Some(parsed))?;
    workspace::touch_last_sync()?;
    crate::install_mgr::spawn_sync_runtime_modules();
    Ok(())
}

/// 在隔离进程中测试函数（dot-source + 执行 example）。
pub fn test_function(file_name: &str, func_name: &str) -> DstResult<FunctionTestResult> {
    let rel = category_rel(file_name)?;
    validate_public_fn_name(func_name)?;
    let content = workspace::read_file(&rel)?;
    let parsed = ps_parser::parse_ps1(&content)?;
    let func = parsed
        .functions
        .iter()
        .find(|f| f.name == func_name)
        .ok_or_else(|| DstError::Other(format!("函数不存在：{func_name}")))?;
    let example = if func.first_example.trim().is_empty() {
        func_name.to_string()
    } else {
        func.first_example.clone()
    };
    assert_safety_ok(safety::check(&example)?)?;

    let common = workspace::read_file("Private/Common.ps1")?;
    let common_tmp = write_temp_ps1(&common)?;
    let content_tmp = write_temp_ps1(&content)?;
    let common_path = common_tmp.to_string_lossy().replace('\'', "''");
    let content_path = content_tmp.to_string_lossy().replace('\'', "''");
    let example_escaped = example.replace('\'', "''");

    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
. '{common_path}'
. '{content_path}'
$result = Invoke-Expression '{example_escaped}' 2>&1 | Out-String
Write-Output $result
"#,
    );

    let result = match run_ps_script(&script) {
        Ok(stdout) => FunctionTestResult {
            ok: true,
            stdout,
            stderr: String::new(),
        },
        Err(e) => FunctionTestResult {
            ok: false,
            stdout: String::new(),
            stderr: e.to_string(),
        },
    };
    let _ = std::fs::remove_file(&common_tmp);
    let _ = std::fs::remove_file(&content_tmp);
    Ok(result)
}

/// 将 AI 生成的代码块合并到指定分类（按函数 AST 合并）。
pub fn apply_code_to_category(
    file_name: &str,
    code: &str,
) -> DstResult<Vec<String>> {
    apply_code_to_category_with_options(file_name, code, false)
}

pub fn apply_code_to_category_with_options(
    file_name: &str,
    code: &str,
    danger_mode: bool,
) -> DstResult<Vec<String>> {
    let report = safety::check_with_options(code, danger_mode)?;
    if !report.ok {
        return Err(DstError::SafetyBlocked(report.violations.join("; ")));
    }
    // parse_ps1 已含语法错误检查，无需再 validate_syntax
    let parsed = ps_parser::parse_ps1(code)?;
    // 返回值只统计公共命令（小写开头）——它们才是用户可敲的命令；
    // 大写辅助函数同样会写入文件，但不作为"插入的命令"汇报
    let names: Vec<String> = parsed
        .functions
        .iter()
        .filter(|f| f.name.chars().next().map(|c| c.is_lowercase()).unwrap_or(false))
        .map(|f| f.name.clone())
        .collect();
    if names.is_empty() {
        return Err(DstError::Other("代码块中无可导出的公共函数".into()));
    }

    let rel = category_rel(file_name)?;
    let target = workspace::workspace_root().join(&rel);
    if !target.exists() {
        return Err(DstError::FileNotFound(rel));
    }

    let mut tmp = std::env::temp_dir();
    tmp.push(format!(
        "dst-merge-{}-{}.ps1",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&[0xEF, 0xBB, 0xBF])?;
        f.write_all(code.as_bytes())?;
    }

    let target_esc = target.to_string_lossy().replace('\'', "''");
    let source_esc = tmp.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
$targetPath = '{target_esc}'
$sourcePath = '{source_esc}'
$targetRaw = Get-Content -LiteralPath $targetPath -Raw -Encoding UTF8
$sourceRaw = Get-Content -LiteralPath $sourcePath -Raw -Encoding UTF8
$te=$null;$tt=$null; $targetAst = [System.Management.Automation.Language.Parser]::ParseInput($targetRaw, [ref]$tt, [ref]$te)
$se=$null;$st=$null; $sourceAst = [System.Management.Automation.Language.Parser]::ParseInput($sourceRaw, [ref]$st, [ref]$se)
$srcFns = $sourceAst.FindAll({{ param($n,$d) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] }}, $true)
foreach ($fn in $srcFns) {{
    # 全部函数一并写入（含大写内部辅助，如 Assert-Xxx）：公共命令可能依赖同块辅助函数，
    # 丢弃会导致插入的命令运行时报错；导出列表由 sync 按小写首字母自动筛选
    $text = $fn.Extent.Text
    $existing = $targetAst.FindAll({{ param($n,$d) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $fn.Name }}, $true)
    if ($existing -and $existing.Count -gt 0) {{
        $start = $existing[0].Extent.StartOffset
        $end = $existing[0].Extent.EndOffset
        $targetRaw = $targetRaw.Substring(0, $start) + $text + $targetRaw.Substring($end)
        $te=$null;$tt=$null; $targetAst = [System.Management.Automation.Language.Parser]::ParseInput($targetRaw, [ref]$tt, [ref]$te)
    }} else {{
        if (-not $targetRaw.EndsWith("`n")) {{ $targetRaw += "`n" }}
        $targetRaw += "`n" + $text + "`n"
        $te=$null;$tt=$null; $targetAst = [System.Management.Automation.Language.Parser]::ParseInput($targetRaw, [ref]$tt, [ref]$te)
    }}
}}
$enc = New-Object System.Text.UTF8Encoding $true
[System.IO.File]::WriteAllText($targetPath, $targetRaw, $enc)
"#,
    );
    run_ps_script(&script)?;
    let _ = std::fs::remove_file(&tmp);

    let content = workspace::read_file(&rel)?;
    assert_safety_ok(safety::check_with_options(&content, danger_mode)?)?;
    let file_parsed = ps_parser::parse_ps1(&content)?;
    sync::regenerate_with_parsed(file_name, Some(file_parsed))?;
    workspace::touch_last_sync()?;
    crate::install_mgr::spawn_sync_runtime_modules();
    Ok(names)
}
