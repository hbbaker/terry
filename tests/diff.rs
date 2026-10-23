use terry::diff::*;

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn should_spot_stage_all_flags_when_commit_has_them() {
    for flag in ["-a", "-am", "--all", "-va"] {
        assert!(is_stage_all_flag(flag), "{flag}");
    }
    for flag in ["-m", "--amend", "-", "-a1"] {
        assert!(!is_stage_all_flag(flag), "{flag}");
    }
}

#[test]
fn should_read_a_flag_value_in_each_spelling() {
    assert_eq!(
        flag_value(&strings(&["--base", "dev"]), &["--base", "-B"]),
        Some("dev")
    );
    assert_eq!(
        flag_value(&strings(&["-B", "dev"]), &["--base", "-B"]),
        Some("dev")
    );
    assert_eq!(
        flag_value(&strings(&["--base=dev"]), &["--base", "-B"]),
        Some("dev")
    );
    assert_eq!(
        flag_value(&strings(&["--title", "x"]), &["--base", "-B"]),
        None
    );
}

#[test]
fn should_find_the_pr_selector_when_command_names_one() {
    assert_eq!(find_pr_selector(&strings(&["20", "--merge"])), Some("20"));
    assert_eq!(find_pr_selector(&strings(&["--merge", "#20"])), Some("#20"));
    assert_eq!(
        find_pr_selector(&strings(&["--squash", "https://github.com/o/r/pull/7"])),
        Some("https://github.com/o/r/pull/7")
    );
    assert_eq!(
        find_pr_selector(&strings(&["--merge", "--delete-branch"])),
        None
    );
}
