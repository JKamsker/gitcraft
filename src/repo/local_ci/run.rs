use super::model::{self, Workflow};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

pub(super) fn read_catalog(path: &Path) -> Result<(Vec<Workflow>, Vec<String>), String> {
    read_catalog_with_witness(path)
        .map(|(workflows, errors)| {
            (
                workflows,
                errors.iter().map(CatalogFailure::display).collect(),
            )
        })
        .map_err(|failure| failure.display())
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

pub(super) fn read_catalog_with_witness(
    path: &Path,
) -> Result<(Vec<Workflow>, Vec<CatalogFailure>), CatalogFailure> {
    if crate::remote::is_remote(path) {
        return Err(CatalogFailure::Remote.into());
    }
    let entries = std::fs::read_dir(path.join(".github/workflows")).map_err(|e| {
        CatalogFailure::Directory {
            detail: e.to_string(),
        }
    })?;
    let mut files = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name())
        .collect::<Vec<_>>();
    files.sort();
    if files.len() > 256 {
        return Err(CatalogFailure::TooMany.into());
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
        let parsed = read_source_with_witness(path, &file)
            .and_then(|s| model::parse_with_witness(file.clone(), s).map_err(SourceFailure::Parse));
        match parsed {
            Ok(w) => workflows.push(w),
            Err(e) => errors.push(CatalogFailure::File { file, cause: e }),
        }
    }
    Ok((workflows, errors))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SourceFailure {
    Unsafe,
    Utf8,
    Parse(model::ParseFailure),
}
impl SourceFailure {
    pub fn raw(&self) -> String {
        match self {
            Self::Unsafe => {
                "Workflow is missing, linked outside the worktree, unreadable, or exceeds 1 MiB"
                    .into()
            }
            Self::Utf8 => "Workflow is not UTF-8".into(),
            Self::Parse(p) => p.raw(),
        }
    }
    pub fn display(&self) -> String {
        match self {
            Self::Unsafe => {
                crate::i18n::text("repo-workflows-local_ci-read-source-0066c097").into()
            }
            Self::Utf8 => crate::i18n::text("repo-workflows-local_ci-read-source-02772e72").into(),
            Self::Parse(p) => p.display(),
        }
    }
}
fn read_source_with_witness(path: &Path, file: &str) -> Result<String, SourceFailure> {
    let bytes = crate::safe_read::read_inside(path, file, model::MAX as u64)
        .ok_or(SourceFailure::Unsafe)?;
    String::from_utf8(bytes).map_err(|_| SourceFailure::Utf8)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CatalogFailure {
    Remote,
    TooMany,
    Directory { detail: String },
    File { file: String, cause: SourceFailure },
    Stale,
    Ended,
}
impl CatalogFailure {
    pub fn raw(&self) -> String {
        match self {
Self::Remote => "Local Actions needs a worktree on this machine. Use act on the repository's machine for a remote worktree.".into(),
Self::TooMany => "More than 256 workflow files; narrow the workflow directory first".into(),
Self::Stale => "Profile changed; the local operation was cancelled. An act run's Docker resources may remain; inspect them before retrying.".into(),
Self::Ended => "Local operation ended without an answer (cancelled or worker failed). Inspect Docker resources before retrying.".into(),

        Self::Directory{detail}=>format!("Cannot read .github/workflows: {detail}"),
        Self::File{file,cause}=>format!("{file}: {}",cause.raw()),
    }
    }
    pub fn display(&self) -> String {
        match self {
            Self::Remote => {
                crate::i18n::text("repo-workflows-local_ci-read-catalog-56e09929").into()
            }
            Self::TooMany => {
                crate::i18n::text("repo-workflows-local_ci-read-catalog-4a734aad").into()
            }
            Self::Stale => crate::i18n::text("repo-workflows-local_ci-poll-93b6c091").into(),
            Self::Ended => crate::i18n::text("repo-workflows-local_ci-poll-3b4f1312").into(),

            Self::Directory { detail } => crate::i18n::message(
                "repo-workflows-local_ci-read-catalog-e3297274",
                &[("e", detail.as_str().into())],
            ),
            Self::File { file, cause } => crate::i18n::message(
                "repo-workflows-local_ci-read-catalog-34bc7dab",
                &[
                    ("file", file.as_str().into()),
                    ("e", cause.display().into()),
                ],
            ),
        }
    }
}

#[cfg(test)]
mod fluent_r1_catalog_tests {
    use super::*;
    #[test]
    fn fluent_partition_r1_local_actions_retains_typed_parser_and_opaque_yaml_details() {
        crate::i18n::set_language("de");
        let cause = model::parse_with_witness("raw.yml".into(), "on: push\n".into()).unwrap_err();
        let error = CatalogFailure::File {
            file: "raw; ;\n{$path}/1234.yml".into(),
            cause: SourceFailure::Parse(cause),
        };
        let raw = error.raw();
        let mut state = super::super::State::default();
        state.errors = vec![error.clone()];
        state.remember_catalog_failure(error.clone());
        for lang in ["de", "en-US", "de"] {
            crate::i18n::set_language(lang);
            assert_eq!(state.errors[0], error);
            let shown = state.errors[0].display();
            assert!(shown.contains("raw; ;\n{$path}/1234.yml"));
            assert_eq!(shown == raw, lang == "en-US");
            assert_eq!(state.display_result_error(&raw), shown);
            assert_eq!(state.result.as_ref().unwrap().as_ref().unwrap_err(), &raw);
            assert!(state.pending.is_none());
        }
        let lookalike = model::ParseFailure::Opaque("Workflow has no jobs mapping".into());
        assert_eq!(lookalike.display(), lookalike.raw());
        crate::i18n::set_language("en-US");
    }
}
