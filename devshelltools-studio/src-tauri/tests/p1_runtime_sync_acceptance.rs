mod common;

use common::IsolatedProfile;
use devshelltools_studio_lib::{runtime_sync, workspace};

#[test]
fn rapid_sync_requests_finish_at_latest_generation() {
    let _guard = IsolatedProfile::new("p1-runtime-sync");
    workspace::init_from_template().expect("init");

    let mut latest = 0;
    for _ in 0..20 {
        latest = runtime_sync::request_sync();
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let status = runtime_sync::sync_status();
        if status.state == runtime_sync::RuntimeSyncState::Succeeded && status.generation == latest
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "runtime sync did not reach generation {latest}: {status:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}
