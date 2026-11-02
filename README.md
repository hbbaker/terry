# terry

A review gate for Claude Code. When Claude tries to **commit, or open, edit or merge a
pull request**, terry stops the command and shows you the diff before anything runs.

- **Diff review prompt.** The permission prompt shows the commits, a file summary, and a
  colored preview of the diff. The full patch is saved under `~/.claude/diffs/`.
- **Lint first.** terry lints the lines the change adds (ruff and `ruff format` for Python,
  shellcheck for shell, ESLint for JS/TS). If lint fails, the command is denied, Claude must fix
  the problems, and **you approve every fix edit**. Then Claude runs the command again. A
  `.terry` file in a project can list files that are not linted.
- **Review in a pane first.** A "Diff review" pane opens beside the chat with the full diff,
  drawn with Claude Code's own diff colors, and **Approve** and **Reject** buttons. Claude waits
  while you scroll and read. Approve closes the pane, and Claude runs the command again: the
  permission prompt then asks you to confirm it.
- **Branch chain.** The "Files" pane and the permission prompt show the branches from the
  default branch down to the one checked out, such as `main -> dev -> subfeature`, one colour per
  layer. In a terminal too narrow for the pane, a band above the prompt shows it instead.
- **File tree.** A "Files" pane stays open beside the chat with terry's owl, the branch chain
  and every file of the repo (tracked files, and untracked ones that `.gitignore` does not
  hide). Changed files take the colour of their status (green added, yellow modified, red
  deleted, cyan untracked) and a letter right after the name (`A`, `M`, `D`, `?`), and a `✎`
  marks a file Claude edited and a `·` a file you changed. A file Claude is editing right now
  turns orange, and the top bar names it. The colours count every change since
  the branch last met what is published (its upstream, else `origin`'s default branch), so
  commits that landed stay marked until you push or open a PR. A file is bold while its change
  is not committed, and dim once a commit holds it; two commits on one file read as one change.
  In a repo where nothing changed since then, the tree is plain. A small Terry sits in a bar
  at the top of the pane, with the branch chain and a count of changed files; the bar stays in
  place while the tree scrolls under it. Click a folder to close or open it (or Tab to it and
  press Enter); a closed folder shows how many changed files it hides. The "Diff review" pane
  opens as a second tab on top of it.
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
| `~/.claude/settings.json` | Adds two `PreToolUse` hooks that run the terry binary, and `ask` rules for commit and `gh pr`. A backup goes to `~/.claude/backups/` first. The file is written back with its keys in A–Z order. |
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
terry's config file, and any Claude command that runs `terry exclude` or `terry include`. A
Claude edit to a project's `.terry` file asks, because that file can switch lint off. This
blocks the plain ways around the review. A forged approval would still only reach the
permission prompt: with the full diff, or, if it claims to come from the pane, with the pane
open again beside the prompt and a line under it that says the diff was not approved there.

| Folder | What it holds |
| --- | --- |
| `src/` | The `terry` binary: `gate_bash.rs` and `gate_edit.rs` (the two hooks), `shell.rs` (reads the command line), the diff builders, the lint runner, `terry_file.rs` and `glob.rs` (the `.terry` file and its patterns), the branch chain in `git.rs`, the pane approvals, the exclude config, and `install.rs`. |
| `tests/` | The gate tests, against throwaway git repos, and unit tests for the parser, the patterns and the branch chain. |
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
| Files that are not linted in a project | `.terry` at the repo root: `{"lint": {"exclude": ["vendor/", "*.generated.py"]}}` (see "Skip files from lint") | none |
| Name, colours and width of the Files pane | The `display` section of terry's config file (see "Display settings") | Terry, the colours in the table below, 34 columns |
| Pane scroll speed (rows per mouse wheel tick, 1 to 50) | `/config`, row "Diff pane scroll speed"; or `~/.claude/settings.json`: `"pluginConfigs": { "terry@terry": { "options": { "scrollRows": 5 } } }` | 3 |
| Skip the pane step | `DIFF_GATE_PANE_REVIEW=0` in the environment Claude Code runs in | pane step on |
| Plain prompt preview | `DIFF_GATE_COLOR=0` in the environment Claude Code runs in | colors on |

A change in `/config` reloads the plugin at once. After you edit `settings.json` by hand, run
`/reload-plugins`.

### Display settings

The name terry shows, the colours of the Files pane and the width it asks for are in the
`display` section of terry's config file, next to the `exclude` list. Every key is optional; this
is the file with every default written out:

```json
{
  "display": {
    "name": "Terry",
    "paneColumns": 34,
    "colors": {
      "added": "success",
      "modified": "warning",
      "deleted": "error",
      "untracked": "#7FD6D6",
      "claude": "claude",
      "owl": "#8FB8ED",
      "alert": "warning",
      "folder": "#8FB8ED",
      "branches": ["#8FB8ED", "#9BD39B", "#F2C879", "#E49AD0", "#7FD6D6", "#F29E8E"]
    }
  }
}
```

| Key | What it sets | Allowed |
| --- | --- | --- |
| `name` | The name in the Files pane, the band above the prompt, the empty Diff review pane and the `/terry` messages. The `terry` command, `/terry` itself and the gate's messages keep "terry". | 1 to 24 characters |
| `paneColumns` | The width the Files pane asks for when it opens: a request, not a grant (see below). | 24 to 100 |
| `colors.added`, `modified`, `deleted`, `untracked` | The colour of a file by its status. | a colour |
| `colors.claude` | Claude's colour: the file it is editing, its `✎` mark, and the top bar line. | a colour |
| `colors.owl` | The owl and his line while he watches. | a colour |
| `colors.alert` | The owl and his line while he holds a command. | a colour |
| `colors.folder` | The folders in the tree. | a colour |
| `colors.branches` | One colour per layer of the branch chain, from the default branch down; the list repeats when the chain is deeper. | 1 to 12 colours |

A colour is `#RRGGBB`, or a name from Claude Code's theme: `text`, `inverseText`, `inactive`,
`subtle`, `suggestion`, `remember`, `success`, `error`, `warning`, `merged`, `claude`,
`permission`, `planMode`, `autoAccept`, `promptBorder`, `bashBorder`, `ide`, `diffAdded`,
`diffRemoved`, `diffAddedDimmed`, `diffRemovedDimmed`, `diffAddedWord` and `diffRemovedWord`. A
theme name follows your theme; a hex colour stays the same in every theme.

A value that does not pass its check is replaced by the default, and the rest of the file still
applies. `terry config` prints the settings in effect as JSON, with a `problems` list that says
what was rejected and why; the plugin also writes each new problem to the debug log once. The
name and the colours change within 15 seconds of an edit; the width applies the next time the
pane opens (close it, then run `/terry`, or start a new session).

The width is only a request. Claude Code docks a pane at most 45% of the terminal wide, and it
keeps the width you last gave a plugin pane (by dragging it): that width wins over `paneColumns`,
in every session, and no plugin can change it. So `paneColumns` is the width of the first pane
you see; after you resize once, the pane keeps your width. Claude Code stores the kept width in
its own settings file, as `pluginPanes.dockColumns`. Terry does not touch it.

Claude cannot edit this file: the gates deny its commands and edits that name it. The commands
that turn terry off or on in a folder keep the `display` section.

## Using it

- **Turn terry off in a folder:** run `terry exclude` there (or `terry exclude <folder>`) in
  your own terminal. It stays off in every folder under it. `terry include` turns it on again.
- **See the branch chain:** look at the "Files" pane (or the band above the prompt in a narrow
  terminal), or at the `branch:` line in the permission prompt. `terry branches` prints the chain, one branch per line, root first. At
  each step terry picks the nearest local branch behind the one above it, and it stops at
  `main` or `master`.
- **Skip files from lint:** put a `.terry` file at the repo root, for example
  `{"lint": {"exclude": ["vendor/", "*.generated.py", "/docs/**"]}}`. The patterns work like
  `.gitignore`: `*` and `?` stay inside one name, `**` crosses folders, a pattern with no `/`
  matches at any depth, a leading or inner `/` starts at the repo root, and a trailing `/` means
  a folder. `!` and `[...]` are not supported. The review lists the files it skipped. A broken
  `.terry` file excludes nothing, and the review says why. Because the file can switch lint off,
  Claude's edits to it go to the permission prompt, and a commit that changes it carries a note.
- **Read the full patch in a terminal:** `delta < <patch path>`. The prompt shows the path.
- **See the files:** look at the "Files" pane. `terry tree` prints the same data as JSON: the
  repo root, the branch, the ref the changes count from (`since`), and the files nested in
  folders, each with its status against that point and whether a commit landed it.
- **Open the panes again:** `/terry` opens the "Files" pane, and the "Diff review" pane too when
  a diff was reviewed.
- **Approve or reject with the keyboard:** the pane takes the keys when it opens (unless you
  are typing). Press `y` to approve or `n` to reject. Arrows, Page Up/Down, Home and End scroll
  the diff, and `a` and `d` scroll it sideways (the ◀ and ▶ buttons do the same). Esc gives the
  keys back.
- **No pane step:** with `DIFF_GATE_PANE_REVIEW=0`, or when the terry plugin is not enabled,
  the permission prompt is the only review.
- **In an IDE or another window that cannot draw the pane:** terry finds this itself. When
  the pane cannot be placed and no terminal is attached, the plugin sends that review to the
  permission prompt and, after Claude's turn, asks Claude to run the command again. The
  permission prompt then shows the full diff. If the pane does not show and nothing happens
  (for example, the plugin does not run in that host), set `DIFF_GATE_PANE_REVIEW=0` for that
  host. Claude cannot skip the pane step itself.
- **A fallback ESLint rule gets in the way:** turn it off in `lint/eslint.config.mjs`.

### Reading the file tree

| You see | It means |
| --- | --- |
| `▾ src/` or `▸ src/` | An open or a closed folder. A click (or Tab and Enter) toggles it. A number after a closed folder is how many changed files it hides. |
| `✎` in orange | Claude's Edit or Write tool changed this file in this session. |
| Name in orange and bold | Claude is on this file: its latest Edit or Write call. It starts with the call, also while the call waits for your permission, and it moves to the next file when Claude starts another call. It ends when Claude's turn ends. |
| `✎ Claude is editing x.rs` in the top bar | The same, in the top bar; `+2` means two more files at once (parallel calls). |
| `·` | The change is not from Claude's Edit or Write: you made it, or a command Claude ran (`sed`, a script). |
| Yellow, green, red, cyan name | Modified, added, deleted, untracked. |
| `M`, `A`, `D`, `?` right after the name | The same status, as a letter. |
| Click on a changed file | Opens its diff against the same base, in a read-only "file diff" pane. `terry diff <file>` prints the same patch. |
| Bold | The change is not committed yet. |
| Dim | A commit holds the change (dim yellow reads as grey), or the file did not change. |

A file with several signs reads left to right: the mark says who, the colour says what happened,
bold or dim says whether it is committed. The orange `✎` stays on a file for the rest of the
session; its name goes back to its status colour when Claude moves to another file or its turn
ends. When the file is out of view, the tree scrolls to it (a file already in view leaves your
scroll place alone). The live marks cover Edit and Write only; a file that Claude creates has no
row until its call ends, so until then only the top bar names it.

## Limits

- Only commands **Claude** runs are gated. Commits you make in your own terminal are not.
- The guards against Claude turning terry off read the command text. A hidden or encoded write
  to the approvals folder or the config file could get past them.
- The `.terry` guard covers Claude's file edits. A shell command that writes `.terry` (for
  example `echo ... >> .terry`) is not stopped, but a commit that changes `.terry` carries a
  note in its review.
