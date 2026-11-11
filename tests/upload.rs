use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use terry::upload::{Request, upload};

static FOLDER_COUNT: AtomicUsize = AtomicUsize::new(0);

/// A throwaway repo with a `src/` folder, and a separate folder that holds the file to upload.
struct Fixture {
    repo: PathBuf,
    desktop: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "terry-upload-{}-{}",
            std::process::id(),
            FOLDER_COUNT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        let desktop = base.join("desktop");
        fs::create_dir_all(repo.join("src")).expect("makes the repo folders");
        fs::create_dir_all(&desktop).expect("makes the desktop folder");
        let init = Command::new("git")
            .args(["init", "--quiet", "-b", "main"])
            .current_dir(&repo)
            .status()
            .expect("runs git");
        assert!(init.success(), "git init fails");
        Fixture { repo, desktop }
    }

    fn dropped_file(&self, name: &str, contents: &str) -> String {
        let path = self.desktop.join(name);
        fs::write(&path, contents).expect("writes the dropped file");
        path.display().to_string()
    }

    fn request(&self, source: String, folder: &str, overwrite: bool) -> Request {
        Request {
            source,
            folder: folder.to_owned(),
            overwrite,
            from_user: None,
        }
    }
}

#[test]
fn should_copy_the_file_into_the_folder_when_the_folder_is_in_the_repo() {
    let fixture = Fixture::new();
    let source = fixture.dropped_file("notes.txt", "hello");

    let dest = upload(&fixture.repo, &fixture.request(source, "src", false), None)
        .expect("uploads the file");

    assert_eq!(dest, "src/notes.txt");
    assert_eq!(
        fs::read_to_string(fixture.repo.join("src/notes.txt")).expect("reads the copy"),
        "hello"
    );
}

#[test]
fn should_copy_the_file_to_the_root_when_the_folder_is_empty() {
    let fixture = Fixture::new();
    let source = fixture.dropped_file("notes.txt", "hello");

    let dest =
        upload(&fixture.repo, &fixture.request(source, "", false), None).expect("uploads the file");

    assert_eq!(dest, "notes.txt");
}

#[test]
fn should_copy_the_file_when_the_pasted_path_is_quoted() {
    let fixture = Fixture::new();
    let source = fixture.dropped_file("my notes.txt", "hello");

    let dest = upload(
        &fixture.repo,
        &fixture.request(format!("'{source}'"), "src", false),
        None,
    )
    .expect("uploads the file");

    assert_eq!(dest, "src/my notes.txt");
}

#[test]
fn should_refuse_the_upload_when_the_file_exists_and_overwrite_is_off() {
    let fixture = Fixture::new();
    fs::write(fixture.repo.join("src/notes.txt"), "old").expect("writes the old file");
    let source = fixture.dropped_file("notes.txt", "new");

    let error = upload(&fixture.repo, &fixture.request(source, "src", false), None)
        .expect_err("refuses to replace");

    assert!(error.contains("already in that folder"), "{error}");
    assert_eq!(
        fs::read_to_string(fixture.repo.join("src/notes.txt")).expect("reads the file"),
        "old"
    );
}

#[test]
fn should_replace_the_file_when_overwrite_is_on() {
    let fixture = Fixture::new();
    fs::write(fixture.repo.join("src/notes.txt"), "old").expect("writes the old file");
    let source = fixture.dropped_file("notes.txt", "new");

    upload(&fixture.repo, &fixture.request(source, "src", true), None).expect("replaces the file");

    assert_eq!(
        fs::read_to_string(fixture.repo.join("src/notes.txt")).expect("reads the file"),
        "new"
    );
}

#[test]
fn should_refuse_the_upload_when_the_folder_is_outside_the_repo() {
    let fixture = Fixture::new();
    let source = fixture.dropped_file("notes.txt", "hello");

    let error = upload(
        &fixture.repo,
        &fixture.request(source, "../desktop", false),
        None,
    )
    .expect_err("refuses a folder outside the repo");

    assert!(error.contains("not a folder inside the repo"), "{error}");
}

#[test]
fn should_refuse_the_upload_when_the_folder_does_not_exist() {
    let fixture = Fixture::new();
    let source = fixture.dropped_file("notes.txt", "hello");

    let error = upload(
        &fixture.repo,
        &fixture.request(source, "missing", false),
        None,
    )
    .expect_err("refuses a missing folder");

    assert!(error.contains("not a folder"), "{error}");
}

#[test]
fn should_refuse_the_upload_when_the_source_is_missing() {
    let fixture = Fixture::new();
    let source = fixture.desktop.join("gone.txt").display().to_string();

    let error = upload(&fixture.repo, &fixture.request(source, "src", false), None)
        .expect_err("refuses a missing file");

    assert!(error.contains("is not a file on this machine"), "{error}");
    assert!(!fixture.repo.join("src/gone.txt").exists());
}

#[test]
fn should_leave_no_partial_file_when_the_upload_fails() {
    let fixture = Fixture::new();
    let source = fixture.desktop.join("gone.txt").display().to_string();

    let _ = upload(&fixture.repo, &fixture.request(source, "src", false), None);

    let leftovers = fs::read_dir(fixture.repo.join("src"))
        .expect("lists the folder")
        .count();
    assert_eq!(leftovers, 0);
}
