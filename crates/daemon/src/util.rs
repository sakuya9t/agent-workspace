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

/// The daemon user's home directory.
///
/// On Windows `USERPROFILE` comes first: it is what the agent CLIs themselves
/// resolve `~` to (Node's `os.homedir()`, Rust's `dirs`), so it is where their
/// `~/.claude` / `~/.codex` live — whereas `HOME` is often set to something else
/// by Git for Windows or MSYS. Elsewhere, `HOME`.
pub fn home_dir() -> Option<PathBuf> {
    let order = if cfg!(windows) {
        ["USERPROFILE", "HOME"]
    } else {
        ["HOME", "USERPROFILE"]
    };
    order
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|p| !p.as_os_str().is_empty())
}

/// A path printed by git, in this platform's spelling. Git for Windows prints
/// `C:/Users/…`; everything the daemon builds itself says `C:\Users\…`, and the
/// two are compared and stored as strings (worktree ownership, the UI). A
/// no-op elsewhere.
pub fn native_separators(git_path: &str) -> String {
    if cfg!(windows) {
        git_path.replace('/', "\\")
    } else {
        git_path.to_string()
    }
}

/// [`std::fs::canonicalize`], without Windows' verbatim `\\?\` prefix.
///
/// On Windows std returns `\\?\C:\…`. Such a path is valid, but it leaks into
/// everything a canonical path feeds — workspace roots, the UI, an agent's cwd,
/// git's arguments — and plenty of programs mishandle it (agents that key their
/// state by cwd file it under a different project). So the prefix is dropped
/// whenever the plain spelling names the same file; the few paths only the
/// verbatim form can express keep it. Elsewhere this is `canonicalize` as is.
pub fn canonicalize(p: &Path) -> io::Result<PathBuf> {
    let canonical = std::fs::canonicalize(p)?;
    #[cfg(windows)]
    let canonical = without_verbatim_prefix(canonical);
    Ok(canonical)
}

#[cfg(any(windows, test))]
fn without_verbatim_prefix(p: PathBuf) -> PathBuf {
    let Some(s) = p.to_str() else { return p };
    let plain = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        // Only a drive path has a plain spelling (`\\?\Volume{…}\` does not).
        let b = rest.as_bytes();
        if !(b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\') {
            return p;
        }
        rest.to_string()
    } else {
        return p;
    };
    // The plain form is held to MAX_PATH, and Win32 rewrites components the
    // verbatim form takes literally: trailing dots/spaces, device names.
    let reinterpreted = |c: &str| {
        let stem = c.split('.').next().unwrap_or(c).trim_end().to_ascii_uppercase();
        c.ends_with('.')
            || c.ends_with(' ')
            || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.len() == 4
                && stem.as_bytes()[3].is_ascii_digit())
    };
    if plain.len() >= 260 || plain.split('\\').any(reinterpreted) {
        return p;
    }
    PathBuf::from(plain)
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

    #[test]
    fn verbatim_prefix_is_dropped_only_where_the_plain_path_is_the_same_file() {
        let plain = |s: &str| without_verbatim_prefix(PathBuf::from(s));
        assert_eq!(plain(r"\\?\C:\Users\me\repo"), PathBuf::from(r"C:\Users\me\repo"));
        assert_eq!(plain(r"\\?\UNC\host\share\x"), PathBuf::from(r"\\host\share\x"));
        // Only the verbatim form can name these; they keep it.
        for keep in [
            r"\\?\Volume{0f0e}\x",
            r"\\?\C:\repo\aux",
            r"\\?\C:\repo\com1.txt",
            r"\\?\C:\repo\trailing.",
            r"\\?\C:\repo\trailing ",
        ] {
            assert_eq!(plain(keep), PathBuf::from(keep), "{keep}");
        }
        let long = format!(r"\\?\C:\{}", "a".repeat(300));
        assert_eq!(plain(&long), PathBuf::from(&long));
        // Not verbatim at all: untouched.
        assert_eq!(plain("/home/me/repo"), PathBuf::from("/home/me/repo"));
    }

    #[test]
    fn canonicalize_yields_a_plain_absolute_path() {
        let dir = std::env::temp_dir();
        let c = canonicalize(&dir).unwrap();
        assert!(c.is_absolute());
        assert!(!c.to_string_lossy().starts_with(r"\\?\"), "{}", c.display());
    }
}
