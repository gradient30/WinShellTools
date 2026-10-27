use crate::error::{DstError, DstResult};
use crate::process_util::{output_hidden, ps_base_args};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::OnceLock;

/// 函数参数（从 AST ParamBlock 提取）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PsParam {
    pub name: String,
    #[serde(default)]
    pub type_name: String,
    /// 默认值源码文本，如 `20`、`""`；无默认则为 null
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default)]
    pub position: Option<i32>,
    #[serde(default)]
    pub is_switch: bool,
    /// 来自注释帮助 `.PARAMETER`，可能为空
    #[serde(default)]
    pub description: String,
}

/// 一个 PowerShell 函数的元信息（从 AST 提取）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PsFunction {
    pub name: String,
    pub synopsis: String,
    #[serde(rename = "first_example")]
    pub first_example: String,
    /// PS ConvertTo-Json 会把单元素数组拆成标量
    #[serde(default, deserialize_with = "deserialize_string_or_array")]
    pub examples: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_param_vec")]
    pub parameters: Vec<PsParam>,
}

/// 一个分类的元信息（从 @DST-Category 块提取）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CategoryMeta {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(deserialize_with = "deserialize_string_or_array")]
    pub aliases: Vec<String>,
}

/// 解析一个 .ps1 文件的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedPsFile {
    pub category: Option<CategoryMeta>,
    pub functions: Vec<PsFunction>,
    #[serde(rename = "parseErrors")]
    pub parse_errors: Vec<String>,
}

/// 兼容 PS5.1 ConvertTo-Json 的 string-or-array 行为：
/// 单元素数组被序列化成字符串，需还原成 Vec。
fn deserialize_string_or_array<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StrOrArr {
        Null,
        Single(String),
        Multi(Vec<String>),
    }
    match StrOrArr::deserialize(deserializer)? {
        StrOrArr::Null => Ok(vec![]),
        StrOrArr::Single(s) => {
            if s.trim().is_empty() {
                Ok(vec![])
            } else {
                Ok(vec![s])
            }
        }
        StrOrArr::Multi(v) => Ok(v),
    }
}

/// 兼容单参数对象被 ConvertTo-Json 拆成非数组。
fn deserialize_param_vec<'de, D>(deserializer: D) -> Result<Vec<PsParam>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        Null,
        One(PsParam),
        Many(Vec<PsParam>),
    }
    match OneOrMany::deserialize(deserializer)? {
        OneOrMany::Null => Ok(vec![]),
        OneOrMany::One(p) => Ok(vec![p]),
        OneOrMany::Many(v) => Ok(v),
    }
}

