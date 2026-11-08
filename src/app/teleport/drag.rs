//! Machine drop targets for a repository tab. Dropping only prepares the existing dialog.
use super::super::{App, Tab};
use crate::{remote, tasks, teleport::Machine};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

pub(in crate::app) struct DragTargets {
    source: PathBuf,
    known: Vec<PathBuf>,
    ssh: Vec<String>,
    wsl: Vec<String>,
    listed: Option<Receiver<Vec<String>>>,
    _scope: tasks::Scope,
    error: Option<String>,
}
impl DragTargets {
    fn new(source: PathBuf, known: Vec<PathBuf>, ctx: &egui::Context) -> Self {
        let scope = tasks::Scope(tasks::cancellation().child());
        let (send, receive) = channel();
        let repaint = ctx.clone();
        let submitted = tasks::pool().and_then(|pool| {
            pool.submit(
                scope.0.clone(),
                tasks::Priority::Interactive,
                None,
                Box::new(move || {
                    let _ = send.send(remote::wsl_distros());
                    repaint.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        Self {
            source,
            known,
            ssh: remote::ssh_config_hosts(),
            wsl: vec![],
            listed: submitted.as_ref().ok().map(|_| receive),
            _scope: scope,
            error: submitted.err(),
        }
    }
    fn draw(&mut self, ctx: &egui::Context) -> Option<Machine> {
        if let Some(receiver) = &self.listed {
            match receiver.try_recv() {
                Ok(wsl) => {
                    self.wsl = wsl;
                    self.listed = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.listed = None;
                    self.error = Some(
                        "Machine discovery stopped; use the tab's Teleport menu to type a host."
                            .into(),
                    );
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let machines = crate::teleport::machines(&self.source, &self.wsl, &self.ssh, &self.known);
        let mut selected = None;
        egui::Area::new(egui::Id::new("teleport-machine-drop-targets"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_TOP, [0., 56.])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(crate::i18n::text("interaction-teleport-drop"));
                    ui.label(
                        egui::RichText::new(crate::i18n::text("interaction-teleport-review"))
                            .small(),
                    );
                    ui.horizontal_wrapped(|ui| {
                        for machine in machines {
                            let response = ui.add(
                                egui::Button::new(machine.display_label())
                                    .min_size(egui::vec2(120., 42.)),
                            );
                            let pointer = ctx.pointer_interact_pos();
                            if drop_hit(
                                response.rect,
                                pointer,
                                ctx.input(|i| i.pointer.primary_released()),
                            ) {
                                selected = Some(machine);
                            }
                            if pointer.is_some_and(|p| response.rect.contains(p)) {
                                ui.painter().rect_stroke(
                                    response.rect,
                                    4.,
                                    egui::Stroke::new(2., ui.visuals().selection.bg_fill),
                                    egui::StrokeKind::Inside,
                                );
                            }
                        }
                    });
                    if self.listed.is_some() {
                        ui.label(crate::i18n::text("interaction-teleport-finding"));
                    }
                    if let Some(error) = &self.error {
                        ui.label(crate::teleport::display::diagnostic(error));
                    }
                    if self.ssh.is_empty()
                        && self.wsl.is_empty()
                        && self
                            .known
                            .iter()
                            .all(|p| Machine::of(p) == Machine::of(&self.source))
                    {
                        ui.label(crate::i18n::text("interaction-teleport-other-host"));
                    }
                });
            });
        selected
    }
}
fn drop_hit(rect: egui::Rect, pointer: Option<egui::Pos2>, released: bool) -> bool {
    released && pointer.is_some_and(|p| rect.contains(p))
}
impl App {
    pub(in crate::app) fn teleport_drop_target(
        &mut self,
        ctx: &egui::Context,
        source: Option<PathBuf>,
    ) -> Option<(PathBuf, Machine)> {
        let Some(source) = source else {
            self.teleport_drag = None;
            return None;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.teleport_drag = None;
            self.tab_drag = None;
            return None;
        }
        if self.teleport.as_ref().is_some_and(|f| f.running.is_some()) {
            self.teleport_drag = None;
            return None;
        }
        if self
            .teleport_drag
            .as_ref()
            .is_none_or(|d| d.source != source)
        {
            let mut known = self.recent.clone();
            for tab in &self.tabs {
                if let Tab::Repo(repo) = tab
                    && !known.contains(&repo.path)
                {
                    known.push(repo.path.clone());
                }
            }
            self.teleport_drag = Some(DragTargets::new(source.clone(), known, ctx));
        }
        let selected = self.teleport_drag.as_mut().and_then(|d| d.draw(ctx));
        if !ctx.input(|i| i.pointer.primary_down()) {
            self.teleport_drag = None;
        }
        selected.map(|machine| (source, machine))
    }
    pub(in crate::app) fn prepare_teleport_drop(
        &mut self,
        source: PathBuf,
        machine: Machine,
        ctx: &egui::Context,
    ) {
        if Machine::of(&source) == machine {
            return;
        }
        self.start_teleport(source, ctx);
        if let Some(form) = &mut self.teleport {
            form.pick_after_read = Some(machine);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_machine_drop_requires_release_inside_target_and_excludes_source() {
        let rect = egui::Rect::from_min_size(egui::pos2(100., 100.), egui::vec2(100., 40.));
        assert!(drop_hit(rect, Some(rect.center()), true));
        assert!(!drop_hit(rect, Some(rect.center()), false));
        assert!(!drop_hit(rect, Some(egui::pos2(0., 0.)), true));
        assert!(!drop_hit(rect, None, true));
        let source = std::path::Path::new("ssh://build/home/me/project");
        let machines = crate::teleport::machines(
            source,
            &["Ubuntu".into()],
            &["build".into(), "test".into()],
            &[],
        );
        assert!(machines.contains(&Machine::Here));
        assert!(machines.contains(&Machine::There(remote::Host::Wsl("Ubuntu".into()))));
        assert!(machines.contains(&Machine::There(remote::Host::Ssh("test".into()))));
        assert!(!machines.contains(&Machine::of(source)));
    }
}
