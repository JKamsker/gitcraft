use super::*;
impl RepoTab{
    pub(in crate::repo) fn saved_graph_header(&mut self,ui:&mut egui::Ui){
        if !self.saved_views.loaded&&!self.loading&&!self.deferred{
            self.saved_views.prefs=ui.ctx().data_mut(|d|d.get_persisted::<Preferences>(preferences_id(self.repo_key()))).unwrap_or_default().normalize();self.saved_views.loaded=true;self.saved_views.dirty=true;
        }
        let ctx=ui.ctx().clone();let mut choose=None;let mut save=false;let mut remove=false;let mut refresh=false;let mut collapse=false;let mut page=false;
        ui.horizontal_wrapped(|ui|{
            let active=self.saved_views.prefs.active.as_ref();
            egui::ComboBox::from_id_salt("graph_relation_view").selected_text(active.map_or("All history",|p|p.name.as_str())).show_ui(ui,|ui|{
                if ui.selectable_label(active.is_none(),"All history").clicked(){choose=Some(None);}
                for (name,kind) in [("My unpushed work",Kind::MyUnpushed),("My recent changes + dependencies",Kind::RecentAncestors)]{
                    if ui.selectable_label(active.is_some_and(|p|p.kind==kind),name).clicked(){choose=Some(Some(Preset{name:name.into(),kind,days:30}));}
                }
                let anchor=self.selected.and_then(|i|self.commits.get(i)).filter(|c|git::investigations::oid(&c.hash)).map(|c|c.hash.clone());
                if ui.add_enabled(anchor.is_some(),egui::Button::new("Descendants of selected commit")).clicked(){if let Some(anchor)=anchor{choose=Some(Some(Preset{name:format!("Descendants of {}",super::super::short(&anchor)),kind:Kind::Descendants{anchor},days:30}));}}
                ui.menu_button("Only merges into…",|ui|{for b in &self.refs.local{if ui.button(&b.name).clicked(){choose=Some(Some(Preset{name:format!("Only merges into {}",b.name),kind:Kind::MergesInto{branch:b.name.clone()},days:30}));ui.close();}}});
                if !self.saved_views.prefs.saved.is_empty(){ui.separator();for p in &self.saved_views.prefs.saved{if ui.selectable_label(active==Some(p),&p.name).clicked(){choose=Some(Some(p.clone()));}}}
            });
            if let Some(preset)=self.saved_views.prefs.active.as_mut(){
                if matches!(preset.kind,Kind::RecentAncestors){ui.label("Days");if ui.add(egui::DragValue::new(&mut preset.days).range(1..=3650)).changed(){refresh=true;}}
                ui.add(egui::TextEdit::singleline(&mut self.saved_views.name).hint_text("Name this view").char_limit(80).desired_width(130.0));
                save=ui.add_enabled(!self.saved_views.name.trim().is_empty()&&self.saved_views.prefs.saved.len()<32,egui::Button::new("Save view")).clicked();
                remove=ui.add_enabled(self.saved_views.prefs.saved.iter().any(|p|p==preset),egui::Button::new("Delete saved")).clicked();
                refresh|=ui.button("Refresh view").clicked();collapse=ui.add_enabled(!self.saved_views.expanded.is_empty(),egui::Button::new("Collapse spans")).clicked();
                if self.saved_views.reader.busy(){ui.add(crate::spinner::Spinner::new());}
                if self.has_more{page=ui.add_enabled(!self.saved_views.reader.busy()&&!self.loading_more,egui::Button::new("Load older context")).on_hover_text("Read one bounded page at the view's pinned tips, without downloading objects").clicked();}
            }
        });
        if let Some(preset)=choose{self.set_graph_view(preset,&ctx);}
        if save{if let Some(mut preset)=self.saved_views.prefs.active.clone(){preset.name=self.saved_views.name.trim().to_owned();if preset.valid(){self.saved_views.prefs.saved.retain(|p|p.name!=preset.name);self.saved_views.prefs.saved.push(preset.clone());self.saved_views.prefs.active=Some(preset);self.saved_views.name.clear();self.save_graph_preferences(&ctx);}}}
        if remove{let active=self.saved_views.prefs.active.clone();self.saved_views.prefs.saved.retain(|p|Some(p)!=active.as_ref());self.save_graph_preferences(&ctx);}
        if refresh{self.saved_views.cancel();self.save_graph_preferences(&ctx);}
        if collapse{self.saved_views.expanded.clear();self.saved_views.dirty=true;}
        if page{self.load_graph_view_page();}
        if self.saved_views.active(){
            if let Some(selection)=&self.saved_views.selection{ui.small(format!("{} matches · {}{} · working changes, stashes and selection kept as context",selection.commits.len(),selection.explanation,if selection.limited{" · partial: 30000-commit budget"}else{""}));}
            else if let Some(error)=&self.saved_views.error{ui.colored_label(crate::theme::Tokens::get(&ctx).danger,error);ui.small("Showing loaded context until a view can be computed.");}
            else{ui.small("Computing view from pinned history; showing loaded context…");}
        }
        self.poll_saved_views(&ctx);self.prepare_graph_projection();
    }
}
