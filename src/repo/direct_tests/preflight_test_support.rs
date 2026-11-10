use super::*;

pub(in crate::repo) fn wait_for(
    tab: &mut RepoTab,
    ctx: &egui::Context,
    done: impl Fn(&RepoTab) -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(tab) && Instant::now() < deadline {
        tab.poll(ctx);
        std::thread::sleep(Duration::from_millis(10));
    }
    if done(tab) {
        Ok(())
    } else {
        Err(format!("Workflow did not settle: {:?}", tab.status))
    }
}

pub(in crate::repo) fn review(tab: &mut RepoTab, ctx: &egui::Context) -> Result<Plan, String> {
    assert!(matches!(
        tab.dialog,
        Some(Dialog::PushLoading { .. } | Dialog::PushPreflight { .. })
    ));
    assert!(tab.busy_op.is_none(), "review must precede dispatch");
    wait_for(tab, ctx, |t| {
        !matches!(t.dialog, Some(Dialog::PushLoading { .. }))
    })?;
    match &mut tab.dialog {
        Some(Dialog::PushPreflight { plan, acknowledge }) => {
            assert!(!*acknowledge, "a fresh checklist must require consent");
            check::push_args(plan)?;
            *acknowledge = true; // the checklist's explicit user consent
        }
        Some(Dialog::PushError(error)) => return Err(error.clone()),
        _ => return Err("No push checklist arrived".into()),
    }
    let Some(Dialog::PushPreflight {
        plan,
        acknowledge: true,
    }) = tab.dialog.take()
    else {
        return Err("Checklist consent was lost".into());
    };
    Ok(*plan)
}

pub(in crate::repo) fn approve(tab: &mut RepoTab, ctx: &egui::Context) -> Result<(), String> {
    let plan = review(tab, ctx)?;
    let previous = tab.last_op.as_ref().map(|op| op.ops.clone());
    tab.apply(Action::SubmitPreflight(Box::new(plan)), ctx);
    wait_for(tab, ctx, |t| {
        t.last_op.as_ref().is_some_and(|op| {
            Some(&op.ops) != previous.as_ref()
                && op
                    .ops
                    .iter()
                    .any(|step| step.first().is_some_and(|s| s == check::APPROVED))
        })
    })?;
    Ok(())
}
