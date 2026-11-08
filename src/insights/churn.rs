//! The churn and complexity timeline (#175 item 12): sample commits spread evenly over the
//! history read, and per file (or folder) at each its size, the churn of the window of commits
//! up to it, its complexity where the heuristic reads its language, and what the test lane
//! recorded for that commit.
//!
//! [`plan`] works out the samples, windows and rows from a [`History`] alone; [`measure`] reads
//! the sizes, complexity and test results at the samples from the repository (bounded, and
//! saying what it left out); [`assemble`] puts the two together and marks the hotspots.
//!
//! Size and churn are factual (lines Git counts); complexity is analyzer-defined (see
//! [`analyzer`]); none of it says anything about who wrote what.

pub mod analyzer;
pub mod measure;

use std::collections::{HashMap, HashSet};

pub use measure::{MAX_BLOB, MAX_BLOBS, MAX_BYTES, Measured, measure};

use super::folder_at;
use super::history::History;
use crate::git::Verdict;

/// How many samples are offered, and the default.
pub const SAMPLES: [usize; 3] = [12, 16, 24];
pub const DEFAULT_SAMPLES: usize = 16;

/// The rule a hotspot meets, as the view states it.
pub const HOTSPOT_RULE: &str = "Hotspot: over the last 4 samples its complexity never fell and \
    ended higher, and its last 2 windows changed more lines than the 2 before them";

/// How the timeline is cut from the history.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Options {
    /// At most so many samples.
    pub samples: usize,
    /// Folders this many levels down as rows; `None`: files.
    pub group: Option<usize>,
    /// Only paths with this in them (any case); empty: all.
    pub filter: String,
    /// At most so many rows, those with the most churn kept.
    pub max_rows: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            samples: DEFAULT_SAMPLES,
            group: None,
            filter: String::new(),
            max_rows: 24,
        }
    }
}

/// A sample: a commit at which sizes and complexity are read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    /// Its index in the history's commits.
    pub commit: usize,
    pub hash: String,
    pub time: i64,
    pub subject: String,
}

/// A row as planned: what it reads at each sample, and its churn in each window.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlanRow {
    /// A file's newest name, or a folder (`""`: the root's files).
    pub name: String,
    pub folder: bool,
    /// What is read at each sample: the file's name at that revision (it follows renames),
    /// or the folder.
    pub paths: Vec<String>,
    /// Lines changed in each window.
    pub churn: Vec<u64>,
    /// The changes of each window: (commit index, the path it changed).
    pub changes: Vec<Vec<(usize, String)>>,
    pub total: u64,
}

impl PlanRow {
    #[cfg(test)]
    pub fn shown(&self) -> &str {
        if self.name.is_empty() {
            "(root)"
        } else {
            &self.name
        }
    }
}

/// The samples, windows and rows of a history.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Oldest first; the last is the tip read.
    pub samples: Vec<Sample>,
    /// The commits of each sample's window, oldest first: those after the sample before it,
    /// up to and including it (indices into the history's commits).
    pub windows: Vec<Vec<usize>>,
    pub rows: Vec<PlanRow>,
    /// Rows left out by the row limit.
    pub rows_cut: usize,
    pub group: Option<usize>,
}

/// Where each of `n` samples falls among `m` commits (oldest first): the last of `n` windows
/// as even as can be, so the last sample is the newest commit.
pub fn sample_points(m: usize, n: usize) -> Vec<usize> {
    let n = n.min(m);
    (0..n).map(|k| ((k + 1) * m).div_ceil(n) - 1).collect()
}

/// One file followed back through its renames.
struct Lineage {
    newest: String,
    /// Its name before the commits seen so far (walking back).
    now: String,
    names: Vec<Option<String>>,
    churn: Vec<u64>,
    changes: Vec<Vec<(usize, String)>>,
}

