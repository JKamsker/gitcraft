//! `gitcraft mcp`: gitcraft as a Model Context Protocol server, for coding agents (Claude Code,
//! Codex…) to add over stdio. It reads the repository it is started in with gitcraft's own git
//! layer, and hands the changes an agent asks for to the gitcraft window that has that
//! repository open: there they land in the undo list, marked as the agent's, and what can't be
//! undone waits for the user on an Approve / Deny card (see `repo::agent`).
//!
//! - [`server`]: the protocol, through the official SDK (`rmcp`).
//! - [`tools`]: what each tool takes and does.
//! - [`link`] and [`wire`]: reaching the window over the single-instance channel
//!   (`crate::instance`), and the operations and answers that go over it.

pub(crate) mod args;
pub mod link;
pub(crate) mod output;
pub(crate) mod server;
pub(crate) mod tools;
pub mod wire;

use std::ffi::OsString;
use std::path::PathBuf;

/// How to start this gitcraft from an agent's settings: `gitcraft` when that is the one found on
/// the PATH, else its whole path (quoted when it has spaces).
pub fn program() -> String {
    // Read once: the settings page asks on every frame, and this looks through the PATH.
    static PROGRAM: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PROGRAM.get_or_init(find_program).clone()
}

fn find_program() -> String {
    let Ok(exe) = std::env::current_exe() else {
        return "gitcraft".to_owned();
    };
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let on_path = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| {
            let found = dir.join(if cfg!(windows) {
                "gitcraft.exe"
            } else {
                "gitcraft"
            });
            std::fs::canonicalize(found).is_ok_and(|f| f == exe)
        })
    });
    if on_path {
        return "gitcraft".to_owned();
    }
    let shown = exe.display().to_string();
    if shown.contains(' ') {
        format!("\"{shown}\"")
    } else {
        shown
    }
}

/// The commands that add the server to Claude Code and to Codex, starting `program`.
pub fn register_commands(program: &str) -> [(&'static str, String); 2] {
    [
        (
            "Claude Code",
            format!("claude mcp add --scope user gitcraft -- {program} mcp"),
        ),
        ("Codex", format!("codex mcp add gitcraft -- {program} mcp")),
    ]
}

const USAGE: &str = "\
gitcraft mcp [--repo <path>]

Speaks the Model Context Protocol over stdin and stdout, for coding agents:

  claude mcp add --scope user gitcraft -- gitcraft mcp
  codex mcp add gitcraft -- gitcraft mcp

It works on the git repository it is started in, or the one --repo names (a folder here, or
one in WSL or over SSH as gitcraft names it: ssh://host/path). Reading tools work on their
own; the tools that change the repository need a gitcraft window with it open.
";

/// How much longer the agent's end waits than the window's: the window's own answer (it took
/// too long) arrives, rather than the connection just ending.
const ANSWER_MARGIN: std::time::Duration = std::time::Duration::from_secs(30);

/// What `gitcraft mcp …` was asked for.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Serve(Option<PathBuf>),
    Help,
}

fn parse(args: &[OsString]) -> Result<Command, String> {
    let mut repo = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.to_str() {
            Some("--repo") => {
                let path = rest.next().ok_or("--repo needs a path")?;
                repo = Some(PathBuf::from(path));
            }
            Some("-h" | "--help") => return Ok(Command::Help),
            _ => return Err(format!("unknown argument {}", arg.to_string_lossy())),
        }
    }
    Ok(Command::Serve(repo))
}

/// Runs `gitcraft mcp` with the arguments after `mcp`; returns the exit code.
pub fn main(args: &[OsString]) -> i32 {
    match parse(args) {
        Ok(Command::Help) => {
            print!("{USAGE}");
            0
        }
        Ok(Command::Serve(repo)) => {
            // No window here to ask ssh's questions in: over SSH it signs in with keys only.
            crate::remote::askpass::no_window();
            match serve(repo) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("gitcraft mcp: {e}");
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("gitcraft mcp: {e}\n\n{USAGE}");
            2
        }
    }
}

/// The repository the server works on, named as the window names it, or why there is none.
pub(crate) fn repository(repo: Option<PathBuf>) -> Result<PathBuf, String> {
    let start = match repo {
        Some(path) => path,
        None => std::env::current_dir().map_err(|e| format!("no working folder: {e}"))?,
    };
    crate::app::repo_root(&start).map_err(|e| {
        format!(
            "{} is not in a git repository ({e}). Start `gitcraft mcp` in one, or pass --repo.",
            start.display()
        )
    })
}

fn serve(repo: Option<PathBuf>) -> Result<(), String> {
    use rmcp::ServiceExt;
    // Started without a repository, the server still answers, and says so on every call.
    let server = server::Gitcraft::new(repo, link::dir(), link::ANSWER_WITHIN + ANSWER_MARGIN);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("could not start: {e}"))?;
    runtime.block_on(async move {
        let running = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| format!("the client didn't start the session: {e}"))?;
        running
            .waiting()
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn agents_are_told_how_to_start_it() {
        let [claude, codex] = register_commands("gitcraft");
        assert_eq!(
            claude.1,
            "claude mcp add --scope user gitcraft -- gitcraft mcp"
        );
        assert_eq!(codex.1, "codex mcp add gitcraft -- gitcraft mcp");
        assert!(!program().is_empty());
    }

    #[test]
    fn the_command_line_takes_a_repository_or_asks_for_help() {
        assert_eq!(parse(&os(&[])), Ok(Command::Serve(None)));
        assert_eq!(
            parse(&os(&["--repo", "/r"])),
            Ok(Command::Serve(Some("/r".into())))
        );
        assert_eq!(parse(&os(&["--help"])), Ok(Command::Help));
        assert!(parse(&os(&["--repo"])).is_err());
        assert!(parse(&os(&["--stdio"])).is_err());
    }

    #[test]
    fn outside_a_repository_the_server_says_where_to_start() {
        let dir = std::env::temp_dir().join(format!("gitcraft-mcp-norepo-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let e = repository(Some(dir.clone()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(e.is_err_and(|e| e.contains("pass --repo")));
    }
}
