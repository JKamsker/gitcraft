//! The code evolution heatmap's matrix (#175 item 4): the commits read (or the days, weeks or
//! months they fall in) across, the files or folders they changed down, and in each cell what
//! changed there. Built from a [`History`] alone, so it is the same however often it is drawn.
//!
//! A commit counts once in a cell however many of the cell's files it changed, and once in the
//! history however often it was listed; merges count nothing (the history gives them no files).

use std::collections::{HashMap, HashSet};

use super::folder_at;
use super::history::History;

/// At most so many columns of days, weeks or months, the newest kept.
pub const MAX_BUCKETS: usize = 4000;

/// What the columns are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Axis {
    /// One column per commit that changed something shown, oldest on the left.
    #[default]
    Commits,
    /// One per day (UTC), every day from the oldest commit to the newest.
    Day,
    /// One per week (UTC, from Monday).
    Week,
    /// One per month (UTC).
    Month,
}

impl Axis {
    pub const ALL: [Axis; 4] = [Axis::Commits, Axis::Day, Axis::Week, Axis::Month];

    pub fn label(self) -> &'static str {
        match self {
            Axis::Commits => "Commits",
            Axis::Day => "Days",
            Axis::Week => "Weeks",
            Axis::Month => "Months",
        }
    }
}

/// What a cell's colour says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Measure {
    /// Lines added and deleted together (a binary file counting as one).
    #[default]
    Lines,
    /// Whether more lines were added or deleted.
    Balance,
    /// In how many commits it changed.
    Frequency,
}

impl Measure {
    pub const ALL: [Measure; 3] = [Measure::Lines, Measure::Balance, Measure::Frequency];

    pub fn label(self) -> &'static str {
        match self {
            Measure::Lines => "Lines changed",
            Measure::Balance => "Added vs deleted",
            Measure::Frequency => "Change frequency",
        }
    }
}

/// How the matrix is cut from the history.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Options {
    pub axis: Axis,
    pub measure: Measure,
    /// Folders this many levels down as rows; `None`: files.
    pub group: Option<usize>,
    /// Only paths with this in them (any case); empty: all.
    pub filter: String,
    /// At most so many rows, those that changed most kept.
    pub max_rows: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            axis: Axis::Commits,
            measure: Measure::Lines,
            group: None,
            filter: String::new(),
            max_rows: 200,
        }
    }
}

/// One column: a commit, or the commits of a day, week or month.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// What its header says: a short id and subject, or a date.
    pub label: String,
    /// The commits it holds (indices into the history's commits), oldest first.
    pub commits: Vec<usize>,
    /// It is the oldest of a history cut short: commits of its time span may not be read.
    pub cut: bool,
}

/// One row: a file, or a folder of them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    /// Its path (`""`: the files at the root, as a folder).
    pub name: String,
    pub folder: bool,
    pub added: u64,
    pub deleted: u64,
    /// Lines changed, a binary file counting as one.
    pub lines: u64,
    /// The commits that changed it.
    pub commits: usize,
}

impl Row {
    /// Its name as shown: the root's files as `(root)`.
    pub fn shown(&self) -> &str {
        if self.name.is_empty() {
            "(root)"
        } else {
            &self.name
        }
    }
}

/// What changed in one cell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub added: u64,
    pub deleted: u64,
    /// Lines changed, a binary file counting as one.
    pub lines: u64,
    /// Changes of binary files, and of text files.
    pub binary: u32,
    pub text: u32,
    /// Changes that renamed a file.
    pub renamed: u32,
    /// The changes it counts: (commit, file) indices into the history.
    pub changes: Vec<(usize, usize)>,
}

impl Tally {
    /// The distinct commits it counts, oldest first.
    pub fn commits(&self) -> Vec<usize> {
        let mut seen = Vec::new();
        for &(c, _) in &self.changes {
            if !seen.contains(&c) {
                seen.push(c);
            }
        }
        seen
    }

    /// Whether more was added (`-1`: only additions) or deleted (`1`: only deletions).
    pub fn balance(&self) -> f32 {
        let total = self.added + self.deleted;
        if total == 0 {
            return 0.0;
        }
        (self.deleted as f32 - self.added as f32) / total as f32
    }

    /// Only binary files changed.
    pub fn binary_only(&self) -> bool {
        self.binary > 0 && self.text == 0
    }
}

/// The heatmap: columns oldest first, rows by how much they changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Matrix {
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    /// (row, column) → what changed there; cells that counted nothing aren't kept.
    pub cells: HashMap<(usize, usize), Tally>,
    /// Rows left out by the row limit.
    pub rows_cut: usize,
    /// Columns left out by [`MAX_BUCKETS`] (the oldest).
    pub columns_cut: usize,
    /// The history was cut short: older commits exist, left of the first column.
    pub boundary: bool,
    /// The most lines, and the most commits, in a cell.
    pub max_lines: u64,
    pub max_commits: usize,
}

