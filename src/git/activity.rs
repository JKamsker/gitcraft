//! Bounded history snapshot shared by calendar and contributor views. Date filtering happens
//! after walking commits: Git's --since would prune out-of-order author dates.
mod date;
mod mailmap;
mod process;
#[cfg(test)]
mod tests;

pub(crate) use date::Day;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

const PAGE: usize = 250;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const FORMAT: &str = "--format=%x00%H%x00%an%x00%ae%x00%aI%x00%cI%x00%s";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Clock {
    #[default]
    Author,
    Committer,
}

#[derive(Clone, Debug)]
pub(crate) struct Record {
    pub hash: String,
    pub name: String,
    pub email: String,
    pub author: Day,
    pub committer: Day,
    pub subject: String,
    pub paths: BTreeSet<Vec<u8>>,
}

impl Record {
    pub fn day(&self, clock: Clock) -> Day {
        match clock {
            Clock::Author => self.author,
            Clock::Committer => self.committer,
        }
    }
}

#[derive(Default)]
pub(crate) struct History {
    pub commits: Vec<Record>,
    pub more: bool,
    pub shallow: bool,
    pub tips: usize,
    pub limit: usize,
    pub bytes_limited: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Query {
    pub clock: Clock,
    pub from: Option<Day>,
    pub through: Option<Day>,
}

#[derive(Debug)]
pub(crate) struct Contributor {
    pub name: String,
    pub email: String,
    pub commits: usize,
    pub files: usize,
    pub days: usize,
    pub first: Day,
    pub last: Day,
}

#[derive(Default)]
pub(crate) struct Summary {
    pub calendar: BTreeMap<Day, usize>,
    pub contributors: Vec<Contributor>,
    /// Indices into the immutable History, sorted by chosen date, then hash.
    pub selected: Vec<usize>,
}

#[derive(Default)]
struct AuthorTotals {
    commits: usize,
    paths: BTreeSet<Vec<u8>>,
    days: BTreeSet<Day>,
}

pub(crate) fn summarize(
    history: &History,
    query: Query,
    active: &dyn Fn() -> bool,
) -> Result<Summary, String> {
    let mut summary = Summary::default();
    let mut authors: BTreeMap<(String, String), AuthorTotals> = BTreeMap::new();
    for (i, c) in history.commits.iter().enumerate() {
        if !active() {
            return Err("Analysis cancelled".into());
        }
        let day = c.day(query.clock);
        *summary.calendar.entry(day).or_default() += 1;
        if query.from.is_some_and(|d| day < d) || query.through.is_some_and(|d| day > d) {
            continue;
        }
        summary.selected.push(i);
        let entry = authors
            .entry((c.name.clone(), c.email.clone()))
            .or_default();
        entry.commits += 1;
        entry.paths.extend(c.paths.iter().cloned());
        entry.days.insert(day);
    }
    summary.selected.sort_by(|a, b| {
        history.commits[*b]
            .day(query.clock)
            .cmp(&history.commits[*a].day(query.clock))
            .then_with(|| history.commits[*a].hash.cmp(&history.commits[*b].hash))
    });
    for (
        (name, email),
        AuthorTotals {
            commits,
            paths: files,
            days,
        },
    ) in authors
    {
        if let (Some(first), Some(last)) = (days.first(), days.last()) {
            summary.contributors.push(Contributor {
                name,
                email,
                commits,
                files: files.len(),
                days: days.len(),
                first: *first,
                last: *last,
            });
        }
    }
    summary.contributors.sort_by(|a, b| {
        b.commits
            .cmp(&a.commits)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.email.cmp(&b.email))
    });
    Ok(summary)
}

pub(crate) fn read(
    repo: &Path,
    limit: usize,
    active: &dyn Fn() -> bool,
    progress: &AtomicUsize,
) -> Result<History, String> {
    read_observing(repo, limit, active, progress, |_| {})
}