- The branch chain is a guess. Git stores no parent for a branch, so terry uses the nearest
  local branch behind the current one. After a squash merge, or when branches share a commit,
  it can differ from the stack you have in mind.
- While Claude Code shows a dialog (the permission prompt, a question), the pane gets no
  mouse wheel or keys, and a pane open beside the prompt can garble the terminal. That is why
  the review happens in the pane first, and why Approve closes the pane.
- The panes open without your request only in a terminal 144 columns or wider (110 once you
  have opened one with `/terry`); in a narrower one, run `/terry`. Claude Code docks a pane at
  most 45% of the width wide, and shows two open panes as tabs, not side by side: while you
  review a diff you see the diff, and the file tree is one tab away.
- The tree colours come from `git diff` against the point where the branch last met what is
  published, so a push or a PR clears them (after the next Bash, Edit or Write call, or within 15
  seconds, also for a push from another terminal). A `git pull` moves that point too, so pulled
  changes are not marked. A file changed outside Claude Code shows after the same delay.
  "Claude edited" counts only Edit and Write calls; a change made by a command Claude runs
  shows as yours.
- `terry tree` lists every file, and the plugin reads the whole answer. A very large repo can
  give more output than the plugin reads, and the tree then keeps its last good state.
- The pane draws up to about 90,000 characters of diff; past that, it lists how many files
  it left out and where the full patch is.
- The command above the diff is cut to 3 rows, with a note that says how many rows it left out,
  so a long commit message does not push the diff down. The permission prompt that follows shows
  the whole command.
- Claude Code runs the gate before the whole command line. When one line runs `git add`, `rm`,
  `mv`, `reset` or `restore --staged` and then `git commit`, terry replays those on a copy of
  the index, so the diff shows what the commit will hold. `git commit <paths>` shows only
  those paths. For other changes before the commit (`stash`, `checkout` and the like), the
  prompt warns that the diff may not match.
- terry reads the command line as a shell would. It finds a commit after `&&`, `;`, `|`, `if`,
  `then`, `do` or `{`, behind wrappers such as `sudo -u x`, `timeout 30` or `xargs`, inside
  `$(...)` or backticks, behind a backslash-newline, and when a heredoc message comes first.
  When `git commit` appears in a place terry cannot read (for example `find -exec`), the
  review shows an "unparsed command" and no diff.
- Lint reads the working copy. When that differs from what is being committed, the
  prompt says so.

## Tests

```sh
cargo test                     # unit tests, and the gate against throwaway git repos
claude plugin test plugin      # the pane plugin
```

Lint tests skip, with a note, when their tool is not installed.
