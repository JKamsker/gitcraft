use super::*;
use serde_json::Map;

fn args(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn op(make: fn(&Args) -> Result<WriteOp, String>, value: Value) -> Result<WriteOp, String> {
    make(&Args::new(&args(value)))
}

#[test]
fn each_tool_reads_its_arguments_into_one_operation() {
    assert_eq!(
        op(stage_op, json!({"paths": ["a", "b/c"]})),
        Ok(WriteOp::Stage {
            paths: vec!["a".into(), "b/c".into()]
        })
    );
    assert_eq!(
        op(commit_op, json!({"message": " Fix \n", "amend": true})),
        Ok(WriteOp::Commit {
            message: "Fix".into(),
            amend: true
        })
    );
    assert_eq!(
        op(create_branch_op, json!({"name": "f/x", "start": "HEAD~1"})),
        Ok(WriteOp::CreateBranch {
            name: "f/x".into(),
            start: Some("HEAD~1".into()),
            checkout: false
        })
    );
    assert_eq!(
        op(reset_op, json!({"commit": "HEAD~1"})),
        Ok(WriteOp::Reset {
            commit: "HEAD~1".into(),
            mode: ResetMode::Mixed
        })
    );
    assert_eq!(
        op(
            move_commit_op,
            json!({"commit": "abc", "direction": "down"})
        ),
        Ok(WriteOp::MoveCommit {
            commit: "abc".into(),
            up: false
        })
    );
    assert_eq!(op(push_op, json!({})), Ok(WriteOp::Push { force: false }));
    assert_eq!(
        op(stash_op, json!({})),
        Ok(WriteOp::Stash {
            message: String::new()
        })
    );
}

#[test]
fn wrong_arguments_are_refused_before_anything_is_sent() {
    let refused =
        |make: fn(&Args) -> Result<WriteOp, String>, value: Value| op(make, value).is_err();
    assert!(refused(stage_op, json!({"paths": ["../x"]})));
    assert!(refused(stage_op, json!({"paths": "a"})));
    assert!(refused(unstage_op, json!({})));
    assert!(refused(commit_op, json!({"message": "  "})));
    assert!(refused(commit_op, json!({"message": "m", "all": true})));
    assert!(refused(create_branch_op, json!({"name": "-D"})));
    assert!(refused(
        create_branch_op,
        json!({"name": "x", "start": "--orphan"})
    ));
    assert!(refused(rename_branch_op, json!({"from": "a", "to": "a"})));
    assert!(refused(checkout_op, json!({"branch": "a b"})));
    assert!(refused(stash_op, json!({"message": "a\nb"})));
    assert!(refused(reset_op, json!({"commit": "HEAD", "mode": "keep"})));
    assert!(refused(
        move_commit_op,
        json!({"commit": "a", "direction": "left"})
    ));
    assert!(refused(
        delete_branch_op,
        json!({"name": "x", "force": "yes"})
    ));
    assert!(refused(push_op, json!({"force": true, "remote": "evil"})));
}

#[test]
fn without_a_window_a_change_is_refused_with_what_to_do() {
    let dir = std::env::temp_dir().join(format!("gitcraft-no-window-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let ctx = Ctx {
        repo: "/r".into(),
        agent: "t".into(),
        link_dir: Some(dir),
        wait: std::time::Duration::from_secs(2),
        cancelled: std::sync::Arc::new(|| false),
        run: None,
    };
    let paths = args(json!({"paths": ["a"]}));
    let e = (STAGE.run)(&ctx, &Args::new(&paths));
    assert!(e.is_err_and(|e| e.contains("start gitcraft and open /r")));
    let nowhere = Ctx {
        link_dir: None,
        ..ctx
    };
    assert!((STAGE.run)(&nowhere, &Args::new(&paths)).is_err());
}
