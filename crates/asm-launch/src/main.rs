//! `asm-launch` — the Windows service scripts' `nohup … &`.
//!
//! ```text
//! asm-launch [--log FILE] [--cwd DIR] [--pid-file FILE] -- PROGRAM [ARG...]
//! ```
//!
//! Starts PROGRAM detached from this terminal (see [`asm_launch::detach`]) with
//! stdin closed and stdout + stderr appended to FILE (or discarded), prints its
//! pid — a real Windows pid, which a shell's `$!` under MSYS is not — and exits
//! without waiting for it.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::process::{Command, ExitCode, Stdio};

const USAGE: &str =
    "usage: asm-launch [--log FILE] [--cwd DIR] [--pid-file FILE] -- PROGRAM [ARG...]";

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let (mut log, mut cwd, mut pid_file) = (None, None, None);
    loop {
        let Some(arg) = args.next() else {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        };
        let slot = match arg.to_str() {
            Some("--") => break,
            Some("--log") => &mut log,
            Some("--cwd") => &mut cwd,
            Some("--pid-file") => &mut pid_file,
            Some("-h" | "--help") => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("asm-launch: unexpected argument {arg:?}\n{USAGE}");
                return ExitCode::from(2);
            }
        };
        match args.next() {
            Some(v) => *slot = Some(v),
            None => {
                eprintln!("asm-launch: {arg:?} needs a value\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(program) = args.next() else {
        eprintln!("asm-launch: no PROGRAM after `--`\n{USAGE}");
        return ExitCode::from(2);
    };
    let rest: Vec<OsString> = args.collect();

    match launch(&program, &rest, log, cwd, pid_file) {
        Ok(pid) => {
            println!("{pid}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!(
                "asm-launch: cannot start {}: {e}",
                program.to_string_lossy()
            );
            ExitCode::FAILURE
        }
    }
}

fn launch(
    program: &OsString,
    args: &[OsString],
    log: Option<OsString>,
    cwd: Option<OsString>,
    pid_file: Option<OsString>,
) -> std::io::Result<u32> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null());
    match log {
        Some(path) => {
            let out = OpenOptions::new().create(true).append(true).open(&path)?;
            cmd.stderr(out.try_clone()?).stdout(out);
        }
        None => {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let child = asm_launch::detach(&mut cmd)?;
    let pid = child.id();
    if let Some(path) = pid_file {
        std::fs::write(path, format!("{pid}\n"))?;
    }
    Ok(pid)
}
