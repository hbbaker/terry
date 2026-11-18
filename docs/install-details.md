# Install details

## What the installer changes

`install.sh` installs the binary with `cargo install` and runs `terry install`. It changes only
these things outside the checkout:

| What | Change |
| --- | --- |
| Missing lint tools and delta | Installs each one only after you say yes (brew, uv, pipx, apt and so on, whichever this machine has). `--yes` installs all of them without a question. |
| `~/.gitconfig` | Adds `include.path` for `git/delta.gitconfig`. |
| `~/.claude/settings.json` | Adds two `PreToolUse` hooks that run the terry binary, and `ask` rules for commit and `gh pr`. A backup goes to `~/.claude/backups/` first. terry writes the file back with its keys in A–Z order. |
| `~/.claude/settings.local.json` | Removes the terry hooks, if an older install wrote them there (they run a build folder path, run the gate a second time, and break at `cargo clean`). A backup goes to `~/.claude/backups/` first. Nothing else in the file changes. |
| `~/.cargo/bin/terry` | The binary, put there by `cargo install`. |
| Claude Code plugins | Installs the `terry` pane plugin from `plugin/`. It removes the installed copy first, so Claude Code does not keep a stale cached copy. |

Run `./install.sh` again after a `git pull`. Use `./install.sh --no-plugin` to skip the pane.

## Where terry installs, and why

The binary goes to `~/.cargo/bin/terry` (under `$CARGO_HOME`, or `$CARGO_INSTALL_ROOT` if you set
it). Two things run `terry` by name, so the folder must be on your `PATH`: the pane plugin (it runs
`terry status`, `terry tree` and more), and you, when you type `terry exclude`. The gate hooks in
`~/.claude/settings.json` use the full path, so they do not depend on `PATH`.

Why `~/.cargo/bin`: Rust is already required, and rustup puts that folder on `PATH` for you. Older
versions linked the binary into `~/.local/bin`. That folder is not on `PATH` by default on macOS,
so the pane could not run `terry` and wrongly said "terry is off in this folder".

If the pane says it cannot run `terry status`, the last step of the installer names the folder to
add. Put `export PATH="$HOME/.cargo/bin:$PATH"` in `~/.zshrc` (or `~/.bashrc`), then open a new
terminal.

## Uninstall

```sh
./uninstall.sh            # remove terry, so ./install.sh can start from scratch
./uninstall.sh --purge    # also remove the exclude list and the saved diffs
```

The script reverses the install:

- It takes the terry hooks out of `~/.claude/settings.json` and `~/.claude/settings.local.json`.
  A backup of each goes to `~/.claude/backups/` first. Your other settings stay.
- It removes the delta `include.path` from `~/.gitconfig`.
- It uninstalls the pane plugin and removes the binary.
- It removes the `~/.local/bin/terry` link that older installs made.

It keeps the lint tools and delta (you chose to install them), this checkout, and the `ask` rules
for `git commit` and `gh pr` in `settings.json`. You may have had those rules before terry, so
delete them by hand if you want them gone.

You can run it as often as you like. A step that has nothing to do says so.

## How approval works

The approval is tied to a hash of the command, the folder and the patch. It works once and expires
after 30 minutes. If the diff changes, you review it in the pane again.

The permission prompt stays as the last check, because a plugin cannot answer it. The built-in
security plugin of Claude Code keeps permission decisions from third-party plugins.

Only the plugin writes approvals (`src/approvals.rs`), and only you change the exclude list. The
gates deny any Claude shell command or file edit that names `~/.claude/diffs/approvals` or the
config file of terry. They also deny any Claude command that runs `terry exclude` or
`terry include`. A Claude edit to the `.terry` file of a project asks, because that file can switch
lint off. This blocks the plain ways around the review. A forged approval still reaches only the
permission prompt: with the full diff, or, if it claims to come from the pane, with the pane open
again beside the prompt and a line under it that says the diff was not approved there.