/// What clicking a cell opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Open {
    /// The diff of one file in one commit.
    Diff {
        hash: String,
        path: String,
    },
    /// Several commits to choose from: each with the paths it changed there.
    Commits(Vec<(String, Vec<String>)>),
    Nothing,
}

impl Matrix {
    /// How far a cell is up its scale: `0..=1` (lines, frequency) or `-1..=1` (balance).
    pub fn value(&self, tally: &Tally, measure: Measure) -> f32 {
        match measure {
            // Log: one huge commit doesn't make every other cell look empty.
            Measure::Lines => {
                let max = (self.max_lines as f32).ln_1p();
                if max <= 0.0 {
                    0.0
                } else {
                    (tally.lines as f32).ln_1p() / max
                }
            }
            Measure::Balance => tally.balance(),
            Measure::Frequency => {
                if self.max_commits == 0 {
                    0.0
                } else {
                    tally.commits().len() as f32 / self.max_commits as f32
                }
            }
        }
    }

    /// What the cell at (`row`, `column`) opens: one file's diff when it is one commit of a
    /// file, the commits otherwise.
    pub fn open(&self, history: &History, row: usize, column: usize) -> Open {
        let (Some(r), Some(tally)) = (self.rows.get(row), self.cells.get(&(row, column))) else {
            return Open::Nothing;
        };
        let mut by_commit: Vec<(String, Vec<String>)> = Vec::new();
        for &(c, f) in &tally.changes {
            let (Some(commit), Some(file)) = (
                history.commits.get(c),
                history.commits.get(c).and_then(|c| c.files.get(f)),
            ) else {
                continue;
            };
            match by_commit.iter_mut().find(|(h, _)| *h == commit.hash) {
                Some((_, paths)) => paths.push(file.path.clone()),
                None => by_commit.push((commit.hash.clone(), vec![file.path.clone()])),
            }
        }
        if !r.folder && by_commit.len() == 1 {
            let (hash, mut paths) = by_commit.remove(0);
            return Open::Diff {
                hash,
                path: paths.remove(0),
            };
        }
        if by_commit.is_empty() {
            Open::Nothing
        } else {
            Open::Commits(by_commit)
        }
    }
}

/// The row a path falls in.
fn row_of(path: &str, group: Option<usize>) -> &str {
    match group {
        Some(depth) => folder_at(path, depth.max(1)),
        None => path,
    }
}

/// The matrix of `history` cut as `options` say.
pub fn build(history: &History, options: &Options) -> Matrix {
    let filter = options.filter.trim().to_lowercase();
    // Oldest first; a commit listed twice counts once.
    let mut seen = HashSet::new();
    let order: Vec<usize> = (0..history.commits.len())
        .rev()
        .filter(|&i| seen.insert(history.commits[i].hash.as_str()))
        .collect();
    let matches = |path: &str| filter.is_empty() || path.to_lowercase().contains(&filter);

    // The rows' totals over everything read, to keep those that changed most.
    let mut totals: HashMap<&str, (Row, HashSet<usize>)> = HashMap::new();
    for &c in &order {
        for file in &history.commits[c].files {
            if !matches(&file.path) {
                continue;
            }
            let name = row_of(&file.path, options.group);
            let (row, commits) = totals.entry(name).or_insert_with(|| {
                (
                    Row {
                        name: name.to_owned(),
                        folder: options.group.is_some(),
                        ..Row::default()
                    },
                    HashSet::new(),
                )
            });
            row.added += u64::from(file.added.unwrap_or(0));
            row.deleted += u64::from(file.deleted.unwrap_or(0));
            row.lines += u64::from(file.lines());
            commits.insert(c);
        }
    }
    let mut rows: Vec<Row> = totals
        .into_values()
        .map(|(mut row, commits)| {
            row.commits = commits.len();
            row
        })
        .collect();
    let key = |r: &Row| match options.measure {
        Measure::Frequency => (r.commits as u64, r.lines),
        Measure::Lines | Measure::Balance => (r.lines, r.commits as u64),
    };
    rows.sort_by(|a, b| key(b).cmp(&key(a)).then_with(|| a.name.cmp(&b.name)));
    let rows_cut = rows.len().saturating_sub(options.max_rows);
    rows.truncate(options.max_rows);
    let row_index: HashMap<&str, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.name.as_str(), i))
        .collect();

    // The commits that changed a row shown, oldest first.
    let shown: Vec<usize> = order
        .iter()
        .copied()
        .filter(|&c| {
            history.commits[c]
                .files
                .iter()
                .any(|f| matches(&f.path) && row_index.contains_key(row_of(&f.path, options.group)))
        })
        .collect();
    let (columns, column_of, columns_cut) = columns(history, &shown, options.axis);

    let mut cells: HashMap<(usize, usize), Tally> = HashMap::new();
    for &c in &shown {
        let Some(&column) = column_of.get(&c) else {
            continue;
        };
        for (f, file) in history.commits[c].files.iter().enumerate() {
            if !matches(&file.path) {
                continue;
            }
            let Some(&row) = row_index.get(row_of(&file.path, options.group)) else {
                continue;
            };
            let tally = cells.entry((row, column)).or_default();
            tally.added += u64::from(file.added.unwrap_or(0));
            tally.deleted += u64::from(file.deleted.unwrap_or(0));
            tally.lines += u64::from(file.lines());
            if file.added.is_none() {
                tally.binary += 1;
            } else {
                tally.text += 1;
            }
            if file.old_path.is_some() {
                tally.renamed += 1;
            }
            tally.changes.push((c, f));
        }
    }
    let max_lines = cells.values().map(|t| t.lines).max().unwrap_or(0);
    let max_commits = cells.values().map(|t| t.commits().len()).max().unwrap_or(0);
    Matrix {
        columns,
        rows,
        cells,
        rows_cut,
        columns_cut,
        boundary: history.truncated,
        max_lines,
        max_commits,
    }
}

