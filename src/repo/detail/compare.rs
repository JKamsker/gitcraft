//! The comparison of two commits, or of one with the working tree.

use egui::{RichText, Ui};

use super::files_header;
use crate::icons;
use crate::repo::{Action, Compare, DiffKey, RepoTab, branch_tree, short, widgets};
use crate::theme::{self, Tokens};

impl RepoTab {
    /// Right pane while comparing: the two ends, how far apart they are, and the files that
    /// differ.
    pub(super) fn compare_panel(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let Some(c) = &self.compare else { return };
        widgets::panel_header(ui, "Compare", |ui| {
            if icons::small_button(ui, "x", "Close (Esc)").clicked() {
                acts.push(Action::CloseCompare);
            }
            branch_tree::files_toggle(ui);
        });
        compare_summary(ui, acts, c);

        match &c.data {
            None => {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| ui.add(crate::spinner::Spinner::new()));
            }
            Some(Err(e)) => {
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    ui.colored_label(t.danger, e);
                });
            }
            Some(Ok(d)) => {
                self.review_all_button(
                    ui,
                    crate::repo::review_workflow::ReviewSource::Comparison {
                        from: d.from.clone(),
                        to: d.to.clone(),
                    },
                    acts,
                );
                files_header(ui, &d.files, None);
                if d.files.is_empty() {
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No differences").color(t.text_dim));
                    });
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt("compare_scroll")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        self.file_list(ui, acts, "compare", &d.files, d.to.as_deref(), |path| {
                            DiffKey::Range {
                                from: d.from.clone(),
                                to: d.to.clone(),
                                path: path.to_owned(),
                            }
                        });
                    });
            }
        }
    }
}

/// The two ends of `c`, how far apart they are, and the switches for how to compare them.
fn compare_summary(ui: &mut Ui, acts: &mut Vec<Action>, c: &Compare) {
    let t = Tokens::get(ui.ctx());
    let data = c.data.as_ref().and_then(|d| d.as_ref().ok());
    let base_name = c.display_base_label();
    let target_name = if c.target.is_some() {
        c.display_target_label()
    } else {
        "the working tree".to_owned()
    };
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            egui::Grid::new("compare_ends")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    // A name, and the commit it stands for (`note` before the hash).
                    let end = |ui: &mut Ui, label: &str, hash: Option<&str>, note: &str| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(label).font(theme::medium(12.5)));
                            // The hash, unless the label already is it.
                            if let Some(hash) = hash.filter(|h| short(h) != label) {
                                ui.label(
                                    RichText::new(format!("{note}{}", short(hash)))
                                        .font(theme::mono(11.5))
                                        .color(t.text_faint),
                                )
                                .on_hover_text(hash);
                            }
                        });
                    };
                    let note = if c.since_fork { "merge base " } else { "" };
                    ui.label(RichText::new("From").color(t.text_faint));
                    end(ui, &base_name, data.map(|d| d.from.as_str()), note);
                    ui.end_row();
                    ui.label(RichText::new("To").color(t.text_faint));
                    end(ui, &target_name, data.and_then(|d| d.to.as_deref()), "");
                    ui.end_row();
                });
            if let Some(d) = data {
                let (behind, ahead) = d.apart;
                let commits = |n: u32| format!("{n} commit{}", if n == 1 { "" } else { "s" });
                let other = if c.target.is_some() {
                    target_name.as_str()
                } else {
                    "HEAD"
                };
                let text = match (ahead, behind) {
                    (0, 0) => "Same history".to_owned(),
                    (a, 0) => format!("{other} is {} ahead", commits(a)),
                    (0, b) => format!("{other} is {} behind", commits(b)),
                    (a, b) => format!("{other} is {} ahead, {b} behind", commits(a)),
                };
                ui.label(RichText::new(text).color(t.text_dim).small());
            }
            ui.horizontal(|ui| {
                let mut since = c.since_fork;
                let label = format!("Only what {target_name} changed since they diverged");
                if ui
                    .checkbox(&mut since, label)
                    .on_hover_text(
                        "Diff from the merge base, as a pull request shows it, instead of \
                         every difference between the two",
                    )
                    .changed()
                {
                    acts.push(Action::CompareSinceFork(since));
                }
            });
            if c.target.is_some() && ui.button("Swap").clicked() {
                acts.push(Action::SwapCompare);
            }
        });
}

