//! `terry install`: sets terry up on this machine from this checkout. Safe to run again.
//!
//! 1. Checks git (and the claude CLI, for the plugin).
//! 2. Checks the lint tools and delta on `PATH`. For each missing one it shows the install
//!    command for this system and asks; `--yes` installs without asking.
//! 3. Adds `git/delta.gitconfig` to `~/.gitconfig` with an `include.path`.
//! 4. Points `~/.claude/settings.json` at this binary: two PreToolUse hooks and `ask` rules for
//!    commit and `gh pr`. A backup goes to `~/.claude/backups/` first. The file is written
//!    back with its keys in A-Z order (serde_json keeps no key order).
//! 5. Checks that the binary's folder (`~/.cargo/bin`, where `install.sh` puts it) is on `PATH`,
//!    and removes the `~/.local/bin/terry` link that older installs made.
//! 6. Installs the terry pane plugin from `plugin/` (skip with `--no-plugin`).

use std::env;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use serde_json::{Value, json};

use crate::paths;
use crate::stamp;
use crate::text::shell_quote;

const BASH_MATCHER: &str = "Bash";
const EDIT_MATCHER: &str = "Edit|Write|MultiEdit|NotebookEdit";
const BASH_HOOK_TIMEOUT_SECONDS: u64 = 90;
const EDIT_HOOK_TIMEOUT_SECONDS: u64 = 10;
// Ask rules win over allow rules, so these keep a prompt even where a project allows the command.
const ASK_RULES: [&str; 4] = [
    "Bash(git commit *)",
    "Bash(gh pr create *)",
    "Bash(gh pr edit *)",
    "Bash(gh pr merge *)",
];
// Ask rule of an older terry that gated pushes; a settings file that has it loses it.
const RETIRED_ASK_RULES: [&str; 1] = ["Bash(git push *)"];
// Hook scripts of the Python terry; a hook entry that runs one is replaced.
const OLD_HOOK_SCRIPTS: [&str; 2] = ["diff-gate.py", "lint-fix-gate.py"];
const SMOKE_TEST_CALL: &str =
    r#"{"tool_name": "Bash", "tool_input": {"command": "ls"}, "cwd": "/"}"#;

struct Options {
    assume_yes: bool,
    plugin: bool,
}

/// A tool terry uses, and the ways to install it, best first.
struct Tool {
    name: &'static str,
    purpose: &'static str,
    installed: bool,
    installers: Vec<Installer>,
    manual: &'static str,
}

struct Installer {
    /// The program this way needs on `PATH`.
    needs: &'static str,
    command: String,
}

