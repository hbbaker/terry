use terry::terry_file::{TerryFile, is_changed_by};

fn parse(text: &str) -> TerryFile {
    TerryFile::parse(text).expect("a valid .terry file")
}

#[test]
fn should_exclude_nothing_when_the_file_has_no_lint_section() {
    let file = parse("{}");

    assert!(!file.excludes_from_lint("app.py"));
}

#[test]
fn should_exclude_the_files_when_they_match_a_lint_exclude_pattern() {
    let file = parse(r#"{"lint": {"exclude": ["vendor/", "*.generated.py"]}}"#);

    assert!(file.excludes_from_lint("vendor/a.py"));
    assert!(file.excludes_from_lint("src/api.generated.py"));
    assert!(!file.excludes_from_lint("src/api.py"));
}

#[test]
fn should_ignore_other_keys_when_the_file_has_them() {
    let file = parse(r#"{"future": 1, "lint": {"exclude": ["a.py"], "other": true}}"#);

    assert!(file.excludes_from_lint("a.py"));
}

#[test]
fn should_name_the_problem_when_the_file_is_not_valid() {
    for (text, expected) in [
        ("not json", "not valid JSON"),
        ("[]", "not a JSON object"),
        (r#"{"lint": []}"#, "\"lint\" is not an object"),
        (r#"{"lint": {"exclude": "a.py"}}"#, "is not a list"),
        (r#"{"lint": {"exclude": [1]}}"#, "not a string"),
        (r#"{"lint": {"exclude": ["!a.py"]}}"#, "not supported"),
    ] {
        let error = TerryFile::parse(text).expect_err(text);

        assert!(error.contains(expected), "{text}: {error}");
    }
}

#[test]
fn should_spot_a_patch_that_changes_the_file_at_the_repo_root() {
    assert!(is_changed_by(
        "diff --git a/.terry b/.terry\nnew file mode 100644\n"
    ));
    assert!(!is_changed_by("diff --git a/sub/.terry b/sub/.terry\n"));
    assert!(!is_changed_by("diff --git a/app.py b/app.py\n+.terry\n"));
}
