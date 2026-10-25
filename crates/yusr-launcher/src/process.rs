//! Child processes without console windows, and cleanup of ones a crash left behind.

use std::path::Path;
use std::process::{Child, Command, Stdio};

/// On Windows a console program started from a windowed app would flash a black console
/// window; this flag gives it a hidden console instead (children inherit it).
#[cfg(windows)]
pub fn hidden(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
pub fn hidden(cmd: &mut Command) -> &mut Command {
    cmd
}

/// Runs a tool to completion and returns its output, or its stderr as the error.
/// For daemon starters (like `pg_ctl start`), stdout/stderr pipes are avoided to prevent
/// pipe handle inheritance deadlocks on Windows where child daemons hold pipes open.
pub fn run(program: &Path, args: &[String]) -> Result<String, String> {
    let is_daemon_starter = args.first().is_some_and(|s| s == "start");
    if is_daemon_starter {
        let status = hidden(
            Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
        )
        .status()
        .map_err(|e| format!("{}: {e}", program.display()))?;

        if status.success() {
            Ok(String::new())
        } else {
            Err(format!("{} failed ({status})", program.display()))
        }
    } else {
        let out = hidden(Command::new(program).args(args).stdin(Stdio::null()))
            .output()
            .map_err(|e| format!("{}: {e}", program.display()))?;
        let text = |b: &[u8]| String::from_utf8_lossy(b).trim().to_owned();
        if out.status.success() {
            Ok(text(&out.stdout))
        } else {
            Err(format!("{} failed ({}): {} {}", program.display(), out.status, text(&out.stderr), text(&out.stdout)))
        }
    }
}

/// Starts a long-running server with its output in `log`.
pub fn background(program: &Path, args: &[String], log: std::fs::File) -> std::io::Result<Child> {
    let err = log.try_clone()?;
    hidden(Command::new(program).args(args).stdin(Stdio::null()).stdout(log).stderr(err)).spawn()
}

/// Ends a model server left running by a previous session that crashed. The pid file is
/// written by us only, and the process is checked by name, so an unrelated process that
/// reused the pid is left alone.
pub fn kill_stale(pid_file: &Path) {
    let Ok(pid) = std::fs::read_to_string(pid_file).map(|s| s.trim().to_owned()) else { return };
    if pid.is_empty() || !pid.chars().all(|c| c.is_ascii_digit()) {
        return;
    }
    #[cfg(windows)]
    {
        let filter = format!("PID eq {pid}");
        let listed = hidden(Command::new("tasklist").args(["/FI", &filter, "/FO", "CSV", "/NH"])).output();
        if listed.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("llama-server")) {
            let _ = hidden(Command::new("taskkill").args(["/PID", &pid, "/F"])).output();
        }
    }
    #[cfg(not(windows))]
    {
        let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok();
        if exe.is_some_and(|e| e.to_string_lossy().contains("llama-server")) {
            let _ = Command::new("kill").arg(&pid).output();
        }
    }
    let _ = std::fs::remove_file(pid_file);
}
