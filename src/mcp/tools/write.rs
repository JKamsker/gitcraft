//! The tools that change the repository. None runs here: each hands its operation to the
//! gitcraft window that has the repository open, which runs it as a step of its undo list
//! (Ctrl+Z there takes it back), or first asks the user on an Approve / Deny card when it can't
//! be undone. Arguments are checked here and again in the window.

use serde_json::{Value, json};

use super::{Ctx, Kind, ToolDef, boolean, object, string};
use crate::mcp::args::{self, Args};
use crate::mcp::link;
use crate::mcp::wire::{Outcome, ResetMode, WriteOp};

pub(super) const TOOLS: &[&ToolDef] = &[
    &STAGE,
    &UNSTAGE,
    &COMMIT,
    &CREATE_BRANCH,
    &RENAME_BRANCH,
    &CHECKOUT,
    &STASH,
    &CHERRY_PICK,
    &REVERT,
    &RESET,
    &MOVE_COMMIT,
    &DROP_COMMIT,
    &DELETE_BRANCH,
    &PUSH,
];

/// Hands `op` to the window and says how it went.
fn send(ctx: &Ctx, op: &WriteOp) -> Result<Value, String> {
    let dir = ctx
        .link_dir
        .as_deref()
        .ok_or("gitcraft can't tell where its windows are on this system")?;
    let reply = link::ask(dir, &ctx.repo.to_string_lossy(), &ctx.agent, op, ctx.wait);
    match reply.outcome {
        Outcome::Done => Ok(json!({ "done": reply.message })),
        Outcome::Denied => Err(format!(
            "The user denied this in gitcraft; nothing was done. {}",
            reply.message
        )),
        _ => Err(reply.message),
    }
}

fn paths_schema(what: &str) -> Value {
    object(
        json!({
            "paths": {
                "type": "array",
                "items": { "type": "string" },
                "minItems": 1,
                "maxItems": args::MAX_PATHS,
                "description": what,
            }
        }),
        &["paths"],
    )
}

static STAGE: ToolDef = ToolDef {
    name: "stage",
    title: "Stage files",
    description: "Stages these files' changes (new, changed or deleted), as `git add` does. Runs \
                  in the gitcraft window that has the repository open, as a step of its undo \
                  list: Ctrl+Z there puts the index back as it was. Staging a conflicted file \
                  marks it resolved, which can't be undone: the user has to approve that in the \
                  window first, and the call waits for their answer.",
    kind: Kind::Write,
    schema: || paths_schema("Files or folders, relative to the top of the repository."),
    run: |ctx, a| send(ctx, &stage_op(a)?),
};

fn stage_op(a: &Args) -> Result<WriteOp, String> {
    a.only(&["paths"])?;
    Ok(WriteOp::Stage {
        paths: a.paths("paths")?,
    })
}

static UNSTAGE: ToolDef = ToolDef {
    name: "unstage",
    title: "Unstage files",
    description: "Takes these files' changes out of the index, keeping them in the working tree. \
                  Runs in the gitcraft window that has the repository open, as a step of its \
                  undo list: Ctrl+Z there stages them again.",
    kind: Kind::Write,
    schema: || paths_schema("Files or folders, relative to the top of the repository."),
    run: |ctx, a| send(ctx, &unstage_op(a)?),
};

fn unstage_op(a: &Args) -> Result<WriteOp, String> {
    a.only(&["paths"])?;
    Ok(WriteOp::Unstage {
        paths: a.paths("paths")?,
    })
}

static COMMIT: ToolDef = ToolDef {
    name: "commit",
    title: "Commit",
    description: "Commits what is staged with this message (first line the summary), or with \
                  `amend` rewrites the latest commit with it and what is staged. Hooks run as \
                  usual. Runs in the gitcraft window that has the repository open, as a step of \
                  its undo list: Ctrl+Z there takes the commit back, keeping its changes staged.",
    kind: Kind::Write,
    schema: || {
        object(
            json!({
                "message": string("The commit message."),
                "amend": boolean("Rewrite the latest commit instead of adding one.", false),
            }),
            &["message"],
        )
    },
    run: |ctx, a| send(ctx, &commit_op(a)?),
};

fn commit_op(a: &Args) -> Result<WriteOp, String> {
    a.only(&["message", "amend"])?;
    Ok(WriteOp::Commit {
        message: args::message(a.str("message")?)?.to_owned(),
        amend: a.bool("amend", false)?,
    })
}

static CREATE_BRANCH: ToolDef = ToolDef {
    name: "create_branch",
    title: "Create a branch",
    description: "Makes a branch at `start` (HEAD by default), and checks it out with \
                  `checkout`. Runs in the gitcraft window that has the repository open, as a \
                  step of its undo list: Ctrl+Z there deletes it again (and goes back).",
    kind: Kind::Write,
    schema: || {
        object(
            json!({
                "name": string("The new branch's name."),
                "start": string("Where it starts: a commit, branch or tag. Default: HEAD."),
                "checkout": boolean("Check it out too.", false),
            }),
            &["name"],
        )
    },
    run: |ctx, a| send(ctx, &create_branch_op(a)?),
};

