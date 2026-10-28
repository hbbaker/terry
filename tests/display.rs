use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};
use terry::display::{Colors, Display, parse};

const TERRY: &str = env!("CARGO_BIN_EXE_terry");

static FOLDER_COUNT: AtomicUsize = AtomicUsize::new(0);

/// A throwaway `$XDG_CONFIG_HOME`, so no test touches the user's own config.
struct ConfigHome {
    path: PathBuf,
}

impl ConfigHome {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "terry-display-{}-{}",
            std::process::id(),
            FOLDER_COUNT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("terry")).expect("makes the config folder");
        ConfigHome { path }
    }

    fn config_file(&self) -> PathBuf {
        self.path.join("terry").join("config.json")
    }

    fn write(&self, text: &str) {
        fs::write(self.config_file(), text).expect("writes the config");
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(TERRY)
            .args(args)
            .env("XDG_CONFIG_HOME", &self.path)
            .output()
            .expect("runs terry")
    }
}

impl Drop for ConfigHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn should_return_the_defaults_when_the_config_has_no_display_section() {
    let report = parse(r#"{"exclude": ["/work"]}"#);

    assert_eq!(report.display, Display::default());
    assert!(report.problems.is_empty());
}

#[test]
fn should_return_the_defaults_when_the_display_section_is_empty() {
    let report = parse(r#"{"display": {}}"#);

    assert_eq!(report.display, Display::default());
    assert!(report.problems.is_empty());
}

#[test]
fn should_read_every_setting_when_the_section_is_complete() {
    let report = parse(
        r##"{"display": {
            "name": "Hedwig",
            "paneColumns": 40,
            "colors": {
                "added": "#00FF00", "modified": "#FFFF00", "deleted": "#FF0000",
                "untracked": "subtle", "claude": "#D77757", "owl": "#FFFFFF",
                "alert": "error", "folder": "text", "branches": ["#111111", "#222222"]
            }
        }}"##,
    );

    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(report.display.name, "Hedwig");
    assert_eq!(report.display.pane_columns, 40);
    assert_eq!(
        report.display.colors,
        Colors {
            added: "#00FF00".to_owned(),
            modified: "#FFFF00".to_owned(),
            deleted: "#FF0000".to_owned(),
            untracked: "subtle".to_owned(),
            claude: "#D77757".to_owned(),
            owl: "#FFFFFF".to_owned(),
            alert: "error".to_owned(),
            folder: "text".to_owned(),
            branches: vec!["#111111".to_owned(), "#222222".to_owned()],
        }
    );
}

#[test]
fn should_keep_the_good_settings_when_another_one_is_bad() {
    let report = parse(
        r##"{"display": {"name": "Hedwig", "paneColumns": 3, "colors": {"added": "#00FF00", "deleted": "red-ish"}}}"##,
    );

    assert_eq!(report.display.name, "Hedwig");
    assert_eq!(report.display.colors.added, "#00FF00");
    assert_eq!(report.display.pane_columns, Display::default().pane_columns);
    assert_eq!(report.display.colors.deleted, Colors::default().deleted);
}

#[test]
fn should_name_each_bad_setting_when_several_are_wrong() {
    let report = parse(
        r#"{"display": {"name": "", "paneColumns": 999, "colors": {"claude": 5, "branches": []}}}"#,
    );

    let keys = [
        "display.name",
        "display.paneColumns",
        "display.colors.claude",
        "display.colors.branches",
    ];
    assert_eq!(report.problems.len(), keys.len(), "{:?}", report.problems);
    for key in keys {
        assert!(
            report
                .problems
                .iter()
                .any(|problem| problem.starts_with(key)),
            "no problem names {key}: {:?}",
            report.problems
        );
    }
    assert_eq!(report.display, Display::default());
}

#[test]
fn should_return_the_defaults_and_say_why_when_the_config_is_not_json() {
    let report = parse("{ not json");

    assert_eq!(report.display, Display::default());
    assert_eq!(report.problems.len(), 1);
    assert!(
        report.problems[0].contains("not valid JSON"),
        "{}",
        report.problems[0]
    );
}

#[test]
fn should_return_the_defaults_and_say_why_when_display_is_not_an_object() {
    let report = parse(r#"{"display": "Hedwig"}"#);

    assert_eq!(report.display, Display::default());
    assert_eq!(report.problems.len(), 1);
}

#[test]
fn should_print_the_settings_as_json_when_terry_config_runs() {
    let home = ConfigHome::new();
    home.write(r#"{"display": {"name": "Hedwig", "paneColumns": 40}}"#);

    let output = home.run(&["config"]);

    assert!(output.status.success());
    let printed: Value = serde_json::from_slice(&output.stdout).expect("prints JSON");
    assert_eq!(printed["name"], "Hedwig");
    assert_eq!(printed["paneColumns"], 40);
    assert_eq!(printed["colors"]["claude"], "claude");
    assert_eq!(
        printed["colors"]["branches"].as_array().map(Vec::len),
        Some(6)
    );
    assert_eq!(printed["problems"], json!([]));
}

#[test]
fn should_print_the_defaults_when_there_is_no_config_file() {
    let home = ConfigHome::new();

    let output = home.run(&["config"]);

    assert!(output.status.success());
    let printed: Value = serde_json::from_slice(&output.stdout).expect("prints JSON");
    assert_eq!(printed["name"], "Terry");
    assert_eq!(printed["paneColumns"], 34);
    assert_eq!(printed["problems"], json!([]));
}

#[test]
fn should_report_a_problem_and_still_succeed_when_the_config_file_is_broken() {
    let home = ConfigHome::new();
    home.write("{ not json");

    let output = home.run(&["config"]);

    assert!(output.status.success());
    let printed: Value = serde_json::from_slice(&output.stdout).expect("prints JSON");
    assert_eq!(printed["name"], "Terry");
    assert_eq!(printed["problems"].as_array().map(Vec::len), Some(1));
}

#[test]
fn should_keep_the_display_section_when_terry_exclude_changes_the_list() {
    let home = ConfigHome::new();
    home.write(r#"{"display": {"name": "Hedwig"}}"#);
    let folder = home.path.join("project");
    fs::create_dir_all(&folder).expect("makes a folder to exclude");

    let output = home.run(&["exclude", folder.to_str().expect("a path is text")]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let config: Value =
        serde_json::from_str(&fs::read_to_string(home.config_file()).expect("reads"))
            .expect("is JSON");
    assert_eq!(config["display"]["name"], "Hedwig");
    assert_eq!(config["exclude"].as_array().map(Vec::len), Some(1));
}
