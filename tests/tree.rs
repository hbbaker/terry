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
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// The status word of the file at `path` (folders split on `/`), or `None` when the tree has no
/// such file.
fn status_of(tree: &Value, path: &str) -> Option<String> {
    let mut entries = tree["tree"].as_array()?;
    let mut parts = path.split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_some() {
            let folder = entries.iter().find(|entry| entry["dir"] == part)?;
            entries = folder["children"].as_array()?;
        } else {
            let file = entries.iter().find(|entry| entry["file"] == part)?;
            return file["status"].as_str().map(str::to_owned);
        }
    }
    None
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
