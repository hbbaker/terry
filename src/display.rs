//! `terry config`: the display settings in effect, as JSON. The pane plugin reads them to draw
//! the Files pane.
//!
//! They live in the `display` section of terry's config file (see `config`):
//! `{"display": {"name": "Terry", "paneColumns": 34, "colors": {"added": "success", ...}}}`.
//! Every key is optional. A value that does not pass its check is replaced by the default and
//! named in `problems`, so a typo in the file can never leave the pane without a name or a
//! colour it can draw.

use std::process::ExitCode;

use serde_json::{Map, Value, json};

use crate::config;

const DISPLAY_KEY: &str = "display";
const MAX_NAME_CHARS: usize = 24;
const MIN_PANE_COLUMNS: u64 = 24;
const MAX_PANE_COLUMNS: u64 = 100;
const MAX_BRANCH_COLORS: usize = 12;
/// The colour names of Claude Code's theme. The plugin hands a colour on as it is written, and
/// the engine knows these by name.
const THEME_KEYS: [&str; 23] = [
    "text",
    "inverseText",
    "inactive",
    "subtle",
    "suggestion",
    "remember",
    "success",
    "error",
    "warning",
    "merged",
    "claude",
    "permission",
    "planMode",
    "autoAccept",
    "promptBorder",
    "bashBorder",
    "ide",
    "diffAdded",
    "diffRemoved",
    "diffAddedDimmed",
    "diffRemovedDimmed",
    "diffAddedWord",
    "diffRemovedWord",
];

/// The colours of the Files pane. Each is `#RRGGBB` or a theme colour name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Colors {
    pub added: String,
    pub modified: String,
    pub deleted: String,
    pub untracked: String,
    /// Claude's own colour: the file it edits, and its mark.
    pub claude: String,
    /// The owl and his line while he watches.
    pub owl: String,
    /// The owl and his line while he holds a command.
    pub alert: String,
    pub folder: String,
    /// One colour per layer of the branch chain, from the default branch down.
    pub branches: Vec<String>,
}

impl Default for Colors {
    fn default() -> Colors {
        Colors {
            added: "success".to_owned(),
            modified: "warning".to_owned(),
            deleted: "error".to_owned(),
            untracked: "#7FD6D6".to_owned(),
            claude: "claude".to_owned(),
            owl: "#8FB8ED".to_owned(),
            alert: "warning".to_owned(),
            folder: "#8FB8ED".to_owned(),
            branches: [
                "#8FB8ED", "#9BD39B", "#F2C879", "#E49AD0", "#7FD6D6", "#F29E8E",
            ]
            .map(str::to_owned)
            .to_vec(),
        }
    }
}

/// What the pane shows of terry, and how wide it asks to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Display {
    pub name: String,
    pub pane_columns: u16,
    pub colors: Colors,
}

impl Default for Display {
    fn default() -> Display {
        Display {
            name: "Terry".to_owned(),
            pane_columns: 34,
            colors: Colors::default(),
        }
    }
}

/// The settings in effect, and what was wrong in the file.
#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub display: Display,
    pub problems: Vec<String>,
}

impl Report {
    /// The JSON `terry config` prints, and the plugin reads.
    pub fn to_json(&self) -> Value {
        let colors = &self.display.colors;
        json!({
            "name": self.display.name,
            "paneColumns": self.display.pane_columns,
            "colors": {
                "added": colors.added,
                "modified": colors.modified,
                "deleted": colors.deleted,
                "untracked": colors.untracked,
                "claude": colors.claude,
                "owl": colors.owl,
                "alert": colors.alert,
                "folder": colors.folder,
                "branches": colors.branches,
            },
            "problems": self.problems,
        })
    }
}

/// `terry config`: prints the settings in effect. It always succeeds: trouble in the file shows
/// in `problems`, and the defaults stand in for it.
pub fn run() -> ExitCode {
    println!("{}", load().to_json());
    ExitCode::SUCCESS
}

/// The settings from terry's config file. A missing file is the defaults with no problem.
pub fn load() -> Report {
    match config::read_config() {
        Ok(config) => from_config(&config),
        Err(error) => Report {
            display: Display::default(),
            problems: vec![format!("{error}; using the defaults")],
        },
    }
}