#[cfg(test)]
mod caption_tests {
    use super::*;
    use crate::repo::state::CompareCaption;
    use egui_kittest::{Harness, kittest::Queryable};

    fn supplied(captions: [Option<CompareCaption>; 2]) -> Compare {
        Compare {
            base: "original-1234".into(),
            base_label: "Original fix".into(),
            target: Some("adapted-5678".into()),
            target_label: "Adapted fix".into(),
            captions,
            since_fork: false,
            data: None,
            request: 7,
        }
    }

    #[test]
    fn actual_comparison_captions_repaint_only_with_source_provenance() {
        for (locale, original, adapted) in [
            ("en-US", "Original fix", "Adapted fix"),
            ("de", "Ursprünglicher Fix", "Angepasster Fix"),
        ] {
            crate::i18n::set_language(locale);
            for typed in [false, true] {
                let comparison = supplied(if typed {
                    [
                        Some(CompareCaption::OriginalFix),
                        Some(CompareCaption::AdaptedFix),
                    ]
                } else {
                    [None, None]
                });
                let mut h = Harness::new_ui_state(
                    |ui, state: &mut (bool, Compare, Vec<Action>)| {
                        if !std::mem::replace(&mut state.0, true) {
                            crate::theme::install_fonts(ui.ctx());
                            return;
                        }
                        compare_summary(ui, &mut state.2, &state.1);
                    },
                    (false, comparison, Vec::new()),
                );
                h.run();
                h.get_by_label(if typed { original } else { "Original fix" });
                h.get_by_label(if typed { adapted } else { "Adapted fix" });
                assert!(h.state().2.is_empty());
                assert_eq!(h.state().1.base_label, "Original fix");
                assert_eq!(h.state().1.target_label, "Adapted fix");
                assert_eq!(h.state().1.base, "original-1234");
                assert_eq!(h.state().1.target.as_deref(), Some("adapted-5678"));
                if typed {
                    let (other_language, other_original, other_adapted) = if locale == "de" {
                        ("en-US", "Original fix", "Adapted fix")
                    } else {
                        ("de", "Ursprünglicher Fix", "Angepasster Fix")
                    };
                    crate::i18n::set_language(other_language);
                    h.run();
                    h.get_by_label(other_original);
                    h.get_by_label(other_adapted);
                    assert_eq!(h.state().1.base_label, "Original fix");
                    assert_eq!(h.state().1.target_label, "Adapted fix");
                    assert!(h.state().2.is_empty());
                    crate::i18n::set_language(locale);
                }
            }
        }
        crate::i18n::set_language("en-US");
    }

    #[test]
    fn range_names_use_caption_provenance_without_reinterpreting_custom_labels() {
        let mut c = supplied([
            Some(CompareCaption::OriginalFix),
            Some(CompareCaption::AdaptedFix),
        ]);
        c.data = Some(Ok(crate::repo::CompareData {
            from: c.base.clone(),
            to: c.target.clone(),
            files: Vec::new(),
            apart: (0, 0),
        }));
        for (locale, base, target) in [
            ("en-US", "Original fix", "Adapted fix"),
            ("de", "Ursprünglicher Fix", "Angepasster Fix"),
        ] {
            crate::i18n::set_language(locale);
            assert_eq!(c.name_of("original-1234").as_deref(), Some(base));
            assert_eq!(c.name_of("adapted-5678").as_deref(), Some(target));
        }
        c.captions = [None, None];
        assert_eq!(c.name_of("original-1234").as_deref(), Some("Original fix"));
        assert_eq!(c.name_of("adapted-5678").as_deref(), Some("Adapted fix"));
        c.captions = [
            Some(CompareCaption::OriginalFix),
            Some(CompareCaption::AdaptedFix),
        ];
        c.base_label = "Opaque Original fix user label".into();
        assert_eq!(
            c.name_of("original-1234").as_deref(),
            Some("Opaque Original fix user label")
        );
        // Caption metadata follows the corresponding end when an existing comparison swaps.
        std::mem::swap(&mut c.base_label, &mut c.target_label);
        c.captions.swap(0, 1);
        assert_eq!(c.display_base_label(), "Angepasster Fix");
        assert_eq!(c.display_target_label(), "Opaque Original fix user label");
        crate::i18n::set_language("en-US");
    }

