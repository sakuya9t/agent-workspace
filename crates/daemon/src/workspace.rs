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
    /// Create a fresh app-managed branch starting at `base` (`"HEAD"` for an
    /// ordinary new session; a fork passes the origin's branch, so the fork
    /// starts from the work it is forking rather than from the repo's HEAD). On
    /// name collision, fall back to a detached worktree so session creation never
    /// blocks.
    Auto { name: &'a str, base: &'a str },
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
        BranchSpec::Auto { name, base } => {
            if git(root, &["worktree", "add", "-b", name, path_str, base]).is_ok() {
                return Ok(Some(name.to_string()));
            }
            // Branch name may collide; fall back to a detached worktree.
            git(root, &["worktree", "add", "--detach", path_str, base])
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

/// Detach a managed worktree from its recorded branch without changing its
/// files. This releases Git's one-worktree-per-branch lock so another checkout
/// (for example, the source checkout used for production verification) can
/// temporarily switch to the branch.
pub fn detach_branch(instance_path: &Path, branch: &str) -> Result<()> {
    match current_branch(instance_path).as_deref() {
        None => return Ok(()),
        Some(current) if current == branch => {}
        Some(current) => bail!(
            "worktree is on `{current}`, not its recorded branch `{branch}`; refusing to detach it"
        ),
    }

    // Switching to the current commit changes only HEAD. Git leaves staged,
    // unstaged, and untracked work exactly where it is.
    git(instance_path, &["switch", "--detach", "HEAD"])?;
    Ok(())
}

/// Reattach a detached managed worktree to its recorded branch. Git performs
/// the safety checks: if another worktree still has the branch checked out, or
/// if switching would overwrite local changes, this fails without modifying
/// the worktree.
pub fn attach_branch(instance_path: &Path, branch: &str) -> Result<()> {
    match current_branch(instance_path).as_deref() {
        Some(current) if current == branch => return Ok(()),
        Some(current) => bail!(
            "worktree is on `{current}`, not detached; refusing to attach `{branch}`"
        ),
        None => {}
    }

    if !branch_exists(instance_path, branch) {
        bail!("recorded branch `{branch}` no longer exists");
    }
    let branch_ref = format!("refs/heads/{branch}");
    let head_is_contained = Command::new("git")
        .args(["merge-base", "--is-ancestor", "HEAD", &branch_ref])
        .current_dir(instance_path)
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !head_is_contained {
        bail!(
            "detached HEAD has commits that are not on `{branch}`; preserve them before reattaching"
        );
    }
    git(instance_path, &["switch", "--no-guess", branch])?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn test_repo(label: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("asm-{label}-{}", uuid::Uuid::new_v4()));
        let worktree = std::env::temp_dir().join(format!(
            "asm-{label}-worktree-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-b", "main"]).unwrap();
        git(&root, &["config", "user.email", "asm-tests@example.invalid"]).unwrap();
        git(&root, &["config", "user.name", "ASM Tests"]).unwrap();
        fs::write(root.join("tracked.txt"), "initial\n").unwrap();
        git(&root, &["add", "tracked.txt"]).unwrap();
        git(&root, &["commit", "-m", "initial"]).unwrap();
        git(&root, &["branch", "feature/deploy-me"]).unwrap();
        create_worktree(
            &root,
            &worktree,
            BranchSpec::Existing {
                name: "feature/deploy-me",
            },
        )
        .unwrap();
        (root, worktree)
    }

    #[test]
    fn detach_releases_branch_and_attach_claims_it_again_without_losing_changes() {
        let (root, worktree) = test_repo("branch-attachment");
        fs::write(worktree.join("tracked.txt"), "staged\n").unwrap();
        git(&worktree, &["add", "tracked.txt"]).unwrap();
        fs::write(worktree.join("tracked.txt"), "staged\nunstaged\n").unwrap();
        fs::write(worktree.join("local.txt"), "keep me\n").unwrap();
        let dirty_state = git(
            &worktree,
            &["status", "--porcelain", "--untracked-files=all"],
        )
        .unwrap();

        detach_branch(&worktree, "feature/deploy-me").unwrap();
        assert_eq!(current_branch(&worktree), None);
        assert_eq!(
            git(
                &worktree,
                &["status", "--porcelain", "--untracked-files=all"],
            )
            .unwrap(),
            dirty_state
        );

        // Detaching releases Git's branch lock, so the source checkout can use
        // the feature branch while production is being verified.
        git(&root, &["switch", "feature/deploy-me"]).unwrap();
        let error = attach_branch(&worktree, "feature/deploy-me")
            .unwrap_err()
            .to_string();
        assert!(error.contains("already checked out"), "{error}");

        // Verification may advance the branch. Reattaching is still safe when
        // the session's detached commit is an ancestor of the new branch tip.
        fs::write(root.join("verified.txt"), "verified\n").unwrap();
        git(&root, &["add", "verified.txt"]).unwrap();
        git(&root, &["commit", "-m", "verified deployment"]).unwrap();
        git(&root, &["switch", "main"]).unwrap();
        attach_branch(&worktree, "feature/deploy-me").unwrap();
        assert_eq!(current_branch(&worktree).as_deref(), Some("feature/deploy-me"));
        assert_eq!(
            git(
                &worktree,
                &["status", "--porcelain", "--untracked-files=all"],
            )
            .unwrap(),
            dirty_state
        );
        assert_eq!(
            fs::read_to_string(worktree.join("verified.txt")).unwrap(),
            "verified\n"
        );

        remove_worktree(&root, &worktree, true).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detach_refuses_a_different_checked_out_branch() {
        let (root, worktree) = test_repo("branch-attachment-wrong-branch");
        git(&worktree, &["switch", "-c", "other"]).unwrap();

        let error = detach_branch(&worktree, "feature/deploy-me")
            .unwrap_err()
            .to_string();
        assert!(error.contains("not its recorded branch"), "{error}");
        assert_eq!(current_branch(&worktree).as_deref(), Some("other"));

        remove_worktree(&root, &worktree, true).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn attach_refuses_to_abandon_commits_made_while_detached() {
        let (root, worktree) = test_repo("branch-attachment-detached-commit");
        detach_branch(&worktree, "feature/deploy-me").unwrap();
        fs::write(worktree.join("detached.txt"), "important\n").unwrap();
        git(&worktree, &["add", "detached.txt"]).unwrap();
        git(&worktree, &["commit", "-m", "detached work"]).unwrap();

        let detached_head = git(&worktree, &["rev-parse", "HEAD"]).unwrap();
        let error = attach_branch(&worktree, "feature/deploy-me")
            .unwrap_err()
            .to_string();
        assert!(error.contains("preserve them before reattaching"), "{error}");
        assert_eq!(current_branch(&worktree), None);
        assert_eq!(git(&worktree, &["rev-parse", "HEAD"]).unwrap(), detached_head);

        remove_worktree(&root, &worktree, true).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
