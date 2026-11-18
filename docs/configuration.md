# Configuration

## Settings

| Setting | Where | Default |
| --- | --- | --- |
| Folders where terry is off | `terry exclude` and `terry include`. The list is in `~/.config/terry/config.json` (or under `$XDG_CONFIG_HOME`). | none |
| Files that lint skips | `.terry` at the repo root: `{"lint": {"exclude": ["vendor/", "*.generated.py"]}}`. See [usage](usage.md#skip-files-from-lint). | none |
| Name, colors and width of the Files pane | The `display` section of terry's config file. See below. | Terry, the colors below, 34 columns |
| Pane scroll speed (rows for each wheel tick, 1 to 50) | `/config`, row "Diff pane scroll speed". Or `~/.claude/settings.json`: `"pluginConfigs": { "terry@terry": { "options": { "scrollRows": 5 } } }` | 3 |
| Skip the pane step | `DIFF_GATE_PANE_REVIEW=0` in the environment of Claude Code | pane step on |
| Plain prompt preview | `DIFF_GATE_COLOR=0` in the environment of Claude Code | colors on |

A change in `/config` reloads the plugin at once. After you edit `settings.json` by hand, run
`/reload-plugins`.

## Display settings

The `display` section of terry's config file sets the name terry shows, the colors of the Files
pane, and the width it asks for. Every key is optional. This file shows every default:

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
| `name` | The name in the Files pane, the band above the prompt, the empty Diff review pane and the `/terry` messages. The `terry` command, `/terry` itself and the gate messages keep "terry". | 1 to 24 characters |
| `paneColumns` | The width the Files pane asks for when it opens. This is a request, not a grant (see below). | 24 to 100 |
| `colors.added`, `modified`, `deleted`, `untracked` | The color of a file, by status. | a color |
| `colors.claude` | The color for Claude: the file it edits, its `✎` mark, and the top bar line. | a color |
| `colors.owl` | The owl and his line while he watches. | a color |
| `colors.alert` | The owl and his line while he holds a command. | a color |
| `colors.folder` | The folders in the tree. | a color |
| `colors.branches` | One color for each layer of the branch chain, from the default branch down. The list repeats when the chain is deeper. | 1 to 12 colors |

A color is `#RRGGBB`, or a name from the theme of Claude Code: `text`, `inverseText`, `inactive`,
`subtle`, `suggestion`, `remember`, `success`, `error`, `warning`, `merged`, `claude`,
`permission`, `planMode`, `autoAccept`, `promptBorder`, `bashBorder`, `ide`, `diffAdded`,
`diffRemoved`, `diffAddedDimmed`, `diffRemovedDimmed`, `diffAddedWord` and `diffRemovedWord`. A
theme name follows your theme. A hex color stays the same in every theme.

terry replaces a value that fails its check with the default. The rest of the file still applies.
`terry config` prints the settings in effect as JSON, with a `problems` list that says what terry
rejected and why. The plugin also writes each new problem to the debug log once. The name and the
colors change within 15 seconds of an edit. The width applies the next time the pane opens: close
it, then run `/terry`, or start a new session.

### Pane width

The width is only a request. Claude Code docks a pane at most 45% of the terminal wide. It also
keeps the width you last gave a plugin pane (by a drag). That width wins over `paneColumns` in every
session, and no plugin can change it. So `paneColumns` sets the width of the first pane you see.
After you resize once, the pane keeps your width. Claude Code stores that width in its own settings
file, as `pluginPanes.dockColumns`. terry does not touch it.

### Protection

Claude cannot edit the config file: the gates deny its commands and edits that name it. The
commands that turn terry off or on in a folder keep the `display` section.
