//! One event consumer per scope/viewport. Prefixes expire and never survive an owner change.
use egui::{Context, Event, Id, Key};

use super::{Command, Scope, Stroke, config};

const TIMEOUT: f64 = 1.2;

#[derive(Clone, Default)]
struct Resolver {
    prefix: Vec<Stroke>,
    deadline: f64,
    owner: Option<Id>,
    config: config::Overrides,
    pass: Option<u64>,
    commands: Vec<Command>,
    active: Vec<Scope>,
    allowed: Option<Vec<Command>>,
}

fn id(ctx: &Context, _scope: Scope) -> Id {
    Id::new(("key-resolver", ctx.viewport_id()))
}

pub(crate) fn cancel(ctx: &Context, scope: Scope) {
    let key = id(ctx, scope);
    ctx.data_mut(|d| {
        if let Some(mut r) = d.get_temp::<Resolver>(key) {
            r.prefix.clear();
            d.insert_temp(key, r);
        }
    });
}

/// A blocked repository preserves only explicit text/modal exceptions installed by its window.
/// Standalone views with no such exception cancel any pending command.
pub(crate) fn retain_allowed(ctx: &Context) {
    let key = id(ctx, Scope::Window);
    ctx.data_mut(|d| {
        if let Some(mut r) = d.get_temp::<Resolver>(key) {
            let permitted = r.allowed.as_ref().is_some_and(|allowed| {
                allowed.iter().any(|&c| {
                    let seq = config::effective(&r.config, c);
                    r.eligible(c) && seq.len() > r.prefix.len() && seq[..r.prefix.len()] == r.prefix
                })
            });
            if !permitted {
                r.prefix.clear();
            }
            d.insert_temp(key, r);
        }
    });
}

/// Changing tabs/repositories cancels prefixes even when no individual widget owned focus.
pub(crate) fn owner(ctx: &Context, owner: Id) {
    let key = id(ctx, Scope::Window).with("owner");
    let changed = ctx.data(|d| d.get_temp::<Id>(key)) != Some(owner);
    if changed {
        cancel(ctx, Scope::Window);
    }
    ctx.data_mut(|d| d.insert_temp(key, owner));
}

impl Resolver {
    fn eligible(&self, command: Command) -> bool {
        self.allowed
            .as_ref()
            .is_none_or(|allowed| allowed.contains(&command))
            && (self.active.is_empty()
                && matches!(command.scope(), Scope::Window | Scope::Repository)
                || self.active.contains(&command.scope()))
    }

    fn resolve(
        &mut self,
        _scope: Scope,
        key: Key,
        physical: Option<Key>,
        modifiers: egui::Modifiers,
        repeat: bool,
        now: f64,
    ) -> (Option<Command>, bool) {
        if now > self.deadline {
            self.prefix.clear();
        }
        if key == Key::Escape && !self.prefix.is_empty() {
            self.prefix.clear();
            return (None, true);
        }
        let pending = !self.prefix.is_empty();
        // A logical match anywhere wins over physical tab digits on alternate layouts.
        let logical = config::commands().any(|c| {
            config::effective(&self.config, c)
                .first()
                .is_some_and(|s| s.matches(key, modifiers))
        });
        let mut found = None;
        let mut candidates = Vec::new();
        for c in config::commands().filter(|&c| self.eligible(c)) {
            let seq = config::effective(&self.config, c);
            if seq.len() <= self.prefix.len() || seq[..self.prefix.len()] != self.prefix {
                continue;
            }
            let stroke = &seq[self.prefix.len()];
            let digit = !logical
                && self.prefix.is_empty()
                && matches!(c, Command::GoToTab(_) | Command::LastTab)
                && physical.is_some_and(|p| stroke.matches(p, modifiers));
            if stroke.matches(key, modifiers) || digit {
                if seq.len() == self.prefix.len() + 1 {
                    if !repeat || c.repeats() {
                        found = Some(c);
                    }
                } else if !repeat {
                    candidates.push(stroke.clone());
                }
            }
        }
        if let Some(c) = found {
            self.prefix.clear();
            return (Some(c), true);
        }
        if let Some(stroke) = candidates.first() {
            self.prefix.push(stroke.clone());
            self.deadline = now + TIMEOUT;
            return (None, true);
        }
        if pending && !repeat {
            self.prefix.clear();
        }
        (None, pending)
    }
}

/// Modal/text owners only expose their deliberate command exceptions. Other bindings must
/// neither dispatch nor consume the editor/terminal's keys.
pub(crate) fn restrict(ctx: &Context, allowed: Option<Vec<Command>>) {
    let key = id(ctx, Scope::Window).with("allowed");
    let old = ctx
        .data(|d| d.get_temp::<Option<Vec<Command>>>(key))
        .flatten();
    if old != allowed {
        cancel(ctx, Scope::Window);
    }
    ctx.data_mut(|d| d.insert_temp(key, allowed));
}

pub(crate) fn active(ctx: &Context, scopes: Vec<Scope>) {
    let key = id(ctx, Scope::Window).with("active");
    let old: Option<Vec<Scope>> = ctx.data(|d| d.get_temp(key));
    if old.as_ref() != Some(&scopes) {
        cancel(ctx, Scope::Window);
    }
    ctx.data_mut(|d| d.insert_temp(key, scopes));
}

pub(crate) fn queued(ctx: &Context, command: Command) -> bool {
    let key = id(ctx, command.scope());
    ctx.data(|d| d.get_temp::<Resolver>(key))
        .is_some_and(|r| r.commands.contains(&command))
}

