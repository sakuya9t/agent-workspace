//! Start a process that outlives whoever started it.
//!
//! The asm components are long-lived background processes, and the asmux
//! holder above all must outlive both the daemon (that is what makes sessions
//! survive a daemon restart) and the terminal that ran `scripts/start.sh`.
//!
//! - **Unix**: the child gets its own process group, so a signal aimed at the
//!   parent's group (Ctrl-C in its terminal, a supervisor stopping the daemon)
//!   misses it. The Unix service scripts use `nohup … &` and never need the
//!   `asm-launch` binary; the daemon's holder autospawn uses [`detach`].
//! - **Windows** has no `nohup`, and a process started from a terminal shares
//!   that terminal's console: Ctrl-C there, or closing the window, ends it. So
//!   the child is started with
//!   - `CREATE_NO_WINDOW` — a console of its own, with no window. Not
//!     `DETACHED_PROCESS`: a process with *no* console makes Windows open a
//!     fresh console window for every console program it starts, i.e. a window
//!     flashing up for each of the daemon's `git` calls.
//!   - `CREATE_NEW_PROCESS_GROUP` — outside the parent's Ctrl-C/Ctrl-Break
//!     broadcast.
//!   - `CREATE_BREAKAWAY_FROM_JOB`, when the parent's job object allows it — a
//!     terminal, IDE or agent harness that kills its whole job on exit then
//!     does not take the holder with it. A job that forbids breakaway fails
//!     the spawn with `ERROR_ACCESS_DENIED`; the child is then started inside
//!     the job rather than not at all.
//!
//!   And the caller's own stdio handles are made non-inheritable first. std
//!   spawns with handle inheritance on (it has to, to pass the child its
//!   stdio), which hands the child a copy of *every* inheritable handle we
//!   hold — including the pipe we were started with. A detached holder
//!   keeping `start.sh 2>&1 | tail` (or a CI step's output pipe) open never
//!   lets that reader see EOF. Our std handles reach a child only through
//!   `Stdio::inherit()`, which std duplicates afresh, so this costs nothing.

use std::io;
use std::process::{Child, Command};

/// Spawn `cmd` detached from the caller's terminal (see the module docs).
/// Configure stdio on `cmd` first; the child is not waited on.
pub fn detach(cmd: &mut Command) -> io::Result<Child> {
    #[cfg(windows)]
    windows::stop_inheriting_stdio();
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        cmd.spawn()
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const ERROR_ACCESS_DENIED: i32 = 5;

        let base = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
        cmd.creation_flags(base | CREATE_BREAKAWAY_FROM_JOB);
        match cmd.spawn() {
            Err(e) if e.raw_os_error() == Some(ERROR_ACCESS_DENIED) => {
                cmd.creation_flags(base);
                cmd.spawn()
            }
            other => other,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        cmd.spawn()
    }
}

#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    const HANDLE_FLAG_INHERIT: u32 = 0x1;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetHandleInformation(handle: *mut c_void, mask: u32, flags: u32) -> i32;
    }

    /// Clear `HANDLE_FLAG_INHERIT` on this process's stdin/stdout/stderr.
    pub(super) fn stop_inheriting_stdio() {
        let handles = [
            std::io::stdin().as_raw_handle(),
            std::io::stdout().as_raw_handle(),
            std::io::stderr().as_raw_handle(),
        ];
        for handle in handles {
            if handle.is_null() {
                continue; // no such stream (e.g. a GUI-subsystem parent)
            }
            // SAFETY: `handle` is one of our own standard handles, valid for the
            // life of the process; changing its inherit flag touches nothing
            // but this process's handle table. Failure (an invalid pseudo-
            // handle) is harmless — it just leaves inheritance as it was.
            unsafe {
                SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0);
            }
        }
    }
}
