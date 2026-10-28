use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;
use terry::tree::build;

const GIT_IDENTITY: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "Gate Test"),
    ("GIT_AUTHOR_EMAIL", "gate@test.invalid"),
    ("GIT_COMMITTER_NAME", "Gate Test"),
    ("GIT_COMMITTER_EMAIL", "gate@test.invalid"),
];

static REPO_COUNT: AtomicUsize = AtomicUsize::new(0);

/// A throwaway repo on `main` with `README.md` and `src/lib.rs` committed.
struct Repo {
    path: PathBuf,
}

impl Repo {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "terry-tree-{}-{}",
            std::process::id(),
            REPO_COUNT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("makes the repo folder");
        let repo = Repo { path };
        repo.git(&["init", "--quiet", "-b", "main"]);
        repo.write("README.md", "readme");
        repo.write("src/lib.rs", "lib");
        repo.git(&["add", "."]);
        repo.git(&["commit", "--quiet", "-m", "initial"]);
        repo
    }

    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.path)
            .envs(GIT_IDENTITY)
            .output()
            .expect("runs git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn write(&self, relative: &str, contents: &str) {
        let file = self.path.join(relative);
        fs::create_dir_all(file.parent().expect("a file has a folder")).expect("makes the folder");
        fs::write(file, contents).expect("writes the file");
    }

    fn tree(&self) -> Value {
        build(&self.path).expect("builds the tree")
    }

    /// Writes the file and commits it.
    fn commit_file(&self, relative: &str, contents: &str) {
        self.write(relative, contents);
        self.git(&["add", relative]);
        self.git(&["commit", "--quiet", "-m", &format!("change {relative}")]);
    }

    /// Makes a bare repo the remote `origin`, and pushes `main` to it with `main` tracking it.
    fn publish_to_new_origin(&self) {
        let origin = PathBuf::from(format!("{}-origin.git", self.path.display()));
        let _ = fs::remove_dir_all(&origin);
        fs::create_dir_all(&origin).expect("makes the origin folder");
        let origin_path = origin.to_str().expect("a path is text");
        Command::new("git")
            .args(["init", "--quiet", "--bare", origin_path])
            .output()
            .expect("inits the origin");
        self.git(&["remote", "add", "origin", origin_path]);
        self.git(&["push", "--quiet", "-u", "origin", "main"]);
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
        let _ = fs::remove_dir_all(format!("{}-origin.git", self.path.display()));
    }
}

/// The tree entry of the file at `path` (folders split on `/`), or `None` when the tree has no
/// such file.
fn file_of<'a>(tree: &'a Value, path: &str) -> Option<&'a Value> {
    let mut entries = tree["tree"].as_array()?;
    let mut parts = path.split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_some() {
            let folder = entries.iter().find(|entry| entry["dir"] == part)?;
            entries = folder["children"].as_array()?;
        } else {
            return entries.iter().find(|entry| entry["file"] == part);
        }
    }
    None
}

/// The status word of the file at `path`, or `None` when the tree has no such file.
fn status_of(tree: &Value, path: &str) -> Option<String> {
    file_of(tree, path)?["status"].as_str().map(str::to_owned)
}

/// Whether the file at `path` is landed, or `None` when the tree has no such file.
fn landed_of(tree: &Value, path: &str) -> Option<bool> {
    file_of(tree, path)?["landed"].as_bool()
}

#[test]
fn should_name_the_branch_and_mark_files_clean_when_nothing_changed() {
    let repo = Repo::new();
    let tree = repo.tree();
    assert_eq!(tree["branch"], "main");
    assert_eq!(status_of(&tree, "README.md").as_deref(), Some("clean"));
    assert_eq!(status_of(&tree, "src/lib.rs").as_deref(), Some("clean"));
}

#[test]
fn should_name_the_repo_root_when_the_folder_is_a_subfolder() {
    let repo = Repo::new();
    let tree = build(&repo.path.join("src")).expect("builds the tree");
    let root = fs::canonicalize(&repo.path).expect("resolves the repo folder");
    assert_eq!(tree["root"], root.display().to_string());
}

#[test]
fn should_mark_a_file_modified_when_it_is_edited() {
    let repo = Repo::new();
    repo.write("src/lib.rs", "changed");
    assert_eq!(
        status_of(&repo.tree(), "src/lib.rs").as_deref(),
        Some("modified")
    );
}

#[test]
fn should_mark_a_file_untracked_when_it_is_new_and_not_ignored() {
    let repo = Repo::new();
    repo.write("docs/new.md", "new");
    assert_eq!(
        status_of(&repo.tree(), "docs/new.md").as_deref(),
        Some("untracked")
    );
}

#[test]
fn should_mark_a_file_added_when_it_is_new_and_staged() {
    let repo = Repo::new();
    repo.write("src/extra.rs", "extra");
    repo.git(&["add", "src/extra.rs"]);
    assert_eq!(
        status_of(&repo.tree(), "src/extra.rs").as_deref(),
        Some("added")
    );
}

#[test]
fn should_leave_a_file_out_when_gitignore_names_it() {
    let repo = Repo::new();
    repo.write(".gitignore", "*.log\n");
    repo.write("debug.log", "noise");
    assert_eq!(status_of(&repo.tree(), "debug.log"), None);
}

