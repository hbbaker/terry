//! `terry gate edit`: the PreToolUse hook for file edits.
//!
//! While a lint fix is pending in a repo, every edit there asks; the user approves each fix in
//! the normal edit prompt, which shows the diff. It also denies every edit to the approvals
//! folder, which only the terry pane writes, and to terry's config file, which only the user
//! changes.
//!
//! This hook fails open (a crash lets the edit follow the normal permission rules), because it
//! runs on every edit in every project; the Bash gate still reviews the result before any git
//! action.

use std::path::PathBuf;

use serde_json::Value;

use crate::approvals;
use crate::config;
use crate::hook::{self, Decision};
use crate::lint_fix;

const PATH_INPUT_KEYS: [&str; 2] = ["file_path", "notebook_path"];

pub fn run() {
    hook::guard(answer, |_| {});
}

fn edited_path(call: &Value) -> Option<PathBuf> {
    let cwd = call["cwd"].as_str().unwrap_or(".");
    PATH_INPUT_KEYS
        .iter()
        .find_map(|key| {
            call["tool_input"][key]
                .as_str()
                .filter(|path| !path.is_empty())
        })
        .map(|path| PathBuf::from(cwd).join(path))
}

fn answer() {
    let Some(path) = hook::read_call().ok().as_ref().and_then(edited_path) else {
        return;
    };
    if approvals::is_approvals_path(&path) {
        hook::decide(Decision::Deny, approvals::DENIAL);
        return;
    }
    if config::is_config_path(&path) {
        hook::decide(Decision::Deny, config::DENIAL);
        return;
    }
    if config::is_excluded(&path) {
        return;
    }
    let Some(repo_root) = lint_fix::pending_repo_for(&path) else {
        return;
    };
    let name = repo_root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    hook::decide(
        Decision::Ask,
        &format!(
            "Lint fix pending in {name}: approve this edit only if it is the fix you want. The git action runs again after the fixes."
        ),
    );
}
