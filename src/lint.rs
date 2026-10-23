//! Lints the files a patch touches, and keeps only the problems on lines the patch adds.
//!
//! Each linter uses the repo's own install and config first (`.venv/bin/ruff`, a local
//! `node_modules/.bin/eslint` with an `eslint.config.*`), else the tool on `PATH`, else
//! terry's fallback ESLint in `lint/`. A tool that is not installed is skipped and named in the
//! report; `terry install` offers to install it. Linters read the working-tree copy of each
//! file, so the caller must make sure it matches the patch (or warn the user when it does not).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

use crate::patch::{added_lines_by_file, formatter_changed_lines};
use crate::paths;
use crate::process;
use crate::text::strip_ansi;

const PROJECT_VENV_DIRS: [&str; 2] = [".venv", "venv"];
const LINT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_LINT_FILES: usize = 200;
pub const PYTHON_SUFFIXES: [&str; 2] = ["py", "pyi"];
const SHELL_SUFFIXES: [&str; 2] = ["sh", "bash"];
pub const ESLINT_SUFFIXES: [&str; 8] = ["js", "jsx", "mjs", "cjs", "ts", "tsx", "mts", "cts"];
const ESLINT_CONFIG_NAMES: [&str; 6] = [
    "eslint.config.js",
    "eslint.config.mjs",
    "eslint.config.cjs",
    "eslint.config.ts",
    "eslint.config.mts",
    "eslint.config.cts",
];
const ESLINT_ERROR_SEVERITY: u64 = 2;
const ESLINT_FALLBACK_NAME: &str = "eslint (terry's fallback config)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintProblem {
    /// Relative to the repo root.
    pub path: String,
    pub line: u64,
    pub column: u64,
    pub tool: String,
    pub code: String,
    pub message: String,
    pub blocking: bool,
}

impl LintProblem {
    pub fn describe(&self) -> String {
        let code = if self.code.is_empty() {
            String::new()
        } else {
            format!(" {}", self.code)
        };
        format!(
            "{}:{}:{} [{}{code}] {}",
            self.path, self.line, self.column, self.tool, self.message
        )
    }
}

#[derive(Debug, Default)]
pub struct LintReport {
    pub tools_run: Vec<String>,
    /// Problems on lines the patch adds.
    pub new_problems: Vec<LintProblem>,
    /// Problems in touched files, on lines the patch does not add.
    pub old_problem_count: usize,
    pub tool_errors: Vec<String>,
    /// Linters these files need that are not installed.
    pub missing_tools: Vec<String>,
}

impl LintReport {
    pub fn blocking_problems(&self) -> Vec<&LintProblem> {
        self.new_problems
            .iter()
            .filter(|problem| problem.blocking)
            .collect()
    }

    pub fn warnings(&self) -> Vec<&LintProblem> {
        self.new_problems
            .iter()
            .filter(|problem| !problem.blocking)
            .collect()
    }
}

/// What one linter gave: the labels of the tools that ran, their problems, and the tools it
/// needed but could not find.
#[derive(Default)]
struct ToolRun {
    labels: Vec<String>,
    problems: Vec<LintProblem>,
    missing: Vec<String>,
}

impl ToolRun {
    fn missing(tool: &str) -> Self {
        ToolRun {
            missing: vec![tool.to_owned()],
            ..ToolRun::default()
        }
    }
}

type Linter = fn(&Path, &[String]) -> Result<ToolRun, String>;

pub fn lint_patch(repo_root: &Path, patch: &str) -> LintReport {
    let repo_root = paths::resolve(repo_root);
    let added = added_lines_by_file(patch);
    let present: Vec<String> = added
        .keys()
        .filter(|path| repo_root.join(path).is_file())
        .take(MAX_LINT_FILES)
        .cloned()
        .collect();
    let python_files = with_suffix(&present, &PYTHON_SUFFIXES);
    let shell_files = with_suffix(&present, &SHELL_SUFFIXES);
    let eslint_files = with_suffix(&present, &ESLINT_SUFFIXES);
    let linters: [(Linter, &[String]); 4] = [
        (run_ruff, &python_files),
        (run_format_check, &python_files),
        (run_shellcheck, &shell_files),
        (run_eslint, &eslint_files),
    ];

    let mut report = LintReport::default();
    let mut problems = Vec::new();
    for (linter, files) in linters {
        if files.is_empty() {
            continue;
        }
        match linter(&repo_root, files) {
            Ok(run) => {
                report.tools_run.extend(run.labels);
                problems.extend(run.problems);
                for tool in run.missing {
                    if !report.missing_tools.contains(&tool) {
                        report.missing_tools.push(tool);
                    }
                }
            }
            Err(error) => report.tool_errors.push(error),
        }
    }
    for problem in problems {
        if added
            .get(&problem.path)
            .is_some_and(|lines| lines.contains(&problem.line))
        {
            report.new_problems.push(problem);
        } else {
            report.old_problem_count += 1;
        }
    }
    report
}

