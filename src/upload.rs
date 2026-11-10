//! `terry upload`: puts a file the user dropped on the terminal into a folder of the repo.
//!
//! A terminal pastes the path of a dropped file as text. On this machine the path names the file
//! to copy. In an SSH session the path names a file on the user's desktop, so terry pulls the
//! file back over SSH from the machine the session came from.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

use serde_json::json;

use crate::{git, paths, process};

const OVERWRITE_FLAG: &str = "--overwrite";
const FROM_USER_FLAG: &str = "--from-user";
/// Set by `sshd` in a session: `client_ip client_port server_ip server_port`.
const SSH_CONNECTION_VAR: &str = "SSH_CONNECTION";
const SSH_CONNECTION_FIELDS: usize = 4;
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const SSH_CONNECT_TIMEOUT_SECS: u32 = 10;
const PARTIAL_SUFFIX: &str = ".terry-part";
/// `scp -s` moves the file over SFTP, so the remote path is never read by a shell on the desktop
/// (a Windows desktop has `cmd` or PowerShell there, not `sh`).
const SCP_FORCE_SFTP_FLAG: &str = "-s";

/// What the user asked for.
pub struct Request {
    /// The pasted path, as the terminal wrote it.
    pub source: String,
    /// The target folder, as a path from the repo root. Empty or `.` is the root.
    pub folder: String,
    pub overwrite: bool,
    /// The user name on the desktop, in an SSH session. Default: the user name here.
    pub from_user: Option<String>,
}

/// The machine an SSH session came from, and the address the session reached this one at.
#[derive(Debug, PartialEq)]
pub struct SshOrigin {
    pub client_ip: String,
    pub server_ip: String,
}

/// `terry upload [--overwrite] [--from-user <name>] <source> <folder>`: prints JSON,
/// `{"ok": true, "dest": path}` or `{"ok": false, "error": text}`.
pub fn run(args: &[&str]) -> ExitCode {
    let result = parse_args(args).and_then(|request| {
        let dir = env::current_dir()
            .map_err(|error| format!("cannot read the current folder: {error}"))?;
        let origin = env::var(SSH_CONNECTION_VAR)
            .ok()
            .and_then(|text| parse_ssh_connection(&text));
        upload(&dir, &request, origin.as_ref())
    });
    match result {
        Ok(dest) => {
            println!("{}", json!({ "ok": true, "dest": dest }));
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("{}", json!({ "ok": false, "error": error }));
            ExitCode::from(2)
        }
    }
}

/// Copies the file into the folder and returns where it landed, as a path from the repo root.
/// With an `origin`, the file is pulled from that machine. Without one, it is copied from here.
pub fn upload(dir: &Path, request: &Request, origin: Option<&SshOrigin>) -> Result<String, String> {
    let root = paths::resolve(&git::repo_root(dir)?);
    let source = normalize_dropped_path(&request.source);
    let name = file_name(&source)?;
    let folder = target_folder(&root, &request.folder)?;
    let dest = folder.join(&name);
    if dest.exists() && !request.overwrite {
        return Err(format!(
            "'{name}' is already in that folder. Use {OVERWRITE_FLAG} to replace it."
        ));
    }
    let partial = folder.join(format!("{name}{PARTIAL_SUFFIX}"));
    let transferred = match origin {
        Some(origin) => pull_from_desktop(origin, request, &source, &partial, &folder),
        None => copy_local(&source, &partial),
    };
    transferred.and_then(|()| {
        fs::rename(&partial, &dest).map_err(|error| format!("cannot save '{name}': {error}"))
    })?;
    let relative = dest.strip_prefix(&root).unwrap_or(&dest);
    Ok(relative.display().to_string())
}

fn parse_args(args: &[&str]) -> Result<Request, String> {
    let mut overwrite = false;
    let mut from_user = None;
    let mut positional = Vec::new();
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match *word {
            OVERWRITE_FLAG => overwrite = true,
            FROM_USER_FLAG => {
                let name = words
                    .next()
                    .ok_or_else(|| format!("{FROM_USER_FLAG} needs a user name"))?;
                from_user = Some((*name).to_owned());
            }
            other => positional.push(other),
        }
    }
    match positional.as_slice() {
        [source, folder] => Ok(Request {
            source: (*source).to_owned(),
            folder: (*folder).to_owned(),
            overwrite,
            from_user,
        }),
        _ => Err("give a file and a folder: terry upload <file> <folder>".to_owned()),
    }
}

