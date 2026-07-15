//! Workspace / worktree / Git service for [`SessionManager`] (RF-M4 split).
//!
//! The workspace-registration and per-session worktree cluster: resolving
//! where a session runs, registering/listing/removing workspaces, branch
//! listing, instance cleanup, orphan-worktree sweeps, and archive/discard.
//! Moved verbatim out of `mod.rs`; the logic is unchanged. Cross-module
//! helpers (`canonical`, `delete_orphan_branch`, `WorktreeCleanupReport`,
//! `NeedsForce`) and the `crate::workspace` git helpers arrive via `super::*`.

use super::*;

impl SessionManager {
    /// Decide where a session runs: an isolated worktree for a Git workspace,
    /// the source root for a direct/plain workspace, or a raw allowlisted path.
    pub(super) fn resolve_workspace(
        &self,
        session_id: &str,
        req: &CreateSessionRequest,
    ) -> Result<(String, Option<WorkspaceInstance>)> {
        let now = now_millis();
        match &req.workspace_id {
            Some(ws_id) => {
                let ws = self
                    .db
                    .get_workspace(ws_id)?
                    .ok_or_else(|| anyhow!("unknown workspace `{ws_id}`"))?;
                let root = PathBuf::from(&ws.root_path);
                if !root.is_dir() {
                    bail!("workspace root does not exist: {}", ws.root_path);
                }

                if ws.is_git && !req.direct_checkout {
                    // Isolated managed worktree. The caller may select an
                    // existing branch, name a new one, or let us auto-generate.
                    let instance_path = self.worktree_root.join(session_id);
                    let auto = format!("asm-session/{}", &session_id[..8.min(session_id.len())]);
                    let requested = req
                        .branch
                        .as_deref()
                        .map(str::trim)
                        .filter(|b| !b.is_empty());
                    let base = req
                        .base_ref
                        .as_deref()
                        .map(str::trim)
                        .filter(|b| !b.is_empty())
                        .unwrap_or("HEAD");

                    // Picking an existing branch that is already checked out
                    // somewhere: share that working tree rather than fail. Git
                    // forbids a second checkout of one branch, and sharing is
                    // exactly what lets two sessions (e.g. plan-with-CC then
                    // review-with-codex) see the same diffs.
                    if let Some(name) = requested {
                        if !req.create_branch {
                            if let Some((existing_path, is_main)) =
                                workspace::worktree_for_branch(&root, name)?
                            {
                                // The repo's own checkout can't become a second
                                // worktree; sharing it is a direct checkout,
                                // which owns no worktree or branch to reclaim.
                                let (path, branch, isolation) = if is_main {
                                    (ws.root_path.clone(), None, "direct")
                                } else {
                                    (existing_path, Some(name.to_string()), "shared")
                                };
                                // Inherit ownership from whoever created this
                                // worktree rather than claiming it by virtue of
                                // sharing it. A worktree we made stays reclaimable
                                // by the last session out; one the user made (no
                                // instance on record) is never ours to remove, and
                                // its branch never ours to delete.
                                let (owns_worktree, owns_branch) = self
                                    .db
                                    .instance_ownership_at_path(&path)?
                                    .unwrap_or((false, false));
                                let inst = WorkspaceInstance {
                                    id: Uuid::new_v4().to_string(),
                                    workspace_id: ws.id.clone(),
                                    session_id: Some(session_id.to_string()),
                                    path: path.clone(),
                                    branch,
                                    isolation: isolation.into(),
                                    status: "active".into(),
                                    created_at: now,
                                    owns_worktree,
                                    owns_branch,
                                };
                                return Ok((path, Some(inst)));
                            }
                        }
                    }

                    let spec = match requested {
                        Some(name) if req.create_branch => {
                            workspace::BranchSpec::New { name, base }
                        }
                        Some(name) => workspace::BranchSpec::Existing { name },
                        // `base` is "HEAD" unless the caller set one. A fork onto
                        // a new branch names no branch (so it gets the unique
                        // `asm-session/<id>` form, which the orphan sweep and
                        // archive-time branch cleanup already understand) but does
                        // set `base_ref` to the origin's branch — so it starts from
                        // the origin's work rather than from the repo's HEAD.
                        None => workspace::BranchSpec::Auto { name: &auto, base },
                    };
                    // `Existing` checks out a branch the user already had (`main`,
                    // `release`, a feature branch). We create the worktree for it,
                    // so that is ours to remove — but the branch is only borrowed,
                    // and archiving must never delete it.
                    let creates_branch = !matches!(spec, workspace::BranchSpec::Existing { .. });
                    let branch = workspace::create_worktree(&root, &instance_path, spec)?;
                    let owns_branch = creates_branch && branch.is_some();
                    let path = instance_path.to_string_lossy().into_owned();
                    let inst = WorkspaceInstance {
                        id: Uuid::new_v4().to_string(),
                        workspace_id: ws.id.clone(),
                        session_id: Some(session_id.to_string()),
                        path: path.clone(),
                        branch,
                        isolation: "worktree".into(),
                        status: "active".into(),
                        created_at: now,
                        owns_worktree: true,
                        owns_branch,
                    };
                    Ok((path, Some(inst)))
                } else {
                    // Direct source checkout (git override) or plain folder: we
                    // created neither the directory nor any branch.
                    let isolation = if ws.is_git { "direct" } else { "plain" };
                    let inst = WorkspaceInstance {
                        id: Uuid::new_v4().to_string(),
                        workspace_id: ws.id.clone(),
                        session_id: Some(session_id.to_string()),
                        path: ws.root_path.clone(),
                        branch: None,
                        isolation: isolation.into(),
                        status: "active".into(),
                        created_at: now,
                        owns_worktree: false,
                        owns_branch: false,
                    };
                    Ok((ws.root_path, Some(inst)))
                }
            }
            None => {
                if req.cwd.trim().is_empty() {
                    bail!("cwd is required when no workspace is selected");
                }
                // Raw path: enforce the allowlist once any workspace is registered.
                let workspaces = self.db.list_workspaces()?;
                if !workspaces.is_empty() {
                    let cwd_abs = canonical(&req.cwd);
                    let allowed = workspaces
                        .iter()
                        .any(|w| cwd_abs.starts_with(canonical(&w.root_path)));
                    if !allowed {
                        bail!("working directory is outside all registered workspace roots");
                    }
                }
                Ok((req.cwd.clone(), None))
            }
        }
    }

