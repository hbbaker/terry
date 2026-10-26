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

fn finds_commit(command: &str) -> bool {
    kinds(command, "/r")
        .iter()
        .any(|(kind, _)| *kind == ActionKind::Commit)
}

#[test]
fn should_find_the_commit_when_it_follows_a_shell_keyword_or_brace() {
    assert!(finds_commit("if true; then git commit -m x; fi"));
    assert!(finds_commit("{ git commit -m x; }"));
    assert!(finds_commit("! git commit -m x"));
    assert!(finds_commit("for f in a; do git commit -m x; done"));
}

#[test]
fn should_find_the_commit_when_it_sits_in_a_command_substitution() {
    assert!(finds_commit(r#"echo "$(git commit -m x)""#));
    assert!(finds_commit("echo `git commit -m x`"));
    assert!(finds_commit("echo $(git commit -m x)"));
}

#[test]
fn should_use_the_folder_when_git_options_are_attached() {
    assert_eq!(
        kinds("git -C/tmp/r commit -m x", "/")[0],
        (ActionKind::Commit, PathBuf::from("/tmp/r"))
    );
    assert!(finds_commit("git --git-dir=/x/.git commit -m x"));
    assert!(finds_commit("git -ccore.editor=true commit"));
    assert!(finds_commit("git --no-pager commit"));
}

#[test]
fn should_join_the_lines_when_a_backslash_ends_a_line() {
    assert!(finds_commit("git add -A && \\\ngit commit -m x"));
    assert!(finds_commit("git \\\n  commit -m x"));
}

#[test]
fn should_skip_wrapper_flags_when_a_wrapper_runs_git() {
    assert!(finds_commit("sudo -u dev git commit -m x"));
    assert!(finds_commit("timeout 30 git commit -m x"));
    assert!(finds_commit("env -i git commit -m x"));
    assert!(finds_commit("nice -n 5 git commit -m x"));
    assert!(finds_commit("echo x | xargs git commit -m"));
}

#[test]
fn should_skip_a_heredoc_body_when_reading_the_command() {
    assert_eq!(
        kinds("git commit -F - <<EOF\nit's x\nEOF", "/r")[0].0,
        ActionKind::Commit
    );
    assert!(kinds("cat <<'EOF'\ngit commit -m x\nEOF", "/r").is_empty());
}

#[test]
fn should_flag_a_git_commit_that_hides_in_an_unknown_position() {
    assert_eq!(
        kinds("find . -name x -exec git commit -m y {} +", "/r")[0].0,
        ActionKind::Unparsed
    );
}

#[test]
fn should_not_flag_git_commands_that_only_mention_commit() {
    assert!(kinds("git log --grep commit", "/r").is_empty());
    assert!(kinds("echo hi", "/r").is_empty());
    assert!(kinds("if true; then git push; fi", "/r").is_empty());
}

#[test]
fn should_find_the_commit_in_a_long_mixed_chain() {
    let command = "cd /r && git add -A && git status | head; if true; then git commit -m \"$(cat <<'EOF'\nmsg\nEOF\n)\"; fi && git push";
    assert_eq!(
        kinds(command, "/")
            .iter()
            .filter(|(kind, _)| *kind == ActionKind::Commit)
            .count(),
        1
    );
}