/// Whether a native semantic event may use an already resolved command context.
/// Native Copy has no Key event on some platforms and must respect the same modal gate.
pub(super) fn permitted(ctx: &Context, command: Command) -> bool {
    let key = id(ctx, command.scope());
    ctx.data(|d| d.get_temp::<Resolver>(key))
        .is_some_and(|r| r.eligible(command))
}

/// Consume one contextual command without draining unrelated keys for the same list.
pub(crate) fn take(ctx: &Context, command: Command) -> bool {
    let key = id(ctx, command.scope());
    // Resolve the frame if no app-wide dispatcher ran (for independent viewports/tests).
    let pass = ctx.cumulative_pass_nr();
    let ready = ctx
        .data(|d| d.get_temp::<Resolver>(key))
        .is_some_and(|r| r.pass == Some(pass));
    if !ready {
        // Window commands are delivered by the window coordinator; preserve local results.
        let _ = pressed(ctx, Scope::Window);
    }
    ctx.data_mut(|d| {
        let mut r = d.get_temp::<Resolver>(key).unwrap_or_default();
        let at = r.commands.iter().position(|c| *c == command);
        if let Some(at) = at {
            r.commands.remove(at);
        }
        d.insert_temp(key, r);
        at.is_some()
    })
}

pub(crate) fn pressed(ctx: &Context, scope: Scope) -> Vec<Command> {
    let key_id = id(ctx, scope);
    let mut r = ctx
        .data_mut(|d| d.remove_temp::<Resolver>(key_id))
        .unwrap_or_default();
    let pass = ctx.cumulative_pass_nr();
    if r.pass == Some(pass) {
        let commands = take_scope(&mut r.commands, scope);
        ctx.data_mut(|d| d.insert_temp(key_id, r));
        return commands;
    }
    r.pass = Some(pass);
    r.allowed = ctx
        .data(|d| d.get_temp::<Option<Vec<Command>>>(key_id.with("allowed")))
        .flatten();
    r.active = ctx
        .data(|d| d.get_temp::<Vec<Scope>>(key_id.with("active")))
        .unwrap_or_default();
    let config = config::usable(&crate::settings::Settings::get(ctx).key_bindings);
    let owner = ctx.memory(|m| m.focused());
    let (now, focused, events) = ctx.input(|i| (i.time, i.focused, i.events.clone()));
    if !focused || owner != r.owner || config != r.config {
        r.prefix.clear();
    }
    r.owner = owner;
    r.config = config;
    if now > r.deadline {
        r.prefix.clear();
    }
    let mut commands = Vec::new();
    let mut consumed = Vec::new();
    let mut chord_text = !r.prefix.is_empty();
    for e in events {
        if let Event::Key {
            key,
            physical_key,
            pressed: true,
            repeat,
            modifiers,
        } = e
        {
            let was_pending = !r.prefix.is_empty();
            let (command, used) = r.resolve(scope, key, physical_key, modifiers, repeat, now);
            if let Some(c) = command {
                commands.push(c);
            }
            if used {
                chord_text |= was_pending || !r.prefix.is_empty();
                consumed.push((key, modifiers));
            }
        }
    }
    ctx.input_mut(|i| {
        i.events.retain(|e| match e {
            Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } => !consumed.contains(&(*key, *modifiers)),
            // Suppress textual continuations of an active chord. They must never leak into
            // an editor appearing during the same frame.
            Event::Text(_) if chord_text => false,
            _ => true,
        });
    });
    if !r.prefix.is_empty() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    r.commands = commands;
    let commands = take_scope(&mut r.commands, scope);
    ctx.data_mut(|d| d.insert_temp(key_id, r));
    commands
}

fn take_scope(commands: &mut Vec<Command>, scope: Scope) -> Vec<Command> {
    let selected = commands
        .iter()
        .filter(|c| c.scope() == scope)
        .copied()
        .collect();
    commands.retain(|c| c.scope() != scope);
    selected
}

pub(crate) fn which_key(ctx: &Context, refused: impl Fn(Command) -> Option<String>) {
    for scope in [Scope::Window] {
        let key = id(ctx, scope);
        let r = ctx.data(|d| d.get_temp::<Resolver>(key));
        let Some(r) =
            r.filter(|r| !r.prefix.is_empty() && ctx.input(|i| i.focused && i.time <= r.deadline))
        else {
            continue;
        };
        egui::Area::new(key.with("which-key"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -28.0])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(crate::i18n::text(
                        "interaction-control-chord-pending-escape-cancels",
                    ));
                    for c in config::commands().filter(|&c| r.eligible(c)) {
                        let seq = config::effective(&r.config, c);
                        if seq.len() > r.prefix.len() && seq[..r.prefix.len()] == r.prefix {
                            ui.horizontal(|ui| {
                                ui.monospace(
                                    seq[r.prefix.len()..]
                                        .iter()
                                        .map(Stroke::label)
                                        .collect::<Vec<_>>()
                                        .join(" → "),
                                );
                                let reason = refused(c);
                                ui.add_enabled(reason.is_none(), egui::Label::new(c.title()));
                                if let Some(reason) = reason {
                                    ui.small(reason);
                                }
                            });
                        }
                    }
                });
            });
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod owner_tests;

#[cfg(test)]
mod modal_chord_tests;

#[cfg(test)]
mod context_tests;
