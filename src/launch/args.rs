//! What gitcraft is asked to do on its command line:
//!
//! ```text
//! gitcraft [<folder>]                 open the repository the folder is in (`.` too)
//! gitcraft log <file>                 the file's history
//! gitcraft blame <file>[:<line>]      the file's blame, at that line
//! gitcraft blame -- <file>            the file's blame, its name taken as it is (`a:12`)
//! gitcraft show <rev>                 that commit, in the repository of this folder
//! gitcraft compare <a>..<b>           what differs between two commits (`a...b`: since they
//!                                     diverged)
//! gitcraft -- <folder>                a folder called `log`, `show`…
//! ```

use std::ffi::OsString;
use std::path::PathBuf;

/// A request from the command line, its paths as typed (see `resolve` for what they mean).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Open(PathBuf),
    Log(PathBuf),
    Blame { file: PathBuf, line: Option<u32> },
    Show(String),
    Compare(Range),
}

/// Two commits to compare.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    pub base: String,
    pub target: String,
    /// `a...b`: only what `b` changed since the two diverged.
    pub since_fork: bool,
}

/// What `main` does with the command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Parsed {
    /// Start the window, for this (or for the start page).
    Run(Option<Command>),
    /// Print this and end: `--help`, `--version`.
    Print(String),
    /// Print this to the error output and end with an error.
    Fail(String),
}

pub const USAGE: &str = "\
Usage:
  gitcraft [<folder>]             open the repository the folder is in
  gitcraft log <file>             a file's history
  gitcraft blame <file>[:<line>]  a file's blame, scrolled to that line
  gitcraft blame -- <file>        a file's blame, its name taken as it is
  gitcraft show <rev>             a commit of the repository of this folder
  gitcraft compare <a>..<b>       the files that differ between two commits
                                  (a...b: what b changed since they diverged)
  gitcraft -- <folder>            a folder named like a command
  gitcraft --help | --version";

/// Reads the arguments after the program's name.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Parsed {
    let args: Vec<OsString> = args.into_iter().collect();
    match args.as_slice() {
        [] => Parsed::Run(None),
        [one] if one == "-h" || one == "--help" => Parsed::Print(USAGE.to_owned()),
        [one] if one == "-V" || one == "--version" => {
            Parsed::Print(format!("gitcraft {}", env!("CARGO_PKG_VERSION")))
        }
        [dashes, path] if dashes == "--" => Parsed::Run(Some(Command::Open(path.into()))),
        [cmd, rest @ ..] => match (cmd.to_str(), rest) {
            (Some("log"), [file]) => Parsed::Run(Some(Command::Log(file.into()))),
            (Some("blame"), [file]) => blame(file),
            // From the file managers' menus: the name as it is, whatever it ends with.
            (Some("blame"), [dashes, file]) if dashes == "--" => {
                Parsed::Run(Some(Command::Blame {
                    file: file.into(),
                    line: None,
                }))
            }
            (Some("show"), [rev]) => {
                rev_text(rev).map_or_else(Parsed::Fail, |rev| Parsed::Run(Some(Command::Show(rev))))
            }
            (Some("compare"), [range]) => compare(range),
            (Some("log" | "blame" | "show" | "compare"), _) => {
                Parsed::Fail(format!("{}\n\n{USAGE}", wrong_count(cmd)))
            }
            (Some(flag), _) if flag.starts_with('-') && flag != "-" => {
                Parsed::Fail(format!("Unknown option {flag}\n\n{USAGE}"))
            }
            (_, []) => Parsed::Run(Some(Command::Open(cmd.into()))),
            _ => Parsed::Fail(format!("Too many arguments\n\n{USAGE}")),
        },
    }
}

fn wrong_count(cmd: &OsString) -> String {
    let what = match cmd.to_str() {
        Some("log") => "a file",
        Some("blame") => "a file, and :<line> if you like",
        Some("show") => "a commit",
        _ => "two commits, as a..b",
    };
    format!("gitcraft {} takes {what}", cmd.to_string_lossy())
}

/// `file`, or `file:42`. Only a number after the last `:` is a line: `C:\x` and `a:b.txt` stay
/// whole, and so does a name that isn't valid text.
fn blame(arg: &OsString) -> Parsed {
    let whole = || {
        Parsed::Run(Some(Command::Blame {
            file: arg.into(),
            line: None,
        }))
    };
    let Some(text) = arg.to_str() else {
        return whole();
    };
    let Some((file, line)) = text.rsplit_once(':') else {
        return whole();
    };
    if file.is_empty() || line.is_empty() || !line.bytes().all(|b| b.is_ascii_digit()) {
        return whole();
    }
    match line.parse::<u32>() {
        Ok(n) if n > 0 => Parsed::Run(Some(Command::Blame {
            file: PathBuf::from(file),
            line: Some(n),
        })),
        _ => Parsed::Fail(format!("{line} is not a line number (the first line is 1)")),
    }
}

fn compare(arg: &OsString) -> Parsed {
    let Some(text) = arg.to_str() else {
        return Parsed::Fail("The commits to compare are not valid text".to_owned());
    };
    let (base, target, since_fork) = if let Some((a, b)) = text.split_once("...") {
        (a, b, true)
    } else if let Some((a, b)) = text.split_once("..") {
        (a, b, false)
    } else {
        return Parsed::Fail(format!(
            "Name the two commits as a..b (or a...b), not {text}"
        ));
    };
    // `a..` is `a..HEAD`, as git reads it.
    let target = if target.is_empty() { "HEAD" } else { target };
    let base = if base.is_empty() { "HEAD" } else { base };
    match (rev_text(&base.into()), rev_text(&target.into())) {
        (Ok(base), Ok(target)) => Parsed::Run(Some(Command::Compare(Range {
            base,
            target,
            since_fork,
        }))),
        (Err(e), _) | (_, Err(e)) => Parsed::Fail(e),
    }
}

/// A revision as typed, refused when git could take it for an option or it can't be a name.
pub fn rev_text(rev: &OsString) -> Result<String, String> {
    let Some(rev) = rev.to_str() else {
        return Err("The commit is not valid text".to_owned());
    };
    if rev.is_empty() {
        return Err("No commit given".to_owned());
    }
    if rev.starts_with('-') {
        return Err(format!("{rev} is not a commit: it starts with -"));
    }
    if rev.len() > 256 || rev.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(format!("{} is not a commit name", rev.escape_debug()));
    }
    Ok(rev.to_owned())
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
