//! The prompt a tool is given: the template filled in, the diff cut to the size the settings
//! allow (each file keeping a fair share, lock files left out), and what was left out listed by
//! name and line counts, so the answer can still mention it. Also what is made of the answer.

use super::settings::AiSettings;

/// A prompt ready to send, and what was left out of it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prompt {
    pub text: String,
    pub files: usize,
    pub left_out: usize,
}

mod diff;
pub use diff::*;

/// Whether a commit summary follows Conventional Commits: `type(scope)!: text`.
pub fn is_conventional(subject: &str) -> bool {
    let Some((head, rest)) = subject.split_once(':') else {
        return false;
    };
    if !rest.starts_with(' ') || rest.trim().is_empty() {
        return false;
    }
    let head = head.strip_suffix('!').unwrap_or(head);
    let (kind, scope) = match head.split_once('(') {
        Some((kind, scope)) => (kind, Some(scope)),
        None => (head, None),
    };
    let kind_ok = (1..=12).contains(&kind.len()) && kind.chars().all(|c| c.is_ascii_lowercase());
    let scope_ok = scope.is_none_or(|s| {
        s.strip_suffix(')')
            .is_some_and(|inner| !inner.trim().is_empty() && !inner.contains(['(', ')']))
    });
    kind_ok && scope_ok
}

/// How many recent summaries are shown to the tool as examples.
const EXAMPLES: usize = 8;

/// What the prompt says of the repository's conventions, from its recent commit summaries
/// (newest first): Conventional Commits when most of them follow it, else their style.
pub fn conventions(subjects: &[String]) -> String {
    let subjects: Vec<&str> = subjects
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if subjects.is_empty() {
        return String::new();
    }
    let conventional = subjects.iter().filter(|s| is_conventional(s)).count();
    let mut s = if subjects.len() >= 3 && conventional * 10 >= subjects.len() * 6 {
        "This repository's history uses Conventional Commits: start the summary with a type \
         (feat, fix, docs, refactor, test, chore…), an optional scope in parentheses, then a \
         colon, like its recent summaries:\n"
            .to_owned()
    } else {
        "Match the style of this repository's recent summaries:\n".to_owned()
    };
    for subject in subjects.iter().take(EXAMPLES) {
        s.push_str(&format!("- {subject}\n"));
    }
    s
}

/// `template` with each `{name}` of `values` replaced, in one pass (a value holding `{diff}`
/// is left as it is). A placeholder alone on its line whose value is empty takes the line with
/// it. When the template has no `{diff}`, the diff goes at the end, so it is always sent.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    let mut diff_used = false;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        let found = values.iter().find(|(name, _)| {
            after
                .strip_prefix('{')
                .and_then(|a| a.strip_prefix(*name))
                .is_some_and(|a| a.starts_with('}'))
        });
        let Some((name, value)) = found else {
            out.push('{');
            rest = &after[1..];
            continue;
        };
        diff_used |= *name == "diff";
        rest = &after[name.len() + 2..];
        let alone = (out.is_empty() || out.ends_with('\n')) && rest.starts_with('\n');
        if value.is_empty() && alone {
            rest = &rest[1..];
        } else {
            out.push_str(value);
        }
    }
    out.push_str(rest);
    if !diff_used && let Some((_, diff)) = values.iter().find(|(n, _)| *n == "diff") {
        out.push_str("\n\n");
        out.push_str(diff);
    }
    out
}

/// The prompt for a commit message of `staged` (a diff), in a repository whose recent
/// summaries are `subjects`.
pub fn commit_prompt(settings: &AiSettings, staged: &str, subjects: &[String]) -> Prompt {
    let bounded = bound(staged, settings.max_diff_bytes());
    let text = fill(
        settings.commit_template(),
        &[
            ("conventions", &conventions(subjects)),
            ("left_out", &left_out_text(&bounded.left_out)),
            ("diff", &bounded.text),
        ],
    );
    Prompt {
        text,
        files: bounded.files,
        left_out: bounded.left_out.len(),
    }
}

/// What a review is about: a branch, the branch it goes into, and what it adds.
pub struct ReviewInput<'a> {
    pub branch: &'a str,
    pub base: &'a str,
    /// Its commits, a line each.
    pub commits: &'a str,
    pub diff: &'a str,
}

/// The prompt for a review of a branch.
pub fn review_prompt(settings: &AiSettings, input: &ReviewInput<'_>) -> Prompt {
    let bounded = bound(input.diff, settings.max_diff_bytes());
    let commits = if input.commits.trim().is_empty() {
        "(none)"
    } else {
        input.commits.trim_end()
    };
    let text = fill(
        settings.review_template(),
        &[
            ("branch", input.branch),
            ("base", input.base),
            ("commits", commits),
            ("left_out", &left_out_text(&bounded.left_out)),
            ("diff", &bounded.text),
        ],
    );
    Prompt {
        text,
        files: bounded.files,
        left_out: bounded.left_out.len(),
    }
}

mod answer;
pub use answer::*;

#[cfg(test)]
mod tests;
