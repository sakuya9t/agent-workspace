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

/// Per-file line churn for one commit (the `git show --numstat` view).
#[derive(Debug, Clone, Serialize)]
pub struct CommitFileStat {
    pub path: String,
    /// For renames, the previous path; else `None`.
    pub orig_path: Option<String>,
    /// Added/removed line counts; `None` for binary files (`-` in numstat).
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
}

/// Full detail for a single commit: metadata, body, and per-file churn.
/// This is what the history panel shows when a commit is clicked.
#[derive(Debug, Clone, Serialize)]
pub struct CommitDetail {
    pub hash: String,
    pub short: String,
    pub subject: String,
    pub body: String,
    pub author: String,
    pub email: String,
    pub timestamp: i64,
    pub parents: Vec<String>,
    pub files: Vec<CommitFileStat>,
    /// Totals across all non-binary files.
    pub additions: u64,
    pub deletions: u64,
}

/// Source-control plugin boundary. The Git provider is the MVP built-in;
/// other VCS providers implement the same trait behind the same panel.
pub trait SourceControl: Send + Sync {
    fn detect(&self, cwd: &Path) -> bool;
    fn status(&self, cwd: &Path) -> Result<ScmStatus>;
    /// Unified diff for one path. When `commit` is set, show that path's diff
    /// as introduced by the commit; otherwise diff the working tree (with
    /// `untracked` files diffed against /dev/null).
    fn diff(&self, cwd: &Path, path: &str, untracked: bool, commit: Option<&str>)
        -> Result<String>;
    fn log(&self, cwd: &Path, limit: usize) -> Result<Vec<Commit>>;
    /// Full detail (metadata + per-file churn) for a single commit.
    fn show(&self, cwd: &Path, hash: &str) -> Result<CommitDetail>;
}

pub struct GitSourceControl;

impl SourceControl for GitSourceControl {
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