/// The settings in the text of a config file.
pub fn parse(text: &str) -> Report {
    match serde_json::from_str(text) {
        Ok(Value::Object(config)) => from_config(&config),
        Ok(_) => Report {
            display: Display::default(),
            problems: vec!["the config is not a JSON object; using the defaults".to_owned()],
        },
        Err(error) => Report {
            display: Display::default(),
            problems: vec![format!(
                "the config is not valid JSON: {error}; using the defaults"
            )],
        },
    }
}

fn from_config(config: &Map<String, Value>) -> Report {
    let mut problems = Vec::new();
    let display = match config.get(DISPLAY_KEY) {
        None => Display::default(),
        Some(Value::Object(section)) => from_section(section, &mut problems),
        Some(_) => {
            problems.push(format!(
                "\"{DISPLAY_KEY}\" is not an object; using the defaults"
            ));
            Display::default()
        }
    };
    Report { display, problems }
}

fn from_section(section: &Map<String, Value>, problems: &mut Vec<String>) -> Display {
    let defaults = Display::default();
    Display {
        name: pick(
            section.get("name"),
            "name",
            check_name,
            defaults.name,
            problems,
        ),
        pane_columns: pick(
            section.get("paneColumns"),
            "paneColumns",
            check_columns,
            defaults.pane_columns,
            problems,
        ),
        colors: colors_from(section.get("colors"), problems),
    }
}

fn colors_from(value: Option<&Value>, problems: &mut Vec<String>) -> Colors {
    let defaults = Colors::default();
    let section = match value {
        None => return defaults,
        Some(Value::Object(section)) => section,
        Some(_) => {
            problems.push("\"display.colors\" is not an object; using the defaults".to_owned());
            return defaults;
        }
    };
    let mut color = |key: &str, default: String| {
        pick(
            section.get(key),
            &format!("colors.{key}"),
            check_color,
            default,
            problems,
        )
    };
    Colors {
        added: color("added", defaults.added),
        modified: color("modified", defaults.modified),
        deleted: color("deleted", defaults.deleted),
        untracked: color("untracked", defaults.untracked),
        claude: color("claude", defaults.claude),
        owl: color("owl", defaults.owl),
        alert: color("alert", defaults.alert),
        folder: color("folder", defaults.folder),
        branches: pick(
            section.get("branches"),
            "colors.branches",
            check_branches,
            defaults.branches,
            problems,
        ),
    }
}

/// The checked value of `key`, or `default` when the key is missing or its check fails. A failed
/// check adds a line to `problems`. `T` is whatever the check produces: text, a number, a list.
fn pick<T>(
    value: Option<&Value>,
    key: &str,
    check: fn(&Value) -> Result<T, String>,
    default: T,
    problems: &mut Vec<String>,
) -> T {
    let Some(value) = value else {
        return default;
    };
    match check(value) {
        Ok(checked) => checked,
        Err(reason) => {
            problems.push(format!("{DISPLAY_KEY}.{key}: {reason}; using the default"));
            default
        }
    }
}

