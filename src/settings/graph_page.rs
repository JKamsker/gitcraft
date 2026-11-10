//! Commit graph preferences, column controls and history-map display modes.

use super::layout::{self, Filter, section};
use super::preview::Preview;
use super::{
    COMBO_WIDTH, DEFAULT_REFS_WIDTH, SLIDER_WIDTH, Settings, compact_tip, indented_note,
    parse_localized_number,
};
use egui::Ui;
use serde::{Deserialize, Serialize};

pub(super) fn render(settings: &mut Settings, ui: &mut Ui, f: &Filter) {
    section(ui, f, "Graph feedback", None, |card| {
        card.row(
            "Explanatory motion",
            "fetch incoming refs scroll reduced motion",
            |ui| {
                ui.checkbox(
                    &mut settings.graph_motion,
                    crate::i18n::text("interaction-settings-ref-transitions"),
                );
                ui.small(crate::i18n::text(
                    "interaction-settings-reduced-motion-note",
                ));
            },
        );
    });
    section(ui, f, "History filters", None, |card| {
        card.row(
            "Global history filters",
            "authors names emails namespaces T3 Code hide exclude",
            |ui| {
                crate::repo::history_filter::edit(ui, &mut settings.history_filter);
            },
        );
    });
    section(ui, f, "Commit graph", None, |card| {
        card.row(
            "Topology navigation",
            "accessibility audio lanes graph parent child",
            |ui| {
                ui.checkbox(
                    &mut settings.graph_lane_walk,
                    crate::i18n::text("interaction-settings-walk-topology"),
                );
                ui.checkbox(
                    &mut settings.graph_sounds,
                    crate::i18n::text("interaction-settings-graph-sounds"),
                );
            },
        );
        card.preview(Preview::GraphRefs);
        card.row("Branch column width", "refs tags pills", |ui| {
            ui.spacing_mut().slider_width = layout::fit(ui, SLIDER_WIDTH + 70.0) - 70.0;
            if ui
                .add(layout::slider(
                    egui::Slider::new(
                        &mut settings.refs_width,
                        crate::repo::columns::MIN_REFS_W..=2000.0,
                    )
                    .step_by(10.0)
                    .custom_formatter(|value, _| crate::i18n::number(value))
                    .custom_parser(parse_localized_number)
                    .suffix(" px"),
                ))
                .changed()
            {
                settings.auto_refs_width = false;
            }
            let mut fit = settings.fit_visible_refs();
            if ui
                .checkbox(&mut fit, crate::i18n::text("interaction-settings-fit-refs"))
                .changed()
            {
                settings.auto_refs_width = fit;
                if fit {
                    settings.refs_width = DEFAULT_REFS_WIDTH;
                }
            }
        });
        card.preview(Preview::GraphRefs);
        card.row(
            "Shortened branch names",
            "refs tags pills abbreviation ellipsis suffix prefix",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    ui.radio_value(
                        &mut settings.branch_names_keep_end,
                        true,
                        crate::i18n::text("interaction-settings-keep-end"),
                    );
                    ui.radio_value(
                        &mut settings.branch_names_keep_end,
                        false,
                        crate::i18n::text("interaction-settings-keep-start"),
                    );
                });
            },
        );
        card.preview(Preview::GraphColumns);
        card.row_hint(
            "Columns",
            crate::i18n::text("interaction-settings-columns-tip"),
            "author when date hash tests show",
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 18.0;
                    ui.checkbox(
                        &mut settings.show_author,
                        crate::i18n::text("interaction-settings-author"),
                    );
                    ui.checkbox(
                        &mut settings.show_when,
                        crate::i18n::text("interaction-settings-when"),
                    );
                    ui.checkbox(
                        &mut settings.show_date,
                        crate::i18n::text("interaction-settings-date"),
                    );
                    ui.checkbox(
                        &mut settings.show_hash,
                        crate::i18n::text("interaction-settings-hash"),
                    );
                    ui.checkbox(
                        &mut settings.show_test_lane,
                        crate::i18n::text("interaction-settings-tests"),
                    );
                });
            },
        );
        card.row("Diffs", "diff split below beside graph", |ui| {
            ui.checkbox(
                &mut settings.diff_beside_graph,
                crate::i18n::text("interaction-settings-diff-below"),
            )
            .on_hover_text(crate::i18n::text("interaction-settings-diff-below-tip"));
        });
        card.preview(Preview::GraphLanes);
        card.row("Lanes", "compact graph narrow", |ui| {
            let mut compact = settings.compact_graph;
            if ui
                .checkbox(
                    &mut compact,
                    crate::i18n::text("interaction-settings-compact"),
                )
                .changed()
            {
                settings.set_compact_graph(compact);
            }
            indented_note(ui, compact_tip());
        });
        card.row(
            "History map",
            "minimap overview scroll strip size hide",
            |ui| {
                egui::ComboBox::from_id_salt("settings-history-map")
                    .width(layout::fit(ui, COMBO_WIDTH))
                    .selected_text(settings.history_map.label())
                    .show_ui(ui, |ui| {
                        for option in HistoryMap::ALL {
                            ui.selectable_value(&mut settings.history_map, option, option.label());
                        }
                    });
            },
        );
        card.preview(Preview::LaneColors);
        card.row(
            "Lane colours",
            "colors colour blind palette okabe ito classic theme",
            |ui| {
                egui::ComboBox::from_id_salt("settings-lane-colors")
                    .width(layout::fit(ui, COMBO_WIDTH))
                    .selected_text(settings.theme_tweaks.lanes.label())
                    .show_ui(ui, |ui| {
                        for option in crate::theme::LaneColors::ALL {
                            ui.selectable_value(
                                &mut settings.theme_tweaks.lanes,
                                option,
                                option.label(),
                            );
                        }
                    });
            },
        );
    });
}

/// The history map's size beside the graph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryMap {
    /// Lanes with a caption above and the loaded extent below.
    #[default]
    Large,
    /// Just the lanes, in a narrow strip.
    Small,
    Hidden,
}

impl HistoryMap {
    pub const ALL: [HistoryMap; 3] = [HistoryMap::Large, HistoryMap::Small, HistoryMap::Hidden];

    pub fn label(self) -> &'static str {
        match self {
            HistoryMap::Large => crate::i18n::text("interaction-settings-history-map-large"),
            HistoryMap::Small => crate::i18n::text("interaction-settings-history-map-small"),
            HistoryMap::Hidden => crate::i18n::text("interaction-settings-history-map-hidden"),
        }
    }

    /// The strip's width, without the gap that parts it from the graph.
    pub fn width(self) -> f32 {
        match self {
            HistoryMap::Large => 78.0,
            HistoryMap::Small => 32.0,
            HistoryMap::Hidden => 0.0,
        }
    }

    /// The sizes as radio items of a menu, which closes on a pick.
    pub fn menu(ui: &mut Ui, map: &mut HistoryMap) {
        for option in HistoryMap::ALL {
            if ui.radio_value(map, option, option.label()).clicked() {
                ui.close();
            }
        }
    }
}
