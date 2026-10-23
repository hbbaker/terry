//! Builds the diff each gated action would publish: staged changes for a commit, the PR's
//! changes for gh.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::git;
use crate::patch::added_lines_by_file;
use crate::shell::{ActionKind, GatedAction};

const DEFAULT_REMOTE: &str = "origin";
const PR_VIEW_FIELDS: &str = "number,title,url,baseRefName,headRefName";

/// The change an action would publish, and what the review should say about it.
#[derive(Debug, Clone)]
pub struct DiffResult {
    pub title: String,
    pub patch: String,
    pub repo_root: PathBuf,
    pub branch: String,
    pub notes: Vec<String>,
    /// True when the working tree holds the content under review, so linters can read it.
    pub lintable: bool,
}

/// The diff of `action`; `None` for an action with no local diff (a gh api write, or a
/// command the gate could not read).
pub fn build(action: &GatedAction) -> Option<Result<DiffResult, String>> {
    match action.kind {
        ActionKind::Commit => Some(build_commit(action)),
        ActionKind::PrCreate => Some(build_pr_create(action)),
        ActionKind::PrEdit | ActionKind::PrMerge => Some(build_existing_pr(action)),
        ActionKind::ApiWrite | ActionKind::Unparsed => None,
    }
}

/// `-a`, `-am` and the like, or `--all`: the commit stages tracked changes itself.
fn is_stage_all_flag(arg: &str) -> bool {
    arg == "--all"
        || arg.strip_prefix('-').is_some_and(|letters| {
            letters.contains('a') && letters.chars().all(|c| c.is_ascii_alphabetic())
        })
}

/// Warns when lint will read working-tree files that differ from the content under review.
fn unsynced_notes(repo: &Path, patch: &str, diff_args: &[&str]) -> Result<Vec<String>, String> {
    let changed = git::changed_files(repo, diff_args)?;
    let unsynced: Vec<String> = added_lines_by_file(patch)
        .into_keys()
        .filter(|path| changed.contains(path))
        .collect();
    Ok(if unsynced.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "Lint reads the working copy, which differs here: {}",
            unsynced.join(", ")
        )]
    })
}

fn build_commit(action: &GatedAction) -> Result<DiffResult, String> {
    let repo = &action.repo_dir;
    let stage_all = action.args.iter().any(|arg| is_stage_all_flag(arg));
    let mut notes = Vec::new();
    let (patch, title) = if stage_all && git::ref_exists(repo, "HEAD") {
        (
            git::diff(repo, &["HEAD"])?,
            "Staged and unstaged changes (commit -a)",
        )
    } else {
        let patch = git::diff(repo, &["--cached"])?;
        notes.extend(unsynced_notes(repo, &patch, &[])?);
        (patch, "Staged changes")
    };
    if action.args.iter().any(|arg| arg == "--amend") {
        notes.push(
            "--amend: this rewrites the last commit; the diff shows only the new changes."
                .to_owned(),
        );
    }
    Ok(DiffResult {
        title: title.to_owned(),
        patch,
        repo_root: git::repo_root(repo)?,
        branch: git::current_branch(repo),
        notes,
        lintable: true,
    })
}

/// The value of the first of `names` in `args`: `--base main`, `-B main` or `--base=main`.
fn flag_value<'a>(args: &'a [String], names: &[&str]) -> Option<&'a str> {
    for (index, arg) in args.iter().enumerate() {
        if names.contains(&arg.as_str()) && index + 1 < args.len() {
            return Some(&args[index + 1]);
        }
        for name in names.iter().filter(|name| name.starts_with("--")) {
            if let Some(value) = arg
                .strip_prefix(name)
                .and_then(|rest| rest.strip_prefix('='))
            {
                return Some(value);
            }
        }
    }
    None
}

