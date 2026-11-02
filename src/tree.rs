//! `terry tree`: the files of a repo as a nested tree, each with how it differs from the point
//! where the branch last met what is published. The pane plugin reads it to draw the file tree.
//!
//! The colours count every change since that point, so commits that landed stay marked until the
//! user pushes or opens a PR.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::{config, git};

const HEAD: &str = "HEAD";
/// Git's name for the tree with no files. A repo with no commit yet has no HEAD, so every file
/// is a change against this.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const NO_COMMITS_LABEL: &str = "no commits yet";

/// How one file differs from the base.
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

/// What a file's change comes to: how it differs from the base, and whether that is all
/// committed. A file with a newer change that is not committed yet is not landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    pub status: Status,
    pub landed: bool,
}

/// The point the tree counts changes from, and a name to show for it.
#[derive(Debug, PartialEq, Eq)]
struct Base {
    commit: String,
    label: String,
}

/// The point where the branch last met what is published. A push moves it, so the marks clear.
/// Git has no word for it, so this tries, in order: the branch's upstream, the remote's default
/// branch, and the local default branch. With none of them it is HEAD, so only uncommitted
/// changes show. This never fails: a tree with fewer marks beats no tree.
fn find_base(repo: &Path) -> Base {
    published_ref(repo)
        .and_then(|reference| {
            let commit = merge_base(repo, &reference)?;
            Some(Base {
                commit,
                label: reference,
            })
        })
        .unwrap_or_else(|| head_base(repo))
}

/// The ref that stands for what is published, for the checked-out branch.
fn published_ref(repo: &Path) -> Option<String> {
    upstream(repo)
        .or_else(|| git::default_base_ref(repo, git::DEFAULT_REMOTE).ok())
        .or_else(|| git::default_branch(repo))
}

