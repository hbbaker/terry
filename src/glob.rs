//! Gitignore-style path patterns, for the `.terry` file.
//!
//! `*` matches any run of characters inside one folder or file name, `?` matches one
//! character, and `**` as a whole name matches any number of folders. A pattern with no `/`
//! (`*.py`, `vendor`) matches a name at any depth. A pattern with a `/` inside it, or a leading
//! `/`, starts at the repo root. A trailing `/` limits the pattern to folders. A pattern that
//! matches a folder matches everything under it. Negation (`!`) and `[...]` are not supported.

const ANY_FOLDERS: &str = "**";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glob {
    /// The pattern's names. An unanchored pattern starts with `**`.
    segments: Vec<String>,
    folders_only: bool,
}

impl Glob {
    pub fn new(pattern: &str) -> Result<Glob, String> {
        let pattern = pattern.trim();
        if pattern.starts_with('!') {
            return Err("a pattern that starts with ! is not supported".to_owned());
        }
        let folders_only = pattern.ends_with('/');
        let body = pattern.trim_end_matches('/');
        let (body, anchored) = match body.strip_prefix('/') {
            Some(rest) => (rest, true),
            None => (body, body.contains('/')),
        };
        let mut segments: Vec<String> = body
            .split('/')
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if segments.is_empty() {
            return Err("the pattern is empty".to_owned());
        }
        if !anchored {
            segments.insert(0, ANY_FOLDERS.to_owned());
        }
        Ok(Glob {
            segments,
            folders_only,
        })
    }

    /// True when `path` (relative to the repo root, with `/` between names) or one of its
    /// folders matches.
    pub fn matches(&self, path: &str) -> bool {
        let names: Vec<&str> = path.split('/').filter(|name| !name.is_empty()).collect();
        (1..=names.len())
            .filter(|&end| !(self.folders_only && end == names.len()))
            .any(|end| match_names(&self.segments, &names[..end]))
    }
}

fn match_names(pattern: &[String], names: &[&str]) -> bool {
    match pattern.split_first() {
        None => names.is_empty(),
        Some((first, rest)) if first == ANY_FOLDERS => {
            (0..=names.len()).any(|skip| match_names(rest, &names[skip..]))
        }
        Some((first, rest)) => names
            .split_first()
            .is_some_and(|(name, tail)| match_name(first, name) && match_names(rest, tail)),
    }
}

/// One name against one pattern name, with `*` and `?`.
fn match_name(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let (mut p, mut n) = (0, 0);
    // Where the last `*` was, and how much of the name it has taken so far.
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, n));
            p += 1;
        } else if let Some((star_p, star_n)) = star {
            p = star_p + 1;
            n = star_n + 1;
            star = Some((star_p, star_n + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}
