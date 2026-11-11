//! Repository layout changes update saved view preferences without reapplying a profile.
use super::Settings;

impl Settings {
    pub(crate) fn set_show_detail(ctx: &egui::Context, on: bool) {
        let mut settings = Self::get(ctx);
        settings.show_detail = on;
        ctx.data_mut(|data| data.insert_temp(super::id(), settings));
    }

    pub(crate) fn set_diff_beside_graph(ctx: &egui::Context, on: bool) {
        let mut settings = Self::get(ctx);
        settings.diff_beside_graph = on;
        ctx.data_mut(|data| data.insert_temp(super::id(), settings));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_layout_choices_preserve_unrelated_preferences_and_applied_zoom() {
        for on in [false, true] {
            let ctx = egui::Context::default();
            let mut settings = Settings::default();
            settings.ui_scale = 1.25;
            settings.language = "de".into();
            settings.profile_mut().author_email = "unapplied-profile@example.invalid".into();
            settings.put_for_test(&ctx);
            ctx.set_zoom_factor(1.75);
            ctx.run_ui(Default::default(), |_| {})
                .drop_without_applying_deltas();
            assert_eq!(
                ctx.zoom_factor(),
                1.75,
                "fixture zoom must already be applied"
            );
            Settings::set_show_detail(&ctx, on);
            Settings::set_diff_beside_graph(&ctx, on);
            ctx.run_ui(Default::default(), |_| {})
                .drop_without_applying_deltas();
            let actual = Settings::get(&ctx);
            assert_eq!((actual.show_detail, actual.diff_beside_graph), (on, on));
            assert_eq!(actual.ui_scale, 1.25);
            assert_eq!(actual.language, "de");
            assert_eq!(
                actual.profile().author_email,
                "unapplied-profile@example.invalid"
            );
            assert_eq!(ctx.zoom_factor(), 1.75);
        }
    }
}
