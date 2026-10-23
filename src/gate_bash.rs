//! `terry gate bash`: the PreToolUse hook for Bash calls. It stops Claude before it commits,
//! pushes or changes a PR, and shows the user the diff.
//!
//! For a gated command the gate saves the full patch under `~/.claude/diffs/` and answers
//! "ask", so the call waits for the user in the permission prompt. Other commands pass through
//! with no output.
//!
//! Before it asks, the gate lints the lines the change adds (see `lint.rs`). If lint finds
//! blocking problems, it answers "deny" with the problem list for Claude, and marks the repo so
//! every fix edit asks the user (see `lint_fix.rs`). Claude then runs the git command again.
//!
//! While the terry pane plugin is on, a clean diff first goes to the pane: the gate answers
//! "deny" and tells Claude to wait, and the pane shows the diff with Approve and Reject. The
//! user can scroll the pane, which a permission prompt would block. Approve writes an approval
//! for that exact review; on the next try the gate uses it up and answers "ask" with a summary
//! (see `approvals.rs`). Only the user can skip the step for good: `DIFF_GATE_PANE_REVIEW=0`.
//!
//! The gate fails closed: if the diff cannot be built, or the input cannot be read, it still
//! answers "ask" and puts the error in the prompt. In a folder the user excluded with
//! `terry exclude`, it lets git and gh actions through (see `config.rs`).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::approvals::{self, ApprovalVia};
use crate::config;
use crate::hook::{self, Decision};
use crate::lint_fix;
use crate::manifest::{self, ReviewStage};
use crate::paths;
use crate::prompt;
use crate::review::{self, ActionReview};
use crate::shell;

// Claude Code names an installed plugin "<plugin>@<marketplace>".
const PANE_PLUGIN_PREFIX: &str = "terry@";

pub fn run() {
    hook::guard(answer, |message| {
        hook::decide(
            Decision::Ask,
            &format!("Diff gate crashed ({message}). Review the command by hand."),
        );
    });
}

fn answer() {
    let call = hook::read_call();
    let Some(command) = call
        .as_ref()
        .ok()
        .and_then(|call| call["tool_input"]["command"].as_str())
    else {
        let problem = call
            .err()
            .unwrap_or_else(|| "no tool_input.command".to_owned());
        hook::decide(
            Decision::Ask,
            &format!(
                "Diff gate could not read the tool call ({problem}). Review the command by hand."
            ),
        );
        return;
    };
    let call = call.as_ref().expect("checked above");
    let cwd = call["cwd"]
        .as_str()
        .filter(|cwd| !cwd.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_default());

    if approvals::names_approvals(command) {
        hook::decide(Decision::Deny, approvals::DENIAL);
        return;
    }
    if config::names_config(command) || shell::runs_terry_exclude(command) {
        hook::decide(Decision::Deny, config::DENIAL);
        return;
    }

    let mut actions = shell::find_gated_actions(command, &cwd);
    actions.retain(|action| !config::is_excluded(&action.repo_dir));
    if actions.is_empty() {
        gate_auto_fix(command, &cwd);
        return;
    }

    let reviews: Vec<ActionReview> = actions.into_iter().map(review::review_action).collect();
    let failing: Vec<&ActionReview> = reviews
        .iter()
        .filter(|review| {
            review
                .lint
                .as_ref()
                .is_some_and(|lint| !lint.blocking_problems().is_empty())
        })
        .collect();
    if !failing.is_empty() {
        for review in &failing {
            if let (Some(result), Some(lint)) = (&review.result, &review.lint) {
                lint_fix::set_pending(&result.repo_root, command, lint.blocking_problems().len());
            }
        }
        hook::decide(Decision::Deny, &prompt::lint_denial(command, &failing));
        return;
    }
    for review in &reviews {
        if let (Some(result), Some(_)) = (&review.result, &review.lint) {
            lint_fix::clear_pending(&result.repo_root);
        }
    }

    let tool_use_id = call["tool_use_id"].as_str().filter(|id| !id.is_empty());
    let key = manifest::review_key(command, &cwd, &reviews);
    // Without a call id the pane cannot find the review, so the prompt is the only review.
    let Some(tool_use_id) = tool_use_id else {
        hook::decide(Decision::Ask, &prompt::full_review(&reviews));
        return;
    };
    if pane_review_enabled() {
        match approvals::take_approval(&key) {
            None => {
                // If the pane cannot get the review, holding the command would leave the user
                // no way to approve it; the full prompt review is the fallback.
                if manifest::write(tool_use_id, command, &reviews, ReviewStage::Pane, &key).is_ok()
                {
                    hook::decide(Decision::Deny, &prompt::pane_review_request(command));
                    return;
                }
            }
            Some(ApprovalVia::Pane) => {
                // The pane only shows this manifest; the prompt works without it.
                let _ = manifest::write(tool_use_id, command, &reviews, ReviewStage::Prompt, &key);
                hook::decide(Decision::Ask, &prompt::approved_summary(&reviews));
                return;
            }
            Some(ApprovalVia::Prompt) => {}
        }
    }
    let _ = manifest::write(
        tool_use_id,
        command,
        &reviews,
        ReviewStage::PromptOnly,
        &key,
    );
    hook::decide(Decision::Ask, &prompt::full_review(&reviews));
}

/// True when the terry pane plugin is on, so the user can approve the diff there.
fn pane_review_enabled() -> bool {
    if env::var("DIFF_GATE_PANE_REVIEW").is_ok_and(|value| value == "0") {
        return false;
    }
    let Some(settings) = fs::read_to_string(paths::claude_settings_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return false;
    };
    settings["enabledPlugins"]
        .as_object()
        .is_some_and(|plugins| {
            plugins.iter().any(|(name, enabled)| {
                name.starts_with(PANE_PLUGIN_PREFIX) && enabled == &Value::Bool(true)
            })
        })
}

/// While a lint fix is pending, a command that can rewrite files asks, as file edits do.
fn gate_auto_fix(command: &str, cwd: &Path) {
    if !shell::is_auto_fix_command(command) || config::is_excluded(cwd) {
        return;
    }
    if let Some(repo) = lint_fix::pending_repo_for(cwd) {
        let name = repo
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        hook::decide(
            Decision::Ask,
            &format!(
                "Lint fix pending in {name}: this command can rewrite files. Approve it only if you want this automatic fix; the diff gate reviews the result before any git action."
            ),
        );
    }
}
