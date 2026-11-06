//! A branch's figures: of the lines its commits added since it left the base, how many an
//! agent wrote, and how many of those were rewritten since in commits no agent is recorded
//! for: _71% agent-written, 9% of that since edited by a person_.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::{Attribution, Who, load};

/// Most commits of a branch looked at.
const MOST_COMMITS: usize = 500;
/// Most `git blame` calls made to tell what people rewrote.
const MOST_BLAMES: usize = 200;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Figures {
    pub commits: usize,
    /// Lines added, by who wrote them.
    pub agent: u64,
    pub mixed: u64,
    pub human: u64,
    pub unknown: u64,
    /// Agent-written lines a later commit with no agent recorded changed or removed.
    pub edited: u64,
    /// Not every commit or rewrite could be looked at (past the limits).
    pub partial: bool,
}

impl Figures {
    pub fn total(&self) -> u64 {
        self.agent + self.mixed + self.human + self.unknown
    }

    fn percent(part: u64, of: u64) -> u64 {
        (part * 100 + of / 2).checked_div(of).unwrap_or(0)
    }

    /// _71% agent-written, 9% of that since edited by a person_, and the rest.
    pub fn headline(&self, branch: &str) -> String {
        let total = self.total();
        if total == 0 {
            return format!("{branch} adds no lines since it left the base");
        }
        let mut parts = vec![format!(
            "{branch}: {}% agent-written ({} of {total} lines added in {} commit{})",
            Self::percent(self.agent, total),
            self.agent,
            self.commits,
            if self.commits == 1 { "" } else { "s" }
        )];
        if self.agent > 0 {
            parts.push(format!(
                "{}% of that since edited by a person",
                Self::percent(self.edited, self.agent)
            ));
        }
        if self.mixed > 0 {
            parts.push(format!(
                "{}% by a person and an agent",
                Self::percent(self.mixed, total)
            ));
        }
        if self.unknown > 0 {
            parts.push(format!(
                "{}% not recorded",
                Self::percent(self.unknown, total)
            ));
        }
        let mut line = parts.join(", ");
        if self.partial {
            line.push_str(" (the newest commits only)");
        }
        line
    }
}

/// A file and a line of it.
type Added = (String, u32);
/// A file, and the start and count of lines a hunk removes from it.
type Removed = (String, u32, u32);

/// What a `-U0` patch adds (new line numbers) and removes (old ranges), by file.
#[derive(Default)]
struct Changes {
    added: Vec<Added>,
    removed: Vec<Removed>,
}

fn changes(patch: &str) -> Changes {
    let mut out = Changes::default();
    let (mut old_path, mut new_path) = (String::new(), String::new());
    let mut new_line = 0u32;
    let mut header = false;
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            header = true;
        } else if header && let Some(p) = line.strip_prefix("--- ") {
            old_path = p
                .strip_prefix("a/")
                .unwrap_or(p)
                .trim_end_matches('\t')
                .to_owned();
        } else if header && let Some(p) = line.strip_prefix("+++ ") {
            new_path = p
                .strip_prefix("b/")
                .unwrap_or(p)
                .trim_end_matches('\t')
                .to_owned();
        } else if line.starts_with("@@") {
            header = false;
            let mut parts = line.split_whitespace().skip(1);
            let range = |p: Option<&str>, sign: char| {
                let p = p.and_then(|p| p.strip_prefix(sign)).unwrap_or("0");
                let (start, count) = p.split_once(',').unwrap_or((p, "1"));
                (
                    start.parse::<u32>().unwrap_or(0),
                    count.parse::<u32>().unwrap_or(1),
                )
            };
            let (old_start, old_count) = range(parts.next(), '-');
            let (new_start, _) = range(parts.next(), '+');
            if old_count > 0 && old_path != "/dev/null" {
                out.removed.push((old_path.clone(), old_start, old_count));
            }
            new_line = new_start;
        } else if !header && line.starts_with('+') {
            out.added.push((new_path.clone(), new_line));
            new_line += 1;
        }
    }
    out
}

