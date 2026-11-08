//! Fixed DVC transfer command routed through Git's portable shell and operation runner.
pub(crate) const PULL_ALIAS: &str = "gitcraft-dvc-pull";
pub(crate) fn pull_steps() -> Vec<Vec<String>> {
    vec![vec!["-c".into(), "alias.gitcraft-dvc-pull=!f() { command -v dvc >/dev/null 2>&1 || { echo 'DVC is not installed on this repository machine' >&2; exit 127; }; exec dvc pull; }; f".into(), PULL_ALIAS.into()]]
}
#[cfg(test)]
mod tests {
    #[test]
    fn pull_is_fixed_argv_and_uses_network_operation_guards() {
        let steps = super::pull_steps();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].last().map(String::as_str), Some(super::PULL_ALIAS));
        assert!(crate::git::goes_over_network(
            &steps[0].iter().map(String::as_str).collect::<Vec<_>>()
        ));
        assert!(crate::repo::operations::reaches_a_remote(&steps[0]));
    }
    #[cfg(unix)]
    #[test]
    fn pull_executes_on_the_repository_machine_with_fixed_arguments_and_exit_status()
    -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let repo = crate::git::file_comparison::tests::Repo::new()?;
        let bin = repo.0.join("fixture tools");
        std::fs::create_dir(&bin).map_err(|e| e.to_string())?;
        let program = bin.join("dvc");
        std::fs::write(&program,b"#!/bin/sh\n[ \"$#\" -eq 1 ] && [ \"$1\" = pull ] || exit 91\nprintf '%s' \"$PWD\" > \"$GITCRAFT_DVC_MARKER\"\nexit \"$GITCRAFT_DVC_EXIT\"\n").map_err(|e|e.to_string())?;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let mut paths = vec![bin];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(paths).map_err(|e| e.to_string())?;
        let marker = repo.0.join("recorded repository cwd");
        let plan = super::pull_steps();
        let args = plan[0].iter().map(String::as_str).collect::<Vec<_>>();
        for code in [0, 17] {
            let mut command = repo.command(&args);
            command
                .env("PATH", &path)
                .env("GITCRAFT_DVC_MARKER", &marker)
                .env("GITCRAFT_DVC_EXIT", code.to_string());
            let output = crate::processes::run(
                command,
                crate::processes::Options {
                    timeout: Some(std::time::Duration::from_secs(5)),
                    output_limit: 1024,
                    stderr_limit: Some(1024),
                    ..Default::default()
                },
            )?
            .complete()?;
            assert_eq!(output.status.code(), Some(code));
            assert_eq!(
                std::fs::read_to_string(&marker).map_err(|e| e.to_string())?,
                repo.0.to_string_lossy()
            );
        }
        Ok(())
    }
}
