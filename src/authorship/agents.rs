//! The coding agents a commit names itself: as its author (a bot account) or in a
//! `Co-authored-by:` trailer. Only what the commit says; nothing is told from its code.

/// Known agents: what their names or emails contain (lower case), and what they are called.
const AGENTS: [(&str, &str); 16] = [
    ("noreply@anthropic.com", "Claude"),
    ("claude", "Claude"),
    ("cursoragent@cursor.com", "Cursor"),
    ("cursor agent", "Cursor"),
    ("copilot", "Copilot"),
    ("codex", "Codex"),
    ("chatgpt", "Codex"),
    ("noreply@openai.com", "Codex"),
    ("aider", "aider"),
    ("devin-ai-integration", "Devin"),
    ("google-labs-jules", "Jules"),
    ("gemini", "Gemini"),
    ("amp@ampcode.com", "Amp"),
    ("openhands", "OpenHands"),
    ("sweep-ai", "Sweep"),
    ("windsurf", "Windsurf"),
];

/// The agent `who` (a `Name <email>`, or a name and an email apart) is, if it is one.
pub fn agent(who: &str) -> Option<&'static str> {
    let lower = who.to_lowercase();
    AGENTS
        .iter()
        .find(|(mark, _)| lower.contains(mark))
        .map(|(_, name)| *name)
}

/// The agents a commit's message names in `Co-authored-by:` trailers.
pub fn co_authors(message: &str) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = message
        .lines()
        .filter_map(|l| {
            let (key, value) = l.trim().split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case("co-authored-by")
                .then(|| agent(value))
                .flatten()
        })
        .collect();
    out.dedup();
    out
}

/// The agent a commit's author is, when the author is an agent's own account (a bot, or an
/// agent's address): a person merely called Claude is not one.
pub fn author_agent(name: &str, email: &str) -> Option<&'static str> {
    let email = email.to_lowercase();
    let bot = email.contains("[bot]") || name.to_lowercase().ends_with("[bot]");
    let own = [
        "noreply@anthropic.com",
        "cursoragent@cursor.com",
        "noreply@openai.com",
        "amp@ampcode.com",
        "noreply@aider.chat",
    ]
    .iter()
    .any(|a| email == *a);
    if bot || own {
        agent(&format!("{name} {email}"))
    } else {
        None
    }
}
