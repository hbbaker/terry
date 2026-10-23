//! Reads a Bash command line the way the gate needs it: which git and gh actions it runs, and
//! in which folder.

use std::path::{Path, PathBuf};

use crate::paths;
use crate::text::{after_whitespace, has_word, is_word_char, starts_with_word, word_matches};

const SEPARATOR_CHARS: &str = ";&|()\n";
const WRAPPER_COMMANDS: [&str; 6] = ["sudo", "command", "env", "time", "nohup", "exec"];
const SHELL_COMMANDS: [&str; 5] = ["bash", "sh", "zsh", "dash", "eval"];
const GIT_OPTIONS_WITH_VALUE: [&str; 5] = ["-C", "-c", "--git-dir", "--work-tree", "--namespace"];
const GH_API_BODY_FLAGS: [&str; 5] = ["-f", "-F", "--field", "--raw-field", "--input"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Commit,
    Push,
    PrCreate,
    PrEdit,
    PrMerge,
    ApiWrite,
    Unparsed,
}

impl ActionKind {
    /// The name the review text and the pane show.
    pub fn label(self) -> &'static str {
        match self {
            ActionKind::Commit => "git commit",
            ActionKind::Push => "git push",
            ActionKind::PrCreate => "gh pr create",
            ActionKind::PrEdit => "gh pr edit",
            ActionKind::PrMerge => "gh pr merge",
            ActionKind::ApiWrite => "gh api (write)",
            ActionKind::Unparsed => "unparsed command",
        }
    }

    /// The name in a saved patch's file name.
    pub fn slug(self) -> &'static str {
        match self {
            ActionKind::Commit => "commit",
            ActionKind::Push => "push",
            ActionKind::PrCreate => "pr-create",
            ActionKind::PrEdit => "pr-edit",
            ActionKind::PrMerge => "pr-merge",
            ActionKind::ApiWrite => "api-write",
            ActionKind::Unparsed => "unparsed",
        }
    }
}

/// One git or gh write in a command, and the folder it runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatedAction {
    pub kind: ActionKind,
    pub repo_dir: PathBuf,
    /// The words after the subcommand.
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizeError {
    UnclosedQuote,
    TrailingEscape,
}

/// Splits a command into words and separator runs (`;`, `&&`, `|`, `(`, newlines), with
/// POSIX quoting. A `#` that starts a word starts a comment.
pub fn tokenize(command: &str) -> Result<Vec<String>, TokenizeError> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    // A quoted empty string ('') is still a word, so track "in a word" apart from `word`.
    let mut in_word = false;
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\r' => end_word(&mut tokens, &mut word, &mut in_word),
            '#' if !in_word => {
                while chars.peek().is_some_and(|&next| next != '\n') {
                    chars.next();
                }
            }
            c if SEPARATOR_CHARS.contains(c) => {
                end_word(&mut tokens, &mut word, &mut in_word);
                let mut run = String::from(c);
                while let Some(&next) = chars.peek()
                    && SEPARATOR_CHARS.contains(next)
                {
                    run.push(next);
                    chars.next();
                }
                tokens.push(run);
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(inner) => word.push(inner),
                        None => return Err(TokenizeError::UnclosedQuote),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(escaped @ ('"' | '\\')) => word.push(escaped),
                            Some(other) => {
                                word.push('\\');
                                word.push(other);
                            }
                            None => return Err(TokenizeError::UnclosedQuote),
                        },
                        Some(inner) => word.push(inner),
                        None => return Err(TokenizeError::UnclosedQuote),
                    }
                }
            }
            '\\' => {
                in_word = true;
                word.push(chars.next().ok_or(TokenizeError::TrailingEscape)?);
            }
            other => {
                in_word = true;
                word.push(other);
            }
        }
    }
    end_word(&mut tokens, &mut word, &mut in_word);
    Ok(tokens)
}

fn end_word(tokens: &mut Vec<String>, word: &mut String, in_word: &mut bool) {
    if *in_word {
        tokens.push(std::mem::take(word));
        *in_word = false;
    }
}

fn is_separator(token: &str) -> bool {
    !token.is_empty() && token.chars().all(|c| SEPARATOR_CHARS.contains(c))
}

