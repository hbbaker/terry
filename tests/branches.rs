use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use terry::git::branch_chain;

const GIT_IDENTITY: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "Gate Test"),
    ("GIT_AUTHOR_EMAIL", "gate@test.invalid"),
    ("GIT_COMMITTER_NAME", "Gate Test"),
    ("GIT_COMMITTER_EMAIL", "gate@test.invalid"),
];

static REPO_COUNT: AtomicUsize = AtomicUsize::new(0);

/// A throwaway repo on `main` with one commit.
struct Repo {
    path: PathBuf,
}

impl Repo {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "terry-branches-{}-{}",
            std::process::id(),
            REPO_COUNT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("makes the repo folder");
        let repo = Repo { path };
        repo.git(&["init", "--quiet", "-b", "main"]);
        repo.commit("initial");
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

    fn commit(&self, message: &str) {
        self.git(&["commit", "--quiet", "--allow-empty", "-m", message]);
    }

    /// Cuts `name` from the checked-out branch and checks it out.
    fn branch(&self, name: &str) {
        self.git(&["checkout", "--quiet", "-b", name]);
    }

    fn chain(&self) -> Vec<String> {
        branch_chain(&self.path)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn names(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn should_give_only_the_default_branch_when_it_is_checked_out() {
    let repo = Repo::new();

    assert_eq!(repo.chain(), names(&["main"]));
}

#[test]
fn should_list_each_branch_from_the_default_branch_down_when_branches_are_stacked() {
    let repo = Repo::new();
    repo.branch("dev");
    repo.commit("on dev");
    repo.branch("subfeature");
    repo.commit("on subfeature");

    assert_eq!(repo.chain(), names(&["main", "dev", "subfeature"]));
}

#[test]
fn should_name_the_stacked_branch_as_parent_when_both_sit_on_one_commit() {
    let repo = Repo::new();
    repo.branch("dev");
    repo.commit("on dev");
    repo.branch("subfeature");

    assert_eq!(repo.chain(), names(&["main", "dev", "subfeature"]));
}

#[test]
fn should_leave_out_a_branch_that_is_not_behind_the_checked_out_one() {
    let repo = Repo::new();
    repo.branch("sibling");
    repo.commit("on sibling");
    repo.git(&["checkout", "--quiet", "main"]);
    repo.branch("feature");
    repo.commit("on feature");

    assert_eq!(repo.chain(), names(&["main", "feature"]));
}

#[test]
fn should_pick_the_nearest_branch_when_several_are_behind() {
    let repo = Repo::new();
    repo.branch("old");
    repo.commit("on old");
    repo.git(&["checkout", "--quiet", "main"]);
    repo.branch("dev");
    repo.commit("one on dev");
    repo.commit("two on dev");
    repo.branch("feature");
    repo.commit("on feature");

    assert_eq!(repo.chain(), names(&["main", "dev", "feature"]));
}

#[test]
fn should_give_an_empty_chain_when_head_is_detached() {
    let repo = Repo::new();
    repo.git(&["checkout", "--quiet", "--detach"]);

    assert!(repo.chain().is_empty());
}

#[test]
fn should_give_an_empty_chain_when_the_folder_is_not_a_repo() {
    let folder = std::env::temp_dir().join(format!("terry-no-repo-{}", std::process::id()));
    fs::create_dir_all(&folder).expect("makes the folder");

    assert!(branch_chain(Path::new(&folder)).is_empty());

    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn should_stop_at_the_depth_limit_when_the_stack_is_deep() {
    let repo = Repo::new();
    for level in 0..12 {
        repo.branch(&format!("level-{level:02}"));
        repo.commit(&format!("on level {level}"));
    }

    let chain = repo.chain();

    assert_eq!(chain.len(), 8);
    assert_eq!(chain.last().map(String::as_str), Some("level-11"));
}
