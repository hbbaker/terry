use std::path::{Path, PathBuf};

use terry::paths::*;

#[test]
fn should_fold_dots_and_keep_the_missing_part_when_resolving() {
    let resolved = resolve(Path::new("/no-such-dir-terry/a/../b/./c"));
    assert_eq!(resolved, PathBuf::from("/no-such-dir-terry/b/c"));
}

#[test]
fn should_resolve_a_relative_argument_from_the_base() {
    assert_eq!(
        resolve_arg(Path::new("/no-such-dir-terry/x"), "../y"),
        PathBuf::from("/no-such-dir-terry/y")
    );
}

#[test]
fn should_count_a_folder_as_within_itself() {
    assert!(is_within(Path::new("/a/b"), Path::new("/a/b")));
    assert!(is_within(Path::new("/a/b/c"), Path::new("/a/b")));
    assert!(!is_within(Path::new("/a/bc"), Path::new("/a/b")));
}
