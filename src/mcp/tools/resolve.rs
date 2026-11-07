//! The tools an agent resolving conflicts in the background reports with (see
//! `crate::resolver`): its plan, each step, and how it ended. Offered only to an agent gitcraft
//! started for that (`gitcraft mcp --run <id>`); each report goes to the window that started
//! the run, which shows it as the agent works.

use serde_json::{Value, json};

use super::{Ctx, Kind, ToolDef, boolean, number, object, string};
use crate::mcp::args::Args;
use crate::mcp::link;
use crate::mcp::wire::{Outcome, WriteOp};
use crate::resolver::report::{
    MAX_ESTIMATE_MINUTES, MAX_STEPS, MAX_TEXT, PlannedStep, Report, StepStatus,
};

pub(super) const TOOLS: &[&ToolDef] = &[&PLAN, &STEP, &FINISH];

/// How long a report waits for the window: it is only shown, so not long.
const REPORT_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Hands `report` to the window that started the run.
fn send(ctx: &Ctx, report: Report) -> Result<Value, String> {
    let run = ctx
        .run
        .clone()
        .ok_or("this gitcraft wasn't started for a resolving run (no --run)")?;
    let dir = ctx
        .link_dir
        .as_deref()
        .ok_or("gitcraft can't tell where its windows are on this system")?;
    let op = WriteOp::Report { run, report };
    let reply = link::ask(
        dir,
        &ctx.repo.to_string_lossy(),
        &ctx.agent,
        &op,
        REPORT_WAIT.min(ctx.wait),
        &*ctx.cancelled,
    );
    match reply.outcome {
        Outcome::Done => Ok(json!({ "noted": true })),
        // The work goes on without the window: the result is in the files.
        _ => Ok(json!({ "noted": false, "why": reply.message })),
    }
}

fn text(a: &Args, name: &str) -> Result<String, String> {
    let t = a.str(name)?;
    if t.chars().count() > MAX_TEXT * 4 {
        return Err(format!("`{name}` is too long: keep it short"));
    }
    Ok(t.to_owned())
}

static PLAN: ToolDef = ToolDef {
    name: "resolve_plan",
    title: "Report the plan",
    description: "Tells gitcraft the steps you will take to resolve the conflicts (one per \
                  file, and a last check if you make one), and how long you think all of it \
                  takes. The user watches them in gitcraft's progress panel. Call it first; \
                  call it again if the plan changes (steps keep how they stand by id).",
    kind: Kind::Report,
    schema: || {
        object(
            json!({
                "steps": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_STEPS,
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": string("Short and unique: the file's path for a file's step."),
                            "title": string("What you will do, in a few words."),
                            "file": string("The file it is about, relative to the top."),
                        },
                        "required": ["id", "title"],
                        "additionalProperties": false,
                    },
                },
                "estimate_minutes": number(
                    "Your honest guess of how long all the steps take, in minutes.",
                    5,
                    1,
                    u64::from(MAX_ESTIMATE_MINUTES),
                ),
            }),
            &["steps"],
        )
    },
    run: |ctx, a| send(ctx, plan_report(a)?),
};

