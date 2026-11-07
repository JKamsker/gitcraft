//! What a resolving agent tells gitcraft about its work (over `gitcraft mcp`'s `resolve_*`
//! tools), and the progress the window keeps of it: the steps it planned, how each went, how
//! long is left.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The most steps kept: an agent planning more is cut short, not believed.
pub const MAX_STEPS: usize = 64;
/// The most characters of a title, a note or a summary kept.
pub const MAX_TEXT: usize = 600;
/// The longest estimate believed, in minutes.
pub const MAX_ESTIMATE_MINUTES: u32 = 240;

/// How a step stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

impl StepStatus {
    /// Whether the step is over, one way or another.
    pub fn is_over(self) -> bool {
        matches!(
            self,
            StepStatus::Done | StepStatus::Failed | StepStatus::Skipped
        )
    }

    pub fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "pending" => StepStatus::Pending,
            "running" => StepStatus::Running,
            "done" => StepStatus::Done,
            "failed" => StepStatus::Failed,
            "skipped" => StepStatus::Skipped,
            _ => return None,
        })
    }
}

/// A step as the agent plans it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedStep {
    pub id: String,
    pub title: String,
    /// The file it is about, if one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// One report of the agent's.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Report {
    /// The steps it is going to take, and how long it thinks they take altogether.
    Plan {
        steps: Vec<PlannedStep>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        estimate_minutes: Option<u32>,
    },
    /// A step started, ended, or has news.
    Step {
        id: String,
        status: StepStatus,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// It is done: whether every conflict was resolved, and what it did.
    Finish { ok: bool, summary: String },
}

/// A step as the window shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub title: String,
    pub file: Option<String>,
    pub status: StepStatus,
    pub note: Option<String>,
    pub started: Option<Instant>,
    pub ended: Option<Instant>,
}

/// How the agent said it ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finish {
    pub ok: bool,
    pub summary: String,
}

/// Everything the agent reported so far.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub steps: Vec<Step>,
    /// The agent's own estimate, from when it made its plan.
    estimate: Option<(Duration, Instant)>,
    pub finished: Option<Finish>,
}

/// `text` without the characters that change how the text around them reads (bidirectional
/// overrides, zero-width and control characters), its runs of white space one space, and at
/// most `max` characters long.
pub fn clean(text: &str, max: usize) -> String {
    let kept: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .filter(|&c| {
            !(c.is_control()
                || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
                    | '\u{2060}'..='\u{2069}' | '\u{feff}' | '\u{061c}' | '\u{00ad}'))
        })
        .collect();
    let words: Vec<&str> = kept.split_whitespace().collect();
    let joined = words.join(" ");
    match joined.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &joined[..cut]),
        None => joined,
    }
}

impl Progress {
    /// Takes in a report that came at `now`.
    pub fn apply(&mut self, report: Report, now: Instant) {
        match report {
            Report::Plan {
                steps,
                estimate_minutes,
            } => {
                self.plan(steps);
                if let Some(m) = estimate_minutes.filter(|m| *m > 0) {
                    let m = m.min(MAX_ESTIMATE_MINUTES);
                    self.estimate = Some((Duration::from_secs(u64::from(m) * 60), now));
                }
            }
            Report::Step { id, status, note } => self.step(&id, status, note.as_deref(), now),
            Report::Finish { ok, summary } => {
                self.finished = Some(Finish {
                    ok,
                    summary: clean(&summary, MAX_TEXT * 4),
                });
            }
        }
    }

    /// A new plan: the steps it names, in its order; one already known keeps how it stands.
    fn plan(&mut self, steps: Vec<PlannedStep>) {
        let old = std::mem::take(&mut self.steps);
        let mut seen = std::collections::HashSet::new();
        for planned in steps.into_iter().take(MAX_STEPS) {
            let id = clean(&planned.id, 80);
            if id.is_empty() || !seen.insert(id.clone()) {
                continue;
            }
            let title = clean(&planned.title, MAX_TEXT);
            let file = planned
                .file
                .map(|f| clean(&f, MAX_TEXT))
                .filter(|f| !f.is_empty());
            let step = match old.iter().find(|s| s.id == id) {
                Some(known) => Step {
                    title: if title.is_empty() {
                        known.title.clone()
                    } else {
                        title
                    },
                    file: file.or_else(|| known.file.clone()),
                    ..known.clone()
                },
                None => Step {
                    title: if title.is_empty() { id.clone() } else { title },
                    id,
                    file,
                    status: StepStatus::Pending,
                    note: None,
                    started: None,
                    ended: None,
                },
            };
            self.steps.push(step);
        }
    }