fn create_branch_op(a: &Args) -> Result<WriteOp, String> {
    a.only(&["name", "start", "checkout"])?;
    Ok(WriteOp::CreateBranch {
        name: args::branch_name(a.str("name")?)?.to_owned(),
        start: a
            .opt_str("start")?
            .map(args::revision)
            .transpose()?
            .map(str::to_owned),
        checkout: a.bool("checkout", false)?,
    })
}

static RENAME_BRANCH: ToolDef = ToolDef {
    name: "rename_branch",
    title: "Rename a branch",
    description: "Renames a local branch (its remote copy keeps its name). Runs in the gitcraft \
                  window that has the repository open, as a step of its undo list: Ctrl+Z there \
                  renames it back.",
    kind: Kind::Write,
    schema: || {
        object(
            json!({
                "from": string("The branch's name now."),
                "to": string("Its new name."),
            }),
            &["from", "to"],
        )
    },
    run: |ctx, a| send(ctx, &rename_branch_op(a)?),
};

fn rename_branch_op(a: &Args) -> Result<WriteOp, String> {
    a.only(&["from", "to"])?;
    let from = args::branch_name(a.str("from")?)?;
    let to = args::branch_name(a.str("to")?)?;
    if from == to {
        return Err("`from` and `to` are the same name".to_owned());
    }
    Ok(WriteOp::RenameBranch {
        from: from.to_owned(),
        to: to.to_owned(),
    })
}

static CHECKOUT: ToolDef = ToolDef {
    name: "checkout",
    title: "Check out a branch",
    description: "Checks out a local branch. Uncommitted changes stay, and git refuses when they \
                  would be overwritten. Runs in the gitcraft window that has the repository \
                  open, as a step of its undo list: Ctrl+Z there goes back to what was checked \
                  out.",
    kind: Kind::Write,
    schema: || object(json!({ "branch": string("A local branch.") }), &["branch"]),
    run: |ctx, a| send(ctx, &checkout_op(a)?),
};

fn checkout_op(a: &Args) -> Result<WriteOp, String> {
    a.only(&["branch"])?;
    Ok(WriteOp::Checkout {
        branch: args::branch_name(a.str("branch")?)?.to_owned(),
    })
}

static STASH: ToolDef = ToolDef {
    name: "stash",
    title: "Stash changes",
    description: "Stashes every uncommitted change, untracked files included, leaving the \
                  working tree clean. Runs in the gitcraft window that has the repository open, \
                  as a step of its undo list: Ctrl+Z there brings the changes back.",
    kind: Kind::Write,
    schema: || object(json!({ "message": string("What the stash is of.") }), &[]),
    run: |ctx, a| send(ctx, &stash_op(a)?),
};

fn stash_op(a: &Args) -> Result<WriteOp, String> {
    a.only(&["message"])?;
    let message = a.opt_str("message")?.unwrap_or("").trim();
    if message.len() > 1000 || message.chars().any(char::is_control) {
        return Err("the stash message must be one line of at most 1000 bytes".to_owned());
    }
    Ok(WriteOp::Stash {
        message: message.to_owned(),
    })
}

fn commit_schema(what: &str) -> Value {
    object(json!({ "commit": string(what) }), &["commit"])
}

static CHERRY_PICK: ToolDef = ToolDef {
    name: "cherry_pick",
    title: "Cherry-pick",
    description: "Applies a commit's changes onto the checked-out branch as a new commit. On \
                  conflicts it stops, as git does (see `conflicts`). Runs in the gitcraft window \
                  that has the repository open, as a step of its undo list: Ctrl+Z there takes \
                  it back.",
    kind: Kind::Write,
    schema: || commit_schema("The commit to apply."),
    run: |ctx, a| {
        send(
            ctx,
            &commit_arg(a, |commit| WriteOp::CherryPick { commit })?,
        )
    },
};

static REVERT: ToolDef = ToolDef {
    name: "revert",
    title: "Revert",
    description: "Adds a commit that undoes a commit's changes. Runs in the gitcraft window that \
                  has the repository open, as a step of its undo list: Ctrl+Z there takes the \
                  revert back.",
    kind: Kind::Write,
    schema: || commit_schema("The commit to revert."),
    run: |ctx, a| send(ctx, &commit_arg(a, |commit| WriteOp::Revert { commit })?),
};

fn commit_arg(a: &Args, op: fn(String) -> WriteOp) -> Result<WriteOp, String> {
    a.only(&["commit"])?;
    Ok(op(args::revision(a.str("commit")?)?.to_owned()))
}

