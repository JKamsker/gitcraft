use super::*;
fn harness() -> egui_kittest::Harness<'static, DesktopApp> {
    egui_kittest::Harness::builder()
        .with_size([1200.0, 800.0])
        .build_eframe(|cc| {
            let mut app =
                App::launched_using(cc, None, || Ok("git version 2.43.0 (recorded)".into()));
            app.settings.check_updates = false;
            app.settings.notifications = false;
            app.settings.set(&cc.egui_ctx);
            DesktopApp::new(app)
        })
}

// build_eframe runs logic/ui during harness construction. These direct adapter tests
// need only recorded App state: never enter tick_model or start background watchers.
fn recorded_harness() -> egui_kittest::Harness<'static, Option<DesktopApp>> {
    egui_kittest::Harness::builder()
        .with_size([1200.0, 800.0])
        .build_ui_state(
            |ui, state: &mut Option<DesktopApp>| {
                if state.is_none() {
                    let cc = eframe::CreationContext::_new_kittest(ui.ctx().clone());
                    let mut app = App::launched_using(&cc, None, || {
                        Ok("git version 2.43.0 (recorded)".into())
                    });
                    app.settings.check_updates = false;
                    app.settings.notifications = false;
                    app.settings.set(ui.ctx());
                    *state = Some(DesktopApp::new(app));
                }
            },
            None,
        )
}

fn deferred(path: &str, ctx: &egui::Context) -> Tab {
    Tab::Repo(Box::new(crate::repo::RepoTab::new_deferred(
        path.into(),
        ctx,
    )))
}

fn identity(tab: &Tab) -> Option<usize> {
    match tab {
        Tab::Repo(r) => Some(r.as_ref() as *const crate::repo::RepoTab as usize),
        _ => None,
    }
}

include!("tests_cases/sessions.rs");

include!("tests_cases/drag_transfer.rs");

include!("tests_cases/restoration.rs");

include!("tests_cases/resident.rs");

include!("tests_cases/dialog_ownership.rs");

include!("tests_cases/completion_ownership.rs");

include!("tests_cases/geometry.rs");

include!("tests_cases/close_contention.rs");
