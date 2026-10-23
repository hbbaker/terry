//! Runs git and gh for the gate, with plain output and a time limit.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::process;

const TIMEOUT: Duration = Duration::from_secs(10);
// The global git pager is delta; keep everything the gate reads plain and non-interactive.
const PLAIN_OUTPUT_ENV: [(&str, &str); 5] = [
    ("GIT_PAGER", "cat"),
    ("GH_PAGER", "cat"),
    ("PAGER", "cat"),
    ("NO_COLOR", "1"),
    ("GH_PROMPT_DISABLED", "1"),
];
const FALLBACK_DEFAULT_BRANCHES: [&str; 2] = ["main", "master"];

/// Runs `args` in `cwd` and returns its stdout. An error names the command and gives its
/// stderr.
pub fn run(args: &[&str], cwd: &Path, stdin: Option<&str>) -> Result<String, String> {
    let (program, rest) = args.split_first().expect("a command names a program");
    let mut command = Command::new(program);
    command.args(rest).current_dir(cwd).envs(PLAIN_OUTPUT_ENV);
    let shown = args.join(" ");
    match process::run_captured(&mut command, stdin, TIMEOUT) {
        Ok(done) if done.status == Some(0) => Ok(done.stdout),
        Ok(done) => Err(format!("`{shown}` failed: {}", done.stderr.trim())),
        Err(error) => Err(format!("`{shown}` failed: {error}")),
    }
}

pub fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let mut full = vec!["git", "--no-pager", "-c", "color.ui=false"];
    full.extend_from_slice(args);
    run(&full, repo, None)
}

pub fn diff(repo: &Path, args: &[&str]) -> Result<String, String> {
    let mut full = vec!["diff", "--no-ext-diff", "--no-color"];
    full.extend_from_slice(args);
    git(repo, &full)
}

pub fn ref_exists(repo: &Path, reference: &str) -> bool {
    let commit = format!("{reference}^{{commit}}");
    git(repo, &["rev-parse", "--verify", "--quiet", &commit]).is_ok()
}

/// The checked-out branch, or `detached`. symbolic-ref also works before the first commit,
/// when `rev-parse HEAD` fails.
pub fn current_branch(repo: &Path) -> String {
    git(repo, &["symbolic-ref", "--short", "HEAD"])
        .map(|branch| branch.trim().to_owned())
        .unwrap_or_else(|_| "detached".to_owned())
}

pub fn repo_root(repo: &Path) -> Result<PathBuf, String> {
    git(repo, &["rev-parse", "--show-toplevel"]).map(|root| PathBuf::from(root.trim()))
}

pub fn upstream_of(repo: &Path, reference: &str) -> Option<String> {
    let upstream = format!("{reference}@{{upstream}}");
    git(repo, &["rev-parse", "--abbrev-ref", &upstream])
        .ok()
        .map(|name| name.trim().to_owned())
}

pub fn default_base_ref(repo: &Path, remote: &str) -> Result<String, String> {
    let head = format!("refs/remotes/{remote}/HEAD");
    if let Ok(base) = git(repo, &["symbolic-ref", "--short", &head]) {
        return Ok(base.trim().to_owned());
    }
    FALLBACK_DEFAULT_BRANCHES
        .iter()
        .map(|branch| format!("{remote}/{branch}"))
        .find(|candidate| ref_exists(repo, candidate))
        .ok_or_else(|| format!("cannot find the default branch of remote '{remote}'"))
}

/// One line per commit in `range`, indented for the review text.
pub fn commit_list(repo: &Path, range: &str) -> Result<Vec<String>, String> {
    let log = git(repo, &["log", "--oneline", "--no-decorate", range])?;
    let commits: Vec<String> = log.lines().map(|line| format!("  {line}")).collect();
    Ok(if commits.is_empty() {
        vec!["  (no commits)".to_owned()]
    } else {
        commits
    })
}

/// The files `git diff --name-only <diff_args>` lists.
pub fn changed_files(repo: &Path, diff_args: &[&str]) -> Result<BTreeSet<String>, String> {
    let mut args = vec!["diff", "--name-only"];
    args.extend_from_slice(diff_args);
    Ok(git(repo, &args)?.lines().map(str::to_owned).collect())
}
