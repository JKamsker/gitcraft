use super::model::{self, Workflow};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

pub(super) fn read_catalog(path: &Path) -> Result<(Vec<Workflow>, Vec<String>), String> {
    if crate::remote::is_remote(path) {
        return Err(crate::i18n::text("repo-workflows-local_ci-read-catalog-56e09929").into());
    }
    let entries = std::fs::read_dir(path.join(".github/workflows")).map_err(|e| {
        crate::i18n::message(
            "repo-workflows-local_ci-read-catalog-e3297274",
            &[("e", (e).to_string().into())],
        )
    })?;
    let mut files = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .collect::<Vec<_>>();
    files.sort();
    if files.len() > 256 {
        return Err(crate::i18n::text("repo-workflows-local_ci-read-catalog-4a734aad").into());
    }
    let (mut workflows, mut errors) = (Vec::new(), Vec::new());
    for name in files {
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.ends_with(".yml") && !name.ends_with(".yaml") {
            continue;
        }
        let file = format!(".github/workflows/{name}");
        let parsed = read_source(path, &file).and_then(|s| model::parse(file.clone(), s));
        match parsed {
            Ok(w) => workflows.push(w),
            Err(e) => errors.push(crate::i18n::message(
                "repo-workflows-local_ci-read-catalog-34bc7dab",
                &[
                    ("file", (file).clone().into()),
                    ("e", (e).to_string().into()),
                ],
            )),
        }
    }
    Ok((workflows, errors))
}
fn read_source(path: &Path, file: &str) -> Result<String, String> {
    let bytes = crate::safe_read::read_inside(path, file, model::MAX as u64).ok_or(
        crate::i18n::text("repo-workflows-local_ci-read-source-0066c097"),
    )?;
    String::from_utf8(bytes)
        .map_err(|_| crate::i18n::text("repo-workflows-local_ci-read-source-02772e72").into())
}

pub(super) struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new() -> Result<Self, String> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        for _ in 0..10 {
            let nonce = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("gitcraft-act-{}-{now}-{nonce}", std::process::id()));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err(crate::i18n::text("repo-workflows-local_ci-new-35401ccc").into())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Refuse ambient actrc flags, which can override selection, enable host execution,