/// The observer lets tests change real metadata at an exact page boundary without timing
/// races or global environment mutation. Production supplies an empty callback.
fn read_observing(
    repo: &Path,
    limit: usize,
    active: &dyn Fn() -> bool,
    progress: &AtomicUsize,
    mut page_read: impl FnMut(usize),
) -> Result<History, String> {
    let run = |args: &[&str], input: Option<&[u8]>| process::run(repo, args, input, active);
    let roots = run(
        &[
            "rev-parse",
            "--revs-only",
            "--branches",
            "--remotes",
            "--tags",
            "HEAD",
        ],
        None,
    )?;
    let mut tips = BTreeSet::new();
    for line in roots.split(|b| *b == b'\n').filter(|s| !s.is_empty()) {
        if !is_hash(line) {
            return Err("Git returned an invalid history tip".into());
        }
        tips.insert(line.to_vec());
    }
    if tips.len() > 10_000 {
        return Err(
            "More than 10,000 reference tips; narrow this repository before analyzing it".into(),
        );
    }
    let shallow = run(&["rev-parse", "--is-shallow-repository"], None)? == b"true\n";
    let mut history = History {
        shallow,
        tips: tips.len(),
        limit,
        ..Default::default()
    };
    if tips.is_empty() {
        return Ok(history);
    }
    let input: Vec<u8> = tips
        .into_iter()
        .flat_map(|mut tip| {
            tip.push(b'\n');
            tip
        })
        .collect();
    let mut seen = HashSet::new();
    let mut skip = 0;
    let mut bytes = 0;
    loop {
        if !active() {
            return Err("Analysis cancelled".into());
        }
        let take = PAGE.min(limit.saturating_sub(history.commits.len()) + 1);
        let data = run(
            &[
                "--no-lazy-fetch",
                "log",
                "--stdin",
                "--no-show-signature",
                "--encoding=UTF-8",
                "--no-use-mailmap",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "--no-relative",
                "--no-color",
                "--ignore-submodules=none",
                "--diff-merges=first-parent",
                "--root",
                "--raw",
                "--no-abbrev",
                "-z",
                FORMAT,
                &format!("--skip={skip}"),
                &format!("--max-count={take}"),
            ],
            Some(&input),
        )?;
        bytes += data.len();
        if bytes > MAX_BYTES {
            history.more = true;
            history.bytes_limited = true;
            break;
        }
        let page = parse(&data)?;
        let count = page.len();
        for record in page {
            if history.commits.len() == limit {
                history.more = true;
                break;
            }
            if seen.insert(record.hash.clone()) {
                history.commits.push(record);
            }
        }
        progress.store(history.commits.len(), Ordering::Relaxed);
        page_read(history.commits.len());
        if history.more || count < take {
            break;
        }
        skip += count;
    }
    mailmap::resolve(repo, &mut history.commits, active)?;
    Ok(history)
}

fn is_hash(s: &[u8]) -> bool {
    matches!(s.len(), 40 | 64) && s.iter().all(u8::is_ascii_hexdigit)
}

fn parse(data: &[u8]) -> Result<Vec<Record>, String> {
    let mut fields = data.split(|b| *b == 0).peekable();
    let mut records = Vec::new();
    while let Some(field) = fields.next() {
        if field.is_empty() || field == b"\n" {
            continue;
        }
        if !is_hash(field) {
            return Err("Unexpected Git history record".into());
        }
        let mut next = || {
            fields
                .next()
                .ok_or_else(|| "Truncated Git history record".to_owned())
        };
        let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        let name = text(next()?);
        let email = text(next()?);
        let author = Day::timestamp(next()?)?;
        let committer = Day::timestamp(next()?)?;
        let subject = text(next()?);
        let mut paths = BTreeSet::new();
        while let Some(raw) = fields.peek() {
            if raw.is_empty() {
                fields.next();
                break;
            }
            let raw = fields.next().ok_or("Missing path record")?;
            let raw = raw.strip_prefix(b"\n").unwrap_or(raw);
            if raw.is_empty() {
                continue;
            }
            let metadata: Vec<_> = raw.split(|b| *b == b' ').collect();
            if metadata.len() != 5
                || !metadata[0].starts_with(b":")
                || !is_hash(metadata[2])
                || !is_hash(metadata[3])
                || metadata[4].len() != 1
                || !b"ACDMTUXB".contains(&metadata[4][0])
            {
                return Err("Invalid changed-path metadata".into());
            }
            let path = fields.next().ok_or("Missing changed path")?;
            if path.is_empty() {
                return Err("Empty changed path".into());
            }
            paths.insert(path.to_vec());
        }
        records.push(Record {
            hash: text(field),
            name,
            email,
            author,
            committer,
            subject,
            paths,
        });
    }
    Ok(records)
}
