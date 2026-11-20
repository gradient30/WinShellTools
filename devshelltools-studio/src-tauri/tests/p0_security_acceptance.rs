use devshelltools_studio_lib::safety;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn git_global_options_cannot_bypass_hard_reset_policy() {
    let report = safety::check("git -c x=y reset --hard").expect("safety");
    assert!(!report.ok, "global git options must not hide reset --hard");
}

#[test]
fn dynamic_command_invocation_is_rejected() {
    let report = safety::check("$cmd = 'git'; & $cmd reset --hard").expect("safety");
    assert!(
        !report.ok,
        "dynamic command expressions require danger mode"
    );
}

#[test]
fn explicit_force_push_is_rejected() {
    let report = safety::check("git push --force origin main").expect("safety");
    assert!(!report.ok);
}

#[test]
fn gpp_rejects_force_before_git_runs() {
    for manifest in module_manifests() {
        let fake_bin = temp_dir("gpp-force");
        std::fs::write(fake_bin.join("git.cmd"), "@exit /b 0\r\n").expect("fake git");
        let script = format!(
            "$env:PATH='{}'; Import-Module '{}' -Force; try {{ gpp --force; exit 9 }} catch {{ exit 0 }}",
            ps_quote(&fake_bin.to_string_lossy()),
            ps_quote(&manifest.to_string_lossy())
        );
        assert_eq!(
            run_powershell(&script),
            0,
            "gpp --force must fail before invoking git for {}",
            manifest.display()
        );
        let _ = std::fs::remove_dir_all(fake_bin);
    }
}

#[test]
fn lpr_clean_user_is_not_a_supported_action() {
    for manifest in module_manifests() {
        let script = format!(
            "Import-Module '{}' -Force; \
             $values=(Get-Command lpr).Parameters['Action'].Attributes | \
               Where-Object {{ $_.ValidValues }} | ForEach-Object {{ $_.ValidValues }}; \
             if ($values -contains 'clean-user') {{ exit 9 }} exit 0",
            ps_quote(&manifest.to_string_lossy())
        );
        assert_eq!(
            run_powershell(&script),
            0,
            "lpr must not expose clean-user for {}",
            manifest.display()
        );
    }
}

fn module_manifests() -> Vec<PathBuf> {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    vec![
        crate_dir.join("..").join("..").join("DevShellTools.psd1"),
        crate_dir
            .join("..")
            .join("templates")
            .join("DevShellTools.psd1"),
    ]
}

fn temp_dir(prefix: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "dst-{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).expect("temp dir");
    path
}

fn ps_quote(value: &str) -> String {
    value.replace('\'', "''")
}

fn run_powershell(script: &str) -> i32 {
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
        ])
        .arg(script)
        .status()
        .expect("powershell")
        .code()
        .unwrap_or(-1)
}
