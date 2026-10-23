//! terry: a review gate for Claude Code.
//!
//! `terry gate bash` and `terry gate edit` are the PreToolUse hooks Claude Code runs before each
//! Bash call and each file edit. `terry install` sets them up on this machine, and
//! `terry exclude` / `terry include` turn terry off and on for a folder.

pub mod approvals;
pub mod config;
pub mod diff;
pub mod gate_bash;
pub mod gate_edit;
pub mod git;
pub mod hook;
pub mod install;
pub mod lint;
pub mod lint_fix;
pub mod manifest;
pub mod patch;
pub mod paths;
pub mod process;
pub mod prompt;
pub mod review;
pub mod sha256;
pub mod shell;
pub mod staging;
pub mod stamp;
pub mod text;
