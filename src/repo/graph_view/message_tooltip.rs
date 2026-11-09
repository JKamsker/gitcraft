//! Lazy messages for graph tooltips. History retains only its compact preview.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use crate::repo::RepoTab;

const CACHE_ENTRIES: usize = 8;
const HOVER_DELAY: f64 = 0.35;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Request {
    id: u64,
    path: PathBuf,
    hash: String,
}

#[derive(Default)]
pub(in crate::repo) struct MessageTooltips {
    scope: Option<PathBuf>,
    hover: Option<(String, f64)>,
    pending: Option<Request>,
    next_id: u64,
    cache: VecDeque<(String, Result<String, String>)>,
}

impl MessageTooltips {
    pub(super) fn message(&self, path: &Path, hash: &str) -> Option<&Result<String, String>> {
        if self.scope.as_deref() != Some(path) {
            return None;
        }
        self.cache
            .iter()
            .find(|(key, _)| key == hash)
            .map(|(_, message)| message)
    }

    fn hovered(&mut self, path: &Path, hash: Option<&str>, now: f64) -> Option<Request> {
        if self.scope.as_deref() != Some(path) {
            self.scope = Some(path.to_owned());
            self.cache.clear();
            self.hover = None;
            // Keep an old worker's slot until it returns: rapid changes cannot spawn an
            // unbounded queue of Git processes. Its path ownership prevents stale display.
        }
        let Some(hash) = hash else {
            self.hover = None;
            return None;
        };
        if self.hover.as_ref().is_none_or(|(key, _)| key != hash) {
            // A transient failure can be retried by leaving and hovering again. Staying
            // over the error keeps it readable instead of issuing a request every frame.
            self.cache
                .retain(|(key, result)| key != hash || result.is_ok());
            self.hover = Some((hash.to_owned(), now));
        }
        let since = self.hover.as_ref().map_or(now, |(_, since)| *since);
        if self.pending.is_some() || self.message(path, hash).is_some() || now - since < HOVER_DELAY
        {
            return None;
        }
        self.next_id = self.next_id.wrapping_add(1);
        let request = Request {
            id: self.next_id,
            path: path.to_owned(),
            hash: hash.to_owned(),
        };
        self.pending = Some(request.clone());
        Some(request)
    }

    fn arrived(&mut self, current_path: &Path, request: Request, result: Result<String, String>) {
        if self.pending.as_ref() != Some(&request) {
            return;
        }
        self.pending = None;
        if current_path != request.path || self.scope.as_ref() != Some(&request.path) {
            return;
        }
        // Hash-keyed immutable objects may finish after hover or selection moves. Reuse the
        // result for that hash only, never for the row now occupying the same screen position.
        let result = result.map_err(|error| error.chars().take(2048).collect());
        self.cache.push_back((request.hash, result));
        while self.cache.len() > CACHE_ENTRIES {
            self.cache.pop_front();
        }
    }
}

impl RepoTab {
    pub(super) fn hover_graph_message(&mut self, ctx: &egui::Context, hash: Option<&str>) {
        let now = ctx.input(|i| i.time);
        let request = self.graph_messages.hovered(&self.path, hash, now);
        if hash.is_some()
            && self
                .graph_messages
                .message(&self.path, hash.unwrap_or_default())
                .is_none()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(request) = request {
            let path = request.path.clone();
            let hash = request.hash.clone();
            self.then_or(
                ctx,
                false,
                Err,
                move || crate::git::commit_message(&path, &hash),
                move |tab, result, _| {
                    tab.graph_messages.arrived(&tab.path, request, result);
                },
            );
        }
    }
}

#[derive(Clone)]
struct OpenMessage {
    since: f64,
    rect: Option<egui::Rect>,
}

