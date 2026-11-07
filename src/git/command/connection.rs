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
pub(crate) fn connection_read(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut extra = unattended_extra(cfg!(unix) || remote::is_remote(dir));
    extra.timeout = Some(Duration::from_secs(10));
    extra.output_limit = Some(128 << 10);
    let out = execute(Some(dir), &extra, args)?;
    finish(out, args, &[0, 1])
}
pub(crate) fn connection_shell(dir: &Path, script: &str) -> Result<String, String> {
    let extra = Extra {
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
    fn real_git_inspects_and_rejects_only_the_selected_helper_url() -> Result<(), String> {
        let repo = crate::practice::create(crate::practice::Scenario::Detached)?;
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(repo.parent().unwrap().to_owned());
        let record = repo.parent().unwrap().join("helper-record");
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
