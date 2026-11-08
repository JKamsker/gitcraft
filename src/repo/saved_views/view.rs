use super::*;
impl RepoTab {
    pub(in crate::repo) fn saved_graph_header(&mut self, ui: &mut egui::Ui) {
        if !self.saved_views.loaded && !self.loading && !self.sleeping() {
            self.saved_views.prefs = ui
                .ctx()
                .data_mut(|d| d.get_persisted::<Preferences>(preferences_id(self.repo_key())))
                .unwrap_or_default()
                .normalize();
            self.saved_views.loaded = true;
            self.saved_views.dirty = true;
        }
        let ctx = ui.ctx().clone();
        let mut choose = None;
        let mut save = false;
        let mut remove = false;
        let mut refresh = false;
        let mut collapse = false;
        let mut page = false;
        ui.horizontal_wrapped(|ui|{
            let active=self.saved_views.prefs.active.as_ref();
            egui::ComboBox::from_id_salt("graph_relation_view").selected_text(active.map_or(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-0b3db67d"),|p|p.name.as_str())).show_ui(ui,|ui|{
                if ui.selectable_label(active.is_none(),crate::i18n::text("repo-workflows-saved_views-saved-graph-header-0b3db67d")).clicked(){choose=Some(None);}
                for (name,kind) in [(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-5f0d0c04"),Kind::MyUnpushed),(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-a5d2f076"),Kind::RecentAncestors)]{
                    if ui.selectable_label(active.is_some_and(|p|p.kind==kind),name).clicked(){choose=Some(Some(Preset{name:name.into(),kind,days:30}));}
                }
                let anchor=self.selected.and_then(|i|self.commits.get(i)).filter(|c|git::investigations::oid(&c.hash)).map(|c|c.hash.clone());
                if ui.add_enabled(anchor.is_some(),egui::Button::new(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-1697aaa3"))).clicked() && let Some(anchor)=anchor {choose=Some(Some(Preset{name:crate::i18n::message("repo-workflows-saved_views-saved-graph-header-d2cb0f94", &[("super-super-short-anchor-1", (super::super::short(&anchor)).clone().into())]),kind:Kind::Descendants{anchor},days:30}));}
                ui.menu_button(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-361c4911"),|ui|{for b in &self.refs.local{if ui.button(&b.name).clicked(){choose=Some(Some(Preset{name:crate::i18n::message("repo-workflows-saved_views-saved-graph-header-b5bc2926", &[("b-name-1", (b.name).clone().into())]),kind:Kind::MergesInto{branch:b.name.clone()},days:30}));ui.close();}}});
                if !self.saved_views.prefs.saved.is_empty(){ui.separator();for p in &self.saved_views.prefs.saved{if ui.selectable_label(active==Some(p),&p.name).clicked(){choose=Some(Some(p.clone()));}}}
            });
            if let Some(preset)=self.saved_views.prefs.active.as_mut(){
                if matches!(preset.kind,Kind::RecentAncestors){ui.label(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-e08c0aa8"));if ui.add(egui::DragValue::new(&mut preset.days).range(1..=3650)).changed(){refresh=true;}}
                ui.add(egui::TextEdit::singleline(&mut self.saved_views.name).hint_text(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-e6fdb6c7")).char_limit(80).desired_width(130.0));
                save=ui.add_enabled(!self.saved_views.name.trim().is_empty()&&self.saved_views.prefs.saved.len()<32,egui::Button::new(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-21153875"))).clicked();
                remove=ui.add_enabled(self.saved_views.prefs.saved.iter().any(|p|p==preset),egui::Button::new(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-eb6259f8"))).clicked();
                refresh|=ui.button(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-112b4ddb")).clicked();collapse=ui.add_enabled(!self.saved_views.expanded.is_empty(),egui::Button::new(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-6009d6cf"))).clicked();
                if self.saved_views.reader.busy(){ui.add(crate::spinner::Spinner::new());if ui.button(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-8e137b60")).clicked(){choose=Some(None);}}
                if self.has_more{page=ui.add_enabled(!self.saved_views.reader.busy()&&!self.loading_more,egui::Button::new(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-0db174ee"))).on_hover_text(crate::i18n::text("repo-workflows-saved_views-saved-graph-header-927f719d")).clicked();}
            }
        });
        if let Some(preset) = choose {
            self.set_graph_view(preset, &ctx);
        }
        if save && let Some(mut preset) = self.saved_views.prefs.active.clone() {
            preset.name = self.saved_views.name.trim().to_owned();
            if preset.valid() {
                self.saved_views
                    .prefs
                    .saved
                    .retain(|p| p.name != preset.name);
                self.saved_views.prefs.saved.push(preset.clone());
                self.saved_views.prefs.active = Some(preset);
                self.saved_views.name.clear();
                self.save_graph_preferences(&ctx);
            }
        }
        if remove {
            let active = self.saved_views.prefs.active.clone();
            self.saved_views
                .prefs
                .saved
                .retain(|p| Some(p) != active.as_ref());
            self.save_graph_preferences(&ctx);
        }
        if refresh {
            self.saved_views.cancel();
            self.save_graph_preferences(&ctx);
        }
        if collapse {
            self.saved_views.expanded.clear();
            self.saved_views.dirty = true;
        }
        if page {
            self.load_graph_view_page();
        }
        if self.saved_views.active() {
            if let Some(selection) = &self.saved_views.selection {
                ui.small(crate::i18n::message(
                    "repo-workflows-saved-view-summary",
                    &[
                        (
                            "loaded",
                            (self
                                .history
                                .iter()
                                .filter(|c| selection.commits.contains(&c.hash))
                                .count())
                            .into(),
                        ),
                        ("selected", (selection.commits.len()).into()),
                        ("explanation", (selection.explanation.as_str()).into()),
                        (
                            "limited",
                            (if selection.limited { "yes" } else { "no" }).into(),
                        ),
                        ("budget", (30000).into()),
                    ],
                ));
            } else if let Some(error) = &self.saved_views.error {
                ui.colored_label(
                    crate::theme::Tokens::get(&ctx).danger,
                    crate::repo::activity::display_workflow_error(error),
                );
                ui.small(crate::i18n::text(
                    "repo-workflows-saved_views-saved-graph-header-ebd44c07",
                ));
            } else {
                ui.small(crate::i18n::text(
                    "repo-workflows-saved_views-saved-graph-header-b30e33e8",
                ));
            }
        }
        self.poll_saved_views(&ctx);
        self.prepare_graph_projection();
    }
}