pub fn with_suffix(paths: &[String], suffixes: &[&str]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| {
            Path::new(path)
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| suffixes.contains(&ext))
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Tool discovery
// ---------------------------------------------------------------------------

fn project_python_tool(repo_root: &Path, name: &str) -> Option<PathBuf> {
    PROJECT_VENV_DIRS
        .iter()
        .map(|venv| repo_root.join(venv).join("bin").join(name))
        .find(|candidate| paths::is_executable(candidate))
}

fn python_tool(repo_root: &Path, name: &str) -> Option<PathBuf> {
    project_python_tool(repo_root, name).or_else(|| paths::find_on_path(name))
}

/// The package folder with an ESLint config and an installed ESLint for `file`, if any.
fn find_eslint_project(repo_root: &Path, file: &Path) -> Option<PathBuf> {
    let mut directory = file.parent()?.to_path_buf();
    loop {
        if ESLINT_CONFIG_NAMES
            .iter()
            .any(|name| directory.join(name).is_file())
        {
            return [directory.as_path(), repo_root]
                .iter()
                .any(|install| local_eslint(install).is_some())
                .then_some(directory);
        }
        if directory == repo_root || !directory.pop() {
            return None;
        }
    }
}

fn local_eslint(dir: &Path) -> Option<PathBuf> {
    Some(dir.join("node_modules").join(".bin").join("eslint"))
        .filter(|candidate| paths::is_executable(candidate))
}

fn fallback_eslint_dir() -> PathBuf {
    paths::checkout_dir().join("lint")
}

// ---------------------------------------------------------------------------
// Linters
// ---------------------------------------------------------------------------

fn run_tool(
    program: &Path,
    args: &[String],
    cwd: &Path,
    ok_codes: &[i32],
) -> Result<String, String> {
    let name = program
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut command = Command::new(program);
    // Some tools (ruff among them) colour their output when FORCE_COLOR is set to any value,
    // even 0, so take the force switches away rather than set them.
    command
        .args(args)
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .env_remove("FORCE_COLOR")
        .env_remove("CLICOLOR_FORCE");
    let done = process::run_captured(&mut command, None, LINT_TIMEOUT)
        .map_err(|error| format!("{name}: {error}"))?;
    let code = done.status.unwrap_or(-1);
    if ok_codes.contains(&code) {
        return Ok(strip_ansi(&done.stdout));
    }
    let output = if done.stderr.trim().is_empty() {
        &done.stdout
    } else {
        &done.stderr
    };
    let lines: Vec<&str> = output.trim().lines().collect();
    let detail = lines[lines.len().saturating_sub(5)..].join(" | ");
    Err(format!("{name} exited {code}: {detail}"))
}

fn relative_path(repo_root: &Path, path: &str) -> String {
    let absolute = repo_root.join(path);
    let resolved = paths::resolve(&absolute);
    match resolved.strip_prefix(repo_root) {
        Ok(relative) if relative.as_os_str().is_empty() => ".".to_owned(),
        Ok(relative) => relative.display().to_string(),
        Err(_) => path.to_owned(),
    }
}

fn parse_json(tool: &str, output: &str, empty: &str) -> Result<Value, String> {
    let text = if output.trim().is_empty() {
        empty
    } else {
        output
    };
    serde_json::from_str(text).map_err(|error| format!("{tool} gave unreadable output: {error}"))
}

fn json_u64(value: &Value) -> u64 {
    value.as_u64().unwrap_or(0)
}

fn json_text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

fn run_ruff(repo_root: &Path, files: &[String]) -> Result<ToolRun, String> {
    let Some(ruff) = python_tool(repo_root, "ruff") else {
        return Ok(ToolRun::missing("ruff"));
    };
    // --force-exclude keeps the project's exclude list in effect for paths passed by name.
    let mut args: Vec<String> = [
        "check",
        "--output-format",
        "json",
        "--no-fix",
        "--exit-zero",
        "--force-exclude",
    ]
    .map(str::to_owned)
    .to_vec();
    args.extend_from_slice(files);
    let results = parse_json("ruff", &run_tool(&ruff, &args, repo_root, &[0])?, "[]")?;
    let problems = results
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| LintProblem {
            path: relative_path(repo_root, item["filename"].as_str().unwrap_or_default()),
            line: json_u64(&item["location"]["row"]),
            column: json_u64(&item["location"]["column"]),
            tool: "ruff".to_owned(),
            code: item["code"].as_str().unwrap_or("syntax").to_owned(),
            message: json_text(&item["message"]),
            blocking: true,
        })
        .collect();
    Ok(ToolRun {
        labels: vec!["ruff".to_owned()],
        problems,
        missing: Vec::new(),
    })
}