    fn diff(
        &self,
        cwd: &Path,
        path: &str,
        untracked: bool,
        commit: Option<&str>,
    ) -> Result<String> {
        guard_path(path)?;
        if let Some(hash) = commit {
            // One file's change as introduced by a specific commit. `--format=`
            // drops the commit message so only the diff body is returned.
            guard_ref(hash)?;
            return git_allow_diff(
                cwd,
                &["show", "--no-color", "--format=", hash, "--", path],
            );
        }
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

    fn show(&self, cwd: &Path, hash: &str) -> Result<CommitDetail> {
        guard_ref(hash)?;
        // Metadata fields are unit-separated and terminated by a record
        // separator (\x1e); the `--numstat` block follows on the next lines.
        // `%b` (body) may contain newlines, so the \x1e is what marks its end.
        let fmt = "%H%x1f%h%x1f%s%x1f%an%x1f%ae%x1f%ct%x1f%P%x1f%b%x1e";
        let out = git(
            cwd,
            &[
                "show",
                "--no-color",
                "--numstat",
                &format!("--format=format:{fmt}"),
                hash,
            ],
        )?;
        parse_show(&out).ok_or_else(|| anyhow!("could not parse commit {hash}"))
    }
}

/// Parse the `git show --numstat --format=…\x1e` output into a `CommitDetail`.
fn parse_show(out: &str) -> Option<CommitDetail> {
    let (header, rest) = out.split_once('\u{1e}')?;
    let f: Vec<&str> = header.split('\u{1f}').collect();
    if f.len() < 8 {
        return None;
    }

    let mut files = Vec::new();
    let (mut additions, mut deletions) = (0u64, 0u64);
    // Skip the blank line(s) git may insert before the numstat block; only
    // lines shaped `<add>\t<del>\t<path>` are file entries.
    for line in rest.lines() {
        let mut cols = line.splitn(3, '\t');
        let (Some(a), Some(d), Some(p)) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        if p.is_empty() {
            continue;
        }
        let add = a.parse::<u64>().ok();
        let del = d.parse::<u64>().ok();
        additions += add.unwrap_or(0);
        deletions += del.unwrap_or(0);
        let (path, orig_path) = split_rename(p);
        files.push(CommitFileStat {
            path,
            orig_path,
            additions: add,
            deletions: del,
        });
    }

    Some(CommitDetail {
        hash: f[0].to_string(),
        short: f[1].to_string(),
        subject: f[2].to_string(),
        author: f[3].to_string(),
        email: f[4].to_string(),
        timestamp: f[5].parse().unwrap_or(0),
        parents: f[6].split_whitespace().map(|s| s.to_string()).collect(),
        body: f[7].trim_end().to_string(),
        files,
        additions,
        deletions,
    })
}

/// Reconstruct the (new, old) paths from a numstat rename entry. Handles both
/// `old => new` and the braced `pre/{old => new}/post` forms; returns
/// `(path, None)` for a plain (non-rename) entry.
fn split_rename(p: &str) -> (String, Option<String>) {
    let Some(arrow) = p.find(" => ") else {
        return (p.to_string(), None);
    };
    if let (Some(lb), Some(rb)) = (p.find('{'), p.find('}')) {
        if lb < arrow && arrow < rb {
            let prefix = &p[..lb];
            let suffix = &p[rb + 1..];
            let old = &p[lb + 1..arrow];
            let new = &p[arrow + 4..rb];
            return (
                format!("{prefix}{new}{suffix}"),
                Some(format!("{prefix}{old}{suffix}")),
            );
        }
    }
    let (old, new) = p.split_at(arrow);
    (new[4..].to_string(), Some(old.to_string()))
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

/// Reject anything that isn't a bare commit hash. The value reaches git as a
/// positional argument, so restricting it to hex digits blocks both option
/// injection (a leading `-`) and revision expressions.
fn guard_ref(hash: &str) -> Result<()> {
    if !(4..=64).contains(&hash.len()) || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("invalid commit hash");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_change_kinds() {
        let text = " M src/a.rs\nM  src/b.rs\nA  src/c.rs\n D src/d.rs\nR  old.rs -> new.rs\n?? src/e.rs\n";
        let files = parse_porcelain(text);
        assert_eq!(files.len(), 6);

        // unstaged modification
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[0].status, "M");
        assert!(!files[0].staged);
        assert!(!files[0].untracked);

        // staged modification
        assert_eq!(files[1].status, "M");
        assert!(files[1].staged);

        // staged add
        assert_eq!(files[2].status, "A");
        assert!(files[2].staged);

        // unstaged delete
        assert_eq!(files[3].status, "D");
        assert!(!files[3].staged);

        // rename keeps both paths
        assert_eq!(files[4].status, "R");
        assert_eq!(files[4].path, "new.rs");
        assert_eq!(files[4].orig_path.as_deref(), Some("old.rs"));
        assert!(files[4].staged);

        // untracked
        assert_eq!(files[5].status, "?");
        assert!(files[5].untracked);
        assert!(!files[5].staged);
    }

    #[test]
    fn guard_rejects_traversal_and_absolute() {
        assert!(guard_path("src/main.rs").is_ok());
        assert!(guard_path("").is_err());
        assert!(guard_path("../secret").is_err());
        assert!(guard_path("a/../../b").is_err());
        assert!(guard_path("/etc/passwd").is_err());
    }

    #[test]
    fn guard_ref_only_accepts_hashes() {
        assert!(guard_ref("1176c78").is_ok());
        assert!(guard_ref("1176c7817edc4f99eea0d10da6200322b4acad66").is_ok());
        assert!(guard_ref("").is_err());
        assert!(guard_ref("abc").is_err()); // too short
        assert!(guard_ref("--format=%s").is_err());
        assert!(guard_ref("HEAD~1").is_err());
        assert!(guard_ref("main").is_err());
    }

    #[test]
    fn split_rename_handles_all_forms() {
        assert_eq!(split_rename("src/a.rs"), ("src/a.rs".into(), None));
        assert_eq!(
            split_rename("old.rs => new.rs"),
            ("new.rs".into(), Some("old.rs".into()))
        );
        assert_eq!(
            split_rename("src/{old.rs => new.rs}"),
            ("src/new.rs".into(), Some("src/old.rs".into()))
        );
        assert_eq!(
            split_rename("a/{b => c}/d.rs"),
            ("a/c/d.rs".into(), Some("a/b/d.rs".into()))
        );
    }

    #[test]
    fn parse_show_reads_metadata_and_numstat() {
        // \x1f = unit sep, \x1e = record sep — the format `show` emits.
        let out = "H1\u{1f}h1\u{1f}Subject line\u{1f}Ann\u{1f}ann@x.io\u{1f}1700000000\u{1f}P1 P2\u{1f}Body line one\nBody line two\u{1e}\n\
            10\t2\tsrc/a.rs\n\
            -\t-\tassets/logo.png\n\
            1\t1\tsrc/{old.rs => new.rs}\n";
        let d = parse_show(out).expect("parses");
        assert_eq!(d.hash, "H1");
        assert_eq!(d.short, "h1");
        assert_eq!(d.subject, "Subject line");
        assert_eq!(d.email, "ann@x.io");
        assert_eq!(d.timestamp, 1_700_000_000);
        assert_eq!(d.parents, vec!["P1", "P2"]);
        assert_eq!(d.body, "Body line one\nBody line two");
        assert_eq!(d.files.len(), 3);
        assert_eq!(d.files[0].additions, Some(10));
        assert_eq!(d.files[1].additions, None); // binary
        assert_eq!(d.files[2].path, "src/new.rs");
        assert_eq!(d.files[2].orig_path.as_deref(), Some("src/old.rs"));
        // Totals ignore the binary file.
        assert_eq!(d.additions, 11);
        assert_eq!(d.deletions, 3);
    }
}