fn plan_report(a: &Args) -> Result<Report, String> {
    a.only(&["steps", "estimate_minutes"])?;
    let list = a
        .value("steps")
        .and_then(Value::as_array)
        .ok_or("`steps` must be a list")?;
    if list.is_empty() || list.len() > MAX_STEPS {
        return Err(format!("`steps` takes 1 to {MAX_STEPS} steps"));
    }
    let steps = list
        .iter()
        .map(|v| {
            let map = v.as_object().ok_or("each step must be an object")?;
            let step = Args::new(map);
            step.only(&["id", "title", "file"])?;
            Ok(PlannedStep {
                id: text(&step, "id")?,
                title: text(&step, "title")?,
                file: step.opt_str("file")?.map(str::to_owned),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let estimate = a.uint("estimate_minutes", 0, 0, u64::from(MAX_ESTIMATE_MINUTES))?;
    Ok(Report::Plan {
        steps,
        estimate_minutes: (estimate > 0).then_some(estimate as u32),
    })
}

static STEP: ToolDef = ToolDef {
    name: "resolve_step",
    title: "Report a step",
    description: "Tells gitcraft that a step of your plan started (`running`) or ended \
                  (`done`, `failed`, `skipped`), with a one-line note of what you did or why \
                  it failed. A step not in the plan is added to it.",
    kind: Kind::Report,
    schema: || {
        object(
            json!({
                "id": string("The step's id, as in the plan."),
                "status": {
                    "type": "string",
                    "enum": ["running", "done", "failed", "skipped"],
                },
                "note": string("One line: how it was resolved, or why it couldn't be."),
            }),
            &["id", "status"],
        )
    },
    run: |ctx, a| send(ctx, step_report(a)?),
};

fn step_report(a: &Args) -> Result<Report, String> {
    a.only(&["id", "status", "note"])?;
    let status = a.str("status")?;
    let status = StepStatus::parse(status)
        .filter(|s| *s != StepStatus::Pending)
        .ok_or_else(|| format!("`status` is running, done, failed or skipped, not {status}"))?;
    let note = match a.opt_str("note")? {
        Some(_) => Some(text(a, "note")?),
        None => None,
    };
    Ok(Report::Step {
        id: text(a, "id")?,
        status,
        note,
    })
}

static FINISH: ToolDef = ToolDef {
    name: "resolve_finish",
    title: "Report the end",
    description: "Tells gitcraft you are done: whether every conflicted file is resolved \
                  (`ok`), and a short summary for the user of what you decided where the \
                  sides disagreed and what they should check. Call it last.",
    kind: Kind::Report,
    schema: || {
        object(
            json!({
                "ok": boolean("Every conflicted file is resolved.", true),
                "summary": string("A few short lines for the user."),
            }),
            &["ok", "summary"],
        )
    },
    run: |ctx, a| send(ctx, finish_report(a)?),
};

fn finish_report(a: &Args) -> Result<Report, String> {
    a.only(&["ok", "summary"])?;
    Ok(Report::Finish {
        ok: a.bool("ok", true)?,
        summary: text(a, "summary")?,
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

    #[test]
    fn each_tool_reads_its_arguments_into_one_report() -> Result<(), String> {
        let plan = args(json!({
            "steps": [
                {"id": "a.rs", "title": "Keep both imports", "file": "a.rs"},
                {"id": "check", "title": "Check it builds"}
            ],
            "estimate_minutes": 3
        }));
        assert_eq!(
            plan_report(&Args::new(&plan))?,
            Report::Plan {
                steps: vec![
                    PlannedStep {
                        id: "a.rs".into(),
                        title: "Keep both imports".into(),
                        file: Some("a.rs".into())
                    },
                    PlannedStep {
                        id: "check".into(),
                        title: "Check it builds".into(),
                        file: None
                    },
                ],
                estimate_minutes: Some(3),
            }
        );
        let step = args(json!({"id": "a.rs", "status": "done", "note": "kept both"}));
        assert_eq!(
            step_report(&Args::new(&step))?,
            Report::Step {
                id: "a.rs".into(),
                status: StepStatus::Done,
                note: Some("kept both".into())
            }
        );
        let finish = args(json!({"ok": false, "summary": "b.rs needs you"}));
        assert_eq!(
            finish_report(&Args::new(&finish))?,
            Report::Finish {
                ok: false,
                summary: "b.rs needs you".into()
            }
        );
        Ok(())
    }

    #[test]
    fn wrong_reports_are_refused() {
        let bad = [
            json!({"steps": []}),
            json!({"steps": [{"title": "no id"}]}),
            json!({"steps": [{"id": "a", "title": "t", "extra": 1}]}),
            json!({"steps": "a"}),
        ];
        for b in bad {
            assert!(plan_report(&Args::new(&args(b.clone()))).is_err(), "{b}");
        }
        let pending = args(json!({"id": "a", "status": "pending"}));
        assert!(step_report(&Args::new(&pending)).is_err());
        let long = args(json!({"ok": true, "summary": "x".repeat(MAX_TEXT * 5)}));
        assert!(finish_report(&Args::new(&long)).is_err());
    }

    #[test]
    fn without_a_run_a_report_says_so() {
        let ctx = Ctx {
            repo: "/r".into(),
            agent: "t".into(),
            link_dir: None,
            wait: std::time::Duration::from_secs(1),
            cancelled: std::sync::Arc::new(|| false),
            run: None,
        };
        let finish = args(json!({"ok": true, "summary": "s"}));
        let e = (FINISH.run)(&ctx, &Args::new(&finish));
        assert!(e.is_err_and(|e| e.contains("--run")));
    }
}
