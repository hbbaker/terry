//! Small text helpers that stand in for the regular expressions of the Python gate.

/// A character of `\w`: a letter, a digit or `_`.
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The byte ranges where `word` stands alone in `text`, as `\bword\b` finds it.
pub fn word_matches(text: &str, word: &str) -> Vec<(usize, usize)> {
    text.match_indices(word)
        .map(|(start, _)| (start, start + word.len()))
        .filter(|&(start, end)| {
            !text[..start].chars().next_back().is_some_and(is_word_char)
                && !text[end..].chars().next().is_some_and(is_word_char)
        })
        .collect()
}

/// True when `word` stands alone somewhere in `text`.
pub fn has_word(text: &str, word: &str) -> bool {
    !word_matches(text, word).is_empty()
}

/// True when `text` starts with `word` and no word character follows it.
pub fn starts_with_word(text: &str, word: &str) -> bool {
    text.strip_prefix(word)
        .is_some_and(|rest| !rest.chars().next().is_some_and(is_word_char))
}

/// The rest of `text` after each whitespace character in it.
pub fn after_whitespace(text: &str) -> impl Iterator<Item = &str> {
    text.char_indices()
        .filter(|(_, c)| c.is_whitespace())
        .map(move |(index, c)| &text[index + c.len_utf8()..])
}

/// `text` without ANSI colour codes (ESC, `[`, digits and `;`, then a letter).
pub fn strip_ansi(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut kept = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            let mut end = index + 2;
            while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b';') {
                end += 1;
            }
            if end < bytes.len() && bytes[end].is_ascii_alphabetic() {
                index = end + 1;
                continue;
            }
        }
        kept.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(kept).expect("only whole ASCII escape sequences were removed")
}

/// `text` for a file name: each run of characters other than ASCII letters, digits, `.` and
/// `_` becomes one `-`.
pub fn slug(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut in_run = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
            slug.push(c);
            in_run = false;
        } else if !in_run {
            slug.push('-');
            in_run = true;
        }
    }
    slug
}

/// `text` for a file name: each character other than ASCII letters, digits, `_` and `-`
/// becomes `_`.
pub fn safe_file_name(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// `text` as one shell word: as it is when it holds only safe characters, else in single
/// quotes.
pub fn shell_quote(text: &str) -> String {
    let is_safe = |c: char| c.is_ascii_alphanumeric() || "@%+=:,./_-".contains(c);
    if !text.is_empty() && text.chars().all(is_safe) {
        return text.to_owned();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
