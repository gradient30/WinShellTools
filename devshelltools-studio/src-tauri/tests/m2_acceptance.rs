#![allow(dead_code)]

mod common;

#[cfg(test)]
mod tests {
    use crate::common::IsolatedProfile;
    use devshelltools_studio_lib::consistency;
    use devshelltools_studio_lib::ps_parser;
    use devshelltools_studio_lib::safety;
    use devshelltools_studio_lib::sync;
    use devshelltools_studio_lib::workspace;

    fn ensure_initialized() {
        if !workspace::is_initialized() {
            workspace::init_from_template().expect("init_from_template");
        }
    }

    #[test]
    fn m2_e2e_baseline_consistency() {
        let _g = IsolatedProfile::new("m2");
        ensure_initialized();
        // 确保无测试残留：若 Docker.ps1 存在则删除并重生成
        if workspace::read_file("Public/Docker.ps1").is_ok() {
            workspace::delete_file("Public/Docker.ps1").expect("delete docker");
            sync::regenerate_all().expect("regenerate");
        }
        let report = consistency::check().expect("consistency check");
        assert!(report.ok, "基线应一致，错误：{:?}", report.errors);
        let cats = sync::scan_categories().expect("scan");
        assert_eq!(cats.len(), 5, "应有 5 个基线分类");
        assert!(report.actual_functions.contains(&"dsh".to_string()));
        assert!(report.actual_functions.contains(&"gg".to_string()));
        // 内部函数不应在导出列表
        assert!(!report.actual_functions.contains(&"Assert-Git".to_string()));
        assert!(!report
            .actual_functions
            .contains(&"Show-DstCategories".to_string()));
    }

    #[test]
    fn m2_e2e_create_category_and_sync() {
        let _g = IsolatedProfile::new("m2");
        ensure_initialized();
        // 构造一个新分类 Docker
        let docker_code = r#"<#!
@DST-Category
Name: docker
Title: Docker
Description: 容器管理快捷命令
Aliases: 容器,container
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
        // 安全检查
        let sr = safety::check(docker_code).expect("safety");
        assert!(sr.ok, "安全检查应通过：{:?}", sr.violations);
        // 语法校验
        ps_parser::validate_syntax(docker_code).expect("syntax");
        // 解析
        let parsed = ps_parser::parse_ps1(docker_code).expect("parse");
        assert_eq!(parsed.category.as_ref().unwrap().name, "docker");
        assert_eq!(parsed.functions.len(), 1);
        assert_eq!(parsed.functions[0].name, "dps");

        // 写入
        workspace::write_file("Public/Docker.ps1", docker_code).expect("write");
        // 重生成
        sync::regenerate_all().expect("regenerate");

        // 校验：现在应有 6 个分类
        let cats = sync::scan_categories().expect("scan");
        assert_eq!(cats.len(), 6, "新建后应有 6 个分类");
        let docker_cat = cats.iter().find(|c| c.category.name == "docker");
        assert!(docker_cat.is_some(), "应含 docker 分类");

        // 一致性校验：dps 应出现在所有导出列表中
        let report = consistency::check().expect("consistency");
        assert!(report.ok, "新建后应一致：{:?}", report.errors);
        assert!(report.actual_functions.contains(&"dps".to_string()));
        assert!(report.psd1_exports.contains(&"dps".to_string()));
        assert!(report.psm1_exports.contains(&"dps".to_string()));

        // 验证 .psd1 文件实际含 dps
        let psd1 = workspace::read_file("DevShellTools.psd1").expect("read psd1");
        assert!(psd1.contains("'dps'"), "psd1 应含 dps");
        let psm1 = workspace::read_file("DevShellTools.psm1").expect("read psm1");
        assert!(psm1.contains("\"dps\""), "psm1 应含 dps");
        let help = workspace::read_file("Public/Help.ps1").expect("read help");
        assert!(
            help.contains("'docker' = [PSCustomObject]"),
            "help 应含 docker 分类"
        );
        assert!(help.contains("'docker'"), "help ValidateSet 应含 docker");
        assert!(
            help.contains("@('dps','列出运行中容器。','dps')"),
            "help HelpData 应含 dps"
        );

        // 清理：删除 docker 分类
        workspace::delete_file("Public/Docker.ps1").expect("delete");
        sync::regenerate_all().expect("regenerate after delete");

        let cats2 = sync::scan_categories().expect("scan after delete");
        assert_eq!(cats2.len(), 5, "删除后应回到 5 个分类");
        let report2 = consistency::check().expect("consistency after delete");
        assert!(report2.ok, "删除后应一致：{:?}", report2.errors);
        assert!(!report2.actual_functions.contains(&"dps".to_string()));
    }

    #[test]
    fn m2_e2e_safety_blocks_dangerous() {
        let _g = IsolatedProfile::new("m2");
        let bad = r#"
function badpush {
    git push --force origin main
}
"#;
        let r = safety::check(bad).expect("safety");
        assert!(!r.ok, "force push 应被拦截");
        assert!(r.violations.iter().any(|v| v.contains("force")));
    }

    #[test]
    fn m2_e2e_syntax_error_rejected() {
        let _g = IsolatedProfile::new("m2");
        let bad = "function { broken";
        let r = ps_parser::validate_syntax(bad);
        assert!(r.is_err(), "语法错误应被拒绝");
    }
}