fn split_segments(tokens: Vec<String>) -> Vec<Vec<String>> {
    let mut segments = vec![Vec::new()];
    for token in tokens {
        if is_separator(&token) {
            segments.push(Vec::new());
        } else {
            segments.last_mut().expect("starts with one").push(token);
        }
    }
    segments.retain(|segment| !segment.is_empty());
    segments
}

/// An assignment such as `FOO=bar` before a command.
fn is_env_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn strip_wrappers(segment: &[String]) -> &[String] {
    let skipped = segment
        .iter()
        .take_while(|word| WRAPPER_COMMANDS.contains(&word.as_str()) || is_env_assignment(word))
        .count();
    &segment[skipped..]
}

/// The git and gh writes in `command`, run from `cwd`. A command that cannot be split into
/// words, but looks like a git or gh write, gives one `Unparsed` action.
pub fn find_gated_actions(command: &str, cwd: &Path) -> Vec<GatedAction> {
    let Ok(tokens) = tokenize(command) else {
        return if looks_gated(command) {
            vec![GatedAction {
                kind: ActionKind::Unparsed,
                repo_dir: cwd.to_path_buf(),
                args: Vec::new(),
            }]
        } else {
            Vec::new()
        };
    };

    let mut actions = Vec::new();
    let mut current_dir = cwd.to_path_buf();
    for segment in split_segments(tokens) {
        let Some((first, rest)) = strip_wrappers(&segment).split_first() else {
            continue;
        };
        let program = Path::new(first)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(first);
        match program {
            "cd" | "pushd" if !rest.is_empty() => {
                current_dir = paths::resolve_arg(&current_dir, &rest[0]);
            }
            "git" => actions.extend(parse_git(rest, &current_dir)),
            "gh" => actions.extend(parse_gh(rest, &current_dir)),
            _ if SHELL_COMMANDS.contains(&program) => {
                for argument in rest.iter().filter(|argument| !argument.starts_with('-')) {
                    actions.extend(find_gated_actions(argument, &current_dir));
                }
            }
            _ => {}
        }
    }
    actions
}

/// True when `command` runs `terry exclude` or `terry include`, directly or in `bash -c`.
/// Only the user may turn terry off for a folder.
pub fn runs_terry_exclude(command: &str) -> bool {
    let Ok(tokens) = tokenize(command) else {
        return command.contains("terry exclude") || command.contains("terry include");
    };
    split_segments(tokens).iter().any(|segment| {
        let Some((first, rest)) = strip_wrappers(segment).split_first() else {
            return false;
        };
        let program = Path::new(first)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(first);
        let changes_list = rest
            .first()
            .is_some_and(|subcommand| subcommand == "exclude" || subcommand == "include");
        (program == "terry" && changes_list)
            || (SHELL_COMMANDS.contains(&program)
                && rest
                    .iter()
                    .any(|argument| !argument.starts_with('-') && runs_terry_exclude(argument)))
    })
}

