//! The environment baseline lives in an owned child, never in the multithreaded parent.
use super::{Scratch, command, options, stdout, wait};
use serde::{Deserialize, Serialize};
use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

const ARG0: &str = "gitcraft-owned-environment-snapshot";
const DRIVER_ARG0: &str = "gitcraft-owned-environment-driver";
const INHERIT: &str = "GITCRAFT_FINITE_ENV_INHERIT";
const SET: &str = "GITCRAFT_FINITE_ENV_SET";
const REMOVE: &str = "GITCRAFT_FINITE_ENV_REMOVE";
const EMPTY: &str = "GITCRAFT_FINITE_ENV_EMPTY";
const NON_UTF8: &[u8] = b"GITCRAFT_FINITE_ENV_NON_UTF8_\xff";
const EXECUTABLE: &str = "gitcraft-owned-env-helper";
const CASES: [&str; 7] = [
    "inherit",
    "set",
    "remove",
    "clear",
    "empty-clear",
    "path-inherit",
    "path-explicit",
];
const DONE: &[u8] = b"opaque-command-environment:7-exact-receipts\n";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Receipt {
    arg0: Vec<u8>,
    inherit: Option<Vec<u8>>,
    set: Option<Vec<u8>>,
    remove: Option<Vec<u8>>,
    empty: Option<Vec<u8>>,
    non_utf8: Option<Vec<u8>>,
    path: Option<Vec<u8>>,
    empty_environment: bool,
    /// Only the empty-clear case records all key names; never capture their values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unexpected_environment_keys: Option<Vec<Vec<u8>>>,
}

pub(super) fn is_snapshot() -> bool {
    std::env::args_os().next().is_some_and(|word| word == ARG0)
}

pub(super) fn is_driver() -> bool {
    std::env::args_os()
        .next()
        .is_some_and(|word| word == DRIVER_ARG0)
}

pub(super) fn snapshot() -> io::Result<()> {
    let unexpected_environment_keys =
        if std::env::current_dir()?.file_name() == Some(OsStr::new("empty-clear")) {
            let mut keys: Vec<_> = std::env::vars_os().map(|(key, _)| key.into_vec()).collect();
            keys.sort();
            Some(keys)
        } else {
            None
        };
    let value = |key: &OsStr| std::env::var_os(key).map(OsString::into_vec);
    let receipt = Receipt {
        arg0: std::env::args_os()
            .next()
            .ok_or_else(|| io::Error::other("missing arg0"))?
            .into_vec(),
        inherit: value(OsStr::new(INHERIT)),
        set: value(OsStr::new(SET)),
        remove: value(OsStr::new(REMOVE)),
        empty: value(OsStr::new(EMPTY)),
        non_utf8: value(OsStr::from_bytes(NON_UTF8)),
        path: value(OsStr::new("PATH")),
        empty_environment: std::env::vars_os().next().is_none(),
        unexpected_environment_keys,
    };
    let mut bytes = serde_json::to_vec(&receipt)?;
    bytes.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open("environment-receipt.json")?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    io::stdout().write_all(&bytes)?;
    io::stdout().flush()
}

fn expected(case: &str, directory: &Path) -> Receipt {
    let cleared = matches!(case, "clear" | "empty-clear" | "path-explicit");
    // CoreFoundation initializes this linked macOS test executable after std launch:
    // CFRuntime.c -> __CFStringGetUserDefaultEncoding -> setenv in CFStringEncodings.c.
    // Parent diagnostic 1043c8a7 observed exactly this singleton. Keep the raw emptiness
    // field truthful and require every actual key to match; do not filter any keys.
    let macos_startup = cfg!(target_os = "macos");
    let mut receipt = Receipt {
        arg0: ARG0.as_bytes().to_vec(),
        inherit: (!cleared).then(|| b"inherit-me".to_vec()),
        set: (!cleared).then(|| b"baseline-set".to_vec()),
        remove: (!cleared).then(|| b"remove-me".to_vec()),
        empty: None,
        non_utf8: (!cleared).then(|| b"baseline-\xfe".to_vec()),
        path: (!cleared).then(|| directory.as_os_str().as_bytes().to_vec()),
        empty_environment: case == "empty-clear" && !macos_startup,
        unexpected_environment_keys: (case == "empty-clear").then(|| {
            if macos_startup {
                vec![b"__CF_USER_TEXT_ENCODING".to_vec()]
            } else {
                Vec::new()
            }
        }),
    };
    match case {
        "set" => {
            receipt.set = Some(b"changed".to_vec());
            receipt.empty = Some(Vec::new());
            receipt.non_utf8 = Some(b"changed-\xfd".to_vec());
        }
        "remove" => {
            receipt.remove = None;
            receipt.non_utf8 = None;
            receipt.path = None;
        }
        "clear" => {
            receipt.set = Some(b"after-clear".to_vec());
            receipt.empty = Some(Vec::new());
            receipt.non_utf8 = Some(b"clear-\xfc".to_vec());
        }
        "path-explicit" => receipt.path = Some(directory.as_os_str().as_bytes().to_vec()),
        _ => {}
    }
    receipt
}

