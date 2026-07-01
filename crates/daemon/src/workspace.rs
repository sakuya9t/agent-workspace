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

/// Create a managed worktree at `instance_path` on a fresh app-managed branch.
/// Falls back to a detached HEAD if the branch already exists.
pub fn create_worktree(root: &Path, instance_path: &Path, branch: &str) -> Result<()> {
    if let Some(parent) = instance_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let path_str = instance_path
        .to_str()
        .ok_or_else(|| anyhow!("non-UTF8 worktree path"))?;

    let with_branch = run(
        root,
        &["worktree", "add", "-b", branch, path_str, "HEAD"],
    );
    if with_branch.is_ok() {
        return Ok(());
    }

    // Branch name may collide; fall back to a detached worktree.
    run(root, &["worktree", "add", "--detach", path_str, "HEAD"])
        .map_err(|e| anyhow!("worktree add failed: {e}"))?;
    Ok(())
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
