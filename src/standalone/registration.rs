//! Explicit named-tool registration. No normal file-tool invocation writes Git configuration.
#[cfg(test)]
use std::path::PathBuf;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug)]
enum Scope {
    Global(Option<std::ffi::OsString>),
    #[cfg(test)]
    File(PathBuf),
}
#[derive(Clone)]
struct Plan {
    entries: Vec<(&'static str, String)>,
}
impl Plan {
    fn new(executable: &Path, defaults: bool) -> Result<Self, String> {
        let path = crate::git::bundle_git_path(executable)?;
        let executable = crate::remote::sh_quote(&path);
        let mut entries = vec![
            (
                "difftool.gitcraft.cmd",
                format!("{executable} --diff -- \"$LOCAL\" \"$REMOTE\""),
            ),
            (
                "mergetool.gitcraft.cmd",
                format!("{executable} --mergetool -- \"$BASE\" \"$LOCAL\" \"$REMOTE\" \"$MERGED\""),
            ),
            ("mergetool.gitcraft.trustExitCode", "true".into()),
        ];
        if defaults {
            entries.extend([
                ("diff.tool", "gitcraft".into()),
                ("merge.tool", "gitcraft".into()),
            ]);
        }
        Ok(Self { entries })
    }
    fn commands(&self) -> String {
        self.entries
            .iter()
            .map(|(key, value)| {
                format!(
                    "git config --global --replace-all {key} {}",
                    crate::remote::sh_quote(value)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
fn install(scope: &Scope, plan: &Plan) -> Result<String, String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    for (i, (key, value)) in plan.entries.iter().enumerate() {
        let mut cmd = std::process::Command::new("git");
        crate::shell_path::apply(&mut cmd);
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                cmd.env_remove(key);
            }
        }
        cmd.current_dir(&temp.0)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CEILING_DIRECTORIES", &temp.0)
            .args(["-c", "core.longpaths=true", "config"]);
        match scope {
            Scope::Global(override_path) => {
                cmd.arg("--global");
                if let Some(path) = override_path {
                    // The isolated command cwd must not change the meaning of
                    // a caller's relative global-config override.
                    cmd.env(
                        "GIT_CONFIG_GLOBAL",
                        std::path::absolute(Path::new(path)).map_err(|e| e.to_string())?,
                    );
                }
                for variable in ["HOME", "XDG_CONFIG_HOME"] {
                    if let Some(path) = std::env::var_os(variable)
                        && Path::new(&path).is_relative()
                    {
                        cmd.env(
                            variable,
                            std::path::absolute(Path::new(&path)).map_err(|e| e.to_string())?,
                        );
                    }
                }
            }
            #[cfg(test)]
            Scope::File(path) => {
                cmd.arg("--file").arg(crate::git::bundle_git_path(path)?);
            }
        }
        cmd.args(["--replace-all", key, value]);
        let out = crate::bounded_process::run(cmd, &|| true, std::time::Duration::from_secs(20))?;
        if !out.status.success() {
            return Err(format!(
                "Installed {i} of {} settings; could not write {key}: {}. Retry or apply the previewed commands manually.",
                plan.entries.len(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    Ok(format!(
        "Registered {} named-tool settings. Use git difftool --tool=gitcraft or git mergetool --tool=gitcraft.",
        plan.entries.len()
    ))
}
#[derive(Clone, Default)]
struct State {
    defaults: bool,
    pending: bool,
    result: Option<Result<String, String>>,
}
pub(crate) fn settings_ui(ui: &mut egui::Ui) {
    match std::env::current_exe() {
        Ok(executable) => panel(
            ui,
            &executable,
            Scope::Global(std::env::var_os("GIT_CONFIG_GLOBAL")),
        ),
        Err(e) => {
            ui.label(format!("Could not find this executable: {e}"));
        }
    }
}
fn panel(ui: &mut egui::Ui, executable: &Path, scope: Scope) {
    let id = egui::Id::new(("standalone-git-registration", format!("{scope:?}")));
    let shared = ui
        .ctx()
        .data_mut(|d| d.get_temp_mut_or_default::<Arc<Mutex<State>>>(id).clone());
    let mut state = shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    ui.label("Writes only the settings below to this machine's global Git config. No repository, profile, merge or file comparison runs.");
    ui.add_enabled_ui(!state.pending, |ui| {
        ui.checkbox(
            &mut state.defaults,
            "Also make gitcraft the default diff and merge tool",
        );
    });
    let plan = match Plan::new(executable, state.defaults) {
        Ok(plan) => plan,
        Err(e) => {
            ui.label(e);
            return;
        }
    };
    let commands = plan.commands();
    egui::ScrollArea::horizontal()
        .id_salt("named-tool-preview")
        .show(ui, |ui| {
            ui.monospace(&commands);
        });
    if ui.button("Copy Git tool commands").clicked() {
        ui.ctx().copy_text(commands);
    }
    let start = ui
        .add_enabled(
            !state.pending,
            egui::Button::new("Register named Git tools"),
        )
        .clicked();
    if state.pending {
        ui.add(crate::spinner::Spinner::new());
        ui.label("Writing the listed Git settings…");
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
    }
    if let Some(result) = &state.result {
        let color = if result.is_ok() {
            crate::theme::Tokens::get(ui.ctx()).success
        } else {
            crate::theme::Tokens::get(ui.ctx()).danger
        };
        ui.colored_label(
            color,
            match result {
                Ok(v) | Err(v) => v,
            },
        );
    }
    let start = {
        let mut current = shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        current.defaults = state.defaults;
        if start && !current.pending {
            current.pending = true;
            current.result = None;
            true
        } else {
            false
        }
    };
    if start {
        let repaint = ui.ctx().clone();
        std::thread::spawn(move || {
            let result = install(&scope, &plan);
            let mut state = shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.pending = false;
            state.result = Some(result);
            repaint.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn get(file: &Path, key: &str) -> Result<String, String> {
        let out = std::process::Command::new("git")
            .args(["config", "--file"])
            .arg(file)
            .args(["--get", key])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("No {key}"));
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().into())
    }
    #[test]
    fn explicit_temp_config_registration_keeps_unrelated_defaults_and_supports_opt_in()
    -> Result<(), String> {
        let temp = crate::git::file_comparison::Scratch::new()?;
        let config = temp.0.join("fixture config");
        std::fs::write(
            &config,
            "[user]\nname = Fixture\n[diff]\ntool = another-tool\n",
        )
        .map_err(|e| e.to_string())?;
        let executable = temp.0.join("gitcraft ' $() & ü");
        let plan = Plan::new(&executable, false)?;
        let context = crate::connections::Context {
            error: Some("Profile must not govern global registration".into()),
            ..Default::default()
        };
        context.run(|| install(&Scope::File(config.clone()), &plan))?;
        assert_eq!(get(&config, "diff.tool")?, "another-tool");
        assert_eq!(get(&config, "user.name")?, "Fixture");
        for (key, value) in plan.entries {
            assert_eq!(get(&config, key)?, value);
        }
        install(&Scope::File(config.clone()), &Plan::new(&executable, true)?)?;
        assert_eq!(get(&config, "diff.tool")?, "gitcraft");
        assert_eq!(get(&config, "merge.tool")?, "gitcraft");
        Ok(())
    }
    #[test]
    fn isolated_global_registration_preserves_relative_override_meaning() -> Result<(), String> {
        const CHILD: &str = "GITCRAFT_RELATIVE_REGISTRATION_FIXTURE";
        const NAME: &str = "standalone::registration::tests::isolated_global_registration_preserves_relative_override_meaning";
        if std::env::var_os(CHILD).is_some() {
            let plan = Plan::new(&std::env::current_exe().map_err(|e| e.to_string())?, false)?;
            install(&Scope::Global(Some("relative config".into())), &plan)?;
            assert_eq!(
                get(Path::new("relative config"), "fixture.untouched")?,
                "keep"
            );
            assert_eq!(
                get(
                    Path::new("relative config"),
                    "mergetool.gitcraft.trustExitCode"
                )?,
                "true"
            );
            return Ok(());
        }
        let temp = crate::git::file_comparison::Scratch::new()?;
        std::fs::write(
            temp.0.join("relative config"),
            "[fixture]\nuntouched = keep\n",
        )
        .map_err(|e| e.to_string())?;
        let out = std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args(["--exact", NAME, "--nocapture"])
            .current_dir(&temp.0)
            .env(CHILD, "1")
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            out.status.success(),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            get(&temp.0.join("relative config"), "fixture.untouched")?,
            "keep"
        );
        Ok(())
    }
    #[test]
    fn registration_settings_do_not_write_until_the_user_clicks_the_concrete_action()
    -> Result<(), String> {
        use egui_kittest::kittest::Queryable;
        let temp = crate::git::file_comparison::Scratch::new()?;
        let config = temp.0.join("fixture config");
        let original = b"[diff]\ntool = previous-tool\n";
        std::fs::write(&config, original).map_err(|e| e.to_string())?;
        let path = config.clone();
        let executable = temp.0.join("gitcraft fixture");
        let mut h = egui_kittest::Harness::builder()
            .with_size([1300., 700.])
            .build_ui(move |ui| panel(ui, &executable, Scope::File(path.clone())));
        h.run_steps(2);
        assert_eq!(std::fs::read(&config).map_err(|e| e.to_string())?, original);
        assert!(
            h.query_by_label_contains("mergetool.gitcraft.trustExitCode")
                .is_some()
        );
        h.get_by_label("Register named Git tools").click();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            h.run_steps(1);
            if h.query_by_label_contains("Registered 3 named-tool settings")
                .is_some()
            {
                break;
            }
            if std::time::Instant::now() > until {
                return Err("Registration did not finish".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(get(&config, "diff.tool")?, "previous-tool");
        assert_eq!(get(&config, "mergetool.gitcraft.trustExitCode")?, "true");
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn registered_shell_commands_pass_literal_native_file_paths_and_propagate_status()
    -> Result<(), String> {
        use std::os::unix::{ffi::OsStringExt, fs::PermissionsExt};
        let temp = crate::git::file_comparison::Scratch::new()?;
        let executable = temp.0.join("gitcraft ' $(touch EXECUTED) & ü");
        std::fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s\\0' \"$@\" >\"$RECEIPT\"\nexit 7\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let plan = Plan::new(&executable, false)?;
        let receipt = temp.0.join("receipt");
        let local = std::ffi::OsString::from_vec(b"local\xff ' $(touch FILE_EXECUTED)".to_vec());
        for (index, expected) in [
            (0, b"--diff\0--\0".as_slice()),
            (1, b"--mergetool\0--\0".as_slice()),
        ] {
            let out = std::process::Command::new("/bin/sh")
                .args(["-c", &plan.entries[index].1])
                .current_dir(&temp.0)
                .env("RECEIPT", &receipt)
                .env("BASE", "base ' $()")
                .env("LOCAL", &local)
                .env("REMOTE", "remote & ;")
                .env("MERGED", "merged ' $()")
                .output()
                .map_err(|e| e.to_string())?;
            assert_eq!(out.status.code(), Some(7));
            let bytes = std::fs::read(&receipt).map_err(|e| e.to_string())?;
            assert!(bytes.starts_with(expected));
            assert!(
                bytes
                    .windows(local.as_encoded_bytes().len())
                    .any(|w| w == local.as_encoded_bytes())
            );
        }
        assert!(!temp.0.join("EXECUTED").exists());
        assert!(!temp.0.join("FILE_EXECUTED").exists());
        Ok(())
    }
}
