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
fn owned(id: Option<String>) -> Result<std::mem::ManuallyDrop<Owned>, String> {
    Ok(std::mem::ManuallyDrop::new(Owned {
        repo: PathBuf::new(),
        plan: plan(
            &format!("sha256:{}", "a".repeat(64)),
            "0123456789abcdef0123456789abcdef",
        )?,
        id,
        removed: false,
    }))
}
#[test]
fn r2_pure_launch_uses_immutable_ids_explicit_exit_code_and_bounded_control() -> Result<(), String>
{
    let id = "a".repeat(64);
    let mut item = owned(None)?;
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
fn r2_pure_interrupted_launch_and_foreign_created_owner_never_start_unowned_code()
-> Result<(), String> {
    let id = "a".repeat(64);
    let mut item = owned(None)?;
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
    let mut item = owned(None)?;
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
    Ok(())
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
fn r2_pure_cleanup_removes_owned_id_even_when_name_is_replaced() -> Result<(), String> {
    let id = "a".repeat(64);
    let foreign = "b".repeat(64);
    let mut item = owned(Some(id.clone()))?;
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
    Ok(())
}
#[test]
fn r2_pure_cleanup_absent_mismatch_interrupted_creation_and_failure_are_recorded()
-> Result<(), String> {
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
        let mut item = owned(Some(id.clone()))?;
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
    let mut item = owned(None)?;
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
    let mut item = owned(None)?;
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
    Ok(())
}
