//! Setup dialog presentation and submission requests.

use super::*;

impl App {
    pub(in crate::app) fn setup_ui(&mut self, ctx: &egui::Context) {
        if !self.setup.open {
            return;
        }
        let busy = self.setup.busy();
        let mut close = false;
        let mut export = false;
        let mut load = false;
        let mut save = false;
        let mut apply = false;
        let mut restore = None;
        let mut activate = None;
        let mut open = None;
        let mut verify = None;
        let modal=egui::Modal::new(egui::Id::new("portable-setup")).show(ctx,|ui|{
            ui.set_width(760.0_f32.min((ctx.content_rect().width()-48.0).max(240.0)));
            ui.heading(crate::i18n::text("interaction-setup-title"));
            ui.label(crate::i18n::text("interaction-setup-retry-info"));
            ui.label(crate::i18n::text("interaction-setup-intro"));
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()*0.72).max(150.0)).show(ui,|ui|{
                ui.add_enabled_ui(!busy,|ui|{ui.horizontal(|ui|{ui.selectable_value(&mut self.setup.import,false,crate::i18n::text("interaction-setup-export"));ui.selectable_value(&mut self.setup.import,true,crate::i18n::text("interaction-setup-import"));});
                    if !self.setup.import {
                        ui.label(crate::i18n::text("interaction-setup-selection"));
                        for candidate in &mut self.setup.candidates {ui.checkbox(&mut candidate.selected,candidate.path.display().to_string());}
                        export=ui.button(crate::i18n::text("interaction-setup-build")).clicked();
                    }else{load=ui.button(crate::i18n::text("interaction-setup-load")).clicked();}
                });
                if let Some(file)=self.setup.file.clone(){
                    ui.separator();preview_summary(ui,&file);
                    ui.add_enabled_ui(!busy,|ui|{if self.setup.import{ui.checkbox(&mut self.setup.preferences,crate::i18n::text("interaction-setup-preferences"));let response=ui.add_enabled(!self.setup.applied,egui::Button::new(crate::i18n::text("interaction-setup-add")));
                        apply=response.clicked();}else{save=ui.button(crate::i18n::text("interaction-setup-save")).clicked();}});
                    for (index,r) in file.repositories.iter().enumerate(){
                        ui.push_id(index,|ui|{ui.group(|ui|{
                            ui.label(crate::i18n::message("interaction-setup-previous", &[("name", r.name.as_str().into()), ("path", r.previous_path.as_str().into())]));
                            ui.label(crate::i18n::message("interaction-setup-checkout", &[("branch", r.branch.as_deref().unwrap_or(crate::i18n::text("interaction-setup-detached")).into()), ("worktrees", r.worktrees.len().into()), ("hidden", r.hidden.len().into()), ("config", r.config.len().into())]));
                            if !self.setup.import {for remote in &r.remotes {ui.label(format!("{}: {}",remote.name,remote.fetch));for url in &remote.push{ui.label(crate::i18n::message("interaction-setup-push", &[("url", url.as_str().into())]));}}}
                            for w in &r.worktrees{ui.label(crate::i18n::message("interaction-setup-worktree", &[("name", w.name.as_str().into()), ("branch", w.branch.as_deref().unwrap_or(crate::i18n::text("interaction-setup-detached")).into()), ("commit", w.commit.as_str().into()), ("path", w.previous_path.as_str().into())]));}
                            for notice in &r.notices {ui.label(portability::display_message(notice));}
                            if self.setup.import {
                                let matching=r.profile_ids.iter().filter_map(|id|self.setup.mappings.get(id)).copied().collect::<Vec<_>>();
                                let permitted=self.setup.applied && (r.profile_ids.is_empty() || matching.contains(&self.settings.active_profile)) && !self.settings.profile().pending_setup();
                                let current_profile=self.settings.profile();
                                let plan=&mut self.setup.plans[index];
                                ui.add_enabled_ui(!busy,|ui|{
                                    ui.checkbox(&mut plan.selected,crate::i18n::text("interaction-setup-select"));
                                    ui.add_enabled_ui(plan.receipt.is_none(),|ui|{
                                        ui.label(crate::i18n::text("interaction-setup-addresses"));
                                        let mut changed=false;
                                        for remote in &mut plan.remotes {
                                            ui.label(crate::i18n::message("interaction-setup-remote", &[("name", remote.name.as_str().into())]));
                                            ui.horizontal(|ui|{ui.label(crate::i18n::text("interaction-setup-fetch"));changed|=ui.add(egui::TextEdit::singleline(&mut remote.fetch).char_limit(4096)).changed();});
                                            for (n,url) in remote.push.iter_mut().enumerate(){ui.horizontal(|ui|{ui.label(crate::i18n::message("interaction-setup-push-address", &[("number", (n+1).into())]));changed|=ui.add(egui::TextEdit::singleline(url).char_limit(4096)).changed();});}
                                        }
                                        if changed {plan.ssh_addresses_confirmed=false;}
                                        if plan.needs_ssh(){
                                            ui.checkbox(&mut plan.ssh_addresses_confirmed,crate::i18n::text("interaction-setup-confirm-ssh"));
                                            if current_profile.connections.ssh_key.is_empty(){ui.label(crate::i18n::text("interaction-setup-ssh-pending"));}
                                        }
                                    });
                                    if plan.source.is_none() && let Some(remote)=plan.remotes.first(){ui.label(crate::i18n::message("interaction-setup-clone-source", &[("source", remote.fetch.as_str().into())]));}
                                    ui.add_enabled_ui(plan.receipt.is_none(),|ui|{
                                        ui.horizontal(|ui|{ui.label(crate::i18n::text("interaction-setup-folder"));ui.text_edit_singleline(&mut plan.name);});
                                        if ui.button(crate::i18n::text("interaction-setup-parent")).clicked() && let Some(path)=rfd::FileDialog::new().pick_folder(){plan.parent=path;}
                                        if ui.button(crate::i18n::text("interaction-setup-source")).clicked() && let Some(path)=rfd::FileDialog::new().pick_folder(){plan.source=Some(path);}
                                        if plan.source.is_some() && ui.button(crate::i18n::text("interaction-setup-use-remote")).clicked(){plan.source=None;}
                                    });
                                    ui.label(crate::i18n::message("interaction-setup-destination", &[("path", plan.parent.join(&plan.name).display().to_string().into())]));
                                    if let Some(source)=&plan.source{ui.label(crate::i18n::message("interaction-setup-current-source", &[("path", source.display().to_string().into())]));}
                                    if !r.worktrees.is_empty(){ui.label(crate::i18n::message("interaction-setup-worktree-root", &[("path", plan.parent.join(format!("{}-worktrees",plan.name)).display().to_string().into())]));}
                                    if !permitted {ui.label(crate::i18n::text("interaction-setup-activate-help"));for id in &matching {if let Some(p)=self.settings.user_profiles.iter().find(|p|p.id==*id && !p.pending_setup()) && ui.button(crate::i18n::message("interaction-setup-activate", &[("profile", p.display_title().into())])).clicked(){activate=Some(*id);}}}
                                    if ui.add_enabled(plan.selected && permitted && plan.binding_ready(&current_profile) && !plan.parent.as_os_str().is_empty() && !plan.complete,egui::Button::new(if plan.receipt.is_some(){crate::i18n::text("interaction-setup-retry")}else{crate::i18n::text("interaction-setup-restore")})).clicked(){restore=Some(index);}
                                    if plan.receipt.is_some() && ui.button(crate::i18n::text("interaction-setup-new-destination")).clicked(){plan.receipt=None;plan.complete=false;plan.parent=PathBuf::new();plan.name=format!("{}-retry",r.name);plan.messages.push("Previous clone/worktrees were retained. Choose a new unused destination.".into());}
                                    if plan.complete && ui.button(crate::i18n::text("interaction-setup-open")).clicked(){open=plan.receipt.as_ref().map(|r|r.root.clone());}
                                });
                                for message in &plan.messages{ui.label(portability::display_message(message));}
                            }
                        });});
                    }
                }
                ui.separator();ui.heading(crate::i18n::text("interaction-setup-finish"));
                ui.label(crate::i18n::text("interaction-setup-pending-help"));
                for profile in self.settings.user_profiles.iter().filter(|p|p.pending_setup()) {ui.push_id(profile.id,|ui|{ui.group(|ui|{ui.label(profile.display_title());let fresh=self.setup.fresh.entry(profile.id).or_default();ui.add_enabled_ui(!busy,|ui|{if pending::fields(ui,profile,fresh){verify=Some((profile.clone(),fresh.clone()));}});});});}
                for message in &self.setup.messages{ui.label(portability::display_message(message));}
            });
            if busy {ui.horizontal(|ui|{ui.add(crate::spinner::Spinner::new());ui.label(crate::i18n::text("interaction-setup-working"));if ui.button(crate::i18n::text("interaction-setup-cancel")).clicked() && let Some(job)=&self.setup.job{job.active.store(false,Ordering::Release);job.cancel.cancel();}});}
            close=ui.add_enabled(!busy,egui::Button::new(crate::i18n::text("interaction-setup-close"))).clicked();
        });
        if close || (!busy && modal.should_close()) {
            self.setup.open = false;
        }
        if load
            && let Some(path) = rfd::FileDialog::new()
                .add_filter(crate::i18n::text("interaction-setup-filter"), &["json"])
                .pick_file()
        {
            self.setup.launch(ctx, move |active| {
                files::load(&path, active).map(Answer::Loaded)
            });
        }
        if export {
            let mut themes = self
                .profile_states
                .iter()
                .map(|(id, state)| (*id, state.theme.clone()))
                .collect::<BTreeMap<_, _>>();
            themes.insert(self.settings.active_profile, self.theme.clone());
            let mut file = Setup::new(&self.settings, &themes);
            let candidates = self
                .setup
                .candidates
                .iter()
                .filter(|c| c.selected)
                .cloned()
                .collect::<Vec<_>>();
            self.setup.launch(ctx, move |active| {
                file.resolve_public_accounts()?;
                let mut errors = Vec::new();
                for c in candidates {
                    if !active() {
                        return Err("Export cancelled".into());
                    }
                    match portability::snapshot(&c.path, c.profiles, c.hidden, active) {
                        Ok(mut repo) => {
                            let base = repo.name.clone();
                            let mut n = 2;
                            while file.repositories.iter().any(|r| r.name == repo.name) {
                                repo.name = format!("{base}-{n}");
                                n += 1;
                            }
                            file.repositories.push(repo);
                        }
                        Err(e) => errors.push(format!("Omitted {}: {e}", c.path.display())),
                    }
                }
                file.validate()?;
                Ok(Answer::Snapshot(file, errors))
            });
        }
        if save
            && let (Some(file), Some(path)) = (
                self.setup.file.clone(),
                rfd::FileDialog::new()
                    .set_file_name("gitcraft-setup.json")
                    .save_file(),
            )
        {
            self.setup.launch(ctx, move |active| {
                files::save(&path, &file, active)?;
                Ok(Answer::Saved(path))
            });
        }
        if apply && let Err(e) = self.apply_setup_public(ctx) {
            self.setup.messages.push(e);
        }
        if let Some(id) = activate {
            self.switch_profile(id, ctx);
        }
        if let Some(path) = open {
            self.open_root(path, ctx);
            self.setup.open = false;
        }
        if let Some(index) = restore
            && let Some(file) = &self.setup.file
        {
            let plan = &self.setup.plans[index];
            let mut repo = file.repositories[index].clone();
            repo.name = plan.name.clone();
            repo.remotes = plan.remotes.clone();
            let parent = plan.parent.clone();
            let source = plan.source.clone();
            let previous = plan.receipt.clone();
            self.setup.launch(ctx, move |active| {
                Ok(Answer::Restored(
                    index,
                    portability::restore(&repo, &parent, source.as_deref(), previous, active),
                ))
            });
        }
        if let Some((profile, fresh)) = verify {
            self.setup.launch(ctx, move |active| {
                let ready = pending::verify(&profile, &fresh, active)?;
                Ok(Answer::Verified(Box::new((profile, ready))))
            });
        }
    }
}
