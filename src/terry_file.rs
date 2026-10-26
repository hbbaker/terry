//! The project's `.terry` file, at the repo root:
//! `{"lint": {"exclude": ["vendor/", "*.generated.py", "/docs/**"]}}`.
//!
//! Files that match a `lint.exclude` pattern (see `glob`) are not linted. An unreadable or
//! invalid file excludes nothing, so a broken file leaves lint on; the lint report says why.
//! Because the file can switch lint off, an edit to it asks (`gate_edit`), and a commit that
//! changes it carries a note in its review.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde_json::Value;

use crate::glob::Glob;

pub const FILE_NAME: &str = ".terry";
pub const EDIT_PROMPT: &str = "terry: .terry can switch lint off for files. Approve this edit only if it is the change you want.";
pub const CHANGED_NOTE: &str = "NOTE: this change edits .terry, which decides which files are not linted. Check its lint.exclude list.";
const LINT_KEY: &str = "lint";
const EXCLUDE_KEY: &str = "exclude";

#[derive(Debug, Default)]
pub struct TerryFile {
    lint_exclude: Vec<Glob>,
}

impl TerryFile {
    /// Reads `.terry` in `repo_root`. A missing file gives the default, which excludes nothing.
    pub fn read(repo_root: &Path) -> Result<TerryFile, String> {
        let path = repo_root.join(FILE_NAME);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(TerryFile::default()),
            Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
        };
        TerryFile::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn parse(text: &str) -> Result<TerryFile, String> {
        let Value::Object(root) =
            serde_json::from_str(text).map_err(|error| format!("not valid JSON: {error}"))?
        else {
            return Err("not a JSON object".to_owned());
        };
        let Some(lint) = root.get(LINT_KEY) else {
            return Ok(TerryFile::default());
        };
        let Some(lint) = lint.as_object() else {
            return Err(format!("\"{LINT_KEY}\" is not an object"));
        };
        let Some(exclude) = lint.get(EXCLUDE_KEY) else {
            return Ok(TerryFile::default());
        };
        let Some(entries) = exclude.as_array() else {
            return Err(format!("\"{LINT_KEY}.{EXCLUDE_KEY}\" is not a list"));
        };
        let lint_exclude = entries
            .iter()
            .map(|entry| {
                let pattern = entry.as_str().ok_or_else(|| {
                    format!("\"{LINT_KEY}.{EXCLUDE_KEY}\" holds a value that is not a string")
                })?;
                Glob::new(pattern).map_err(|error| {
                    format!("\"{LINT_KEY}.{EXCLUDE_KEY}\" entry \"{pattern}\": {error}")
                })
            })
            .collect::<Result<Vec<Glob>, String>>()?;
        Ok(TerryFile { lint_exclude })
    }

    /// True when `path`, relative to the repo root, matches a `lint.exclude` pattern.
    pub fn excludes_from_lint(&self, path: &str) -> bool {
        self.lint_exclude.iter().any(|glob| glob.matches(path))
    }
}

/// True when `patch` changes the `.terry` file at the repo root.
pub fn is_changed_by(patch: &str) -> bool {
    let header = format!("diff --git a/{FILE_NAME} b/{FILE_NAME}");
    patch.lines().any(|line| line == header)
}
