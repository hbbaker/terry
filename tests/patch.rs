use std::collections::BTreeSet;

use terry::patch::*;

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
