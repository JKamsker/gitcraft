//! Owned policy fixtures for the native protocol tests: literal single paths, no recursion.
use std::path::{Path, PathBuf};
use std::process::Command;

fn system_program(name: &str) -> Result<PathBuf, String> {
    let program = PathBuf::from(std::env::var_os("SystemRoot").ok_or("No SystemRoot")?)
        .join("System32")
        .join(name);
    if !program.is_absolute() {
        return Err("Fixture requires absolute SystemRoot".into());
    }
    Ok(program)
}

pub(super) fn run(mut command: Command) -> Result<(), String> {
    command.env_remove("PSModulePath");
    let out = crate::processes::run(
        command,
        crate::processes::Options {
            timeout: Some(std::time::Duration::from_secs(20)),
            output_limit: 1 << 20,
            stderr_limit: Some(4096),
            ..Default::default()
        },
    )?
    .complete()?;
    if !out.status.success() {
        return Err(format!(
            "Owned policy fixture failed: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

/// `icacls <path> <args...> /Q` on one owned path.
pub(super) fn icacls(path: &Path, args: &[&str]) -> Result<(), String> {
    let mut command = Command::new(system_program("icacls.exe")?);
    command.arg(path).args(args).arg("/Q");
    run(command)
}

/// A directory junction (no privilege needed), the reparse point the protocol refuses.
pub(super) fn junction(link: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let quoted = |path: &Path| {
        let path = path.display().to_string();
        // cmd.exe has no escape for a quote inside a quoted name; owned temp names have none.
        (!path.contains('"')).then(|| format!("\"{path}\""))
    };
    let (Some(link), Some(target)) = (quoted(link), quoted(target)) else {
        return Err("Junction fixture paths must not contain quotes".into());
    };
    let mut command = Command::new(system_program("cmd.exe")?);
    command.raw_arg(format!("/d /c mklink /J {link} {target}"));
    run(command).map_err(|e| format!("mklink /J {link} {target}: {e}"))
}

pub(super) fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

pub(super) fn native(path: &Path, scope: u32) -> Result<Vec<u8>, String> {
    gitcraft_os::file_policy_snapshot_in_scope(path, scope).map_err(|e| format!("{e:?}"))
}

pub(super) fn scope_of(snapshot: &[u8]) -> Result<u32, String> {
    snapshot
        .first_chunk::<4>()
        .map(|b| u32::from_le_bytes(*b))
        .ok_or_else(|| "empty snapshot".into())
}
