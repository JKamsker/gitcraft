use std::path::{Path, PathBuf};

use super::*;

#[test]
fn found_tools_are_read() {
    assert_eq!(
        parse("copilot\r\nclaude\n"),
        [AiTool::Claude, AiTool::Copilot]
    );
    assert!(parse("").is_empty());
    assert!(parse("claudex codexy").is_empty());
}

/// Its folder isn't where the look runs: one that is gone doesn't matter. The look itself is
/// one that needs no tool of this machine's, so only that is tested.
#[test]
fn tools_here_are_looked_for_outside_the_repository() -> Result<(), String> {
    let gone = Path::new("/nonexistent/gitcraft-ai-detect");
    let folder = look_from(gone);
    assert_eq!(folder.as_ref(), std::env::temp_dir());
    let echo = CommandLine::Script {
        sh: "echo claude".to_owned(),
        cmd: "echo claude".to_owned(),
    };
    assert_eq!(look(&folder, &echo)?, [AiTool::Claude]);
    Ok(())
}

/// On Windows, `where` with `$PATH:` and cmd's `NoDefaultCurrentDirectoryInExePath` keep
/// a repository's own `claude.cmd` from being found or run.
#[test]
fn the_windows_look_searches_only_the_path() {
    assert_eq!(CMD.matches("where $PATH:").count(), 4);
    assert!(!CMD.contains("where claude"));
}

/// Neither look runs a tool: `gh copilot` prints its help without the Copilot CLI, starts gh's
/// telemetry, and on first real use downloads the Copilot CLI.
#[test]
fn the_looks_run_no_tool() {
    for look in [SH, CMD] {
        assert!(!look.contains("gh copilot"), "{look}");
        assert!(!look.contains("--help"), "{look}");
    }
}

/// A machine of fake tools: a PATH folder and a home whose gh data folder can hold the
/// Copilot CLI gh downloads. The fakes are found, never run.
struct Machine {
    root: PathBuf,
}

impl Machine {
    fn new(name: &str) -> Result<Self, String> {
        let root = std::env::temp_dir().join(format!(
            "gitcraft-ai-detect-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("bin")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.join("home")).map_err(|e| e.to_string())?;
        Ok(Self { root })
    }

    fn bin(&self) -> PathBuf {
        self.root.join("bin")
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    /// A program on the PATH that succeeds whatever it is asked (as `gh copilot --help` does).
    fn program(&self, name: &str) -> Result<(), String> {
        let file = if cfg!(windows) {
            format!("{name}.cmd")
        } else {
            name.to_owned()
        };
        executable(&self.bin().join(file))
    }

    /// The Copilot CLI where `gh copilot` downloads it.
    fn downloaded_copilot(&self) -> Result<(), String> {
        let (folder, file) = if cfg!(windows) {
            (self.home().join(r"local\GitHub CLI\copilot"), "copilot.exe")
        } else {
            (self.home().join(".local/share/gh/copilot"), "copilot")
        };
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        executable(&folder.join(file))
    }

    /// The look, with only this machine's PATH and home: a real tool here isn't found.
    #[cfg(unix)]
    fn look(&self) -> Result<Vec<AiTool>, String> {
        let script = CommandLine::Script {
            sh: format!(
                "PATH='{}'\nHOME='{}'\nunset XDG_DATA_HOME\n{SH}",
                self.bin().display(),
                self.home().display()
            ),
            cmd: String::new(),
        };
        look(&self.root, &script)
    }

    /// The look, with only this machine's PATH (and the system's, for `where`) and home.
    /// `%LOCALAPPDATA%` is read when cmd reads the line, so it is set on cmd itself.
    #[cfg(windows)]
    fn look(&self) -> Result<Vec<AiTool>, String> {
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
        let mut cmd = run::shell_here(&look_script());
        cmd.current_dir(&self.root)
            .env("PATH", format!("{};{system}\\System32", self.bin().display()))
            .env("LOCALAPPDATA", self.home().join("local"))
            .env_remove("XDG_DATA_HOME");
        let options = crate::processes::Options {
            background: true,
            timeout: Some(LOOK_FOR),
            ..Default::default()
        };
        let out = crate::processes::run_checked(cmd, options).map_err(|e| e.message())?;
        if let Some(stopped) = out.stopped {
            return Err(stopped);
        }
        Ok(parse(&String::from_utf8_lossy(&out.stdout)))
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A program that succeeds whatever it is asked.
fn executable(path: &Path) -> Result<(), String> {
    let body = if cfg!(windows) {
        "@exit /b 0\r\n"
    } else {
        "#!/bin/sh\nexit 0\n"
    };
    std::fs::write(path, body).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// `gh` alone isn't Copilot (its `copilot --help` answers without the Copilot CLI); with the
/// copy gh downloaded, it is. A real `codex` of this machine's isn't found.
#[test]
fn copilot_needs_the_copilot_cli_beside_gh() -> Result<(), String> {
    let machine = Machine::new("downloaded")?;
    machine.program("claude")?;
    machine.program("gh")?;
    assert_eq!(machine.look()?, [AiTool::Claude]);
    machine.downloaded_copilot()?;
    assert_eq!(machine.look()?, [AiTool::Claude, AiTool::Copilot]);
    Ok(())
}

/// The Copilot CLI on the PATH counts, but only with `gh` to run it through.
#[test]
fn copilot_on_the_path_needs_gh() -> Result<(), String> {
    let machine = Machine::new("on-path")?;
    machine.program("copilot")?;
    assert!(machine.look()?.is_empty());
    machine.program("gh")?;
    assert_eq!(machine.look()?, [AiTool::Copilot]);
    Ok(())
}
