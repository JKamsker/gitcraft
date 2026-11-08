//! A recording-testable bounded full native Windows security-descriptor protocol.
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
  # Request every applicable native component explicitly. Managed FileSecurity's
  # Access/Owner/Group subset cannot establish mandatory-label or SACL equality.
  Microsoft.PowerShell.Core\Import-Module ([IO.Path]::Combine($PSHOME, 'Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1')) -ErrorAction Stop
  Microsoft.PowerShell.Utility\Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Security.AccessControl;
public static class GitcraftNativePolicy {
  // OWNER|GROUP|DACL|SACL|LABEL|ATTRIBUTE|SCOPE|PROCESS_TRUST_LABEL|ACCESS_FILTER.
  public const uint AllPolicy = 0x000001ff;
  [DllImport("advapi32.dll", CharSet=CharSet.Unicode)]
  static extern uint GetNamedSecurityInfo(string path, uint type, uint info, out IntPtr owner, out IntPtr group, out IntPtr dacl, out IntPtr sacl, out IntPtr descriptor);
  [DllImport("advapi32.dll", CharSet=CharSet.Unicode)]
  static extern uint SetNamedSecurityInfo(string path, uint type, uint info, IntPtr owner, IntPtr group, IntPtr dacl, IntPtr sacl);
  [DllImport("advapi32.dll")] static extern uint GetSecurityDescriptorLength(IntPtr descriptor);
  [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr memory);
  static void Check(uint error) {
    if (error != 0) throw new InvalidOperationException("Full Windows policy unavailable (native error " + error + "); SACL/audit access may require SeSecurityPrivilege. Original must remain unchanged.");
  }
  static byte[] Canonical(IntPtr descriptor) {
    uint length = GetSecurityDescriptorLength(descriptor);
    if (length == 0 || length > 1048576) throw new InvalidOperationException("Full native descriptor exceeds 1 MiB");
    byte[] raw = new byte[length]; Marshal.Copy(descriptor,raw,0,(int)length);
    RawSecurityDescriptor parsed = new RawSecurityDescriptor(raw,0);
    byte[] normalized = new byte[parsed.BinaryLength]; parsed.GetBinaryForm(normalized,0);
    return normalized;
  }
  public static byte[] Read(string path) {
    IntPtr owner,group,dacl,sacl,descriptor=IntPtr.Zero;
    try { Check(GetNamedSecurityInfo(path,1,AllPolicy,out owner,out group,out dacl,out sacl,out descriptor)); return Canonical(descriptor); }
    finally { if (descriptor != IntPtr.Zero) LocalFree(descriptor); }
  }
  public static void Copy(string source,string target) {
    IntPtr owner,group,dacl,sacl,descriptor=IntPtr.Zero;
    try {
      Check(GetNamedSecurityInfo(source,1,AllPolicy,out owner,out group,out dacl,out sacl,out descriptor));
      byte[] before=Canonical(descriptor);
      RawSecurityDescriptor parsed=new RawSecurityDescriptor(before,0);
      uint protection = (parsed.ControlFlags & ControlFlags.DiscretionaryAclProtected) != 0 ? 0x80000000u : 0x20000000u;
      protection |= (parsed.ControlFlags & ControlFlags.SystemAclProtected) != 0 ? 0x40000000u : 0x10000000u;
      Check(SetNamedSecurityInfo(target,1,AllPolicy|protection,owner,group,dacl,sacl));
      if (!Convert.ToBase64String(before).Equals(Convert.ToBase64String(Read(target)),StringComparison.Ordinal)) throw new InvalidOperationException("Full native policy differs after copy");
      if (!Convert.ToBase64String(before).Equals(Convert.ToBase64String(Read(source)),StringComparison.Ordinal)) throw new InvalidOperationException("Source policy changed during copy");
    } finally { if (descriptor != IntPtr.Zero) LocalFree(descriptor); }
  }
}
'@ -ErrorAction Stop
  $descriptor = [GitcraftNativePolicy]::Read($source)
  if ($snapshot) {
    [Console]::Out.WriteLine([Convert]::ToBase64String($descriptor))
    exit 0
  }
  [GitcraftNativePolicy]::Copy($source, $target)
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
    let system = std::env::var_os("SystemRoot").ok_or(crate::i18n::text(
        "interaction-standalone-error-system-root-unavailable",
    ))?;
    let program =
        std::path::PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    if !program.is_absolute() {
        return Err(crate::i18n::text("interaction-standalone-error-system-root-absolute").into());
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
        return Err(display_failure(
            &String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(4096)
                .collect::<String>(),
        ));
    }
    Ok(out.stdout)
}
// Exact application-authored SCRIPT diagnostics. These are display classifiers only;
// the script, native protocol and unknown PowerShell/OS details stay unchanged.
const HELPER_FAILURE: &str = "Could not preserve bounded Windows access metadata: ";
const HELPER_DIAGNOSTICS: &[(&str, &str)] = &[
    (
        "Missing metadata paths",
        "interaction-standalone-helper-missing-paths",
    ),
    (
        "Named data streams cannot be preserved with the bounded save protocol",
        "interaction-standalone-helper-named-streams",
    ),
    (
        "Full native descriptor exceeds 1 MiB",
        "interaction-standalone-helper-descriptor-limit",
    ),
    (
        "Full native policy differs after copy",
        "interaction-standalone-helper-policy-differs",
    ),
    (
        "Source policy changed during copy",
        "interaction-standalone-helper-source-policy-changed",
    ),
];
const NATIVE_POLICY_PREFIX: &str = "Full Windows policy unavailable (native error ";
const NATIVE_POLICY_SUFFIX: &str =
    "); SACL/audit access may require SeSecurityPrivilege. Original must remain unchanged.";

