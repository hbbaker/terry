use terry::text::*;

#[test]
fn should_find_a_word_only_where_it_stands_alone() {
    assert_eq!(
        word_matches("git gitx agit git", "git"),
        vec![(0, 3), (14, 17)]
    );
}

#[test]
fn should_remove_colour_codes_and_keep_other_text() {
    assert_eq!(strip_ansi("\x1b[1;32mok\x1b[0m done"), "ok done");
}

#[test]
fn should_join_unsafe_runs_into_one_dash_when_making_a_slug() {
    assert_eq!(slug("feat/x  y.z_1"), "feat-x-y.z_1");
}

#[test]
fn should_quote_a_path_when_it_has_a_space() {
    assert_eq!(shell_quote("/a b/it's"), r"'/a b/it'\''s'");
    assert_eq!(shell_quote("/usr/bin/terry"), "/usr/bin/terry");
}

#[test]
fn should_give_the_rest_after_each_whitespace() {
    let rests: Vec<&str> = after_whitespace("sed -i x").collect();
    assert_eq!(rests, vec!["-i x", "x"]);
}