#[test]
fn should_mark_a_file_deleted_when_it_is_removed_from_disk() {
    let repo = Repo::new();
    fs::remove_file(repo.path.join("README.md")).expect("removes the file");
    assert_eq!(
        status_of(&repo.tree(), "README.md").as_deref(),
        Some("deleted")
    );
}

#[test]
fn should_still_list_a_file_when_it_is_deleted_and_staged() {
    let repo = Repo::new();
    repo.git(&["rm", "--quiet", "README.md"]);
    assert_eq!(
        status_of(&repo.tree(), "README.md").as_deref(),
        Some("deleted")
    );
}

#[test]
fn should_show_the_whole_repo_when_the_folder_is_a_subfolder() {
    let repo = Repo::new();
    repo.write("src/lib.rs", "changed");
    let tree = build(&repo.path.join("src")).expect("builds the tree");
    assert_eq!(status_of(&tree, "README.md").as_deref(), Some("clean"));
    assert_eq!(status_of(&tree, "src/lib.rs").as_deref(), Some("modified"));
}

#[test]
fn should_return_an_error_when_the_folder_is_not_a_repo() {
    let folder = std::env::temp_dir().join(format!("terry-tree-plain-{}", std::process::id()));
    fs::create_dir_all(&folder).expect("makes the folder");
    let result = build(Path::new(&folder));
    let _ = fs::remove_dir_all(&folder);
    assert!(result.is_err());
}

#[test]
fn should_keep_a_file_marked_when_it_was_committed_after_the_push() {
    let repo = Repo::new();
    repo.publish_to_new_origin();
    repo.commit_file("src/lib.rs", "changed and committed");

    let tree = repo.tree();

    assert_eq!(status_of(&tree, "src/lib.rs").as_deref(), Some("modified"));
    assert_eq!(landed_of(&tree, "src/lib.rs"), Some(true));
    assert_eq!(status_of(&tree, "README.md").as_deref(), Some("clean"));
}

#[test]
fn should_name_what_the_changes_count_from_when_the_branch_has_an_upstream() {
    let repo = Repo::new();
    repo.publish_to_new_origin();

    assert_eq!(repo.tree()["since"], "origin/main");
}

#[test]
fn should_report_one_status_for_a_file_when_two_commits_changed_it() {
    let repo = Repo::new();
    repo.publish_to_new_origin();
    repo.commit_file("src/new.rs", "first");
    repo.commit_file("src/new.rs", "second");

    assert_eq!(
        status_of(&repo.tree(), "src/new.rs").as_deref(),
        Some("added")
    );
}

#[test]
fn should_clear_the_marks_when_the_branch_is_pushed() {
    let repo = Repo::new();
    repo.publish_to_new_origin();
    repo.commit_file("src/lib.rs", "changed and committed");
    repo.git(&["push", "--quiet"]);

    let tree = repo.tree();

    assert_eq!(status_of(&tree, "src/lib.rs").as_deref(), Some("clean"));
    assert_eq!(landed_of(&tree, "src/lib.rs"), Some(false));
}

#[test]
fn should_not_call_a_file_landed_when_it_has_a_newer_uncommitted_change() {
    let repo = Repo::new();
    repo.publish_to_new_origin();
    repo.commit_file("src/lib.rs", "committed");
    repo.write("src/lib.rs", "committed, then edited again");

    let tree = repo.tree();

    assert_eq!(status_of(&tree, "src/lib.rs").as_deref(), Some("modified"));
    assert_eq!(landed_of(&tree, "src/lib.rs"), Some(false));
}

#[test]
fn should_count_from_the_default_branch_when_the_branch_has_no_upstream() {
    let repo = Repo::new();
    repo.publish_to_new_origin();
    repo.git(&["checkout", "--quiet", "-b", "feature"]);
    repo.commit_file("src/lib.rs", "feature work");

    let tree = repo.tree();

    assert_eq!(tree["since"], "origin/main");
    assert_eq!(status_of(&tree, "src/lib.rs").as_deref(), Some("modified"));
    assert_eq!(landed_of(&tree, "src/lib.rs"), Some(true));
}

#[test]
fn should_show_only_uncommitted_changes_when_there_is_no_remote() {
    let repo = Repo::new();
    repo.commit_file("src/lib.rs", "committed");
    repo.write("README.md", "edited");

    let tree = repo.tree();

    assert_eq!(status_of(&tree, "src/lib.rs").as_deref(), Some("clean"));
    assert_eq!(status_of(&tree, "README.md").as_deref(), Some("modified"));
}

#[test]
fn should_mark_a_file_deleted_when_a_landed_commit_removed_it() {
    let repo = Repo::new();
    repo.publish_to_new_origin();
    repo.git(&["rm", "--quiet", "README.md"]);
    repo.git(&["commit", "--quiet", "-m", "remove the readme"]);

    let tree = repo.tree();

    assert_eq!(status_of(&tree, "README.md").as_deref(), Some("deleted"));
    assert_eq!(landed_of(&tree, "README.md"), Some(true));
}
