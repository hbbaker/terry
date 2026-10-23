use std::path::Path;

use terry::git::IndexOp;
use terry::shell::GitStep;
use terry::staging::*;

fn step(subcommand: &str, args: &[&str]) -> GitStep {
    GitStep {
        subcommand: subcommand.to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn should_find_the_paths_when_commit_names_files_after_its_options() {
    assert_eq!(
        commit_paths(&strings(&["-m", "msg", "a.py", "b.py"])),
        ["a.py", "b.py"]
    );
    assert_eq!(commit_paths(&strings(&["a.py", "-m", "msg"])), ["a.py"]);
    assert_eq!(
        commit_paths(&strings(&["-am", "msg", "--", "a.py"])),
        ["a.py"]
    );
    assert_eq!(commit_paths(&strings(&["--message=msg", "a.py"])), ["a.py"]);
}

#[test]
fn should_find_no_paths_when_the_message_is_the_only_word() {
    assert!(commit_paths(&strings(&["-m", "msg"])).is_empty());
    assert!(commit_paths(&strings(&["-mmsg"])).is_empty());
    assert!(commit_paths(&strings(&["--amend", "--no-edit"])).is_empty());
}

#[test]
fn should_replay_rm_as_a_cached_removal_when_it_would_delete_files() {
    let sorted = sort_preceding(&[step("rm", &["-r", "old"])], Path::new("/r"));

    assert_eq!(
        sorted.ops,
        vec![IndexOp::Git(strings(&["rm", "--cached", "-r", "old"]))]
    );
}

#[test]
fn should_warn_when_a_command_changes_the_working_tree_before_the_commit() {
    let sorted = sort_preceding(
        &[step("stash", &[]), step("reset", &["--hard"])],
        Path::new("/r"),
    );

    assert!(sorted.ops.is_empty());
    assert_eq!(sorted.warnings.len(), 2);
}

#[test]
fn should_not_replay_a_reset_that_names_a_commit() {
    let sorted = sort_preceding(&[step("reset", &["HEAD~1"])], Path::new("/r"));

    assert!(sorted.ops.is_empty());
    assert_eq!(sorted.warnings.len(), 1);
}

#[test]
fn should_not_replay_restore_when_it_touches_the_working_tree() {
    let sorted = sort_preceding(&[step("restore", &["a.py"])], Path::new("/r"));

    assert!(sorted.ops.is_empty());
    assert_eq!(sorted.warnings.len(), 1);
}

#[test]
fn should_ignore_read_only_git_commands_before_the_commit() {
    let sorted = sort_preceding(
        &[step("status", &[]), step("log", &["-1"])],
        Path::new("/r"),
    );

    assert_eq!(sorted, Preceding::default());
}
