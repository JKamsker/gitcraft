//! Bounded, unattended connection checks using the repository's real transport context.
use super::*;
use std::time::{Duration, Instant};

pub(crate) struct ConnectionProbe {
    pub success: bool,
    pub trace: String,
    pub elapsed: Duration,
}
fn probe_extra(dir: &Path) -> Extra {
    let mut extra = unattended_extra(cfg!(unix) || remote::is_remote(dir));
    extra.normalize_repository = true;
    extra.timeout = Some(Duration::from_secs(45));
    extra.output_limit = Some(2 << 20);
    extra.env.extend([
        ("GIT_TRACE_CURL", "1".into()),
        ("GIT_TRACE_CURL_NO_DATA", "1".into()),
        ("GIT_TRACE_REDACT", "1".into()),
        ("GCM_INTERACTIVE", "never".into()),
    ]);
    extra
}
pub(crate) fn connection_probe(dir: &Path, url: &str) -> Result<ConnectionProbe, String> {
    if let Some(pause) = crate::network::refused() {
        return Err(pause.why().into());
    }
    let started = Instant::now();
    let out = execute(Some(dir), &probe_extra(dir), &["ls-remote", "--", url])?;
    Ok(ConnectionProbe {
        success: out.code == Some(0),
        trace: String::from_utf8_lossy(&out.stderr).into_owned(),
        elapsed: started.elapsed(),
    })
}
/// Captured exit codes, bounded output/deadline, selected repository and captured profile.
/// Callers interpreting exit 1 must not fall back to unsanitized run_captured.
pub(crate) fn connection_captured(
    dir: &Path,
    args: &[&str],
    input: Option<&[u8]>,
) -> Result<Captured, String> {
    let mut extra = unattended_extra(cfg!(unix) || remote::is_remote(dir));
    extra.normalize_repository = true;
    extra.stdin = input.map(<[u8]>::to_vec);
    extra.timeout = Some(Duration::from_secs(10));
    extra.output_limit = Some(128 << 10);
    let out = execute(Some(dir), &extra, args)?;
    Ok(Captured {
        code: out.code,
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        raw: out.stdout,
    })
}
pub(crate) fn connection_read(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = connection_captured(dir, args, None)?;
    if matches!(out.code, Some(0 | 1)) {
        Ok(out.stdout)
    } else {
        Err(crate::diagnostics::crash::scrub(&out.stderr))
    }
}
pub(crate) fn connection_shell(dir: &Path, script: &str) -> Result<String, String> {
    let extra = Extra {
        normalize_repository: true,
        pre: vec!["-c", SHELL_ALIAS_ENV_DEF],
        env: vec![(SHELL_SCRIPT_VAR, script.into())],
        timeout: Some(Duration::from_secs(10)),
        output_limit: Some(128 << 10),
        ..Default::default()
    };
    finish(
        execute(Some(dir), &extra, &[SHELL_ALIAS])?,
        &[SHELL_ALIAS],
        &[0],
    )
}
/// Returns only an account name, never retaining a helper's password in view state.
pub(crate) struct HttpsAccount {
    pub username: Option<String>,
    pub helpers: Vec<String>,
}
fn helper_trace(trace: &str) -> Vec<String> {
    trace
        .lines()
        .filter(|line| line.contains("run_command:"))
        .map(|line| {
            [
                "manager-core",
                "manager",
                "osxkeychain",
                "libsecret",
                "wincred",
                "cache",
                "store",
            ]
            .into_iter()
            .find(|name| {
                line.contains(&format!("credential-{name} "))
                    || line.contains(&format!("credential-{name}\" "))
            })
            .map(|name| name.to_owned())
            .unwrap_or_else(|| "custom or profile helper (command withheld)".into())
        })
        .collect()
}
pub(crate) fn https_account(dir: &Path, url: &str) -> Result<HttpsAccount, String> {
    let mut extra = unattended_extra(cfg!(unix) || remote::is_remote(dir));
    extra.normalize_repository = true;
    extra.env.extend([
        ("GCM_INTERACTIVE", "never".into()),
        ("GIT_TRACE", "1".into()),
    ]);
    extra.stdin = Some(format!("url={url}\n\n").into_bytes());
    extra.timeout = Some(Duration::from_secs(15));
    extra.output_limit = Some(128 << 10);
    let out = execute(Some(dir), &extra, &["credential", "fill"])?;
    let helpers = helper_trace(&String::from_utf8_lossy(&out.stderr));
    let username = (out.code == Some(0))
        .then(|| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .find_map(|l| l.strip_prefix("username=").map(str::to_owned))
        })
        .flatten();
    Ok(HttpsAccount { username, helpers })
}
pub(crate) fn https_reject(dir: &Path, url: &str) -> Result<(), String> {
    let mut extra = unattended_extra(cfg!(unix) || remote::is_remote(dir));
    extra.normalize_repository = true;
    extra.stdin = Some(format!("url={url}\n\n").into_bytes());
    extra.timeout = Some(Duration::from_secs(15));
    extra.output_limit = Some(64 << 10);
    finish(
        execute(Some(dir), &extra, &["credential", "reject"])?,
        &["credential", "reject"],
        &[0],
    )
    .map(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalized_connection_commands_remove_repository_redirects_and_keep_the_selected_profile() {
        let profile = crate::connections::Context {
            git_config: Some("/synthetic/profile.gitconfig".into()),
            ..Default::default()
        };
        profile.run(|| {
            let cmd = apply_command_extras(
                Command::new("/recorded/git"),
                Some(Path::new("/synthetic/repository")),
                &probe_extra(Path::new("/synthetic/repository")),
                &["remote"],
            );
            let env: std::collections::HashMap<_, _> = cmd
                .get_envs()
                .map(|(k, v)| {
                    (
                        k.to_string_lossy().into_owned(),
                        v.map(|v| v.to_string_lossy().into_owned()),
                    )
                })
                .collect();
            for key in [
                "GIT_DIR",
                "GIT_WORK_TREE",
                "GIT_CONFIG_COUNT",
                "GIT_CONFIG_PARAMETERS",
            ] {
                assert_eq!(env.get(key), Some(&None));
            }
            assert_eq!(
                env.get("GIT_CONFIG_GLOBAL"),
                Some(&Some("/synthetic/profile.gitconfig".into()))
            );
        });
    }
    #[test]
    fn all_push_urls_are_inventoried_and_only_the_selected_failed_push_is_rejected()
    -> Result<(), String> {
        let repo = crate::practice::create(crate::practice::Scenario::Detached)?;
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let root = repo.parent().ok_or("No owned root")?;
        let _cleanup = Cleanup(root.to_owned());
        run(
            Some(&repo),
            &["remote", "add", "origin", "https://fetch.invalid/team/repo"],
        )?;
        for url in [
            "https://push-a.invalid/team/repo",
            "https://push-b.invalid/team/repo",
        ] {
            run(
                Some(&repo),
                &["config", "--local", "--add", "remote.origin.pushurl", url],
            )?;
        }
        let endpoints = remote_endpoints(&repo)?;
        assert_eq!(
            endpoints.iter().map(|e| e.key()).collect::<Vec<_>>(),
            ["origin · fetch 1", "origin · push 1", "origin · push 2"]
        );
        let attempted = operation_endpoints(&repo, &["push", "origin", "HEAD:main"])?;
        assert_eq!(
            attempted,
            [
                "https://push-a.invalid/team/repo",
                "https://push-b.invalid/team/repo"
            ]
        );
        let failure = AuthFailure::from_error(
            "fatal: Authentication failed for 'https://push-b.invalid/team/repo'",
            &attempted,
        );
        let record = root.join("push-helper-record");
        let quoted = crate::remote::sh_quote(&record.to_string_lossy());
        let helper = format!(
            "!f() {{ printf '%s\\n' \"$1\" >> {quoted}; while IFS= read -r line && test -n \"$line\"; do case \"$line\" in host=*|path=*) printf '%s\\n' \"$line\" >> {quoted};; esac; done; if test \"$1\" = get; then printf 'username=dummy-push-b\\npassword=dummy-token\\n'; fi; }}; f"
        );
        run(Some(&repo), &["config", "--local", "credential.helper", ""])?;
        run(
            Some(&repo),
            &["config", "--local", "--add", "credential.helper", &helper],
        )?;
        run(
            Some(&repo),
            &["config", "--local", "credential.useHttpPath", "true"],
        )?;
        assert_eq!(
            https_account(&repo, &failure.endpoints[0])?
                .username
                .as_deref(),
            Some("dummy-push-b")
        );
        https_reject(&repo, &failure.endpoints[0])?;
        let log = std::fs::read_to_string(record).map_err(|e| e.to_string())?;
        assert!(
            log.contains("get\nhost=push-b.invalid\npath=team/repo"),
            "{log}"
        );
        assert!(
            log.contains("erase\nhost=push-b.invalid\npath=team/repo"),
            "{log}"
        );
        assert!(
            !log.contains("fetch.invalid")
                && !log.contains("push-a.invalid")
                && !log.contains("dummy-token")
        );
        Ok(())
    }
    #[test]
    fn real_git_inspects_and_rejects_only_the_selected_helper_url() -> Result<(), String> {
        let repo = crate::practice::create(crate::practice::Scenario::Detached)?;
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let root = repo.parent().ok_or("No owned credential fixture root")?;
        let _cleanup = Cleanup(root.to_owned());
        let record = root.join("helper-record");
        let quoted = crate::remote::sh_quote(&record.to_string_lossy());
        let helper = format!(
            "!f() {{ printf '%s\\n' \"$1\" >> {quoted}; while IFS= read -r line && test -n \"$line\"; do case \"$line\" in host=*|path=*) printf '%s\\n' \"$line\" >> {quoted};; esac; done; if test \"$1\" = get; then printf 'username=fixture-account\\npassword=fixture-secret\\n'; fi; }}; f"
        );
        // Reset inherited helpers and use only this recording fixture; no host is contacted.
        run(Some(&repo), &["config", "--local", "credential.helper", ""])?;
        run(
            Some(&repo),
            &["config", "--local", "--add", "credential.helper", &helper],
        )?;
        run(
            Some(&repo),
            &["config", "--local", "credential.useHttpPath", "true"],
        )?;
        let account = https_account(&repo, "https://example.invalid/team/repo")?;
        assert_eq!(account.username.as_deref(), Some("fixture-account"));
        https_reject(&repo, "https://example.invalid/team/repo")?;
        let log = std::fs::read_to_string(record).map_err(|e| e.to_string())?;
        assert!(
            log.contains("get\nhost=example.invalid\npath=team/repo"),
            "{log}"
        );
        assert!(
            log.contains("erase\nhost=example.invalid\npath=team/repo"),
            "{log}"
        );
        assert!(!log.contains("fixture-secret"));
        Ok(())
    }
    #[test]
    fn helper_trace_identifies_execution_without_command_secrets() {
        assert_eq!(
            helper_trace(
                "trace: run_command: 'git credential-manager get'\ntrace: run_command: 'echo supersecret get'"
            ),
            vec!["manager", "custom or profile helper (command withheld)"]
        );
    }
    #[test]
    fn probe_uses_real_context_without_payload_or_prompts() {
        let cmd = local_command(
            Some(Path::new("/tmp/repository")),
            &probe_extra(Path::new("/tmp/repository")),
            &["ls-remote", "--", "https://example.org/r.git"],
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.ends_with(&[
            "ls-remote".into(),
            "--".into(),
            "https://example.org/r.git".into()
        ]));
        let vars: Vec<_> = cmd
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        for key in ["GIT_TRACE_CURL_NO_DATA", "GIT_TRACE_REDACT"] {
            assert!(vars.contains(&(key.into(), Some("1".into()))));
        }
        assert!(vars.contains(&("GCM_INTERACTIVE".into(), Some("never".into()))));
        assert_eq!(
            probe_extra(Path::new("/tmp/repository")).timeout,
            Some(Duration::from_secs(45))
        );
    }
}
