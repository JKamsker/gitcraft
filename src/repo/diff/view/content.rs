//! Specialized diff presentations keep their loading/error/search behavior together.
use super::*;

impl RepoTab {
    pub(super) fn markdown_content(&mut self, ui: &mut Ui, path: &str, as_file: bool) {
        let t = Tokens::get(ui.ctx());
        let Some(view) = &mut self.diff else { return };
        match &view.rendered {
            None => {
                ui.centered_and_justified(|ui| ui.add(crate::spinner::Spinner::new()));
            }
            Some(Err(e)) => {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.colored_label(
                        t.danger,
                        crate::git::display_error(&crate::forge::inbox::display_error(e)),
                    );
                });
            }
            Some(Ok(md)) => {
                if self.find.open {
                    self.find.rendered(
                        ui,
                        format!(
                            "markdown:{:?}:{}:{}:{as_file}",
                            view.key, view.request, view.rendered_load
                        ),
                        |t| {
                            let mut doc = find::rendered::Document::default();
                            if as_file {
                                doc.markdown(&md.file, t.text);
                            } else {
                                for (change, text) in &md.blocks {
                                    let color = match change {
                                        rendered::Change::Kept => t.text,
                                        rendered::Change::Removed => t.danger,
                                        rendered::Change::Added => t.success,
                                    };
                                    doc.markdown(text, color);
                                }
                            }
                            doc
                        },
                    );
                } else {
                    rendered::show(ui, &path, md, &mut view.markdown, !as_file);
                }
            }
        }
    }

    pub(super) fn whole_file_content(&mut self, ui: &mut Ui, path: &str, acts: &mut Vec<Action>) {
        let Some(view) = &self.diff else { return };
        let before = matches!(&view.file, Some(Ok(f)) if f.before);
        let old_path = match &view.data {
            Some(Ok(d)) => old_name(&d.header),
            _ => None,
        };
        let rev = sides(&view.key).map(|(new, old)| if before { old } else { new });
        let link_menu = self.link_menu(None);
        let menu = || link_menu.clone();
        let mut link = |ui: &mut Ui, lines| {
            if let (Some(rev), Some(menu)) = (&rev, menu()) {
                let target = Target::File {
                    rev: rev.clone(),
                    path: path.to_owned(),
                    lines: Some(lines),
                };
                menu.items(ui, &lines.label(), &target, acts);
            }
        };
        // Lines are picked only when there's something to link them to.
        let links = (rev.is_some() && self.link_menu(None).is_some())
            .then_some(&mut link as file_text::LineMenu<'_>);
        let pick = |range| picked_in_file(&view.key, old_path.as_deref(), before, range);
        let history = pick((1, 1))
            .map(|_| ())
            .ok_or(crate::i18n::text("repo-core-1856"));
        // Lines picked in one version aren't those of another one of the same file.
        let id = ("diff_file", &path, format!("{:?}", view.key));
        if let Some(p) = file_text::show(
            ui,
            id,
            view.file.as_ref(),
            &mut self.find,
            || find::Source::DiffFile(path.to_owned()),
            history,
            links,
            None,
        )
        .and_then(pick)
        {
            acts.push(Action::LineHistory {
                path: p.path,
                at: p.at,
                range: p.range,
            });
        }
    }

    /// Ready rich content owns the area; a Plain result annotates ordinary diff rows.
    pub(super) fn rich_content(
        &mut self,
        ui: &mut Ui,
        kind: rich::RichKind,
        acts: &mut Vec<Action>,
    ) -> bool {
        let t = Tokens::get(ui.ctx());
        let Some(view) = &mut self.diff else {
            return true;
        };
        match &view.rich {
            None => {
                ui.centered_and_justified(|ui| ui.add(crate::spinner::Spinner::new()));
                return true;
            }
            Some((_, Err(e))) => {
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.colored_label(
                        t.danger,
                        crate::git::display_error(&crate::forge::inbox::display_error(e)),
                    );
                });
                return true;
            }
            Some((_, Ok(rich::Rich::Ready(diff)))) => {
                let metadata_generation = view.rich_state.search_generation(ui.ctx());
                if self.find.open {
                    self.find.rendered(
                        ui,
                        format!(
                            "rich:{:?}:{}:{}:{metadata_generation}",
                            view.key, view.request, view.rich_load
                        ),
                        |t| rich::search_document(diff, &view.rich_state, t),
                    );
                } else {
                    rich::show(ui, diff, &mut view.rich_state, acts);
                }
                return true;
            }
            Some((_, Ok(rich::Rich::Plain(note)))) => rich::fallback_note(ui, kind, note),
        }
        false
    }
}
