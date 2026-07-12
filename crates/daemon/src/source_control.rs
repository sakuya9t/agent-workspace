use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Result};
use serde::Serialize;

use crate::workspace;

/// Largest file we'll read into memory to serve as an inline preview. A blob
/// above this is refused rather than buffered, so a single click on a giant
/// tracked file can't balloon daemon memory.
pub const MAX_PREVIEW_BYTES: u64 = 10 * 1024 * 1024;

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

#[derive(Debug, thiserror::Error)]
#[error("merge from `{source_branch}` into `{target}` had conflicts and was aborted: {output}")]
pub struct MergeConflict {
    pub source_branch: String,
    pub target: String,
    pub output: String,
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
    /// Raw bytes of one file for inline preview (images in the diff panel).
    /// The working-tree version when `commit` is `None`; the version at that
    /// commit otherwise. `Ok(None)` means the file has no content at that
    /// version (added later, or deleted) — the caller shows the other side.
    fn file_bytes(&self, cwd: &Path, path: &str, commit: Option<&str>) -> Result<Option<Vec<u8>>>;
    /// Resolve a controlled revision expression (`HEAD`, `<hash>^`) to a bare
    /// commit hash, or `None` when it does not resolve (a root commit's parent,
    /// or `HEAD` in an empty repo). Never called with raw client input.
    fn resolve_commit(&self, cwd: &Path, rev: &str) -> Result<Option<String>>;
    fn log(&self, cwd: &Path, limit: usize) -> Result<Vec<Commit>>;
    /// Full detail (metadata + per-file churn) for a single commit.
    fn show(&self, cwd: &Path, hash: &str) -> Result<CommitDetail>;
    /// Local branch names and the current branch (`None` when detached).
    fn branches(&self, cwd: &Path) -> Result<(Vec<String>, Option<String>)>;
    /// Fast-forward-only pull of the current branch from its upstream. Never
    /// creates a merge commit or leaves the worktree in a conflicted state; if
    /// the branch has diverged it fails cleanly (the user can rebase instead).
    /// Returns git's combined stdout+stderr for display.
    fn pull(&self, cwd: &Path) -> Result<String>;
    /// Rebase the current branch onto `onto` (a local branch). On any failure
    /// (conflicts, dirty tree) the rebase is aborted so the worktree is left in
    /// a clean, usable state rather than half-rebased.
    fn rebase(&self, cwd: &Path, onto: &str) -> Result<String>;
    /// Merge the current branch into `target` (a local branch). Failed merges
    /// are aborted so conflict files are not left in either worktree.
    fn merge_to_branch(&self, cwd: &Path, target: &str) -> Result<String>;
    /// Push the current branch to `origin`, creating the remote branch and
    /// recording it as the upstream when it doesn't exist yet. Never forces:
    /// a diverged remote is rejected (non-fast-forward) and that error is
    /// surfaced. Returns git's combined stdout+stderr for display; auth and
    /// configuration failures come straight from git.
    fn push(&self, cwd: &Path) -> Result<String>;
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

        let branch = current_branch(cwd);
        let detached = branch.is_none();
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
            return git_allow_diff(cwd, &["show", "--no-color", "--format=", hash, "--", path]);
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

    fn file_bytes(&self, cwd: &Path, path: &str, commit: Option<&str>) -> Result<Option<Vec<u8>>> {
        guard_path(path)?;
        if let Some(hash) = commit {
            // The blob as it existed at that commit. The `./` prefix keeps the
            // path resolved relative to `cwd`, matching the working-tree branch
            // below rather than defaulting to the repo root.
            guard_ref(hash)?;
            let output = Command::new("git")
                .args(["show", &format!("{hash}:./{path}")])
                .current_dir(cwd)
                .output()
                .map_err(|e| anyhow!("failed to run git: {e}"))?;
            // git ran but the path isn't present at that commit (added later, or
            // this is the parent side of an addition): an absent version, not an
            // error — the caller renders only the other side of the diff.
            if !output.status.success() {
                return Ok(None);
            }
            if output.stdout.len() as u64 > MAX_PREVIEW_BYTES {
                bail!("file too large to preview");
            }
            return Ok(Some(output.stdout));
        }
        // Working-tree version, straight from disk. `guard_path` already blocked
        // `..` and absolute paths; canonicalizing the result and confining it to
        // `cwd` additionally stops a symlink committed in the repo from serving a
        // file outside the session's working directory.
        let abs = cwd.join(path);
        let real = match std::fs::canonicalize(&abs) {
            Ok(p) => p,
            // A deleted file has no working-tree version — absent, not an error.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(anyhow!("resolve {path}: {e}")),
        };
        let root = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
        if !real.starts_with(&root) {
            bail!("path escapes the working directory");
        }
        let meta = std::fs::metadata(&real).map_err(|e| anyhow!("stat {path}: {e}"))?;
        if meta.len() > MAX_PREVIEW_BYTES {
            bail!("file too large to preview");
        }
        Ok(Some(
            std::fs::read(&real).map_err(|e| anyhow!("read {path}: {e}"))?,
        ))
    }

