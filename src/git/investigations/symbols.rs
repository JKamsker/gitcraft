//! Structural function history. Pinned trees and rename/copy edges narrow similarity matches;
//! ambiguous lineage and inferred splits are exposed instead of silently picking a new symbol.
#[cfg(feature = "symbol-history")]
mod syntax;
#[cfg(not(feature = "symbol-history"))]
#[path = "symbols/disabled.rs"]
mod syntax;
#[cfg(test)]
mod tests;
use super::{Reader, oid};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Symbol {
    pub name: String,
    pub qualified: String,
    pub path: String,
    pub first: usize,
    pub last: usize,
    pub body: String,
    pub blob: String,
    tokens: Vec<String>,
}
#[derive(Clone, Debug)]
pub(crate) struct Step {
    pub commit: String,
    pub author: String,
    pub time: i64,
    pub subject: String,
    pub event: String,
    pub symbol: Option<Symbol>,
    pub related: Vec<Symbol>,
    pub confidence: Option<u8>,
}
#[derive(Clone, Debug)]
pub(crate) struct Biography {
    pub tip: String,
    pub steps: Vec<Step>,
    pub notes: Vec<String>,
    pub limited: bool,
}
#[derive(Clone, Debug)]
pub(crate) enum Answer {
    Choices(Vec<Symbol>),
    History(Biography),
}
#[derive(Clone, Debug, Default)]
struct Changes {
    old: BTreeSet<String>,
    new: BTreeSet<String>,
    edges: Vec<(String, String)>,
    limited: bool,
}
struct Trace<'a> {
    reader: Reader<'a>,
    files: BTreeMap<(String, String), Arc<Vec<Symbol>>>,
    changes: BTreeMap<String, Changes>,
    bytes: usize,
}
fn path_ok(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|p| !matches!(p, "" | "." | ".." | ".git"))
}
impl Trace<'_> {
    fn file(&mut self, at: &str, path: &str) -> Result<Arc<Vec<Symbol>>, String> {
        let key = (at.to_owned(), path.to_owned());
        if let Some(symbols) = self.files.get(&key) {
            return Ok(symbols.clone());
        }
        if !self.reader.active() {
            return Err("Symbol history cancelled or reached its time budget".into());
        }
        if self.files.len() >= 1500 {
            return Err(
                "Symbol history reached 1500 file versions; reduce the history depth".into(),
            );
        }
        if !path_ok(path) {
            return Err("Unsafe symbol history path".into());
        }
        if !syntax::supported(path) {
            return Ok(Arc::new(Vec::new()));
        }
        let tree = self.reader.run(
            &["--literal-pathspecs", "ls-tree", "-z", at, "--", path],
            vec![],
            &[0],
        )?;
        if tree.is_empty() {
            let result = Arc::new(Vec::new());
            self.files.insert(key, result.clone());
            return Ok(result);
        }
        let fields = tree
            .split(|b| *b == b'\t')
            .next()
            .ok_or("Invalid symbol file metadata")?;
        let fields = std::str::from_utf8(fields)
            .map_err(|_| "Invalid symbol metadata")?
            .split_whitespace()
            .collect::<Vec<_>>();
        let [mode, kind, blob] = fields.as_slice() else {
            return Err("Invalid symbol file metadata".into());
        };
        if !matches!(*mode, "100644" | "100755") || *kind != "blob" || !oid(blob) {
            return Err("Symbol history supports regular source files only".into());
        }
        let size = self.reader.run(&["cat-file", "-s", blob], vec![], &[0])?;
        let size = std::str::from_utf8(&size)
            .map_err(|_| "Invalid source size")?
            .trim()
            .parse::<usize>()
            .map_err(|_| "Invalid source size")?;
        if size > 2 * 1024 * 1024 || self.bytes.saturating_add(size) > 64 * 1024 * 1024 {
            return Err("Symbol history reached 2 MiB/file or 64 MiB total source budget".into());
        }
        let bytes = self.reader.run(&["cat-file", "blob", blob], vec![], &[0])?;
        if bytes.len() != size || bytes.contains(&0) {
            return Err("Binary or incomplete source is not parsed".into());
        }
        let text =
            std::str::from_utf8(&bytes).map_err(|_| "Symbol history requires UTF-8 source")?;
        self.bytes += size;
        let symbols = Arc::new(syntax::parse(path, text, blob, &|| self.reader.active())?);
        self.files.insert(key, symbols.clone());
        Ok(symbols)
    }
    fn diff(&mut self, parent: &str, child: &str) -> Result<Changes, String> {
        if let Some(changes) = self.changes.get(child) {
            return Ok(changes.clone());
        }
        let bytes = self.reader.run(
            &[
                "diff-tree",
                "-r",
                "-z",
                "--no-commit-id",
                "--name-status",
                "--no-ext-diff",
                "--no-textconv",
                "-M20%",
                "-C20%",
                "--find-copies-harder",
                parent,
                child,
                "--",
            ],
            vec![],
            &[0],
        )?;
        let fields = bytes
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>();
        let mut i = 0;
        let mut changes = Changes::default();
        while i < fields.len() {
            let status = fields[i];
            i += 1;
            let path = std::str::from_utf8(*fields.get(i).ok_or("Truncated file change")?)
                .map_err(|_| "Non-UTF-8 file history")?;
            i += 1;
            if !path_ok(path) {
                changes.limited = true;
                continue;
            }
            if matches!(status.first(), Some(b'R' | b'C')) {
                let next = std::str::from_utf8(*fields.get(i).ok_or("Truncated rename")?)
                    .map_err(|_| "Non-UTF-8 rename")?;
                i += 1;
                if !path_ok(next) {
                    changes.limited = true;
                    continue;
                }
                changes.old.insert(path.into());
                changes.new.insert(next.into());
                changes.edges.push((path.into(), next.into()));
            } else {
                if status.first() != Some(&b'A') {
                    changes.old.insert(path.into());
                }
                if status.first() != Some(&b'D') {
                    changes.new.insert(path.into());
                }
            }
        }
        if changes.old.len() > 50 || changes.new.len() > 50 {
            changes.limited = true;
        }
        self.changes.insert(child.into(), changes.clone());
        Ok(changes)
    }
    fn candidates(
        &mut self,
        at: &str,
        current: &Symbol,
        changes: &Changes,
        forward: bool,
    ) -> Result<Vec<Symbol>, String> {
        let mut paths = if forward {
            changes.new.clone()
        } else {
            changes.old.clone()
        };
        paths.insert(current.path.clone());
        for (old, new) in &changes.edges {
            if forward && old == &current.path {
                paths.insert(new.clone());
            }
            if !forward && new == &current.path {
                paths.insert(old.clone());
            }
        }
        // Keep the known path first even at a bulk transition. Refuse a false creation/deletion
        // when other changed source files fell outside the bounded candidate set.
        let mut ordered = vec![current.path.clone()];
        ordered.extend(paths.into_iter().filter(|p| p != &current.path).take(50));
        let mut out = Vec::new();
        for path in ordered {
            out.extend(self.file(at, &path)?.iter().cloned());
            if out.len() > 10000 {
                return Err("Too many structural function candidates".into());
            }
        }
        Ok(out)
    }
    fn step(
        &self,
        commit: &str,
        event: String,
        symbol: Option<Symbol>,
        related: Vec<Symbol>,
        confidence: Option<u8>,
    ) -> Result<Step, String> {
        let bytes = self.reader.run(
            &[
                "show",
                "-s",
                "--no-show-signature",
                "--format=%an%x00%ct%x00%s%x00",
                commit,
                "--",
            ],
            vec![],
            &[0],
        )?;
        let fields = bytes.split(|b| *b == 0).collect::<Vec<_>>();
        if fields.len() != 4 {
            return Err("Invalid symbol event metadata".into());
        }
        Ok(Step {
            commit: commit.into(),
            author: String::from_utf8_lossy(fields[0])
                .chars()
                .take(200)
                .collect(),
            time: std::str::from_utf8(fields[1])
                .map_err(|_| "Invalid event time")?
                .parse()
                .map_err(|_| "Invalid event time")?,
            subject: String::from_utf8_lossy(fields[2])
                .chars()
                .take(500)
                .collect(),
            event,
            symbol,
            related,
            confidence,
        })
    }
}
fn fingerprints(tokens: &[String]) -> BTreeSet<String> {
    if tokens.len() < 3 {
        return tokens.iter().cloned().collect();
    }
    tokens.windows(3).map(|w| w.join("\0")).collect()
}
fn similarity(a: &Symbol, b: &Symbol) -> f32 {
    let a = fingerprints(&a.tokens);
    let b = fingerprints(&b.tokens);
    if a.is_empty() || b.is_empty() {
        return 0.;
    }
    (2 * a.intersection(&b).count()) as f32 / (a.len() + b.len()) as f32
}
fn choose(current: &Symbol, candidates: &[Symbol]) -> Result<Option<(Symbol, u8)>, String> {
    let mut ranked = candidates
        .iter()
        .filter_map(|s| {
            let score = similarity(current, s);
            let same = s.path == current.path && s.qualified == current.qualified;
            (score >= 0.55 || (same && score >= 0.20)).then_some((
                s,
                score,
                if same { 0.15 } else { 0. },
            ))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|a, b| (b.1 + b.2).total_cmp(&(a.1 + a.2)));
    let Some((best, score, bonus)) = ranked.first() else {
        return Ok(None);
    };
    if ranked
        .get(1)
        .is_some_and(|other| ((other.1 + other.2) - (score + bonus)).abs() < 0.04)
    {
        return Err("Two functions have similarly strong bodies; lineage is ambiguous. Open a nearer revision or choose a qualified function name.".into());
    }
    Ok(Some(((*best).clone(), (score * 100.).round() as u8)))
}
fn event(before: &Symbol, after: &Symbol) -> String {
    let mut events = Vec::new();
    if before.path != after.path {
        events.push("Moved");
    }
    if before.qualified != after.qualified {
        events.push("Renamed");
    }
    if before.tokens != after.tokens {
        events.push("Body edited");
    } else if before.body != after.body {
        events.push("Reformatted");
    }
    if events.is_empty() {
        "Selected version".into()
    } else {
        events.join(" · ")
    }
}
fn splits(before: &Symbol, primary: &Symbol, all: &[Symbol]) -> Vec<Symbol> {
    let old = fingerprints(&before.tokens);
    let primary_fp = fingerprints(&primary.tokens);
    if old.len() < 12 {
        return vec![];
    }
    let covered = old
        .intersection(&primary_fp)
        .cloned()
        .collect::<BTreeSet<_>>();
    all.iter()
        .filter(|s| s.path != primary.path || s.first != primary.first)
        .filter(|s| {
            let fp = fingerprints(&s.tokens);
            let overlap = old.intersection(&fp).count();
            let union = covered.union(&fp).filter(|p| old.contains(*p)).count();
            overlap >= 6
                && overlap * 100 >= fp.len() * 55
                && union * 100 >= old.len() * 65
                && primary_fp.intersection(&fp).count() * 100 < fp.len() * 45
        })
        .take(8)
        .cloned()
        .collect()
}
pub(crate) fn investigate(
    repo: &Path,
    revision: &str,
    path: &str,
    name: &str,
    line: Option<usize>,
    depth: usize,
    active: &dyn Fn() -> bool,
) -> Result<Answer, String> {
    if !cfg!(feature = "symbol-history") {
        return Err(
            "This build has symbol history disabled; enable the symbol-history Cargo feature"
                .into(),
        );
    }
    if !path_ok(path) || !syntax::supported(path) {
        return Err(
            "Choose a Rust, Python, JavaScript, TypeScript/TSX, Go, C or C++ source file".into(),
        );
    }
    if !(16..=2048).contains(&depth) {
        return Err("History depth must be 16–2048 first-parent commits".into());
    }
    if revision.starts_with('-') || revision.chars().any(char::is_control) || revision.len() > 256 {
        return Err("Invalid history revision".into());
    }
    let mut trace = Trace {
        reader: Reader::new(repo, active),
        files: BTreeMap::new(),
        changes: BTreeMap::new(),
        bytes: 0,
    };
    let resolved = trace.reader.run(
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{revision}^{{commit}}"),
        ],
        vec![],
        &[0],
    )?;
    let selected = std::str::from_utf8(&resolved)
        .map_err(|_| "Invalid revision")?
        .trim()
        .to_owned();
    if !oid(&selected) {
        return Err("Invalid pinned revision".into());
    }
    let head = trace
        .reader
        .run(&["rev-parse", "--verify", "HEAD"], vec![], &[0])?;
    let head = std::str::from_utf8(&head)
        .map_err(|_| "Invalid HEAD")?
        .trim()
        .to_owned();
    let mut tip = head.clone();
    let mut ids = trace.reader.run(
        &[
            "rev-list",
            "--first-parent",
            &format!("--max-count={}", depth + 1),
            &tip,
            "--",
        ],
        vec![],
        &[0],
    )?;
    let parse_ids = |bytes: &[u8]| -> Result<Vec<String>, String> {
        let text = std::str::from_utf8(bytes).map_err(|_| "Invalid history")?;
        text.lines()
            .map(|s| {
                if oid(s) {
                    Ok(s.into())
                } else {
                    Err("Invalid history ID".into())
                }
            })
            .collect()
    };
    let mut commits = parse_ids(&ids)?;
    let mut notes=vec!["Body similarity and split relationships are structural heuristics; inspect the pinned source. Merge events follow the first parent; alternative branch lifetimes can be opened separately. Reads do not fetch missing objects or run repository tools.".into()];
    if !commits.contains(&selected) {
        tip = selected.clone();
        ids = trace.reader.run(
            &[
                "rev-list",
                "--first-parent",
                &format!("--max-count={}", depth + 1),
                &tip,
                "--",
            ],
            vec![],
            &[0],
        )?;
        commits = parse_ids(&ids)?;
        notes.push("Selected revision is outside the loaded first-parent HEAD path. This timeline ends at that revision.".into());
    }
    let limited = commits.len() > depth;
    commits.truncate(depth);
    if limited {
        notes.push(format!("Loaded the newest {depth} first-parent commits; increase history depth to follow earlier versions."));
    }
    let mut at = commits
        .iter()
        .position(|id| id == &selected)
        .ok_or("Selected revision exceeds history budget")?;
    let mut lookup_path = path.to_owned();
    let mut matches = Vec::new();
    while at < commits.len() {
        let symbols = trace.file(&commits[at], &lookup_path)?;
        matches = symbols
            .iter()
            .filter(|s| match line {
                Some(n) if at == commits.iter().position(|id| id == &selected).unwrap_or(0) => {
                    s.first <= n && n <= s.last
                }
                _ => name.is_empty() || s.name == name || s.qualified == name,
            })
            .cloned()
            .collect();
        if !matches.is_empty() {
            break;
        }
        if line.is_some() && name.is_empty() {
            return Ok(Answer::Choices(symbols.as_ref().clone()));
        }
        if at + 1 >= commits.len() {
            break;
        }
        let diff = trace.diff(&commits[at + 1], &commits[at])?;
        if let Some((old, _)) = diff.edges.iter().find(|(_, new)| new == &lookup_path) {
            lookup_path = old.clone();
        }
        at += 1;
    }
    if matches.len() != 1 {
        if matches.is_empty() {
            return Err("No matching function was found within the local history budget".into());
        }
        return Ok(Answer::Choices(matches));
    }
    let seed = matches.remove(0);
    let seed_at = at;
    let mut steps = Vec::new();
    let mut current = seed.clone();
    let mut ended = false;
    let mut incomplete = limited;
    while at + 1 < commits.len() {
        let changes = trace.diff(&commits[at + 1], &commits[at])?;
        if !changes.new.contains(&current.path) && !changes.old.contains(&current.path) {
            at += 1;
            continue;
        }
        let candidates = trace.candidates(&commits[at + 1], &current, &changes, false)?;
        let chosen = match choose(&current, &candidates) {
            Ok(c) => c,
            Err(e) => {
                notes.push(e);
                incomplete = true;
                break;
            }
        };
        let Some((previous, confidence)) = chosen else {
            if changes.limited {
                notes.push("A bulk/unsupported file transition prevents proving creation; lineage stops here.".into());
                incomplete = true;
            } else {
                steps.push(trace.step(
                    &commits[at],
                    "Created".into(),
                    Some(current.clone()),
                    vec![],
                    None,
                )?);
                ended = true;
            }
            break;
        };
        if previous.body != current.body
            || previous.path != current.path
            || previous.qualified != current.qualified
        {
            let all = trace.candidates(&commits[at], &previous, &changes, true)?;
            let related = splits(&previous, &current, &all);
            let mut what = event(&previous, &current);
            if !related.is_empty() {
                what.push_str(" · Possible split");
            }
            steps.push(trace.step(
                &commits[at],
                what,
                Some(current.clone()),
                related,
                Some(confidence),
            )?);
        }
        current = previous;
        at += 1;
    }
    if !ended {
        let parents = trace.reader.run(
            &["show", "-s", "--format=%P", &commits[at], "--"],
            vec![],
            &[0],
        )?;
        let root = parents.iter().all(u8::is_ascii_whitespace);
        steps.push(trace.step(
            &commits[at],
            if root {
                "Created".into()
            } else {
                "Earlier boundary (creation unproven)".into()
            },
            Some(current),
            vec![],
            None,
        )?);
        if !root {
            incomplete = true;
        }
    }
    steps.reverse();
    current = seed;
    for next in (0..seed_at).rev() {
        let changes = trace.diff(&commits[next + 1], &commits[next])?;
        if !changes.new.contains(&current.path) && !changes.old.contains(&current.path) {
            continue;
        }
        let candidates = trace.candidates(&commits[next], &current, &changes, true)?;
        let chosen = match choose(&current, &candidates) {
            Ok(c) => c,
            Err(e) => {
                notes.push(e);
                incomplete = true;
                break;
            }
        };
        let Some((following, confidence)) = chosen else {
            if changes.limited {
                notes.push("A bulk/unsupported transition prevents proving deletion.".into());
                incomplete = true;
            } else {
                steps.push(trace.step(&commits[next], "Deleted".into(), None, vec![], None)?);
            }
            break;
        };
        if following.body != current.body
            || following.path != current.path
            || following.qualified != current.qualified
        {
            let related = splits(&current, &following, &candidates);
            let mut what = event(&current, &following);
            if !related.is_empty() {
                what.push_str(" · Possible split");
            }
            steps.push(trace.step(
                &commits[next],
                what,
                Some(following.clone()),
                related,
                Some(confidence),
            )?);
        }
        current = following;
    }
    if steps.is_empty() {
        steps.push(trace.step(
            &commits[seed_at],
            "Selected version".into(),
            Some(current),
            vec![],
            None,
        )?);
    }
    Ok(Answer::History(Biography {
        tip,
        steps,
        notes,
        limited: incomplete,
    }))
}
