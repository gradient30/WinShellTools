mod common;

use common::IsolatedProfile;
use devshelltools_studio_lib::{sync, workspace};

#[test]
fn workspace_writes_and_regeneration_leave_no_pending_transaction() {
    let _guard = IsolatedProfile::new("p1-transaction");
    workspace::init_from_template().expect("init");

    let category = r#"
<#!
@DST-Category
Name: txn
Title: Transaction
Description: test
Aliases: txn
@DST-Category-End
#>

function txnok {
<#
.SYNOPSIS
test
.EXAMPLE
txnok
#>
    Write-Output ok
}
"#;
    workspace::write_file("Public/Transaction.ps1", category).expect("write category");
    sync::regenerate_all().expect("regenerate");

    assert!(workspace::read_file("Public/Transaction.ps1")
        .expect("read")
        .contains("function txnok"));
    assert!(workspace::read_file("Public/Help.ps1")
        .expect("help")
        .contains("'txn'"));
    assert!(workspace::recover_pending_transactions()
        .expect("recover")
        .is_empty());
}