/// The branch the checked-out branch tracks, such as `origin/dev`.
fn upstream(repo: &Path) -> Option<String> {
    let name = git::git(
        repo,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .ok()?;
    Some(name.trim().to_owned())
}

/// The newest commit that HEAD and `reference` both hold.
fn merge_base(repo: &Path, reference: &str) -> Option<String> {
    let commit = git::git(repo, &["merge-base", HEAD, reference]).ok()?;
    Some(commit.trim().to_owned())
}

/// The base for a repo that has nothing to compare with: HEAD, or the empty tree before the
/// first commit.
fn head_base(repo: &Path) -> Base {
    if git::ref_exists(repo, HEAD) {
        return Base {
            commit: HEAD.to_owned(),
            label: HEAD.to_owned(),
        };
    }
    Base {
        commit: EMPTY_TREE.to_owned(),
        label: NO_COMMITS_LABEL.to_owned(),
    }
}

/// The letter `git diff --name-status` gives a file as a `Status`.
fn parse_diff_letter(letter: &str) -> Status {
    match letter {
        "A" => Status::Added,
        "D" => Status::Deleted,
        _ => Status::Modified,
    }
}

/// Reads the output of `git diff --name-status -z`: a letter, then a path, each ended by a NUL.
fn parse_name_status(output: &str) -> BTreeMap<String, Status> {
    let mut tokens = output.split('\0').filter(|token| !token.is_empty());
    let mut statuses = BTreeMap::new();
    while let (Some(letter), Some(path)) = (tokens.next(), tokens.next()) {
        statuses.insert(path.to_owned(), parse_diff_letter(letter));
    }
    statuses
}

/// Splits the output of a git command that ends each path with a NUL (`-z`). Git then prints
/// paths as they are, with no quotes around an odd name.
fn split_nul(output: &str) -> Vec<String> {
    output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect()
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

/// `terry diff <file>`: prints the patch of one file of the repo, against the same base the tree
/// marks count from. The pane plugin shows it when the user clicks a changed file.
pub fn run_diff(args: &[&str]) -> ExitCode {
    let result = match args {
        [file] => env::current_dir()
            .map_err(|error| format!("cannot read the current folder: {error}"))
            .and_then(|dir| file_diff(&dir, file)),
        _ => Err("give one file, as a path from the repo root".to_owned()),
    };
    match result {
        Ok(patch) => {
            print!("{patch}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::from(2)
        }
    }
}

/// The patch of `file` (a path from the repo root) against the base of the tree. A file that
/// git does not track yet shows as all added lines. A file with no change gives an empty patch.
pub fn file_diff(dir: &Path, file: &str) -> Result<String, String> {
    if !stays_in_repo(file) {
        return Err(format!("'{file}' is not a path inside the repo"));
    }
    let root = git::repo_root(dir)?;
    if list_untracked(&root)?.iter().any(|path| path == file) {
        return git::diff_new_file(&root, file);
    }
    let base = find_base(&root);
    git::diff(&root, &[&base.commit, "--", file])
}

/// True for a relative path with no `..` part.
fn stays_in_repo(file: &str) -> bool {
    let path = Path::new(file);
    path.is_relative()
        && !path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
}

/// The root folder, the branch, what the changes count from, and the file tree of the repo that
/// holds `dir`, as JSON.
pub fn build(dir: &Path) -> Result<Value, String> {
    // Git prints some paths from the repo root, so every git call here runs at the root.
    let root = git::repo_root(dir)?;
    let base = find_base(&root);
    let mut paths = list_files(&root)?;
    let statuses = read_statuses(&root, &base)?;
    let landed = read_landed(&root, &base)?;
    add_deleted(&mut paths, &statuses);
    let tree = build_tree(&paths, &statuses, &landed);
    Ok(json!({
        "root": root.display().to_string(),
        "branch": git::current_branch(&root),
        "since": base.label,
        "tree": to_json(&tree),
    }))
}

/// Adds the files that are deleted but that `git ls-files` no longer lists: deleted and staged,
/// or deleted in a commit that landed. The tree must still show them.
fn add_deleted(paths: &mut Vec<String>, statuses: &BTreeMap<String, Status>) {
    for (path, status) in statuses {
        if *status == Status::Deleted && !paths.contains(path) {
            paths.push(path.clone());
        }
    }
}

/// The tree as nested JSON: `{"dir": name, "children": [...]}` and
/// `{"file": name, "status": word, "landed": bool}`.
fn to_json(nodes: &[Node]) -> Value {
    let entries = nodes.iter().map(|node| match node {
        Node::File { name, change } => {
            json!({ "file": name, "status": change.status.as_str(), "landed": change.landed })
        }
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
    Ok(split_nul(&output))
}

/// The untracked files that .gitignore does not hide.
fn list_untracked(repo: &Path) -> Result<Vec<String>, String> {
    let output = git::git(repo, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    Ok(split_nul(&output))
}

/// How each file in the work tree differs from the base, keyed by its path. A file with no
/// difference is not in the map. Several commits on one file read as one change: a file added
/// in one commit and edited in the next is added.
fn read_statuses(repo: &Path, base: &Base) -> Result<BTreeMap<String, Status>, String> {
    let output = git::git(
        repo,
        &["diff", "--name-status", "--no-renames", "-z", &base.commit],
    )?;
    let mut statuses = parse_name_status(&output);
    for path in list_untracked(repo)? {
        statuses.insert(path, Status::Untracked);
    }
    Ok(statuses)
}

/// The files `git diff --name-only` lists for `range`.
fn diff_names(repo: &Path, range: &[&str]) -> Result<BTreeSet<String>, String> {
    let mut args = vec!["diff", "--name-only", "--no-renames", "-z"];
    args.extend_from_slice(range);
    Ok(split_nul(&git::git(repo, &args)?).into_iter().collect())
}

/// The files a commit since the base changed, and that have no newer change left uncommitted.
fn read_landed(repo: &Path, base: &Base) -> Result<BTreeSet<String>, String> {
    if !git::ref_exists(repo, HEAD) {
        return Ok(BTreeSet::new());
    }
    let committed = diff_names(repo, &[&base.commit, HEAD])?;
    let uncommitted = diff_names(repo, &[HEAD])?;
    Ok(committed.difference(&uncommitted).cloned().collect())
}

/// One entry of the tree: a file, or a folder that holds more entries.
#[derive(Debug, PartialEq, Eq)]
pub enum Node {
    File { name: String, change: Change },
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
fn build_tree(
    paths: &[String],
    statuses: &BTreeMap<String, Status>,
    landed: &BTreeSet<String>,
) -> Vec<Node> {
    let mut roots = Vec::new();
    for path in paths {
        let change = Change {
            status: statuses.get(path).copied().unwrap_or(Status::Clean),
            landed: landed.contains(path),
        };
        let parts: Vec<&str> = path.split('/').collect();
        insert(&mut roots, &parts, change);
    }
    sort_nodes(&mut roots);
    roots
}

/// Adds the file at `parts` (a path split on `/`) below `nodes`, making folders as needed.
fn insert(nodes: &mut Vec<Node>, parts: &[&str], change: Change) {
    match parts {
        [] => {}
        [name] => nodes.push(Node::File {
            name: (*name).to_owned(),
            change,
        }),
        [folder, rest @ ..] => insert(folder_children(nodes, folder), rest, change),
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

    const CLEAN: Change = Change {
        status: Status::Clean,
        landed: false,
    };

    #[test]
    fn should_return_added_when_the_letter_is_a() {
        assert_eq!(parse_diff_letter("A"), Status::Added);
    }

    #[test]
    fn should_return_deleted_when_the_letter_is_d() {
        assert_eq!(parse_diff_letter("D"), Status::Deleted);
    }

    #[test]
    fn should_return_modified_when_the_letter_is_any_other_change() {
        assert_eq!(parse_diff_letter("M"), Status::Modified);
        assert_eq!(parse_diff_letter("T"), Status::Modified);
    }

    #[test]
    fn should_pair_each_letter_with_its_path_when_git_prints_name_status() {
        let output = "M\0src/a.rs\0A\0docs/new file.md\0D\0old.txt\0";

        let statuses = parse_name_status(output);

        assert_eq!(statuses["src/a.rs"], Status::Modified);
        assert_eq!(statuses["docs/new file.md"], Status::Added);
        assert_eq!(statuses["old.txt"], Status::Deleted);
        assert_eq!(statuses.len(), 3);
    }

    #[test]
    fn should_ignore_a_letter_with_no_path_when_the_output_is_cut_short() {
        assert!(parse_name_status("M\0").is_empty());
    }

    #[test]
    fn should_use_the_empty_tree_when_the_repo_has_no_commit_yet() {
        let folder = std::env::temp_dir().join(format!("terry-tree-unit-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("makes the folder");
        git::git(&folder, &["init", "--quiet"]).expect("inits the repo");

        let base = head_base(&folder);
        let _ = std::fs::remove_dir_all(&folder);

        assert_eq!(base.commit, EMPTY_TREE);
        assert_eq!(base.label, NO_COMMITS_LABEL);
    }

    fn file(name: &str, status: Status, landed: bool) -> Node {
        Node::File {
            name: name.to_owned(),
            change: Change { status, landed },
        }
    }

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|path| (*path).to_owned()).collect()
    }

    #[test]
    fn should_nest_files_under_one_folder_when_their_paths_share_it() {
        let tree = build_tree(
            &paths(&["src/a.rs", "src/b.rs"]),
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        let clean = |name| Node::File {
            name: String::from(name),
            change: CLEAN,
        };
        let expected = vec![Node::Dir {
            name: "src".to_owned(),
            children: vec![clean("a.rs"), clean("b.rs")],
        }];
        assert_eq!(tree, expected);
    }

    #[test]
    fn should_list_folders_before_files_when_they_sit_in_the_same_folder() {
        let tree = build_tree(
            &paths(&["a.txt", "z/b.txt"]),
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        let names: Vec<&str> = tree.iter().map(Node::name).collect();
        assert_eq!(names, ["z", "a.txt"]);
    }

    #[test]
    fn should_give_a_file_its_status_and_landed_mark_when_the_maps_have_them() {
        let statuses = BTreeMap::from([
            ("a.txt".to_owned(), Status::Modified),
            ("b.txt".to_owned(), Status::Added),
        ]);
        let landed = BTreeSet::from(["b.txt".to_owned()]);

        let tree = build_tree(&paths(&["a.txt", "b.txt", "c.txt"]), &statuses, &landed);

        assert_eq!(
            tree,
            [
                file("a.txt", Status::Modified, false),
                file("b.txt", Status::Added, true),
                Node::File {
                    name: "c.txt".to_owned(),
                    change: CLEAN,
                },
            ]
        );
    }
}
