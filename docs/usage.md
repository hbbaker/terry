# Usage

## Common tasks

- **Turn terry off in a folder.** Run `terry exclude` there (or `terry exclude <folder>`) in your
  own terminal. terry stays off in every folder under it. `terry include` turns it on again.
- **See the branch chain.** Look at the "Files" pane (or the band above the prompt in a narrow
  terminal), or at the `branch:` line in the permission prompt. `terry branches` prints the chain,
  one branch for each line, root first. At each step terry picks the nearest local branch behind
  the one above it. It stops at `main` or `master`.
- **Read the full patch in a terminal.** Run `delta < <patch path>`. The prompt shows the path.
- **See the files.** Look at the "Files" pane. `terry tree` prints the same data as JSON: the repo
  root, the branch, the ref the changes count from (`since`), and the files nested in folders.
  Each file has its status against that point and whether a commit landed it.
- **Open the panes again.** `/terry` opens the "Files" pane. It also opens the "Diff review" pane
  when a diff was reviewed. When terry cannot run, or is off in the folder, the pane opens without
  files, and `/terry` answers "File tree opened, without files." with the reason.
- **Approve or reject with the keyboard.** The pane takes the keys when it opens (unless you type).
  Press `y` to approve or `n` to reject. Arrows, Page Up/Down, Home and End scroll the diff. `a`
  and `d` scroll it sideways (the ◀ and ▶ buttons do the same). Esc gives the keys back.
- **Skip the pane step.** Set `DIFF_GATE_PANE_REVIEW=0`. If the terry plugin is not enabled, the
  permission prompt is the only review.
- **Use an IDE or another window that cannot draw the pane.** terry detects this. When the pane
  cannot open and no terminal is attached, the plugin sends the review to the permission prompt.
  After the turn of Claude, it asks Claude to run the command again. The prompt then shows the full
  diff. If the pane does not show and nothing happens (for example, the plugin does not run in that
  host), set `DIFF_GATE_PANE_REVIEW=0` for that host. Claude cannot skip the pane step itself.
- **Turn off a fallback ESLint rule.** Edit `lint/eslint.config.mjs`.

## Skip files from lint

Put a `.terry` file at the repo root. For example:

```json
{"lint": {"exclude": ["vendor/", "*.generated.py", "/docs/**"]}}
```

The patterns work like `.gitignore`:

- `*` and `?` stay inside one name.
- `**` crosses folders.
- A pattern with no `/` matches at any depth.
- A leading or inner `/` starts at the repo root.
- A trailing `/` means a folder.
- `!` and `[...]` are not supported.

The review lists the files it skipped. A broken `.terry` file excludes nothing, and the review says
why. The file can switch lint off, so Claude's edits to it go to the permission prompt. A commit
that changes it carries a note.

Each linter uses the repo's own install and config first (`.venv/bin/ruff`, `.venv/bin/black`, a
local `node_modules/.bin/eslint` with an `eslint.config.*`). Then it uses the tool on your PATH.
Last, it uses the fallback ESLint in `lint/`. terry skips a tool that is not installed, and the
prompt says so.

## Reading the file tree

| You see | It means |
| --- | --- |
| `▾ src/` or `▸ src/` | An open or a closed folder. A click (or Tab and Enter) toggles it. A number after a closed folder is how many changed files it hides. |
| `✎` in orange | The Edit or Write tool of Claude changed this file in this session. |
| Name in orange and bold | Claude is on this file: its latest Edit or Write call. It starts with the call, also while the call waits for your permission. It moves to the next file when Claude starts another call. It ends when the turn of Claude ends. |
| `✎ Claude is editing x.rs` in the top bar | The same, in the top bar. `+2` means two more files at once (parallel calls). |
| `·` | The change is not from Edit or Write. You made it, or a command Claude ran (`sed`, a script). |
| Yellow, green, red, cyan name | Modified, added, deleted, untracked. |
| `M`, `A`, `D`, `?` after the name | The same status, as a letter. |
| Click on a changed file | Opens its diff against the same base, in a read-only "file diff" pane. `terry diff <file>` prints the same patch. |
| `Upload x.png to:` above the files | A file was dropped on the terminal. Press a folder (or `[repo root]`) to put the file there, or `✕ Cancel`. See [Dropping a file into a folder](#dropping-a-file-into-a-folder). |
| Bold | The change is not committed yet. |
| Dim | A commit holds the change (dim yellow reads as grey), or the file did not change. |

The marks on a file read left to right: the mark says who, the color says what happened, and bold
or dim says whether it is committed. The orange `✎` stays on a file for the rest of the session.
Its name goes back to its status color when Claude moves to another file or its turn ends. When the
file is out of view, the tree scrolls to it. A file already in view leaves your scroll place alone.
The live marks cover Edit and Write only. A file that Claude creates has no row until its call
ends. Until then, only the top bar names it.

## Dropping a file into a folder

Drop a file from your desktop on the terminal. The terminal pastes its path, and terry takes the
path out of the prompt box. The Files pane then shows `Upload <name> to:`. Press the folder that
must get the file. The tree reads again, and the pane says where the file landed.

`terry upload [--overwrite] [--from-user <name>] <file> <folder>` does the work and prints JSON.
The folder is a path from the repo root, and it must be inside the repo. terry does not replace a
file that is already there unless you pass `--overwrite`.

- **In a local session**, terry copies the file from the path.
- **In an SSH session** (`SSH_CONNECTION` is set), the path names a file on your desktop. terry
  pulls it with `scp -s` from the machine the session came from. It uses the same user name unless
  you pass `--from-user`. A Windows path (`D:\Downloads\a.png`) works too. This needs an SSH server
  and a key login (no password prompt) on the desktop. If the pull fails, the pane shows the `scp`
  command to run on the desktop.
