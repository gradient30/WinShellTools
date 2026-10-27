mod common;

#[cfg(test)]
mod tests {
    use crate::common::IsolatedProfile;
    use devshelltools_studio_lib::function_edit::{self, FunctionDraft};
    use devshelltools_studio_lib::install_mgr;
    use devshelltools_studio_lib::workspace;

    fn setup_workspace() {
        if !workspace::is_initialized() {
            workspace::init_from_template().expect("init");
        }
    }

    #[test]
    fn m6_upsert_and_delete_function() {
        let _g = IsolatedProfile::new("m6-fn");
        setup_workspace();

        let draft = FunctionDraft {
            name: "m6test".into(),
            synopsis: "M6 测试命令".into(),
            example: "m6test".into(),
            body: None,
            param_defaults: None,
            extra_examples: vec![],
        };
        function_edit::upsert_function("Files.ps1", draft, false).expect("upsert");

        let content = workspace::read_file("Public/Files.ps1").expect("read");
        assert!(content.contains("function m6test"));

        function_edit::delete_function("Files.ps1", "m6test").expect("delete");
        let content2 = workspace::read_file("Public/Files.ps1").expect("read2");
        assert!(!content2.contains("function m6test"));
    }

    /// 工作区 == PS5.1 模块目录时，安装不得清空自身，且应能 Import-Module。
    #[test]
    fn m6_install_does_not_destroy_workspace_when_source_equals_ps51() {
        let _g = IsolatedProfile::new("m6-install-preserve");
        setup_workspace();

        let root = workspace::workspace_root();
        let psd1 = root.join("DevShellTools.psd1");
        assert!(psd1.exists(), "安装前工作区应有清单");
        assert_eq!(
            root,
            install_mgr::ps51_module_dir(),
            "Studio 工作区必须等于 PS5.1 模块目录（本 bug 的前提）"
        );

        let result = install_mgr::install_module().expect("install_module 应成功");
        assert!(
            psd1.exists(),
            "安装后工作区 DevShellTools.psd1 必须仍存在（禁止 source==target 自毁）"
        );
        assert!(
            workspace::is_initialized(),
            "安装后工作区仍应处于已初始化状态"
        );
        assert!(result.status.profile_configured);
        assert!(result.status.ps7_module_present);
        assert!(result.status.installed);
        assert!(result.verified, "安装后 status.installed 应被验证为 true");
    }

