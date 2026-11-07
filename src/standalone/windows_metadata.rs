//! A recording-testable Windows metadata protocol using managed, shipped OS APIs only.
//! Real execution requires the independent static protocol review before Windows fixtures.
use base64::Engine;
use std::{path::Path, process::Command};

const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$PSModuleAutoloadingPreference = 'None'
try {
  $source = [Environment]::GetEnvironmentVariable('GITCRAFT_ACL_SOURCE')
  $target = [Environment]::GetEnvironmentVariable('GITCRAFT_ACL_TARGET')
  $snapshot = [Environment]::GetEnvironmentVariable('GITCRAFT_ACL_SNAPSHOT') -eq '1'
  if ([String]::IsNullOrEmpty($source) -or (-not $snapshot -and [String]::IsNullOrEmpty($target))) { throw 'Missing metadata paths' }
  Microsoft.PowerShell.Core\Import-Module ([IO.Path]::Combine($PSHOME, 'Modules\Microsoft.PowerShell.Management\Microsoft.PowerShell.Management.psd1')) -ErrorAction Stop
  Microsoft.PowerShell.Management\Get-Item -LiteralPath $source -Stream '*' -ErrorAction Stop | Microsoft.PowerShell.Core\ForEach-Object {
    if ($_.Stream -ne ':$DATA' -and $_.Stream -ne '$DATA' -and $_.Stream -ne '::$DATA') { throw 'Named data streams cannot be preserved with the bounded save protocol' }
  }
  $sections = [Security.AccessControl.AccessControlSections]::Access -bor [Security.AccessControl.AccessControlSections]::Owner -bor [Security.AccessControl.AccessControlSections]::Group
  $security = [IO.File]::GetAccessControl($source, $sections)
  $descriptor = $security.GetSecurityDescriptorSddlForm($sections)
  if ($descriptor.Length -gt 1048576) { throw 'Access metadata exceeds 1 MiB' }
  if ($snapshot) {
    [Console]::Out.WriteLine([Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($descriptor)))
    exit 0
  }
  $copy = [Security.AccessControl.FileSecurity]::new()
  $copy.SetSecurityDescriptorSddlForm($descriptor, $sections)
  [IO.File]::SetAccessControl($target, $copy)
  $after = [IO.File]::GetAccessControl($target, $sections).GetSecurityDescriptorSddlForm($sections)
  if ($descriptor -cne $after) { throw 'Exact access-rule preservation is unavailable' }
  [Console]::Out.WriteLine('gitcraft-acl-v1-ok')
  exit 0
} catch {
  [Console]::Error.WriteLine('Could not preserve bounded Windows access metadata: ' + $_.Exception.Message)
  exit 1
}
"#;

