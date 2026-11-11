//! The AI authorship lens: what agents record about the code they wrote, read and shown, never
//! guessed from the code itself. Three sources:
//!
//! - git-ai's line attribution in `refs/notes/ai` ([`gitai`]);
//! - Agent Trace records in `refs/notes/agent-trace` ([`trace`]);
//! - a commit's own word: an agent's account as its author, or an agent in a `Co-authored-by:`
//!   trailer ([`agents`]).
//!
//! A line or commit no source speaks of has no known author ([`Who::Unknown`]): no stripe, no
//! badge, and it counts for neither side.

pub mod display;

pub mod agents;
pub mod branch;
pub mod gitai;
pub mod load;
pub mod trace;

#[cfg(test)]
pub(crate) mod tests;

use std::collections::HashMap;

/// Who wrote a line, or a commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Who {
    Human,
    Agent,
    /// A person and an agent together (an agent co-author, or both in the same lines).
    Mixed,
    Unknown,
}

/// Ranges of a commit's lines (new side, from 1, inclusive) by file, and who wrote them.
pub type FileLines = HashMap<String, Vec<(u32, u32, Who)>>;

/// Where an attribution comes from, to say so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    GitAi,
    AgentTrace,
    Author,
    Trailer,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::GitAi => "git-ai notes",
            Source::AgentTrace => "Agent Trace",
            Source::Author => "an agent's account as the author",
            Source::Trailer => "a Co-authored-by trailer",
        }
    }
}

/// What is known of a commit's authorship.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribution {
    pub who: Who,
    /// The agents named (`Claude`, `cursor`…).
    pub agents: Vec<String>,
    pub source: Source,
    /// Line by line, when a source says (git-ai, Agent Trace).
    pub lines: FileLines,
}

impl Attribution {
    /// Who wrote line `line` (new side) of `path` in the commit: the line-level source when it
    /// covers the file, else what the commit says as a whole.
    pub fn line(&self, path: &str, line: u32) -> Who {
        match self.lines.get(path) {
            Some(ranges) => ranges
                .iter()
                .find(|(a, b, _)| (*a..=*b).contains(&line))
                .map_or(Who::Unknown, |r| r.2),
            None if self.lines.is_empty() => self.who,
            None => Who::Unknown,
        }
    }

    /// A short description for a hover: _Agent-written (Claude), from git-ai notes_.
    #[cfg(test)]
    pub fn describe(&self) -> String {
        // Line by line, a commit may record only some of its lines: it has agent lines, it
        // isn't said to be all an agent's.
        let what = match (self.who, self.lines.is_empty()) {
            (Who::Agent, true) => "Agent-written",
            (Who::Agent, false) => "Has agent-written lines",
            (Who::Mixed, true) => "Co-written by a person and an agent",
            (Who::Mixed, false) => "Has lines by agents and by people",
            (Who::Human, _) => "Written by a person",
            (Who::Unknown, _) => "Author not recorded",
        };
        let agents = if self.agents.is_empty() {
            String::new()
        } else {
            format!(" ({})", self.agents.join(", "))
        };
        format!("{what}{agents}, from {}", self.source.label())
    }
}

/// Who a commit is as a whole from line ranges: agent only, people only, or both.
fn overall(lines: &FileLines) -> Who {
    let all: Vec<Who> = lines.values().flatten().map(|r| r.2).collect();
    let agent = all.iter().any(|w| matches!(w, Who::Agent | Who::Mixed));
    let human = all.iter().any(|w| matches!(w, Who::Human | Who::Mixed));
    match (agent, human) {
        (true, true) => Who::Mixed,
        (true, false) => Who::Agent,
        (false, true) => Who::Human,
        (false, false) => Who::Unknown,
    }
}

/// What a commit's message and author, and its notes when it has some, say of it; `None`
/// when nothing does.
pub fn attribute(
    author: &str,
    email: &str,
    message: &str,
    git_ai: Option<&str>,
    agent_trace: Option<&str>,
) -> Option<Attribution> {
    // A note that attributes nothing leaves the word to the commit's author and trailers.
    if let Some(note) = git_ai.and_then(gitai::parse)
        && overall(&note.files) != Who::Unknown
    {
        return Some(Attribution {
            who: overall(&note.files),
            agents: note.tools,
            source: Source::GitAi,
            lines: note.files,
        });
    }
    if let Some((lines, tools)) = agent_trace.and_then(trace::parse)
        && overall(&lines) != Who::Unknown
    {
        return Some(Attribution {
            who: overall(&lines),
            agents: tools,
            source: Source::AgentTrace,
            lines,
        });
    }
    if let Some(agent) = agents::author_agent(author, email) {
        return Some(Attribution {
            who: Who::Agent,
            agents: vec![agent.to_owned()],
            source: Source::Author,
            lines: FileLines::default(),
        });
    }
    let co = agents::co_authors(message);
    (!co.is_empty()).then(|| Attribution {
        who: Who::Mixed,
        agents: co.iter().map(|a| (*a).to_owned()).collect(),
        source: Source::Trailer,
        lines: FileLines::default(),
    })
}
