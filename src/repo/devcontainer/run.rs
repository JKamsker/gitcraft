use super::model::{self, Binding, Config, Engine};
use serde_json::Value;
use std::{path::Path, time::Duration};
pub(super) fn detect(path: &Path) -> Result<Option<Config>, String> {
    if crate::remote::is_remote(path) {
        return Ok(None);
    }
    match std::fs::symlink_metadata(path.join(model::CONFIG)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let source = crate::safe_read::read_inside(path, model::CONFIG, 1024 * 1024).ok_or(
        crate::i18n::text("repo-workflows-devcontainer-detect-01821b94"),
    )?;
    let physical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    let linked = metadata(&physical)?;
    Ok(Some(Config {
        path: physical,
        linked,
        source: String::from_utf8(source)
            .map_err(|_| crate::i18n::text("repo-workflows-devcontainer-detect-764255b7"))?,
        context: crate::connections::current(),
    }))
}
// Bounded metadata reads only; no Git command, helper or metadata rewrite.
fn metadata_file(path: &Path) -> Result<String, String> {
    use std::io::Read;
    if !std::fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_file()
    {
        return Err(crate::i18n::text("repo-workflows-devcontainer-metadata-file-bd4adcf1").into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 8192 {
        return Err(crate::i18n::text("repo-workflows-devcontainer-metadata-file-ddb7553c").into());
    }
    String::from_utf8(bytes)
        .map_err(|_| crate::i18n::text("repo-workflows-devcontainer-metadata-file-a1a99ba5").into())
}
fn metadata(path: &Path) -> Result<Option<model::Linked>, String> {
    let kind = std::fs::symlink_metadata(path.join(".git"))
        .map_err(|e| {
            crate::i18n::message(
                "repo-workflows-devcontainer-metadata-b9388ff2",
                &[("e", (e).to_string().into())],
            )
        })?
        .file_type();
    if kind.is_dir() {
        return Ok(None);
    }
    if !kind.is_file() || !cfg!(unix) {
        return Err(crate::i18n::text("repo-workflows-devcontainer-metadata-735e6123").into());
    }
    let dotgit = metadata_file(&path.join(".git"))?;
    let gitdir = path.join(model::gitdir_pointer(&dotgit)?);
    let linked = model::linked(
        path,
        &dotgit,
        &metadata_file(&gitdir.join("commondir"))?,
        &metadata_file(&gitdir.join("gitdir"))?,
    )?;
    let expected_gitdir = linked
        .common
        .join("worktrees")
        .join(gitdir.file_name().ok_or(crate::i18n::text(
            "repo-workflows-devcontainer-metadata-0a13f60e",
        ))?);
    if std::fs::canonicalize(&gitdir).map_err(|e| e.to_string())? != expected_gitdir
        || std::fs::canonicalize(&linked.common).map_err(|e| e.to_string())? != linked.common
        || !linked.common.join("objects").is_dir()
    {
        return Err(crate::i18n::text("repo-workflows-devcontainer-metadata-b2bab91f").into());
    }
    Ok(Some(linked))
}
fn guard(config: &Config) -> Result<(), String> {
    config.same(&detect(&config.path)?.ok_or(crate::i18n::text(
        "repo-workflows-devcontainer-guard-7103bfb5",
    ))?)?;
    if crate::tasks::cancellation().is_cancelled() {
        return Err("Cancelled".into());
    }
    Ok(())
}
fn invoke(config: &Config, tool: &str, context: &str, args: &[String]) -> Result<String, String> {
    let args = model::command(tool, context, args);
    crate::git::run_unattended_within(
        &config.path,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        Duration::from_secs(1800),
    )
}
fn json(config: &Config, tool: &str, context: &str, args: &[String]) -> Result<Value, String> {
    let raw = invoke(config, tool, context, args)?;
    if raw.len() > 8 * 1024 * 1024 {
        return Err(crate::i18n::text("repo-workflows-devcontainer-json-0d639c4b").into());
    }
    serde_json::from_str(&raw).map_err(|e| {
        crate::i18n::message(
            "repo-workflows-devcontainer-json-edd11b13",
            &[("tool", (tool).into()), ("e", (e).to_string().into())],
        )
    })
}
pub(super) fn engine(config: &Config) -> Result<Engine, String> {
    if std::env::var_os("DOCKER_HOST").is_some_and(|v| !v.is_empty()) {
        return Err(crate::i18n::text("repo-workflows-devcontainer-engine-1cfeec69").into());
    }
    // Docker context show honors DOCKER_CONTEXT. A context is then pinned for all later invocations.
    let context = invoke(config, "docker", "", &["context".into(), "show".into()])?
        .trim()
        .to_string();
    let endpoint = json(
        config,
        "docker",
        &context,
        &[
            "context".into(),
            "inspect".into(),
            context.clone(),
            "--format".into(),
            "{{json .Endpoints.docker.Host}}".into(),
        ],
    )?;
    let id = json(
        config,
        "docker",
        &context,
        &["info".into(), "--format".into(), "{{json .ID}}".into()],
    )?;
    model::engine(
        &context,
        id.as_str().ok_or(crate::i18n::text(
            "repo-workflows-devcontainer-engine-f6690c9e",
        ))?,
        endpoint.as_str().ok_or(crate::i18n::text(
            "repo-workflows-devcontainer-engine-161bfc12",
        ))?,
    )
}
fn verify_owned(binding: &Binding, current: &Config) -> Result<Value, String> {
    guard(current)?;
    let engine = engine(current)?;
    let data = json(
        current,
        "docker",
        &engine.context,
        &["inspect".into(), binding.id.clone()],
    )?;
    model::inspect(binding, current, &engine, &data)?;
    Ok(data)
}
pub(super) fn verify(binding: &Binding, config: &Config) -> Result<Value, String> {
    binding.config.same(config)?;
    let data = verify_owned(binding, config)?;
    model::metadata_ready(binding, &data)?;
    Ok(data)
}
pub(super) fn start(config: &Config, old: Option<&Binding>) -> Result<Binding, String> {
    guard(config)?;
    if let Some(pause) = crate::network::refused() {
        return Err(crate::i18n::message(
            "repo-workflows-devcontainer-start-87b3fed2",
            &[("pause-why-1", (pause.why()).into())],
        ));
    }
    let engine = engine(config)?;
    if config.linked.is_some() {
        let native = json(
            config,
            "devcontainer",
            &engine.context,
            &model::arguments(config, "read-configuration"),
        )?;
        model::native_configuration(config, &native)?;
        guard(config)?;
    }
    if let Some(binding) = old {
        if engine != binding.engine {
            return Err(crate::i18n::text("repo-workflows-devcontainer-start-6a0249a9").into());
        }
        // Only the exact displayed ID, after ownership inspection. No ps selector/prune/global cleanup.
        model::rebuild_checked(
            binding,
            || verify_owned(binding, config).map(|_| ()),
            |args| invoke(config, "docker", &engine.context, args),
        )?;
    }
    let value = json(
        config,
        "devcontainer",
        &engine.context,
        &model::arguments(config, "up"),
    )?;
    guard(config)?;
    let binding = model::started(config.clone(), engine, &value)?;
    verify(&binding, config)?;
    Ok(binding)
}
pub(super) fn terminal(
    binding: &Binding,
    config: &Config,
    ctx: &egui::Context,
) -> Result<crate::terminal::Terminal, String> {
    let data = verify(binding, config)?;
    if data[0]["State"]["Running"] != true {
        return Err(crate::i18n::text("repo-workflows-devcontainer-terminal-b74a6469").into());
    }
    if crate::tasks::cancellation().is_cancelled() {
        return Err("Cancelled before terminal start".into());
    }
    let shell = crate::terminal::ShellSpec {
        program: crate::shell_path::resolve("docker"),
        args: model::terminal(binding)?,
        label: format!("Container {} · {}", &binding.id[..12], binding.folder),
    };
    crate::terminal::Terminal::spawn(&config.path, &shell, ctx)
}
pub(super) fn editor(binding: &Binding, config: &Config) -> Result<String, String> {
    let data = verify(binding, config)?;
    if data[0]["State"]["Running"] != true {
        return Err(crate::i18n::text("repo-workflows-devcontainer-editor-47c00405").into());
    }
    let uri = model::editor_uri(binding)?;
    invoke(
        config,
        "devcontainer",
        &binding.engine.context,
        &["--version".into()],
    )?;
    // VS Code's CLI accepts an explicit remote folder URI; no host editor relabelled as a container.
    let args = [
        "-c".to_string(),
        "alias.gitcraft-code=!f() { exec code --new-window --folder-uri \"$1\"; }; f".to_string(),
        "gitcraft-code".into(),
        uri,
    ];
    crate::git::run_unattended_within(
        &config.path,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        Duration::from_secs(30),
    )?;
    Ok(crate::i18n::text("repo-workflows-devcontainer-editor-806df56f").into())
}
