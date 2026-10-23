# terry

A review gate for Claude Code. When Claude tries to **commit, push, or open, edit or merge a
pull request**, terry stops the command and shows you the diff before anything runs.

- **Diff review prompt.** The permission prompt shows the commits, a file summary, and a
  colored preview of the diff. The full patch is saved under `~/.claude/diffs/`.
- **Lint first.** terry lints the lines the change adds (ruff and `ruff format` for Python,
  shellcheck for shell, ESLint for JS/TS). If lint fails, the command is denied, Claude must fix
  the problems, and **you approve every fix edit**. Then Claude runs the command again.
- **Review in a pane first.** A "Diff review" pane opens beside the chat with the full diff,
  drawn with Claude Code's own diff colors, and **Approve** and **Reject** buttons. Claude waits
  while you scroll and read. Approve closes the pane, and Claude runs the command again: the
  permission prompt then asks you to confirm it.
- **Off where you say.** `terry exclude` turns terry off in a folder and every folder under it.
- **Better `git diff`.** delta becomes git's pager, with strong add/remove colors and
  word-level marks.

One Rust binary, `terry`, with one dependency (`serde_json`). The pane is a Claude Code plugin
in TypeScript, because Claude Code loads plugins only as TypeScript or JavaScript.

## Install

Needs: git, Rust (cargo), and the `claude` CLI. For linting: ruff, shellcheck, and Node with
npm for the fallback ESLint. The installer checks each one and offers to install what is
missing.

```sh
git clone <this repo> ~/Projects/terry
cd ~/Projects/terry
./install.sh
```

Then start a new Claude Code session, or run `/reload-plugins` in an open one.

`install.sh` builds `target/release/terry` and runs `terry install`, which changes only these
things outside the checkout:

| What | Change |
| --- | --- |
| Missing lint tools and delta | Installs each one only after you say yes (brew, uv, pipx, apt and so on, whichever this machine has). `--yes` installs all of them without asking. |
| `~/.gitconfig` | Adds `include.path` for `git/delta.gitconfig`. |
| `~/.claude/settings.json` | Adds two `PreToolUse` hooks that run the terry binary, and `ask` rules for commit, push and `gh pr`. A backup goes to `~/.claude/backups/` first. The file is written back with its keys in A–Z order. |
| `~/.local/bin/terry` | A link to the binary, so `terry exclude` works in any folder. |
| Claude Code plugins | Installs the `terry` pane plugin, read straight from `plugin/` in this folder. |

Run `./install.sh` again after a `git pull`. Use `./install.sh --no-plugin` to skip the pane.

## How it works

```
Claude runs `git commit` ─▶ terry gate bash (PreToolUse hook)
                              ├─ excluded folder? ─▶ lets it through
                              ├─ builds the diff, saves the patch
                              ├─ lints the added lines
                              │    └─ problems? ─▶ deny; Claude fixes, you approve each edit
                              └─ clean, not yet approved in the pane
                                   ─▶ deny; plugin/ opens the "Diff review" pane; Claude waits
                                        ├─ Approve ─▶ approval file for this exact diff;
                                        │             the pane closes; Claude runs the command
                                        │             again ─▶ the gate uses up the approval
                                        │             ─▶ "ask": the permission prompt, with
                                        │             a one-line summary per action
                                        └─ Reject ─▶ a reply in your prompt box; add the reason
```

The approval is tied to a hash of the command, the folder and the patch. It works once and
expires after 30 minutes. If the diff changes, you review it in the pane again.

The permission prompt stays as the last check, because a plugin cannot answer it: Claude
Code's built-in security plugin keeps permission decisions from third-party plugins.

Only the plugin writes approvals (`src/approvals.rs`), and only you change the exclude list.
The gates deny any Claude shell command or file edit that names `~/.claude/diffs/approvals` or
terry's config file, and any Claude command that runs `terry exclude` or `terry include`. This
blocks the plain ways around the review. A forged approval would still only reach the
permission prompt: with the full diff, or, if it claims to come from the pane, with the pane
open again beside the prompt and a line under it that says the diff was not approved there.