/// 从 FunctionDefinitionAst 提取 synopsis / examples / parameters。
const PS_FN_META_HELPER: &str = r#"
function ConvertTo-DstFunctionMeta {
    param($FnAst)
    $synopsis = ''; $examples = @(); $parameters = @()
    $help = $FnAst.GetHelpContent()
    if ($help -and $help.Synopsis) { $synopsis = $help.Synopsis.Trim() }
    if ($help -and $help.Examples) {
        foreach ($ex in $help.Examples) {
            $line = (($ex -split "`r?`n") | Where-Object { $_.Trim() } | Select-Object -First 1)
            if ($line) { $examples += $line.Trim() }
        }
    }
    $firstExample = if ($examples.Count -gt 0) { $examples[0] } else { '' }
    $pb = $FnAst.Body.ParamBlock
    if ($pb) {
        foreach ($p in $pb.Parameters) {
            $pname = $p.Name.VariablePath.UserPath
            $typeName = 'object'
            if ($p.StaticType) { $typeName = $p.StaticType.Name }
            $isSwitch = ($typeName -eq 'SwitchParameter') -or ($typeName -eq 'switch')
            $mandatory = $false
            $position = $null
            foreach ($a in $p.Attributes) {
                $tn = $a.TypeName.Name
                if ($tn -eq 'Parameter') {
                    foreach ($named in $a.NamedArguments) {
                        if ($named.ArgumentName -eq 'Mandatory') {
                            try { if ($named.Argument.SafeGetValue()) { $mandatory = $true } } catch {}
                        }
                        elseif ($named.ArgumentName -eq 'Position') {
                            try { $position = [int]$named.Argument.SafeGetValue() } catch {}
                        }
                    }
                }
            }
            $def = $null
            if ($null -ne $p.DefaultValue) { $def = $p.DefaultValue.Extent.Text.Trim() }
            $desc = ''
            if ($help -and $help.Parameters) {
                try {
                    $rawDesc = $help.Parameters[$pname]
                    if ($null -ne $rawDesc) { $desc = ([string]$rawDesc).Trim() }
                } catch {}
            }
            if (-not $desc) {
                if ($pname -match '^(Count|n|Num|Number|Limit|Max)$') {
                    if ($synopsis -match '提交|历史|log') { $desc = '提交历史数量' }
                    else { $desc = '数量' }
                }
                elseif ($pname -match '^(Path|File|Dir|Directory)$') { $desc = '路径' }
                elseif ($isSwitch) { $desc = '开关' }
                else { $desc = "参数 $pname" }
            }
            $parameters += [PSCustomObject]@{
                name = $pname
                type_name = $typeName
                default_value = $def
                mandatory = $mandatory
                position = $position
                is_switch = [bool]$isSwitch
                description = $desc
            }
        }
    }
    [PSCustomObject]@{
        name = $FnAst.Name
        synopsis = $synopsis
        first_example = $firstExample
        examples = $examples
        parameters = $parameters
    }
}
"#;

/// 调用 powershell.exe 解析 .ps1 文本，返回结构化结果。
/// 把 content 通过临时文件传递，避免 stdin 管道死锁。
pub fn parse_ps1(content: &str) -> DstResult<ParsedPsFile> {
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);
    let exe = which_powershell()?;
    // 写临时 .ps1 文件（带 BOM 让 PS5.1 正确识别 UTF-8）
    let mut tmp_path = std::env::temp_dir();
    tmp_path.push(format!(
        "dst-parse-{}-{}.ps1",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp_path)
            .map_err(|e| DstError::PsParse(format!("创建临时文件失败：{e}")))?;
        // 写 UTF-8 BOM + 内容
        f.write_all(&[0xEF, 0xBB, 0xBF])
            .map_err(|e| DstError::PsParse(format!("写 BOM 失败：{e}")))?;
        f.write_all(content.as_bytes())
            .map_err(|e| DstError::PsParse(format!("写内容失败：{e}")))?;
    }

    let tmp_path_str = tmp_path.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"{helper}