    fn resolve_commit(&self, cwd: &Path, rev: &str) -> Result<Option<String>> {
        // `--verify --quiet` exits non-zero with empty output when the rev does
        // not resolve. Peeling with `^{commit}` guarantees a commit hash out.
        let output = Command::new("git")
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{rev}^{{commit}}"),
            ])
            .current_dir(cwd)
            .output()
            .map_err(|e| anyhow!("failed to run git: {e}"))?;
        if !output.status.success() {
            return Ok(None);
        }
        let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(if hash.is_empty() { None } else { Some(hash) })
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

    fn branches(&self, cwd: &Path) -> Result<(Vec<String>, Option<String>)> {
        if !self.detect(cwd) {
            return Ok((vec![], None));
        }
        let out = git(cwd, &["branch", "--format=%(refname:short)"])?;
        let branches: Vec<String> = out
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        Ok((branches, current_branch(cwd)))
    }

    fn pull(&self, cwd: &Path) -> Result<String> {
        if !self.detect(cwd) {
            bail!("not a git repository");
        }
        // Only a branch with somewhere to pull *from* can be pulled. Prefer the
        // configured upstream; if none is set (common for local-only session
        // branches) fall back to origin/<same-name> when it has been fetched
        // before. Otherwise there is genuinely nothing to pull, so say so
        // plainly instead of surfacing git's multi-line tracking-info error.
        let has_upstream = git(
            cwd,
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        )
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

        let out = if has_upstream {
            git_output(cwd, &["pull", "--ff-only"])?
        } else {
            let branch = git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?
                .trim()
                .to_string();
            if branch.is_empty() || branch == "HEAD" {
                bail!("cannot pull: HEAD is detached");
            }
            if remote_tracking_exists(cwd, "origin", &branch) {
                git_output(cwd, &["pull", "--ff-only", "origin", &branch])?
            } else {
                bail!(
                    "current branch '{branch}' is not tracking a remote branch, so there is \
                     nothing to pull. Push it first (git push -u), or use Rebase to bring in \
                     another branch's commits."
                );
            }
        };

        if out.status.success() {
            Ok(combined_output(&out))
        } else {
            bail!("git pull failed: {}", combined_output(&out).trim());
        }
    }

    fn rebase(&self, cwd: &Path, onto: &str) -> Result<String> {
        if !self.detect(cwd) {
            bail!("not a git repository");
        }
        guard_branch(onto)?;
        // Only rebase onto a branch that actually exists here. Beyond catching
        // typos, the exact-match membership check makes argument/option
        // injection impossible even though `onto` reaches git positionally.
        let (branches, head) = self.branches(cwd)?;
        if !branches.iter().any(|b| b == onto) {
            bail!("unknown branch: {onto}");
        }
        if head.as_deref() == Some(onto) {
            bail!("cannot rebase a branch onto itself");
        }
        let out = git_output(cwd, &["rebase", onto])?;
        if out.status.success() {
            return Ok(combined_output(&out));
        }
        // A failed rebase (conflicts, or a dirty tree it refused to touch) can
        // leave a rebase in progress; abort so the session's worktree returns
        // to a clean state instead of a confusing half-rebased one.
        let _ = git_output(cwd, &["rebase", "--abort"]);
        bail!(
            "git rebase onto {onto} failed (rebase aborted): {}",
            combined_output(&out).trim()
        );
    }

    fn merge_to_branch(&self, cwd: &Path, target: &str) -> Result<String> {
        if !self.detect(cwd) {
            bail!("not a git repository");
        }
        guard_branch(target)?;
        let (branches, head) = self.branches(cwd)?;
        if !branches.iter().any(|b| b == target) {
            bail!("unknown branch: {target}");
        }
        let source = head.ok_or_else(|| anyhow!("cannot merge to branch: HEAD is detached"))?;
        if source == target {
            bail!("cannot merge a branch into itself");
        }
        ensure_clean_worktree(cwd, &format!("source branch `{source}`"))?;

        let mut temp_path = None;
        let merge_dir = match workspace::worktree_for_branch(cwd, target)? {
            Some((path, _)) => PathBuf::from(path),
            None => {
                let path = temp_merge_worktree_path(target);
                let path_str = path_arg(&path)?;
                git(cwd, &["worktree", "add", path_str, target]).map_err(|e| {
                    anyhow!("could not create temporary worktree for `{target}`: {e}")
                })?;
                temp_path = Some(path.clone());
                path
            }
        };

        if let Err(e) = ensure_clean_worktree(&merge_dir, &format!("target branch `{target}`")) {
            cleanup_temp_worktree(cwd, temp_path.as_deref());
            return Err(e);
        }

        let out = match git_output(&merge_dir, &["merge", "--no-edit", &source]) {
            Ok(out) => out,
            Err(e) => {
                cleanup_temp_worktree(cwd, temp_path.as_deref());
                return Err(e);
            }
        };
        let output = combined_output(&out);
        if out.status.success() {
            let message = if output.trim().is_empty() {
                format!("Merged {source} into {target}.")
            } else {
                format!("Merged {source} into {target}.\n{output}")
            };
            if let Err(e) = remove_temp_worktree(cwd, temp_path.as_deref()) {
                return Ok(format!(
                    "{message}\n\nWarning: could not remove temporary worktree: {e:#}"
                ));
            }
            return Ok(message);
        }

        let had_conflicts = has_unmerged_paths(&merge_dir) || output.contains("CONFLICT");
        let _ = git_output(&merge_dir, &["merge", "--abort"]);
        cleanup_temp_worktree(cwd, temp_path.as_deref());
        if had_conflicts {
            return Err(MergeConflict {
                source_branch: source,
                target: target.to_string(),
                output,
            }
            .into());
        }
        bail!(
            "git merge {source} into {target} failed (merge aborted): {}",
            output.trim()
        );
    }

    fn push(&self, cwd: &Path) -> Result<String> {
        if !self.detect(cwd) {
            bail!("not a git repository");
        }
        let branch = git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?
            .trim()
            .to_string();
        if branch.is_empty() || branch == "HEAD" {
            bail!("cannot push: HEAD is detached");
        }
        // The branch name comes from git itself, but it reaches `git push`
        // positionally after `origin`; guarding is cheap defence in depth.
        guard_branch(&branch)?;

        // Pre-empt git's cryptic "'origin' does not appear to be a git
        // repository" with a message that says what to do about it.
        if !remote_exists(cwd, "origin") {
            bail!(
                "no 'origin' remote is configured, so there is nowhere to push. \
                 Add one with `git remote add origin <url>` first."
            );
        }

        // `--set-upstream` creates the remote branch when it doesn't exist yet
        // and records origin/<branch> as the upstream, so a later Pull has
        // somewhere to pull from. An existing remote branch is fast-forwarded;
        // a diverged one is rejected (non-fast-forward) and surfaced rather
        // than forced. `GIT_TERMINAL_PROMPT=0` makes a missing/expired
        // credential fail fast with git's own message instead of blocking the
        // daemon on an interactive username/password prompt that never comes.
        let out = Command::new("git")
            .args(["push", "--set-upstream", "origin", &branch])
            .env("GIT_TERMINAL_PROMPT", "0")
            .current_dir(cwd)
            .output()
            .map_err(|e| anyhow!("failed to run git: {e}"))?;
        if out.status.success() {
            Ok(combined_output(&out))
        } else {
            bail!("git push failed: {}", combined_output(&out).trim());
        }
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

/// The checked-out branch name, or `None` when detached (or unreadable —
/// `git` failing degrades to the detached presentation, not an error).
pub(crate) fn current_branch(cwd: &Path) -> Option<String> {
    let raw = git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_string();
    if raw == "HEAD" || raw.is_empty() {
        None
    } else {
        Some(raw)
    }
}

pub(crate) fn git(cwd: &Path, args: &[&str]) -> Result<String> {
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

/// Run git and return the raw `Output` (both streams, exit status) without
/// treating a non-zero exit as an error. Used by pull/rebase, which want git's
/// message on both success and failure and decide how to react themselves.
fn git_output(cwd: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| anyhow!("failed to run git: {e}"))
}

/// Whether `refs/remotes/<remote>/<branch>` exists locally (i.e. the branch has
/// been fetched before). Checked without touching the network so pull can pick
/// a fallback source for a branch whose tracking config was never set.
fn remote_tracking_exists(cwd: &Path, remote: &str, branch: &str) -> bool {
    git(
        cwd,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/remotes/{remote}/{branch}"),
        ],
    )
    .map(|s| !s.trim().is_empty())
    .unwrap_or(false)
}

