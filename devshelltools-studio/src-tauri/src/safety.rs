use crate::error::DstResult;
use crate::ps_parser::{self, ParsedPsFile, PsCommand};

/// 安全检查结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SafetyReport {
    pub ok: bool,
    pub violations: Vec<String>,
}

/// 静态扫描 PowerShell 代码是否违反安全边界。
/// 规则与 README "安全边界" 一致，禁止危险命令。
pub fn check(code: &str) -> DstResult<SafetyReport> {
    check_with_options(code, false)
}

/// `danger_mode=true` 时跳过全部红线（仅 AI 会话 `/danger` 激活后的生成/插入路径）。
pub fn check_with_options(code: &str, danger_mode: bool) -> DstResult<SafetyReport> {
    if danger_mode {
        return Ok(SafetyReport {
            ok: true,
            violations: vec![],
        });
    }
    let parsed = ps_parser::parse_ps1(code)?;
    check_parsed(&parsed)
}

/// 使用已经解析的 AST 结果执行安全判定，避免重复启动 PowerShell。
pub fn check_parsed(parsed: &ParsedPsFile) -> DstResult<SafetyReport> {
    check_parsed_with_options(parsed, false)
}

pub fn check_parsed_with_options(
    parsed: &ParsedPsFile,
    danger_mode: bool,
) -> DstResult<SafetyReport> {
    if danger_mode {
        return Ok(SafetyReport {
            ok: true,
            violations: vec![],
        });
    }
    let mut violations = vec![];
    for command in &parsed.commands {
        check_command(command, &mut violations);
    }

    Ok(SafetyReport {
        ok: violations.is_empty(),
        violations,
    })
}

fn check_command(command: &PsCommand, violations: &mut Vec<String>) {
    if command.dynamic || command.name.trim().is_empty() {
        violations.push("默认禁止动态命令调用；请使用可静态检查的命令名称".into());
        return;
    }

    let name = command.name.to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);

    if name == "iex" || name == "invoke-expression" {
        violations.push("默认禁止 Invoke-Expression 动态执行".into());
        return;
    }

    if name == "git" {
        check_git(command, violations);
    }

    if is_remove_item(name)
        && has_parameter(&command.args, "recurse", 'r')
        && has_parameter(&command.args, "force", 'f')
    {
        violations.push("禁止 Remove-Item 同时使用 -Recurse 与 -Force（危险删除）".into());
    }

    if (name == "stop-process" || name == "spps" || name == "kill")
        && !command.within_should_process
        && !has_parameter(&command.args, "confirm", 'c')
    {
        violations.push("Stop-Process 必须配合 -Confirm 或 SupportsShouldProcess".into());
    }

    if name == "setenvironmentvariable"
        && command
            .args
            .iter()
            .any(|arg| arg.eq_ignore_ascii_case("User"))
    {
        violations.push("禁止 SetEnvironmentVariable 写入 User 级环境变量".into());
    }
}

fn check_git(command: &PsCommand, violations: &mut Vec<String>) {
    let Some((subcommand, index)) = git_subcommand(&command.args) else {
        return;
    };
    let args = &command.args[index + 1..];
    match subcommand.as_str() {
        "push" if args.iter().any(|arg| force_push_flag(arg)) =>
        {
            violations.push("禁止 git push --force / --force-with-lease".into());
        }
        "reset" if has_parameter(args, "hard", '\0') => {
            violations.push("禁止 git reset --hard".into());
        }
        "clean" if real_clean_flag(args) => {
            violations.push("禁止 git clean -f / -fd（真实删除），只允许 -n 预览".into());
        }
        _ => {}
    }
}

fn git_subcommand(args: &[String]) -> Option<(String, usize)> {
    let takes_value = ["-c", "-C", "--git-dir", "--work-tree", "--namespace"];
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if takes_value.iter().any(|value| arg.eq_ignore_ascii_case(value)) {
            index += 2;
            continue;
        }
        if arg.starts_with("--git-dir=")
            || arg.starts_with("--work-tree=")
            || arg.starts_with("--namespace=")
            || (arg.starts_with("-c") && arg.len() > 2)
            || (arg.starts_with("-C") && arg.len() > 2)
            || arg.starts_with('-')
        {
            index += 1;
            continue;
        }
        return Some((arg.to_ascii_lowercase(), index));
    }
    None
}

fn real_clean_flag(args: &[String]) -> bool {
    let mut force = false;
    let mut dry_run = false;
    for arg in args {
        if !arg.starts_with('-') || arg.starts_with("--") {
            continue;
        }
        let flags = arg.trim_start_matches('-').to_ascii_lowercase();
        if flags.contains('f') {
            force = true;
        }
        if flags.contains('n') {
            dry_run = true;
        }
    }
    force && !dry_run
}

fn is_remove_item(name: &str) -> bool {
    matches!(
        name,
        "remove-item" | "ri" | "rm" | "del" | "erase" | "rd" | "rmdir"
    )
}

fn has_parameter(args: &[String], long: &str, short: char) -> bool {
    args.iter().any(|arg| {
        if !arg.starts_with('-') {
            return false;
        }
        if arg.starts_with("--") {
            return arg
                .trim_start_matches('-')
                .split('=')
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case(long));
        }
        let value = arg
            .trim_start_matches('-')
            .split(':')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if value.chars().count() == 1 {
            return short != '\0' && value.starts_with(short);
        }
        (value.len() <= 3 && short != '\0' && value.contains(short))
            || (value.len() >= 2 && long.to_ascii_lowercase().starts_with(&value))
    })
}

