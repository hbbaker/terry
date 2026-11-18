# Limits

## Scope and security

- terry gates only commands that **Claude** runs. Commits you make in your own terminal are not
  gated.
- The guards against Claude turning terry off read the command text. A hidden or encoded write to
  the approvals folder or the config file can get past them.
- The `.terry` guard covers the file edits of Claude. A shell command that writes `.terry` (for
  example `echo ... >> .terry`) is not stopped. A commit that changes `.terry` carries a note in its
  review.

## Branch chain and tree colors

- The branch chain is a guess. Git stores no parent for a branch, so terry uses the nearest local
  branch behind the current one. After a squash merge, or when branches share a commit, the chain
  can differ from the stack you have in mind.
- The tree colors come from `git diff` against the point where the branch last met what is
  published. A push or a PR clears them (after the next Bash, Edit or Write call, or within 15
  seconds, also for a push from another terminal). A `git pull` moves that point too, so terry does
  not mark pulled changes. A file changed outside Claude Code shows after the same delay.
  "Claude edited" counts only Edit and Write calls. A change made by a command Claude runs shows as
  yours.
- `terry tree` lists every file, and the plugin reads the whole answer. A very large repo can give
  more output than the plugin reads. The tree then keeps its last good state.

## Panes

- While Claude Code shows a dialog (the permission prompt, a question), the pane gets no mouse
  wheel or keys. A pane open beside the prompt can garble the terminal. This is why the review
  happens in the pane first, and why Approve closes the pane.
- The panes open without your request only in a terminal 144 columns or wider (110 once you have
  opened one with `/terry`). In a narrower terminal, run `/terry`. Claude Code docks a pane at most
  45% of the width wide, and shows two open panes as tabs, not side by side. While you review a
  diff you see the diff, and the file tree is one tab away.
- The pane draws up to about 90,000 characters of diff. Past that, it lists how many files it left
  out and where the full patch is.
- The command above the diff is cut to 3 rows, with a note that says how many rows were left out.
  A long commit message does not push the diff down. The permission prompt that follows shows the
  whole command.

## Command parsing

- Claude Code runs the gate before the whole command line. When one line runs `git add`, `rm`,
  `mv`, `reset` or `restore --staged` and then `git commit`, terry replays those on a copy of the
  index. The diff then shows what the commit will hold. `git commit <paths>` shows only those
  paths. For other changes before the commit (`stash`, `checkout` and the like), the prompt warns
  that the diff may not match.
- terry reads the command line as a shell would. It finds a commit after `&&`, `;`, `|`, `if`,
  `then`, `do` or `{`. It also finds one behind wrappers such as `sudo -u x`, `timeout 30` or
  `xargs`, inside `$(...)` or backticks, behind a backslash-newline, and when a heredoc message
  comes first. When `git commit` appears in a place terry cannot read (for example `find -exec`),
  the review shows an "unparsed command" and no diff.

## Lint and upload

- Lint reads the working copy. When that differs from what the commit holds, the prompt says so.
- terry does not take a drop of several files at once as an upload. The pane has no button to
  replace a file that exists. Run `terry upload --overwrite` for that.