/// Formats only this helper's exact failure envelope; arbitrary stderr remains opaque.
fn display_failure(detail: &str) -> String {
    let localized = if let Some(cause) = detail.strip_prefix(HELPER_FAILURE) {
        // Console.Error.WriteLine adds a line ending; preserve it without trimming
        // arbitrary whitespace from the exception's diagnostic payload.
        let (cause, ending) = if let Some(cause) = cause.strip_suffix("\r\n") {
            (cause, "\r\n")
        } else if let Some(cause) = cause.strip_suffix('\n') {
            (cause, "\n")
        } else {
            (cause, "")
        };
        let mut rendered = crate::i18n::message(
            "interaction-standalone-helper-failure",
            &[("detail", helper_cause(cause).into())],
        );
        rendered.push_str(ending);
        rendered
    } else {
        detail.to_owned()
    };
    crate::i18n::message(
        "interaction-standalone-error-windows-metadata",
        &[("detail", localized.into())],
    )
}

fn helper_cause(detail: &str) -> String {
    // PowerShell can wrap the two C# entry points' exceptions. Recognize only
    // their exact method/arity frames; retain those external frame bytes as data.
    let (cause, frame) = [
        "Exception calling \"Read\" with \"1\" argument(s): \"",
        "Exception calling \"Copy\" with \"2\" argument(s): \"",
    ]
    .into_iter()
    .find_map(|prefix| {
        detail
            .strip_prefix(prefix)?
            .strip_suffix('"')
            .map(|cause| (cause, Some(prefix)))
    })
    .unwrap_or((detail, None));
    let translated = if let Some((_, id)) = HELPER_DIAGNOSTICS.iter().find(|(raw, _)| *raw == cause)
    {
        Some(crate::i18n::text(id).to_owned())
    } else {
        cause
            .strip_prefix(NATIVE_POLICY_PREFIX)
            .and_then(|code| code.strip_suffix(NATIVE_POLICY_SUFFIX))
            .and_then(|raw| {
                // SCRIPT's Check(uint) only throws for nonzero canonical decimal
                // codes. Reject whitespace, signs, leading zeros and overflow.
                let code = raw.parse::<u32>().ok()?;
                (code != 0 && code.to_string() == raw).then_some(code)
            })
            .map(|code| {
                crate::i18n::message(
                    "interaction-standalone-helper-native-policy",
                    &[("code", code.into())],
                )
            })
    };
    match (translated, frame) {
        (Some(translated), Some(prefix)) => format!("{prefix}{translated}\""),
        (Some(translated), None) => translated,
        (None, _) => detail.to_owned(),
    }
}

