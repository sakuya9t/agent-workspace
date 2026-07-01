use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, bail, Result};
use serde::Serialize;

/// A changed file in the working tree.
#[derive(Debug, Clone, Serialize)]
pub struct ChangedFile {
    pub path: String,
    /// Single-letter change kind: A, M, D, R, C, U, or `?` (untracked).
    pub status: String,
    pub staged: bool,
    pub untracked: bool,
    /// For renames, the previous path.
    pub orig_path: Option<String>,
}

/// Provider-neutral repository status for the right-side panel.
#[derive(Debug, Clone, Serialize)]
pub struct ScmStatus {
    pub is_repo: bool,
    pub provider: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub detached: bool,
    pub changed_files: Vec<ChangedFile>,
}

/// A commit in the history graph (simplified single-lane model for MVP).
#[derive(Debug, Clone, Serialize)]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub subject: String,
    pub author: String,
    pub timestamp: i64,
    pub parents: Vec<String>,
}

/// Source-control plugin boundary. The Git provider is the MVP built-in;
/// other VCS providers implement the same trait behind the same panel.
pub trait SourceControl: Send + Sync {
    fn id(&self) -> &'static str;
    fn detect(&self, cwd: &Path) -> bool;
    fn status(&self, cwd: &Path) -> Result<ScmStatus>;
    /// Unified diff for one path. `untracked` files diff against /dev/null.
    fn diff(&self, cwd: &Path, path: &str, untracked: bool) -> Result<String>;
    fn log(&self, cwd: &Path, limit: usize) -> Result<Vec<Commit>>;
}

pub struct GitSourceControl;

impl SourceControl for GitSourceControl {
    fn id(&self) -> &'static str {
        "git"
    }

    fn detect(&self, cwd: &Path) -> bool {
        matches!(
            git(cwd, &["rev-parse", "--is-inside-work-tree"]),
            Ok(out) if out.trim() == "true"
        )
    }

    fn status(&self, cwd: &Path) -> Result<ScmStatus> {
        if !self.detect(cwd) {
            return Ok(ScmStatus {
                is_repo: false,
                provider: "git".into(),
                branch: None,
                head: None,
                detached: false,
                changed_files: vec![],
            });
        }

        let branch_raw = git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
            .unwrap_or_default()
            .trim()
            .to_string();
        let detached = branch_raw == "HEAD" || branch_raw.is_empty();
        let branch = if detached { None } else { Some(branch_raw) };
        let head = git(cwd, &["rev-parse", "--short", "HEAD"])
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let porcelain = git(cwd, &["status", "--porcelain", "--untracked-files=all"])?;
        let changed_files = parse_porcelain(&porcelain);

        Ok(ScmStatus {
            is_repo: true,
            provider: "git".into(),
            branch,
            head,
            detached,
            changed_files,
        })
    }

    fn diff(&self, cwd: &Path, path: &str, untracked: bool) -> Result<String> {
        guard_path(path)?;
        if untracked {
            // /dev/null diff shows the whole file as added; git exits 1 here.
            let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
            git_allow_diff(cwd, &["diff", "--no-index", "--", null, path])
        } else {
            // Everything changed vs HEAD (staged + unstaged).
            git_allow_diff(cwd, &["diff", "HEAD", "--", path])
        }
    }

    fn log(&self, cwd: &Path, limit: usize) -> Result<Vec<Commit>> {
        if !self.detect(cwd) {
            return Ok(vec![]);
        }
        // Unit-separator-delimited fields, one commit per line.
        let fmt = "%H%x1f%h%x1f%s%x1f%an%x1f%ct%x1f%P";
        let out = git(
            cwd,
            &[
                "log",
                &format!("--pretty=format:{fmt}"),
                "-n",
                &limit.to_string(),
            ],
        )?;
        let mut commits = Vec::new();
        for line in out.lines() {
            let f: Vec<&str> = line.split('\u{1f}').collect();
            if f.len() < 6 {
                continue;
            }
            commits.push(Commit {
                hash: f[0].to_string(),
                short: f[1].to_string(),
                subject: f[2].to_string(),
                author: f[3].to_string(),
                timestamp: f[4].parse().unwrap_or(0),
                parents: f[5].split_whitespace().map(|s| s.to_string()).collect(),
            });
        }
        Ok(commits)
    }
}

/// Parse `git status --porcelain` (v1) into changed-file entries.
fn parse_porcelain(text: &str) -> Vec<ChangedFile> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.len() < 3 {
            continue;
        }
        let x = &line[0..1];
        let y = &line[1..2];
        let rest = &line[3..];
        let untracked = x == "?" && y == "?";
        let staged = !untracked && x != " " && x != "?";

        let (path, orig_path) = if let Some(idx) = rest.find(" -> ") {
            let (old, new) = rest.split_at(idx);
            (new[4..].to_string(), Some(old.to_string()))
        } else {
            (rest.to_string(), None)
        };

        // Prefer the worktree status letter, then the index status letter.
        let status = if untracked {
            "?".to_string()
        } else if y != " " {
            y.to_uppercase()
        } else {
            x.to_uppercase()
        };

        out.push(ChangedFile {
            path,
            status,
            staged,
            untracked,
            orig_path,
        });
    }
    out
}

fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| anyhow!("failed to run git: {e}"))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Like `git`, but tolerates exit code 1 (used by diff, which returns 1 when
/// there are differences — notably `--no-index`).
fn git_allow_diff(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| anyhow!("failed to run git: {e}"))?;
    match output.status.code() {
        Some(0) | Some(1) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        _ => bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

/// Reject path arguments that could escape the repository.
fn guard_path(path: &str) -> Result<()> {
    if path.is_empty() {
        bail!("empty path");
    }
    if Path::new(path).is_absolute() {
        bail!("absolute paths are not allowed");
    }
    if path.split(['/', '\\']).any(|c| c == "..") {
        bail!("path traversal is not allowed");
    }
    Ok(())
}