fn build_pr_create(action: &GatedAction) -> Result<DiffResult, String> {
    let repo = &action.repo_dir;
    let base = match flag_value(&action.args, &["--base", "-B"]) {
        Some(branch) => format!("{DEFAULT_REMOTE}/{branch}"),
        None => git::default_base_ref(repo, DEFAULT_REMOTE)?,
    };
    // `--head owner:branch` names a fork's branch; the local branch is the part after `:`.
    let head_branch = flag_value(&action.args, &["--head", "-H"])
        .and_then(|head| head.rsplit(':').next())
        .unwrap_or("");
    let head = if !head_branch.is_empty() && git::ref_exists(repo, head_branch) {
        head_branch
    } else {
        "HEAD"
    };
    let current = git::current_branch(repo);
    let head_label = if head == "HEAD" {
        current.as_str()
    } else {
        head
    };
    let patch = git::diff(repo, &[&format!("{base}...{head}")])?;
    let mut notes = vec!["Commits:".to_owned()];
    notes.extend(git::commit_list(repo, &format!("{base}..{head}"))?);
    let uses_checkout = head == "HEAD" || head == current;
    if uses_checkout {
        notes.extend(unsynced_notes(repo, &patch, &["HEAD"])?);
    } else {
        notes.push(format!("Not linted: {head} is not the checked-out branch."));
    }
    Ok(DiffResult {
        title: format!("New PR: {head_label} -> {base}"),
        patch,
        repo_root: git::repo_root(repo)?,
        branch: head_label.to_owned(),
        notes,
        lintable: uses_checkout,
    })
}

/// `#12`, `12` or a PR URL.
fn is_pr_selector(arg: &str) -> bool {
    let number = arg.strip_prefix('#').unwrap_or(arg);
    if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    let Some(rest) = arg
        .strip_prefix("https://")
        .or_else(|| arg.strip_prefix("http://"))
    else {
        return false;
    };
    !rest.chars().any(char::is_whitespace)
        && rest.match_indices("/pull/").any(|(index, marker)| {
            index > 0 && rest[index + marker.len()..].starts_with(|c: char| c.is_ascii_digit())
        })
}

fn find_pr_selector(args: &[String]) -> Option<&str> {
    match args.first() {
        Some(first) if !first.starts_with('-') => Some(first),
        _ => args
            .iter()
            .map(String::as_str)
            .find(|arg| is_pr_selector(arg)),
    }
}

fn build_existing_pr(action: &GatedAction) -> Result<DiffResult, String> {
    let repo = &action.repo_dir;
    let selector = find_pr_selector(&action.args);
    let mut view = vec!["gh", "pr", "view"];
    view.extend(selector);
    view.extend(["--json", PR_VIEW_FIELDS]);
    let pr: Value = serde_json::from_str(&git::run(&view, repo, None)?)
        .map_err(|error| format!("gh pr view gave unreadable output: {error}"))?;
    let field = |name: &str| {
        pr[name]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("gh pr view gave no {name}"))
    };
    let number = pr["number"]
        .as_u64()
        .ok_or("gh pr view gave no PR number")?
        .to_string();
    let patch = git::run(
        &["gh", "pr", "diff", &number, "--color", "never"],
        repo,
        None,
    )?;
    let head = field("headRefName")?;
    let mut notes = vec![field("url")?];
    if selector.is_none() {
        notes.push(
            "No PR number in the command; gh picked the PR for the current branch.".to_owned(),
        );
    }
    notes.push("Not linted: the PR content is on GitHub, not in the working tree.".to_owned());
    Ok(DiffResult {
        title: format!(
            "PR #{number} \"{}\" ({head} -> {})",
            field("title")?,
            field("baseRefName")?
        ),
        patch,
        repo_root: git::repo_root(repo)?,
        branch: head,
        notes,
        lintable: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn should_spot_stage_all_flags_when_commit_has_them() {
        for flag in ["-a", "-am", "--all", "-va"] {
            assert!(is_stage_all_flag(flag), "{flag}");
        }
        for flag in ["-m", "--amend", "-", "-a1"] {
            assert!(!is_stage_all_flag(flag), "{flag}");
        }
    }

    #[test]
    fn should_read_a_flag_value_in_each_spelling() {
        assert_eq!(
            flag_value(&strings(&["--base", "dev"]), &["--base", "-B"]),
            Some("dev")
        );
        assert_eq!(
            flag_value(&strings(&["-B", "dev"]), &["--base", "-B"]),
            Some("dev")
        );
        assert_eq!(
            flag_value(&strings(&["--base=dev"]), &["--base", "-B"]),
            Some("dev")
        );
        assert_eq!(
            flag_value(&strings(&["--title", "x"]), &["--base", "-B"]),
            None
        );
    }

    #[test]
    fn should_find_the_pr_selector_when_command_names_one() {
        assert_eq!(find_pr_selector(&strings(&["20", "--merge"])), Some("20"));
        assert_eq!(find_pr_selector(&strings(&["--merge", "#20"])), Some("#20"));
        assert_eq!(
            find_pr_selector(&strings(&["--squash", "https://github.com/o/r/pull/7"])),
            Some("https://github.com/o/r/pull/7")
        );
        assert_eq!(
            find_pr_selector(&strings(&["--merge", "--delete-branch"])),
            None
        );
    }
}