#[cfg(windows)]
pub(super) fn prepare(source: &Path, target: &Path) -> Result<(), String> {
    if String::from_utf8_lossy(&execute(source, Some(target))?).trim() != "gitcraft-acl-v1-ok" {
        return Err(crate::i18n::text("interaction-standalone-error-windows-copy-response").into());
    }
    Ok(())
}
#[cfg(windows)]
pub(super) fn snapshot(source: &Path) -> Result<Vec<u8>, String> {
    let out = execute(source, None)?;
    let encoded = std::str::from_utf8(&out)
        .map_err(|_| crate::i18n::text("interaction-standalone-error-descriptor-response"))?
        .trim();
    let descriptor = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| e.to_string())?;
    if descriptor.len() > 1024 * 1024 {
        return Err(crate::i18n::text("interaction-standalone-error-access-limit").into());
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
            .as_chunks::<2>()
            .0
            .iter()
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
        assert!(SCRIPT.contains("AllPolicy = 0x000001ff"));
        assert!(SCRIPT.contains("SACL|LABEL|ATTRIBUTE|SCOPE|PROCESS_TRUST_LABEL|ACCESS_FILTER"));
        assert!(SCRIPT.contains("GetNamedSecurityInfo(path,1,AllPolicy"));
        assert!(SCRIPT.contains("SetNamedSecurityInfo(target,1,AllPolicy|protection"));
        assert!(SCRIPT.contains("SeSecurityPrivilege"));
        assert!(!SCRIPT.contains("[IO.File]::GetAccessControl"));
        assert!(SCRIPT.contains("length > 1048576"));
        assert!(SCRIPT.contains("if ($snapshot)"));
        assert!(SCRIPT.find("if ($snapshot)") < SCRIPT.rfind("[GitcraftNativePolicy]::Copy"));
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

#[cfg(test)]
const AUDIT_FIXTURE: &str = r#"
$ErrorActionPreference='Stop'
$path=[Environment]::GetEnvironmentVariable('GITCRAFT_OWNED_POLICY_PATH')
$sections=[Security.AccessControl.AccessControlSections]::All
$acl=[IO.File]::GetAccessControl($path,$sections)
$sid=[Security.Principal.SecurityIdentifier]::new('S-1-1-0')
$rule=[Security.AccessControl.FileSystemAuditRule]::new($sid,[Security.AccessControl.FileSystemRights]::ReadData,[Security.AccessControl.AuditFlags]::Success)
$acl.AddAuditRule($rule)
[IO.File]::SetAccessControl($path,$acl)
"#;
#[cfg(test)]
pub(super) fn fixture_policy_command(program: &Path, path: &Path, audit: bool) -> Command {
    let mut command = Command::new(program);
    if audit {
        let bytes = AUDIT_FIXTURE
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
            ])
            .arg(base64::engine::general_purpose::STANDARD.encode(bytes))
            .env("GITCRAFT_OWNED_POLICY_PATH", path);
    } else {
        command.arg(path).args(["/setintegritylevel", "L", "/Q"]);
    }
    command
}
#[cfg(test)]
mod full_policy_fixture_recording_tests {
    use super::*;
    #[test]
    fn label_and_audit_fixtures_record_only_owned_literal_paths() -> Result<(), String> {
        let path = Path::new("owned ' $() & ü");
        let label = fixture_policy_command(Path::new("icacls-fixture"), path, false);
        assert_eq!(
            label.get_args().collect::<Vec<_>>(),
            [
                path.as_os_str(),
                std::ffi::OsStr::new("/setintegritylevel"),
                std::ffi::OsStr::new("L"),
                std::ffi::OsStr::new("/Q")
            ]
        );
        let audit = fixture_policy_command(Path::new("powershell-fixture"), path, true);
        let args = audit.get_args().collect::<Vec<_>>();
        assert_eq!(args.len(), 5);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(args[4].to_string_lossy().as_bytes())
            .map_err(|e| e.to_string())?;
        let units = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .collect::<Vec<_>>();
        assert_eq!(
            String::from_utf16(&units).map_err(|e| e.to_string())?,
            AUDIT_FIXTURE
        );
        assert!(AUDIT_FIXTURE.contains("AccessControlSections]::All"));
        assert!(AUDIT_FIXTURE.contains("FileSystemAuditRule"));
        assert!(!AUDIT_FIXTURE.contains("owned '"));
        assert_eq!(
            audit
                .get_envs()
                .find(|(k, _)| *k == "GITCRAFT_OWNED_POLICY_PATH")
                .and_then(|(_, v)| v),
            Some(path.as_os_str())
        );
        Ok(())
    }
}

