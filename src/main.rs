//! terry: a review gate for Claude Code.
//!
//! `terry gate bash` and `terry gate edit` are the PreToolUse hooks Claude Code runs before each
//! Bash call and each file edit. `terry install` sets them up on this machine, and
//! `terry exclude` / `terry include` turn terry off and on for a folder.

mod approvals;
mod config;
mod diff;
mod gate_bash;
mod gate_edit;
mod git;
mod hook;
mod install;
mod lint;
mod lint_fix;
mod manifest;
mod patch;
mod paths;
mod process;
mod prompt;
mod review;
mod sha256;
mod shell;
mod stamp;
mod text;

use std::env;
use std::process::ExitCode;

const USAGE: &str = "usage:
  terry install [--yes] [--no-plugin] set terry up on this machine
  terry exclude [folder]              turn terry off in a folder (default: this one)
  terry include [folder]              turn terry on again in a folder
  terry status [folder]               exit 0 if terry is on in a folder, 1 if it is off
  terry gate bash                     PreToolUse hook for Bash calls (reads the call on stdin)
  terry gate edit                     PreToolUse hook for file edits (reads the call on stdin)";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["gate", "bash"] => {
            gate_bash::run();
            ExitCode::SUCCESS
        }
        ["gate", "edit"] => {
            gate_edit::run();
            ExitCode::SUCCESS
        }
        ["install", options @ ..] => install::run(options),
        ["exclude", folder @ ..] => config::run(folder, true),
        ["include", folder @ ..] => config::run(folder, false),
        ["status", folder @ ..] => config::status(folder),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