/// Whether a remote by this name is configured. Checked so push can give a
/// friendly "no origin" message instead of git's raw "does not appear to be a
/// git repository" when the remote is missing.
fn remote_exists(cwd: &Path, remote: &str) -> bool {
    git(cwd, &["remote"])
        .map(|out| out.lines().any(|l| l.trim() == remote))
        .unwrap_or(false)
}

fn ensure_clean_worktree(cwd: &Path, label: &str) -> Result<()> {
    let out = git(cwd, &["status", "--porcelain", "--untracked-files=all"])?;
    if !out.trim().is_empty() {
        bail!("{label} has uncommitted changes; commit or stash them before merging");
    }
    Ok(())
}

fn has_unmerged_paths(cwd: &Path) -> bool {
    git(cwd, &["diff", "--name-only", "--diff-filter=U"])
        .map(|out| !out.trim().is_empty())
        .unwrap_or(false)
}

fn temp_merge_worktree_path(target: &str) -> PathBuf {
    let safe_target: String = target
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "asm-merge-{safe_target}-{}-{now}",
        std::process::id()
    ))
}

fn path_arg(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("non-UTF8 worktree path"))
}

fn remove_temp_worktree(root: &Path, path: Option<&Path>) -> Result<()> {
    if let Some(path) = path {
        git(root, &["worktree", "remove", "--force", path_arg(path)?])?;
    }
    Ok(())
}

