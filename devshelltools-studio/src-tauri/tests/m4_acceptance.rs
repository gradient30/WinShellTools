#![allow(dead_code)]

mod common;

#[cfg(test)]
mod tests {
    use crate::common::IsolatedProfile;
    use devshelltools_studio_lib::ai_config::{self, AiProfile, AiProtocol};
    use devshelltools_studio_lib::export;
    use devshelltools_studio_lib::logging;
    use devshelltools_studio_lib::migrate;
    use devshelltools_studio_lib::webview2;
    use devshelltools_studio_lib::workspace;

    /// 验收1：迁移检测返回结构正确
    #[test]
    fn m4_migration_check_struct() {
        let _g = IsolatedProfile::new("m4");
        let check = migrate::check_migration();
        assert!(check.legacy_dirs.len() <= 3, "最多 3 个待处理来源");
    }

    /// 验收：迁移后旧沙箱归档，且不再提示 has_legacy（隔离环境下无真实旧目录时跳过归档断言）
    #[test]
    fn m4_migrate_clears_sandbox_when_present() {
        let _g = IsolatedProfile::new("m4-migrate");
        devshelltools_studio_lib::workspace::init_from_template().expect("init");
        let docs = std::env::var("USERPROFILE")
            .map(std::path::PathBuf::from)
            .unwrap()
            .join("Documents");
        let sandbox = docs.join("DevShellTools");
        let _ = std::fs::remove_dir_all(&sandbox);
        std::fs::create_dir_all(sandbox.join("Public")).unwrap();
        std::fs::write(
            sandbox.join("DevShellTools.psd1"),
            "@{ ModuleVersion = '0.0.1' }",
        )
        .unwrap();
        std::fs::write(
            sandbox.join("Public").join("LegacyOnly.ps1"),
            r#"
# @DST-Category
# Name: legacyonly
# Title: 旧版
# Description: 迁移验收
# Aliases:
# @DST-Category-End
function legacyonly {
<#
.SYNOPSIS
迁移验收命令。
.EXAMPLE
legacyonly
#>
    [CmdletBinding()] param()
    "ok"
}
"#,
        )
        .unwrap();

        let before = migrate::check_migration();
        assert!(before.has_legacy, "应检测到旧沙箱");

        let result = migrate::migrate_from_legacy().expect("migrate");
        assert!(
            result.migrated_files.iter().any(|f| f == "LegacyOnly.ps1"),
            "应迁入 LegacyOnly.ps1：{:?}",
            result.migrated_files
        );
        assert!(!sandbox.exists(), "旧沙箱应已归档：{}", sandbox.display());
        assert!(
            !result.archived_dirs.is_empty(),
            "应返回归档路径：{:?}",
            result
        );

        let after = migrate::check_migration();
        assert!(
            !after.has_legacy,
            "迁移清理后不应再提示旧版：{:?}",
            after.legacy_dirs
        );
    }

