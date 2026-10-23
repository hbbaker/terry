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
fn hunk_starts(line: &str) -> Option<(u64, u64)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(numbers: &[u64]) -> BTreeSet<u64> {
        numbers.iter().copied().collect()
    }

    #[test]
    fn should_list_new_side_line_numbers_of_added_lines() {
        let patch = "diff --git a/app.py b/app.py\n--- a/app.py\n+++ b/app.py\n@@ -1,3 +1,4 @@\n import os\n-x = 1\n+x = 2\n+y = 3\n print(x)\n";
        assert_eq!(added_lines_by_file(patch)["app.py"], lines(&[2, 3]));
    }

    #[test]
    fn should_leave_out_deleted_files_and_files_with_no_added_lines() {
        let patch = "diff --git a/gone.py b/gone.py\n--- a/gone.py\n+++ /dev/null\n@@ -1 +0,0 @@\n-x = 1\ndiff --git a/same.py b/same.py\n--- a/same.py\n+++ b/same.py\n@@ -1,2 +1 @@\n x\n-y\n";
        assert!(added_lines_by_file(patch).is_empty());
    }

    #[test]
    fn should_read_changed_lines_when_diff_comes_from_black() {
        let diff = "--- app.py\t2026-10-23 18:35:56+00:00\n+++ app.py\t2026-10-23 18:35:57+00:00\n@@ -1,3 +1,3 @@\n-x = {  'a':1 }\n+x = {\"a\": 1}\n y = 2\n+z = 3\n";
        assert_eq!(formatter_changed_lines(diff)["app.py"], lines(&[1, 3]));
    }

    #[test]
    fn should_read_changed_lines_when_diff_comes_from_ruff_format() {
        let diff = "--- bad.py\n+++ bad.py\n@@ -1,2 +1,2 @@\n x = 1\n-y=2\n+y = 2\n\n1 file would be reformatted\n";
        assert_eq!(formatter_changed_lines(diff)["bad.py"], lines(&[2]));
    }

    #[test]
    fn should_count_a_pure_insertion_at_the_line_it_goes_before() {
        let diff = "--- a.py\n+++ a.py\n@@ -1,2 +1,4 @@\n import os\n+\n+\n x = 1\n";
        assert_eq!(formatter_changed_lines(diff)["a.py"], lines(&[2]));
    }

    #[test]
    fn should_not_take_a_removed_dash_line_for_a_file_header() {
        let diff = "--- a.sql\n+++ a.sql\n@@ -1,2 +1,1 @@\n--- a comment\n x\n";
        let changed = formatter_changed_lines(diff);
        assert_eq!(changed.keys().collect::<Vec<_>>(), vec!["a.sql"]);
        assert_eq!(changed["a.sql"], lines(&[1]));
    }

    #[test]
    fn should_read_both_starts_of_a_hunk_header() {
        assert_eq!(hunk_starts("@@ -12,3 +14 @@ fn x"), Some((12, 14)));
        assert_eq!(hunk_starts("@@ -a +1 @@"), None);
    }
}
