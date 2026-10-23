use std::process::Command;
use std::time::Duration;

use terry::process::*;

#[test]
fn should_return_stdout_and_exit_code_when_the_command_ends() {
    let done = run_captured(
        Command::new("sh").args(["-c", "cat; exit 3"]),
        Some("hello"),
        Duration::from_secs(5),
    )
    .expect("sh runs");
    assert_eq!(done.status, Some(3));
    assert_eq!(done.stdout, "hello");
}

#[test]
fn should_fail_when_the_command_outlives_its_time_limit() {
    let result = run_captured(
        Command::new("sleep").arg("5"),
        None,
        Duration::from_millis(100),
    );
    assert!(matches!(result, Err(RunError::TimedOut(_))));
}