/// The plan of `history` cut as `options` say.
pub fn plan(history: &History, options: &Options) -> Plan {
    let mut seen = HashSet::new();
    let order: Vec<usize> = (0..history.commits.len())
        .rev()
        .filter(|&i| seen.insert(history.commits[i].hash.as_str()))
        .collect();
    let points = sample_points(order.len(), options.samples.max(1));
    let n = points.len();
    if n == 0 {
        return Plan {
            group: options.group,
            ..Plan::default()
        };
    }
    let mut window_of = vec![0; order.len()];
    let mut windows = vec![Vec::new(); n];
    let mut start = 0;
    for (k, &end) in points.iter().enumerate() {
        for (p, w) in window_of.iter_mut().enumerate().take(end + 1).skip(start) {
            *w = k;
            windows[k].push(order[p]);
        }
        start = end + 1;
    }
    let samples = points
        .iter()
        .map(|&p| {
            let c = &history.commits[order[p]];
            Sample {
                commit: order[p],
                hash: c.hash.clone(),
                time: c.time,
                subject: c.subject.clone(),
            }
        })
        .collect();

    // Back from the newest commit, each file's lineage under the name it had then.
    let mut lineages: Vec<Lineage> = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut next_sample = n;
    for p in (0..order.len()).rev() {
        // Every commit newer than sample k undone: the names are those at sample k.
        while next_sample > 0 && points[next_sample - 1] >= p {
            next_sample -= 1;
            if points[next_sample] == p {
                for l in &mut lineages {
                    l.names[next_sample] = Some(l.now.clone());
                }
            }
        }
        let c = order[p];
        let window = window_of[p];
        for file in &history.commits[c].files {
            let id = match by_name.get(&file.path) {
                Some(&id) => id,
                None => {
                    lineages.push(Lineage {
                        newest: file.path.clone(),
                        now: file.path.clone(),
                        names: vec![None; n],
                        churn: vec![0; n],
                        changes: vec![Vec::new(); n],
                    });
                    by_name.insert(file.path.clone(), lineages.len() - 1);
                    lineages.len() - 1
                }
            };
            let l = &mut lineages[id];
            l.churn[window] += u64::from(file.lines());
            l.changes[window].push((c, file.path.clone()));
            if let Some(old) = &file.old_path {
                by_name.remove(&file.path);
                by_name.insert(old.clone(), id);
                l.now = old.clone();
            }
        }
    }

    let filter = options.filter.trim().to_lowercase();
    let mut rows: Vec<PlanRow> = Vec::new();
    let mut row_of: HashMap<String, usize> = HashMap::new();
    for l in lineages {
        if !filter.is_empty() && !l.newest.to_lowercase().contains(&filter) {
            continue;
        }
        let (name, folder) = match options.group {
            Some(depth) => (folder_at(&l.newest, depth.max(1)).to_owned(), true),
            None => (l.newest.clone(), false),
        };
        let id = *row_of.entry(name.clone()).or_insert_with(|| {
            rows.push(PlanRow {
                paths: vec![name.clone(); n],
                name,
                folder,
                churn: vec![0; n],
                changes: vec![Vec::new(); n],
                total: 0,
            });
            rows.len() - 1
        });
        let row = &mut rows[id];
        if !folder {
            // Not renamed since: the name it has now.
            row.paths = l
                .names
                .iter()
                .map(|name| name.clone().unwrap_or_else(|| l.newest.clone()))
                .collect();
        }
        for k in 0..n {
            row.churn[k] += l.churn[k];
            row.changes[k].extend(l.changes[k].iter().cloned());
        }
    }
    for row in &mut rows {
        row.total = row.churn.iter().sum();
        for changes in &mut row.changes {
            // Oldest first, as the windows list their commits.
            changes.reverse();
        }
    }
    rows.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.name.cmp(&b.name)));
    let rows_cut = rows.len().saturating_sub(options.max_rows);
    rows.truncate(options.max_rows);
    Plan {
        samples,
        windows,
        rows,
        rows_cut,
        group: options.group,
    }
}

/// One row at one sample.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Point {
    /// Lines at the sample; `None`: not there then (or nothing of it could be read).
    pub size: Option<u64>,
    /// Lines changed in the window up to the sample.
    pub churn: u64,
    /// Branch points at the sample; `None`: not measured (no file of a language the
    /// heuristic reads).
    pub complexity: Option<u64>,
    /// Everything there was read: not a lower bound.
    pub complete: bool,
    /// Files there, read, binary, and of a language measured.
    pub files: u32,
    pub read: u32,
    pub binary: u32,
    pub measured: u32,
}

