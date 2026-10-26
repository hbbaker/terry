//! The text the gate writes: the review the permission prompt shows ("ask"), and the
//! instructions Claude reads ("deny").
//!
//! Set `DIFF_GATE_COLOR=0` to turn off the ANSI colours in the prompt text.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::git;
use crate::lint::LintReport;
use crate::paths;
use crate::process;
use crate::review::ActionReview;
use crate::shell::ActionKind;
use crate::text::strip_ansi;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const CYAN: &str = "\x1b[36m";
/// One colour per layer of the branch chain, from the default branch down.
const LAYER_COLORS: [&str; 6] = [BLUE, GREEN, YELLOW, MAGENTA, CYAN, RED];
const BRANCH_SEPARATOR: &str = " -> ";
const MAX_PREVIEW_LINES: usize = 120;
const MAX_PREVIEW_LINE_CHARS: usize = 200;
const MAX_DENIAL_PROBLEMS: usize = 50;
const DELTA_TIMEOUT: Duration = Duration::from_secs(10);

fn color_on() -> bool {
    env::var("DIFF_GATE_COLOR").map_or(true, |value| value != "0")
}

fn paint(text: &str, styles: &[&str]) -> String {
    if !color_on() {
        return text.to_owned();
    }
    format!("{}{text}{RESET}", styles.concat())
}

/// The branch chain of `repo_dir` (`main -> dev -> subfeature`), one colour per layer and the
/// checked-out branch in bold. None when HEAD names no branch.
fn branch_line(repo_dir: &Path) -> Option<String> {
    let chain = git::branch_chain(repo_dir);
    let current = chain.len().checked_sub(1)?;
    let layers: Vec<String> = chain
        .iter()
        .enumerate()
        .map(|(depth, branch)| {
            let color = LAYER_COLORS[depth % LAYER_COLORS.len()];
            if depth == current {
                paint(branch, &[BOLD, color])
            } else {
                paint(branch, &[color])
            }
        })
        .collect();
    Some(format!(
        "branch: {}",
        layers.join(&paint(BRANCH_SEPARATOR, &[DIM]))
    ))
}

/// The permission prompt's text when it is the only review: every action, with its diff.
pub fn full_review(reviews: &[ActionReview]) -> String {
    let sections: Vec<String> = reviews.iter().map(review_section).collect();
    format!(
        "{}\n\n{}",
        paint("Diff review gate", &[BOLD]),
        sections.join("\n\n")
    )
}

fn message_section(kind: ActionKind, message: &str) -> String {
    format!(
        "{}\n{message}",
        paint(&format!("== {} ==", kind.label()), &[BOLD])
    )
}

fn review_section(review: &ActionReview) -> String {
    let kind = review.action.kind;
    match kind {
        ActionKind::ApiWrite => {
            return message_section(
                kind,
                "Writes to the GitHub API. No local diff. Check the endpoint and fields in the command.",
            );
        }
        ActionKind::Unparsed => {
            return message_section(
                kind,
                "Could not parse the command, and it looks like a git or gh write. Read it with care.",
            );
        }
        _ => {}
    }
    let Some(result) = &review.result else {
        return message_section(kind, &paint(review.error.as_deref().unwrap_or(""), &[RED]));
    };

    let mut parts = vec![paint(
        &format!("== {}: {} ==", kind.label(), result.title),
        &[BOLD],
    )];
    parts.extend(branch_line(&review.action.repo_dir));
    parts.extend(result.notes.iter().cloned());
    if let Some(lint) = &review.lint {
        parts.push(String::new());
        parts.extend(lint_summary(lint));
    }
    parts.push(String::new());
    parts.push(diff_stat(&result.patch, &result.repo_root));
    if !result.patch.trim().is_empty() {
        parts.push(String::new());
        parts.push(diff_preview(&result.patch));
    }
    if let Some(path) = &review.patch_path {
        // delta reads a patch from stdin; with one file argument it waits for stdin and hangs.
        parts.push(String::new());
        parts.push(format!("View full diff: delta < {}", path.display()));
    }
    parts.join("\n")
}

/// The text of an action with no diff (an API write, an unparsed command, an error).
pub fn section_message(review: &ActionReview) -> String {
    let section = review_section(review);
    section
        .split_once('\n')
        .map_or(section.clone(), |(_, message)| message.to_owned())
}

pub fn lint_summary(lint: &LintReport) -> Vec<String> {
    let mut lines = Vec::new();
    if !lint.tools_run.is_empty() {
        lines.push(paint(
            &format!("LINT passed on added lines ({})", lint.tools_run.join(", ")),
            &[GREEN, BOLD],
        ));
    } else if lint.missing_tools.is_empty() {
        lines.push("LINT: no linter for these file types".to_owned());
    }
    if lint.old_problem_count > 0 {
        lines.push(format!(
            "  {} older problem(s) in touched files, on lines this change does not add (not blocking)",
            lint.old_problem_count
        ));
    }
    let warnings = lint.warnings();
    if !warnings.is_empty() {
        lines.push(paint(
            &format!("LINT warnings on added lines ({}):", warnings.len()),
            &[YELLOW],
        ));
        lines.extend(
            warnings
                .iter()
                .map(|problem| format!("  {}", problem.describe())),
        );
    }
    if !lint.missing_tools.is_empty() {
        lines.push(paint(
            &format!(
                "LINT skipped, not installed: {} (run `terry install` to add)",
                lint.missing_tools.join(", ")
            ),
            &[YELLOW],
        ));
    }
    if !lint.tool_errors.is_empty() {
        lines.push(paint("LINT could not run:", &[YELLOW, BOLD]));
        lines.extend(lint.tool_errors.iter().map(|error| format!("  {error}")));
    }
    lines
}

