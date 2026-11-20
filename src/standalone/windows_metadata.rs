//! A bounded, privilege-aware Windows security-descriptor protocol, run in-process.
//!
//! Index staging takes about twenty policy snapshots per transaction. Each used to start
//! PowerShell and compile the policy type with Add-Type under a 20-second child bound;
//! loaded Windows runners exceeded that bound (#211). `gitcraft_os` now runs the protocol
//! natively, through handles that never follow the final component (name-surrogate reparse
//! points such as links and junctions are refused; Cloud Files placeholders are not), enables
//! a held SeSecurityPrivilege for the call, and keeps the integrity label when the audit SACL
//! needs a privilege the caller lacks. The PowerShell helper stays as the
//! reviewed reference; the Windows tests check both against each other.
//!
//! These are synchronous file-system calls with no process deadline. Every caller runs on a
//! background worker (staging mutations via `spawn_mutation`, standalone loads and saves via
//! `standalone::submit`), never on a UI thread, and inherits the OS I/O timeouts.
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
    fn text(id: &'static str) -> String {
        crate::i18n::text(id).to_owned()
    }
    let coded = |id: &'static str, code: u32| crate::i18n::message(id, &[("code", code.into())]);
    let cause = match error {
        PolicyError::NamedStreams => text("interaction-standalone-helper-named-streams"),
        PolicyError::ReparsePoint => text("interaction-standalone-policy-reparse-point"),
        PolicyError::Native(code) => native_cause(code, coded),
        PolicyError::StreamQuery(code) => coded("interaction-standalone-policy-streams", code),
        PolicyError::TooLarge => text("interaction-standalone-helper-descriptor-limit"),
        PolicyError::Malformed => text("interaction-standalone-error-descriptor-response"),
        PolicyError::Differs => text("interaction-standalone-helper-policy-differs"),
        PolicyError::SourceChanged => text("interaction-standalone-helper-source-policy-changed"),
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

/// Only a missing privilege gets the SeSecurityPrivilege hint.
#[cfg(windows)]
fn native_cause(code: u32, coded: impl Fn(&'static str, u32) -> String) -> String {
    const ACCESS_DENIED: u32 = 5;
    const SHARING_VIOLATION: u32 = 32;
    const INVALID_OWNER: u32 = 1307;
    const PRIVILEGE_NOT_HELD: u32 = 1314;
    let id = match code {
        PRIVILEGE_NOT_HELD => "interaction-standalone-helper-native-policy",
        ACCESS_DENIED => "interaction-standalone-policy-access-denied",
        SHARING_VIOLATION => "interaction-standalone-policy-in-use",
        INVALID_OWNER => "interaction-standalone-policy-invalid-owner",
        _ => "interaction-standalone-policy-native",
    };
    coded(id, code)
}

#[cfg(test)]
#[path = "windows_metadata/tests.rs"]
mod tests;

#[cfg(all(test, windows))]
#[path = "windows_metadata/native_equivalence_tests.rs"]
mod native_equivalence_tests;
#[cfg(all(test, windows))]
#[path = "windows_metadata/native_fixtures.rs"]
mod native_fixtures;
#[cfg(all(test, windows))]
#[path = "windows_metadata/native_refusal_tests.rs"]
mod native_refusal_tests;

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