pub(super) fn driver(directory: &Path) -> io::Result<()> {
    let service = crate::processes::owned_fixture_service()?;
    let executable = std::env::current_exe()?;
    std::os::unix::fs::symlink(&executable, directory.join(EXECUTABLE))?;
    for case in CASES {
        let case_directory = directory.join(case);
        std::fs::create_dir(&case_directory)?;
        // Pass the original opaque Command directly to Service/OwnedChild. In
        // particular, do not serialize or reconstruct its env_clear state/argv0.
        let mut target = if matches!(case, "path-inherit" | "path-explicit") {
            Command::new(EXECUTABLE)
        } else {
            Command::new(&executable)
        };
        target
            .args(["--exact", super::HELPER, "--nocapture", "--test-threads=1"])
            .arg0(ARG0)
            // A broken arg0/env boundary must fail closed, never recursively run the
            // inherited driver mode. Driver dispatch also requires DRIVER_ARG0.
            .env_remove(super::MODE)
            .env_remove(super::DIRECTORY)
            .current_dir(&case_directory);
        match case {
            "set" => {
                target.env(SET, "changed").env(EMPTY, "").env(
                    OsStr::from_bytes(NON_UTF8),
                    OsStr::from_bytes(b"changed-\xfd"),
                );
            }
            "remove" => {
                target
                    .env_remove(REMOVE)
                    .env_remove(OsStr::from_bytes(NON_UTF8))
                    .env_remove("PATH");
            }
            "clear" => {
                target
                    .env_clear()
                    .env(SET, "after-clear")
                    .env(EMPTY, "")
                    .env(
                        OsStr::from_bytes(NON_UTF8),
                        OsStr::from_bytes(b"clear-\xfc"),
                    );
            }
            "empty-clear" => {
                target.env_clear();
            }
            "path-explicit" => {
                target.env_clear().env("PATH", directory);
            }
            _ => {}
        }
        let ticket = service
            .submit(target, options())
            .map_err(io::Error::other)?;
        let output = wait(&ticket).map_err(io::Error::other)?;
        assert!(output.stopped.is_none(), "{case}: {:?}", output.stopped);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{case}: snapshot stdout={:?}; stderr={:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(output.stderr.is_empty(), "{case}: {:?}", output.stderr);
        let receipt = expected(case, directory);
        let actual_bytes = std::fs::read(case_directory.join("environment-receipt.json"))?;
        let actual: Receipt = serde_json::from_slice(&actual_bytes)?;
        // Report only escaped key names from empty-clear. Strict receipt equality
        // requires the exact platform startup key set and truthful raw emptiness.
        let unexpected_names: Vec<_> = actual
            .unexpected_environment_keys
            .iter()
            .flatten()
            .map(|key| OsStr::from_bytes(key))
            .collect();
        assert_eq!(
            actual, receipt,
            "{case}: unexpected environment key names (no values)={unexpected_names:?}"
        );
        let mut bytes = serde_json::to_vec(&receipt)?;
        bytes.push(b'\n');
        assert_eq!(output.stdout, stdout(&bytes), "{case}");
        assert_eq!(actual_bytes, bytes, "{case}");
    }
    drop(service);
    io::stdout().write_all(DONE)?;
    io::stdout().flush()
}

#[test]
#[ignore = "runtime hold: exact source-reviewed invocation only"]
fn owned_runtime_opaque_command_environment() -> Result<(), Box<dyn std::error::Error>> {
    let mut scratch = Scratch::new()?;
    let service = crate::processes::owned_fixture_service()?;
    let mut driver = command("environment-driver", &scratch.0)?;
    driver
        .arg0(DRIVER_ARG0)
        .env(INHERIT, "inherit-me")
        .env(SET, "baseline-set")
        .env(REMOVE, "remove-me")
        .env_remove(EMPTY)
        .env("PATH", &scratch.0)
        .env(
            OsStr::from_bytes(NON_UTF8),
            OsStr::from_bytes(b"baseline-\xfe"),
        );
    // Seven sequential child launches have individual 12s budgets; this parent budget
    // is finite too. Each snapshot has its own retained Service/anchor owner in the
    // driver, is fixed and immediately exits; it cannot dispatch the driver recursively.
    let mut limits = options();
    limits.timeout = Some(std::time::Duration::from_secs(100));
    let ticket = service.submit(driver, limits)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(100);
    let output = loop {
        if let Some(result) = ticket.try_result() {
            break result?;
        }
        if std::time::Instant::now() >= deadline {
            return Err("environment driver deadline".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    assert!(
        output.stopped.is_none(),
        "{:?}; driver stdout={:?}; stderr={:?}",
        output.stopped,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "driver stdout={:?}; stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, stdout(DONE));
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    for case in CASES {
        let mut bytes = serde_json::to_vec(&expected(case, &scratch.0))?;
        bytes.push(b'\n');
        assert_eq!(
            std::fs::read(scratch.0.join(case).join("environment-receipt.json"))?,
            bytes
        );
    }
    drop(service);
    scratch.complete();
    Ok(())
}