fn force_push_flag(arg: &str) -> bool {
    let lower = arg.to_ascii_lowercase();
    if lower.starts_with("--force") {
        return true;
    }
    arg.starts_with('-') && !arg.starts_with("--") && lower[1..].contains('f')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_dry_run_ok() {
        let r = check("git clean -nd").unwrap();
        assert!(r.ok);
    }

    #[test]
    fn clean_real_delete_blocked() {
        let r = check("git clean -fd").unwrap();
        assert!(!r.ok);
        assert!(r.violations[0].contains("clean"));
    }

    #[test]
    fn force_push_blocked() {
        let r = check("git push --force origin main").unwrap();
        assert!(!r.ok);
    }

    #[test]
    fn hard_reset_blocked() {
        let r = check("git reset --hard HEAD~1").unwrap();
        assert!(!r.ok);
    }

    #[test]
    fn reset_substring_without_git_reset_ok() {
        // decorate / preset 等含 reset 子串但非 git reset --hard
        let r = check(r#"& git log --decorate --oneline --all "--max-count=$Count""#).unwrap();
        assert!(r.ok, "{:?}", r.violations);
    }

    #[test]
    fn synopsis_mention_git_reset_hard_in_other_fn_block_not_checked() {
        // upsert 只扫当前块；gg 块本身安全即应通过
        let gg = r#"function gg {
<#
.SYNOPSIS
显示图形化精简提交历史
.EXAMPLE
gg
#>
    Assert-Git
    & git log --graph --decorate --oneline --all "--max-count=$Count"
}"#;
        let r = check(gg).unwrap();
        assert!(r.ok, "gg 块不应被拦截：{:?}", r.violations);
    }

    #[test]
    fn danger_mode_allows_hard_reset() {
        let r = check_with_options("git reset --hard origin/main", true).unwrap();
        assert!(r.ok);
        assert!(r.violations.is_empty());
    }

    #[test]
    fn user_env_blocked() {
        let r = check(r#"[Environment]::SetEnvironmentVariable("x","y","User")"#).unwrap();
        assert!(!r.ok);
    }

    #[test]
    fn process_env_ok() {
        let r = check(r#"[Environment]::SetEnvironmentVariable("x","y","Process")"#).unwrap();
        assert!(r.ok);
    }

    #[test]
    fn safe_command_ok() {
        let r = check(r#"function gs { git status -sb }"#).unwrap();
        assert!(r.ok);
    }

    #[test]
    fn remove_item_force_without_recurse_ok() {
        let code = r#"
function rmf {
    [CmdletBinding(SupportsShouldProcess)]
    param([string]$Path)
    Remove-Item -LiteralPath $Path -Force -ErrorAction Stop
}
"#;
        let r = check(code).unwrap();
        assert!(r.ok, "仅 Remove-Item -Force 应放行：{:?}", r.violations);
    }

    #[test]
    fn remove_item_recurse_force_blocked() {
        let code = r#"
function badrm {
    param([string]$Path)
    Remove-Item -LiteralPath $Path -Recurse -Force
}
"#;
        let r = check(code).unwrap();
        assert!(!r.ok);
        assert!(r.violations.iter().any(|v| v.contains("Remove-Item")));
    }

    #[test]
    fn remove_item_recurse_force_multiline_blocked() {
        let code = "Remove-Item -LiteralPath $p `\n  -Recurse `\n  -Force\n";
        let r = check(code).unwrap();
        assert!(!r.ok, "续行形式也应拦截");
    }

    #[test]
    fn get_childitem_recurse_force_with_safe_remove_ok() {
        // 复现：sz 使用 Get-ChildItem -Recurse -Force，rmf 使用 Remove-Item -Force
        let code = r#"
function sz {
    Get-ChildItem -LiteralPath $Path -File -Recurse -Force -ErrorAction SilentlyContinue
}
function rmf {
    Remove-Item -LiteralPath $item.FullName -Force -ErrorAction Stop
}
"#;
        let r = check(code).unwrap();
        assert!(
            r.ok,
            "跨命令的 -Recurse/-Force 不应误报：{:?}",
            r.violations
        );
    }

    #[test]
    fn git_global_options_do_not_bypass_hard_reset_rule() {
        let report = check("& git -c x=y reset --hard").unwrap();
        assert!(!report.ok);
    }

    #[test]
    fn git_executable_does_not_bypass_clean_rule() {
        let report = check("git.exe clean -fd").unwrap();
        assert!(!report.ok);
    }

    #[test]
    fn dynamic_invocation_is_blocked_by_default() {
        let report = check("$cmd = 'git'; & $cmd reset --hard").unwrap();
        assert!(!report.ok);
    }

    #[test]
    fn invoke_expression_is_blocked_by_default() {
        let report = check("Invoke-Expression 'git reset --hard'").unwrap();
        assert!(!report.ok);
    }

    #[test]
    fn same_call_remove_recursion_and_force_is_blocked() {
        let report = check("Remove-Item -LiteralPath $p -Recurse -Force").unwrap();
        assert!(!report.ok);
    }

    #[test]
    fn remove_item_alias_with_combined_flags_is_blocked() {
        let report = check("ri -rf $p").unwrap();
        assert!(!report.ok);
    }

    #[test]
    fn combined_force_push_flag_is_blocked() {
        let report = check("git push -fu origin main").unwrap();
        assert!(!report.ok);
    }
}
