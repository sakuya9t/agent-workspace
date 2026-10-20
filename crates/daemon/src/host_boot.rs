//! Which boot of this host a session's process belongs to.
//!
//! The holder keeps sessions alive across a *daemon* restart, but nothing
//! survives a host reboot — the agent, its PTY and the holder all go. After one,
//! every session that was running is recorded live (or `indeterminate`) with no
//! process behind it. Telling that apart from a holder that merely crashed is the
//! whole point of this module: a crashed holder can leave orphans still running
//! in the worktree, while a reboot leaves none. Only the second is safe to
//! relaunch without asking (see `session_manager/resume.rs`).
//!
//! The proof is the kernel's per-boot random id, stamped on a session each time
//! its process starts. A different id now means that process is gone.

use std::sync::OnceLock;

/// Rows written before `boot_id` existed are judged by their creation time
/// against the boot time instead. Those rows were spawned exactly once, at
/// `created_at`, so "created before this boot" is the same proof — given a
/// margin for wall-clock correction between then and now. A session created
/// within this window of the boot is simply not proven dead, and stays
/// `indeterminate`.
const LEGACY_MARGIN_MS: i64 = 60_000;

/// This host's current boot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BootInfo {
    /// The kernel's random id for this boot. `None` where the platform has none.
    pub id: Option<String>,
    /// When this boot started, in ms since the Unix epoch.
    pub started_at_ms: Option<i64>,
}

impl BootInfo {
    /// The running host's boot, read once.
    pub fn current() -> &'static BootInfo {
        static CURRENT: OnceLock<BootInfo> = OnceLock::new();
        CURRENT.get_or_init(read_current)
    }

    /// Whether a session whose process was stamped with `recorded` (and, for an
    /// unstamped legacy row, created at `created_at_ms`) provably died with an
    /// earlier boot of this host. `false` whenever there is no proof.
    pub fn outlived(&self, recorded: Option<&str>, created_at_ms: i64) -> bool {
        match (recorded, self.id.as_deref()) {
            (Some(recorded), Some(now)) => recorded != now,
            // Stamped, but this host can't say which boot it is on now.
            (Some(_), None) => false,
            (None, _) => self
                .started_at_ms
                .is_some_and(|boot| created_at_ms < boot - LEGACY_MARGIN_MS),
        }
    }
}

#[cfg(target_os = "linux")]
fn read_current() -> BootInfo {
    let id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let started_at_ms = std::fs::read_to_string("/proc/stat")
        .ok()
        .and_then(|stat| parse_btime(&stat));
    BootInfo { id, started_at_ms }
}

#[cfg(not(target_os = "linux"))]
fn read_current() -> BootInfo {
    BootInfo::default()
}

/// The `btime <seconds>` line of `/proc/stat`, in ms.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_btime(stat: &str) -> Option<i64> {
    stat.lines()
        .find_map(|line| line.strip_prefix("btime "))
        .and_then(|secs| secs.trim().parse::<i64>().ok())
        .map(|secs| secs * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boot(id: Option<&str>, started_at_ms: Option<i64>) -> BootInfo {
        BootInfo {
            id: id.map(str::to_string),
            started_at_ms,
        }
    }

    #[test]
    fn a_stamped_session_died_only_if_the_boot_changed() {
        let now = boot(Some("boot-b"), Some(1_000_000));
        assert!(now.outlived(Some("boot-a"), 0));
        assert!(!now.outlived(Some("boot-b"), 0), "same boot: maybe an orphan, no proof");
        // Without a boot id now, a stamp proves nothing either way.
        assert!(!boot(None, Some(1_000_000)).outlived(Some("boot-a"), 0));
    }

    #[test]
    fn a_legacy_row_died_if_it_was_created_before_this_boot() {
        let now = boot(Some("boot-b"), Some(10_000_000));
        assert!(now.outlived(None, 1_000));
        // Too close to the boot to rule out a clock correction: no proof.
        assert!(!now.outlived(None, 10_000_000 - LEGACY_MARGIN_MS + 1));
        assert!(!now.outlived(None, 10_000_001), "created after this boot");
        // No boot time: no proof.
        assert!(!boot(Some("boot-b"), None).outlived(None, 1_000));
    }

    #[test]
    fn btime_is_read_from_proc_stat() {
        let stat = "cpu  1 2 3\nintr 5\nbtime 1760915598\nprocesses 9\n";
        assert_eq!(parse_btime(stat), Some(1_760_915_598_000));
        assert_eq!(parse_btime("cpu 1\n"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn this_host_reports_its_boot() {
        let b = BootInfo::current();
        assert!(b.id.as_deref().is_some_and(|id| id.len() >= 32), "{b:?}");
        assert!(b.started_at_ms.is_some_and(|t| t > 0), "{b:?}");
        // A process alive now cannot have died with an earlier boot.
        assert!(!b.outlived(b.id.as_deref(), crate::util::now_millis()));
    }
}
