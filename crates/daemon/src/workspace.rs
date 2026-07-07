//! Git worktree helpers for per-session workspace isolation.
//!
//! Independent sessions on the same repository each get their own managed
//! worktree (separate working directory + index) on an app-managed branch, so
//! they never share one writable working tree.

use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, bail, Result};

use crate::source_control::{current_branch, git};

pub fn is_git_repo(root: &Path) -> bool {
    matches!(
        git(root, &["rev-parse", "--is-inside-work-tree"]),
        Ok(out) if out.trim() == "true"
    )
}

/// `git init` a plain folder so it gains full change tracking.
pub fn init_repo(root: &Path) -> Result<()> {
    git(root, &["init"])?;
    Ok(())
}

/// How a managed worktree's branch is chosen.
pub enum BranchSpec<'a> {
    /// Create a fresh app-managed branch off HEAD; on name collision fall back
    /// to a detached HEAD so session creation never blocks.
    Auto { name: &'a str },
    /// Create a new branch `name` starting at `base` (a branch, tag, or commit).
    New { name: &'a str, base: &'a str },
    /// Check out an existing branch `name` in the new worktree.
    Existing { name: &'a str },
}

/// Create a managed worktree at `instance_path` following `spec`. Returns the
/// branch that ended up checked out, or `None` for a detached-HEAD worktree.
pub fn create_worktree(
    root: &Path,
    instance_path: &Path,
    spec: BranchSpec<'_>,
) -> Result<Option<String>> {
    if let Some(parent) = instance_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let path_str = instance_path
        .to_str()
        .ok_or_else(|| anyhow!("non-UTF8 worktree path"))?;

    match spec {
        BranchSpec::Auto { name } => {
            if git(root, &["worktree", "add", "-b", name, path_str, "HEAD"]).is_ok() {
                return Ok(Some(name.to_string()));
            }
            // Branch name may collide; fall back to a detached worktree.
            git(root, &["worktree", "add", "--detach", path_str, "HEAD"])
                .map_err(|e| anyhow!("worktree add failed: {e}"))?;
            Ok(None)
        }
        BranchSpec::New { name, base } => {
            git(root, &["worktree", "add", "-b", name, path_str, base])
                .map_err(|e| anyhow!("could not create branch `{name}`: {e}"))?;
            Ok(Some(name.to_string()))
        }
        BranchSpec::Existing { name } => {
            // No -b: check out the existing branch. Git refuses if it is already
            // checked out in another worktree, which surfaces as a clear error.
            git(root, &["worktree", "add", path_str, name])
                .map_err(|e| anyhow!("could not check out branch `{name}`: {e}"))?;
            Ok(Some(name.to_string()))
        }
    }
}

/// List local branch names plus the current HEAD branch (`None` if detached).
pub fn list_branches(root: &Path) -> Result<(Vec<String>, Option<String>)> {
    if !is_git_repo(root) {
        return Ok((vec![], None));
    }
    let out = git(root, &["branch", "--format=%(refname:short)"])?;
    let branches: Vec<String> = out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok((branches, current_branch(root)))
}

/// True if the worktree has uncommitted changes (tracked or untracked).
pub fn worktree_is_dirty(instance_path: &Path) -> bool {
    match git(instance_path, &["status", "--porcelain", "--untracked-files=all"]) {
        Ok(out) => !out.trim().is_empty(),
        // If we cannot tell, err on the side of "dirty" so cleanup is guarded.
        Err(_) => true,
    }
}

/// Remove a managed worktree. Refuses a dirty worktree unless `force`.
pub fn remove_worktree(root: &Path, instance_path: &Path, force: bool) -> Result<()> {
    if !force && worktree_is_dirty(instance_path) {
        bail!("worktree has uncommitted changes; pass force to remove it");
    }
    let path_str = instance_path
        .to_str()
        .ok_or_else(|| anyhow!("non-UTF8 worktree path"))?;
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(path_str);
    git(root, &args).map_err(|e| anyhow!("worktree remove failed: {e}"))?;
    Ok(())
}

/// One entry from `git worktree list --porcelain`.
pub struct WorktreeEntry {
    pub path: String,
    /// The checked-out branch (short name), or `None` for a detached worktree.
    pub branch: Option<String>,
}

/// List all worktrees registered on `root`. The first entry is the main worktree.
pub fn list_worktrees(root: &Path) -> Result<Vec<WorktreeEntry>> {
    let out = git(root, &["worktree", "list", "--porcelain"])?;
    let mut entries = Vec::new();
    let mut path: Option<String> = None;
    let mut branch: Option<String> = None;
    for line in out.lines() {
        if line.trim().is_empty() {
            if let Some(p) = path.take() {
                entries.push(WorktreeEntry { path: p, branch: branch.take() });
            }
            continue;
        }
        if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(p.to_string());
        } else if let Some(b) = line.strip_prefix("branch ") {
            branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
        }
    }
    if let Some(p) = path.take() {
        entries.push(WorktreeEntry { path: p, branch: branch.take() });
    }
    Ok(entries)
}

/// Find where `branch` is currently checked out, if anywhere. Returns the
/// worktree path and whether it is the repository's main working tree (the
/// first entry from `git worktree list`). `None` means the branch exists only
/// as a ref with no live checkout, so a fresh worktree can be created for it.
pub fn worktree_for_branch(root: &Path, branch: &str) -> Result<Option<(String, bool)>> {
    let worktrees = list_worktrees(root)?;
    Ok(worktrees
        .iter()
        .enumerate()
        .find(|(_, wt)| wt.branch.as_deref() == Some(branch))
        .map(|(i, wt)| (wt.path.clone(), i == 0)))
}

/// Drop registrations for worktrees whose directories no longer exist.
pub fn prune_worktrees(root: &Path) -> Result<()> {
    git(root, &["worktree", "prune"])?;
    Ok(())
}

/// Whether a local branch of this name exists in `root`.
pub fn branch_exists(root: &Path, branch: &str) -> bool {
    // `--output()` (not `status()`): `rev-parse --verify` prints the resolved
    // SHA on success, which must not leak to the daemon's stdout.
    Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .current_dir(root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Whether `branch` is fully contained in the main worktree's current HEAD
/// (i.e. deleting it loses no unique commits).
pub fn branch_is_merged(root: &Path, branch: &str) -> bool {
    Command::new("git")
        .args(["merge-base", "--is-ancestor", branch, "HEAD"])
        .current_dir(root)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Delete a local branch. `force` uses `-D` (drops unmerged commits).
pub fn delete_branch(root: &Path, branch: &str, force: bool) -> Result<()> {
    git(root, &["branch", if force { "-D" } else { "-d" }, branch])?;
    Ok(())
}