#[cfg(test)]
mod diagnostic_localization_tests {
    use super::*;

    #[test]
    fn actual_failure_envelope_localizes_source_bound_named_stream_and_native_policy_errors()
    -> Result<(), String> {
        // These exact source bindings stop the finite classifier drifting from the
        // unchanged helper. This test never constructs or executes a native command.
        assert!(SCRIPT.contains(&format!("'{HELPER_FAILURE}' + $_.Exception.Message")));
        for (raw, _) in HELPER_DIAGNOSTICS {
            assert!(SCRIPT.contains(&format!("'{raw}'")) || SCRIPT.contains(&format!("\"{raw}\"")));
        }
        assert!(SCRIPT.contains(&format!(
            "\"{NATIVE_POLICY_PREFIX}\" + error + \"{NATIVE_POLICY_SUFFIX}\""
        )));
        assert!(SCRIPT.contains("static void Check(uint error)"));
        assert!(SCRIPT.contains("if (error != 0)"));
        assert!(SCRIPT.contains("public static byte[] Read(string path)"));
        assert!(SCRIPT.contains("public static void Copy(string source,string target)"));
        let cases = [
            ("Missing metadata paths", "Metadatenpfade fehlen"),
            (
                "Named data streams cannot be preserved with the bounded save protocol",
                "Benannte Datenströme können mit dem begrenzten Speicherprotokoll nicht erhalten werden",
            ),
            (
                "Full native descriptor exceeds 1 MiB",
                "Der vollständige native Sicherheitsdeskriptor überschreitet 1 MiB",
            ),
            (
                "Full native policy differs after copy",
                "Die vollständige native Zugriffsrichtlinie weicht nach dem Kopieren ab",
            ),
            (
                "Source policy changed during copy",
                "Die Zugriffsrichtlinie der Quelle hat sich beim Kopieren geändert",
            ),
        ];
        for (locale, outer, prefix) in [
            (
                "en-US",
                "Windows access metadata could not be preserved; original unchanged: ",
                HELPER_FAILURE,
            ),
            (
                "de",
                "Die Windows-Zugriffsmetadaten konnten nicht erhalten werden; das Original ist unverändert: ",
                "Die begrenzten Windows-Zugriffsmetadaten konnten nicht erhalten werden: ",
            ),
        ] {
            crate::i18n::set_language(locale);
            for (en, de) in cases {
                for ending in ["", "\n", "\r\n"] {
                    let input = format!("{HELPER_FAILURE}{en}{ending}");
                    let expected = if locale == "de" { de } else { en };
                    assert_eq!(
                        display_failure(&input),
                        format!("{outer}{prefix}{expected}{ending}")
                    );
                }
            }
            // Error codes identify native failures; they must never acquire locale
            // grouping, a sign, or another spelling, even at uint's maximum value.
            for code in [5u32, 1234, u32::MAX] {
                let en = format!("{NATIVE_POLICY_PREFIX}{code}{NATIVE_POLICY_SUFFIX}");
                let translated = if locale == "de" {
                    format!(
                        "Die vollständige Windows-Zugriffsrichtlinie ist nicht verfügbar (nativer Fehler {code}); der Zugriff auf SACL/Auditdaten erfordert möglicherweise SeSecurityPrivilege. Das Original muss unverändert bleiben."
                    )
                } else {
                    en.clone()
                };
                for frame in [
                    "",
                    "Exception calling \"Read\" with \"1\" argument(s): \"",
                    "Exception calling \"Copy\" with \"2\" argument(s): \"",
                ] {
                    let close = if frame.is_empty() { "" } else { "\"" };
                    let input = format!("{HELPER_FAILURE}{frame}{en}{close}\r\n");
                    assert_eq!(
                        display_failure(&input),
                        format!("{outer}{prefix}{frame}{translated}{close}\r\n")
                    );
                }
            }
        }
        crate::i18n::set_language("en-US");
        Ok(())
    }