fn cleanup_temp_worktree(root: &Path, path: Option<&Path>) {
    let _ = remove_temp_worktree(root, path);
}

/// Merge git's stdout and stderr into one human-readable blob. Porcelain like
/// "Already up to date." lands on stdout; progress ("From …", conflict notes)
/// on stderr — the panel wants both.
fn combined_output(out: &std::process::Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(&err);
    }
    s.trim_end().to_string()
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
/// Whether a string is a bare commit hash (4–64 hex chars). Shared by the ref
/// guard and the preview endpoint, which validates a client-supplied commit
/// before building a `<hash>^` parent expression from it.
pub(crate) fn is_commit_hash(hash: &str) -> bool {
    (4..=64).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

fn guard_ref(hash: &str) -> Result<()> {
    if !is_commit_hash(hash) {
        bail!("invalid commit hash");
    }
    Ok(())
}

/// Reject anything that could be mistaken for an option or shell/ref trickery
/// before a branch name reaches git positionally. The caller additionally
/// checks membership against the repo's real branch list, so this is defence
/// in depth rather than the sole guard.
fn guard_branch(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("empty branch name");
    }
    if name.starts_with('-') {
        bail!("invalid branch name");
    }
    if name
        .bytes()
        .any(|b| b == 0 || b == b'\n' || b == b'\r' || b == b' ')
    {
        bail!("invalid branch name");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_repo(name: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "asm-source-control-{name}-{}-{now}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        git_test(&dir, &["init", "-q"]);
        git_test(&dir, &["config", "user.name", "ASM Test"]);
        git_test(&dir, &["config", "user.email", "asm-test@example.com"]);
        git_test(&dir, &["checkout", "-b", "main"]);
        dir
    }

    fn git_test(cwd: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn write_file(cwd: &Path, path: &str, contents: &str) {
        fs::write(cwd.join(path), contents).unwrap();
    }

    fn commit_all(cwd: &Path, message: &str) {
        git_test(cwd, &["add", "."]);
        git_test(cwd, &["commit", "-q", "-m", message]);
    }

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
    fn guard_branch_rejects_options_and_whitespace() {
        assert!(guard_branch("main").is_ok());
        assert!(guard_branch("release/next").is_ok());
        assert!(guard_branch("feature/foo-bar_1").is_ok());
        assert!(guard_branch("").is_err());
        assert!(guard_branch("--onto").is_err());
        assert!(guard_branch("-x").is_err());
        assert!(guard_branch("has space").is_err());
        assert!(guard_branch("has\nnewline").is_err());
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

    #[test]
    fn file_bytes_reads_working_tree_and_committed_versions() {
        // Two distinct blobs standing in for an image's before/after content.
        // (`file_bytes` doesn't sniff — the header just mirrors a real PNG.)
        const V1: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 1, 1];
        const V2: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 2, 2, 2, 2];
        let repo = test_repo("file-bytes");

        fs::write(repo.join("img.png"), V1).unwrap();
        commit_all(&repo, "add image v1");
        let c1 = git_test(&repo, &["rev-parse", "HEAD"]).trim().to_string();
        fs::write(repo.join("img.png"), V2).unwrap();
        commit_all(&repo, "image v2");
        let c2 = git_test(&repo, &["rev-parse", "HEAD"]).trim().to_string();

        let bytes = |commit: Option<&str>| GitSourceControl.file_bytes(&repo, "img.png", commit);

        // Working-tree and HEAD both read v2; the parent commit reads v1 — the
        // "before" and "after" sides of the diff differ, as they must.
        assert_eq!(bytes(None).unwrap().as_deref(), Some(V2));
        assert_eq!(bytes(Some(&c2)).unwrap().as_deref(), Some(V2));
        assert_eq!(bytes(Some(&c1)).unwrap().as_deref(), Some(V1));

        // Parent resolution powers the "before" side of a commit diff.
        assert_eq!(GitSourceControl.resolve_commit(&repo, "HEAD").unwrap(), Some(c2));
        assert_eq!(GitSourceControl.resolve_commit(&repo, &format!("{c1}^")).unwrap(), None);

        // A path absent at a commit is `Ok(None)` (not an error) so the caller
        // can drop that side; traversal is still rejected outright.
        assert_eq!(GitSourceControl.file_bytes(&repo, "nope.png", Some(&c1)).unwrap(), None);
        assert!(GitSourceControl.file_bytes(&repo, "../secret", None).is_err());
        let _ = fs::remove_dir_all(repo);
    }

    #[cfg(unix)]
    #[test]
    fn file_bytes_rejects_symlink_escaping_the_working_dir() {
        let repo = test_repo("file-bytes-symlink");
        // A file outside the repo that a repo symlink points at.
        let outside = std::env::temp_dir().join(format!("asm-outside-{}.png", std::process::id()));
        fs::write(&outside, [0x89, b'P', b'N', b'G', 1, 2, 3]).unwrap();
        std::os::unix::fs::symlink(&outside, repo.join("evil.png")).unwrap();

        // The symlink resolves outside `cwd`, so the read is refused even though
        // it names a real image — a repo can't exfiltrate host files this way.
        assert!(GitSourceControl.file_bytes(&repo, "evil.png", None).is_err());

        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn merge_to_branch_merges_current_branch_into_target() {
        let repo = test_repo("merge-success");
        write_file(&repo, "base.txt", "base\n");
        commit_all(&repo, "initial");
        git_test(&repo, &["checkout", "-b", "feature"]);
        write_file(&repo, "feature.txt", "feature\n");
        commit_all(&repo, "feature work");

        let output = GitSourceControl.merge_to_branch(&repo, "main").unwrap();

        assert!(output.contains("Merged feature into main."));
        assert_eq!(git_test(&repo, &["show", "main:feature.txt"]), "feature\n");
        assert_eq!(
            git_test(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
            "feature"
        );
        assert_eq!(git_test(&repo, &["status", "--porcelain"]), "");
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn merge_to_branch_conflict_aborts_without_changing_target() {
        let repo = test_repo("merge-conflict");
        write_file(&repo, "file.txt", "base\n");
        commit_all(&repo, "initial");
        git_test(&repo, &["checkout", "-b", "feature"]);
        write_file(&repo, "file.txt", "feature\n");
        commit_all(&repo, "feature edit");
        git_test(&repo, &["checkout", "main"]);
        write_file(&repo, "file.txt", "main\n");
        commit_all(&repo, "main edit");
        git_test(&repo, &["checkout", "feature"]);

        let err = GitSourceControl.merge_to_branch(&repo, "main").unwrap_err();

        assert!(err.downcast_ref::<MergeConflict>().is_some());
        assert_eq!(git_test(&repo, &["show", "main:file.txt"]), "main\n");
        assert_eq!(
            git_test(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
            "feature"
        );
        assert_eq!(git_test(&repo, &["status", "--porcelain"]), "");
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn merge_to_branch_uses_targets_live_worktree() {
        // The ASM model checks each session's branch out in its own worktree,
        // so the target branch is frequently already live. That takes the
        // "merge in place" path instead of the temporary-worktree path.
        let repo = test_repo("merge-livewt");
        write_file(&repo, "base.txt", "base\n");
        commit_all(&repo, "initial");
        git_test(&repo, &["checkout", "-b", "feature"]);
        let main_wt = repo.with_extension("main-wt");
        git_test(
            &repo,
            &["worktree", "add", main_wt.to_str().unwrap(), "main"],
        );
        write_file(&repo, "feature.txt", "feature\n");
        commit_all(&repo, "feature work");

        let output = GitSourceControl.merge_to_branch(&repo, "main").unwrap();

        assert!(output.contains("Merged feature into main."));
        // The merge landed in main's live worktree, which stays checked out and
        // clean; nothing was removed since no temporary worktree was created.
        assert_eq!(
            fs::read_to_string(main_wt.join("feature.txt")).unwrap(),
            "feature\n"
        );
        assert_eq!(
            git_test(&main_wt, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
            "main"
        );
        assert_eq!(git_test(&main_wt, &["status", "--porcelain"]), "");
        assert_eq!(
            git_test(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
            "feature"
        );
        let _ = fs::remove_dir_all(&main_wt);
        let _ = fs::remove_dir_all(repo);
    }

    /// A bare repo standing in for `origin`.
    fn bare_origin(name: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "asm-origin-{name}-{}-{now}.git",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        git_test(&dir, &["init", "-q", "--bare"]);
        dir
    }

    #[test]
    fn push_creates_remote_branch_and_sets_upstream() {
        let repo = test_repo("push-create");
        let origin = bare_origin("push-create");
        git_test(&repo, &["remote", "add", "origin", origin.to_str().unwrap()]);
        write_file(&repo, "a.txt", "hello\n");
        commit_all(&repo, "initial");

        // The remote has no `main` yet; push must create it and record tracking.
        let output = GitSourceControl.push(&repo).unwrap();
        assert!(!output.is_empty());

        // The branch now exists on origin at our HEAD, and @{u} resolves — so a
        // later Pull has an upstream to pull from.
        let local_head = git_test(&repo, &["rev-parse", "HEAD"]).trim().to_string();
        let remote_head = git_test(&origin, &["rev-parse", "main"]).trim().to_string();
        assert_eq!(local_head, remote_head);
        assert_eq!(
            git_test(&repo, &["rev-parse", "--abbrev-ref", "@{u}"]).trim(),
            "origin/main"
        );

        // A second push of new work fast-forwards the existing remote branch.
        write_file(&repo, "b.txt", "more\n");
        commit_all(&repo, "second");
        GitSourceControl.push(&repo).unwrap();
        assert_eq!(
            git_test(&repo, &["rev-parse", "HEAD"]).trim(),
            git_test(&origin, &["rev-parse", "main"]).trim()
        );

        let _ = fs::remove_dir_all(origin);
        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn push_without_origin_reports_it() {
        let repo = test_repo("push-no-origin");
        write_file(&repo, "a.txt", "hello\n");
        commit_all(&repo, "initial");

        let err = GitSourceControl.push(&repo).unwrap_err().to_string();
        assert!(err.contains("origin"), "unexpected message: {err}");

        let _ = fs::remove_dir_all(repo);
    }

    #[test]
    fn push_rejects_detached_head() {
        let repo = test_repo("push-detached");
        write_file(&repo, "a.txt", "hello\n");
        commit_all(&repo, "initial");
        let head = git_test(&repo, &["rev-parse", "HEAD"]).trim().to_string();
        git_test(&repo, &["checkout", "-q", &head]); // detach

        let err = GitSourceControl.push(&repo).unwrap_err().to_string();
        assert!(err.contains("detached"), "unexpected message: {err}");

        let _ = fs::remove_dir_all(repo);
    }
}
