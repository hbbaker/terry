//! Reads unified diffs: the lines a patch adds, and the lines a formatter would change.

use std::collections::{BTreeMap, BTreeSet};

/// Line numbers by file path.
pub type LinesByFile = BTreeMap<String, BTreeSet<u64>>;

/// Each file the patch keeps, with the new-side line numbers it adds. Deleted files, and
/// files with no added lines, are left out.
pub fn added_lines_by_file(patch: &str) -> LinesByFile {
    let mut added = LinesByFile::new();
    let mut current: Option<String> = None;
    let mut new_line = 0;
    for line in patch.lines() {
        if line == "+++ /dev/null" {
            current = None;
            continue;
        }
        if let Some(path) = line.strip_prefix("+++ b/") {
            added.entry(path.to_owned()).or_default();
            current = Some(path.to_owned());
            continue;
        }
        if let Some((_, new_start)) = hunk_starts(line) {
            new_line = new_start;
            continue;
        }
        let Some(path) = &current else {
            continue;
        };
        if line.starts_with("---") || line.starts_with("diff --git") || line.starts_with('\\') {
            continue;
        }
        if line.starts_with('+') {
            added.entry(path.clone()).or_default().insert(new_line);
            new_line += 1;
        } else if line.starts_with(' ') {
            new_line += 1;
        }
    }
    added.retain(|_, lines| !lines.is_empty());
    added
}

/// Each file in a formatter's `--diff` output (black's or `ruff format`'s), with the current
/// line numbers the formatter would change. A removed line counts itself; a pure insertion
/// counts the line before which it goes. Added lines that replace removed ones count nothing
/// more.
pub fn formatter_changed_lines(diff: &str) -> LinesByFile {
    let mut changed = LinesByFile::new();
    let mut current: Option<String> = None;
    let mut old_line = 0;
    let mut after_removal = false;
    let mut lines = diff.lines().peekable();
    while let Some(line) = lines.next() {
        // A file header is `--- <path>` (black adds a tab and a time) followed by `+++`. Checking
        // the next line keeps a removed line that starts with "-- " from looking like one.
        if let Some(header) = line.strip_prefix("--- ")
            && lines.peek().is_some_and(|next| next.starts_with("+++ "))
        {
            let path = header.split('\t').next().unwrap_or(header);
            changed.entry(path.to_owned()).or_default();
            current = Some(path.to_owned());
            lines.next();
            continue;
        }
        if let Some((old_start, _)) = hunk_starts(line) {
            old_line = old_start;
            after_removal = false;
            continue;
        }
        let Some(path) = &current else {
            continue;
        };
        if line.starts_with('-') {
            changed.entry(path.clone()).or_default().insert(old_line);
            old_line += 1;
            after_removal = true;
        } else if line.starts_with('+') {
            if !after_removal {
                changed.entry(path.clone()).or_default().insert(old_line);
            }
        } else if line.starts_with(' ') {
            old_line += 1;
            after_removal = false;
        }
    }
    changed
}

/// The old and new start lines of a hunk header, `@@ -12,3 +14,5 @@`.
pub fn hunk_starts(line: &str) -> Option<(u64, u64)> {
    let ranges = line.strip_prefix("@@ -")?;
    let (old, rest) = ranges.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    Some((range_start(old)?, range_start(new)?))
}

/// The start of a hunk range, `12` or `12,3`.
fn range_start(range: &str) -> Option<u64> {
    let (start, count) = range.split_once(',').unwrap_or((range, "0"));
    let is_number = |text: &str| !text.is_empty() && text.chars().all(|c| c.is_ascii_digit());
    if is_number(start) && is_number(count) {
        start.parse().ok()
    } else {
        None
    }
}
