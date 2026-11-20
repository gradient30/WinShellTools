use devshelltools_studio_lib::process_runner::{self, ProcessSpec};
use std::time::Duration;

#[test]
fn process_execution_is_bounded_by_timeout() {
    let result = process_runner::run(ProcessSpec::powershell(
        "Start-Sleep -Seconds 30",
        Duration::from_millis(200),
        1024,
    ));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("超时"));
}
