//! The OS shell calls of gitcraft, behind a safe API: on Windows, the taskbar button's progress
//! (`ITaskbarList3`) and the jump list (`ICustomDestinationList`). Elsewhere the functions do
//! nothing and say so.

/// What the taskbar button shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Progress {
    /// Nothing: no operation runs.
    None,
    /// Something runs, how far isn't known.
    Indeterminate,
    /// How far, from 0 to 1.
    Value(f32),
    /// It failed: the button turns red.
    Error,
}

/// An item of the jump list: a shortcut to the program itself with these arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub title: String,
    /// The arguments, each one word (quoted as needed when the shortcut is made).
    pub args: Vec<String>,
    /// Shown on hover.
    pub tooltip: String,
}

#[cfg(windows)]
mod process_job;
#[cfg(windows)]
mod windows_shell;
#[cfg(windows)]
pub use process_job::{ProcessJob, supervise_suspended_process};

/// Stop a process group owned by the caller. A group already gone needs no cleanup.
#[cfg(unix)]
pub fn kill_process_group(pid: u32) -> std::io::Result<()> {
    let pid = i32::try_from(pid).map_err(|_| std::io::Error::other("invalid process group"))?;
    if pid <= 1 {
        return Err(std::io::Error::other("invalid process group"));
    }
    // SAFETY: kill takes only integers, and a negative PID addresses this child's group.
    if unsafe { libc::kill(-pid, libc::SIGKILL) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

/// Sets the progress the taskbar button of the window `hwnd` (its `HWND`, as a number) shows.
/// From the window's own thread.
pub fn set_taskbar_progress(hwnd: isize, progress: Progress) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows_shell::set_progress(hwnd, progress)
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, progress);
        Err("no taskbar progress on this system".to_owned())
    }
}

/// Replaces the program's jump list: `recent` under the category `category`, then `tasks`.
/// Items the user removed from the list (by their arguments) are left out, as Windows asks.
/// `exe` is the program the items start.
pub fn set_jump_list(
    exe: &std::path::Path,
    category: &str,
    recent: &[Link],
    tasks: &[Link],
) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows_shell::set_jump_list(exe, category, recent, tasks)
    }
    #[cfg(not(windows))]
    {
        let _ = (exe, category, recent, tasks);
        Err("no jump list on this system".to_owned())
    }
}

/// The arguments as one command line, as Windows programs split it (`CommandLineToArgvW`): a
/// word with spaces or quotes in quotes, backslashes before a quote doubled.
pub fn command_line(args: &[String]) -> String {
    args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}

fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '"']) {
        return arg.to_owned();
    }
    let mut out = String::from('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_quoted_as_windows_splits_them() {
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(command_line(&args(&["--clone"])), "--clone");
        assert_eq!(
            command_line(&args(&[r"C:\code\my repo"])),
            r#""C:\code\my repo""#
        );
        assert_eq!(command_line(&args(&[r"C:\a b\"])), r#""C:\a b\\""#);
        assert_eq!(command_line(&args(&[r#"say "hi""#])), r#""say \"hi\"""#);
        assert_eq!(command_line(&args(&[""])), r#""""#);
        assert_eq!(
            command_line(&args(&[r"\\wsl.localhost\Ubuntu\home\me\dépôt"])),
            r"\\wsl.localhost\Ubuntu\home\me\dépôt"
        );
    }
}