/// The permission prompt's text for a diff the user already read and approved in the pane:
/// one line per action, plus its notes (a force push, an out-of-sync branch), and no diff.
pub fn approved_summary(reviews: &[ActionReview]) -> String {
    let mut lines = vec![paint(
        "terry: you approved this diff in the Diff review pane.",
        &[BOLD],
    )];
    for review in reviews {
        let label = review.action.kind.label();
        let Some(result) = &review.result else {
            lines.push(format!("{label}: {}", strip_ansi(&section_message(review))));
            continue;
        };
        let stat = diff_stat(&result.patch, &result.repo_root);
        let total = stat.lines().last().unwrap_or_default().trim();
        lines.push(format!("{label}: {} · {total}", result.title));
        lines.extend(result.notes.iter().cloned());
    }
    lines.join("\n")
}

/// What Claude reads when lint blocks the action.
pub fn lint_denial(command: &str, failing: &[&ActionReview]) -> String {
    let mut lines = vec![
        "Diff gate: lint found problems on lines this change adds. The command did not run."
            .to_owned(),
        format!("Command: {command}"),
        String::new(),
    ];
    for review in failing {
        let (Some(result), Some(lint)) = (&review.result, &review.lint) else {
            continue;
        };
        let problems = lint.blocking_problems();
        lines.push(format!(
            "{} in {}:",
            review.action.kind.label(),
            result.repo_root.display()
        ));
        lines.extend(
            problems
                .iter()
                .take(MAX_DENIAL_PROBLEMS)
                .map(|problem| format!("  {}", problem.describe())),
        );
        if problems.len() > MAX_DENIAL_PROBLEMS {
            lines.push(format!(
                "  ... and {} more",
                problems.len() - MAX_DENIAL_PROBLEMS
            ));
        }
    }
    lines.extend([
        String::new(),
        "Next steps:".to_owned(),
        "1. Fix each problem with the Edit tool. A lint-fix gate is now on for this repo, so the user approves every edit. Do not use auto-fix commands; they also need approval.".to_owned(),
        "2. For a commit, stage the fixed files with git add. For a PR, commit the fixes first.".to_owned(),
        "3. Run the same command again. The gate lints again and, if clean, shows the user the diff for approval.".to_owned(),
    ]);
    lines.join("\n")
}

/// What Claude reads while the command waits for the user's answer in the pane.
pub fn pane_review_request(command: &str) -> String {
    [
        "terry: the user reviews this diff in the \"Diff review\" pane first. The command did not run.".to_owned(),
        format!("Command: {command}"),
        String::new(),
        "Next steps:".to_owned(),
        "1. End your turn now with one short line that asks the user to read the diff in the Diff review pane and press Approve or Reject there. If the pane is not open, the user can run /terry.".to_owned(),
        "2. Until the user answers, do not run this command again, and do not run other git or gh commands.".to_owned(),
        "3. When terry says the user approved, run exactly the same command again. The user then confirms it in the permission prompt.".to_owned(),
    ]
    .join("\n")
}

fn diff_stat(patch: &str, repo: &Path) -> String {
    if patch.trim().is_empty() {
        return "(no file changes)".to_owned();
    }
    match git::run(&["git", "apply", "--stat"], repo, Some(patch)) {
        Ok(stat) => stat.trim_end().to_owned(),
        Err(_) => format!("{} file(s) changed", patch.matches("diff --git").count()),
    }
}

fn diff_preview(patch: &str) -> String {
    let lines: Vec<&str> = patch.lines().collect();
    let shown: Vec<String> = lines
        .iter()
        .take(MAX_PREVIEW_LINES)
        .map(|line| {
            if line.chars().count() <= MAX_PREVIEW_LINE_CHARS {
                (*line).to_owned()
            } else {
                let cut: String = line.chars().take(MAX_PREVIEW_LINE_CHARS).collect();
                format!("{cut}…")
            }
        })
        .collect();
    let header = paint(
        &format!("[preview: first {} of {} lines]", shown.len(), lines.len()),
        &[BOLD],
    );
    let body = colorize_patch(&format!("{}\n", shown.join("\n")));
    format!("{header}\n{}", body.trim_end_matches('\n'))
}

fn find_delta() -> Option<PathBuf> {
    paths::find_on_path("delta").or_else(|| {
        let cargo_delta = paths::home_dir().join(".cargo").join("bin").join("delta");
        paths::is_executable(&cargo_delta).then_some(cargo_delta)
    })
}

/// Adds delta's syntax and +/- colours (the `[delta]` styles in git config), same layout.
fn colorize_patch(patch: &str) -> String {
    let Some(delta) = find_delta().filter(|_| color_on()) else {
        return patch.to_owned();
    };
    let mut command = Command::new(delta);
    command.args(["--color-only", "--paging", "never"]);
    match process::run_captured(&mut command, Some(patch), DELTA_TIMEOUT) {
        Ok(done) if done.status == Some(0) => done.stdout,
        _ => patch.to_owned(),
    }
}