/// The columns of `commits` (oldest first) on `axis`, which column each commit is in, and how
/// many of the oldest columns were left out.
fn columns(
    history: &History,
    commits: &[usize],
    axis: Axis,
) -> (Vec<Column>, HashMap<usize, usize>, usize) {
    let mut column_of = HashMap::new();
    if axis == Axis::Commits {
        let columns = commits
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                column_of.insert(c, i);
                let commit = &history.commits[c];
                let subject: String = commit.subject.chars().take(24).collect();
                Column {
                    label: format!("{} {subject}", short(&commit.hash)),
                    commits: vec![c],
                    cut: false,
                }
            })
            .collect();
        return (columns, column_of, 0);
    }
    let (Some(first), Some(last)) = (
        commits
            .iter()
            .map(|&c| bucket(history.commits[c].time, axis))
            .min(),
        commits
            .iter()
            .map(|&c| bucket(history.commits[c].time, axis))
            .max(),
    ) else {
        return (Vec::new(), column_of, 0);
    };
    let span = (last - first + 1) as usize;
    let cut = span.saturating_sub(MAX_BUCKETS);
    let first = first + cut as i64;
    let mut columns: Vec<Column> = (first..=last)
        .map(|b| Column {
            label: bucket_label(b, axis),
            commits: Vec::new(),
            cut: false,
        })
        .collect();
    for &c in commits {
        let b = bucket(history.commits[c].time, axis);
        if b < first {
            continue;
        }
        let i = (b - first) as usize;
        column_of.insert(c, i);
        columns[i].commits.push(c);
    }
    // The history begins inside the oldest bucket: what came before in it isn't read.
    if history.truncated && cut == 0 {
        let oldest_read = history.commits.iter().map(|c| c.time).min();
        if let (Some(t), Some(col)) = (oldest_read, columns.first_mut())
            && bucket(t, axis) == first
        {
            col.cut = true;
            col.label = format!("{} (cut)", col.label);
        }
    }
    (columns, column_of, cut)
}

fn short(hash: &str) -> &str {
    hash.get(..7).unwrap_or(hash)
}

/// The bucket of time `t` on `axis`: days, weeks (from Monday) or months since the epoch, UTC.
pub fn bucket(t: i64, axis: Axis) -> i64 {
    let day = t.div_euclid(86_400);
    match axis {
        Axis::Commits | Axis::Day => day,
        // 1970-01-01 was a Thursday: day -3 a Monday.
        Axis::Week => (day + 3).div_euclid(7),
        Axis::Month => {
            let (y, m, _) = civil(day);
            y * 12 + i64::from(m) - 1
        }
    }
}

/// What a bucket's header says: its date, a week by its Monday, a month by its year and month.
pub fn bucket_label(b: i64, axis: Axis) -> String {
    match axis {
        Axis::Commits | Axis::Day => {
            let (y, m, d) = civil(b);
            format!("{y}-{m:02}-{d:02}")
        }
        Axis::Week => {
            let (y, m, d) = civil(b * 7 - 3);
            format!("wk {y}-{m:02}-{d:02}")
        }
        Axis::Month => format!("{}-{:02}", b.div_euclid(12), b.rem_euclid(12) + 1),
    }
}

/// The calendar date of a day since the epoch (Howard Hinnant's `civil_from_days`).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests;
