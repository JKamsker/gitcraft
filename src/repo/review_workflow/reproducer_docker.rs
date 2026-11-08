//! Docker control uses bounded shared process/remote APIs. Names are discovery hints only:
//! every start/removal uses a validated immutable container ID and an exact owner label.
use super::{Case, Observation, materialize};
use crate::{remote, tasks};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
const CONTROL: Duration = Duration::from_secs(15);
const RUN: Duration = Duration::from_secs(135);
#[derive(Clone)]
struct Plan {
    name: String,
    token: String,
    create: Vec<String>,
}
#[derive(Default)]
struct Answer {
    code: Option<i32>,
    out: Vec<u8>,
    err: Vec<u8>,
    stopped: Option<String>,
}
impl Answer {
    fn require(self, what: &str) -> Result<Vec<u8>, String> {
        if let Some(reason) = self.stopped {
            return Err(format!("{what}: {reason}"));
        }
        if self.code == Some(0) {
            Ok(self.out)
        } else {
            Err(format!(
                "{what}: {}",
                String::from_utf8_lossy(&self.err).trim()
            ))
        }
    }
}
trait Control {
    fn call(
        &mut self,
        args: &[String],
        input: Option<Vec<u8>>,
        timeout: Duration,
    ) -> Result<Answer, String>;
}
struct Docker<'a>(&'a Path);
impl Control for Docker<'_> {
    fn call(
        &mut self,
        args: &[String],
        input: Option<Vec<u8>>,
        timeout: Duration,
    ) -> Result<Answer, String> {
        if let Some(target) = remote::parse(self.0) {
            let words = args
                .iter()
                .map(|s| remote::sh_quote(s))
                .collect::<Vec<_>>()
                .join(" ");
            let out = remote::exec(
                &target.host,
                &remote::Request {
                    cwd: Some(target.path),
                    script: format!("exec docker {words}"),
                    stdin: input,
                    timeout: Some(timeout),
                    apart: true,
                    ..Default::default()
                },
            )?;
            Ok(Answer {
                code: Some(out.code),
                out: out.stdout,
                err: out.stderr,
                stopped: None,
            })
        } else {
            let mut command = std::process::Command::new(
                crate::shell_path::executable("docker").unwrap_or_else(|| "docker".into()),
            );
            command.args(args).current_dir(self.0);
            crate::shell_path::apply(&mut command);
            let out = crate::processes::run(
                command,
                crate::processes::Options {
                    input,
                    timeout: Some(timeout),
                    background: true,
                    output_limit: 4 * 1024 * 1024,
                    ..Default::default()
                },
            )?;
            Ok(Answer {
                code: out.status.code(),
                out: out.stdout,
                err: out.stderr,
                stopped: out.stopped,
            })
        }
    }
}
fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).into()).collect()
}
fn identity(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
fn plan(image: &str, token: &str) -> Result<Plan, String> {
    if !image.strip_prefix("sha256:").is_some_and(identity)
        || token.len() != 32
        || !token.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Invalid immutable image/owner identity".into());
    }
    let name = format!("gitcraft-reproducer-{token}");
    let body = "set -eu; mkdir -p /work/project /work/cargo; cd /work/project; tar --no-same-owner --no-same-permissions -xf -; for cache in registry git; do if [ -d /usr/local/cargo/$cache ]; then cp -R /usr/local/cargo/$cache /work/cargo/; fi; done; exec cargo test --offline --test gitcraft_reproducer -- --nocapture";
    // Image ENTRYPOINT cannot replace this command. Tool checks precede all repository
    // code; the timeout supervises extraction, build scripts and tests in the PID namespace.
    let inside = format!(
        "set -eu; for tool in timeout tar cargo mkdir cp; do command -v \"$tool\" >/dev/null || {{ echo \"Sandbox image lacks $tool\" >&2; exit 125; }}; done; exec timeout --signal=TERM --kill-after=5s 120s /bin/sh -c {}",
        remote::sh_quote(body)
    );
    let mut create = words(&[
        "create",
        "--pull=never",
        "--name",
        &name,
        "--label",
        &format!("gitcraft.owner={token}"),
        "--network=none",
        "--read-only",
        "--user=65534:65534",
        "--cap-drop=ALL",
        "--security-opt=no-new-privileges",
        "--pids-limit=128",
        "--memory=1g",
        "--cpus=2",
        "--tmpfs",
        "/work:rw,exec,nosuid,nodev,size=1g,mode=1777",
        "--tmpfs",
        "/tmp:rw,exec,nosuid,nodev,size=64m,mode=1777",
        "--env",
        "CARGO_HOME=/work/cargo",
        "--env",
        "RUSTUP_HOME=/usr/local/rustup",
        "--entrypoint",
        "/bin/sh",
        "-i",
        image,
        "-c",
    ]);
    create.push(inside);
    Ok(Plan {
        name,
        token: token.into(),
        create,
    })
}
fn inspect(control: &mut impl Control, target: &str) -> Result<Option<(String, String)>, String> {
    let answer = control.call(
        &words(&[
            "inspect",
            "--type",
            "container",
            "--format",
            "{{.Id}} {{ index .Config.Labels \"gitcraft.owner\" }}",
            target,
        ]),
        None,
        CONTROL,
    )?;
    if answer.stopped.is_some() {
        return Err("Container ownership inspection was interrupted".into());
    }
    if answer.code != Some(0) {
        let error = String::from_utf8_lossy(&answer.err);
        if error.contains("No such object") || error.contains("No such container") {
            return Ok(None);
        }
        return Err(format!(
            "Container ownership inspection failed: {}",
            error.trim()
        ));
    }
    let text = String::from_utf8(answer.out).map_err(|_| "Invalid container ownership response")?;
    let fields: Vec<_> = text.split_whitespace().collect();
    match fields.as_slice() {
        [id, owner] if identity(id) => Ok(Some(((*id).into(), (*owner).into()))),
        _ => Err("Invalid immutable container ownership response".into()),
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Cleanup {
    pub name: String,
    pub id: Option<String>,
    pub result: String,
}
struct Owned {
    repo: PathBuf,
    plan: Plan,
    id: Option<String>,
    removed: bool,
}
impl Owned {
    fn cleanup_with(&mut self, control: &mut impl Control) -> Cleanup {
        let result = (|| {
            // During interrupted creation a name may discover the candidate. From here on
            // both owner validation and deletion address its immutable ID, never that name.
            let candidate = if let Some(id) = &self.id {
                Some(id.clone())
            } else {
                match inspect(control, &self.plan.name)? {
                    Some((id, owner)) if owner == self.plan.token => Some(id),
                    Some(_) => return Err("Name belongs to another owner; cleanup refused".into()),
                    None => return Err("Creation outcome is unknown; no owned ID was found. A stopped container may remain if creation finished after inspection; no removal by name was attempted.".into()),
                }
            };
            let id = candidate.ok_or("No immutable container ID")?;
            if !identity(&id) {
                return Err("Invalid cleanup ID".into());
            }
            self.id = Some(id.clone());
            match inspect(control, &id)? {
                None => {
                    self.removed = true;
                    return Ok("Container was already absent".into());
                }
                Some((seen, owner)) if seen == id && owner == self.plan.token => {}
                Some(_) => {
                    return Err("Immutable container owner/ID mismatch; cleanup refused".into());
                }
            }
            control
                .call(&words(&["rm", "--force", "--volumes", &id]), None, CONTROL)?
                .require("Container cleanup")?;
            self.removed = true;
            Ok("Owned container removed by immutable ID".into())
        })();
        Cleanup {
            name: self.plan.name.clone(),
            id: self.id.clone(),
            result: result.unwrap_or_else(|e: String| format!("Cleanup needs attention: {e}")),
        }
    }
}
fn preserve_cleanup(repo: &Path, cleanup: &Cleanup) {
    if let Err(error) = materialize::save(repo, "reproducer-cleanup", &cleanup.name, cleanup)
        .and_then(|_| materialize::save(repo, "reproducer-cleanup", "latest", cleanup))
    {
        eprintln!(
            "Could not preserve reproducer cleanup evidence for {}: {error}; {}",
            cleanup.name, cleanup.result
        );
    }
}
impl Drop for Owned {
    fn drop(&mut self) {
        if self.removed {
            return;
        }
        // Owned is created only inside the repository worker. Even unwinding cleanup is
        // bounded and recorded, with a token independent from the cancelled execution.
        let repo = self.repo.clone();
        tasks::with_cancellation(tasks::Cancellation::default(), || {
            let evidence = self.cleanup_with(&mut Docker(&repo));
            preserve_cleanup(&repo, &evidence);
        });
        self.removed = true;
    }
}
pub(super) fn image(repo: &Path, image: &str) -> Result<String, String> {
    if image.is_empty()
        || image.starts_with('-')
        || !image
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:@-".contains(c))
    {
        return Err("Choose a preloaded Rust image by name or digest".into());
    }
    let out = Docker(repo)
        .call(
            &words(&["image", "inspect", "--format", "{{.Id}}", image]),
            None,
            CONTROL,
        )?
        .require("Inspect preloaded image")?;
    let out = String::from_utf8(out).map_err(|_| "Invalid image ID")?;
    let id = out.trim();
    if !id.strip_prefix("sha256:").is_some_and(identity) {
        return Err("Selected image is unavailable locally or did not return an immutable SHA256 ID; images are never pulled".into());
    }
    Ok(id.into())
}
fn launch(
    control: &mut impl Control,
    owned: &mut Owned,
    payload: Vec<u8>,
) -> Result<Answer, String> {
    let created = control
        .call(&owned.plan.create, None, CONTROL)?
        .require("Create sandbox")?;
    let id = String::from_utf8(created).map_err(|_| "Invalid container creation response")?;
    let id = id.trim();
    if !identity(id) {
        return Err("Docker did not return a complete immutable container ID".into());
    }
    owned.id = Some(id.into());
    match inspect(control, id)? {
        Some((seen, owner)) if seen == id && owner == owned.plan.token => {}
        _ => {
            return Err(
                "Created container ownership could not be established; execution refused".into(),
            );
        }
    }
    let mut answer = control.call(
        &words(&["start", "--attach", "--interactive", id]),
        Some(payload),
        RUN,
    )?;
    if answer.stopped.is_none() {
        // Docker start's CLI status is not the test's status. Read the exited container's
        // status explicitly and retain attach output as the observed build/test output.
        let waited = control
            .call(&words(&["wait", id]), None, CONTROL)?
            .require("Read sandbox exit status")?;
        let code = std::str::from_utf8(&waited)
            .map_err(|_| "Invalid sandbox exit status")?
            .trim()
            .parse::<i32>()
            .map_err(|_| "Invalid sandbox exit status")?;
        if !(0..=255).contains(&code) {
            return Err("Sandbox exit status is outside Docker's range".into());
        }
        answer.code = Some(code);
    }
    Ok(answer)
}
pub(super) fn run_one(
    repo: &Path,
    rev: &str,
    tree: &str,
    case: &Case,
    token: &str,
) -> Result<(Observation, Cleanup), String> {
    let payload = materialize::archive(repo, rev, &case.source)?;
    let plan = plan(&case.image, token)?;
    let mut owned = Owned {
        repo: repo.into(),
        plan,
        id: None,
        removed: false,
    };
    let outcome = launch(&mut Docker(repo), &mut owned, payload);
    let cleanup = tasks::with_cancellation(tasks::Cancellation::default(), || {
        let cleanup = owned.cleanup_with(&mut Docker(repo));
        preserve_cleanup(repo, &cleanup);
        cleanup
    });
    // A failed cleanup is evidence for manual follow-up, not an unbounded Drop retry loop.
    owned.removed = true;
    let answer = outcome.unwrap_or_else(|error: String| Answer {
        stopped: Some(error),
        ..Default::default()
    });
    let mut lines: Vec<String> = String::from_utf8_lossy(&answer.out)
        .lines()
        .chain(String::from_utf8_lossy(&answer.err).lines())
        .map(str::to_owned)
        .take(10_000)
        .collect();
    if let Some(error) = &answer.stopped {
        lines.push(format!("Sandbox did not complete: {error}"));
    }
    let observation = Observation {
        commit: rev.into(),
        tree: tree.into(),
        command: format!(
            "Docker {} · cargo test --offline --test gitcraft_reproducer",
            case.image
        ),
        code: answer.code,
        stopped: answer.stopped.is_some() || tasks::cancellation().is_cancelled(),
        lines,
        machine: remote::parse(repo).map_or_else(|| "local".into(), |t| t.host.label()),
    };
    Ok((observation, cleanup))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Recording {
        replies: std::collections::VecDeque<Answer>,
        calls: Vec<(Vec<String>, Duration, Option<Vec<u8>>)>,
    }
    impl Control for Recording {
        fn call(
            &mut self,
            args: &[String],
            input: Option<Vec<u8>>,
            timeout: Duration,
        ) -> Result<Answer, String> {
            self.calls.push((args.to_vec(), timeout, input));
            self.replies.pop_front().ok_or("No recorded answer".into())
        }
    }
    fn reply(code: i32, text: &str, error: &str) -> Answer {
        Answer {
            code: Some(code),
            out: text.as_bytes().to_vec(),
            err: error.as_bytes().to_vec(),
            stopped: None,
        }
    }
    fn owned(id: Option<String>) -> std::mem::ManuallyDrop<Owned> {
        std::mem::ManuallyDrop::new(Owned {
            repo: PathBuf::new(),
            plan: plan(
                &format!("sha256:{}", "a".repeat(64)),
                "0123456789abcdef0123456789abcdef",
            )
            .expect("plan"),
            id,
            removed: false,
        })
    }
    #[test]
    fn r2_pure_launch_uses_immutable_ids_explicit_exit_code_and_bounded_control()
    -> Result<(), String> {
        let id = "a".repeat(64);
        let mut item = owned(None);
        let mut recording = Recording {
            replies: [
                reply(0, &id, ""),
                reply(0, &format!("{id} {}", item.plan.token), ""),
                reply(0, "test output", ""),
                reply(0, "101\n", ""),
            ]
            .into(),
            calls: vec![],
        };
        let answer = launch(&mut recording, &mut item, b"raw archive".to_vec())?;
        assert_eq!(answer.code, Some(101));
        assert_eq!(answer.out, b"test output");
        assert_eq!(recording.calls[0].1, CONTROL);
        assert_eq!(recording.calls[1].0.last(), Some(&id));
        assert_eq!(
            recording.calls[2].0,
            words(&["start", "--attach", "--interactive", &id])
        );
        assert_eq!(recording.calls[2].1, RUN);
        assert_eq!(recording.calls[2].2.as_deref(), Some(&b"raw archive"[..]));
        assert_eq!(recording.calls[3].0, words(&["wait", &id]));
        assert_eq!(recording.calls[3].1, CONTROL);
        Ok(())
    }
    #[test]
    fn r2_pure_interrupted_launch_and_foreign_created_owner_never_start_unowned_code() {
        let id = "a".repeat(64);
        let mut item = owned(None);
        let mut recording = Recording {
            replies: [Answer {
                stopped: Some("cancelled/timeout".into()),
                ..Default::default()
            }]
            .into(),
            calls: vec![],
        };
        assert!(launch(&mut recording, &mut item, vec![]).is_err());
        assert_eq!(recording.calls.len(), 1);
        let mut item = owned(None);
        let mut recording = Recording {
            replies: [reply(0, &id, ""), reply(0, &format!("{id} foreign"), "")].into(),
            calls: vec![],
        };
        assert!(launch(&mut recording, &mut item, vec![]).is_err());
        assert_eq!(recording.calls.len(), 2);
        assert!(
            !recording
                .calls
                .iter()
                .any(|(args, _, _)| args[0] == "start")
        );
    }
    #[test]
    fn r2_pure_recorded_container_plan_overrides_entrypoint_and_deadlines_cover_repository_code()
    -> Result<(), String> {
        let p = plan(
            &format!("sha256:{}", "a".repeat(64)),
            "0123456789abcdef0123456789abcdef",
        )?;
        for flag in [
            "--network=none",
            "--read-only",
            "--user=65534:65534",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--pids-limit=128",
            "--memory=1g",
            "--pull=never",
        ] {
            assert!(p.create.iter().any(|s| s == flag));
        }
        assert!(
            !p.create
                .iter()
                .any(|s| s == "--mount" || s == "--volume" || s == "run")
        );
        let at = p
            .create
            .iter()
            .position(|s| s == "--entrypoint")
            .ok_or("entrypoint")?;
        assert_eq!(p.create[at + 1], "/bin/sh");
        let body = p.create.last().ok_or("body")?;
        assert!(body.contains("command -v"));
        assert!(body.contains("exec timeout --signal=TERM --kill-after=5s 120s"));
        assert!(body.contains("tar --no-same-owner --no-same-permissions -xf -"));
        assert!(body.contains("cargo test --offline"));
        assert!(plan("rust:mutable", "0123456789abcdef0123456789abcdef").is_err());
        Ok(())
    }
    #[test]
    fn r2_pure_cleanup_removes_owned_id_even_when_name_is_replaced() {
        let id = "a".repeat(64);
        let foreign = "b".repeat(64);
        let mut item = owned(Some(id.clone()));
        let mut recording = Recording {
            replies: [
                reply(0, &format!("{id} {}", item.plan.token), ""),
                reply(0, "removed", ""),
            ]
            .into(),
            calls: vec![],
        };
        let result = item.cleanup_with(&mut recording);
        assert!(item.removed);
        assert_eq!(result.id, Some(id.clone()));
        assert_eq!(recording.calls[1].0.last(), Some(&id));
        assert!(
            !recording
                .calls
                .iter()
                .any(|(args, _, _)| args.contains(&item.plan.name) || args.contains(&foreign))
        );
        assert!(
            recording
                .calls
                .iter()
                .all(|(_, duration, _)| *duration == CONTROL)
        );
    }
    #[test]
    fn r2_pure_cleanup_absent_mismatch_interrupted_creation_and_failure_are_recorded() {
        let id = "a".repeat(64);
        for (answer, expected, removed) in [
            (reply(1, "", "No such container"), "already absent", true),
            (
                reply(0, &format!("{id} other-owner"), ""),
                "mismatch",
                false,
            ),
            (
                reply(1, "", "daemon unavailable"),
                "inspection failed",
                false,
            ),
        ] {
            let mut item = owned(Some(id.clone()));
            let mut recording = Recording {
                replies: [answer].into(),
                calls: vec![],
            };
            let result = item.cleanup_with(&mut recording);
            assert!(result.result.contains(expected), "{}", result.result);
            assert_eq!(item.removed, removed);
            assert_eq!(recording.calls.len(), 1);
            item.removed = true; // Pure recording cases never submit a real cleanup job.
        }
        let mut item = owned(None);
        let mut recording = Recording {
            replies: [
                reply(0, &format!("{id} {}", item.plan.token), ""),
                reply(0, &format!("{id} {}", item.plan.token), ""),
                reply(1, "", "rm failed"),
            ]
            .into(),
            calls: vec![],
        };
        let result = item.cleanup_with(&mut recording);
        assert!(result.result.contains("rm failed"));
        assert_eq!(recording.calls[0].0.last(), Some(&item.plan.name));
        assert_eq!(recording.calls[1].0.last(), Some(&id));
        assert_eq!(recording.calls[2].0.last(), Some(&id));
        item.removed = true;
        let mut item = owned(None);
        let mut recording = Recording {
            replies: [reply(1, "", "No such container")].into(),
            calls: vec![],
        };
        assert!(
            item.cleanup_with(&mut recording)
                .result
                .contains("Creation outcome is unknown")
        );
        assert_eq!(recording.calls.len(), 1);
        item.removed = true;
    }
}