pub fn run(args: &[&str]) -> ExitCode {
    let mut options = Options {
        assume_yes: false,
        plugin: true,
    };
    for arg in args {
        match *arg {
            "--yes" => options.assume_yes = true,
            "--no-plugin" => options.plugin = false,
            other => {
                eprintln!("unknown option: {other}");
                return ExitCode::from(2);
            }
        }
    }
    match install(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

/// `terry uninstall [--purge]`: undoes `terry install`. The shell script `uninstall.sh` runs it,
/// then removes the binary itself.
pub fn run_uninstall(args: &[&str]) -> ExitCode {
    let mut purge = false;
    for arg in args {
        match *arg {
            "--purge" => purge = true,
            other => {
                eprintln!("unknown option: {other}");
                return ExitCode::from(2);
            }
        }
    }
    match uninstall(purge) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

fn uninstall(purge: bool) -> Result<(), String> {
    step("Removing terry's hooks and rules from ~/.claude/settings.json");
    unconfigure_claude_settings()?;

    step("Removing the delta include from ~/.gitconfig");
    remove_delta_include(&paths::checkout_dir().join("git").join("delta.gitconfig"))?;

    step("Removing the terry pane plugin");
    remove_plugin();

    step("Removing the old ~/.local/bin/terry link");
    if !remove_old_link()? {
        println!("  ok: none found");
    }

    if purge {
        step("Removing terry's exclude list and saved diffs");
        purge_data()?;
    } else {
        println!(
            "\nKept: the exclude list and saved diffs (run with --purge to remove them), and the\n\
             lint tools and delta that install added."
        );
    }
    Ok(())
}

fn unconfigure_claude_settings() -> Result<(), String> {
    let path = paths::claude_settings_path();
    if !path.exists() {
        println!("  ok: no settings file");
        return Ok(());
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut settings: Value = serde_json::from_str(&text)
        .map_err(|error| format!("{} is not valid JSON: {error}", path.display()))?;
    if !settings.is_object() || !unconfigure(&mut settings) {
        println!("  ok: nothing of terry's in {}", path.display());
        return Ok(());
    }
    let backup = back_up(&path)?;
    let text = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(&path, text + "\n")
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    println!("  removed: terry's hooks and ask rules");
    if let Some(backup) = backup {
        println!("  settings backup: {}", backup.display());
    }
    Ok(())
}

fn remove_delta_include(include: &Path) -> Result<(), String> {
    let include = include.display().to_string();
    let status = Command::new("git")
        .args([
            "config",
            "--global",
            "--fixed-value",
            "--unset-all",
            "include.path",
            &include,
        ])
        .status()
        .map_err(|error| format!("cannot run git config: {error}"))?;
    // git exits 5 when the key or value is not set.
    match status.code() {
        Some(0) => println!("  removed: include.path = {include}"),
        Some(5) => println!("  ok: not included"),
        _ => {
            return Err(format!(
                "`git config --global --unset-all include.path` failed ({status})"
            ));
        }
    }
    Ok(())
}

/// Removal is best effort: a missing `claude`, or a plugin that is not installed, is not an error.
fn remove_plugin() {
    let commands: [&[&str]; 2] = [
        &["plugin", "uninstall", "terry@terry"],
        &["plugin", "marketplace", "remove", "terry"],
    ];
    for args in commands {
        match Command::new("claude").args(args).status() {
            Ok(status) if status.success() => println!("  ok: claude {}", args.join(" ")),
            Ok(_) => println!(
                "  skipped: `claude {}` did not succeed (not installed?)",
                args.join(" ")
            ),
            Err(_) => println!("  skipped: the claude CLI is not on your PATH"),
        }
    }
}

fn purge_data() -> Result<(), String> {
    let config = paths::config_path();
    for path in [config.as_path(), paths::diffs_dir().as_path()] {
        let result = if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        match result {
            Ok(()) => println!("  removed: {}", path.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("cannot remove {}: {error}", path.display())),
        }
    }
    Ok(())
}

fn install(options: &Options) -> Result<(), String> {
    let checkout = paths::checkout_dir();
    let binary = env::current_exe()
        .and_then(|exe| exe.canonicalize())
        .map_err(|error| format!("cannot find this terry binary: {error}"))?;

    step("Checking prerequisites");
    require("git")?;
    if options.plugin {
        require("claude")?;
    }
    println!("  ok: git{}", if options.plugin { ", claude" } else { "" });

    step("Checking the lint tools and delta");
    for tool in tools(&checkout) {
        ensure_tool(&tool, options.assume_yes);
    }

    step("Adding delta colours to ~/.gitconfig");
    add_delta_include(&checkout.join("git").join("delta.gitconfig"))?;

    step("Pointing Claude Code at the gate");
    configure_claude_settings(&binary)?;
    smoke_test(&binary)?;

    step("Checking that terry is on your PATH");
    remove_old_link()?;
    check_on_path(&binary);

    if options.plugin {
        step("Installing the terry pane plugin from this folder");
        let status = Command::new("claude")
            .args(["plugin", "install", "terry", "--marketplace"])
            .arg(&checkout)
            .status()
            .map_err(|error| format!("cannot run claude: {error}"))?;
        if !status.success() {
            return Err(format!("`claude plugin install` failed ({status})"));
        }
    }

    println!(
        "\nterry is set up. Start a new Claude Code session (or run /reload-plugins in an open one).\n\
         Claude's git commit and gh pr create/edit/merge now stop for your review.\n\
         To turn terry off in a folder, run `terry exclude` there in your own terminal."
    );
    Ok(())
}

/// Warns when the folder of `binary` is not on `PATH`: the pane plugin and `terry exclude` run
/// `terry` by name. `cargo install` puts it in `~/.cargo/bin`, which rustup adds to `PATH`.
fn check_on_path(binary: &Path) {
    let Some(dir) = binary.parent() else { return };
    if dir_on_path(dir, env::var_os("PATH").as_deref()) {
        println!("  ok: {} is on your PATH", dir.display());
        return;
    }
    println!(
        "  note: {} is not on your PATH, so the pane cannot find terry.\n\
         \x20 Add this line to your shell's startup file (~/.zshrc or ~/.bashrc), then open a new terminal:\n\
         \x20   export PATH=\"{}:$PATH\"",
        dir.display(),
        dir.display()
    );
}

fn dir_on_path(dir: &Path, path: Option<&OsStr>) -> bool {
    path.is_some_and(|path| env::split_paths(path).any(|entry| entry == dir))
}

/// The link `~/.local/bin/terry` that older installs made. Returns true when it removed one. A
/// regular file there is not terry's, so it stays.
fn remove_old_link() -> Result<bool, String> {
    let link = paths::home_dir().join(".local").join("bin").join("terry");
    let Ok(meta) = fs::symlink_metadata(&link) else {
        return Ok(false);
    };
    if !meta.file_type().is_symlink() {
        println!(
            "  skipped: {} is a file, not a link; terry left it alone",
            link.display()
        );
        return Ok(false);
    }
    fs::remove_file(&link).map_err(|error| format!("cannot remove {}: {error}", link.display()))?;
    println!("  removed: old link {}", link.display());
    Ok(true)
}

fn step(title: &str) {
    println!("\n==> {title}");
}

fn require(program: &str) -> Result<(), String> {
    paths::find_on_path(program)
        .map(|_| ())
        .ok_or_else(|| format!("{program} is not installed"))
}

fn tools(checkout: &Path) -> Vec<Tool> {
    let cargo_delta = paths::home_dir().join(".cargo").join("bin").join("delta");
    let eslint_dir = checkout.join("lint");
    let installer = |needs: &'static str, command: &str| Installer {
        needs,
        command: command.to_owned(),
    };
    vec![
        Tool {
            name: "delta",
            purpose: "diff colours for git and the review prompt",
            installed: paths::find_on_path("delta").is_some() || paths::is_executable(&cargo_delta),
            installers: vec![
                installer("brew", "brew install git-delta"),
                installer("cargo", "cargo install --locked git-delta"),
            ],
            manual: "https://dandavison.github.io/delta/installation.html",
        },
        Tool {
            name: "ruff",
            purpose: "Python lint and format check",
            installed: paths::find_on_path("ruff").is_some(),
            installers: vec![
                installer("brew", "brew install ruff"),
                installer("uv", "uv tool install ruff"),
                installer("pipx", "pipx install ruff"),
                installer("curl", "curl -LsSf https://astral.sh/ruff/install.sh | sh"),
            ],
            manual: "https://docs.astral.sh/ruff/installation/",
        },
        Tool {
            name: "shellcheck",
            purpose: "shell script lint",
            installed: paths::find_on_path("shellcheck").is_some(),
            installers: vec![
                installer("brew", "brew install shellcheck"),
                installer("apt-get", "sudo apt-get install -y shellcheck"),
                installer("dnf", "sudo dnf install -y ShellCheck"),
                installer("pacman", "sudo pacman -S --noconfirm shellcheck"),
            ],
            manual: "https://github.com/koalaman/shellcheck#installing",
        },
        Tool {
            name: "eslint fallback",
            purpose: "JS/TS lint for repos with no ESLint of their own",
            installed: paths::is_executable(
                &eslint_dir.join("node_modules").join(".bin").join("eslint"),
            ),
            installers: vec![installer(
                "npm",
                &format!(
                    "cd {} && npm ci --no-audit --no-fund",
                    shell_quote(&eslint_dir.display().to_string())
                ),
            )],
            manual: "install Node.js and npm, then run this again",
        },
    ]
}

fn ensure_tool(tool: &Tool, assume_yes: bool) {
    if tool.installed {
        println!("  ok: {}", tool.name);
        return;
    }
    let Some(installer) = tool
        .installers
        .iter()
        .find(|installer| paths::find_on_path(installer.needs).is_some())
    else {
        println!(
            "  missing: {} ({}). Install it by hand: {}",
            tool.name, tool.purpose, tool.manual
        );
        return;
    };
    let question = format!(
        "  {} ({}) is not installed. Install it with `{}`?",
        tool.name, tool.purpose, installer.command
    );
    if !confirm(&question, assume_yes) {
        println!(
            "  skipped: {}. To install it later: {}",
            tool.name, installer.command
        );
        return;
    }
    match Command::new("sh").args(["-c", &installer.command]).status() {
        Ok(status) if status.success() => println!("  installed: {}", tool.name),
        Ok(status) => println!(
            "  FAILED: {} ({status}). Try it by hand: {}",
            tool.name, installer.command
        ),
        Err(error) => println!(
            "  FAILED: {} ({error}). Try it by hand: {}",
            tool.name, installer.command
        ),
    }
}

/// Asks a yes/no question on the terminal. With no terminal to ask (Claude ran this, or CI),
/// the answer is no.
fn confirm(question: &str, assume_yes: bool) -> bool {
    if assume_yes {
        println!("{question} yes (--yes)");
        return true;
    }
    let Ok(terminal) = OpenOptions::new().read(true).write(true).open("/dev/tty") else {
        println!("{question} no (no terminal to ask)");
        return false;
    };
    let mut writer = &terminal;
    if write!(writer, "{question} [y/N] ")
        .and_then(|()| writer.flush())
        .is_err()
    {
        return false;
    }
    let mut answer = String::new();
    if BufReader::new(&terminal).read_line(&mut answer).is_err() {
        return false;
    }
    answer.trim().eq_ignore_ascii_case("y") || answer.trim().eq_ignore_ascii_case("yes")
}

fn add_delta_include(include: &Path) -> Result<(), String> {
    let include = include.display().to_string();
    let listed = Command::new("git")
        .args(["config", "--global", "--get-all", "include.path"])
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("cannot run git config: {error}"))?;
    if String::from_utf8_lossy(&listed.stdout)
        .lines()
        .any(|line| line == include)
    {
        println!("  ok: already included");
        return Ok(());
    }
    let status = Command::new("git")
        .args(["config", "--global", "--add", "include.path", &include])
        .status()
        .map_err(|error| format!("cannot run git config: {error}"))?;
    if !status.success() {
        return Err(format!(
            "`git config --global --add include.path` failed ({status})"
        ));
    }
    println!("  added: include.path = {include}");
    Ok(())
}

fn configure_claude_settings(binary: &Path) -> Result<(), String> {
    let path = paths::claude_settings_path();
    let mut settings = if path.exists() {
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        serde_json::from_str(&text)
            .map_err(|error| format!("{} is not valid JSON: {error}", path.display()))?
    } else {
        json!({})
    };
    let backup = back_up(&path)?;
    configure(&mut settings, binary)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    fs::write(&path, text + "\n")
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    println!("  settings: hooks run {}", binary.display());
    if let Some(backup) = backup {
        println!("  settings backup: {}", backup.display());
    }
    Ok(())
}

fn back_up(settings: &Path) -> Result<Option<PathBuf>, String> {
    if !settings.exists() {
        return Ok(None);
    }
    let dir = paths::claude_dir().join("backups");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    let backup = dir.join(format!("settings.json.bak-terry-{}", stamp::utc_stamp()));
    fs::copy(settings, &backup).map_err(|error| format!("cannot back up the settings: {error}"))?;
    Ok(Some(backup))
}

pub fn hook_command(binary: &Path, gate: &str) -> String {
    format!("{} gate {gate}", shell_quote(&binary.display().to_string()))
}

/// True for a PreToolUse entry that runs terry: this binary's gates, or the Python scripts.
fn is_terry_hook(entry: &Value) -> bool {
    entry["hooks"].as_array().into_iter().flatten().any(|hook| {
        let command = hook["command"].as_str().unwrap_or_default();
        OLD_HOOK_SCRIPTS
            .iter()
            .any(|script| command.contains(script))
            || (command.contains("terry")
                && (command.ends_with(" gate bash") || command.ends_with(" gate edit")))
    })
}

/// Adds terry's hooks (replacing older ones) and ask rules, and keeps every other setting.
pub fn configure(settings: &mut Value, binary: &Path) -> Result<(), String> {
    let root = settings
        .as_object_mut()
        .ok_or("settings.json is not a JSON object")?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("settings.json: \"hooks\" is not an object")?;
    let pre_tool_use = hooks
        .entry("PreToolUse")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("settings.json: \"hooks.PreToolUse\" is not a list")?;
    pre_tool_use.retain(|entry| !is_terry_hook(entry));
    for (matcher, gate, timeout) in [
        (BASH_MATCHER, "bash", BASH_HOOK_TIMEOUT_SECONDS),
        (EDIT_MATCHER, "edit", EDIT_HOOK_TIMEOUT_SECONDS),
    ] {
        pre_tool_use.push(json!({
            "matcher": matcher,
            "hooks": [{"type": "command", "command": hook_command(binary, gate), "timeout": timeout}],
        }));
    }

    let ask = root
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("settings.json: \"permissions\" is not an object")?
        .entry("ask")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("settings.json: \"permissions.ask\" is not a list")?;
    ask.retain(|existing| !RETIRED_ASK_RULES.iter().any(|rule| existing == rule));
    for rule in ASK_RULES {
        if !ask.iter().any(|existing| existing == rule) {
            ask.push(json!(rule));
        }
    }
    Ok(())
}

/// Removes terry's hooks and ask rules, and keeps every other setting. A list or object that
/// this empties is removed too. Returns true when it changed anything.
pub fn unconfigure(settings: &mut Value) -> bool {
    let hooks_changed = remove_terry_hooks(settings);
    let rules_changed = remove_terry_ask_rules(settings);
    if hooks_changed {
        drop_if_empty(settings, "hooks");
    }
    if rules_changed {
        drop_if_empty(settings, "permissions");
    }
    hooks_changed || rules_changed
}

fn remove_terry_hooks(settings: &mut Value) -> bool {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return false;
    };
    let Some(pre_tool_use) = hooks.get_mut("PreToolUse").and_then(Value::as_array_mut) else {
        return false;
    };
    let before = pre_tool_use.len();
    pre_tool_use.retain(|entry| !is_terry_hook(entry));
    let removed = pre_tool_use.len() != before;
    if removed && pre_tool_use.is_empty() {
        hooks.remove("PreToolUse");
    }
    removed
}

fn remove_terry_ask_rules(settings: &mut Value) -> bool {
    let Some(permissions) = settings
        .get_mut("permissions")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let Some(ask) = permissions.get_mut("ask").and_then(Value::as_array_mut) else {
        return false;
    };
    let before = ask.len();
    ask.retain(|rule| {
        !ASK_RULES
            .iter()
            .chain(RETIRED_ASK_RULES.iter())
            .any(|terry_rule| rule == terry_rule)
    });
    let removed = ask.len() != before;
    if removed && ask.is_empty() {
        permissions.remove("ask");
    }
    removed
}

/// Removes `key` from the settings when it is an empty object, which terry's removal left behind.
fn drop_if_empty(settings: &mut Value, key: &str) {
    let Some(root) = settings.as_object_mut() else {
        return;
    };
    if root
        .get(key)
        .and_then(Value::as_object)
        .is_some_and(|object| object.is_empty())
    {
        root.remove(key);
    }
}

/// Runs the installed Bash gate once on a harmless call: it must answer with no output.
fn smoke_test(binary: &Path) -> Result<(), String> {
    let mut child = Command::new(binary)
        .args(["gate", "bash"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run the gate: {error}"))?;
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(SMOKE_TEST_CALL.as_bytes())
        .map_err(|error| format!("cannot talk to the gate: {error}"))?;
    let output = child
        .wait_with_output()
        .map_err(|error| format!("the gate did not finish: {error}"))?;
    if !output.status.success() || !output.stdout.is_empty() {
        return Err(format!(
            "the gate answered a plain `ls` ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stdout)
        ));
    }
    println!("  ok: the gate runs");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::path::Path;

    use super::dir_on_path;

    #[test]
    fn should_find_a_folder_that_is_on_path() {
        let path = env::join_paths(["/usr/bin", "/home/a/.cargo/bin"]).expect("joins");

        assert!(dir_on_path(Path::new("/home/a/.cargo/bin"), Some(&path)));
    }

    #[test]
    fn should_not_find_a_folder_that_is_missing_from_path() {
        let path = env::join_paths(["/usr/bin"]).expect("joins");

        assert!(!dir_on_path(Path::new("/home/a/.local/bin"), Some(&path)));
        assert!(!dir_on_path(Path::new("/home/a/.local/bin"), None));
    }
}