/// The name: text of 1 to `MAX_NAME_CHARS` characters, with no control characters, which would
/// break the pane's lines.
fn check_name(value: &Value) -> Result<String, String> {
    let name = value.as_str().ok_or("must be text")?.trim();
    let length = name.chars().count();
    if length == 0 || length > MAX_NAME_CHARS {
        return Err(format!(
            "must be 1 to {MAX_NAME_CHARS} characters, not {length}"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("must not hold control characters".to_owned());
    }
    Ok(name.to_owned())
}

/// The pane width: a whole number from `MIN_PANE_COLUMNS` to `MAX_PANE_COLUMNS`. Claude Code
/// keeps a docked pane to at most 45% of the terminal, so a wide value is a request.
fn check_columns(value: &Value) -> Result<u16, String> {
    let columns = value.as_u64().ok_or("must be a whole number")?;
    if !(MIN_PANE_COLUMNS..=MAX_PANE_COLUMNS).contains(&columns) {
        return Err(format!(
            "must be {MIN_PANE_COLUMNS} to {MAX_PANE_COLUMNS}, not {columns}"
        ));
    }
    u16::try_from(columns).map_err(|error| error.to_string())
}

/// A colour: `#RRGGBB`, or the name of a colour in Claude Code's theme.
fn check_color(value: &Value) -> Result<String, String> {
    let color = value.as_str().ok_or("must be text")?;
    if is_hex_color(color) || THEME_KEYS.contains(&color) {
        return Ok(color.to_owned());
    }
    Err(format!("\"{color}\" is not #RRGGBB or a theme colour name"))
}

fn is_hex_color(text: &str) -> bool {
    text.strip_prefix('#').is_some_and(|digits| {
        digits.len() == 6 && digits.chars().all(|digit| digit.is_ascii_hexdigit())
    })
}

/// The branch chain's colours: 1 to `MAX_BRANCH_COLORS` colours, each one valid. One bad entry
/// rejects the list, so the layers keep their default order.
fn check_branches(value: &Value) -> Result<Vec<String>, String> {
    let entries = value.as_array().ok_or("must be a list")?;
    if entries.is_empty() || entries.len() > MAX_BRANCH_COLORS {
        return Err(format!(
            "must hold 1 to {MAX_BRANCH_COLORS} colours, not {}",
            entries.len()
        ));
    }
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            check_color(entry).map_err(|reason| format!("entry {index} {reason}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_trim_the_name_when_it_has_spaces_around_it() {
        assert_eq!(check_name(&json!("  Hedwig ")), Ok("Hedwig".to_owned()));
    }

    #[test]
    fn should_reject_the_name_when_it_is_empty_or_too_long() {
        assert!(check_name(&json!("   ")).is_err());
        assert!(check_name(&json!("x".repeat(MAX_NAME_CHARS + 1))).is_err());
        assert!(check_name(&json!("x".repeat(MAX_NAME_CHARS))).is_ok());
    }

    #[test]
    fn should_count_characters_not_bytes_when_the_name_is_not_ascii() {
        assert!(check_name(&json!("é".repeat(MAX_NAME_CHARS))).is_ok());
    }

    #[test]
    fn should_reject_the_name_when_it_is_not_text_or_holds_a_control_character() {
        assert!(check_name(&json!(7)).is_err());
        assert!(check_name(&json!("two\nlines")).is_err());
    }

    #[test]
    fn should_accept_the_columns_when_they_are_in_range() {
        assert_eq!(check_columns(&json!(24)), Ok(24));
        assert_eq!(check_columns(&json!(100)), Ok(100));
    }

    #[test]
    fn should_reject_the_columns_when_they_are_out_of_range_or_not_whole() {
        assert!(check_columns(&json!(23)).is_err());
        assert!(check_columns(&json!(101)).is_err());
        assert!(check_columns(&json!(34.5)).is_err());
        assert!(check_columns(&json!("34")).is_err());
        assert!(check_columns(&json!(-5)).is_err());
    }

    #[test]
    fn should_accept_a_color_when_it_is_hex_or_a_theme_name() {
        assert_eq!(check_color(&json!("#8fB8ed")), Ok("#8fB8ed".to_owned()));
        assert_eq!(check_color(&json!("claude")), Ok("claude".to_owned()));
    }

    #[test]
    fn should_reject_a_color_when_it_is_neither() {
        assert!(check_color(&json!("#8FB8E")).is_err());
        assert!(check_color(&json!("#8FB8EG")).is_err());
        assert!(check_color(&json!("8FB8ED")).is_err());
        assert!(check_color(&json!("orange")).is_err());
        assert!(check_color(&json!(3)).is_err());
    }

    #[test]
    fn should_name_the_entry_when_one_branch_color_is_bad() {
        let error = check_branches(&json!(["#8FB8ED", "nope"])).unwrap_err();
        assert!(error.starts_with("entry 1 "), "{error}");
    }

    #[test]
    fn should_reject_the_branch_colors_when_the_list_is_empty_or_too_long() {
        assert!(check_branches(&json!([])).is_err());
        assert!(check_branches(&json!(vec!["claude"; MAX_BRANCH_COLORS + 1])).is_err());
        assert!(check_branches(&json!(vec!["claude"; MAX_BRANCH_COLORS])).is_ok());
    }

    #[test]
    fn should_use_the_default_and_note_the_problem_when_a_check_fails() {
        let mut problems = Vec::new();

        let name = pick(
            Some(&json!(5)),
            "name",
            check_name,
            "Terry".to_owned(),
            &mut problems,
        );

        assert_eq!(name, "Terry");
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("display.name: "), "{}", problems[0]);
    }

    #[test]
    fn should_use_the_default_and_note_nothing_when_the_key_is_missing() {
        let mut problems = Vec::new();

        let name = pick(None, "name", check_name, "Terry".to_owned(), &mut problems);

        assert_eq!(name, "Terry");
        assert!(problems.is_empty());
    }
}
