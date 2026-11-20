use devshelltools_studio_lib::safety;

#[test]
fn git_global_options_cannot_bypass_hard_reset_policy() {
    let report = safety::check("git -c x=y reset --hard").expect("safety");
    assert!(!report.ok, "global git options must not hide reset --hard");
}

#[test]
fn dynamic_command_invocation_is_rejected() {
    let report = safety::check("$cmd = 'git'; & $cmd reset --hard").expect("safety");
    assert!(!report.ok, "dynamic command expressions require danger mode");
}

#[test]
fn explicit_force_push_is_rejected() {
    let report = safety::check("git push --force origin main").expect("safety");
    assert!(!report.ok);
}
