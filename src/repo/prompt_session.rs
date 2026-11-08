//! Commit-local reader UI. Every Git/file read stays on a worker; late results are
//! bound to immutable commit, worktree, profile and a per-request generation.
use super::{Action, RepoTab};
use crate::git::prompt_session::{self as logic, Evidence, Options};
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone, Debug)]
pub(crate) struct Hunk {
    pub commit: String,
    pub path: String,
    pub range: (u32, u32),
    pub old: bool,
    pub alternate: Option<(u32, u32)>,
}
pub(super) struct State {
    pub open: bool,
    pub local: bool,
    pub roots: String,
    pub hours: u32,
    request: u64,
    pub requested: Option<String>,
    result: Option<Result<Evidence, String>>,
    loading: bool,
    hunk: Option<Hunk>,
    jump: Option<(usize, usize)>,
}
impl Default for State {
    fn default() -> Self {
        let o = Options::default();
        Self {
            open: false,
            local: o.include_local,
            roots: o
                .roots
                .iter()
                .filter_map(|p| p.to_str())
                .collect::<Vec<_>>()
                .join("\n"),
            hours: o.hours,
            request: 0,
            requested: None,
            result: None,
            loading: false,
            hunk: None,
            jump: None,
        }
    }
}
impl RepoTab {
    pub(super) fn show_prompt_hunk(&mut self, seed: Hunk, ctx: &egui::Context) {
        if !crate::git::investigations::oid(&seed.commit)
            || seed.range.0 == 0
            || seed.range.0 > seed.range.1
        {
            return;
        }
        let commit = seed.commit.clone();
        self.apply(Action::SelectHash(commit.clone()), ctx);
        self.prompt_session.open = true;
        self.prompt_session.hunk = Some(seed);
        if self.prompt_session.requested.as_deref() != Some(&commit)
            || self.prompt_session.result.is_none()
        {
            self.load_prompt_session(commit, ctx);
        } else {
            self.prompt_session.jump = matches(&self.prompt_session).first().copied();
        }
    }
    pub(super) fn load_prompt_session(&mut self, commit: String, ctx: &egui::Context) {
        if !crate::git::investigations::oid(&commit) {
            return;
        }
        static IDS: AtomicU64 = AtomicU64::new(1);
        let id = IDS.fetch_add(1, Ordering::Relaxed);
        let repo = self.path.clone();
        let expected = repo.clone();
        let profile = crate::git::profile_config();
        let options = Options {
            include_local: self.prompt_session.local,
            roots: self
                .prompt_session
                .roots
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .take(8)
                .map(Into::into)
                .collect(),
            hours: self.prompt_session.hours.clamp(1, 168),
        };
        let asked = commit.clone();
        self.prompt_session.request = id;
        self.prompt_session.requested = Some(commit.clone());
        self.prompt_session.result = None;
        self.prompt_session.loading = true;
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                logic::load(&repo, &asked, &options, &|| {
                    !crate::tasks::cancellation().is_cancelled()
                })
            },
            move |tab, result, _| {
                if tab.prompt_session.request != id || tab.prompt_session.requested.as_deref() != Some(&commit) {return;}
                tab.prompt_session.loading = false;
                if tab.path != expected || crate::git::profile_config() != profile {
                    tab.prompt_session.result=Some(Err("Worktree or profile changed while reading; read evidence again in the selected context".into()));return;
                }
                tab.prompt_session.result = Some(result);
                tab.prompt_session.jump = matches(&tab.prompt_session).first().copied();
            },
        );
    }
    pub(super) fn prompt_session_panel(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let hash = self.detail_hash.clone().unwrap_or_default();
        panel(ui, &mut self.prompt_session, &hash, acts);
    }
}
fn matches(state: &State) -> Vec<(usize, usize)> {
    let Some(hunk) = &state.hunk else {
        return vec![];
    };
    let Some(Ok(e)) = &state.result else {
        return vec![];
    };
    if hunk.commit != e.commit {
        return vec![];
    }
    let mut matched = logic::hunk_steps(e, &hunk.path, hunk.range, hunk.old);
    if let Some(other) = hunk.alternate {
        matched.extend(logic::hunk_steps(e, &hunk.path, other, !hunk.old));
    }
    matched.sort_unstable();
    matched.dedup();
    matched
}
fn panel(ui: &mut egui::Ui, state: &mut State, hash: &str, acts: &mut Vec<Action>) {
    ui.heading("Prompt & session");
    ui.label("Local evidence only. Nothing is uploaded, executed or exported through MCP. Entire checkpoints already in Git and the recorded commit message are read; local Claude/Codex discovery is opt in. Multiple time/path matches remain separate candidates.");
    ui.checkbox(
        &mut state.local,
        "Include local Claude Code and Codex transcripts",
    );
    if state.local {
        ui.label("Physical transcript roots, one per line (up to eight). Only bounded JSONL files are read; config files are excluded by the JSONL-only allowlist.");
        ui.add(egui::TextEdit::multiline(&mut state.roots).desired_rows(3));
        ui.horizontal(|ui| {
            ui.label("Match records within hours before commit:");
            ui.add(egui::DragValue::new(&mut state.hours).range(1..=168));
            ui.label("and 10 minutes after");
        });
    }
    ui.label("Read limits: 4 MiB per file, 32 MiB total, 200 newest local files/root, 2,000 steps/transcript. Unavailable, truncated or ambiguous evidence is reported explicitly.");
    if ui
        .add_enabled(
            !state.loading && !hash.is_empty(),
            egui::Button::new("Read local prompt/session evidence"),
        )
        .clicked()
    {
        acts.push(Action::LoadPromptSession(hash.into()));
    }
    if state.loading {
        ui.label("Reading bounded local evidence…");
        return;
    }
    if state.requested.as_deref() != Some(hash) {
        ui.label(
            "Read evidence for this selected commit. Previous commit evidence is not shown here.",
        );
        return;
    }
    let matching = matches(state);
    let Some(result) = &state.result else {
        return;
    };
    let e = match result {
        Ok(e) => e,
        Err(error) => {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, error);
            return;
        }
    };
    if let Some(hunk) = &state.hunk
        && hunk.commit == e.commit
    {
        ui.strong(format!(
            "Hunk {}:{}–{} ({})",
            logic::redact(&hunk.path),
            hunk.range.0,
            hunk.range.1,
            if hunk.old { "old" } else { "new" }
        ));
        if matching.is_empty() {
            ui.label("No unique recorded edit snippet matches these commit lines. A session/time match alone is not hunk attribution.");
        } else {
            ui.label(format!(
                "{} recorded content match(es); inspect each candidate step",
                matching.len()
            ));
            ui.horizontal_wrapped(|ui| {
                for (i, j) in &matching {
                    if ui
                        .button(format!("Session {}, step {}", i + 1, j + 1))
                        .clicked()
                    {
                        state.jump = Some((*i, *j));
                    }
                }
            });
        }
    }
    egui::ScrollArea::vertical().id_salt("prompt-session-local").show(ui,|ui|{
        for notice in &e.notices {ui.weak(notice);}
        for (i,session) in e.sessions.iter().enumerate() {
            ui.separator();ui.strong(&session.source);ui.label(&session.relation);
            for (j,step) in session.steps.iter().enumerate() {
                let highlighted=matching.contains(&(i,j));
                let response=egui::Frame::new().inner_margin(6.0).fill(if highlighted{crate::theme::Tokens::get(ui.ctx()).accent.gamma_multiply(0.12)}else{egui::Color32::TRANSPARENT}).show(ui,|ui|{
                    ui.strong(format!("Step {} · {}{}",j+1,step.kind,step.at.map_or(String::new(),|t|format!(" · UTC epoch {t}"))));
                    ui.label(&step.text);
                    if let Some(path)=&step.path {
                        ui.monospace(logic::redact(path));
                        if step.ranges.is_empty(){ui.weak("No unique exact old/new content match in this commit; no line attribution asserted.");}
                        for span in &step.ranges {
                            if ui.button(if span.old {format!("Open commit diff · parent lines {}–{}",span.start,span.end)}else{format!("Open commit lines {}–{}",span.start,span.end)}).clicked(){
                                if !span.old {acts.push(Action::OpenPinnedFileLine(crate::git::investigations::dossier::Seed{commit:e.commit.clone(),path:path.clone(),line:span.start as usize}));}
                                else{acts.push(Action::OpenDiff(super::DiffKey::Commit{hash:e.commit.clone(),path:path.clone()}));}
                            }
                        }
                    }
                    if !step.before.is_empty(){ui.label("Recorded before:");ui.monospace(&step.before);}
                    if !step.after.is_empty(){ui.label("Recorded after:");ui.monospace(&step.after);}
                }).response;
                if state.jump==Some((i,j)){response.scroll_to_me(Some(egui::Align::Center));state.jump=None;}
            }
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_prompt_panel_hunk_jump_uses_recorded_range_and_read_action_pins_selected_oid()
    -> Result<(), String> {
        let hash = "a".repeat(40);
        let mut state = State {
            requested: Some(hash.clone()),
            ..Default::default()
        };
        state.result = Some(Ok(Evidence {
            commit: hash.clone(),
            sessions: vec![logic::Session {
                source: "synthetic local".into(),
                relation: "candidate".into(),
                steps: vec![logic::Step {
                    kind: "Tool",
                    text: "Edit".into(),
                    at: None,
                    path: Some("file".into()),
                    before: "old".into(),
                    after: "new".into(),
                    exact: true,
                    ranges: vec![logic::Span {
                        start: 5,
                        end: 6,
                        old: false,
                    }],
                }],
            }],
            notices: vec![],
        }));
        state.hunk = Some(Hunk {
            commit: hash.clone(),
            path: "file".into(),
            range: (6, 8),
            old: false,
            alternate: None,
        });
        assert_eq!(matches(&state), vec![(0, 0)]);
        let ctx = egui::Context::default();
        let mut acts = Vec::new();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 1600.0),
                )),
                ..Default::default()
            },
            |ui| panel(ui, &mut state, &hash, &mut acts),
        );
        let texts = out
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(texts.contains(&"1 recorded content match(es); inspect each candidate step"));
        let p = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == "Open commit lines 5–6" => {
                    Some(t.pos + t.galley.size() / 2.0)
                }
                _ => None,
            })
            .ok_or("expected Some")?;
        out.textures_delta.clear();
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(p),
                    egui::Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| panel(ui, &mut state, &hash, &mut acts),
        );
        out.textures_delta.clear();
        assert!(acts.iter().any(|a|matches!(a,Action::OpenPinnedFileLine(s)if s.commit==hash&&s.path=="file"&&s.line==5)));
        state.hunk.as_mut().ok_or("expected Some")?.commit = "b".repeat(40);
        assert!(matches(&state).is_empty());
        Ok(())
    }
}