    #[test]
    fn actual_failure_envelope_preserves_unknown_details_and_rejects_near_matches() {
        let unknown = [
            "opaque OS error: ' $() & ü\n{}\r\n",
            "Named data streams cannot be preserved with the bounded save protocol (external detail)",
            "named data streams cannot be preserved with the bounded save protocol",
            "Missing metadata paths ",
            "before Missing metadata paths after",
            "Full native policy differs after copy\nextra detail",
            "Exception calling \"Other\" with \"1\" argument(s): \"Full native policy differs after copy\"",
            "Exception calling \"Read\" with \"2\" argument(s): \"Full native policy differs after copy\"",
            "Exception calling \"Read\" with \"1\" argument(s): \"Full native policy differs after copy\" extra",
            "Exception calling \"Copy\" with \"2\" argument(s): \"opaque detail: ' $() & ü\n{}\"",
            "gitcraft-acl-v1-ok",
        ];
        for (locale, outer, prefix) in [
            (
                "en-US",
                "Windows access metadata could not be preserved; original unchanged: ",
                HELPER_FAILURE,
            ),
            (
                "de",
                "Die Windows-Zugriffsmetadaten konnten nicht erhalten werden; das Original ist unverändert: ",
                "Die begrenzten Windows-Zugriffsmetadaten konnten nicht erhalten werden: ",
            ),
        ] {
            crate::i18n::set_language(locale);
            for raw in unknown {
                // Unframed stderr cannot be attributed to this helper's catch.
                assert_eq!(display_failure(raw), format!("{outer}{raw}"));
                // The known catch sentence translates, while all unknown cause bytes
                // (including whitespace, line endings, braces and shell text) survive.
                let input = format!("{HELPER_FAILURE}{raw}");
                assert_eq!(display_failure(&input), format!("{outer}{prefix}{raw}"));
            }
            for raw in [
                "0",
                "00",
                "05",
                "+5",
                "-5",
                " 5",
                "5 ",
                "５",
                "5.0",
                "5e0",
                "4294967296",
                "5) spoof",
            ] {
                let cause = format!("{NATIVE_POLICY_PREFIX}{raw}{NATIVE_POLICY_SUFFIX}");
                assert_eq!(
                    display_failure(&format!("{HELPER_FAILURE}{cause}")),
                    format!("{outer}{prefix}{cause}")
                );
            }
            for raw in [
                format!(
                    "could not preserve bounded Windows access metadata: {}",
                    unknown[1]
                ),
                format!(" {HELPER_FAILURE}Missing metadata paths"),
                format!("before {HELPER_FAILURE}Missing metadata paths"),
                format!("{HELPER_FAILURE}Missing metadata paths\r"),
            ] {
                let expected = if let Some(rest) = raw.strip_prefix(HELPER_FAILURE) {
                    format!("{outer}{prefix}{rest}")
                } else {
                    format!("{outer}{raw}")
                };
                assert_eq!(display_failure(&raw), expected);
            }
        }
        crate::i18n::set_language("en-US");
    }
}
