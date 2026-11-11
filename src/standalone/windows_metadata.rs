//! A recording-testable bounded, privilege-aware Windows security-descriptor protocol.
//! Real execution requires the independent static protocol review before Windows fixtures.
use base64::Engine;
use std::{path::Path, process::Command};

const SCRIPT: &str = include_str!("windows_metadata/policy.ps1");

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
    // Tests share the expensive PowerShell compiler/runtime resource. Queue before
    // starting the existing bounded child deadline; do not weaken native checks.
    #[cfg(test)]
    let _fixture_resource = fixture_resources::acquire();
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
    if descriptor.len() > 1024 * 1024 + 4 {
        return Err(crate::i18n::text("interaction-standalone-error-access-limit").into());
    }
    Ok(descriptor)
}
#[cfg(test)]
#[path = "windows_metadata/tests.rs"]
mod tests;

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
#[path = "windows_metadata/fixture_recording_tests.rs"]
mod fixture_recording_tests;

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
#[path = "windows_metadata/full_policy_fixture_recording_tests.rs"]
mod full_policy_fixture_recording_tests;

#[cfg(test)]
#[path = "windows_metadata/diagnostic_localization_tests.rs"]
mod diagnostic_localization_tests;

#[cfg(test)]
#[path = "windows_metadata/fixture_resources.rs"]
mod fixture_resources;
