//! One reviewed action: its diff, the saved patch, and the lint report on its added lines.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use crate::diff::{self, DiffResult};
use crate::lint::{self, LintReport};
use crate::paths;
use crate::shell::{ActionKind, GatedAction};
use crate::stamp;
use crate::terry_file;
use crate::text::slug;

const PATCH_RETENTION: Duration = Duration::from_secs(14 * 24 * 60 * 60);

pub struct ActionReview {
    pub action: GatedAction,
    /// None for an action with no local diff, or one whose diff could not be built.
    pub result: Option<DiffResult>,
    pub error: Option<String>,
    pub lint: Option<LintReport>,
    pub patch_path: Option<PathBuf>,
}

pub fn review_action(action: GatedAction) -> ActionReview {
    let mut review = ActionReview {
        action,
        result: None,
        error: None,
        lint: None,
        patch_path: None,
    };
    let Some(built) = diff::build(&review.action) else {
        return review;
    };
    let saved = built.and_then(|result| {
        save_patch(&result, review.action.kind).map(|patch_path| (result, patch_path))
    });
    match saved {
        Ok((mut result, patch_path)) => {
            if terry_file::is_changed_by(&result.patch) {
                result.notes.push(terry_file::CHANGED_NOTE.to_owned());
            }
            if result.lintable && !result.patch.trim().is_empty() {
                review.lint = Some(lint::lint_patch(&result.repo_root, &result.patch));
            }
            review.result = Some(result);
            review.patch_path = Some(patch_path);
        }
        Err(error) => review.error = Some(format!("Could not build the diff: {error}")),
    }
    review
}

/// Saves the full patch as `~/.claude/diffs/<repo>-<branch>-<kind>-<UTC stamp>.patch`.
fn save_patch(result: &DiffResult, kind: ActionKind) -> Result<PathBuf, String> {
    let dir = paths::diffs_dir();
    fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    paths::remove_older_than(&dir, "patch", PATCH_RETENTION);
    let repo_name = result
        .repo_root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = format!(
        "{}-{}-{}-{}.patch",
        slug(&repo_name),
        slug(&result.branch),
        kind.slug(),
        stamp::utc_stamp()
    );
    let path = dir.join(name);
    fs::write(&path, &result.patch).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(path)
}