/// Retain ownership while the pointer moves into the message and while it scrolls.
/// Ordinary hover tooltips close on wheel events, even with an internal ScrollArea.
pub(super) fn show(
    ui: &mut egui::Ui,
    response: &egui::Response,
    description: egui::Rect,
    path: &Path,
    hash: &str,
    message: Option<&Result<String, String>>,
) -> bool {
    let id = response.id.with(("full-message-popup", path, hash));
    let now = ui.input(|i| i.time);
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let anchor_hovered = response.hovered() && ui.rect_contains_pointer(description);
    let mut state = ui.data(|d| d.get_temp::<OpenMessage>(id));
    let over_popup = state
        .as_ref()
        .and_then(|s| s.rect)
        .is_some_and(|r| pointer.is_some_and(|p| r.expand(6.0).contains(p)));
    if (!anchor_hovered && !over_popup) || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.data_mut(|d| d.remove::<OpenMessage>(id));
        return false;
    }
    let state = state.get_or_insert(OpenMessage {
        since: now,
        rect: None,
    });
    let delay = f64::from(ui.style().interaction.tooltip_delay).max(HOVER_DELAY);
    if now - state.since >= delay {
        let width = (ui.ctx().content_rect().width() - 24.0).clamp(100.0, 480.0);
        if let Some(shown) = egui::Tooltip::for_widget(response).width(width).show(|ui| {
            egui::ScrollArea::vertical()
                .id_salt(("full-commit-message", path, hash))
                .max_height((ui.ctx().content_rect().height() * 0.6).max(60.0))
                .show(ui, |ui| {
                    ui.label(match message {
                        Some(Ok(message)) => message.as_str(),
                        Some(Err(error)) => error.as_str(),
                        None => crate::i18n::text("repo-core-2668"),
                    });
                });
        }) {
            state.rect = Some(shown.response.rect);
        }
    } else {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(
                (delay - (now - state.since)).max(0.001),
            ));
    }
    ui.data_mut(|d| d.insert_temp(id, state.clone()));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_message_read_releases_pending_and_can_retry() {
        let ctx = egui::Context::default();
        let mut tab = RepoTab::new_deferred("unread-message-repository".into(), &ctx);
        tab.tx.cancel.cancel();
        for time in [0.0, 1.0] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ui| {
                    tab.hover_graph_message(ui.ctx(), Some("hash"));
                },
            );
            output.textures_delta.clear();
        }
        let start = std::time::Instant::now();
        while tab.graph_messages.pending.is_some()
            && start.elapsed() < std::time::Duration::from_secs(5)
        {
            tab.poll(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(tab.graph_messages.pending.is_none());
        assert!(
            matches!(tab.graph_messages.message(&tab.path, "hash"), Some(Err(e)) if e.contains("cancelled"))
        );
        tab.graph_messages.hovered(&tab.path, None, 2.0);
        tab.graph_messages.hovered(&tab.path, Some("hash"), 3.0);
        assert!(
            tab.graph_messages
                .hovered(&tab.path, Some("hash"), 4.0)
                .is_some()
        );
    }

    #[test]
    fn hover_requests_are_debounced_serialized_hash_owned_and_bounded() {
        let path = Path::new("repo");
        let mut cache = MessageTooltips::default();
        assert!(cache.hovered(path, Some("a"), 0.0).is_none());
        let request = cache.hovered(path, Some("a"), 1.0);
        assert!(request.is_some());
        assert!(cache.hovered(path, Some("b"), 2.0).is_none());
        assert!(
            cache.hovered(path, Some("b"), 3.0).is_none(),
            "only one worker at a time"
        );
        if let Some(request) = request {
            cache.arrived(path, request, Ok("full a".to_owned()));
        }
        assert!(cache.message(path, "b").is_none());
        assert!(cache.message(path, "a").is_some());
        let second = cache.hovered(path, Some("b"), 4.0);
        assert!(second.is_some());
        if let Some(request) = second {
            cache.arrived(path, request, Err("unavailable b".to_owned()));
        }
        assert!(
            cache.hovered(path, Some("b"), 5.0).is_none(),
            "errors are cached too"
        );
        assert!(
            cache.hovered(path, Some("a"), 6.0).is_none(),
            "revisiting a hash reuses its message"
        );
        assert!(cache.hovered(path, Some("b"), 7.0).is_none());
        let retry = cache.hovered(path, Some("b"), 8.0);
        assert!(
            retry.is_some(),
            "a failed read can be retried on a fresh hover"
        );
        if let Some(request) = retry {
            cache.arrived(path, request, Ok("recovered b".to_owned()));
        }
        for i in 0..20 {
            let hash = i.to_string();
            cache.hovered(path, Some(&hash), 10.0 + i as f64 * 2.0);
            let request = cache.hovered(path, Some(&hash), 11.0 + i as f64 * 2.0);
            assert!(request.is_some());
            if let Some(request) = request {
                cache.arrived(path, request, Ok(hash));
            }
        }
        assert_eq!(cache.cache.len(), CACHE_ENTRIES);
        assert!(cache.message(path, "a").is_none());
    }

    #[test]
    fn moved_repository_and_superseded_worker_cannot_populate_current_tooltip() {
        let old = Path::new("old");
        let new = Path::new("new");
        let mut cache = MessageTooltips::default();
        cache.hovered(old, Some("same hash"), 0.0);
        let request = cache.hovered(old, Some("same hash"), 1.0);
        assert!(request.is_some());
        cache.hovered(new, Some("same hash"), 2.0);
        if let Some(request) = request {
            cache.arrived(new, request, Ok("old repository".to_owned()));
        }
        assert!(cache.message(new, "same hash").is_none());
        let current = cache.hovered(new, Some("same hash"), 3.0);
        assert!(current.is_some());
        if let Some(request) = current {
            let mut stale = request.clone();
            stale.id = stale.id.wrapping_add(1);
            cache.arrived(new, stale, Ok("stale worker".to_owned()));
            assert!(cache.pending.is_some());
            cache.arrived(new, request, Ok("current repository".to_owned()));
        }
        assert!(
            matches!(cache.message(new, "same hash"), Some(Ok(text)) if text == "current repository")
        );
        assert!(cache.message(old, "same hash").is_none());
        cache.hovered(new, None, 4.0);
        assert!(cache.hover.is_none());
    }
}