/// One row over the samples.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Series {
    pub name: String,
    pub folder: bool,
    /// What was read at each sample: the file's name then, or the folder.
    pub paths: Vec<String>,
    pub points: Vec<Point>,
    /// The changes of each window: (commit index, path).
    pub changes: Vec<Vec<(usize, String)>>,
    pub total: u64,
    pub hotspot: bool,
}

impl Series {
    pub fn shown(&self) -> &str {
        if self.name.is_empty() {
            "(root)"
        } else {
            &self.name
        }
    }

    /// The distinct commits of window `k` that changed it, oldest first, with the paths each
    /// changed.
    pub fn commits_in(&self, k: usize) -> Vec<(usize, Vec<String>)> {
        let mut out: Vec<(usize, Vec<String>)> = Vec::new();
        for (c, path) in self.changes.get(k).into_iter().flatten() {
            match out.iter_mut().find(|(seen, _)| seen == c) {
                Some((_, paths)) => paths.push(path.clone()),
                None => out.push((*c, vec![path.clone()])),
            }
        }
        out
    }
}

/// A sample with what the test lane recorded for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tested {
    pub sample: Sample,
    /// `None`: no result recorded (unknown, which isn't failing).
    pub verdict: Option<Verdict>,
}

/// The timeline: samples oldest first, rows by churn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Timeline {
    pub samples: Vec<Tested>,
    pub windows: Vec<Vec<usize>>,
    pub rows: Vec<Series>,
    pub rows_cut: usize,
    /// What was left out, said (rows, files too big, the read budget).
    pub notes: Vec<String>,
    /// Why the test results couldn't be read, when they couldn't.
    pub tests_error: Option<String>,
}

/// The plan and what was read at its samples, together; hotspots marked.
pub fn assemble(plan: Plan, measured: Measured) -> Timeline {
    let mut rows = Vec::with_capacity(plan.rows.len());
    for (r, row) in plan.rows.into_iter().enumerate() {
        let points: Vec<Point> = row
            .churn
            .iter()
            .enumerate()
            .map(|(k, &churn)| {
                let reading = measured
                    .readings
                    .get(r)
                    .and_then(|rs| rs.get(k))
                    .cloned()
                    .unwrap_or_default();
                Point {
                    size: (reading.read > reading.binary).then_some(reading.lines),
                    churn,
                    complexity: (reading.measured > 0).then_some(reading.branches),
                    complete: reading.read == reading.files,
                    files: reading.files,
                    read: reading.read,
                    binary: reading.binary,
                    measured: reading.measured,
                }
            })
            .collect();
        rows.push(Series {
            hotspot: hotspot(&points),
            name: row.name,
            folder: row.folder,
            paths: row.paths,
            points,
            changes: row.changes,
            total: row.total,
        });
    }
    let samples = plan
        .samples
        .into_iter()
        .enumerate()
        .map(|(k, sample)| Tested {
            sample,
            verdict: measured.tests.get(k).copied().flatten(),
        })
        .collect();
    let mut notes = Vec::new();
    if plan.rows_cut > 0 {
        notes.push(format!(
            "{} more rows changed less and aren't shown (row limit)",
            plan.rows_cut
        ));
    }
    notes.extend(measured.cut);
    Timeline {
        samples,
        windows: plan.windows,
        rows,
        rows_cut: plan.rows_cut,
        notes,
        tests_error: measured.tests_error,
    }
}

/// Whether `points` meet [`HOTSPOT_RULE`]: complexity measured in full at the last 4 samples,
/// never falling and ending higher, and the churn of the last 2 windows above that of the 2
/// before.
pub fn hotspot(points: &[Point]) -> bool {
    let Some(last) = points.len().checked_sub(4).map(|i| &points[i..]) else {
        return false;
    };
    let Some(cx) = last
        .iter()
        .map(|p| p.complexity.filter(|_| p.complete))
        .collect::<Option<Vec<u64>>>()
    else {
        return false;
    };
    let rising = cx.windows(2).all(|w| w[1] >= w[0]) && cx[3] > cx[0];
    let churn = |a: usize, b: usize| last[a].churn + last[b].churn;
    rising && churn(2, 3) > churn(0, 1)
}

#[cfg(test)]
mod tests;
