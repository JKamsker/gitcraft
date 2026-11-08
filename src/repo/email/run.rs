use super::model::{self, Message};
use crate::{connections, git};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub context: connections::Context,
    pub path: PathBuf,
    pub head: String,
    pub(super) branch: String,
    pub(super) status: String,
    pub(super) config: String,
}
fn git(path: &Path, args: &[&str]) -> Result<String, String> {
    git::run_unattended_within(path, args, Duration::from_secs(180))
}
pub(super) fn snapshot(path: &Path) -> Result<Snapshot, String> {
    if crate::remote::is_remote(path) {
        return Err(crate::i18n::text("repo-workflows-email-snapshot-a55ce185").into());
    }
    let config = git(path, &["config", "--null", "--list", "--includes"])?;
    let mut config = config.into_bytes();
    match std::fs::symlink_metadata(path.join(".b4-config")) {
        Ok(_) => config.extend(
            crate::safe_read::read_inside(path, ".b4-config", 1024 * 1024)
                .ok_or(crate::i18n::text("repo-workflows-email-snapshot-2627ca9e"))?,
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    Ok(Snapshot {
        context: connections::current(),
        path: std::fs::canonicalize(path).map_err(|e| e.to_string())?,
        head: git(path, &["rev-parse", "HEAD"])?.trim().into(),
        branch: git(path, &["rev-parse", "--abbrev-ref", "HEAD"])?
            .trim()
            .into(),
        status: git(path, &["status", "--porcelain=v1"])?,
        config: model::digest(&config),
    })
}
pub(super) fn guard(path: &Path, expected: &Snapshot) -> Result<(), String> {
    compare(expected, &snapshot(path)?)?;
    if crate::tasks::cancellation().is_cancelled() {
        return Err("Cancelled".into());
    }
    Ok(())
}
pub(super) fn compare(expected: &Snapshot, current: &Snapshot) -> Result<(), String> {
    if current != expected {
        return Err(crate::i18n::text("repo-workflows-email-compare-b5921a15").into());
    }
    if !expected.status.is_empty() {
        return Err(crate::i18n::text("repo-workflows-email-compare-6921220f").into());
    }
    Ok(())
}
fn invoke_b4(path: &Path, args: &[String], confirmed: bool) -> Result<String, String> {
    use std::io::Read;
    let script = crate::shell_path::resolve("b4");
    if !Path::new(&script).is_absolute() {
        return Err(crate::i18n::text("repo-workflows-email-invoke-b4-8728290f").into());
    }
    let mut prefix = Vec::new();
    std::fs::File::open(&script)
        .map_err(|e| {
            crate::i18n::message(
                "repo-workflows-email-invoke-b4-c9d943f4",
                &[("e", (e).to_string().into())],
            )
        })?
        .take(4096)
        .read_to_end(&mut prefix)
        .map_err(|e| e.to_string())?;
    let python = model::interpreter(&prefix)?;
    let args = model::command(&python, args, confirmed);
    git(path, &args.iter().map(String::as_str).collect::<Vec<_>>())
}
pub(super) fn b4(path: &Path, args: &[String]) -> Result<String, String> {
    invoke_b4(path, args, false)
}
pub(super) struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new() -> Result<Arc<Self>, String> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        for _ in 0..10 {
            let path = std::env::temp_dir().join(format!(
                "gitcraft-mail-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Arc::new(Self(path))),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Err(crate::i18n::text("repo-workflows-email-new-21656993").into())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) struct Received {
    pub snapshot: Snapshot,
    pub directory: Arc<Scratch>,
    pub file: PathBuf,
    pub messages: Vec<Message>,
    pub cover: String,
}
pub(super) fn receive(path: &Path, source: &str, revision: u32) -> Result<Received, String> {
    if let Some(pause) = crate::network::refused() {
        return Err(pause.why().into());
    }
    let source = model::source(source)?;
    let snapshot = snapshot(path)?;
    let directory = Scratch::new()?;
    let mut args = vec![
        "am".into(),
        "-o".into(),
        directory.0.to_string_lossy().into_owned(),
    ];
    if revision > 0 {
        args.extend(["-v".into(), revision.to_string()]);
    }
    args.extend(["--".into(), source]);
    b4(path, &args)?;
    let mut file = None;
    let mut cover = String::new();
    for entry in std::fs::read_dir(&directory.0).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or(crate::i18n::text("repo-workflows-email-receive-7fcd485d"))?;
        if (name.ends_with(".mbx") || name.ends_with(".mbox"))
            && file.replace(entry.path()).is_some()
        {
            return Err(crate::i18n::text("repo-workflows-email-receive-b0772e5b").into());
        }
        if name.ends_with(".cover") {
            let bytes = crate::safe_read::read_inside(&directory.0, name, model::MAX as u64)
                .ok_or(crate::i18n::text("repo-workflows-email-receive-9820d0e4"))?;
            cover = model::parse(&bytes)
                .map(|m| format!("{}\n{}", m.headers, m.body))
                .unwrap_or_else(|_| String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    let file = file.ok_or(crate::i18n::text("repo-workflows-email-receive-4350d1be"))?;
    let bytes = crate::safe_read::read_inside(
        &directory.0,
        file.file_name()
            .and_then(|n| n.to_str())
            .ok_or(crate::i18n::text("repo-workflows-email-receive-61e11166"))?,
        model::MAX as u64,
    )
    .ok_or(crate::i18n::text("repo-workflows-email-receive-3b985b29"))?;
    Ok(Received {
        snapshot,
        directory,
        file,
        messages: model::mailbox(&bytes)?,
        cover,
    })
}
pub(super) fn apply(path: &Path, received: &Received) -> Result<String, String> {
    guard(path, &received.snapshot)?;
    // Keep the private mailbox alive until git am has finished; conflicts use normal git am state.
    let _keep = &received.directory;
    git(
        path,
        &[
            "am",
            "--3way",
            received
                .file
                .to_str()
                .ok_or(crate::i18n::text("repo-workflows-email-apply-61c57973"))?,
        ],
    )
}
#[derive(Clone)]
pub(super) struct Preview {
    pub snapshot: Snapshot,
    pub messages: Vec<Message>,
    pub recipients: Vec<String>,
}
pub(super) fn preview(path: &Path, recipients: Vec<String>) -> Result<Preview, String> {
    let before = snapshot(path)?;
    guard(path, &before)?;
    let dir = Scratch::new()?;
    let mut args = vec![
        "send".into(),
        "-o".into(),
        dir.0.to_string_lossy().into_owned(),
    ];
    args.extend(recipients.clone());
    b4(path, &args)?;
    let mut files = std::fs::read_dir(&dir.0)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    files.sort_by_key(|e| e.file_name());
    if files.len() > 1024 {
        return Err(crate::i18n::text("repo-workflows-email-preview-ca11bd96").into());
    }
    let mut messages = Vec::new();
    let mut total = 0;
    for file in files {
        let name = file.file_name();
        let name = name
            .to_str()
            .ok_or(crate::i18n::text("repo-workflows-email-preview-7a897da3"))?;
        let bytes = crate::safe_read::read_inside(&dir.0, name, model::MAX as u64)
            .ok_or(crate::i18n::text("repo-workflows-email-preview-2345df14"))?;
        total += bytes.len();
        if total > model::MAX {
            return Err(crate::i18n::text("repo-workflows-email-preview-d135ecfa").into());
        }
        messages.push(model::parse(&bytes)?);
    }
    if messages.is_empty() {
        return Err(crate::i18n::text("repo-workflows-email-preview-ff2f2c5c").into());
    }
    guard(path, &before)?;
    Ok(Preview {
        snapshot: before,
        messages,
        recipients,
    })
}
pub(super) fn send(path: &Path, shown: &Preview) -> Result<String, String> {
    if let Some(pause) = crate::network::refused() {
        return Err(pause.why().into());
    }
    guard(path, &shown.snapshot)?;
    let fresh = preview(path, shown.recipients.clone())?;
    let mut args = vec!["send".into()];
    args.extend(shown.recipients.clone());
    model::send_reviewed(&shown.messages, &fresh.messages, || {
        guard(path, &shown.snapshot)?;
        invoke_b4(path, &args, true)
            .map(|output| {
                crate::i18n::message(
                    "repo-workflows-email-send-a590b131",
                    &[("output", (output).clone().into())],
                )
            })
            .map_err(|e| {
                crate::i18n::message(
                    "repo-workflows-email-send-9753828b",
                    &[("e", (e).to_string().into())],
                )
            })
    })
}
pub(super) fn prepare(
    path: &Path,
    expected: &Snapshot,
    mode: &str,
    value: &str,
) -> Result<String, String> {
    guard(path, expected)?;
    let mut args = vec!["prep".into()];
    let dir;
    match mode {
        "cover" => {
            dir = Scratch::new()?;
            let file = dir.0.join("cover.txt");
            std::fs::write(&file, value).map_err(|e| e.to_string())?;
            args.extend([
                "--cover-from-file".into(),
                file.to_string_lossy().into_owned(),
            ]);
        }
        "enroll" => {
            if value.is_empty() || value.starts_with('-') || value.chars().any(char::is_whitespace)
            {
                return Err(crate::i18n::text("repo-workflows-email-prepare-b8cd601f").into());
            }
            args.extend(["-e".into(), value.into()]);
        }
        "revision" => {
            let number = value
                .parse::<u32>()
                .map_err(|_| crate::i18n::text("repo-workflows-email-prepare-7fa1230f"))?;
            if number == 0 {
                return Err(crate::i18n::text("repo-workflows-email-prepare-d287cfde").into());
            }
            args.extend(["--force-revision".into(), number.to_string()]);
        }
        "reroll" => {
            args.extend(["--manual-reroll".into(), model::reroll_id(value)?]);
        }
        _ => return Err(crate::i18n::text("repo-workflows-email-prepare-ad82832b").into()),
    }
    b4(path, &args)
}
