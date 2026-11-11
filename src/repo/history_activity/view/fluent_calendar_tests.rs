use super::*;

#[test]
fn fluent_partition_calendar_renders_both_languages_without_changing_iso_range() {
    use egui_kittest::{Harness, kittest::Queryable};
    let date = Day {
        year: 2026,
        month: 1,
        day: 1,
    };
    for (language, previous, day_label) in [
        ("en-US", "Previous month", "1\n1 commit"),
        ("de", "Vorheriger Monat", "1\n1 Commit"),
    ] {
        crate::i18n::set_language(language);
        // No RepoTab constructor or reader is polled: all state is supplied.
        let mut view = Activity::new(Page::Calendar);
        view.range(date, date);
        view.month = Some(date);
        let mut summary = Summary::default();
        summary.calendar.insert(date, 1);
        let mut initialized = false;
        let harness = Harness::new_ui_state(
            move |ui, view| {
                if !initialized {
                    crate::theme::install_fonts(ui.ctx());
                    ui.ctx().request_discard("install supplied-state fonts");
                    initialized = true;
                    return;
                }
                let t = Tokens::get(ui.ctx());
                calendar(ui, view, &summary, &t);
            },
            view,
        );
        harness.get_by_label(previous);
        harness.get_by_label(day_label);
        assert_eq!(harness.state().from, "2026-01-01");
        assert_eq!(harness.state().through, "2026-01-01");
        assert_eq!(
            harness.state().requested_range().map(|q| q.from),
            Ok(Some(date))
        );
    }
    crate::i18n::set_language("en-US");
}
