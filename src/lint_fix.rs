//! Repos with a pending lint fix.
//!
//! When the gate denies a git action because of lint, it marks the repo here. While the mark
//! is set, `terry gate edit` makes every file edit in that repo ask the user, and the Bash gate
//! does the same for auto-fix commands. The gate clears the mark when the repo lints clean.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::paths;
use crate::sha256;
use crate::stamp;

/// A mark older than this is stale (the fix was dropped), so it no longer gates edits.
const MARK_MAX_AGE_SECONDS: f64 = 24.0 * 60.0 * 60.0;
const MARK_NAME_CHARS: usize = 16;

fn state_dir() -> PathBuf {
    paths::diffs_dir().join("lint-fix")
}

fn mark_path(repo_root: &Path) -> PathBuf {
    let resolved = paths::resolve(repo_root).display().to_string();
    let digest = sha256::hex_digest(resolved.as_bytes());
    state_dir().join(format!("{}.json", &digest[..MARK_NAME_CHARS]))
}

pub fn set_pending(repo_root: &Path, command: &str, problem_count: usize) {
    let mark = json!({
        "repo_root": paths::resolve(repo_root).display().to_string(),
        "command": command,
        "problem_count": problem_count,
        "created_at": stamp::unix_seconds(),
    });
    // A mark that cannot be written only means the fix edits will not ask; the gate still
    // lints again before the git action.
    let _ = fs::create_dir_all(state_dir());
    let _ = fs::write(mark_path(repo_root), mark.to_string());
}

pub fn clear_pending(repo_root: &Path) {
    let _ = fs::remove_file(mark_path(repo_root));
}

/// The repo root with a live mark that holds `path`, if there is one.
pub fn pending_repo_for(path: &Path) -> Option<PathBuf> {
    let target = paths::resolve(path);
    let entries = fs::read_dir(state_dir()).ok()?;
    for entry in entries.flatten() {
        let mark_file = entry.path();
        if mark_file.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Some(mark) = fs::read_to_string(&mark_file)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        else {
            continue;
        };
        let created_at = mark["created_at"].as_f64().unwrap_or(0.0);
        if stamp::unix_seconds() - created_at > MARK_MAX_AGE_SECONDS {
            let _ = fs::remove_file(&mark_file);
            continue;
        }
        let Some(repo_root) = mark["repo_root"].as_str().map(PathBuf::from) else {
            continue;
        };
        if paths::is_within(&target, &repo_root) {
            return Some(repo_root);
        }
    }
    None
}