| Folder | What it holds |
| --- | --- |
| `src/` | The `terry` binary: `gate_bash.rs` and `gate_edit.rs` (the two hooks), the diff builders, the lint runner, the pane approvals, the exclude config, and `install.rs`. |
| `tests/` | The gate tests, against throwaway git repos. |
| `lint/` | The fallback ESLint config, for repos with no ESLint of their own on this machine. |
| `plugin/` | The Claude Code mod that draws the pane. |
| `git/` | delta's colors. |

Each linter uses the repo's own install and config first (`.venv/bin/ruff`, `.venv/bin/black`,
a local `node_modules/.bin/eslint` with an `eslint.config.*`), then the tool on your PATH, then
the fallback ESLint in `lint/`. A tool that is not installed is skipped, and the prompt says so.

## Settings

| Setting | Where | Default |
| --- | --- | --- |
| Folders where terry is off | `terry exclude` / `terry include`; the list is in `~/.config/terry/config.json` (or under `$XDG_CONFIG_HOME`) | none |
| Pane scroll speed (rows per mouse wheel tick, 1 to 50) | `/config`, row "Diff pane scroll speed"; or `~/.claude/settings.json`: `"pluginConfigs": { "terry@terry": { "options": { "scrollRows": 5 } } }` | 3 |
| Skip the pane step | `DIFF_GATE_PANE_REVIEW=0` in the environment Claude Code runs in | pane step on |
| Plain prompt preview | `DIFF_GATE_COLOR=0` in the environment Claude Code runs in | colors on |

A change in `/config` reloads the plugin at once. After you edit `settings.json` by hand, run
`/reload-plugins`.

## Using it

- **Turn terry off in a folder:** run `terry exclude` there (or `terry exclude <folder>`) in
  your own terminal. It stays off in every folder under it. `terry include` turns it on again.
- **Read the full patch in a terminal:** `delta < <patch path>`. The prompt shows the path.
- **Open the pane again:** `/terry`.
- **Approve or reject with the keyboard:** the pane takes the keys when it opens (unless you
  are typing). Press `a` or `r`. Arrows and Page Up/Down scroll. Esc gives the keys back.
- **No pane step:** with `DIFF_GATE_PANE_REVIEW=0`, or when the terry plugin is not enabled,
  the permission prompt is the only review.
- **In an IDE or another window that cannot draw the pane:** terry finds this itself. When
  the pane cannot be placed and no terminal is attached, the plugin sends that review to the
  permission prompt and, after Claude's turn, asks Claude to run the command again. The
  permission prompt then shows the full diff. If the pane does not show and nothing happens
  (for example, the plugin does not run in that host), set `DIFF_GATE_PANE_REVIEW=0` for that
  host. Claude cannot skip the pane step itself.
- **A fallback ESLint rule gets in the way:** turn it off in `lint/eslint.config.mjs`.

## Limits

- Only commands **Claude** runs are gated. Commits you make in your own terminal are not.
- The guards against Claude turning terry off read the command text. A hidden or encoded write
  to the approvals folder or the config file could get past them.
- While Claude Code shows a dialog (the permission prompt, a question), the pane gets no
  mouse wheel or keys, and a pane open beside the prompt can garble the terminal. That is why
  the review happens in the pane first, and why Approve closes the pane.
- The pane opens without your request only in a terminal 144 columns or wider (110 once you
  have opened it with `/terry`); in a narrower one, run `/terry`.
- The pane draws up to about 90,000 characters of diff; past that, it lists how many files
  it left out and where the full patch is.
- Lint reads the working copy. When that differs from what is being committed or pushed, the
  prompt says so.

## Tests

```sh
cargo test                     # unit tests, and the gate against throwaway git repos
claude plugin test plugin      # the pane plugin
```

Lint tests skip, with a note, when their tool is not installed.
