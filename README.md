# terry

A file tree and diff review tool for Claude Code, with git hooks and linting.

[![CI](https://github.com/hbbaker/terry/actions/workflows/ci.yml/badge.svg)](https://github.com/hbbaker/terry/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/hbbaker/terry/branch/main/graph/badge.svg)](https://codecov.io/gh/hbbaker/terry)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust edition 2024](https://img.shields.io/badge/rust-edition%202024-orange.svg)](Cargo.toml)

terry shows you what Claude changes, and it stops Claude from committing until you approve the diff.

## Features

- **File tree.** A pane beside the chat lists every file of the repo. Colors and letters show
  each change (`A`, `M`, `D`, `?`). A `✎` marks a file that Claude edited. The file that Claude
  edits now turns orange.
- **Diff review.** A second pane shows the full diff with **Approve** and **Reject** buttons.
  Claude waits while you read.
- **Git hooks.** terry stops `git commit` and `gh pr` commands before they run.
- **Linting.** terry lints the lines that a change adds: ruff for Python, shellcheck for shell,
  ESLint for JS and TS. If lint fails, terry denies the command and Claude must fix the problems.
- **Branch chain.** The panes show the branches from the default branch down to the current one,
  such as `main -> dev -> subfeature`.
- **Off where you say.** `terry exclude` turns terry off in a folder and in every folder under it.
- **Better `git diff`.** delta becomes the pager of git, with word-level marks.

## How it works

```
Claude runs `git commit` ─▶ terry gate bash (PreToolUse hook)
                              ├─ excluded folder? ─▶ lets it through
                              ├─ builds the diff, saves the patch
                              ├─ lints the added lines
                              │    └─ problems? ─▶ deny; Claude fixes, you approve each edit
                              └─ clean, not yet approved in the pane
                                   ─▶ deny; the "Diff review" pane opens; Claude waits
                                        ├─ Approve ─▶ Claude runs the command again
                                        │             ─▶ permission prompt: you confirm
                                        └─ Reject ─▶ you add the reason in the prompt box
```

An approval works once for one exact diff. It expires after 30 minutes. See
[how approval works](docs/install-details.md#how-approval-works).

## Install

You need git, Rust (cargo), and the `claude` CLI. For linting, you also need ruff, shellcheck, and
Node with npm. The installer checks each tool and offers to install what is missing.

```sh
git clone https://github.com/hbbaker/terry.git ~/Projects/terry
cd ~/Projects/terry
./install.sh
```

Then start a new Claude Code session, or run `/reload-plugins` in an open one.

The installer puts the `terry` binary in `~/.cargo/bin`, adds two hooks to
`~/.claude/settings.json`, and installs the pane plugin. It makes a backup before it edits any
file. Run `./install.sh` again after a `git pull`. For the full list of changes, see
[install details](docs/install-details.md).

To remove terry, run `./uninstall.sh`. Add `--purge` to also remove the exclude list and the saved
diffs.

## Usage

| Task | How |
| --- | --- |
| Open the panes | `/terry` |
| Approve or reject a diff | Click the button, or press `y` or `n` |
| Turn terry off in a folder | `terry exclude` (run it in your own terminal) |
| Turn terry on again | `terry include` |
| Show the branch chain | `terry branches` |
| Print the file tree as JSON | `terry tree` |
| Skip files from lint | Add a `.terry` file at the repo root |
| Print the settings in effect | `terry config` |

For more tasks, see the [usage guide](docs/usage.md).

## Documentation

- [Usage](docs/usage.md): common tasks, lint skip patterns, how to read the file tree, file upload.
- [Configuration](docs/configuration.md): settings, colors, pane width.
- [Install details](docs/install-details.md): what the installer changes, where terry installs, approval rules.
- [Limits](docs/limits.md): known limits and edge cases.

## Development

terry is one Rust binary with one dependency (`serde_json`). The pane is a Claude Code plugin in
TypeScript, because Claude Code loads plugins only as TypeScript or JavaScript.

```sh
cargo test                     # unit tests, and the gate against throwaway git repos
claude plugin test plugin      # the pane plugin
```

Lint tests skip, with a note, when their tool is not installed.

| Folder | What it holds |
| --- | --- |
| `src/` | The `terry` binary: the two hooks, the command parser, the diff builders, the lint runner, the branch chain, approvals, config, and the installer. |
| `tests/` | Gate tests against throwaway git repos, and unit tests. |
| `plugin/` | The Claude Code plugin that draws the panes. |
| `lint/` | The fallback ESLint config. |
| `git/` | The colors for delta. |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

[MIT](LICENSE)
