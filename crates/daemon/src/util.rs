use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Current wall-clock time in milliseconds since the Unix epoch.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Create `<cwd>/.asm/` — where we drop session-local artifacts an agent should be
/// able to read (pasted attachments, a fork's handoff brief) — and return it.
///
/// The directory ignores *itself*: a `.gitignore` of `*` inside it keeps
/// everything we write out of the user's version control without touching a
/// tracked file or the repo's git config, and it works in every worktree layout.
/// Without it, our scratch files land in `git status` and can be committed by an
/// agent tidying up after itself.
///
/// The ignore write is best-effort: failing it should not fail the operation that
/// needed the directory, it only risks a dirty status entry.
pub fn asm_dir(cwd: &Path) -> io::Result<PathBuf> {
    let dir = cwd.join(".asm");
    std::fs::create_dir_all(&dir)?;
    let _ = std::fs::write(dir.join(".gitignore"), "*\n");
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asm_dir_ignores_itself_so_our_scratch_files_stay_out_of_git() {
        let tmp = std::env::temp_dir().join(format!("asm-util-{}", now_millis()));
        std::fs::create_dir_all(&tmp).unwrap();

        let dir = asm_dir(&tmp).unwrap();
        assert_eq!(dir, tmp.join(".asm"));
        assert_eq!(
            std::fs::read_to_string(dir.join(".gitignore")).unwrap(),
            "*\n",
            "without this, a fork's brief and every pasted attachment show up as \
             untracked files in the user's repo"
        );

        // Idempotent: a second fork into the same worktree must not fail.
        assert!(asm_dir(&tmp).is_ok());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