/// The formatting check: the repo's own black when it has one, else `ruff format`, which
/// follows black's style.
fn run_format_check(repo_root: &Path, files: &[String]) -> Result<ToolRun, String> {
    let (tool, program, flags): (&str, PathBuf, &[&str]) =
        if let Some(black) = project_python_tool(repo_root, "black") {
            ("black", black, &["--check", "--diff", "--quiet"])
        } else if let Some(ruff) = python_tool(repo_root, "ruff") {
            (
                "ruff format",
                ruff,
                &["format", "--check", "--diff", "--force-exclude"],
            )
        } else {
            return Ok(ToolRun::missing("ruff"));
        };
    let mut args: Vec<String> = flags.iter().map(|flag| (*flag).to_owned()).collect();
    args.extend_from_slice(files);
    // Exit 1 means "would reformat"; anything else (black's 123, ruff's 2) is an error.
    let output = run_tool(&program, &args, repo_root, &[0, 1])?;
    let problems = formatter_changed_lines(&output)
        .into_iter()
        .flat_map(|(path, lines)| {
            let path = relative_path(repo_root, &path);
            lines.into_iter().map(move |line| LintProblem {
                path: path.clone(),
                line,
                column: 1,
                tool: tool.to_owned(),
                code: String::new(),
                message: format!("{tool} would reformat this line"),
                blocking: true,
            })
        })
        .collect();
    Ok(ToolRun {
        labels: vec![tool.to_owned()],
        problems,
        missing: Vec::new(),
    })
}

fn run_shellcheck(repo_root: &Path, files: &[String]) -> Result<ToolRun, String> {
    let Some(shellcheck) = paths::find_on_path("shellcheck") else {
        return Ok(ToolRun::missing("shellcheck"));
    };
    let mut args: Vec<String> = ["-f", "json1", "-S", "warning"].map(str::to_owned).to_vec();
    args.extend_from_slice(files);
    let output = parse_json(
        "shellcheck",
        &run_tool(&shellcheck, &args, repo_root, &[0, 1])?,
        "{}",
    )?;
    let problems = output["comments"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| LintProblem {
            path: relative_path(repo_root, item["file"].as_str().unwrap_or_default()),
            line: json_u64(&item["line"]),
            column: json_u64(&item["column"]),
            tool: "shellcheck".to_owned(),
            code: format!("SC{}", json_u64(&item["code"])),
            message: json_text(&item["message"]),
            blocking: true,
        })
        .collect();
    Ok(ToolRun {
        labels: vec!["shellcheck".to_owned()],
        problems,
        missing: Vec::new(),
    })
}

/// Lints each file with its project's ESLint when it has one, else with terry's fallback.
fn run_eslint(repo_root: &Path, files: &[String]) -> Result<ToolRun, String> {
    let mut project_groups: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    let mut fallback_files = Vec::new();
    for file in files {
        match find_eslint_project(repo_root, &repo_root.join(file)) {
            Some(package_dir) => project_groups
                .entry(package_dir)
                .or_default()
                .push(file.clone()),
            None => fallback_files.push(file.clone()),
        }
    }

    let mut run = ToolRun::default();
    for (package_dir, group) in &project_groups {
        let eslint = local_eslint(package_dir)
            .or_else(|| local_eslint(repo_root))
            .expect("find_eslint_project checked for an install");
        let mut args = vec!["--format".to_owned(), "json".to_owned()];
        args.extend(
            group
                .iter()
                .map(|file| repo_root.join(file).display().to_string()),
        );
        run.problems.extend(parse_eslint_json(
            repo_root,
            &run_tool(&eslint, &args, package_dir, &[0, 1])?,
        )?);
        let config_dir = relative_path(repo_root, &package_dir.display().to_string());
        run.labels.push(format!("eslint ({config_dir} config)"));
    }
    if !fallback_files.is_empty() {
        let fallback_dir = fallback_eslint_dir();
        let Some(eslint) = local_eslint(&fallback_dir) else {
            run.missing.push(ESLINT_FALLBACK_NAME.to_owned());
            return Ok(run);
        };
        let config = fallback_dir.join("eslint.config.mjs").display().to_string();
        let mut args = vec![
            "--config".to_owned(),
            config,
            "--format".to_owned(),
            "json".to_owned(),
        ];
        args.extend(fallback_files);
        run.problems.extend(parse_eslint_json(
            repo_root,
            &run_tool(&eslint, &args, repo_root, &[0, 1])?,
        )?);
        run.labels.push("eslint (fallback config)".to_owned());
    }
    Ok(run)
}

pub fn parse_eslint_json(repo_root: &Path, output: &str) -> Result<Vec<LintProblem>, String> {
    let results = parse_json("eslint", output, "[]")?;
    let mut problems = Vec::new();
    for result in results.as_array().into_iter().flatten() {
        let path = relative_path(repo_root, result["filePath"].as_str().unwrap_or_default());
        for message in result["messages"].as_array().into_iter().flatten() {
            // Messages with no line (such as "file ignored") are notes about the file.
            let line = json_u64(&message["line"]);
            if line == 0 {
                continue;
            }
            problems.push(LintProblem {
                path: path.clone(),
                line,
                column: message["column"].as_u64().unwrap_or(1),
                tool: "eslint".to_owned(),
                code: message["ruleId"].as_str().unwrap_or("parse").to_owned(),
                message: json_text(&message["message"]),
                blocking: message["severity"].as_u64() == Some(ESLINT_ERROR_SEVERITY),
            });
        }
    }
    Ok(problems)
}
