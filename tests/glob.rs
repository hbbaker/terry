use terry::glob::Glob;

fn matches(pattern: &str, path: &str) -> bool {
    Glob::new(pattern).expect("a valid pattern").matches(path)
}

#[test]
fn should_match_a_name_at_any_depth_when_the_pattern_has_no_slash() {
    assert!(matches("*.generated.py", "api.generated.py"));
    assert!(matches("*.generated.py", "src/deep/api.generated.py"));
    assert!(!matches("*.generated.py", "api.py"));
}

#[test]
fn should_match_only_from_the_repo_root_when_the_pattern_starts_with_a_slash() {
    assert!(matches("/build", "build/out.js"));
    assert!(!matches("/build", "src/build/out.js"));
}

#[test]
fn should_match_only_from_the_repo_root_when_the_pattern_has_a_slash_inside() {
    assert!(matches("docs/*.py", "docs/conf.py"));
    assert!(!matches("docs/*.py", "src/docs/conf.py"));
    assert!(!matches("docs/*.py", "docs/api/conf.py"));
}

#[test]
fn should_match_every_file_under_a_folder_when_the_pattern_names_the_folder() {
    assert!(matches("vendor/", "vendor/lib/a.py"));
    assert!(matches("vendor", "src/vendor/a.py"));
    assert!(!matches("vendor", "vendored/a.py"));
}

#[test]
fn should_not_match_a_file_when_the_pattern_ends_with_a_slash() {
    assert!(!matches("vendor/", "src/vendor"));
    assert!(matches("vendor/", "src/vendor/a.py"));
}

#[test]
fn should_match_any_number_of_folders_when_the_pattern_has_double_stars() {
    assert!(matches("docs/**/*.py", "docs/a.py"));
    assert!(matches("docs/**/*.py", "docs/x/y/a.py"));
    assert!(!matches("docs/**/*.py", "src/docs/a.py"));
    assert!(matches("/docs/**", "docs/x/y.txt"));
}

#[test]
fn should_match_one_character_when_the_pattern_has_a_question_mark() {
    assert!(matches("file?.py", "file1.py"));
    assert!(!matches("file?.py", "file12.py"));
}

#[test]
fn should_not_let_a_star_cross_a_folder_when_it_is_inside_a_name() {
    assert!(!matches("/src/*.py", "src/a/b.py"));
    assert!(matches("/src/*.py", "src/b.py"));
}

#[test]
fn should_reject_a_pattern_when_it_is_empty_or_negated() {
    assert!(Glob::new("").is_err());
    assert!(Glob::new("/").is_err());
    assert!(Glob::new("!keep.py").is_err());
}
