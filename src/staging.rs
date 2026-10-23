//! Works out what a `git commit` will contain when the same command line changes the index
//! first (`git add x && git commit`) or names paths (`git commit x`).

use std::path::Path;

use crate::git::IndexOp;
use crate::shell::GitStep;

/// Flags that make `git add` ask questions; the gate cannot replay those.
const INTERACTIVE_ADD_FLAGS: [&str; 6] = ["-p", "--patch", "-i", "--interactive", "-e", "--edit"];
const MV_FLAGS: [&str; 3] = ["-f", "-k", "-v"];
const RESET_FLAGS: [&str; 4] = ["-q", "--quiet", "--mixed", "--"];
const RESTORE_INDEX_FLAGS: [&str; 4] = ["--staged", "-S", "-q", "--quiet"];
/// Git commands that change the working tree, the branch or the index in ways the gate does
/// not replay. The review warns when one runs before the commit.
const UNREPLAYED_COMMANDS: [&str; 15] = [
    "stash",
    "checkout",
    "switch",
    "merge",
    "rebase",
    "cherry-pick",
    "revert",
    "apply",
    "am",
    "clean",
    "pull",
    "restore",
    "reset",
    "rm",
    "mv",
];
/// `git commit` options that take the next word as their value.
const COMMIT_VALUE_LETTERS: &str = "mFCct";
const COMMIT_VALUE_FLAGS: [&str; 12] = [
    "--message",
    "--file",
    "--reuse-message",
    "--reedit-message",
    "--template",
    "--author",
    "--date",
    "--cleanup",
    "--fixup",
    "--squash",
    "--trailer",
    "--pathspec-from-file",
];

/// The earlier git commands of a commit line, sorted into those the gate replays on an index
/// copy and those it can only warn about.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Preceding {
    pub ops: Vec<IndexOp>,
    /// The commands replayed, as `git add` and the like, for the review text.
    pub replayed: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn sort_preceding(steps: &[GitStep], repo_dir: &Path) -> Preceding {
    let mut sorted = Preceding::default();
    for step in steps {
        match replay_op(step, repo_dir) {
            Some(op) => {
                sorted.ops.push(op);
                let name = format!("`git {}`", step.subcommand);
                if !sorted.replayed.contains(&name) {
                    sorted.replayed.push(name);
                }
            }
            None if UNREPLAYED_COMMANDS.contains(&step.subcommand.as_str())
                || step.subcommand == "add" =>
            {
                sorted.warnings.push(format!(
                    "This command runs `git {}` before the commit; the gate cannot replay it, so the diff may not match what is committed.",
                    step.subcommand
                ));
            }
            None => {}
        }
    }
    sorted
}

/// The index change of `step`, when it changes only the index and the gate can replay it.
fn replay_op(step: &GitStep, repo_dir: &Path) -> Option<IndexOp> {
    let args = &step.args;
    let words = |first: &str, rest: Vec<&str>| {
        let mut all = vec![first.to_owned()];
        all.extend(rest.into_iter().map(str::to_owned));
        IndexOp::Git(all)
    };
    match step.subcommand.as_str() {
        "add" => {
            let interactive = args
                .iter()
                .any(|arg| INTERACTIVE_ADD_FLAGS.contains(&arg.as_str()));
            let dry_run = args.iter().any(|arg| arg == "-n" || arg == "--dry-run");
            (!interactive && !dry_run).then(|| {
                let mut all = vec!["add".to_owned()];
                all.extend(args.iter().cloned());
                IndexOp::Git(all)
            })
        }
        "rm" => {
            let dry_run = args.iter().any(|arg| arg == "-n" || arg == "--dry-run");
            (!dry_run).then(|| {
                // `--cached` gives the index change of `git rm` without deleting files.
                let rest = args.iter().filter(|arg| *arg != "--cached");
                let mut all = vec!["rm".to_owned(), "--cached".to_owned()];
                all.extend(rest.cloned());
                IndexOp::Git(all)
            })
        }
        "mv" => {
            let flags_ok = args
                .iter()
                .filter(|arg| arg.starts_with('-'))
                .all(|arg| MV_FLAGS.contains(&arg.as_str()));
            let paths: Vec<&String> = args.iter().filter(|arg| !arg.starts_with('-')).collect();
            match (flags_ok, paths.as_slice()) {
                (true, [from, to]) if !repo_dir.join(to).is_dir() => Some(IndexOp::Move {
                    from: (*from).clone(),
                    to: (*to).clone(),
                }),
                _ => None,
            }
        }
        "reset" => {
            let flags_ok = args
                .iter()
                .filter(|arg| arg.starts_with('-'))
                .all(|arg| RESET_FLAGS.contains(&arg.as_str()));
            let positionals: Vec<&str> = args
                .iter()
                .map(String::as_str)
                .filter(|arg| !arg.starts_with('-'))
                .collect();
            // A commit named here would move HEAD; only the index-only forms are replayed.
            let index_only = positionals.is_empty()
                || positionals[0] == "HEAD"
                || args.iter().any(|arg| arg == "--");
            (flags_ok && index_only)
                .then(|| words("reset", args.iter().map(String::as_str).collect()))
        }
        "restore" => {
            let flags_ok = args
                .iter()
                .filter(|arg| arg.starts_with('-'))
                .all(|arg| RESTORE_INDEX_FLAGS.contains(&arg.as_str()) || arg == "--");
            let staged = args.iter().any(|arg| arg == "--staged" || arg == "-S");
            (flags_ok && staged)
                .then(|| words("restore", args.iter().map(String::as_str).collect()))
        }
        _ => None,
    }
}

/// The paths a `git commit` names (`git commit a.py -m x`). Git then commits only those
/// paths, from the working tree, and ignores the rest of the index.
pub fn commit_paths(args: &[String]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--" {
            paths.extend(args[index + 1..].iter().cloned());
            break;
        }
        if COMMIT_VALUE_FLAGS.contains(&arg) {
            index += 2;
            continue;
        }
        if let Some(letters) = arg.strip_prefix('-').filter(|rest| !rest.starts_with('-')) {
            // In a cluster such as `-am`, a value letter takes the rest of the word, or the
            // next word when it ends the cluster.
            let value_at = letters.find(|c| COMMIT_VALUE_LETTERS.contains(c));
            let takes_next = value_at.is_some_and(|at| at + 1 == letters.len());
            index += if takes_next { 2 } else { 1 };
            continue;
        }
        if !arg.starts_with('-') {
            paths.push(arg.to_owned());
        }
        index += 1;
    }
    paths
}
