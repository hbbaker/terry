//! `terry tree`: the files of a repo as a nested tree, each with how it differs from the last
//! commit. The pane plugin reads it to draw the file tree.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::{config, git};

/// How one file differs from the last commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Clean,
    Added,
    Modified,
    Deleted,
    Untracked,
}

impl Status {
    /// The word the plugin reads in the JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Clean => "clean",
            Status::Added => "added",
            Status::Modified => "modified",
            Status::Deleted => "deleted",
            Status::Untracked => "untracked",
        }
    }
}

/// Turns the two-letter code of `git status --porcelain` into a `Status`.
fn parse_status_code(code: &str) -> Status {
    match code {
        "??" => Status::Untracked,
        code if code.contains('D') => Status::Deleted,
        code if code.contains('A') => Status::Added,
        "  " => Status::Clean,
        _ => Status::Modified,
    }
}

/// `terry tree [folder]`: prints the tree of the folder's repo as JSON. The pane plugin reads it
/// to draw the file tree.
pub fn run(args: &[&str]) -> ExitCode {
    match config::target_dir(args).and_then(|dir| build(&dir)) {
        Ok(tree) => {
            println!("{tree}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::from(2)
        }
    }
}

/// The root folder, the branch and the file tree of the repo that holds `dir`, as JSON.
pub fn build(dir: &Path) -> Result<Value, String> {
    // Git prints status paths from the repo root, so every git call here runs at the root.
    let root = git::repo_root(dir)?;
    let mut paths = list_files(&root)?;
    let statuses = read_statuses(&root)?;
    add_deleted(&mut paths, &statuses);
    let tree = build_tree(&paths, &statuses);
    Ok(json!({
        "root": root.display().to_string(),
        "branch": git::current_branch(&root),
        "tree": to_json(&tree),
    }))
}

/// Adds the files that are deleted and staged. `git ls-files` no longer lists them, but the
/// tree must still show them.
fn add_deleted(paths: &mut Vec<String>, statuses: &BTreeMap<String, Status>) {
    for (path, status) in statuses {
        if *status == Status::Deleted && !paths.contains(path) {
            paths.push(path.clone());
        }
    }
}

/// The tree as nested JSON: `{"dir": name, "children": [...]}` and `{"file": name, "status": word}`.
fn to_json(nodes: &[Node]) -> Value {
    let entries = nodes.iter().map(|node| match node {
        Node::File { name, status } => json!({ "file": name, "status": status.as_str() }),
        Node::Dir { name, children } => json!({ "dir": name, "children": to_json(children) }),
    });
    Value::Array(entries.collect())
}

/// Every tracked file and every untracked file that .gitignore does not hide, as paths relative
/// to the repo root.
fn list_files(repo: &Path) -> Result<Vec<String>, String> {
    let output = git::git(
        repo,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    )?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The status of each changed file, keyed by its path. A file with no change is not in the map.
fn read_statuses(repo: &Path) -> Result<BTreeMap<String, Status>, String> {
    let output = git::git(
        repo,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let mut statuses = BTreeMap::new();
    let mut entries = output.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        let Some((code, path)) = entry.split_at_checked(2) else {
            continue;
        };
        if code.starts_with(['R', 'C']) {
            entries.next();
        }
        statuses.insert(path.trim_start().to_owned(), parse_status_code(code));
    }
    Ok(statuses)
}

/// One entry of the tree: a file, or a folder that holds more entries.
#[derive(Debug, PartialEq, Eq)]
pub enum Node {
    File { name: String, status: Status },
    Dir { name: String, children: Vec<Node> },
}

impl Node {
    fn name(&self) -> &str {
        match self {
            Node::File { name, .. } | Node::Dir { name, .. } => name,
        }
    }
}

/// Nests the paths into folders. A path with no entry in `statuses` is clean.
fn build_tree(paths: &[String], statuses: &BTreeMap<String, Status>) -> Vec<Node> {
    let mut roots = Vec::new();
    for path in paths {
        let status = statuses.get(path).copied().unwrap_or(Status::Clean);
        let parts: Vec<&str> = path.split('/').collect();
        insert(&mut roots, &parts, status);
    }
    sort_nodes(&mut roots);
    roots
}

/// Adds the file at `parts` (a path split on `/`) below `nodes`, making folders as needed.
fn insert(nodes: &mut Vec<Node>, parts: &[&str], status: Status) {
    match parts {
        [] => {}
        [name] => nodes.push(Node::File {
            name: (*name).to_owned(),
            status,
        }),
        [folder, rest @ ..] => insert(folder_children(nodes, folder), rest, status),
    }
}

/// The children of the folder called `name` in `nodes`. It makes the folder when it is missing.
fn folder_children<'a>(nodes: &'a mut Vec<Node>, name: &str) -> &'a mut Vec<Node> {
    let position = nodes
        .iter()
        .position(|node| matches!(node, Node::Dir { .. }) && node.name() == name);
    let index = position.unwrap_or_else(|| {
        nodes.push(Node::Dir {
            name: name.to_owned(),
            children: Vec::new(),
        });
        nodes.len() - 1
    });
    match &mut nodes[index] {
        Node::Dir { children, .. } => children,
        Node::File { .. } => unreachable!("the index points at a folder"),
    }
}

