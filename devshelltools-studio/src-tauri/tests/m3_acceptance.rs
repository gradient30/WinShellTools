#![allow(dead_code)]

mod common;

#[cfg(test)]
mod tests {
    use crate::common::IsolatedProfile;
    use devshelltools_studio_lib::ai_config;
    use devshelltools_studio_lib::function_edit;
    use devshelltools_studio_lib::ps_parser;
    use devshelltools_studio_lib::safety;
    use devshelltools_studio_lib::workspace;

    /// 验收1：AI 配置默认值 + 持久化
    #[test]
    fn m3_ai_config_default_and_persist() {
        let cfg = ai_config::AiConfig::default();
        assert_eq!(cfg.protocol, ai_config::AiProtocol::Openai);
        assert!(cfg.base_url.contains("openai.com"));
        assert!(!cfg.model.is_empty());
    }

    /// 验收2：System Prompt 含全部安全规则
    #[test]
    fn m3_system_prompt_has_safety_rules() {
        let p = ai_config::system_prompt();
        assert!(p.contains("--force"));
        assert!(p.contains("--hard"));
        assert!(p.contains("clean -fd"));
        assert!(p.contains("Stop-Process"));
        assert!(p.contains("SetEnvironmentVariable"));
        assert!(p.contains("Remove-Item -Recurse -Force"));
        assert!(p.contains(".SYNOPSIS"));
        assert!(p.contains("@DST-Category"));
    }

    /// 验收3：代码块提取
    #[test]
    fn m3_extract_code_blocks() {
        let ai_reply = r#"我为你生成了一个查看容器日志的命令：

```powershell
<#!
@DST-Category
Name: docker
Title: Docker
Description: 容器管理
Aliases: 容器
@DST-Category-End
#>

function dlogs {
<#
.SYNOPSIS
查看容器日志。
.EXAMPLE
dlogs mycontainer
#>
    [CmdletBinding()] param([Parameter(Mandatory)][string]$Name)
    Assert-DstCommand "docker"
    & docker logs $Name
}
```

这个命令使用了 `docker logs`。
"#;
        let blocks = ai_config::extract_code_blocks(ai_reply);
        assert_eq!(blocks.len(), 1, "应提取 1 个代码块");
        let code = &blocks[0];
        assert!(code.contains("function dlogs"));
        assert!(code.contains("@DST-Category"));
        assert!(code.contains(".SYNOPSIS"));
    }

    /// 验收4：AI 生成的安全代码通过全链路校验
    #[test]
    fn m3_safe_ai_code_passes_full_validation() {
        let code = r#"<#!
@DST-Category
Name: docker
Title: Docker
Description: 容器管理
Aliases: 容器
@DST-Category-End
#>

function dps {
<#
.SYNOPSIS
列出运行中容器。
.EXAMPLE
dps
#>
    [CmdletBinding()] param()
    Assert-DstCommand "docker"
    & docker ps
}
"#;
        // 语法校验
        ps_parser::validate_syntax(code).expect("语法应通过");
        // 安全校验
        let sr = safety::check(code).expect("安全检查");
        assert!(sr.ok, "安全检查应通过：{:?}", sr.violations);
        // AST 解析
        let parsed = ps_parser::parse_ps1(code).expect("解析");
        assert_eq!(parsed.category.as_ref().unwrap().name, "docker");
        assert_eq!(parsed.functions.len(), 1);
        assert_eq!(parsed.functions[0].name, "dps");
        assert!(!parsed.functions[0].synopsis.is_empty());
    }

    /// 验收5：AI 生成的危险代码被安全规则拦截
    #[test]
    fn m3_dangerous_ai_code_blocked() {
        let dangerous = r#"
function badpush {
<#
.SYNOPSIS
强制推送。
.EXAMPLE
badpush
#>
    [CmdletBinding()] param()
    & git push --force origin main
}
"#;
        // 语法应通过（语法层面合法）
        ps_parser::validate_syntax(dangerous).expect("语法应通过");
        // 安全检查应拦截
        let sr = safety::check(dangerous).expect("安全检查");
        assert!(!sr.ok, "force push 应被拦截");
        assert!(sr.violations.iter().any(|v| v.contains("force")));
    }

    /// 验收6：AI 生成的代码缺 .SYNOPSIS 时，AST 能解析但 consistency 会警告
    #[test]
    fn m3_missing_synopsis_detectable() {
        let code = r#"
function nohelp {
    [CmdletBinding()] param()
    Write-Host "no help"
}
"#;
        let parsed = ps_parser::parse_ps1(code).expect("解析");
        assert_eq!(parsed.functions.len(), 1);
        assert!(parsed.functions[0].synopsis.is_empty(), "缺 synopsis 应为空");
    }

    /// 验收7：AI 块内大写辅助函数随公共命令一并写入（此前被丢弃导致命令运行时报错）；
    /// 返回值只汇报公共命令名
    #[test]
    fn m3_apply_keeps_helper_functions() {
        let _g = IsolatedProfile::new("m3-apply-helper");
        workspace::init_from_template().expect("init");
        let code = r#"function Assert-FakeTool {
    Assert-DstCommand "git"
}

function faketool {
<#
.SYNOPSIS
测试辅助函数随命令保留。
.EXAMPLE
faketool
#>
    [CmdletBinding()] param()
    Assert-FakeTool
    Write-Output "ok"
}
"#;
        let names = function_edit::apply_code_to_category("Files.ps1", code).expect("apply");
        assert_eq!(names, vec!["faketool".to_string()], "只汇报公共命令名");
        let content = workspace::read_file("Public/Files.ps1").unwrap();
        assert!(
            content.contains("function Assert-FakeTool"),
            "辅助函数应写入分类文件，否则 faketool 调用会失败"
        );
        assert!(content.contains("function faketool"));
        // 辅助函数不得进入导出清单
        let psd1 = workspace::read_file("DevShellTools.psd1").unwrap();
        assert!(psd1.contains("'faketool'"), "公共命令应入导出清单");
        assert!(!psd1.contains("Assert-FakeTool"), "辅助函数不入导出清单");
        // 等待后台运行时同步线程在隔离 guard 内完成
        std::thread::sleep(std::time::Duration::from_millis(800));
    }
}