//! Inert CLI plans and container ownership checks. No Docker is launched here.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(super) const CONFIG: &str = ".devcontainer/devcontainer.json";
pub(super) fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Config {
    pub path: PathBuf,
    pub source: String,
    pub context: crate::connections::Context,
}
impl Config {
    pub fn label(&self) -> String {
        format!(
            "gitcraft.worktree={}",
            hash(self.path.to_string_lossy().as_bytes())
        )
    }
    pub fn same(&self, other: &Self) -> Result<(), String> {
        if self == other {
            Ok(())
        } else {
            Err(
                "Worktree, configuration or selected profile changed; read configuration again"
                    .into(),
            )
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Engine {
    pub context: String,
    pub id: String,
}
#[derive(Clone, Debug)]
pub(super) struct Binding {
    pub config: Config,
    pub engine: Engine,
    pub id: String,
    pub folder: String,
    pub user: String,
}
fn clean(value: &str) -> bool {
    !value.is_empty() && value.len() <= 8192 && !value.chars().any(char::is_control)
}
pub(super) fn container_id(value: &str) -> Result<(), String> {
    if value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("Docker must return a complete 64-character container ID".into())
    }
}
pub(super) fn engine(context: &str, id: &str, endpoint: &str) -> Result<Engine, String> {
    if !clean(context)
        || context.starts_with('-')
        || !context
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        || !clean(id)
    {
        return Err("Invalid Docker context/daemon identity".into());
    }
    if !endpoint.starts_with("unix:///") && !endpoint.starts_with("npipe://") {
        return Err("This integration supports local Docker sockets. SSH/TCP Docker hosts require a separate native setup; no automatic remote handoff is attempted.".into());
    }
    Ok(Engine {
        context: context.into(),
        id: id.into(),
    })
}
pub(super) fn arguments(config: &Config, verb: &str) -> Vec<String> {
    vec![
        verb.into(),
        "--workspace-folder".into(),
        config.path.to_string_lossy().into_owned(),
        "--config".into(),
        config.path.join(CONFIG).to_string_lossy().into_owned(),
        "--mount-workspace-git-root=false".into(),
        "--mount-git-worktree-common-dir=true".into(),
        "--id-label".into(),
        config.label(),
        "--id-label".into(),
        format!("gitcraft.config={}", hash(config.source.as_bytes())),
        "--id-label".into(),
        format!("devcontainer.local_folder={}", config.path.display()),
        "--id-label".into(),
        format!(
            "devcontainer.config_file={}",
            config.path.join(CONFIG).display()
        ),
    ]
}
/// Fixed alias only; context and every argument are positional, never shell text.
pub(super) fn command(program: &str, context: &str, args: &[String]) -> Vec<String> {
    let alias = match program {
        "docker" => {
            "alias.gitcraft-container=!f() { ctx=\"$1\"; shift; if [ -n \"$ctx\" ]; then export DOCKER_CONTEXT=\"$ctx\"; fi; unset DOCKER_HOST; exec docker \"$@\"; }; f"
        }
        _ => {
            "alias.gitcraft-container=!f() { ctx=\"$1\"; shift; if [ -n \"$ctx\" ]; then export DOCKER_CONTEXT=\"$ctx\"; fi; unset DOCKER_HOST; exec devcontainer \"$@\"; }; f"
        }
    };
    let mut result = vec![
        "-c".into(),
        alias.into(),
        "gitcraft-container".into(),
        context.into(),
    ];
    result.extend_from_slice(args);
    result
}
pub(super) fn started(config: Config, engine: Engine, value: &Value) -> Result<Binding, String> {
    if value["outcome"] != "success" {
        return Err(format!(
            "Dev Container CLI did not start the environment: {}",
            value["description"].as_str().unwrap_or("no success result")
        ));
    }
    let id = value["containerId"]
        .as_str()
        .ok_or("Missing containerId")?
        .to_string();
    container_id(&id)?;
    let folder = value["remoteWorkspaceFolder"]
        .as_str()
        .ok_or("Missing remoteWorkspaceFolder")?
        .to_string();
    let user = value["remoteUser"]
        .as_str()
        .ok_or("Missing remoteUser")?
        .to_string();
    if !clean(&folder)
        || !folder.starts_with('/')
        || Path::new(&folder)
            .components()
            .any(|p| matches!(p, std::path::Component::ParentDir))
        || !clean(&user)
        || user.starts_with('-')
    {
        return Err("Unsupported container workspace/user returned by CLI".into());
    }
    Ok(Binding {
        config,
        engine,
        id,
        folder,
        user,
    })
}
pub(super) fn inspect(
    binding: &Binding,
    current: &Config,
    engine: &Engine,
    data: &Value,
) -> Result<(), String> {
    if binding.engine != *engine
        || binding.config.path != current.path
        || binding.config.context != current.context
    {
        return Err(
            "Container belongs to another worktree, Docker daemon or selected profile".into(),
        );
    }
    container_id(&binding.id)?;
    let entries = data
        .as_array()
        .ok_or("Docker inspect must return an array")?;
    if entries.len() != 1 {
        return Err("Expected exactly one bound container".into());
    }
    let item = &entries[0];
    let labels = &item["Config"]["Labels"];
    let expected = current.label();
    let (_, label) = expected.split_once('=').ok_or("Invalid ownership label")?;
    if labels["gitcraft.config"] != hash(binding.config.source.as_bytes())
        || item["Id"] != binding.id
        || labels["gitcraft.worktree"] != label
        || labels["devcontainer.local_folder"] != current.path.to_string_lossy().as_ref()
        || labels["devcontainer.config_file"]
            != current.path.join(CONFIG).to_string_lossy().as_ref()
    {
        return Err("Container ID or worktree/configuration ownership labels no longer match; refusing this target".into());
    }
    if !item["Mounts"].as_array().is_some_and(|mounts| {
        mounts.iter().any(|m| {
            m["Type"] == "bind"
                && m["Source"] == current.path.to_string_lossy().as_ref()
                && m["Destination"].as_str().is_some_and(|dest| {
                    dest.starts_with('/') && Path::new(&binding.folder).starts_with(dest)
                })
        })
    }) {
        return Err("Container does not bind this exact worktree into its displayed workspace. Volume-only or parent-directory workspace mounts are currently unsupported.".into());
    }
    Ok(())
}
pub(super) fn removal(binding: &Binding) -> Result<Vec<String>, String> {
    container_id(&binding.id)?;
    Ok(vec!["rm".into(), "--force".into(), binding.id.clone()])
}
pub(super) fn terminal(binding: &Binding) -> Result<Vec<String>, String> {
    container_id(&binding.id)?;
    Ok(vec![
        "--context".into(),
        binding.engine.context.clone(),
        "exec".into(),
        "--interactive".into(),
        "--tty".into(),
        "--workdir".into(),
        binding.folder.clone(),
        "--user".into(),
        binding.user.clone(),
        binding.id.clone(),
        "/bin/sh".into(),
        "-l".into(),
    ])
}
pub(super) fn editor_uri(binding: &Binding) -> Result<String, String> {
    container_id(&binding.id)?;
    // Native Dev Containers maintainer's JSON/hex attached-container authority; context pins the local engine.
    let metadata =
        json!({"containerName": binding.id, "settings": {"context": binding.engine.context}})
            .to_string();
    let hex: String = metadata
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut uri = url::Url::parse(&format!("vscode-remote://attached-container+{hex}/"))
        .map_err(|e| e.to_string())?;
    uri.set_path(&binding.folder);
    Ok(uri.into())
}

pub(super) fn rebuild_checked<T>(
    binding: &Binding,
    verify: impl FnOnce() -> Result<(), String>,
    remove: impl FnOnce(&[String]) -> Result<T, String>,
) -> Result<T, String> {
    verify()?;
    remove(&removal(binding)?)
}
