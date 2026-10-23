//! The per-call manifest the terry pane reads: `~/.claude/diffs/calls/<tool_use_id>.json`.
//! Its format is shared with `plugin/hooks/register.tsx`.

use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use crate::paths;
use crate::prompt::{lint_summary, section_message};
use crate::review::ActionReview;
use crate::sha256;
use crate::text::{safe_file_name, strip_ansi};

const MANIFEST_VERSION: u64 = 2;
const MANIFEST_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);

/// Where the user reviews this call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewStage {
    /// In the terry pane, with Approve and Reject; the command is held.
    Pane,
    /// The rerun after a pane approval: the permission prompt confirms it with a summary.
    Prompt,
    /// The permission prompt alone, with the full review: no pane step, or no pane can be drawn.
    PromptOnly,
}

impl ReviewStage {
    fn as_str(self) -> &'static str {
        match self {
            ReviewStage::Pane => "pane",
            ReviewStage::Prompt => "prompt",
            ReviewStage::PromptOnly => "prompt_only",
        }
    }
}

/// Names one exact review (command, folder and every patch), so an approval does not carry
/// over to a changed diff.
pub fn review_key(command: &str, cwd: &Path, reviews: &[ActionReview]) -> String {
    let mut parts: Vec<String> = vec![command.to_owned(), cwd.display().to_string()];
    parts.extend(reviews.iter().map(|review| match &review.result {
        Some(result) => result.patch.clone(),
        None => review.error.clone().unwrap_or_default(),
    }));
    let mut input = Vec::new();
    for part in parts {
        input.extend_from_slice(part.as_bytes());
        input.push(0);
    }
    sha256::hex_digest(&input)
}

pub fn write(
    tool_use_id: &str,
    command: &str,
    reviews: &[ActionReview],
    stage: ReviewStage,
    key: &str,
) -> io::Result<()> {
    let dir = paths::diffs_dir().join("calls");
    fs::create_dir_all(&dir)?;
    paths::remove_older_than(&dir, "json", MANIFEST_RETENTION);
    let manifest = json!({
        "version": MANIFEST_VERSION,
        "tool_use_id": tool_use_id,
        "command": command,
        "stage": stage.as_str(),
        "review_key": key,
        "sections": reviews.iter().map(section).collect::<Vec<Value>>(),
    });
    let path = dir.join(format!("{}.json", safe_file_name(tool_use_id)));
    fs::write(path, manifest.to_string())
}

/// One reviewed action, in plain text, as the pane reads it.
fn section(review: &ActionReview) -> Value {
    let label = review.action.kind.label();
    let Some(result) = &review.result else {
        return json!({
            "title": label,
            "notes": [strip_ansi(&section_message(review))],
            "lint": [],
            "patch_path": null,
        });
    };
    let lint: Vec<String> = review
        .lint
        .as_ref()
        .map(|lint| {
            lint_summary(lint)
                .iter()
                .map(|line| strip_ansi(line))
                .collect()
        })
        .unwrap_or_default();
    json!({
        "title": format!("{label}: {}", result.title),
        "notes": result.notes,
        "lint": lint,
        "patch_path": review.patch_path.as_ref().map(|path| path.display().to_string()),
    })
}
