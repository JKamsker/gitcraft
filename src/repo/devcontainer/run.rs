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
        "Dev container configuration must be readable inside the worktree and at most 1 MiB",
    )?;
    Ok(Some(Config {
        path: std::fs::canonicalize(path).map_err(|e| e.to_string())?,
        source: String::from_utf8(source)
            .map_err(|_| "Dev container configuration is not UTF-8")?,
        context: crate::connections::current(),
    }))
}
fn guard(config: &Config) -> Result<(), String> {
    config.same(&detect(&config.path)?.ok_or("Dev container configuration disappeared")?)?;
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
        return Err("Container CLI result exceeds 8 MiB".into());
    }
    serde_json::from_str(&raw).map_err(|e| format!("Invalid {tool} JSON result: {e}"))
}
pub(super) fn engine(config: &Config) -> Result<Engine, String> {
    if std::env::var_os("DOCKER_HOST").is_some_and(|v| !v.is_empty()) {
        return Err(
            "Select a local Docker context and unset DOCKER_HOST before using this integration"
                .into(),
        );
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
        id.as_str().ok_or("Missing Docker daemon identity")?,
        endpoint.as_str().ok_or("Missing Docker endpoint")?,
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
    verify_owned(binding, config)
}
pub(super) fn start(config: &Config, old: Option<&Binding>) -> Result<Binding, String> {
    guard(config)?;
    if let Some(pause) = crate::network::refused() {
        return Err(format!(
            "{}; starting an environment can pull images/features",
            pause.why()
        ));
    }
    let engine = engine(config)?;
    if let Some(binding) = old {
        if engine != binding.engine {
            return Err("Docker context/daemon changed since display; refusing rebuild".into());
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
        return Err(
            "Container stopped; start the environment again before opening a terminal".into(),
        );
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
        return Err("Container stopped; start the environment before editor handoff".into());
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
    Ok("Handed the bound container workspace to VS Code. Its Dev Containers extension must be installed.".into())
}
