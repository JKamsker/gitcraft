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
  if ([String]::IsNullOrEmpty($source) -or [String]::IsNullOrEmpty($target)) { throw 'Missing metadata paths' }
  Microsoft.PowerShell.Core\Import-Module ([IO.Path]::Combine($PSHOME, 'Modules\Microsoft.PowerShell.Management\Microsoft.PowerShell.Management.psd1')) -ErrorAction Stop
  Microsoft.PowerShell.Management\Get-Item -LiteralPath $source -Stream '*' -ErrorAction Stop | Microsoft.PowerShell.Core\ForEach-Object {
    if ($_.Stream -ne ':$DATA' -and $_.Stream -ne '$DATA' -and $_.Stream -ne '::$DATA') { throw 'Named data streams cannot be preserved with the bounded save protocol' }
  }
  $sections = [Security.AccessControl.AccessControlSections]::Access -bor [Security.AccessControl.AccessControlSections]::Owner -bor [Security.AccessControl.AccessControlSections]::Group
  $security = [IO.File]::GetAccessControl($source, $sections)
  $descriptor = $security.GetSecurityDescriptorSddlForm($sections)
  if ($descriptor.Length -gt 1048576) { throw 'Access metadata exceeds 1 MiB' }
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
    .env_remove("PSModulePath")
    .env_remove("PSExecutionPolicyPreference");
    cmd
}
#[cfg(windows)]
pub(super) fn prepare(source: &Path, target: &Path) -> Result<(), String> {
    let system = std::env::var_os("SystemRoot")
        .ok_or("SystemRoot is unavailable; cannot safely preserve output access rules")?;
    let program =
        std::path::PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    if !program.is_absolute() {
        return Err("SystemRoot must be an absolute native path".into());
    }
    let out = crate::bounded_process::run(
        command(&program, source, target),
        &|| true,
        std::time::Duration::from_secs(20),
    )?;
    if !out.status.success() || String::from_utf8_lossy(&out.stdout).trim() != "gitcraft-acl-v1-ok"
    {
        return Err(format!(
            "Windows access metadata could not be preserved; original unchanged: {}",
            String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(4096)
                .collect::<String>()
        ));
    }
    Ok(())
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
        Ok(())
    }
}
