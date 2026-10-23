//! Behaviour tests for the gates. Each test runs the real `terry` binary against a throwaway
//! git repo that pushes to a local bare remote, with a throwaway HOME, so no network, GitHub
//! or real settings are touched.
//!
//! Lint tests use the tools on PATH (ruff, shellcheck) and terry's fallback ESLint in `lint/`.
//! A test whose tool is not installed prints why and passes; `terry install` adds the tools.

use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

const TERRY: &str = env!("CARGO_BIN_EXE_terry");
const ANSI_ESCAPE: &str = "\x1b[";
const GIT_IDENTITY: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "Gate Test"),
    ("GIT_AUTHOR_EMAIL", "gate@test.invalid"),
    ("GIT_COMMITTER_NAME", "Gate Test"),
    ("GIT_COMMITTER_EMAIL", "gate@test.invalid"),
];

static SANDBOX_COUNT: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    root: PathBuf,
    home: PathBuf,
    repo: PathBuf,
    color: bool,
    path_override: Option<String>,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Sandbox {
    fn new() -> Self {
        let root = env::temp_dir().join(format!(
            "terry-test-{}-{}",
            std::process::id(),
            SANDBOX_COUNT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("home")).expect("makes the sandbox");
        let sandbox = Sandbox {
            home: root.join("home"),
            repo: root.join("work"),
            root,
            color: false,
            path_override: None,
        };
        let remote = sandbox.root.join("remote.git");
        sandbox.git(
            &sandbox.root,
            &[
                "init",
                "--quiet",
                "--bare",
                "-b",
                "main",
                &remote.display().to_string(),
            ],
        );
        sandbox.git(
            &sandbox.root,
            &[
                "init",
                "--quiet",
                "-b",
                "main",
                &sandbox.repo.display().to_string(),
            ],
        );
        sandbox.write_file("app.py", "print(\"v1\")\n");
        sandbox.git(&sandbox.repo, &["add", "app.py"]);
        sandbox.git(&sandbox.repo, &["commit", "--quiet", "-m", "initial"]);
        sandbox.git(
            &sandbox.repo,
            &["remote", "add", "origin", &remote.display().to_string()],
        );
        sandbox.git(&sandbox.repo, &["push", "--quiet", "-u", "origin", "main"]);
        sandbox.git(&sandbox.repo, &["remote", "set-head", "origin", "main"]);
        sandbox
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command
            .env("HOME", &self.home)
            .env("DIFF_GATE_COLOR", if self.color { "1" } else { "0" })
            .env_remove("DIFF_GATE_PANE_REVIEW")
            .env_remove("XDG_CONFIG_HOME")
            .envs(GIT_IDENTITY);
        if let Some(path) = &self.path_override {
            command.env("PATH", path);
        }
        command
    }

    fn git(&self, cwd: &Path, args: &[&str]) {
        let output = self
            .command("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("runs git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn write_file(&self, name: &str, content: &str) {
        let path = self.repo.join(name);
        fs::create_dir_all(path.parent().expect("has a folder")).expect("makes the folder");
        fs::write(path, content).expect("writes the file");
    }

    fn run_terry(&self, args: &[&str], cwd: &Path) -> Output {
        self.command(TERRY)
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("runs terry")
    }

    fn run_gate(&self, gate: &str, call: &Value) -> Option<Value> {
        let mut child = self
            .command(TERRY)
            .args(["gate", gate])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("runs the gate");
        child
            .stdin
            .take()
            .expect("stdin is piped")
            .write_all(call.to_string().as_bytes())
            .expect("sends the call");
        let output = child.wait_with_output().expect("the gate finishes");
        assert!(output.status.success(), "the gate exits 0");
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
        (!stdout.trim().is_empty()).then(|| serde_json::from_str(&stdout).expect("one JSON answer"))
    }

    fn run_hook(&self, command: &str) -> Option<Value> {
        self.run_hook_in(command, &self.repo, None)
    }

    fn run_hook_with_id(&self, command: &str, tool_use_id: &str) -> Option<Value> {
        self.run_hook_in(command, &self.repo, Some(tool_use_id))
    }

    fn run_hook_in(&self, command: &str, cwd: &Path, tool_use_id: Option<&str>) -> Option<Value> {
        let mut call = json!({
            "tool_name": "Bash",
            "tool_input": {"command": command},
            "cwd": cwd.display().to_string(),
        });
        if let Some(id) = tool_use_id {
            call["tool_use_id"] = json!(id);
        }
        self.run_gate("bash", &call)
    }

    fn run_edit_gate(&self, file: &Path) -> Option<Value> {
        let call = json!({
            "tool_name": "Edit",
            "tool_input": {"file_path": file.display().to_string()},
            "cwd": self.repo.display().to_string(),
        });
        self.run_gate("edit", &call)
    }

    fn decision_of(&self, output: Option<Value>, expected: &str) -> String {
        let output = output.expect("the gate answered");
        let decision = &output["hookSpecificOutput"];
        assert_eq!(decision["permissionDecision"], expected, "{output}");
        decision["permissionDecisionReason"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    fn reason_of(&self, output: Option<Value>) -> String {
        self.decision_of(output, "ask")
    }

    fn diffs_dir(&self) -> PathBuf {
        self.home.join(".claude").join("diffs")
    }

    fn call_manifest(&self, tool_use_id: &str) -> PathBuf {
        self.diffs_dir()
            .join("calls")
            .join(format!("{tool_use_id}.json"))
    }

    fn read_manifest(&self, tool_use_id: &str) -> Value {
        serde_json::from_str(
            &fs::read_to_string(self.call_manifest(tool_use_id)).expect("manifest exists"),
        )
        .expect("manifest is JSON")
    }

    fn saved_patches(&self) -> Vec<PathBuf> {
        let mut patches: Vec<PathBuf> = fs::read_dir(self.diffs_dir())
            .expect("diffs folder exists")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "patch"))
            .collect();
        patches.sort();
        patches
    }

    fn stage_lint_failure(&self) -> String {
        self.write_file("app.py", "import os\nprint(\"v2\")\n");
        self.git(&self.repo, &["add", "app.py"]);
        self.decision_of(self.run_hook("git commit -m \"v2\""), "deny")
    }

    fn stage_clean_change(&self) {
        self.write_file("app.py", "print(\"v2\")\n");
        self.git(&self.repo, &["add", "app.py"]);
    }

    fn enable_pane_plugin(&self) {
        let settings = self.home.join(".claude").join("settings.json");
        fs::create_dir_all(settings.parent().expect("has a folder")).expect("makes ~/.claude");
        fs::write(
            settings,
            json!({"enabledPlugins": {"terry@terry": true}}).to_string(),
        )
        .expect("writes the settings");
    }

    fn approvals_dir(&self) -> PathBuf {
        self.diffs_dir().join("approvals")
    }

    /// Does what the pane plugin does: writes the approval for that review. `via` is "pane"
    /// for the Approve button, "prompt" where no window could draw the pane.
    fn approve_in_pane(&self, tool_use_id: &str, via: &str) {
        let key = self.read_manifest(tool_use_id)["review_key"]
            .as_str()
            .expect("has a review key")
            .to_owned();
        fs::create_dir_all(self.approvals_dir()).expect("makes the folder");
        fs::write(
            self.approvals_dir().join(format!("{key}.json")),
            json!({"via": via}).to_string(),
        )
        .expect("writes the approval");
    }
}

fn on_path(name: &str) -> bool {
    env::var_os("PATH")
        .is_some_and(|path| env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

/// False, with a note, when the test's linter is not installed.
fn has_tool(name: &str) -> bool {
    let present = on_path(name);
    if !present {
        eprintln!("skipped: {name} is not on PATH (run `terry install`)");
    }
    present
}

fn has_fallback_eslint() -> bool {
    let present = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("lint/node_modules/.bin/eslint")
        .is_file();
    if !present {
        eprintln!("skipped: terry's fallback ESLint is not installed (run `terry install`)");
    }
    present
}

// --- diff review -------------------------------------------------------------

#[test]
fn should_return_ask_with_staged_diff_when_command_is_git_commit() {
    let sandbox = Sandbox::new();
    sandbox.stage_clean_change();

    let reason = sandbox.reason_of(sandbox.run_hook("git commit -m \"bump to v2\""));

    assert!(reason.contains("git commit: Staged changes"), "{reason}");
    assert!(reason.contains("+print(\"v2\")"), "{reason}");
    let patch = fs::read_to_string(&sandbox.saved_patches()[0]).expect("patch saved");
    assert!(patch.contains("+print(\"v2\")"));
}

#[test]
fn should_show_the_added_files_when_git_add_runs_in_the_same_command_as_the_commit() {
    let sandbox = Sandbox::new();
    sandbox.write_file("fresh.py", "FRESH = 1\n");

    let reason = sandbox.reason_of(sandbox.run_hook("git add -A && git commit -m \"add fresh\""));

    assert!(reason.contains("git commit: Staged changes"), "{reason}");
    assert!(reason.contains("+FRESH = 1"), "{reason}");
    assert!(reason.contains("nothing is staged yet"), "{reason}");
}

#[test]
fn should_leave_the_index_untouched_when_it_replays_git_add() {
    let sandbox = Sandbox::new();
    sandbox.write_file("fresh.py", "FRESH = 1\n");

    sandbox.run_hook("git add -A && git commit -m \"add fresh\"");

    let staged = sandbox
        .command("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&sandbox.repo)
        .output()
        .expect("runs git");
    assert!(
        staged.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&staged.stdout)
    );
}

fn staged_names(sandbox: &Sandbox) -> Vec<u8> {
    sandbox
        .command("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&sandbox.repo)
        .output()
        .expect("runs git")
        .stdout
}

#[test]
fn should_show_the_deletion_when_git_rm_runs_in_the_same_command_as_the_commit() {
    let sandbox = Sandbox::new();

    let reason = sandbox.reason_of(sandbox.run_hook("git rm app.py && git commit -m drop"));

    assert!(reason.contains("-print(\"v1\")"), "{reason}");
    assert!(
        sandbox.repo.join("app.py").exists(),
        "the gate must not delete files"
    );
    assert!(staged_names(&sandbox).is_empty());
}

#[test]
fn should_show_the_rename_when_git_mv_runs_in_the_same_command_as_the_commit() {
    let sandbox = Sandbox::new();

    let reason =
        sandbox.reason_of(sandbox.run_hook("git mv app.py main.py && git commit -m rename"));

    assert!(reason.contains("main.py"), "{reason}");
    assert!(
        sandbox.repo.join("app.py").exists(),
        "the gate must not move files"
    );
    assert!(!sandbox.repo.join("main.py").exists());
}

#[test]
fn should_show_the_named_paths_only_when_commit_names_files() {
    let sandbox = Sandbox::new();
    sandbox.write_file("app.py", "print(\"v2\")\n");
    sandbox.write_file("other.py", "OTHER = 1\n");
    sandbox.git(&sandbox.repo, &["add", "other.py"]);

    let reason = sandbox.reason_of(sandbox.run_hook("git commit app.py -m bump"));

    assert!(reason.contains("+print(\"v2\")"), "{reason}");
    assert!(!reason.contains("OTHER = 1"), "{reason}");
}

#[test]
fn should_warn_when_a_stash_runs_before_the_commit() {
    let sandbox = Sandbox::new();
    sandbox.stage_clean_change();

    let reason = sandbox.reason_of(sandbox.run_hook("git stash && git commit -m x"));

    assert!(reason.contains("git stash"), "{reason}");
    assert!(reason.contains("may not match"), "{reason}");
}

#[test]
fn should_exit_silently_when_command_is_git_push() {
    let sandbox = Sandbox::new();

    assert!(sandbox.run_hook("git push").is_none());
}

#[test]
fn should_return_ask_with_pr_diff_when_command_is_gh_pr_create() {
    let sandbox = Sandbox::new();
    sandbox.git(&sandbox.repo, &["checkout", "--quiet", "-b", "feat/pr"]);
    sandbox.write_file("app.py", "print(\"pr\")\n");
    sandbox.git(&sandbox.repo, &["commit", "--quiet", "-am", "pr change"]);

    let reason =
        sandbox.reason_of(sandbox.run_hook("gh pr create --base main --title \"PR\" --body \"x\""));

    assert!(
        reason.contains("New PR: feat/pr -> origin/main"),
        "{reason}"
    );
    assert!(reason.contains("+print(\"pr\")"), "{reason}");
}

#[test]
fn should_ask_when_gh_api_writes() {
    let sandbox = Sandbox::new();

    let reason = sandbox
        .reason_of(sandbox.run_hook("gh api repos/o/r/pulls -f title=x -f head=a -f base=main"));

    assert!(reason.contains("gh api (write)"), "{reason}");
}

#[test]
fn should_exit_silently_when_command_is_not_gated() {
    let sandbox = Sandbox::new();
    for command in [
        "git status",
        "git log --grep 'git push'",
        "gh api repos/o/r/pulls",
        "gh pr list",
    ] {
        assert!(sandbox.run_hook(command).is_none(), "{command}");
    }
}

#[test]
fn should_return_ask_with_error_text_when_cwd_is_not_a_git_repo() {
    let sandbox = Sandbox::new();
    let not_a_repo = sandbox.root.join("plain");
    fs::create_dir_all(&not_a_repo).expect("makes the folder");

    let reason = sandbox.reason_of(sandbox.run_hook_in("git commit -m x", &not_a_repo, None));

    assert!(reason.contains("Could not build the diff"), "{reason}");
}

#[test]
fn should_color_preview_when_color_is_on() {
    let mut sandbox = Sandbox::new();
    sandbox.color = true;
    sandbox.stage_clean_change();

    let reason = sandbox.reason_of(sandbox.run_hook("git commit -m \"v2\""));

    assert!(reason.contains(ANSI_ESCAPE), "{reason}");
}

// --- call manifest for the terry pane ----------------------------------------

#[test]
fn should_write_call_manifest_with_patch_path_when_gate_asks() {
    let sandbox = Sandbox::new();
    sandbox.stage_clean_change();

    sandbox.reason_of(sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_01"));

    let manifest = sandbox.read_manifest("toolu_01");
    let section = &manifest["sections"][0];
    assert_eq!(manifest["command"], "git commit -m \"v2\"");
    assert_eq!(section["title"], "git commit: Staged changes");
    assert!(
        section["lint"][0]
            .as_str()
            .is_some_and(|line| line.starts_with("LINT")),
        "{section}"
    );
    let patch = fs::read_to_string(section["patch_path"].as_str().expect("has a patch path"))
        .expect("patch saved");
    assert!(patch.contains("+print(\"v2\")"));
}

#[test]
fn should_not_write_call_manifest_when_lint_denies() {
    if !has_tool("ruff") {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.write_file("app.py", "import os\nprint(\"v2\")\n");
    sandbox.git(&sandbox.repo, &["add", "app.py"]);

    sandbox.decision_of(
        sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_02"),
        "deny",
    );

    assert!(!sandbox.call_manifest("toolu_02").exists());
}

// --- pane review (terry plugin on) -------------------------------------------

#[test]
fn should_deny_and_send_diff_to_pane_when_pane_plugin_is_on() {
    let sandbox = Sandbox::new();
    sandbox.enable_pane_plugin();
    sandbox.stage_clean_change();

    let reason = sandbox.decision_of(
        sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_10"),
        "deny",
    );

    assert_eq!(sandbox.read_manifest("toolu_10")["stage"], "pane");
    assert!(reason.contains("Approve or Reject"), "{reason}");
}

#[test]
fn should_ask_when_user_approved_the_same_diff_in_pane() {
    let sandbox = Sandbox::new();
    sandbox.enable_pane_plugin();
    sandbox.stage_clean_change();
    sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_11");
    sandbox.approve_in_pane("toolu_11", "pane");

    let reason = sandbox.reason_of(sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_12"));

    assert!(
        reason.contains("you approved this diff in the Diff review pane"),
        "{reason}"
    );
    assert_eq!(sandbox.read_manifest("toolu_12")["stage"], "prompt");
}

#[test]
fn should_show_summary_without_diff_lines_when_user_approved_in_pane() {
    let sandbox = Sandbox::new();
    sandbox.enable_pane_plugin();
    sandbox.stage_clean_change();
    sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_18");
    sandbox.approve_in_pane("toolu_18", "pane");

    let reason = sandbox.reason_of(sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_19"));

    assert!(
        reason.contains("git commit: Staged changes · 1 file changed"),
        "{reason}"
    );
    assert!(!reason.contains("+print(\"v2\")"), "{reason}");
}

#[test]
fn should_ask_with_full_diff_when_plugin_found_no_window_for_the_pane() {
    let sandbox = Sandbox::new();
    sandbox.enable_pane_plugin();
    sandbox.stage_clean_change();
    sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_20");
    sandbox.approve_in_pane("toolu_20", "prompt");

    let reason = sandbox.reason_of(sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_21"));

    assert!(reason.contains("+print(\"v2\")"), "{reason}");
    assert_eq!(sandbox.read_manifest("toolu_21")["stage"], "prompt_only");
}

#[test]
fn should_hold_command_for_pane_when_claude_adds_a_review_prefix() {
    let sandbox = Sandbox::new();
    sandbox.enable_pane_plugin();
    sandbox.stage_clean_change();

    sandbox.decision_of(
        sandbox.run_hook_with_id("TERRY_REVIEW=prompt git commit -m \"v2\"", "toolu_22"),
        "deny",
    );
}

#[test]
fn should_deny_command_that_names_the_approvals_folder() {
    let sandbox = Sandbox::new();

    let reason = sandbox.decision_of(
        sandbox.run_hook("echo '{}' > ~/.claude/diffs/approvals/abc.json"),
        "deny",
    );

    assert!(reason.contains("only the Diff review pane"), "{reason}");
}

#[test]
fn should_deny_edit_inside_the_approvals_folder() {
    let sandbox = Sandbox::new();

    let reason = sandbox.decision_of(
        sandbox.run_edit_gate(&sandbox.approvals_dir().join("abc.json")),
        "deny",
    );

    assert!(reason.contains("only the Diff review pane"), "{reason}");
}

#[test]
fn should_send_diff_to_pane_again_when_diff_changed_after_approval() {
    let sandbox = Sandbox::new();
    sandbox.enable_pane_plugin();
    sandbox.stage_clean_change();
    sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_13");
    sandbox.approve_in_pane("toolu_13", "pane");
    sandbox.write_file("app.py", "print(\"v3\")\n");
    sandbox.git(&sandbox.repo, &["add", "app.py"]);

    sandbox.decision_of(
        sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_14"),
        "deny",
    );
}

#[test]
fn should_use_up_approval_when_gate_asks() {
    let sandbox = Sandbox::new();
    sandbox.enable_pane_plugin();
    sandbox.stage_clean_change();
    sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_15");
    sandbox.approve_in_pane("toolu_15", "pane");
    sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_16");

    sandbox.decision_of(
        sandbox.run_hook_with_id("git commit -m \"v2\"", "toolu_17"),
        "deny",
    );
}

// --- lint --------------------------------------------------------------------

#[test]
fn should_report_lint_pass_when_added_lines_are_clean() {
    if !has_tool("ruff") {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.stage_clean_change();

    let reason = sandbox.reason_of(sandbox.run_hook("git commit -m \"v2\""));

    assert!(
        reason.contains("LINT passed on added lines (ruff, ruff format)"),
        "{reason}"
    );
}

#[test]
fn should_name_a_missing_linter_when_it_is_not_installed() {
    let mut sandbox = Sandbox::new();
    // A PATH with git alone: no ruff, shellcheck or delta.
    let bin = sandbox.root.join("bin");
    fs::create_dir_all(&bin).expect("makes the folder");
    let git = env::var_os("PATH")
        .and_then(|path| {
            env::split_paths(&path)
                .map(|dir| dir.join("git"))
                .find(|git| git.is_file())
        })
        .expect("git is on PATH");
    symlink(git, bin.join("git")).expect("links git");
    sandbox.path_override = Some(bin.display().to_string());
    sandbox.stage_clean_change();

    let reason = sandbox.reason_of(sandbox.run_hook("git commit -m \"v2\""));

    assert!(
        reason.contains("LINT skipped, not installed: ruff"),
        "{reason}"
    );
}

#[test]
fn should_deny_and_list_problem_when_added_line_fails_lint() {
    if !has_tool("ruff") {
        return;
    }
    let sandbox = Sandbox::new();

    let reason = sandbox.stage_lint_failure();

    assert!(reason.contains("app.py:1:8 [ruff F401]"), "{reason}");
    assert!(reason.contains("Run the same command again"), "{reason}");
}

#[test]
fn should_deny_when_added_line_needs_formatting() {
    if !has_tool("ruff") {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.write_file("app.py", "print('v2')\n");
    sandbox.git(&sandbox.repo, &["add", "app.py"]);

    let reason = sandbox.decision_of(sandbox.run_hook("git commit -m \"v2\""), "deny");

    assert!(reason.contains("app.py:1:1 [ruff format]"), "{reason}");
}

#[test]
fn should_not_block_when_lint_problem_is_on_a_line_the_change_does_not_add() {
    if !has_tool("ruff") {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.write_file("app.py", "import os\n\nprint(\"v1\")\n");
    sandbox.git(
        &sandbox.repo,
        &["commit", "--quiet", "-am", "old lint debt"],
    );
    sandbox.write_file("app.py", "import os\n\nprint(\"v2\")\n");
    sandbox.git(&sandbox.repo, &["add", "app.py"]);

    let reason = sandbox.reason_of(sandbox.run_hook("git commit -m \"v2\""));

    assert!(
        reason.contains("1 older problem(s) in touched files"),
        "{reason}"
    );
}

#[test]
fn should_lint_typescript_with_fallback_eslint() {
    if !has_fallback_eslint() {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.write_file("src/widget.ts", "const unused = 1;\nexport {};\n");
    sandbox.git(&sandbox.repo, &["add", "src/widget.ts"]);

    let reason = sandbox.decision_of(sandbox.run_hook("git commit -m \"ts\""), "deny");

    assert!(
        reason.contains("src/widget.ts:1:7 [eslint @typescript-eslint/no-unused-vars]"),
        "{reason}"
    );
}

#[test]
fn should_lint_shell_scripts_with_shellcheck() {
    if !has_tool("shellcheck") {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.write_file("run.sh", "#!/bin/bash\ncd $1\n");
    sandbox.git(&sandbox.repo, &["add", "run.sh"]);

    let reason = sandbox.decision_of(sandbox.run_hook("git commit -m \"sh\""), "deny");

    assert!(reason.contains("run.sh:2"), "{reason}");
    assert!(reason.contains("[shellcheck SC"), "{reason}");
}

// --- lint-fix flow -----------------------------------------------------------

#[test]
fn should_ask_on_edit_in_repo_when_lint_fix_is_pending() {
    if !has_tool("ruff") {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.stage_lint_failure();

    let reason = sandbox.reason_of(sandbox.run_edit_gate(&sandbox.repo.join("app.py")));

    assert!(reason.contains("Lint fix pending in work"), "{reason}");
}

#[test]
fn should_not_gate_edit_outside_repo_when_lint_fix_is_pending() {
    if !has_tool("ruff") {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.stage_lint_failure();

    assert!(
        sandbox
            .run_edit_gate(&sandbox.root.join("elsewhere.py"))
            .is_none()
    );
}

#[test]
fn should_not_gate_edit_when_no_lint_fix_is_pending() {
    let sandbox = Sandbox::new();

    assert!(
        sandbox
            .run_edit_gate(&sandbox.repo.join("app.py"))
            .is_none()
    );
}

#[test]
fn should_ask_for_auto_fix_command_when_lint_fix_is_pending() {
    if !has_tool("ruff") {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.stage_lint_failure();

    let reason = sandbox.reason_of(sandbox.run_hook("ruff check --fix app.py"));

    assert!(reason.contains("can rewrite files"), "{reason}");
}

#[test]
fn should_clear_pending_fix_and_ask_when_lint_passes_on_retry() {
    if !has_tool("ruff") {
        return;
    }
    let sandbox = Sandbox::new();
    sandbox.stage_lint_failure();
    sandbox.stage_clean_change();

    let reason = sandbox.reason_of(sandbox.run_hook("git commit -m \"v2\""));

    assert!(reason.contains("LINT passed"), "{reason}");
    assert!(
        sandbox
            .run_edit_gate(&sandbox.repo.join("app.py"))
            .is_none()
    );
}

// --- excluded folders --------------------------------------------------------

#[test]
fn should_let_git_actions_through_when_the_folder_is_excluded() {
    let sandbox = Sandbox::new();
    let excluded = sandbox.run_terry(&["exclude"], &sandbox.repo);
    assert!(
        excluded.status.success(),
        "{}",
        String::from_utf8_lossy(&excluded.stderr)
    );
    sandbox.stage_clean_change();

    assert!(sandbox.run_hook("git commit -m \"v2\"").is_none());
}

#[test]
fn should_let_git_actions_through_when_a_parent_folder_is_excluded() {
    let sandbox = Sandbox::new();
    sandbox.run_terry(
        &["exclude", &sandbox.root.display().to_string()],
        &sandbox.home,
    );
    sandbox.stage_clean_change();

    assert!(sandbox.run_hook("git commit -m \"v2\"").is_none());
}

#[test]
fn should_gate_again_when_the_folder_is_included_after_an_exclude() {
    let sandbox = Sandbox::new();
    sandbox.run_terry(&["exclude"], &sandbox.repo);
    let included = sandbox.run_terry(&["include"], &sandbox.repo);
    assert!(
        included.status.success(),
        "{}",
        String::from_utf8_lossy(&included.stderr)
    );
    sandbox.stage_clean_change();

    sandbox.reason_of(sandbox.run_hook("git commit -m \"v2\""));
}

#[test]
fn should_deny_when_claude_runs_terry_exclude() {
    let sandbox = Sandbox::new();

    let reason = sandbox.decision_of(sandbox.run_hook("cd /tmp && terry exclude"), "deny");

    assert!(reason.contains("only the user turns terry off"), "{reason}");
}

#[test]
fn should_deny_edit_of_the_config_file() {
    let sandbox = Sandbox::new();
    let config = sandbox
        .home
        .join(".config")
        .join("terry")
        .join("config.json");

    let reason = sandbox.decision_of(sandbox.run_edit_gate(&config), "deny");

    assert!(reason.contains("only the user turns terry off"), "{reason}");
}

#[test]
fn should_report_terry_on_when_no_folder_is_excluded() {
    let sandbox = Sandbox::new();

    let status = sandbox.run_terry(&["status"], &sandbox.repo);

    assert_eq!(status.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&status.stdout).starts_with("terry is on in "));
}

#[test]
fn should_report_terry_off_when_a_parent_folder_is_excluded() {
    let sandbox = Sandbox::new();
    sandbox.run_terry(
        &["exclude", &sandbox.root.display().to_string()],
        &sandbox.home,
    );

    let status = sandbox.run_terry(&["status"], &sandbox.repo);

    assert_eq!(status.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&status.stdout).contains("is excluded"));
}

#[test]
fn should_fail_status_when_given_two_folders() {
    let sandbox = Sandbox::new();

    let status = sandbox.run_terry(&["status", "/a", "/b"], &sandbox.repo);

    assert_eq!(status.status.code(), Some(2));
}
