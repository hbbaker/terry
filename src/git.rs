//! Runs git and gh for the gate, with plain output and a time limit.

use std::collections::BTreeSet;
use std::env;
use std::fs;
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
pub const DEFAULT_REMOTE: &str = "origin";
/// What `current_branch` gives when HEAD names no branch.
const DETACHED: &str = "detached";
const MAX_CHAIN_DEPTH: usize = 8;
const MAX_CHAIN_CANDIDATES: usize = 30;

/// Runs `args` in `cwd` and returns its stdout. An error names the command and gives its
/// stderr.
pub fn run(args: &[&str], cwd: &Path, stdin: Option<&str>) -> Result<String, String> {
    run_with_env(args, cwd, stdin, &[])
}

fn run_with_env(
    args: &[&str],
    cwd: &Path,
    stdin: Option<&str>,
    extra_env: &[(&str, &str)],
) -> Result<String, String> {
    let (program, rest) = args.split_first().expect("a command names a program");
    let mut command = Command::new(program);
    command
        .args(rest)
        .current_dir(cwd)
        .envs(PLAIN_OUTPUT_ENV)
        .envs(extra_env.iter().copied());
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

/// Where a replayed index starts: a copy of the real index, or the last commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexBase {
    Current,
    Head,
}

/// One change to replay on the index copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexOp {
    /// A git command that changes only the index, as the words after `git`.
    Git(Vec<String>),
    /// `git mv`: the index entry moves; the file on disk stays where it is.
    Move { from: String, to: String },
}

/// What `git diff --cached` would show after `ops` run on a copy of the index. The real index
/// and the working tree stay as they are.
pub fn staged_diff_after(repo: &Path, base: IndexBase, ops: &[IndexOp]) -> Result<String, String> {
    let copy = env::temp_dir().join(format!("terry-index-{}", std::process::id()));
    let _ = fs::remove_file(&copy);
    let result = diff_with_index(repo, base, ops, &copy);
    let _ = fs::remove_file(&copy);
    result
}

fn diff_with_index(
    repo: &Path,
    base: IndexBase,
    ops: &[IndexOp],
    index: &Path,
) -> Result<String, String> {
    match base {
        IndexBase::Current => {
            let real = git(
                repo,
                &["rev-parse", "--path-format=absolute", "--git-path", "index"],
            )?;
            // A repo with no commits and nothing staged has no index file; git starts one.
            if Path::new(real.trim()).exists() {
                fs::copy(real.trim(), index)
                    .map_err(|error| format!("cannot copy the git index: {error}"))?;
            }
        }
        IndexBase::Head => {
            let tree = if ref_exists(repo, "HEAD") {
                "HEAD"
            } else {
                "--empty"
            };
            run_on_index(repo, index, &["read-tree", tree])?;
        }
    }
    for op in ops {
        apply_op(repo, index, op)?;
    }
    run_on_index(
        repo,
        index,
        &["diff", "--cached", "--no-ext-diff", "--no-color"],
    )
}

fn apply_op(repo: &Path, index: &Path, op: &IndexOp) -> Result<(), String> {
    match op {
        IndexOp::Git(args) => {
            let words: Vec<&str> = args.iter().map(String::as_str).collect();
            run_on_index(repo, index, &words).map(drop)
        }
        IndexOp::Move { from, to } => {
            let entry = run_on_index(repo, index, &["ls-files", "-s", "--", from])?;
            let mut fields = entry.split_whitespace();
            let (Some(mode), Some(sha)) = (fields.next(), fields.next()) else {
                return Err(format!("git mv: {from} is not tracked"));
            };
            let prefix = git(repo, &["rev-parse", "--show-prefix"])?;
            let target = format!("{mode},{sha},{}{to}", prefix.trim());
            run_on_index(repo, index, &["rm", "--cached", "-q", "--", from])?;
            run_on_index(
                repo,
                index,
                &["update-index", "--add", "--cacheinfo", &target],
            )
            .map(drop)
        }
    }
}

fn run_on_index(repo: &Path, index: &Path, args: &[&str]) -> Result<String, String> {
    let index = index.to_string_lossy();
    let mut full = vec!["git", "--no-pager", "-c", "color.ui=false"];
    full.extend_from_slice(args);
    run_with_env(&full, repo, None, &[("GIT_INDEX_FILE", index.as_ref())])
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
        .unwrap_or_else(|_| DETACHED.to_owned())
}

pub fn repo_root(repo: &Path) -> Result<PathBuf, String> {
    git(repo, &["rev-parse", "--show-toplevel"]).map(|root| PathBuf::from(root.trim()))
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

/// The branches from the default branch down to the checked-out one, such as
/// `main`, `dev`, `subfeature`. Git stores no parent for a branch, so each step picks the local
/// branch nearest behind the one above it: the ancestor with the fewest commits between them.
/// When two ancestors tie (a new branch cut from `dev` still sits on `dev`'s commit), the
/// default branch loses, then the name that sorts first wins. A detached HEAD, or a folder that
/// is not a repo, gives an empty chain.
pub fn branch_chain(repo: &Path) -> Vec<String> {
    let current = current_branch(repo);
    if current == DETACHED {
        return Vec::new();
    }
    let trunk = default_branch(repo);
    let mut chain = vec![current];
    while chain.len() < MAX_CHAIN_DEPTH && trunk.as_ref() != chain.last() {
        let Some(parent) = nearest_ancestor_branch(repo, &chain, trunk.as_deref()) else {
            break;
        };
        chain.push(parent);
    }
    chain.reverse();
    chain
}

/// The default branch, when a local branch of that name exists.
pub(crate) fn default_branch(repo: &Path) -> Option<String> {
    let from_remote = default_base_ref(repo, DEFAULT_REMOTE)
        .ok()
        .and_then(|base| {
            base.strip_prefix(&format!("{DEFAULT_REMOTE}/"))
                .map(str::to_owned)
        });
    from_remote
        .into_iter()
        .chain(
            FALLBACK_DEFAULT_BRANCHES
                .iter()
                .map(|name| (*name).to_owned()),
        )
        .find(|name| ref_exists(repo, &format!("refs/heads/{name}")))
}

/// The local branch behind the last branch of `chain` that is not in `chain` yet.
fn nearest_ancestor_branch(repo: &Path, chain: &[String], trunk: Option<&str>) -> Option<String> {
    let tip = chain.last()?;
    let merged = format!("--merged={tip}");
    let listing = git(
        repo,
        &[
            "for-each-ref",
            &merged,
            "--format=%(refname:short)",
            "refs/heads",
        ],
    )
    .ok()?;
    listing
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty() && !chain.iter().any(|seen| seen == name))
        .take(MAX_CHAIN_CANDIDATES)
        .filter_map(|name| {
            let range = format!("{name}..{tip}");
            let distance: usize = git(repo, &["rev-list", "--count", &range])
                .ok()?
                .trim()
                .parse()
                .ok()?;
            Some((distance, trunk == Some(name), name))
        })
        .min()
        .map(|(_, _, name)| name.to_owned())
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
