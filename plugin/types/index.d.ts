/** One changed file: its path and its unified-diff hunks (empty for a binary or mode-only change). */
export type ReviewFile = { path: string; hunks: string }

/** One gated action of the call (a commit, a PR create, ...), as the pane draws it. */
export type ReviewSection = {
  title: string
  notes: string[]
  lint: string[]
  patchPath: string | null
  files: ReviewFile[]
  /** Files left out so one drawing stays under the engine's character limit. */
  omittedFiles: number
}

/**
 * Where the user reviews the call: `pane` first (Approve and Reject buttons, while the gate
 * holds the command), then `prompt` (the permission prompt, after an approval); or
 * `prompt_only`, the permission prompt alone with the full diff (no pane can be drawn).
 */
export type ReviewStage = 'pane' | 'prompt' | 'prompt_only'

/** The user's answer to a `pane` review; `pending` until they press a button. */
export type PaneDecision = 'pending' | 'approved' | 'rejected'

/** What the pane shows: the last call the gate reviewed. */
export type Review = {
  toolUseId: string
  command: string
  stage: ReviewStage
  /** The gate's name for this exact review; the Approve button writes an approval under it. */
  reviewKey: string
  decision: PaneDecision
  sections: ReviewSection[]
}

/**
 * Whether the gate is on for the session's folder, by `terry status`: `off` when an excluded
 * folder covers it, `unknown` when the binary did not answer (not installed, or it failed).
 */
export type GateState = 'on' | 'off' | 'unknown'

/** How a file differs from the last commit, as `terry tree` words it. */
export type TreeStatus = 'clean' | 'added' | 'modified' | 'deleted' | 'untracked'

/** One entry of `terry tree`: a file with its status, or a folder with its entries. */
export type TreeNode = { file: string; status: TreeStatus; landed: boolean } | { dir: string; children: TreeNode[] }

/** The colours of the Files pane, each `#RRGGBB` or a name from Claude Code's theme. */
export type Palette = {
  added: string
  modified: string
  deleted: string
  untracked: string
  /** Claude's own: the file it edits, and its mark. */
  claude: string
  /** The owl and his line at rest. */
  owl: string
  /** The owl and his line while he holds a command. */
  alert: string
  folder: string
  /** One colour per layer of the branch chain, from the default branch down. */
  branches: string[]
}

/** What `terry config` printed: the name, the width the tree pane asks for, the colours. */
export type DisplaySettings = {
  name: string
  paneColumns: number
  colors: Palette
  /** What was wrong in the config file; the defaults stand in for it. */
  problems: string[]
}

/**
 * What `terry tree` printed: the repo root (absolute), the checked-out branch, the ref the changes
 * count from (where the branch last met what is published), and the files. A file is `landed` when
 * a commit since then changed it and nothing newer is uncommitted.
 */
export type TreeSnapshot = { root: string; branch: string; since: string; nodes: TreeNode[] }

declare module 'claude-code' {
  interface PluginState {
    terry: {
      review: Review | null
      /** A held command to review in the permission prompt, as no pane can be drawn. */
      promptReviewFor: string | null
      gate: GateState
      /** The terry binary the gate's hooks run, from settings.json; `terry` on PATH when no hook names one. */
      binary: string
      /** The branches from the default branch down to the checked-out one, by `terry branches`; empty when none. */
      branchChain: string[]
      /** The display settings by `terry config`; the defaults until they were read. */
      display: DisplaySettings
      /** The file tree by `terry tree`; null until it was read, or when it cannot be read. */
      tree: TreeSnapshot | null
      /** Repo-relative paths Claude edited or wrote in this session. */
      claudeTouched: string[]
      /** Repo-relative paths with an Edit or Write call of Claude's in flight; one entry per call. */
      claudeActive: string[]
      /** The file of Claude's latest Edit or Write call: it stays Claude's colour until another call starts or the turn ends. */
      claudeLast: string | null
      /** Repo-relative paths of the folders the user collapsed in the file tree. */
      collapsed: string[]
      /** The file tree's own scroll place: the index of the first tree row it shows. */
      treeRow: number
      /** The pane's own scroll place: the index of the first diff row it shows. */
      diffRow: number
      /** How many columns the diff is scrolled to the left; 0 shows each line from its start. */
      diffColumn: number
      /** The diff of the file the user clicked in the tree, as a review that has no command and no gate; null until one was clicked. */
      fileDiff: Review | null
      /** The file diff pane's own scroll place: the index of the first row it shows. */
      fileDiffRow: number
      /** The path of a file the user dropped on the terminal, until they press a folder to put it in or cancel; null when none waits. */
      uploadSource: string | null
      /** What the last upload answered, shown in the Files pane; null before one ran, and after a new drop. */
      uploadNote: string | null
    }
  }
}
