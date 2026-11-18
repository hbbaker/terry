# Contributing

Thank you for your help. Follow these rules.

## Set up

1. Install Rust, git, and the `claude` CLI.
2. Install ruff, shellcheck, and Node with npm. Lint tests skip when a tool is missing.
3. Run `cargo test` and `claude plugin test plugin`. Both must pass before you start.

## Make a change

- Name the branch `feature/<short-description>`, `fix/<issue>`, or `chore/<task>`.
- Write commit messages as Conventional Commits: `type(scope): description`.
  Types: `feat`, `fix`, `chore`, `refactor`, `docs`, `test`, `ci`.
- Put a test with every change. A unit test of a private function goes in the same module. Tests
  in `tests/` use only the public behavior of the binary.
- Do not edit or loosen a test to make it pass. Fix the code.
- Run `cargo clippy --all-targets -- -D warnings` before you push. CI runs it.
- When you change anything in `plugin/`, raise `version` in `plugin/.claude-plugin/plugin.json`.
  Claude Code caches a plugin by its version.

## Open a pull request

- Make the PR title the same as the commit message it will have when squashed.
- Explain why you make the change. The diff shows what changed.
- Link the issue with `Refs #N`.
- The CI check must pass.

## Report a bug

Open an issue. Give the steps, what you expected, what happened, and the output of `terry config`.