fn command(program: &Path, source: &Path, target: &Path) -> Command {
    let mut cmd = Command::new(program);
    let encoded = SCRIPT
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-EncodedCommand",
    ])
    .arg(base64::engine::general_purpose::STANDARD.encode(encoded))
    .env("GITCRAFT_ACL_SOURCE", source)
    .env("GITCRAFT_ACL_TARGET", target)
    .env_remove("GITCRAFT_ACL_SNAPSHOT")
    .env_remove("PSModulePath")
    .env_remove("PSExecutionPolicyPreference");
    cmd
}
#[cfg(windows)]
fn execute(source: &Path, target: Option<&Path>) -> Result<Vec<u8>, String> {
    let system = std::env::var_os("SystemRoot")
        .ok_or("SystemRoot is unavailable; cannot safely preserve output access rules")?;
    let program =
        std::path::PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    if !program.is_absolute() {
        return Err("SystemRoot must be an absolute native path".into());
    }
    let mut command = command(&program, source, target.unwrap_or(Path::new("")));
    if target.is_none() {
        command.env("GITCRAFT_ACL_SNAPSHOT", "1");
    }
    let out = crate::processes::run(
        command,
        crate::processes::Options {
            timeout: Some(std::time::Duration::from_secs(20)),
            output_limit: 2 * 1024 * 1024,
            stderr_limit: Some(4096),
            ..Default::default()
        },
    )?
    .complete()?;
    if !out.status.success() {
        return Err(format!(
            "Windows access metadata could not be preserved; original unchanged: {}",
            String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(4096)
                .collect::<String>()
        ));
    }
    Ok(out.stdout)
}
#[cfg(windows)]
pub(super) fn prepare(source: &Path, target: &Path) -> Result<(), String> {
    if String::from_utf8_lossy(&execute(source, Some(target))?).trim() != "gitcraft-acl-v1-ok" {
        return Err("Unexpected Windows metadata-copy response".into());
    }
    Ok(())
}
#[cfg(windows)]
pub(super) fn snapshot(source: &Path) -> Result<Vec<u8>, String> {
    let out = execute(source, None)?;
    let encoded = std::str::from_utf8(&out)
        .map_err(|_| "Invalid access descriptor response")?
        .trim();
    let descriptor = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| e.to_string())?;
    if descriptor.len() > 1024 * 1024 {
        return Err("Output access metadata exceeds 1 MiB".into());
    }
    Ok(descriptor)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_protocol_records_only_constant_script_and_literal_native_path_data()
    -> Result<(), String> {
        let source = Path::new("source ' $() & ü");
        let target = Path::new("target ' $() & ü");
        let cmd = command(Path::new("powershell-fixture"), source, target);
        let args = cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            &args[..4],
            [
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand"
            ]
        );
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&args[4])
            .map_err(|e| e.to_string())?;
        let units = bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>();
        assert_eq!(
            String::from_utf16(&units).map_err(|e| e.to_string())?,
            SCRIPT
        );
        assert!(!SCRIPT.contains("source ' $()"));
        assert!(!SCRIPT.contains("Invoke-Expression"));
        let env = cmd.get_envs().collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            env.get(std::ffi::OsStr::new("GITCRAFT_ACL_SOURCE")),
            Some(&Some(source.as_os_str()))
        );
        assert_eq!(
            env.get(std::ffi::OsStr::new("GITCRAFT_ACL_TARGET")),
            Some(&Some(target.as_os_str()))
        );
        assert_eq!(env.get(std::ffi::OsStr::new("PSModulePath")), Some(&None));
        assert_eq!(
            env.get(std::ffi::OsStr::new("GITCRAFT_ACL_SNAPSHOT")),
            Some(&None)
        );
        assert!(SCRIPT.contains("if ($snapshot)"));
        assert!(SCRIPT.find("if ($snapshot)") < SCRIPT.find("[IO.File]::SetAccessControl"));
        Ok(())
    }
}

// Native regression fixtures only: a literal single owned file, no shell or recursion.
#[cfg(test)]
pub(super) fn fixture_acl_command(program: &Path, path: &Path, grant: bool) -> Command {
    let mut command = Command::new(program);
    command.arg(path);
    if grant {
        command.args(["/grant:r", "*S-1-1-0:(R)"]);
    } else {
        command.args(["/remove:g", "*S-1-1-0"]);
    }
    command.arg("/Q");
    command
}
#[cfg(test)]
mod fixture_recording_tests {
    #[test]
    fn owned_acl_fixture_records_literal_single_file_arguments_without_recursion() {
        use std::path::Path;
        for grant in [false, true] {
            let command = super::fixture_acl_command(
                Path::new("icacls-fixture"),
                Path::new("owned ' $() & ü"),
                grant,
            );
            let args = command.get_args().collect::<Vec<_>>();
            assert_eq!(args[0], "owned ' $() & ü");
            assert_eq!(args[1], if grant { "/grant:r" } else { "/remove:g" });
            assert_eq!(args[2], if grant { "*S-1-1-0:(R)" } else { "*S-1-1-0" });
            assert_eq!(args[3], "/Q");
            assert_eq!(args.len(), 4);
        }
    }
}
