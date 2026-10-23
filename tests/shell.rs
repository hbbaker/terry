use std::path::{Path, PathBuf};

use terry::shell::*;

fn words(command: &str) -> Vec<String> {
    tokenize(command).expect("tokenizes")
}

fn kinds(command: &str, cwd: &str) -> Vec<(ActionKind, PathBuf)> {
    find_gated_actions(command, Path::new(cwd))
        .into_iter()
        .map(|action| (action.kind, action.repo_dir))
        .collect()
}

#[test]
fn should_split_words_and_separators_when_command_has_quotes_and_chains() {
    assert_eq!(
        words(r#"git commit -m "a \"b\" c" && echo 'x y'; ls"#),
        vec![
            "git",
            "commit",
            "-m",
            r#"a "b" c"#,
            "&&",
            "echo",
            "x y",
            ";",
            "ls"
        ]
    );
}

#[test]
fn should_keep_an_empty_quoted_word() {
    assert_eq!(words("git commit -m ''"), vec!["git", "commit", "-m", ""]);
}

#[test]
fn should_drop_a_comment_that_starts_a_word() {
    assert_eq!(
        words("git commit # later\nls"),
        vec!["git", "commit", "\n", "ls"]
    );
}

#[test]
fn should_fail_when_a_quote_is_not_closed() {
    assert_eq!(
        tokenize("git commit -m 'oops"),
        Err(TokenizeError::UnclosedQuote)
    );
}

#[test]
fn should_follow_cd_when_git_runs_after_it() {
    assert_eq!(
        kinds("cd /no-such-terry/repo && git status && git commit", "/"),
        vec![(ActionKind::Commit, PathBuf::from("/no-such-terry/repo"))]
    );
}

#[test]
fn should_attach_the_earlier_git_add_to_the_commit_when_they_share_a_command() {
    let actions = find_gated_actions("git add -A && git commit -m x", Path::new("/r"));

    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].preceding_git,
        vec![GitStep {
            subcommand: "add".to_owned(),
            args: vec!["-A".to_owned()]
        }]
    );
}

#[test]
fn should_not_attach_a_git_add_from_another_folder_to_the_commit() {
    let actions = find_gated_actions("git -C /a add -A && git -C /b commit -m x", Path::new("/r"));

    assert!(actions[0].preceding_git.is_empty());
}

#[test]
fn should_use_the_git_c_folder_when_command_names_one() {
    assert_eq!(
        kinds("git -C ../other commit -m x", "/no-such-terry/repo"),
        vec![(ActionKind::Commit, PathBuf::from("/no-such-terry/other"))]
    );
}

#[test]
fn should_find_the_commit_when_it_runs_inside_bash_c() {
    assert_eq!(
        kinds("bash -c 'git commit -m x'", "/r")[0].0,
        ActionKind::Commit
    );
}

#[test]
fn should_skip_wrappers_and_assignments_before_the_command() {
    assert_eq!(
        kinds("FOO=1 sudo env git commit", "/r")[0].0,
        ActionKind::Commit
    );
}

#[test]
fn should_find_pr_writes_and_ignore_pr_reads() {
    assert_eq!(
        kinds("gh pr merge 20 --merge", "/r")[0].0,
        ActionKind::PrMerge
    );
    assert!(kinds("gh pr list", "/r").is_empty());
    assert!(kinds("git log --grep 'git commit'", "/r").is_empty());
    assert!(kinds("git push origin main", "/r").is_empty());
}

#[test]
fn should_treat_gh_api_as_a_write_only_when_it_sends_fields_or_a_write_method() {
    let args = |line: &str| words(line);
    assert!(is_api_write(&args("repos/o/r/pulls -f title=x")));
    assert!(!is_api_write(&args("repos/o/r/pulls")));
    assert!(!is_api_write(&args("repos/o/r/pulls -X GET -f state=open")));
    assert!(is_api_write(&args("repos/o/r/pulls/1 --method=PATCH")));
    assert!(is_api_write(&args("graphql -f query='mutation { x }'")));
}

#[test]
fn should_flag_an_unparsable_command_only_when_it_looks_like_a_write() {
    assert_eq!(
        kinds("git commit -m 'oops", "/r")[0].0,
        ActionKind::Unparsed
    );
    assert!(kinds("echo 'oops", "/r").is_empty());
}

#[test]
fn should_spot_terry_exclude_only_when_terry_runs_it() {
    assert!(runs_terry_exclude("terry exclude"));
    assert!(runs_terry_exclude(
        "cd /x && ~/Projects/terry/target/release/terry include /x"
    ));
    assert!(runs_terry_exclude("bash -c 'terry exclude'"));
    assert!(!runs_terry_exclude("git commit -m 'add terry exclude'"));
    assert!(!runs_terry_exclude("terry install"));
}

#[test]
fn should_spot_commands_that_rewrite_files() {
    for command in [
        "ruff check --fix app.py",
        "ruff format .",
        "black .",
        "npx eslint --fix src",
        "prettier -w x.ts",
        "sed -i 's/a/b/' f",
        "perl -pi -e 's/a/b/' f",
    ] {
        assert!(is_auto_fix_command(command), "{command}");
    }
    for command in [
        "ruff check app.py",
        "sed 's/a/b/' f",
        "eslint src",
        "prettier --check x",
    ] {
        assert!(!is_auto_fix_command(command), "{command}");
    }
}