    #[test]
    fn m6_install_status_and_soft_uninstall() {
        let _g = IsolatedProfile::new("m6-install");
        setup_workspace();

        let before = install_mgr::install_status();
        assert!(before.workspace_ready);
        assert!(before.ps51_module_present);

        let after_install = install_mgr::install_module().expect("install");
        assert!(after_install.status.profile_configured);
        assert!(after_install.status.ps7_module_present);
        assert!(
            workspace::is_initialized(),
            "安装后工作区应保留"
        );

        let after_uninstall = install_mgr::uninstall_module().expect("uninstall");
        assert!(!after_uninstall.status.installed);
        assert!(
            workspace::is_initialized(),
            "软卸载后工作区应保留（不得删除含 .studio 的 PS5.1 目录）"
        );
        assert!(
            !workspace::is_shell_enabled(),
            "软卸载后应禁用活动清单，避免 PS 自动加载"
        );
        assert!(
            workspace::workspace_root()
                .join(workspace::PSD1_DISABLED_NAME)
                .exists(),
            "软卸载后禁用清单应保留"
        );
        assert!(
            workspace::workspace_root()
                .join(workspace::PSM1_DISABLED_NAME)
                .exists(),
            "软卸载后禁用加载器应保留（仅禁 psd1 仍会被 .psm1 自动加载）"
        );
        assert!(
            !after_uninstall.status.ps51_module_present,
            "软卸载后 install_status 不应再视 PS5.1 模块为已加载可用"
        );

        // 命令发现不应再解析到 dsh（把隔离 Modules 放进子进程 PSModulePath）
        let docs = _g.documents_dir();
        let modules = docs.join("WindowsPowerShell").join("Modules");
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &format!(
                    // 仅使用隔离 Modules，避免主机真实 Modules 里的历史 backup 污染断言；
                    // 限定 -CommandType Function：PATH 中的同名可执行（如 nodejs 的 dsh shim）不参与模块自动加载判定
                    "$env:PSModulePath = '{}'; if (Get-Command dsh -CommandType Function -ErrorAction SilentlyContinue) {{ 'FOUND' }} else {{ 'MISSING' }}",
                    modules.to_string_lossy().replace('\'', "''")
                ),
            ])
            .output()
            .expect("spawn powershell");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("MISSING"),
            "软卸载后 Get-Command dsh 应失败，实际输出：{stdout}"
        );
    }

    #[test]
    fn m6_isolation_uses_temp_documents_not_real() {
        let g = IsolatedProfile::new("m6-iso");
        let root = workspace::workspace_root();
        let root_s = root.to_string_lossy().to_lowercase();
        let docs = g.documents_dir().to_string_lossy().to_lowercase();
        assert!(
            root_s.starts_with(&docs),
            "隔离后工作区应落在临时 Documents 下：root={root_s} docs={docs}"
        );
        assert!(
            root_s.contains("dst-m6-iso-profile"),
            "工作区路径应包含隔离前缀：{root_s}"
        );
    }

    /// 验收：删除函数后命令在运行时真实失效——PS7 副本不得残留（内容级同步），
    /// 新 PowerShell 会话显式加载模块清单后命令应不可发现。
    #[test]
    fn m6_delete_function_takes_effect_in_runtime() {
        let _g = IsolatedProfile::new("m6-del");
        setup_workspace();

        let draft = FunctionDraft {
            name: "delcmd".into(),
            synopsis: "删除生效验收".into(),
            example: "delcmd".into(),
            body: Some("    [CmdletBinding()] param()\n    Write-Output \"ok\"".into()),
            param_defaults: None,
            extra_examples: vec![],
        };
        function_edit::upsert_function("Files.ps1", draft, false).expect("upsert");
        // 等待后台运行时同步在隔离 guard 内完成
        std::thread::sleep(std::time::Duration::from_millis(800));

        let ps7_files = install_mgr::ps7_module_dir().join("Public").join("Files.ps1");
        let ps7_text = std::fs::read_to_string(&ps7_files).expect("PS7 副本应存在");
        assert!(
            ps7_text.contains("function delcmd"),
            "插入后 PS7 副本应含 delcmd（否则本用例前提不成立）"
        );

        function_edit::delete_function("Files.ps1", "delcmd").expect("delete");
        std::thread::sleep(std::time::Duration::from_millis(800));

        // PS7 副本内容级一致：无残留函数文本，psd1 导出清单亦无
        let ps7_text2 = std::fs::read_to_string(&ps7_files).expect("PS7 副本 2");
        assert!(
            !ps7_text2.contains("delcmd"),
            "删除后 PS7 副本不得残留 delcmd（内容比较同步失效会复现 gpush 式残留）"
        );
        let ps7_psd1 = std::fs::read_to_string(
            install_mgr::ps7_module_dir().join("DevShellTools.psd1"),
        )
        .unwrap();
        assert!(
            !ps7_psd1.contains("'delcmd'"),
            "删除后 PS7 副本 psd1 导出清单不得含 delcmd"
        );

        // 新会话显式加载工作区清单：命令应不可发现（-CommandType Function 防 PATH shim 干扰）
        let manifest = workspace::workspace_root().join("DevShellTools.psd1");
        let script = format!(
            "Import-Module -Name '{}' -Force -ErrorAction Stop; if (Get-Command delcmd -CommandType Function -ErrorAction SilentlyContinue) {{ 'FOUND' }} else {{ 'MISSING' }}",
            manifest.to_string_lossy().replace('\'', "''")
        );
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .output()
            .expect("spawn powershell");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("MISSING"),
            "删除后新会话不应再发现 delcmd，实际输出：{stdout} / stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
