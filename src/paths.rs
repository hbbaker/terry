//! Where terry keeps its files, and path helpers that match what the Python gate did.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The checkout this binary was built from. `lint/`, the fallback ESLint, lives there.
pub fn checkout_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn claude_dir() -> PathBuf {
    home_dir().join(".claude")
}

/// Saved patches, call manifests, approvals and lint-fix marks.
pub fn diffs_dir() -> PathBuf {
    claude_dir().join("diffs")
}

pub fn claude_settings_path() -> PathBuf {
    claude_dir().join("settings.json")
}

pub fn claude_settings_local_path() -> PathBuf {
    claude_dir().join("settings.local.json")
}

/// terry's own config: `$XDG_CONFIG_HOME/terry/config.json`, else `~/.config/terry/config.json`.
pub fn config_path() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".config"))
        .join("terry")
        .join("config.json")
}

/// `arg` as a shell command names it from `base`: `~` expanded, then resolved.
pub fn resolve_arg(base: &Path, arg: &str) -> PathBuf {
    let expanded = if arg == "~" {
        home_dir()
    } else if let Some(rest) = arg.strip_prefix("~/") {
        home_dir().join(rest)
    } else {
        PathBuf::from(arg)
    };
    resolve(&base.join(expanded))
}

/// `path` made absolute with `.` and `..` folded, and links resolved in the part that exists,
/// as Python's non-strict `Path.resolve()` does. The part that does not exist is kept.
pub fn resolve(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir().unwrap_or_default().join(path)
    };
    let folded = fold_dots(&absolute);
    let mut existing = folded.clone();
    let mut missing = Vec::new();
    loop {
        if let Ok(real) = existing.canonicalize() {
            return missing
                .iter()
                .rev()
                .fold(real, |path, part| path.join(part));
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                missing.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            _ => return folded,
        }
    }
}

fn fold_dots(path: &Path) -> PathBuf {
    let mut folded = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                folded.pop();
            }
            other => folded.push(other),
        }
    }
    folded
}

/// True when `path` is `dir` or inside it. Both should be resolved.
pub fn is_within(path: &Path, dir: &Path) -> bool {
    path.starts_with(dir)
}

/// The first executable file named `name` in a `PATH` folder.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    let search = env::var_os("PATH")?;
    env::split_paths(&search)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

pub fn is_executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// Deletes the files in `dir` with this extension that were last changed before `max_age`
/// ago. Errors are ignored: pruning is housekeeping, never a reason to fail a review.
pub fn remove_older_than(dir: &Path, extension: &str, max_age: Duration) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|changed| now.duration_since(changed).unwrap_or_default() > max_age);
        if path.extension().is_some_and(|ext| ext == extension) && is_stale {
            let _ = fs::remove_file(path);
        }
    }
}