    /// News of the step `id`; one not planned is added at the end.
    fn step(&mut self, id: &str, status: StepStatus, note: Option<&str>, now: Instant) {
        let id = clean(id, 80);
        if id.is_empty() {
            return;
        }
        let i = match self.steps.iter().position(|s| s.id == id) {
            Some(i) => i,
            None if self.steps.len() < MAX_STEPS => {
                self.steps.push(Step {
                    title: id.clone(),
                    id,
                    file: None,
                    status: StepStatus::Pending,
                    note: None,
                    started: None,
                    ended: None,
                });
                self.steps.len() - 1
            }
            None => return,
        };
        let step = &mut self.steps[i];
        if status == StepStatus::Running && step.started.is_none() {
            step.started = Some(now);
        }
        if status.is_over() {
            step.started.get_or_insert(now);
            step.ended = Some(now);
        } else {
            step.ended = None;
        }
        step.status = status;
        if let Some(note) = note.map(|n| clean(n, MAX_TEXT)).filter(|n| !n.is_empty()) {
            step.note = Some(note);
        }
    }

    /// How many steps are over, of how many.
    pub fn counts(&self) -> (usize, usize) {
        let over = self.steps.iter().filter(|s| s.status.is_over()).count();
        (over, self.steps.len())
    }

    /// How far along it is, from 0 to 1: the steps over, a running one counting half.
    pub fn fraction(&self) -> f32 {
        if self.finished.is_some() {
            return 1.0;
        }
        let total = self.steps.len();
        if total == 0 {
            return 0.0;
        }
        let over = self.steps.iter().filter(|s| s.status.is_over()).count() as f32;
        let running = self
            .steps
            .iter()
            .filter(|s| s.status == StepStatus::Running)
            .count() as f32;
        ((over + running * 0.5) / total as f32).clamp(0.0, 1.0)
    }

    /// How long is likely left at `now`: from how long the steps over took, once one is, else
    /// from the agent's estimate. `None` when there is nothing to tell it from, or it is done.
    pub fn remaining(&self, now: Instant) -> Option<Duration> {
        if self.finished.is_some() {
            return None;
        }
        // Only steps seen running count: one reported over at once, or skipped, took no time
        // that tells anything.
        let took: Vec<Duration> = self
            .steps
            .iter()
            .filter(|s| matches!(s.status, StepStatus::Done | StepStatus::Failed))
            .filter_map(|s| Some(s.ended?.saturating_duration_since(s.started?)))
            .filter(|d| !d.is_zero())
            .collect();
        let left = self.steps.iter().filter(|s| !s.status.is_over()).count();
        if !took.is_empty() && left > 0 {
            let total: Duration = took.iter().sum();
            let each = total / took.len() as u32;
            // The running one is part way through.
            let running_for = self
                .steps
                .iter()
                .filter(|s| s.status == StepStatus::Running)
                .filter_map(|s| s.started)
                .map(|t| now.saturating_duration_since(t))
                .max()
                .unwrap_or_default();
            return Some((each * left as u32).saturating_sub(running_for.min(each)));
        }
        let (estimate, given) = self.estimate?;
        Some(estimate.saturating_sub(now.saturating_duration_since(given)))
    }
}

/// `1:05`, `12:30`, `1:02:03`.
pub fn clock(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// "about 3 min left", "less than a minute left".
pub fn left_words(d: Duration) -> String {
    let minutes = (d.as_secs() + 30) / 60;
    if minutes == 0 {
        "less than a minute left".to_owned()
    } else {
        format!("about {minutes} min left")
    }
}

#[cfg(test)]
mod tests;
