use super::model::{self, Binding, Config, Engine};
use serde_json::json;
fn config() -> Config {
    Config {
        path: "/owned/worktree one".into(),
        source: "{\n// jsonc\n\"image\":\"debian\"\n}".into(),
        context: Default::default(),
    }
}
fn binding() -> Binding {
    Binding {
        config: config(),
        engine: Engine {
            context: "desktop-linux".into(),
            id: "owned-daemon".into(),
        },
        id: "a".repeat(64),
        folder: "/workspaces/worktree one".into(),
        user: "vscode".into(),
    }
}
fn inspection(binding: &Binding) -> serde_json::Value {
    json!([{"Id":binding.id,"Config":{"Labels":{"gitcraft.config":model::hash(binding.config.source.as_bytes()),"gitcraft.worktree":binding.config.label().split_once('=').map(|(_,v)|v).unwrap_or_default(),"devcontainer.local_folder":binding.config.path,"devcontainer.config_file":binding.config.path.join(model::CONFIG)}},"State":{"Running":true},"Mounts":[{"Type":"bind","Source":binding.config.path,"Destination":"/workspaces/worktree one"}]}])
}
#[test]
fn per_worktree_up_has_no_global_cleanup_and_positional_arguments() {
    let config = config();
    let args = model::arguments(&config, "up");
    assert!(
        args.windows(2)
            .any(|w| w == ["--workspace-folder", "/owned/worktree one"])
    );
    assert!(args.contains(&config.label()));
    assert!(args.contains(&"--mount-workspace-git-root=false".into()));
    assert!(
        !args
            .iter()
            .any(|a| a.contains("remove-existing") || a.contains("prune"))
    );
    let plan = model::command("devcontainer", "desktop-linux", &args);
    assert_eq!(plan[3], "desktop-linux");
    assert!(!plan[1].contains("worktree one"));
    assert!(plan[1].contains("\"$@\""));
    let mut other = config.clone();
    other.path = "/owned/second worktree".into();
    assert_ne!(config.label(), other.label());
}
#[test]
fn start_result_and_native_ownership_bind_exact_container() -> Result<(), String> {
    let old = binding();
    let value = json!({"outcome":"success","containerId":old.id,"remoteWorkspaceFolder":old.folder,"remoteUser":old.user});
    let bound = model::started(config(), old.engine.clone(), &value)?;
    model::inspect(&bound, &config(), &bound.engine, &inspection(&bound))?;
    for field in ["containerId", "remoteWorkspaceFolder", "remoteUser"] {
        let mut bad = value.clone();
        bad[field] = json!("");
        assert!(model::started(config(), old.engine.clone(), &bad).is_err());
    }
    assert!(
        model::started(
            config(),
            old.engine,
            &json!({"outcome":"error","description":"image build failed"})
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn stale_daemon_profile_labels_and_mounts_refuse_owned_target() {
    let binding = binding();
    let mut config = config();
    let good = inspection(&binding);
    let mut wrong = binding.engine.clone();
    wrong.id = "another-daemon".into();
    assert!(model::inspect(&binding, &config, &wrong, &good).is_err());
    config.context.choices.github.user = "another-account".into();
    assert!(model::inspect(&binding, &config, &binding.engine, &good).is_err());
    config = binding.config.clone();
    for field in [
        "gitcraft.worktree",
        "devcontainer.local_folder",
        "devcontainer.config_file",
    ] {
        let mut bad = good.clone();
        bad[0]["Config"]["Labels"][field] = json!("foreign");
        assert!(model::inspect(&binding, &config, &binding.engine, &bad).is_err());
    }
    let mut bad = good.clone();
    bad[0]["Id"] = json!("b".repeat(64));
    assert!(model::inspect(&binding, &config, &binding.engine, &bad).is_err());
    bad = good;
    bad[0]["Mounts"][0]["Source"] = json!("/another/worktree");
    assert!(model::inspect(&binding, &config, &binding.engine, &bad).is_err());
}
#[test]
fn rebuild_removes_only_full_displayed_id_after_external_guard() -> Result<(), String> {
    let mut binding = binding();
    assert_eq!(
        model::removal(&binding)?,
        vec!["rm", "--force", &"a".repeat(64)]
    );
    binding.id = "--all".into();
    assert!(model::removal(&binding).is_err());
    binding.id = "abc123".into();
    assert!(model::removal(&binding).is_err());
    Ok(())
}
#[test]
fn terminal_and_editor_use_container_context_user_workspace() -> Result<(), String> {
    let binding = binding();
    let args = model::terminal(&binding)?;
    assert_eq!(&args[..3], &["--context", "desktop-linux", "exec"]);
    assert!(args.windows(2).any(|w| w == ["--user", "vscode"]));
    assert!(
        args.windows(2)
            .any(|w| w == ["--workdir", "/workspaces/worktree one"])
    );
    assert_eq!(args[args.len() - 3], binding.id);
    let uri = model::editor_uri(&binding)?;
    let parsed = url::Url::parse(&uri).map_err(|e| e.to_string())?;
    assert_eq!(parsed.scheme(), "vscode-remote");
    assert!(parsed.path().contains("worktree%20one"));
    let authority = parsed
        .host_str()
        .ok_or("Missing authority")?
        .strip_prefix("attached-container+")
        .ok_or("Wrong authority")?;
    let bytes = (0..authority.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&authority[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let metadata: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    assert_eq!(metadata["containerName"], binding.id);
    assert_eq!(metadata["settings"]["context"], "desktop-linux");
    Ok(())
}
#[test]
fn configuration_and_engine_gate_are_pure_and_explicit() {
    let config = config();
    let mut changed = config.clone();
    changed.source.push(' ');
    assert!(config.same(&changed).is_err());
    assert!(model::engine("default", "daemon", "unix:///var/run/docker.sock").is_ok());
    assert!(model::engine("remote", "daemon", "ssh://another-host").is_err());
    assert!(model::engine("--bad", "daemon", "unix:///socket").is_err());
    assert!(model::engine("default", "", "unix:///socket").is_err());
}
#[test]
fn refused_ownership_never_dispatches_remove_and_success_is_exactly_once() -> Result<(), String> {
    let binding = binding();
    let calls = std::cell::RefCell::new(Vec::new());
    assert!(
        model::rebuild_checked(
            &binding,
            || Err("wrong daemon".into()),
            |args| {
                calls.borrow_mut().push(args.to_vec());
                Ok(())
            }
        )
        .is_err()
    );
    assert!(calls.borrow().is_empty());
    model::rebuild_checked(
        &binding,
        || model::inspect(&binding, &config(), &binding.engine, &inspection(&binding)),
        |args| {
            calls.borrow_mut().push(args.to_vec());
            Ok(())
        },
    )?;
    assert_eq!(
        calls.borrow().as_slice(),
        &[vec!["rm".to_string(), "--force".into(), binding.id]]
    );
    Ok(())
}
