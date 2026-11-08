//! Local crash evidence and an argument-free operation ring for bug reports.
use std::{
    collections::VecDeque,
    path::Path,
    sync::{Mutex, Once},
};
static OPERATIONS: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

pub(crate) fn operation(args: &[&str], code: Option<i32>, elapsed: std::time::Duration) {
    // Never retain raw arguments, output, URLs, refs, authors, paths or environment.
    let noun = args
        .iter()
        .copied()
        .find(|word| !word.starts_with('-'))
        .unwrap_or_default();
    let noun = match noun {
        "fetch" | "push" | "pull" | "clone" | "commit" | "merge" | "rebase" | "checkout"
        | "switch" | "reset" | "revert" | "cherry-pick" | "add" | "restore" | "stash"
        | "branch" | "tag" | "worktree" => noun,
        _ => "operation",
    };
    if let Ok(mut ring) = OPERATIONS.lock() {
        if ring.len() == 30 {
            ring.pop_front();
        }
        ring.push_back(format!(
            "git {noun} [arguments and paths redacted] · exit {} · {} ms",
            code.map_or_else(|| "stopped".into(), |c| c.to_string()),
            elapsed.as_millis()
        ));
    }
}
pub(super) fn recent() -> Vec<String> {
    OPERATIONS
        .lock()
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default()
}

pub(crate) fn scrub(text: &str) -> String {
    let text = crate::git::redact(text, None);
    let text =
        regex::Regex::new(r"(?i)(?:https?|ssh)://[^\s\x22'<>]+|(?:[a-z]:[\\/]|/)[^\s\x22'<>]+")
            .map(|r| {
                r.replace_all(&text, "[path or address redacted]")
                    .into_owned()
            })
            .unwrap_or_default();
    regex::Regex::new(r"(?im)(password|passphrase|token|secret|authorization)[\s:=]+[^\r\n]+")
        .map(|r| r.replace_all(&text, "$1=[redacted]").into_owned())
        .unwrap_or_default()
}
pub(crate) fn install() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
                .unwrap_or("non-text panic");
            let location = info
                .location()
                .map(|l| {
                    format!(
                        "{}:{}",
                        Path::new(l.file())
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                        l.line()
                    )
                })
                .unwrap_or_default();
            let text = format!(
                "gitcraft {} · {location}\n{}\n",
                crate::update::CURRENT,
                scrub(&message.chars().take(8000).collect::<String>())
            );
            if let Some(dir) = crate::portable::data_dir() {
                let _ = persist(&dir, &text);
            }
            previous(info);
        }));
    });
}
fn persist(dir: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("last-panic-{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(tmp, dir.join("last-panic.log"))
}
pub(super) fn last() -> Option<String> {
    let dir = crate::portable::data_dir()?;
    let bytes = crate::safe_read::read_inside(&dir, "last-panic.log", 32768)?;
    Some(scrub(&String::from_utf8_lossy(&bytes)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panic_and_operation_evidence_excludes_paths_and_credentials() {
        let text = scrub(
            "failed /tmp/private/repo/file C:\\work\\repo token=fixture-secret\nhttps://user:password@host/private/repo",
        );
        for absent in ["/tmp/", "C:", "fixture-secret", "user:", "host/private"] {
            assert!(!text.contains(absent), "{text}");
        }
        operation(
            &["push", "https://token@host/private", "/work/repo"],
            Some(1),
            std::time::Duration::from_millis(42),
        );
        let recent = recent();
        let row = recent.last().cloned().unwrap_or_default();
        assert!(
            row.contains("push")
                && row.contains("42 ms")
                && !row.contains("host")
                && !row.contains("/work")
        );
    }
    #[test]
    fn persistent_panic_evidence_is_bounded_local_and_private() -> Result<(), String> {
        let dir =
            std::env::temp_dir().join(format!("gitcraft-panic-evidence-{}", std::process::id()));
        persist(&dir, "fixture panic").map_err(|e| e.to_string())?;
        assert_eq!(
            std::fs::read_to_string(dir.join("last-panic.log")).map_err(|e| e.to_string())?,
            "fixture panic"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.join("last-panic.log"))
                    .map_err(|e| e.to_string())?
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(dir).map_err(|e| e.to_string())
    }
}