$ErrorActionPreference = 'Stop'
$raw = Get-Content -LiteralPath '{tmp_path_str}' -Raw -Encoding UTF8
$errors = $null
$tokens = $null
$ast = [System.Management.Automation.Language.Parser]::ParseInput($raw, [ref]$tokens, [ref]$errors)
$result = [ordered]@{{ category = $null; functions = @(); parseErrors = @() }}
foreach ($e in $errors) {{ $result.parseErrors += $e.Extent.StartLineNumber.ToString() + ':' + $e.Message }}
if ($null -ne $ast) {{
    $text = $ast.ToString()
    $m = [regex]::Match($text, '(?s)@DST-Category\s*\r?\n(.*?)\r?\n@DST-Category-End')
    if ($m.Success) {{
        $block = $m.Groups[1].Value
        $name = ''; $title = ''; $description = ''; $aliases = @()
        foreach ($line in $block -split "`r?`n") {{
            if ($line -match '^\s*Name:\s*(.+?)\s*$') {{ $name = $Matches[1] }}
            elseif ($line -match '^\s*Title:\s*(.+?)\s*$') {{ $title = $Matches[1] }}
            elseif ($line -match '^\s*Description:\s*(.+?)\s*$') {{ $description = $Matches[1] }}
            elseif ($line -match '^\s*Aliases:\s*(.*)$') {{
                $a = $Matches[1].Trim()
                if ($a) {{ $aliases = ($a -split ',') | ForEach-Object {{ $_.Trim() }} | Where-Object {{ $_ }} }}
            }}
        }}
        if ($name) {{ $result.category = [PSCustomObject]@{{ name = $name; title = $title; description = $description; aliases = $aliases }} }}
    }}
    $fnAsts = $ast.FindAll({{ param($n, $_) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] }}, $true)
    foreach ($fnAst in $fnAsts) {{
        $result.functions += ConvertTo-DstFunctionMeta -FnAst $fnAst
    }}
}}
$result | ConvertTo-Json -Depth 8 -Compress
"#,
        helper = PS_FN_META_HELPER,
        tmp_path_str = tmp_path_str
    );

    let mut cmd = Command::new(exe);
    for arg in ps_base_args(exe) {
        cmd.arg(arg);
    }
    cmd.args(["-Command", &script]);
    let output = output_hidden(cmd)
        .map_err(|e| DstError::PsParse(format!("启动 powershell 失败：{e}")))?;

    // 清理临时文件
    let _ = std::fs::remove_file(&tmp_path);

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(DstError::PsParse(format!(
            "powershell 退出码 {}：{}",
            output.status,
            err.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_line = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with('{'))
        .ok_or_else(|| DstError::PsParse(format!("未找到 JSON 输出，stdout: {}", stdout.trim())))?;

    let parsed: ParsedPsFile =
        serde_json::from_str(json_line).map_err(|e| DstError::PsParse(format!("反序列化失败：{e}")))?;

    if !parsed.parse_errors.is_empty() {
        return Err(DstError::PsParse(format!(
            "语法错误：{}",
            parsed.parse_errors.join("; ")
        )));
    }
    Ok(parsed)
}

/// 批量解析 Public 目录下多个 .ps1（单次 PowerShell 进程，避免逐个启动）。
/// 任一文件有语法错误即整体失败（工作区扫描用：坏文件视为损坏）。
pub fn parse_public_batch(paths: &[std::path::PathBuf]) -> DstResult<Vec<(String, ParsedPsFile)>> {
    let out = run_batch_parse(paths)?;
    for (file_name, parsed) in &out {
        if !parsed.parse_errors.is_empty() {
            return Err(DstError::PsParse(format!(
                "{} 语法错误：{}",
                file_name,
                parsed.parse_errors.join("; ")
            )));
        }
    }
    Ok(out)
}

/// 批量解析（容错）：单文件语法错误不中断，错误记入该文件的 parse_errors。
/// 供同步包预览使用：外来包内可能混有坏文件，需逐个标记而非整体失败。
pub fn parse_public_batch_lenient(
    paths: &[std::path::PathBuf],
) -> DstResult<Vec<(String, ParsedPsFile)>> {
    run_batch_parse(paths)
}

/// 单次 PowerShell 进程批量解析；语法错误随各文件结果带回，不在此处判定。
fn run_batch_parse(paths: &[std::path::PathBuf]) -> DstResult<Vec<(String, ParsedPsFile)>> {
    if paths.is_empty() {
        return Ok(vec![]);
    }
    let exe = which_powershell()?;
    let path_literals: Vec<String> = paths
        .iter()
        .map(|p| format!("'{}'", p.to_string_lossy().replace('\'', "''")))
        .collect();
    let paths_ps = path_literals.join(", ");

    let script = format!(
        r#"{helper}
$ErrorActionPreference = 'Stop'
function Get-ParsedFile {{
    param([string]$Path)
    $fileName = [System.IO.Path]::GetFileName($Path)
    $raw = Get-Content -LiteralPath $Path -Raw -Encoding UTF8
    $errors = $null; $tokens = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseInput($raw, [ref]$tokens, [ref]$errors)
    $parseErrors = @()
    foreach ($e in $errors) {{ $parseErrors += ($e.Extent.StartLineNumber.ToString() + ':' + $e.Message) }}
    $category = $null
    $functions = @()
    if ($null -ne $ast) {{
        $text = $ast.ToString()
        $m = [regex]::Match($text, '(?s)@DST-Category\s*\r?\n(.*?)\r?\n@DST-Category-End')
        if ($m.Success) {{
            $block = $m.Groups[1].Value
            $name = ''; $title = ''; $description = ''; $aliases = @()
            foreach ($line in $block -split "`r?`n") {{
                if ($line -match '^\s*Name:\s*(.+?)\s*$') {{ $name = $Matches[1] }}
                elseif ($line -match '^\s*Title:\s*(.+?)\s*$') {{ $title = $Matches[1] }}
                elseif ($line -match '^\s*Description:\s*(.+?)\s*$') {{ $description = $Matches[1] }}
                elseif ($line -match '^\s*Aliases:\s*(.*)$') {{
                    $a = $Matches[1].Trim()
                    if ($a) {{ $aliases = ($a -split ',') | ForEach-Object {{ $_.Trim() }} | Where-Object {{ $_ }} }}
                }}
            }}
            if ($name) {{ $category = [PSCustomObject]@{{ name = $name; title = $title; description = $description; aliases = $aliases }} }}
        }}
        $fnAsts = $ast.FindAll({{ param($n, $_) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] }}, $true)
        foreach ($fnAst in $fnAsts) {{
            $functions += ConvertTo-DstFunctionMeta -FnAst $fnAst
        }}
    }}
    [PSCustomObject]@{{ fileName = $fileName; category = $category; functions = $functions; parseErrors = $parseErrors }}
}}
$items = @()
foreach ($p in @({paths_ps})) {{ $items += Get-ParsedFile -Path $p }}
[PSCustomObject]@{{ items = $items }} | ConvertTo-Json -Depth 8 -Compress
"#,
        helper = PS_FN_META_HELPER,
        paths_ps = paths_ps
    );

    let mut cmd = Command::new(exe);
    for arg in ps_base_args(exe) {
        cmd.arg(arg);
    }
    cmd.args(["-Command", &script]);
    let output = output_hidden(cmd)
        .map_err(|e| DstError::PsParse(format!("启动 powershell 失败：{e}")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(DstError::PsParse(format!(
            "powershell 退出码 {}：{}",
            output.status,
            err.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_line = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with('{'))
        .ok_or_else(|| DstError::PsParse(format!("未找到 JSON 输出，stdout: {}", stdout.trim())))?;

    #[derive(Deserialize)]
    struct BatchWrap {
        items: Vec<ParsedPsFileWithName>,
    }
    #[derive(Deserialize)]
    struct ParsedPsFileWithName {
        #[serde(rename = "fileName")]
        file_name: String,
        category: Option<CategoryMeta>,
        functions: Vec<PsFunction>,
        #[serde(rename = "parseErrors")]
        parse_errors: Vec<String>,
    }

    let wrapped: BatchWrap =
        serde_json::from_str(json_line).map_err(|e| DstError::PsParse(format!("反序列化失败：{e}")))?;

    let mut out = vec![];
    for item in wrapped.items {
        out.push((
            item.file_name,
            ParsedPsFile {
                category: item.category,
                functions: item.functions,
                parse_errors: item.parse_errors,
            },
        ));
    }
    Ok(out)
}

fn which_powershell() -> DstResult<&'static str> {
    static EXE: OnceLock<&'static str> = OnceLock::new();
    Ok(*EXE.get_or_init(|| {
        let mut cmd = std::process::Command::new("pwsh");
        cmd.arg("--version");
        if crate::process_util::output_hidden_ref(&mut cmd).is_ok() {
            "pwsh"
        } else {
            "powershell"
        }
    }))
}

/// 仅做语法校验：轻量 ParseInput，不提取帮助/分类（比 parse_ps1 快一个数量级以上）。
pub fn validate_syntax(content: &str) -> DstResult<()> {
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);
    let exe = which_powershell()?;
    let mut tmp_path = std::env::temp_dir();
    tmp_path.push(format!(
        "dst-syntax-{}-{}.ps1",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp_path)
            .map_err(|e| DstError::PsParse(format!("创建临时文件失败：{e}")))?;
        f.write_all(&[0xEF, 0xBB, 0xBF])
            .map_err(|e| DstError::PsParse(format!("写 BOM 失败：{e}")))?;
        f.write_all(content.as_bytes())
            .map_err(|e| DstError::PsParse(format!("写内容失败：{e}")))?;
    }
    let tmp = tmp_path.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
$raw = Get-Content -LiteralPath '{tmp}' -Raw -Encoding UTF8
$errors = $null; $tokens = $null
[void][System.Management.Automation.Language.Parser]::ParseInput($raw, [ref]$tokens, [ref]$errors)
if ($errors -and $errors.Count -gt 0) {{
  ($errors | ForEach-Object {{ $_.Extent.StartLineNumber.ToString() + ':' + $_.Message }}) -join '; '
}} else {{
  'OK'
}}
"#
    );
    let mut cmd = Command::new(exe);
    for arg in ps_base_args(exe) {
        cmd.arg(arg);
    }
    cmd.args(["-Command", &script]);
    let output = output_hidden(cmd)
        .map_err(|e| DstError::PsParse(format!("启动 powershell 失败：{e}")))?;
    let _ = std::fs::remove_file(&tmp_path);
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(DstError::PsParse(format!(
            "powershell 退出码 {}：{}",
            output.status,
            err.trim()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("OK");
    if line != "OK" {
        return Err(DstError::PsParse(line.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_files_ps1() {
        let code = include_str!("../../templates/Public/Files.ps1");
        let r = parse_ps1(code).expect("parse");
        let cat = r.category.expect("category");
        assert_eq!(cat.name, "files");
        assert_eq!(cat.title, "文件管理");
        assert!(cat.aliases.contains(&"文件".to_string()));
        // Files.ps1 应含 lt/ltf/ltd/ll/la/mkcd/up/up2/open/here/sz
        let names: Vec<_> = r.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"lt"));
        assert!(names.contains(&"sz"));
        assert!(names.contains(&"mkcd"));
        // 每个函数应有 synopsis
        for f in &r.functions {
            assert!(!f.synopsis.is_empty(), "函数 {} 缺 synopsis", f.name);
        }
    }

    #[test]
    fn parse_git_ps1() {
        let code = include_str!("../../templates/Public/Git.ps1");
        let r = parse_ps1(code).expect("parse");
        let cat = r.category.expect("category");
        assert_eq!(cat.name, "git");
        let names: Vec<_> = r.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"gs"));
        assert!(names.contains(&"gg"));
        assert!(names.contains(&"gclean"));
        let gg = r.functions.iter().find(|f| f.name == "gg").expect("gg");
        assert!(
            gg.parameters.iter().any(|p| p.name == "Count" && p.default_value.as_deref() == Some("20")),
            "gg 应解析出 Count=20，实际 {:?}",
            gg.parameters
        );
        assert!(gg.examples.len() >= 1, "gg 应有示例");
    }

    #[test]
    fn parse_help_ps1_has_dsh() {
        let code = include_str!("../../templates/Public/Help.ps1");
        let r = parse_ps1(code).expect("parse");
        // Help.ps1 无 @DST-Category 块（它是公共部分）
        assert!(r.category.is_none());
        let names: Vec<_> = r.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"dsh"));
        assert!(names.contains(&"Show-DstCategories"));
    }

    #[test]
    fn parse_invalid_syntax_fails() {
        let bad = "function { invalid syntax here";
        let r = parse_ps1(bad);
        assert!(r.is_err());
    }
}