    #[test]
    fn generated_turn_and_branch_captions_repaint_with_owned_fields_and_opaque_lookalikes() {
        let label = "Opaque: before turn 1234 / {$x}";
        let branch = "topic-1234/now:{$x}";
        let cases = [
            (
                [
                    "before turn 1234",
                    "turn 1234: Opaque: before turn 1234 / {$x}",
                ],
                [
                    Some(CompareCaption::BeforeTurn { number: 1234 }),
                    Some(CompareCaption::Turn {
                        number: 1234,
                        label: label.into(),
                    }),
                ],
                [
                    "vor Durchgang 1234",
                    "Durchgang 1234: Opaque: before turn 1234 / {$x}",
                ],
            ),
            (
                ["after turn 1234", "now"],
                [
                    Some(CompareCaption::AfterTurn { number: 1234 }),
                    Some(CompareCaption::Now),
                ],
                ["nach Durchgang 1234", "jetzt"],
            ),
            (
                [
                    "original topic-1234/now:{$x}",
                    "candidate topic-1234/now:{$x}",
                ],
                [
                    Some(CompareCaption::OriginalBranch {
                        branch: branch.into(),
                    }),
                    Some(CompareCaption::CandidateBranch {
                        branch: branch.into(),
                    }),
                ],
                [
                    "Original topic-1234/now:{$x}",
                    "Kandidat topic-1234/now:{$x}",
                ],
            ),
        ];
        for (raw, captions, german) in cases {
            for typed in [true, false] {
                let mut comparison = supplied(if typed {
                    captions.clone()
                } else {
                    [None, None]
                });
                comparison.base_label = raw[0].into();
                comparison.target_label = raw[1].into();
                let mut h = Harness::new_ui_state(
                    |ui, state: &mut (bool, Compare, Vec<Action>)| {
                        if !std::mem::replace(&mut state.0, true) {
                            crate::theme::install_fonts(ui.ctx());
                            return;
                        }
                        compare_summary(ui, &mut state.2, &state.1);
                    },
                    (false, comparison, Vec::new()),
                );
                for locale in ["en-US", "de", "en-US"] {
                    crate::i18n::set_language(locale);
                    h.run();
                    let expected = if typed && locale == "de" { german } else { raw };
                    h.get_by_label(expected[0]);
                    h.get_by_label(expected[1]);
                    assert_eq!(h.state().1.base_label, raw[0]);
                    assert_eq!(h.state().1.target_label, raw[1]);
                    assert_eq!(h.state().1.base, "original-1234");
                    assert_eq!(h.state().1.target.as_deref(), Some("adapted-5678"));
                    assert_eq!(h.state().1.request, 7);
                    assert!(h.state().2.is_empty());
                }
                if typed {
                    crate::i18n::set_language("de");
                    let state = h.state_mut();
                    state.1.base_label = "before turn 1234 (user edited)".into();
                    assert_eq!(
                        state.1.display_base_label(),
                        "before turn 1234 (user edited)"
                    );
                    state.1.captions.swap(0, 1);
                    std::mem::swap(&mut state.1.base_label, &mut state.1.target_label);
                    assert_eq!(state.1.display_base_label(), german[1]);
                    assert_eq!(
                        state.1.display_target_label(),
                        "before turn 1234 (user edited)"
                    );
                }
            }
        }
        crate::i18n::set_language("en-US");
    }
}
