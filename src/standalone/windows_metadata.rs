//! A bounded, privilege-aware Windows security-descriptor protocol, run in-process.
//!
//! Index staging takes about twenty policy snapshots per transaction. Each used to start
//! PowerShell and compile the policy type with Add-Type under a 20-second child bound;
//! loaded Windows runners exceeded that bound (#211). `gitcraft_os` now runs the same
//! protocol natively. The PowerShell helper stays as the reviewed reference, and the
//! Windows tests check that both give identical snapshots and copies.
#[cfg(test)]
use base64::Engine;
#[cfg(test)]
use std::{path::Path, process::Command};

#[cfg(test)]
#[path = "windows_metadata/script.rs"]
mod script;
#[cfg(test)]
use script::*;

#[cfg(windows)]
pub(super) fn prepare(source: &std::path::Path, target: &std::path::Path) -> Result<(), String> {
    gitcraft_os::copy_file_policy(source, target).map_err(display_policy_error)
}
#[cfg(windows)]
pub(super) fn snapshot(source: &std::path::Path) -> Result<Vec<u8>, String> {
    gitcraft_os::file_policy_snapshot(source).map_err(display_policy_error)
}

/// The same localized envelope the helper's failures are displayed in.
#[cfg(windows)]
fn display_policy_error(error: gitcraft_os::PolicyError) -> String {
    use gitcraft_os::PolicyError;
    let cause = match error {
        PolicyError::NamedStreams => {
            crate::i18n::text("interaction-standalone-helper-named-streams").to_owned()
        }
        PolicyError::Native(code) => crate::i18n::message(
            "interaction-standalone-helper-native-policy",
            &[("code", code.into())],
        ),
        PolicyError::StreamQuery(code) => {
            std::io::Error::from_raw_os_error(code as i32).to_string()
        }
        PolicyError::TooLarge => {
            crate::i18n::text("interaction-standalone-helper-descriptor-limit").to_owned()
        }
        PolicyError::Malformed => {
            crate::i18n::text("interaction-standalone-error-descriptor-response").to_owned()
        }
        PolicyError::Differs => {
            crate::i18n::text("interaction-standalone-helper-policy-differs").to_owned()
        }
        PolicyError::SourceChanged => {
            crate::i18n::text("interaction-standalone-helper-source-policy-changed").to_owned()
        }
    };
    crate::i18n::message(
        "interaction-standalone-error-windows-metadata",
        &[(
            "detail",
            crate::i18n::message(
                "interaction-standalone-helper-failure",
                &[("detail", cause.into())],
            )
            .into(),
        )],
    )
}

#[cfg(test)]
#[path = "windows_metadata/tests.rs"]
mod tests;

#[cfg(all(test, windows))]
#[path = "windows_metadata/native_equivalence_tests.rs"]
mod native_equivalence_tests;

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
