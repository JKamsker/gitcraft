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

mod close_file;
pub use close_file::close_file;
mod workspace_lock;
pub use workspace_lock::{WorkspaceClaim, WorkspaceFence, WorkspaceGrant};
mod private_directory;
pub use private_directory::{PrivateDirectory, PrivateFileMode, file_identity};
mod receipt_retirement;
pub use receipt_retirement::retire_private_receipt;
mod private;
pub use private::{create_private_directory, detach_controller, random_bytes};
mod bounded_input;
pub use bounded_input::BoundedInput;
mod controller_reaper;
mod owned_child;
#[cfg(windows)]
mod process_job;
#[cfg(any(windows, test))]
mod suspended;
#[cfg(unix)]
mod unix_anchor;
#[cfg(windows)]
mod windows_shell;
pub use owned_child::OwnedChild;

/// Retire this process's installed helper only after every process service has shut down.
pub fn retire_process_helpers() {
    #[cfg(unix)]
    unix_anchor::retire();
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

#[cfg(unix)]
mod account;
#[cfg(unix)]
pub use account::account_home;
