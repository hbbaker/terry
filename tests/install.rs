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