/// Puts folders before files, then sorts by name, at every level.
fn sort_nodes(nodes: &mut [Node]) {
    nodes.sort_by(|a, b| match (a, b) {
        (Node::Dir { .. }, Node::File { .. }) => Ordering::Less,
        (Node::File { .. }, Node::Dir { .. }) => Ordering::Greater,
        _ => a.name().cmp(b.name()),
    });
    for node in nodes {
        if let Node::Dir { children, .. } = node {
            sort_nodes(children);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_untracked_when_the_code_is_two_question_marks() {
        assert_eq!(parse_status_code("??"), Status::Untracked);
    }

    #[test]
    fn should_return_deleted_when_either_side_of_the_code_is_d() {
        assert_eq!(parse_status_code(" D"), Status::Deleted);
        assert_eq!(parse_status_code("D "), Status::Deleted);
    }

    #[test]
    fn should_return_added_when_the_code_has_an_a_and_no_d() {
        assert_eq!(parse_status_code("A "), Status::Added);
    }

    #[test]
    fn should_return_clean_when_the_code_is_two_spaces() {
        assert_eq!(parse_status_code("  "), Status::Clean);
    }

    #[test]
    fn should_return_modified_when_the_code_is_any_other_change() {
        assert_eq!(parse_status_code(" M"), Status::Modified);
        assert_eq!(parse_status_code("R "), Status::Modified);
    }

    fn file(name: &str, status: Status) -> Node {
        Node::File {
            name: name.to_owned(),
            status,
        }
    }

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|path| (*path).to_owned()).collect()
    }

    #[test]
    fn should_nest_files_under_one_folder_when_their_paths_share_it() {
        let tree = build_tree(&paths(&["src/a.rs", "src/b.rs"]), &BTreeMap::new());
        let expected = vec![Node::Dir {
            name: "src".to_owned(),
            children: vec![file("a.rs", Status::Clean), file("b.rs", Status::Clean)],
        }];
        assert_eq!(tree, expected);
    }

    #[test]
    fn should_list_folders_before_files_when_they_sit_in_the_same_folder() {
        let tree = build_tree(&paths(&["a.txt", "z/b.txt"]), &BTreeMap::new());
        let names: Vec<&str> = tree.iter().map(Node::name).collect();
        assert_eq!(names, ["z", "a.txt"]);
    }

    #[test]
    fn should_give_a_file_its_status_when_the_map_has_one() {
        let statuses = BTreeMap::from([("a.txt".to_owned(), Status::Modified)]);
        let tree = build_tree(&paths(&["a.txt", "b.txt"]), &statuses);
        assert_eq!(
            tree,
            [
                file("a.txt", Status::Modified),
                file("b.txt", Status::Clean)
            ]
        );
    }
}