/// reuse containers, or import credentials. The invocation directory is private.
fn check_config() -> Result<(), String> {
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        let home = PathBuf::from(home);
        paths.push(home.join(".actrc"));
        paths.push(home.join(".config/act/actrc"));
        paths.push(home.join("Library/Application Support/act/actrc"));
    }
    for env in ["XDG_CONFIG_HOME", "APPDATA"] {
        if let Some(base) = std::env::var_os(env) {
            paths.push(PathBuf::from(base).join("act/actrc"));
        }
    }
    for path in paths {
        match std::fs::read_to_string(&path) {
            Ok(s)
                if s.lines()
                    .any(|s| !s.trim().is_empty() && !s.trim().starts_with('#')) =>
            {
                return Err(crate::i18n::message(
                    "repo-workflows-local_ci-check-config-284c844f",
                    &[("path-display-1", path.display().to_string().into())],
                ));
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(crate::i18n::message(
                    "repo-workflows-local_ci-check-config-d57e1227",
                    &[
                        ("path-display-1", path.display().to_string().into()),
                        ("e", (e).to_string().into()),
                    ],
                ));
            }
            _ => {}
        }
    }
    Ok(())
}
pub(super) struct Selection {
    pub workflow: Workflow,
    pub context: super::context::Context,
    pub event: String,
    pub job: String,
    pub matrix: yaml_rust2::Yaml,
    pub payload: String,
    pub platforms: String,
    pub architecture: String,
    pub minutes: u64,
}
pub(super) fn arguments(path: &Path, scratch: &Path, s: &Selection) -> Result<Vec<String>, String> {
    let payload: serde_json::Value = serde_json::from_str(&s.payload).map_err(|e| {
        crate::i18n::message(
            "repo-workflows-local_ci-arguments-d09062d3",
            &[("e", (e).to_string().into())],
        )
    })?;
    if !payload.is_object() || s.payload.len() > model::MAX {
        return Err(crate::i18n::text("repo-workflows-local_ci-arguments-7e0ae020").into());
    }
    let mut args = vec![
        s.event.clone(),
        "--remote-name".into(),
        s.context.name.clone(),
        "--github-instance".into(),
        "github.com".into(),
        "--env".into(),
        format!("GITHUB_REPOSITORY={}", s.context.repository),
        "--env".into(),
        format!(
            "GITHUB_REPOSITORY_OWNER={}",
            s.context.repository.split('/').next().unwrap_or_default()
        ),
        "--directory".into(),
        path.to_string_lossy().into_owned(),
        "--workflows".into(),
        scratch.join("workflow.yml").to_string_lossy().into_owned(),
        "--job".into(),
        s.job.clone(),
        "--eventpath".into(),
        scratch.join("event.json").to_string_lossy().into_owned(),
        "--rm".into(),
        "--reuse=false".into(),
        "--bind=false".into(),
        "--watch=false".into(),
        "--detect-event=false".into(),
        "--insecure-secrets=false".into(),
        "--no-cache-server".into(),
        "--container-daemon-socket=-".into(),
    ];
    for flag in ["--env-file", "--secret-file", "--var-file", "--input-file"] {
        args.extend([
            flag.into(),
            scratch.join("empty").to_string_lossy().into_owned(),
        ]);
    }
    let platforms = s
        .platforms
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if platforms.is_empty() {
        return Err(crate::i18n::text("repo-workflows-local_ci-arguments-9ada5400").into());
    }
    for mapping in platforms {
        let (label, image) = mapping.split_once('=').ok_or(crate::i18n::text(
            "repo-workflows-local_ci-arguments-2686361a",
        ))?;
        if label.trim().is_empty()
            || image.trim().is_empty()
            || image.starts_with('-')
            || mapping.contains('\0')
        {
            return Err(crate::i18n::text("repo-workflows-local_ci-arguments-48f831c2").into());
        }
        args.extend(["--platform".into(), mapping.into()]);
    }
    if !s.architecture.is_empty() {
        if !["linux/amd64", "linux/arm64", "linux/arm/v7"].contains(&s.architecture.as_str()) {
            return Err(crate::i18n::text("repo-workflows-local_ci-arguments-39e7fe22").into());
        }
        args.extend(["--container-architecture".into(), s.architecture.clone()]);
    }
    Ok(args)
}
#[derive(Default)]
pub(super) struct Log {
    pub text: String,
    pub rows: Vec<String>,
    pub truncated: bool,
}
impl Log {
    pub fn append(&mut self, bytes: &[u8]) {
        const LIMIT: usize = 2 << 20;
        let text = String::from_utf8_lossy(bytes);
        let remaining = LIMIT.saturating_sub(self.text.len());
        let mut end = remaining.min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.text.push_str(&text[..end]);
        if self.rows.is_empty() {
            self.rows.push(String::new());
        }
        for ch in text[..end].chars() {
            if self.rows.len() >= 32768 {
                self.truncated = true;
                break;
            }
            if ch == '\n' {
                self.rows.push(String::new());
                continue;
            }
            if self.rows.last().is_some_and(|row| row.len() >= 400) {
                self.rows.push(String::new());
            }
            if let Some(row) = self.rows.last_mut() {
                row.push(ch);
            }
        }
        self.truncated |= end < text.len();
    }
}
pub(super) fn execute(
    path: PathBuf,
    s: Selection,
    log: Arc<Mutex<Log>>,
    ctx: egui::Context,
) -> Result<String, String> {
    execute_with(path, s, log, ctx, std::ffi::OsStr::new("act"))
}
pub(super) fn execute_with(
    path: PathBuf,
    s: Selection,
    log: Arc<Mutex<Log>>,
    ctx: egui::Context,
    program: &std::ffi::OsStr,
) -> Result<String, String> {
    check_config()?;
    if read_source(&path, &s.workflow.file)? != s.workflow.source {
        return Err(crate::i18n::text("repo-workflows-local_ci-execute-with-bdf39918").into());
    }
    s.context.revalidate_with(|args| {
        crate::git::run_unattended_within(&path, args, Duration::from_secs(20))
    })?;
    let scratch = Scratch::new()?;
    let nonce = scratch
        .0
        .file_name()
        .ok_or(crate::i18n::text(
            "repo-workflows-local_ci-execute-with-77a6f13d",
        ))?
        .to_string_lossy();
    let snapshot = s.workflow.snapshot(&s.event, &s.job, &s.matrix, &nonce)?;
    let args = arguments(&path, &scratch.0, &s)?;
    std::fs::write(scratch.0.join("workflow.yml"), snapshot).map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("event.json"), &s.payload).map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("empty"), []).map_err(|e| e.to_string())?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(&scratch.0)
        .env("NO_COLOR", "1")
        .env("CI", "true");
    // Never lend the selected forge account to a workflow. Users must explicitly provide
    // workflow credentials through their trusted workflow; automatic .secrets is disabled.
    for key in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GH_ENTERPRISE_TOKEN",
        "GITLAB_TOKEN",
        "GLAB_TOKEN",
        "GITEA_TOKEN",
        "TEA_TOKEN",
        "AZURE_DEVOPS_EXT_PAT",
    ] {
        command.env_remove(key);
    }
    crate::shell_path::apply(&mut command);
    let out = crate::processes::run(command, options(&s, &log, &ctx))?;
    if let Some(reason) = out.stopped {
        return Err(crate::i18n::message(
            "repo-workflows-local_ci-execute-with-2ea8f492",
            &[("reason", (reason).clone().into())],
        ));
    }
    if !out.status.success() {
        return Err(crate::i18n::message(
            "repo-workflows-local_ci-execute-with-142bda92",
            &[("out-status-1", out.status.to_string().into())],
        ));
    }
    Ok(crate::i18n::text("repo-workflows-local_ci-execute-with-3e3b1e0a").into())
}

// Both callbacks deliver bounded visible output while the operation is running.
// The process owner separately bounds capture and supervises termination.
pub(super) fn options(
    s: &Selection,
    log: &Arc<Mutex<Log>>,
    ctx: &egui::Context,
) -> crate::processes::Options {
    fn stream(log: Arc<Mutex<Log>>, repaint: egui::Context) -> crate::processes::Chunk {
        Box::new(move |bytes| {
            log.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .append(bytes);
            repaint.request_repaint();
        })
    }
    crate::processes::Options {
        timeout: Some(Duration::from_secs(s.minutes.clamp(1, 120) * 60)),
        background: true,
        output_limit: 8 << 20,
        stdout: Some(stream(Arc::clone(log), ctx.clone())),
        stderr: Some(stream(Arc::clone(log), ctx.clone())),
        ..Default::default()
    }
}