/// The figures of `branch` (a full ref) since it left `base`, `known` what the commits
/// record (those not in it are read one by one).
pub fn figures(
    repo: &Path,
    branch: &str,
    base: &str,
    known: &HashMap<String, Attribution>,
) -> Result<Figures, String> {
    let git = |args: &[&str]| {
        crate::git::run(Some(repo), args)
            .map_err(|e| format!("Could not work out who wrote {branch}: {e}"))
    };
    if branch.starts_with('-') || base.starts_with('-') {
        return Err(format!("{branch} or {base} is not a branch"));
    }
    let fork = git(&["merge-base", "--end-of-options", base, branch])?;
    let range = format!("{}..{branch}", fork.trim());
    let max = format!("--max-count={}", MOST_COMMITS + 1);
    let mut commits: Vec<String> =
        git(&["rev-list", "--no-merges", &max, "--end-of-options", &range])?
            .lines()
            .map(str::to_owned)
            .collect();
    let mut figures = Figures {
        partial: commits.len() > MOST_COMMITS,
        ..Figures::default()
    };
    commits.truncate(MOST_COMMITS);
    figures.commits = commits.len();
    let in_branch: HashSet<&str> = commits.iter().map(String::as_str).collect();
    let mut read: HashMap<String, Option<Attribution>> = HashMap::new();
    let mut attribution = |hash: &str| -> Option<Attribution> {
        if let Some(a) = known.get(hash) {
            return Some(a.clone());
        }
        read.entry(hash.to_owned())
            .or_insert_with(|| load::one(repo, hash).ok().flatten())
            .clone()
    };
    let mut blames = 0;
    for hash in &commits {
        let patch = git(&[
            "-c",
            "core.quotePath=false",
            "show",
            "--no-show-signature",
            "--format=",
            "-U0",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--end-of-options",
            hash,
        ])?;
        let found = changes(&patch);
        let attr = attribution(hash);
        for (path, line) in &found.added {
            let who = attr.as_ref().map_or(Who::Unknown, |a| a.line(path, *line));
            match who {
                Who::Agent => figures.agent += 1,
                Who::Mixed => figures.mixed += 1,
                Who::Human => figures.human += 1,
                Who::Unknown => figures.unknown += 1,
            }
        }
        // A commit an agent had no part in: the agent lines it rewrote.
        let by_agent = attr
            .as_ref()
            .is_some_and(|a| matches!(a.who, Who::Agent | Who::Mixed));
        if by_agent {
            continue;
        }
        let mut by_file: HashMap<&str, Vec<(u32, u32)>> = HashMap::new();
        for (path, start, count) in &found.removed {
            by_file.entry(path).or_default().push((*start, *count));
        }
        for (path, ranges) in by_file {
            if blames >= MOST_BLAMES {
                figures.partial = true;
                break;
            }
            blames += 1;
            let parent = format!("{hash}^");
            let mut args = vec!["-c", "core.quotePath=false", "blame", "--porcelain"];
            let specs: Vec<String> = ranges.iter().map(|(s, n)| format!("{s},+{n}")).collect();
            for s in &specs {
                args.extend(["-L", s.as_str()]);
            }
            args.extend([parent.as_str(), "--", path]);
            let Ok(out) = crate::git::run(Some(repo), &args) else {
                continue;
            };
            for (origin, file, line) in blamed(&out) {
                if !in_branch.contains(origin.as_str()) {
                    continue;
                }
                if attribution(&origin).is_some_and(|a| a.line(&file, line) == Who::Agent) {
                    figures.edited += 1;
                }
            }
        }
    }
    Ok(figures)
}

/// `git blame --porcelain`: each line's commit, the file there and its line there.
fn blamed(out: &str) -> Vec<(String, String, u32)> {
    let mut files: HashMap<String, String> = HashMap::new();
    let mut lines = Vec::new();
    let mut current: Option<(String, u32)> = None;
    for line in out.lines() {
        if line.starts_with('\t') {
            if let Some((hash, orig)) = current.take() {
                let file = files.get(&hash).cloned().unwrap_or_default();
                lines.push((hash, file, orig));
            }
            continue;
        }
        let mut words = line.split(' ');
        let first = words.next().unwrap_or_default();
        if current.is_none()
            && matches!(first.len(), 40 | 64)
            && first.bytes().all(|b| b.is_ascii_hexdigit())
        {
            let orig = words.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            current = Some((first.to_owned(), orig));
        } else if let (Some((hash, _)), Some(file)) = (&current, line.strip_prefix("filename ")) {
            files.insert(hash.clone(), file.to_owned());
        }
    }
    lines
}

#[cfg(test)]
pub(super) fn changes_for_test(patch: &str) -> (Vec<Added>, Vec<Removed>) {
    let c = changes(patch);
    (c.added, c.removed)
}