/// The path as it was pasted, cleaned: a terminal pastes a dropped file as a quoted path, a path
/// with a backslash before each odd character, or a `file://` link.
pub fn normalize_dropped_path(raw: &str) -> String {
    let text = raw.trim();
    match text.strip_prefix("file://") {
        // A link may name a host before the path: file://localhost/home/me/a.txt
        Some(link) => percent_decode(link.find('/').map_or("", |start| &link[start..])),
        None => unquote(text),
    }
}

fn unquote(text: &str) -> String {
    for quote in ['\'', '"'] {
        if let Some(inner) = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner.to_owned();
        }
    }
    // A Windows path uses the backslash as its separator, not as an escape.
    if is_windows_path(text) {
        return text.to_owned();
    }
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            plain.extend(chars.next());
        } else {
            plain.push(c);
        }
    }
    plain
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = bytes
            .get(at + 1..at + 3)
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
        match (bytes[at], hex) {
            (b'%', Some(byte)) => {
                decoded.push(byte);
                at += 3;
            }
            (byte, _) => {
                decoded.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// True for a path that starts with a drive letter, like `D:\Downloads\a.png` or `D:/a.png`.
fn is_windows_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn file_name(source: &str) -> Result<String, String> {
    let name = if is_windows_path(source) {
        source.rsplit(['\\', '/']).next().map(str::to_owned)
    } else {
        Path::new(source)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
    };
    name.filter(|name| !name.is_empty() && name != "." && name != "..")
        .ok_or_else(|| format!("'{source}' does not name a file"))
}

/// The path as `scp` names a file on a Windows desktop: `D:\a\b.png` becomes `/D:/a/b.png`.
fn remote_path(source: &str) -> String {
    if is_windows_path(source) {
        format!("/{}", source.replace('\\', "/"))
    } else {
        source.to_owned()
    }
}

/// The folder as an absolute path, if it is a folder inside the repo.
fn target_folder(root: &Path, folder: &str) -> Result<PathBuf, String> {
    let resolved = paths::resolve(&root.join(folder));
    if !paths::is_within(&resolved, root) {
        return Err(format!("'{folder}' is not a folder inside the repo"));
    }
    if !resolved.is_dir() {
        return Err(format!("'{folder}' is not a folder"));
    }
    Ok(resolved)
}

fn copy_local(source: &str, partial: &Path) -> Result<(), String> {
    if !Path::new(source).is_file() {
        return Err(format!("'{source}' is not a file on this machine"));
    }
    let copied = fs::copy(source, partial)
        .map(|_| ())
        .map_err(|error| format!("cannot copy '{source}': {error}"));
    if copied.is_err() {
        let _ = fs::remove_file(partial);
    }
    copied
}

/// Reads the file over SSH, from the machine the session came from, into `partial`.
fn pull_from_desktop(
    origin: &SshOrigin,
    request: &Request,
    source: &str,
    partial: &Path,
    folder: &Path,
) -> Result<(), String> {
    let user = request
        .from_user
        .clone()
        .or_else(|| env::var("USER").ok())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("give the user name on the desktop with {FROM_USER_FLAG}"))?;
    let mut command = Command::new("scp");
    command
        .args([SCP_FORCE_SFTP_FLAG, "-o", "BatchMode=yes", "-o"])
        .arg(format!("ConnectTimeout={SSH_CONNECT_TIMEOUT_SECS}"))
        .arg("--")
        .arg(format!(
            "{user}@{}:{}",
            origin.client_ip,
            remote_path(source)
        ))
        .arg(partial);

    let failure = match process::run_captured(&mut command, None, UPLOAD_TIMEOUT) {
        Ok(done) if done.status == Some(0) => return Ok(()),
        Ok(done) => done.stderr.trim().to_owned(),
        Err(error) => error.to_string(),
    };
    let _ = fs::remove_file(partial);
    Err(format!(
        "cannot read '{source}' from the desktop ({failure}). Run this on the desktop instead: {}",
        push_command(origin, source, folder)
    ))
}

/// The `scp` command the user can run on the desktop to send the file here. The quotes are
/// double, because they work in PowerShell, `cmd` and a POSIX shell.
fn push_command(origin: &SshOrigin, source: &str, folder: &Path) -> String {
    let user = env::var("USER").unwrap_or_default();
    format!(
        "scp \"{source}\" \"{user}@{}:{}/\"",
        origin.server_ip,
        folder.display()
    )
}

fn parse_ssh_connection(text: &str) -> Option<SshOrigin> {
    let fields: Vec<&str> = text.split_whitespace().collect();
    match fields.as_slice() {
        [client_ip, _, server_ip, _] if fields.len() == SSH_CONNECTION_FIELDS => Some(SshOrigin {
            client_ip: (*client_ip).to_owned(),
            server_ip: (*server_ip).to_owned(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_remove_the_quotes_when_the_path_is_quoted() {
        assert_eq!(
            normalize_dropped_path("'/home/me/a b.txt'"),
            "/home/me/a b.txt"
        );
        assert_eq!(
            normalize_dropped_path("\"/home/me/a.txt\""),
            "/home/me/a.txt"
        );
    }

    #[test]
    fn should_remove_the_backslashes_when_the_path_is_escaped() {
        assert_eq!(
            normalize_dropped_path(r"/home/me/a\ b\(1\).txt"),
            "/home/me/a b(1).txt"
        );
    }

    #[test]
    fn should_decode_the_path_when_it_is_a_file_link() {
        assert_eq!(
            normalize_dropped_path("file:///home/me/a%20b.txt"),
            "/home/me/a b.txt"
        );
        assert_eq!(
            normalize_dropped_path("file://localhost/home/me/a.txt"),
            "/home/me/a.txt"
        );
    }

    #[test]
    fn should_keep_a_stray_percent_when_it_is_not_an_escape() {
        assert_eq!(
            normalize_dropped_path("file:///home/me/100%.txt"),
            "/home/me/100%.txt"
        );
    }

    #[test]
    fn should_trim_the_path_when_the_terminal_adds_a_trailing_space() {
        assert_eq!(
            normalize_dropped_path("/home/me/a.txt \n"),
            "/home/me/a.txt"
        );
    }

    #[test]
    fn should_read_both_addresses_when_the_connection_has_four_fields() {
        assert_eq!(
            parse_ssh_connection("10.0.0.5 51234 10.0.0.9 22"),
            Some(SshOrigin {
                client_ip: "10.0.0.5".to_owned(),
                server_ip: "10.0.0.9".to_owned()
            })
        );
    }

    #[test]
    fn should_give_no_origin_when_the_connection_is_malformed() {
        assert_eq!(parse_ssh_connection(""), None);
        assert_eq!(parse_ssh_connection("10.0.0.5 51234"), None);
    }

    #[test]
    fn should_keep_the_backslashes_when_the_path_is_a_quoted_windows_path() {
        assert_eq!(
            normalize_dropped_path(r#""D:\Downloads\My File.png""#),
            r"D:\Downloads\My File.png"
        );
    }

    #[test]
    fn should_keep_the_backslashes_when_the_path_is_an_unquoted_windows_path() {
        assert_eq!(
            normalize_dropped_path(r"D:\Downloads\a.png"),
            r"D:\Downloads\a.png"
        );
    }

    #[test]
    fn should_take_the_name_after_the_last_separator_when_the_path_is_a_windows_path() {
        assert_eq!(
            file_name(r"D:\Downloads\My File.png").as_deref(),
            Ok("My File.png")
        );
        assert_eq!(file_name("D:/Downloads/a.png").as_deref(), Ok("a.png"));
    }

    #[test]
    fn should_reject_the_name_when_it_is_a_dot_part_or_empty() {
        assert!(file_name(r"D:\Downloads\..").is_err());
        assert!(file_name(r"D:\Downloads\").is_err());
        assert!(file_name("/").is_err());
    }

    #[test]
    fn should_name_the_file_for_scp_with_a_leading_slash_when_the_path_is_a_windows_path() {
        assert_eq!(
            remote_path(r"D:\Downloads\a b.png"),
            "/D:/Downloads/a b.png"
        );
        assert_eq!(remote_path("/home/me/a.png"), "/home/me/a.png");
    }

    #[test]
    fn should_read_the_flags_when_they_come_before_or_after_the_paths() {
        let request = parse_args(&["--overwrite", "a.txt", "src", "--from-user", "me"])
            .expect("valid arguments");
        assert!(request.overwrite);
        assert_eq!(request.from_user.as_deref(), Some("me"));
        assert_eq!(
            (request.source.as_str(), request.folder.as_str()),
            ("a.txt", "src")
        );
    }

    #[test]
    fn should_reject_the_arguments_when_a_path_is_missing() {
        assert!(parse_args(&["a.txt"]).is_err());
        assert!(parse_args(&["a.txt", "src", "extra"]).is_err());
        assert!(parse_args(&["a.txt", "src", "--from-user"]).is_err());
    }
}
