//! Approvals the terry pane plugin writes for a review the gate held.
//!
//! Each one is `~/.claude/diffs/approvals/<review key>.json`, written by the plugin alone:
//!
//! - `"via": "pane"`: the user pressed Approve in the Diff review pane. The rerun asks with a
//!   summary.
//! - `"via": "prompt"`: no window could draw the pane (an IDE). The rerun asks with the full
//!   review.
//!
//! Claude never chooses either. The gates deny Claude's shell commands and file edits that
//! name this folder, which blocks the plain ways to forge one. A forged approval still only
//! reaches the permission prompt, never past it.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::paths;

const APPROVAL_LIFETIME: Duration = Duration::from_secs(30 * 60);
pub const DENIAL: &str = "terry: only the Diff review pane writes or reads ~/.claude/diffs/approvals. Do not touch that folder. To get a held git or gh command approved, ask the user to answer in the pane.";

/// How the user will have reviewed the diff when the held command runs again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalVia {
    Pane,
    Prompt,
}

fn approvals_dir() -> PathBuf {
    paths::diffs_dir().join("approvals")
}

/// Uses up the approval of this review; `None` if there is none, or it is stale.
pub fn take_approval(key: &str) -> Option<ApprovalVia> {
    let path = approvals_dir().join(format!("{key}.json"));
    let changed = fs::metadata(&path).and_then(|meta| meta.modified()).ok()?;
    let content = fs::read_to_string(&path).unwrap_or_default();
    let _ = fs::remove_file(&path);
    let age = SystemTime::now()
        .duration_since(changed)
        .unwrap_or_default();
    (age <= APPROVAL_LIFETIME).then(|| approval_via(&content))
}

/// Reads the approval's route; anything unclear gets the full review in the prompt.
pub fn approval_via(content: &str) -> ApprovalVia {
    let via = serde_json::from_str::<Value>(content)
        .ok()
        .and_then(|approval| approval["via"].as_str().map(str::to_owned));
    if via.as_deref() == Some("pane") {
        ApprovalVia::Pane
    } else {
        ApprovalVia::Prompt
    }
}

/// True when `text` names the approvals folder (`diffs/approvals`, with any number of `/`).
pub fn names_approvals(text: &str) -> bool {
    text.match_indices("diffs/").any(|(index, marker)| {
        text[index + marker.len()..]
            .trim_start_matches('/')
            .starts_with("approvals")
    })
}

pub fn is_approvals_path(path: &Path) -> bool {
    paths::is_within(&paths::resolve(path), &paths::resolve(&approvals_dir()))
}
