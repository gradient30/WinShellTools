mod common;

use common::IsolatedProfile;
use devshelltools_studio_lib::{key_store, workspace};

#[test]
fn key_is_encrypted_and_legacy_plaintext_is_migrated() {
    let _guard = IsolatedProfile::new("p1-key-store");
    workspace::init_from_template().expect("init");
    let keys = workspace::studio_dir().join("ai_keys");
    std::fs::create_dir_all(&keys).expect("keys dir");

    let legacy = keys.join("prof-migrate.txt");
    std::fs::write(&legacy, "legacy-secret").expect("legacy");
    assert_eq!(
        key_store::load_secret("prof-migrate").expect("load legacy"),
        "legacy-secret"
    );
    assert!(
        !legacy.exists(),
        "legacy plaintext must be removed after migration"
    );
    assert!(keys.join("prof-migrate.dpapi").exists());

    key_store::save_secret("prof-new", "new-secret").expect("save");
    assert!(!keys.join("prof-new.txt").exists());
    assert_eq!(
        key_store::load_secret("prof-new").expect("load new"),
        "new-secret"
    );
}