fn parse_git(args: &[String], cwd: &Path) -> Option<GatedAction> {
    let mut repo_dir = cwd.to_path_buf();
    let mut index = 0;
    while index < args.len() && args[index].starts_with('-') {
        let option = args[index].as_str();
        if GIT_OPTIONS_WITH_VALUE.contains(&option) && index + 1 < args.len() {
            if option == "-C" {
                repo_dir = paths::resolve_arg(&repo_dir, &args[index + 1]);
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    let (subcommand, rest) = args.get(index..)?.split_first()?;
    let kind = match subcommand.as_str() {
        "commit" => ActionKind::Commit,
        "push" => ActionKind::Push,
        _ => return None,
    };
    Some(GatedAction {
        kind,
        repo_dir,
        args: rest.to_vec(),
    })
}

fn parse_gh(args: &[String], cwd: &Path) -> Option<GatedAction> {
    let positionals: Vec<usize> = (0..args.len())
        .filter(|&index| !args[index].starts_with('-'))
        .collect();
    let &group_index = positionals.first()?;
    let action = |kind, after: usize| GatedAction {
        kind,
        repo_dir: cwd.to_path_buf(),
        args: args[after + 1..].to_vec(),
    };
    match args[group_index].as_str() {
        "api" => is_api_write(&args[group_index + 1..])
            .then(|| action(ActionKind::ApiWrite, group_index)),
        "pr" => {
            let &subcommand_index = positionals.get(1)?;
            let kind = match args[subcommand_index].as_str() {
                "create" => ActionKind::PrCreate,
                "edit" => ActionKind::PrEdit,
                "merge" => ActionKind::PrMerge,
                _ => return None,
            };
            Some(action(kind, subcommand_index))
        }
        _ => None,
    }
}

/// gh's own rule: the method is POST when fields are given, unless `-X` says otherwise.
fn is_api_write(args: &[String]) -> bool {
    let mut method: Option<String> = None;
    let mut has_body = false;
    for (index, arg) in args.iter().enumerate() {
        if (arg == "-X" || arg == "--method") && index + 1 < args.len() {
            method = Some(args[index + 1].to_uppercase());
        } else if let Some(value) = arg.strip_prefix("--method=") {
            method = Some(value.to_uppercase());
        } else if let Some(value) = arg.strip_prefix("-X").filter(|value| !value.is_empty()) {
            method = Some(value.to_uppercase());
        } else if GH_API_BODY_FLAGS.iter().any(|flag| arg.starts_with(flag)) {
            has_body = true;
        }
    }
    let endpoint = args.iter().find(|arg| !arg.starts_with('-'));
    if endpoint.is_some_and(|endpoint| endpoint == "graphql") {
        return args
            .iter()
            .any(|arg| arg.to_lowercase().contains("mutation"));
    }
    match method {
        Some(method) => method != "GET",
        None => has_body,
    }
}

/// The text check for a command that cannot be split into words (an unclosed quote):
/// `git` then `push` or `commit` on one line, or `gh pr` or `gh api`.
fn looks_gated(command: &str) -> bool {
    let git_write = command.lines().any(|line| {
        word_matches(line, "git").first().is_some_and(|&(_, end)| {
            has_word(&line[end..], "push") || has_word(&line[end..], "commit")
        })
    });
    let gh_write = word_matches(command, "gh").iter().any(|&(_, end)| {
        let rest = &command[end..];
        let after_space = rest.trim_start();
        after_space.len() < rest.len()
            && (starts_with_word(after_space, "pr") || starts_with_word(after_space, "api"))
    });
    git_write || gh_write
}

/// True for a command that can rewrite files on its own: a formatter, a `--fix`, `sed -i`.
/// While a lint fix is pending, these must ask, as file edits do.
pub fn is_auto_fix_command(command: &str) -> bool {
    command.lines().any(|line| {
        let after = |word: &str| {
            word_matches(line, word)
                .first()
                .map(|&(_, end)| &line[end..])
        };
        after("ruff").is_some_and(|rest| rest.contains("--fix") || has_word(rest, "format"))
            || has_word(line, "black")
            || after("eslint").is_some_and(|rest| rest.contains("--fix"))
            || after("prettier").is_some_and(|rest| {
                rest.contains("--write")
                    || after_whitespace(rest).any(|s| starts_with_word(s, "-w"))
            })
            || after("sed").is_some_and(|rest| after_whitespace(rest).any(|s| s.starts_with("-i")))
            || after("perl").is_some_and(|rest| {
                after_whitespace(rest).any(|s| {
                    s.strip_prefix('-').is_some_and(|flags| {
                        flags
                            .chars()
                            .take_while(|&c| is_word_char(c))
                            .any(|c| c == 'i')
                    })
                })
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            words("git push # later\nls"),
            vec!["git", "push", "\n", "ls"]
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
            kinds("cd /no-such-terry/repo && git status && git push", "/"),
            vec![(ActionKind::Push, PathBuf::from("/no-such-terry/repo"))]
        );
    }

    #[test]
    fn should_use_the_git_c_folder_when_command_names_one() {
        assert_eq!(
            kinds("git -C ../other commit -m x", "/no-such-terry/repo"),
            vec![(ActionKind::Commit, PathBuf::from("/no-such-terry/other"))]
        );
    }

    #[test]
    fn should_find_the_push_when_it_runs_inside_bash_c() {
        assert_eq!(
            kinds("bash -c 'git push origin main'", "/r")[0].0,
            ActionKind::Push
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
        assert!(kinds("git log --grep 'git push'", "/r").is_empty());
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
}
