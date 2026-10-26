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

declare module 'claude-code' {
  interface PluginState {
    terry: {
      review: Review | null
      /** A held command to review in the permission prompt, as no pane can be drawn. */
      promptReviewFor: string | null
      gate: GateState
      /** The branches from the default branch down to the checked-out one, by `terry branches`; empty when none. */
      branchChain: string[]
      /** The pane's own scroll place: the index of the first diff row it shows. */
      diffRow: number
      /** How many columns the diff is scrolled to the left; 0 shows each line from its start. */
      diffColumn: number
    }
  }
}
