use std::path::Path;

use serde_json::json;

use terry::install::*;

#[test]
fn should_replace_python_hooks_and_keep_other_settings() {
    let mut settings = json!({
        "model": "opus",
        "hooks": {"PreToolUse": [
            {"matcher": "Bash", "hooks": [{"type": "command", "command": "python3 /x/gate/diff-gate.py"}]},
            {"matcher": "Read", "hooks": [{"type": "command", "command": "other-hook"}]}
        ]},
        "permissions": {"ask": ["Bash(git push *)"], "allow": ["Read"]}
    });

    configure(&mut settings, Path::new("/opt/terry/terry")).expect("configures");

    let entries = settings["hooks"]["PreToolUse"].as_array().expect("a list");
    let commands: Vec<&str> = entries
        .iter()
        .map(|entry| entry["hooks"][0]["command"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        commands,
        vec![
            "other-hook",
            "/opt/terry/terry gate bash",
            "/opt/terry/terry gate edit"
        ]
    );
    assert_eq!(settings["model"], "opus");
    assert_eq!(settings["permissions"]["allow"], json!(["Read"]));
    assert_eq!(
        settings["permissions"]["ask"].as_array().map(Vec::len),
        Some(4)
    );
    assert!(
        !settings["permissions"]["ask"]
            .as_array()
            .is_some_and(|ask| ask.iter().any(|rule| rule == "Bash(git push *)"))
    );
}

#[test]
fn should_not_add_a_second_copy_when_run_again() {
    let mut settings = json!({});
    configure(&mut settings, Path::new("/opt/terry/terry")).expect("configures");
    configure(&mut settings, Path::new("/opt/terry/terry")).expect("configures again");

    assert_eq!(
        settings["hooks"]["PreToolUse"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        settings["permissions"]["ask"].as_array().map(Vec::len),
        Some(4)
    );
}

#[test]
fn should_quote_the_binary_path_when_it_has_a_space() {
    assert_eq!(
        hook_command(Path::new("/my tools/terry"), "bash"),
        "'/my tools/terry' gate bash"
    );
}

#[test]
fn should_refuse_settings_that_are_not_an_object() {
    assert!(configure(&mut json!([]), Path::new("/t")).is_err());
}

#[test]
fn should_restore_the_settings_it_found_when_unconfigured_after_configure() {
    let original = json!({
        "model": "opus",
        "hooks": {"PreToolUse": [
            {"matcher": "Read", "hooks": [{"type": "command", "command": "other-hook"}]}
        ]},
        "permissions": {"ask": ["Bash(rm *)"], "allow": ["Read"]}
    });
    let mut settings = original.clone();
    configure(&mut settings, Path::new("/opt/terry/terry")).expect("configures");

    let changed = unconfigure(&mut settings);

    assert!(changed);
    assert_eq!(settings, original);
}

#[test]
fn should_drop_the_hooks_and_permissions_it_emptied_when_unconfigured() {
    let mut settings = json!({"model": "opus"});
    configure(&mut settings, Path::new("/opt/terry/terry")).expect("configures");

    unconfigure(&mut settings);

    assert_eq!(settings, json!({"model": "opus"}));
}

#[test]
fn should_change_nothing_when_unconfigured_without_terry_settings() {
    let original = json!({"hooks": {}, "permissions": {"ask": []}});
    let mut settings = original.clone();

    let changed = unconfigure(&mut settings);

    assert!(!changed);
    assert_eq!(settings, original);
}
