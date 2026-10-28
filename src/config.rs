//! terry's own config: folders where terry stays off.
//!
//! The file is `~/.config/terry/config.json` (or under `$XDG_CONFIG_HOME`):
//! `{"exclude": ["/abs/folder", ...]}`. In an excluded folder, or anything under it, the gate
//! does not stop git or gh actions and the lint-fix gate does not stop edits.
//!
//! Only the user changes the list, with `terry exclude` and `terry include`. The gates deny
//! Claude's commands that run those, and its commands and edits that name this file.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Map, Value, json};

use crate::git;
use crate::paths;

const EXCLUDE_KEY: &str = "exclude";
pub const DENIAL: &str = "terry: only the user turns terry off for a folder, with `terry exclude` in their own terminal. Do not run `terry exclude` or `terry include`, and do not touch terry's config file. Ask the user if terry should be off here.";

/// The excluded folders, resolved. A missing or unreadable file excludes nothing, so a broken
/// config leaves the gate on.
pub fn excluded_dirs() -> Vec<PathBuf> {
    read_config()
        .ok()
        .and_then(|config| config.get(EXCLUDE_KEY).cloned())
        .and_then(|list| list.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
        .map(|dir| paths::resolve(Path::new(dir)))
        .collect()
}

pub fn is_excluded(path: &Path) -> bool {
    let path = paths::resolve(path);
    excluded_dirs()
        .iter()
        .any(|dir| paths::is_within(&path, dir))
}

/// True when `text` names terry's config file.
pub fn names_config(text: &str) -> bool {
    let config = paths::config_path();
    let parent = config
        .parent()
        .map(|dir| dir.display().to_string())
        .unwrap_or_default();
    text.contains(".config/terry") || (!parent.is_empty() && text.contains(&parent))
}

pub fn is_config_path(path: &Path) -> bool {
    paths::resolve(path) == paths::resolve(&paths::config_path())
}

fn read_config() -> Result<Map<String, Value>, String> {
    let path = paths::config_path();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    match serde_json::from_str(&text) {
        Ok(Value::Object(config)) => Ok(config),
        Ok(_) => Err(format!("{} is not a JSON object", path.display())),
        Err(error) => Err(format!("{} is not valid JSON: {error}", path.display())),
    }
}

fn write_config(config: &Map<String, Value>) -> Result<(), String> {
    let path = paths::config_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(config.clone()))
        .map_err(|error| error.to_string())?;
    fs::write(&path, text + "\n")
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// `terry exclude [folder]` and `terry include [folder]`: add the folder (the current one by
/// default) to the exclude list, or take it off.
pub fn run(args: &[&str], exclude: bool) -> ExitCode {
    match change_list(args, exclude) {
        Ok(message) => {
            println!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

/// `terry status [folder]`: exit 0 when terry is on in the folder, 1 when an excluded folder
/// covers it, 2 on an error. The pane plugin reads the exit code for its band.
pub fn status(args: &[&str]) -> ExitCode {
    let dir = match target_dir(args) {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("ERROR: {error}");
            return ExitCode::from(2);
        }
    };
    match excluded_dirs()
        .into_iter()
        .find(|excluded| paths::is_within(&dir, excluded))
    {
        Some(excluded) => {
            println!(
                "terry is off in {} because {} is excluded",
                dir.display(),
                excluded.display()
            );
            ExitCode::FAILURE
        }
        None => {
            println!("terry is on in {}", dir.display());
            ExitCode::SUCCESS
        }
    }
}

/// `terry branches [folder]`: the branch chain of the folder's repo, root first, one branch per
/// line. It prints nothing for a detached HEAD or a folder that is not a repo. The pane plugin
/// reads it for its band.
pub fn branches(args: &[&str]) -> ExitCode {
    match target_dir(args) {
        Ok(dir) => {
            for branch in git::branch_chain(&dir) {
                println!("{branch}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::from(2)
        }
    }
}

/// The one folder a config command names, resolved; the current folder when it names none.
pub(crate) fn target_dir(args: &[&str]) -> Result<PathBuf, String> {
    let dir = match args {
        [] => env::current_dir()
            .map_err(|error| format!("cannot read the current folder: {error}"))?,
        [dir] => PathBuf::from(dir),
        _ => return Err("give at most one folder".to_owned()),
    };
    Ok(paths::resolve(&dir))
}

fn change_list(args: &[&str], exclude: bool) -> Result<String, String> {
    let dir = target_dir(args)?;
    let shown = dir.display().to_string();
    let mut config = read_config()?;
    let mut list: Vec<String> = config
        .get(EXCLUDE_KEY)
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let listed = list
        .iter()
        .any(|entry| paths::resolve(Path::new(entry)) == dir);

    let message = match (exclude, listed) {
        (true, true) => format!("terry is already off in {shown}"),
        (true, false) => {
            list.push(shown.clone());
            format!("terry is now off in {shown} and every folder under it")
        }
        (false, true) => {
            list.retain(|entry| paths::resolve(Path::new(entry)) != dir);
            format!("terry is on again in {shown}")
        }
        (false, false) => {
            let covering = excluded_dirs()
                .into_iter()
                .find(|excluded| paths::is_within(&dir, excluded));
            return Ok(match covering {
                Some(parent) => format!(
                    "{shown} is not on the list itself, but terry is off there because {} is; run `terry include {}`",
                    parent.display(),
                    parent.display()
                ),
                None => format!("terry is already on in {shown}"),
            });
        }
    };
    config.insert(EXCLUDE_KEY.to_owned(), json!(list));
    write_config(&config)?;
    Ok(format!(
        "{message}\nconfig: {}",
        paths::config_path().display()
    ))
}
