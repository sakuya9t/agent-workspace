use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, bail, Result};

/// Git worktree helpers for per-session workspace isolation.
///
/// Independent sessions on the same repository each get their own managed
/// worktree (separate working directory + index) on an app-managed branch, so
/// they never share one writable working tree.

pub fn is_git_repo(root: &Path) -> bool {
    matches!(
        run(root, &["rev-parse", "--is-inside-work-tree"]),
        Ok(out) if out.trim() == "true"
    )
}

/// `git init` a plain folder so it gains full change tracking.
pub fn init_repo(root: &Path) -> Result<()> {
    run(root, &["init"])?;
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
            if run(root, &["worktree", "add", "-b", name, path_str, "HEAD"]).is_ok() {
                return Ok(Some(name.to_string()));
            }
            // Branch name may collide; fall back to a detached worktree.
            run(root, &["worktree", "add", "--detach", path_str, "HEAD"])
                .map_err(|e| anyhow!("worktree add failed: {e}"))?;
            Ok(None)
        }
        BranchSpec::New { name, base } => {
            run(root, &["worktree", "add", "-b", name, path_str, base])
                .map_err(|e| anyhow!("could not create branch `{name}`: {e}"))?;
            Ok(Some(name.to_string()))
        }
        BranchSpec::Existing { name } => {
            // No -b: check out the existing branch. Git refuses if it is already
            // checked out in another worktree, which surfaces as a clear error.
            run(root, &["worktree", "add", path_str, name])
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
    let out = run(root, &["branch", "--format=%(refname:short)"])?;
    let branches: Vec<String> = out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let head_raw = run(root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_string();
    let head = if head_raw == "HEAD" || head_raw.is_empty() {
        None
    } else {
        Some(head_raw)
    };
    Ok((branches, head))
}

/// True if the worktree has uncommitted changes (tracked or untracked).
pub fn worktree_is_dirty(instance_path: &Path) -> bool {
    match run(instance_path, &["status", "--porcelain", "--untracked-files=all"]) {
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
    run(root, &args).map_err(|e| anyhow!("worktree remove failed: {e}"))?;
    Ok(())
}

fn run(cwd: &Path, args: &[&str]) -> Result<String> {
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