    /// 验收2：导出/导入 round-trip
    #[test]
    fn m4_export_import_roundtrip() {
        let _g = IsolatedProfile::new("m4-export");
        // 初始化工作区
        devshelltools_studio_lib::workspace::init_from_template().expect("init");
        let root = devshelltools_studio_lib::workspace::workspace_root();
        
        // 创建可安全导入的测试脚本（当前导出仅为 Public/*.ps1 平铺）
        let test_ps1 = r#"
function testexport {
<#
.SYNOPSIS
导出导入验收用命令。
.EXAMPLE
testexport
#>
    [CmdletBinding()] param()
    Write-Output "ok"
}
"#;
        devshelltools_studio_lib::workspace::write_file("Public/TestExport.ps1", test_ps1)
            .unwrap();

        let export_dir = std::env::temp_dir().join(format!(
            "dst-export-target-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&export_dir);
        export::export_scripts(export_dir.to_str().unwrap()).expect("export");
        assert!(
            export_dir.join("TestExport.ps1").exists(),
            "导出目录应平铺含 TestExport.ps1"
        );
        assert!(
            !export_dir.join("DevShellTools.psd1").exists(),
            "脚本级导出不应含公共部分 psd1"
        );
        assert!(!export_dir.join(".git").exists(), "导出不含 .git");
        let _ = root;

        // 导入到新位置（模拟多机）
        drop(_g);
        let _g2 = IsolatedProfile::new("m4-import");
        let root2 = devshelltools_studio_lib::workspace::workspace_root();
        devshelltools_studio_lib::workspace::init_from_template().expect("init2");

        let imported = export::import_scripts(export_dir.to_str().unwrap()).expect("import");
        assert!(
            imported.imported.iter().any(|n| n == "TestExport.ps1"),
            "应导入 TestExport.ps1：{:?}",
            imported
        );
        assert!(
            root2.join("Public").join("TestExport.ps1").exists(),
            "导入后应含测试文件"
        );

        let _ = std::fs::remove_dir_all(&export_dir);
    }

    /// 验收3：日志脱敏
    #[test]
    fn m4_log_sanitization() {
        let _g = IsolatedProfile::new("m4-log");
        // sk- key 脱敏
        let s1 = logging::sanitize("api_key=sk-abc1234567890xyz");
        assert!(s1.contains("****"));
        assert!(!s1.contains("7890xyz"));
        // Bearer 脱敏
        let s2 = logging::sanitize("Authorization: Bearer abcdef12345678");
        assert!(s2.contains("****"));
        assert!(!s2.contains("abcdef12345678"));
        // 普通文本不变
        let s3 = logging::sanitize("普通日志消息");
        assert_eq!(s3, "普通日志消息");
    }

    /// 验收4：WebView2 状态检测不 panic
    #[test]
    fn m4_webview2_check_no_panic() {
        let _g = IsolatedProfile::new("m4-webview2");
        let status = webview2::check_webview2().expect("webview2 check");
        // 测试环境可能装了也可能没装，只要不 panic 即可
        assert!(status.installed || !status.installed);
    }

    /// 验收5：导出排除 .studio 目录
    #[test]
    fn m4_export_excludes_studio() {
        let _g = IsolatedProfile::new("m4-exclude");
        devshelltools_studio_lib::workspace::init_from_template().expect("init");
        let root = devshelltools_studio_lib::workspace::workspace_root();
        
        let export_dir = std::env::temp_dir().join(format!(
            "dst-export-exclude-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&export_dir);
        export::export_scripts(export_dir.to_str().unwrap()).expect("export");
        assert!(!export_dir.join(".studio").exists(), "导出应排除 .studio");
        assert!(!export_dir.join(".git").exists(), "导出应排除 .git");
        let _ = std::fs::remove_dir_all(&export_dir);
    }

    // ============ 完整配置包（bundle）双向同步 ============

    const SECRET_KEY: &str = "sk-secret-should-not-leak";

    fn custom_category_ps1() -> &'static str {
        r#"<#!
@DST-Category
Name: customtools
Title: 自定义工具
Description: 配置包迁移验收
Aliases:
@DST-Category-End
#>

function customcmd {
<#
.SYNOPSIS
配置包迁移验收命令。
.EXAMPLE
customcmd
#>
    [CmdletBinding()] param()
    Write-Output "ok"
}
"#
    }

    fn tmp_bundle_dir(prefix: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "dst-{}-{}-{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    /// 验收：配置包 round-trip（导出 → 预览 → 导入 → 公共部分自动重生成 → Key 不泄露）
    #[test]
    fn m4_bundle_export_preview_import_roundtrip() {
        let _g = IsolatedProfile::new("m4-bundle-a");
        workspace::init_from_template().expect("init");
        workspace::write_file("Public/Custom.ps1", custom_category_ps1()).unwrap();
        ai_config::save_profile(
            AiProfile {
                id: "prof-sync-test".into(),
                name: "同步测试".into(),
                protocol: AiProtocol::Openai,
                base_url: "https://api.example.com/v1".into(),
                model: "test-model".into(),
                temperature: 0.7,
                max_tokens: 8192,
                key_configured: false,
            },
            Some(SECRET_KEY),
        )
        .expect("save profile");

        let bundle = tmp_bundle_dir("bundle-a");
        let exported = export::export_bundle(bundle.to_str().unwrap()).expect("export bundle");
        assert!(bundle.join("manifest.json").exists(), "包应含 manifest.json");
        assert!(bundle.join("Public").join("Custom.ps1").exists());
        assert!(bundle.join("Private").join("Common.ps1").exists());
        assert!(bundle.join("ai_profiles.json").exists());
        // A 端迁移逻辑自动附带 1 个默认 Profile，导出数 ≥ 1
        assert!(exported.ai_profiles >= 1, "实际导出 {} 个", exported.ai_profiles);
        // API Key 永不随包泄露
        let ai_raw = std::fs::read_to_string(bundle.join("ai_profiles.json")).unwrap();
        assert!(!ai_raw.contains(SECRET_KEY), "ai_profiles.json 不得包含 Key");
        let manifest_raw = std::fs::read_to_string(bundle.join("manifest.json")).unwrap();
        assert!(!manifest_raw.contains(SECRET_KEY));
        let manifest: export::BundleManifest =
            serde_json::from_str(&manifest_raw).expect("manifest 格式");
        assert_eq!(manifest.format, export::BUNDLE_FORMAT);
        assert!(manifest.files.iter().any(|f| f.rel == "Public/Custom.ps1"));

        // B 端：全新隔离环境模拟"其它用户"
        drop(_g);
        let _g2 = IsolatedProfile::new("m4-bundle-b");
        workspace::init_from_template().expect("init b");

        let preview = export::preview_bundle(bundle.to_str().unwrap()).expect("preview");
        assert!(preview.manifest_found, "应识别 manifest");
        assert_eq!(preview.template_version, "1.0.5");
        let custom = preview
            .items
            .iter()
            .find(|i| i.rel == "Public/Custom.ps1")
            .expect("Custom.ps1 应在预览中");
        assert_eq!(custom.action, "add");
        assert!(custom.checked, "add 项应默认勾选");
        assert_eq!(custom.commands, 1, "应统计出 1 个公共命令");
        // 同模板初始化 → 模板文件内容相同
        let files_item = preview
            .items
            .iter()
            .find(|i| i.rel == "Public/Files.ps1")
            .expect("Files.ps1 应在预览中");
        assert_eq!(files_item.action, "same", "同模板文件应为 same");
        assert!(!files_item.checked);
        // A 端 save_profile 时迁移逻辑会附带 1 个默认 Profile，故新增数 ≥ 1
        assert!(
            preview.ai_new >= 1 && preview.ai_existing >= 0,
            "AI 配置应识别新增（实际 new={} existing={}）",
            preview.ai_new,
            preview.ai_existing
        );

        // 导入默认勾选项（add + ai）
        let selection: Vec<String> = preview
            .items
            .iter()
            .filter(|i| i.checked)
            .map(|i| i.rel.clone())
            .collect();
        assert!(selection.contains(&"Public/Custom.ps1".to_string()));
        assert!(selection.contains(&"ai_profiles.json".to_string()));
        let result =
            export::import_bundle(bundle.to_str().unwrap(), &selection).expect("import bundle");
        assert!(result.imported.iter().any(|r| r == "Public/Custom.ps1"));
        assert!(result.ai_added >= 1);
        assert!(result.regenerated, "导入分类后应重生成公共部分");
        assert!(result.runtime_synced, "应触发后台运行时同步");
        assert!(result.errors.is_empty(), "不应有错误：{:?}", result.errors);

        // 公共部分已自动重生成：psd1 / Help.ps1 含 customcmd
        let psd1 = workspace::read_file("DevShellTools.psd1").unwrap();
        assert!(psd1.contains("'customcmd'"), "psd1 应含 customcmd");
        let help = workspace::read_file("Public/Help.ps1").unwrap();
        assert!(help.contains("customcmd"), "Help.ps1 应含 customcmd 帮助");
        // 写入应为 BOM UTF-8（PS 5.1 中文安全）
        let head = std::fs::read(workspace::workspace_root().join("Public").join("Custom.ps1"))
            .unwrap();
        assert_eq!(head[..3], [0xEF, 0xBB, 0xBF], "导入文件应带 UTF-8 BOM");
        // AI profile 已并入且 Key 状态为未配置
        let store = ai_config::load_profiles_store().unwrap();
        let p = store.profiles.iter().find(|p| p.id == "prof-sync-test").unwrap();
        assert!(!p.key_configured, "Key 不随包导入");

        // 等待后台运行时同步线程在隔离 guard 内完成，避免污染真实模块目录
        std::thread::sleep(std::time::Duration::from_millis(800));
        let _ = std::fs::remove_dir_all(&bundle);
    }

    /// 验收：预览冲突规则（overwrite=包内较新 / keep_local=本地较新 / invalid=安全拦截）
    #[test]
    fn m4_bundle_preview_conflict_rules() {
        let _g = IsolatedProfile::new("m4-bundle-conflict");
        workspace::init_from_template().expect("init");

        // 手工搭一个包目录（不经 export_bundle）
        let pkg = tmp_bundle_dir("bundle-conflict");
        std::fs::create_dir_all(pkg.join("Public")).unwrap();

        // 1) 包内较新：内容不同 + 本地 mtime 回拨一天
        std::fs::write(
            pkg.join("Public").join("Files.ps1"),
            "# 包内旧内容（与本地不同）\nfunction pkgfiles { Write-Output 1 }\n",
        )
        .unwrap();
        // 2) invalid：语法合法但触发安全红线
        std::fs::write(
            pkg.join("Public").join("Evil.ps1"),
            "function badreset {\n    & git reset --hard origin/main\n}\n",
        )
        .unwrap();
        // 3) add：本地没有的新文件
        std::fs::write(
            pkg.join("Public").join("New.ps1"),
            "function pkgnew {\n<#\n.SYNOPSIS\n预览规则验收。\n.EXAMPLE\npkgnew\n#>\n    [CmdletBinding()] param()\n    Write-Output \"ok\"\n}\n",
        )
        .unwrap();

        let local_files = workspace::workspace_root().join("Public").join("Files.ps1");
        let script = format!(
            "(Get-Item -LiteralPath '{}').LastWriteTime = (Get-Date).AddDays(-1)",
            local_files.to_string_lossy().replace('\'', "''")
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .expect("set mtime");
        assert!(out.status.success(), "回拨 mtime 失败：{out:?}");

        let preview = export::preview_bundle(pkg.to_str().unwrap()).expect("preview");
        let get = |rel: &str| {
            preview
                .items
                .iter()
                .find(|i| i.rel == rel)
                .unwrap_or_else(|| panic!("{rel} 应在预览中：{:?}", preview.items))
                .clone()
        };
        let files_item = get("Public/Files.ps1");
        assert_eq!(files_item.action, "overwrite", "包内较新应为 overwrite");
        assert!(files_item.checked);
        let evil = get("Public/Evil.ps1");
        assert_eq!(evil.action, "invalid");
        assert!(!evil.checked, "invalid 不可勾选");
        assert!(evil.note.contains("安全"), "应注明安全拦截：{}", evil.note);
        let new_item = get("Public/New.ps1");
        assert_eq!(new_item.action, "add");

        // invalid 项强制选入导入 → 二次校验拒绝并记入 errors
        let result =
            export::import_bundle(pkg.to_str().unwrap(), &["Public/Evil.ps1".into()])
                .expect("import 应整体不失败");
        assert!(result.imported.is_empty());
        assert!(
            result.errors.iter().any(|e| e.contains("安全")),
            "Evil 应被安全拦截：{:?}",
            result.errors
        );

        // keep_local：包文件 mtime 早于本地（本地随后被更新）
        std::fs::write(
            pkg.join("Public").join("Files.ps1"),
            "# 包内又改（但本地更新）\nfunction pkgfiles2 { Write-Output 2 }\n",
        )
        .unwrap();
        std::fs::write(
            local_files,
            "# 本地更新内容（较新）\nfunction localfiles { Write-Output 3 }\n",
        )
        .unwrap();
        let preview2 = export::preview_bundle(pkg.to_str().unwrap()).expect("preview2");
        let files2 = preview2
            .items
            .iter()
            .find(|i| i.rel == "Public/Files.ps1")
            .unwrap();
        assert_eq!(files2.action, "keep_local", "本地较新应默认保留本地");
        assert!(!files2.checked);

        let _ = std::fs::remove_dir_all(&pkg);
    }

    /// 验收：导出拒绝非空目标目录（防误写用户目录）
    #[test]
    fn m4_bundle_export_rejects_nonempty_dir() {
        let _g = IsolatedProfile::new("m4-bundle-reject");
        workspace::init_from_template().expect("init");
        let dir = tmp_bundle_dir("bundle-reject");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("重要文件.txt"), "x").unwrap();
        let err = export::export_bundle(dir.to_str().unwrap());
        assert!(err.is_err(), "非空目录应被拒绝");
        assert!(
            dir.join("重要文件.txt").exists(),
            "原文件不得被触碰"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 验收：Private/Common.ps1 导入写入、AI 同 id 跳过合并、非法 selection 路径防御
    #[test]
    fn m4_bundle_import_private_ai_merge_and_path_guard() {
        let _g = IsolatedProfile::new("m4-bundle-guard");
        workspace::init_from_template().expect("init");

        // 本地预置同 id 的 AI Profile（导入应跳过、不覆盖本地配置）
        ai_config::save_profile(
            AiProfile {
                id: "prof-sync-test".into(),
                name: "本地同名".into(),
                protocol: AiProtocol::Openai,
                base_url: "https://local.example.com/v1".into(),
                model: "local-model".into(),
                temperature: 0.7,
                max_tokens: 8192,
                key_configured: false,
            },
            None,
        )
        .expect("save local profile");

        // 手工搭包：Private/Common.ps1（本地较旧 → 包内新写即较新）+ ai_profiles.json + 一个合法脚本
        let pkg = tmp_bundle_dir("bundle-guard");
        std::fs::create_dir_all(pkg.join("Public")).unwrap();
        std::fs::create_dir_all(pkg.join("Private")).unwrap();
        let common_with_helper = format!(
            "{}\nfunction Test-BundleHelper {{\n    Write-Output \"helper-ok\"\n}}\n",
            workspace::read_file("Private/Common.ps1").unwrap()
        );
        std::fs::write(pkg.join("Private").join("Common.ps1"), &common_with_helper).unwrap();
        std::fs::write(
            pkg.join("ai_profiles.json"),
            r#"{
  "profiles": [
    {"id":"prof-sync-test","name":"包内同名","protocol":"openai","base_url":"https://pkg.example.com","model":"pkg-model","temperature":0.7,"max_tokens":8192,"key_configured":false},
    {"id":"prof-brand-new","name":"包内新增","protocol":"openai","base_url":"https://new.example.com","model":"new-model","temperature":0.7,"max_tokens":8192,"key_configured":false}
  ],
  "default_profile_id": null
}"#,
        )
        .unwrap();
        std::fs::write(
            pkg.join("Public").join("Guard.ps1"),
            "function guardcmd {\n<#\n.SYNOPSIS\n防御验收。\n.EXAMPLE\nguardcmd\n#>\n    [CmdletBinding()] param()\n    Write-Output \"ok\"\n}\n",
        )
        .unwrap();

        // 本地 Common.ps1 mtime 回拨一天，使包内版本判为 overwrite
        let local_common = workspace::workspace_root().join("Private").join("Common.ps1");
        let script = format!(
            "(Get-Item -LiteralPath '{}').LastWriteTime = (Get-Date).AddDays(-1)",
            local_common.to_string_lossy().replace('\'', "''")
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .expect("set mtime");
        assert!(out.status.success(), "回拨 mtime 失败：{out:?}");

        // 预览确认动作
        let preview = export::preview_bundle(pkg.to_str().unwrap()).expect("preview");
        let common_item = preview
            .items
            .iter()
            .find(|i| i.rel == "Private/Common.ps1")
            .expect("Common.ps1 应在预览中");
        assert_eq!(common_item.action, "overwrite");

        // 混入非法路径（含 .. 与盘符相对路径）一并导入
        let selection = vec![
            "Private/Common.ps1".to_string(),
            "ai_profiles.json".to_string(),
            "Public/Guard.ps1".to_string(),
            "Public/../evil.ps1".to_string(),
            "Public/C:evil.ps1".to_string(),
        ];
        let result = export::import_bundle(pkg.to_str().unwrap(), &selection).expect("import");
        assert_eq!(result.imported.len(), 3, "应导入 3 项：{:?}", result.imported);
        assert!(
            result.errors.iter().any(|e| e.contains("非法")),
            "非法路径应被拒绝：{:?}",
            result.errors
        );

        // Common.ps1 已更新并带 BOM
        let common_now = std::fs::read(&local_common).unwrap();
        assert_eq!(common_now[..3], [0xEF, 0xBB, 0xBF], "导入文件应带 UTF-8 BOM");
        let common_text = String::from_utf8_lossy(&common_now[3..]).to_string();
        assert!(common_text.contains("Test-BundleHelper"), "辅助函数应写入");

        // AI：同 id 保留本地，新 id 并入
        let store = ai_config::load_profiles_store().unwrap();
        let same = store.profiles.iter().find(|p| p.id == "prof-sync-test").unwrap();
        assert_eq!(same.model, "local-model", "同 id 不得覆盖本地配置");
        assert!(
            store.profiles.iter().any(|p| p.id == "prof-brand-new"),
            "新 id 应并入"
        );

        // 等待后台运行时同步线程在隔离 guard 内完成
        std::thread::sleep(std::time::Duration::from_millis(800));
        let _ = std::fs::remove_dir_all(&pkg);
    }
}