static RESET: ToolDef = ToolDef {
    name: "reset",
    title: "Reset the branch",
    description: "Moves the checked-out branch to `commit`. `soft` keeps every change staged, \
                  `mixed` keeps them unstaged; both run in the gitcraft window that has the \
                  repository open, as a step of its undo list (Ctrl+Z there moves the branch \
                  back). `hard` throws uncommitted changes away for good, so the user has to \
                  approve it in the window first; the call waits for their answer.",
    kind: Kind::Write,
    schema: || {
        object(
            json!({
                "commit": string("Where the branch goes."),
                "mode": { "type": "string", "enum": ["soft", "mixed", "hard"], "default": "mixed" },
            }),
            &["commit"],
        )
    },
    run: |ctx, a| send(ctx, &reset_op(a)?),
};

fn reset_op(a: &Args) -> Result<WriteOp, String> {
    a.only(&["commit", "mode"])?;
    let mode = match a.opt_str("mode")?.unwrap_or("mixed") {
        "soft" => ResetMode::Soft,
        "mixed" => ResetMode::Mixed,
        "hard" => ResetMode::Hard,
        other => return Err(format!("`mode` is soft, mixed or hard, not `{other}`")),
    };
    Ok(WriteOp::Reset {
        commit: args::revision(a.str("commit")?)?.to_owned(),
        mode,
    })
}

static MOVE_COMMIT: ToolDef = ToolDef {
    name: "move_commit",
    title: "Move a commit",
    description: "Swaps a commit of the checked-out branch with the one after it (`up`, towards \
                  the newer ones) or before it (`down`), rebasing the commits after it. Not past \
                  merges. Runs in the gitcraft window that has the repository open, as a step of \
                  its undo list: Ctrl+Z there puts the branch back.",
    kind: Kind::Write,
    schema: || {
        object(
            json!({
                "commit": string("A commit of the checked-out branch."),
                "direction": { "type": "string", "enum": ["up", "down"] },
            }),
            &["commit", "direction"],
        )
    },
    run: |ctx, a| send(ctx, &move_commit_op(a)?),
};

fn move_commit_op(a: &Args) -> Result<WriteOp, String> {
    a.only(&["commit", "direction"])?;
    let up = match a.str("direction")? {
        "up" => true,
        "down" => false,
        other => return Err(format!("`direction` is up or down, not `{other}`")),
    };
    Ok(WriteOp::MoveCommit {
        commit: args::revision(a.str("commit")?)?.to_owned(),
        up,
    })
}

static DROP_COMMIT: ToolDef = ToolDef {
    name: "drop_commit",
    title: "Drop a commit",
    description: "Removes a commit and its changes from the checked-out branch, rebasing the \
                  commits after it. The user has to approve it in the gitcraft window that has \
                  the repository open; the call waits for their answer. Once done, it is a step \
                  of the window's undo list.",
    kind: Kind::Approve,
    schema: || commit_schema("A commit of the checked-out branch."),
    run: |ctx, a| {
        send(
            ctx,
            &commit_arg(a, |commit| WriteOp::DropCommit { commit })?,
        )
    },
};

static DELETE_BRANCH: ToolDef = ToolDef {
    name: "delete_branch",
    title: "Delete a branch",
    description: "Deletes a local branch (not the checked-out one); `force` even when it isn't \
                  merged. The user has to approve it in the gitcraft window that has the \
                  repository open; the call waits for their answer. Once done, it is a step of \
                  the window's undo list.",
    kind: Kind::Approve,
    schema: || {
        object(
            json!({
                "name": string("The local branch."),
                "force": boolean("Even if it isn't merged (git branch -D).", false),
            }),
            &["name"],
        )
    },
    run: |ctx, a| send(ctx, &delete_branch_op(a)?),
};

fn delete_branch_op(a: &Args) -> Result<WriteOp, String> {
    a.only(&["name", "force"])?;
    Ok(WriteOp::DeleteBranch {
        name: args::branch_name(a.str("name")?)?.to_owned(),
        force: a.bool("force", false)?,
    })
}

static PUSH: ToolDef = ToolDef {
    name: "push",
    title: "Push",
    description: "Pushes the checked-out branch to its upstream (publishing it on the default \
                  remote when it has none). `force` overwrites the remote branch, with \
                  --force-with-lease. A push can't be undone, so the user has to approve it in \
                  the gitcraft window that has the repository open; the call waits for their \
                  answer.",
    kind: Kind::Approve,
    schema: || {
        object(
            json!({ "force": boolean("Force-push (--force-with-lease).", false) }),
            &[],
        )
    },
    run: |ctx, a| send(ctx, &push_op(a)?),
};

fn push_op(a: &Args) -> Result<WriteOp, String> {
    a.only(&["force"])?;
    Ok(WriteOp::Push {
        force: a.bool("force", false)?,
    })
}

#[cfg(test)]
mod tests {
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
}