    pub fn register_workspace(&self, name: String, root_path: String) -> Result<Workspace> {
        let root = PathBuf::from(&root_path);
        if !root.is_dir() {
            bail!("root path is not a directory: {root_path}");
        }
        let canonical_root = canonical(&root_path).to_string_lossy().into_owned();
        let is_git = workspace::is_git_repo(&root);
        let w = Workspace {
            id: Uuid::new_v4().to_string(),
            name,
            root_path: canonical_root,
            is_git,
            created_at: now_millis(),
        };
        self.db.insert_workspace(&w)?;
        Ok(w)
    }

    pub fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        self.db.list_workspaces()
    }

    /// Unregister a workspace (removes it from the allowlist). Refuses while it
    /// still has live sessions. Does not stop sessions or delete worktrees on
    /// disk — it only drops the registration; existing session records keep
    /// their (now dangling) `workspace_id`.
    pub fn remove_workspace(&self, id: &str) -> Result<()> {
        let ws = self
            .db
            .get_workspace(id)?
            .ok_or_else(|| anyhow!("no such workspace"))?;
        let has_live = self
            .db
            .list_sessions()?
            .iter()
            .any(|s| s.workspace_id.as_deref() == Some(id) && !s.status.is_terminal());
        if has_live {
            bail!(
                "workspace `{}` still has live sessions; stop them first",
                ws.name
            );
        }
        self.db.delete_workspace(id)?;
        Ok(())
    }

    /// Local branches and current HEAD for a workspace, for the new-session
    /// branch picker. Empty for non-Git workspaces.
    pub fn list_workspace_branches(&self, id: &str) -> Result<(Vec<String>, Option<String>)> {
        let w = self
            .db
            .get_workspace(id)?
            .ok_or_else(|| anyhow!("no such workspace"))?;
        if !w.is_git {
            return Ok((vec![], None));
        }
        workspace::list_branches(Path::new(&w.root_path))
    }

    pub fn init_workspace_git(&self, id: &str) -> Result<Workspace> {
        let w = self
            .db
            .get_workspace(id)?
            .ok_or_else(|| anyhow!("no such workspace"))?;
        if w.is_git {
            return Ok(w);
        }
        workspace::init_repo(Path::new(&w.root_path))?;
        self.db.set_workspace_git(id, true)?;
        self.db
            .get_workspace(id)?
            .ok_or_else(|| anyhow!("workspace vanished"))
    }

    pub fn get_instance_for_session(&self, session_id: &str) -> Result<Option<WorkspaceInstance>> {
        self.db.get_instance_for_session(session_id)
    }

    /// Toggle whether an isolated session worktree holds its recorded branch.
    /// The branch name remains in the instance record while detached so the UI
    /// can safely attach the same branch again after external verification.
    pub fn set_instance_branch_attached(&self, session_id: &str, attached: bool) -> Result<String> {
        let inst = self
            .db
            .get_instance_for_session(session_id)?
            .ok_or_else(|| anyhow!("no workspace instance for session"))?;
        if inst.status != "active" {
            bail!("workspace instance is no longer active");
        }
        if inst.isolation != "worktree" && inst.isolation != "shared" {
            bail!("only isolated Git worktrees can detach from a branch");
        }
        let branch = inst
            .branch
            .as_deref()
            .ok_or_else(|| anyhow!("this worktree has no recorded branch to reattach"))?;
        let path = Path::new(&inst.path);

        if attached {
            workspace::attach_branch(path, branch)?;
        } else {
            workspace::detach_branch(path, branch)?;
        }
        Ok(branch.to_string())
    }

    /// Remove a session's managed worktree. Guards against dirty worktrees and
    /// live sessions unless `force`.
    pub fn cleanup_instance(&self, session_id: &str, force: bool) -> Result<()> {
        let inst = self
            .db
            .get_instance_for_session(session_id)?
            .ok_or_else(|| anyhow!("no workspace instance for session"))?;
        if inst.status == "released" {
            return Ok(());
        }
        if inst.isolation == "worktree" || inst.isolation == "shared" {
            if self.live_handle(session_id).is_some() {
                bail!("stop the session before cleaning up its worktree");
            }
            // Only reclaim the worktree once the last session sharing it leaves.
            if self.db.count_active_instances_at_path(&inst.path, &inst.id)? == 0 {
                let ws = self
                    .db
                    .get_workspace(&inst.workspace_id)?
                    .ok_or_else(|| anyhow!("workspace record missing"))?;
                workspace::remove_worktree(Path::new(&ws.root_path), Path::new(&inst.path), force)?;
            }
        }
        self.db.set_instance_status(&inst.id, "released")?;
        Ok(())
    }

    /// Find and remove worktrees/branches in a workspace's repo that this daemon
    /// no longer owns — leftovers from throwaway/other daemons that shared the
    /// repo (the "branch already checked out" cause). "Orphaned" = an
    /// `asm-session/*` worktree or branch whose session is unknown to this daemon.
    /// Guards uncommitted (dirty) worktrees and unmerged branches unless `force`.
    pub fn cleanup_orphan_worktrees(
        &self,
        workspace_id: &str,
        force: bool,
    ) -> Result<WorktreeCleanupReport> {
        let ws = self
            .db
            .get_workspace(workspace_id)?
            .ok_or_else(|| anyhow!("no such workspace"))?;
        if !ws.is_git {
            bail!("workspace `{}` is not a git repository", ws.name);
        }
        let root = Path::new(&ws.root_path);

        // Auto branches are `asm-session/<first 8 chars of the session uuid>`. A
        // worktree/branch whose suffix matches a session this daemon knows about
        // (live or ended) is owned, not orphaned.
        let known: std::collections::HashSet<String> = self
            .db
            .list_sessions()?
            .iter()
            .filter_map(|s| s.id.get(..8).map(str::to_string))
            .collect();

        let mut report = WorktreeCleanupReport::default();

        // 1. Drop registrations whose directories are already gone (always safe).
        let _ = workspace::prune_worktrees(root);

        // 2. Remove orphaned managed worktrees.
        let worktrees = workspace::list_worktrees(root)?;
        for (i, wt) in worktrees.iter().enumerate() {
            if i == 0 {
                continue; // the main worktree
            }
            let Some(branch) = wt.branch.as_deref() else {
                continue; // detached / no branch
            };
            let Some(suffix) = branch.strip_prefix("asm-session/") else {
                continue; // only our auto-managed worktrees
            };
            if known.contains(suffix) {
                continue; // owned by a session we know
            }
            let path = Path::new(&wt.path);
            if !force && workspace::worktree_is_dirty(path) {
                report.skipped_dirty.push(wt.path.clone());
                continue;
            }
            if workspace::remove_worktree(root, path, force).is_ok() {
                report.removed_worktrees.push(wt.path.clone());
                delete_orphan_branch(root, branch, force, &mut report);
            } else {
                report.skipped_dirty.push(wt.path.clone());
            }
        }

        // 3. Orphaned `asm-session/*` branches that have no worktree left.
        let (branches, _head) = workspace::list_branches(root)?;
        for b in branches {
            let Some(suffix) = b.strip_prefix("asm-session/") else {
                continue;
            };
            if known.contains(suffix) || report.deleted_branches.contains(&b) {
                continue;
            }
            delete_orphan_branch(root, &b, force, &mut report);
        }

        Ok(report)
    }

    /// Archive a finished session. Unlike a plain "finished" session (which stays
    /// in history with its worktree kept), archiving is the "throw this away"
    /// step: it reclaims what the session created — its managed worktree, and its
    /// branch if we made that branch — then marks the record `archived` (dropped
    /// from the history view). A session that ran on a pre-existing branch keeps
    /// that branch. Refuses to discard uncommitted or unmerged work unless
    /// `force` — see `discard_instance`.
    pub fn archive_session(&self, id: &str, force: bool) -> Result<Session> {
        let s = self
            .db
            .get_session(id)?
            .ok_or_else(|| anyhow!("no such session"))?;
        if !s.status.is_terminal() {
            bail!("cannot archive a live session; stop it first");
        }
        self.discard_instance(id, force)?;
        self.db
            .update_status(id, SessionStatus::Archived, s.exit_code, now_millis())?;
        self.db
            .get_session(id)?
            .ok_or_else(|| anyhow!("session vanished"))
    }

    /// Tear down whatever this session's instance *created* — its managed
    /// worktree, its branch — and reclaim it. A no-op for ad-hoc sessions and
    /// direct/plain instances (which share the source checkout and own nothing).
    ///
    /// What gets removed is decided by the ownership recorded at creation, never
    /// by isolation: a session can be handed a branch that already existed
    /// (`main`, `release`) or dropped into a worktree the user made themselves,
    /// and archiving such a session must leave both standing. Only what we
    /// created is ours to delete; anything else we merely release our claim on.
    ///
    /// Guards against data loss unless `force`: a dirty worktree or an unmerged
    /// branch raises [`NeedsForce`] so the caller can confirm before anything is
    /// removed. Both checks run before the worktree is touched, so a refusal
    /// leaves everything intact. `force` discards *our* work — it never widens
    /// what we own.
    fn discard_instance(&self, session_id: &str, force: bool) -> Result<()> {
        let Some(inst) = self.db.get_instance_for_session(session_id)? else {
            return Ok(()); // ad-hoc session: nothing managed to remove
        };
        if inst.isolation != "worktree" && inst.isolation != "shared" {
            return Ok(()); // direct/plain: no owned worktree or branch
        }
        let ws = self
            .db
            .get_workspace(&inst.workspace_id)?
            .ok_or_else(|| anyhow!("workspace record missing"))?;
        let root = Path::new(&ws.root_path);
        let inst_path = Path::new(&inst.path);
        let active = inst.status == "active";

        if active && self.live_handle(session_id).is_some() {
            bail!("stop the session before archiving it");
        }

        // Another session is still working in this shared worktree: relinquish
        // our own claim but leave the directory and branch for the remaining
        // sharer(s). Whoever leaves last (this check returns 0) reclaims both.
        // No `force` bypass — force discards *our* work, never evicts a sharer.
        if self.db.count_active_instances_at_path(&inst.path, &inst.id)? > 0 {
            if active {
                self.db.set_instance_status(&inst.id, "released")?;
            }
            return Ok(());
        }

        let owns_worktree = inst.owns_worktree;
        let branch = inst.branch.as_deref().filter(|_| inst.owns_branch);

        // Refuse to silently discard work unless forced. Both guards surface as
        // `NeedsForce` (→ HTTP 409) so the client can confirm and retry. Neither
        // fires for a resource we are not going to touch: uncommitted changes in
        // a worktree we won't remove are in no danger, so there is nothing to
        // confirm.
        if !force {
            if active && owns_worktree && workspace::worktree_is_dirty(inst_path) {
                return Err(NeedsForce(
                    "worktree has uncommitted changes; archiving would discard them".into(),
                )
                .into());
            }
            if let Some(branch) = branch {
                if workspace::branch_exists(root, branch)
                    && !workspace::branch_is_merged(root, branch)
                {
                    return Err(NeedsForce(format!(
                        "branch `{branch}` has unmerged commits; archiving would delete them"
                    ))
                    .into());
                }
            }
        }

        // Safe (or forced): drop the worktree first (a branch checked out in a
        // worktree cannot be deleted), then the branch itself.
        if active && owns_worktree {
            workspace::remove_worktree(root, inst_path, force)?;
        }
        if active {
            self.db.set_instance_status(&inst.id, "released")?;
        }
        if let Some(branch) = branch {
            if workspace::branch_exists(root, branch) {
                workspace::delete_branch(root, branch, force)?;
            }
        }
        Ok(())
    }
}
