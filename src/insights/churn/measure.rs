//! What the samples hold: each row's files at each sample (`git ls-tree`), their lines and
//! branch points (`git cat-file --batch`, each distinct blob once, within a budget), and the
//! test lane's verdicts on the samples' trees. Read-only, and bounded: what the budget leaves
//! out is said, and the sizes it touches are lower bounds.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::Plan;
use super::analyzer::{self, Lang};
use crate::git::{self, Verdict};
use crate::insights::folder_at;

/// Files bigger than this aren't read.
pub const MAX_BLOB: u64 = 1 << 20;
/// At most so many distinct files are read…
pub const MAX_BLOBS: usize = 5000;
/// …and so many bytes in all.
pub const MAX_BYTES: u64 = 48 << 20;

/// What one row holds at one sample.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// Files there, and of them read (within the budget), binary, and of a language the
    /// heuristic reads.
    pub files: u32,
    pub read: u32,
    pub binary: u32,
    pub measured: u32,
    /// Lines of the text files read.
    pub lines: u64,
    /// Branch points of the files measured.
    pub branches: u64,
}

/// What [`measure`] read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Measured {
    /// By row, then sample.
    pub readings: Vec<Vec<Reading>>,
    /// By sample: what the test lane recorded for its tree (`None`: nothing).
    pub tests: Vec<Option<Verdict>>,
    /// Why the test results couldn't be read, when they couldn't.
    pub tests_error: Option<String>,
    /// What the budget left out, said.
    pub cut: Vec<String>,
}

/// One blob read.
struct Blob {
    binary: bool,
    lines: u64,
    branches: HashMap<Lang, u64>,
}

/// Reads what `plan`'s rows hold at its samples, in `repo`.
pub fn measure(repo: &Path, plan: &Plan) -> Result<Measured, String> {
    let n = plan.samples.len();
    let mut found: Vec<Vec<Vec<(String, String)>>> = vec![vec![Vec::new(); n]; plan.rows.len()];
    for (k, sample) in plan.samples.iter().enumerate() {
        let mut rows_of: HashMap<&str, Vec<usize>> = HashMap::new();
        for (r, row) in plan.rows.iter().enumerate() {
            if let Some(path) = row.paths.get(k) {
                rows_of.entry(path.as_str()).or_default().push(r);
            }
        }
        let wanted: Vec<&str> = rows_of.keys().copied().collect();
        for (oid, path) in list(repo, &sample.hash, &wanted, plan.group.is_some())? {
            let key = match plan.group {
                Some(depth) => folder_at(&path, depth.max(1)),
                None => path.as_str(),
            };
            for &r in rows_of.get(key).into_iter().flatten() {
                found[r][k].push((oid.clone(), path.clone()));
            }
        }
    }

    // Each distinct blob once, and the languages it is read as.
    let mut order: Vec<&str> = Vec::new();
    let mut langs: HashMap<&str, HashSet<Lang>> = HashMap::new();
    for (oid, path) in found.iter().flatten().flatten() {
        let entry = langs.entry(oid.as_str()).or_insert_with(|| {
            order.push(oid.as_str());
            HashSet::new()
        });
        entry.extend(analyzer::language(path));
    }
    let sizes = sizes(repo, &order)?;
    let (mut chosen, mut too_big, mut over) = (Vec::new(), 0usize, 0usize);
    let mut bytes = 0u64;
    for oid in &order {
        let Some(&size) = sizes.get(*oid) else {
            continue;
        };
        if size > MAX_BLOB {
            too_big += 1;
        } else if chosen.len() >= MAX_BLOBS || bytes + size > MAX_BYTES {
            over += 1;
        } else {
            bytes += size;
            chosen.push(*oid);
        }
    }
    let blobs = read_blobs(repo, &chosen, &langs)?;

    let readings = found
        .iter()
        .map(|by_sample| {
            by_sample
                .iter()
                .map(|files| {
                    let mut reading = Reading {
                        files: files.len() as u32,
                        ..Reading::default()
                    };
                    for (oid, path) in files {
                        let Some(blob) = blobs.get(oid.as_str()) else {
                            continue;
                        };
                        reading.read += 1;
                        if blob.binary {
                            reading.binary += 1;
                            continue;
                        }
                        reading.lines += blob.lines;
                        if let Some(lang) = analyzer::language(path) {
                            reading.measured += 1;
                            reading.branches += blob.branches.get(&lang).copied().unwrap_or(0);
                        }
                    }
                    reading
                })
                .collect()
        })
        .collect();
    let mut cut = Vec::new();
    if too_big > 0 {
        cut.push(format!(
            "{too_big} files over {} KiB weren't read: sizes counting them are lower bounds",
            MAX_BLOB >> 10
        ));
    }
    if over > 0 {
        cut.push(format!(
            "the read budget ({MAX_BLOBS} files, {} MiB) was reached: {over} more files weren't \
             read, and sizes counting them are lower bounds",
            MAX_BYTES >> 20
        ));
    }
    let (tests, tests_error) = match verdicts(repo, plan) {
        Ok(tests) => (tests, None),
        Err(e) => (vec![None; n], Some(e)),
    };
    Ok(Measured {
        readings,
        tests,
        tests_error,
        cut,
    })
}

/// The blobs (id, path) at `rev` under `paths`: those files, or the files in those folders
/// (`folders`; `""` is the root's own files).
fn list(
    repo: &Path,
    rev: &str,
    paths: &[&str],
    folders: bool,
) -> Result<Vec<(String, String)>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let named: Vec<&str> = paths.iter().copied().filter(|p| !p.is_empty()).collect();
    if !named.is_empty() {
        let mut args = vec!["ls-tree", "-r", "-z", "--full-tree", rev, "--"];
        args.extend(named);
        out.extend(parse_tree(&git::run(Some(repo), &args)?));
    }
    if folders && paths.contains(&"") {
        let listing = git::run(Some(repo), &["ls-tree", "-z", "--full-tree", rev])?;
        out.extend(parse_tree(&listing));
    }
    Ok(out)
}

/// The blobs in what `ls-tree -z` printed: (id, path). Links and submodules are left out.
fn parse_tree(listing: &str) -> Vec<(String, String)> {
    listing
        .split('\0')
        .filter_map(|entry| {
            let (meta, path) = entry.trim_start_matches('\n').split_once('\t')?;
            let mut meta = meta.split(' ');
            let (mode, kind, oid) = (meta.next()?, meta.next()?, meta.next()?);
            (kind == "blob" && mode != "120000").then(|| (oid.to_owned(), path.to_owned()))
        })
        .collect()
}

/// The size in bytes of each of `oids`.
fn sizes(repo: &Path, oids: &[&str]) -> Result<HashMap<String, u64>, String> {
    if oids.is_empty() {
        return Ok(HashMap::new());
    }
    let input: String = oids.iter().map(|o| format!("{o}\n")).collect();
    let out = git::run_input(Some(repo), &["cat-file", "--batch-check"], &input)?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let mut words = line.split(' ');
            let (oid, kind, size) = (words.next()?, words.next()?, words.next()?);
            (kind == "blob").then(|| Some((oid.to_owned(), size.parse().ok()?)))?
        })
        .collect())
}

/// Reads `oids`: whether each is binary, its lines, and its branch points in each language
/// `langs` gives for it.
fn read_blobs<'a>(
    repo: &Path,
    oids: &[&'a str],
    langs: &HashMap<&str, HashSet<Lang>>,
) -> Result<HashMap<&'a str, Blob>, String> {
    if oids.is_empty() {
        return Ok(HashMap::new());
    }
    let input: String = oids.iter().map(|o| format!("{o}\n")).collect();
    let out = git::run_captured(repo, &["cat-file", "--batch"], Some(input.as_bytes()))?;
    if out.code != Some(0) {
        return Err(format!("couldn't read the files: {}", out.stderr.trim()));
    }
    let mut blobs = HashMap::new();
    let mut rest = out.raw.as_slice();
    for &oid in oids {
        let Some(end) = rest.iter().position(|&b| b == b'\n') else {
            break;
        };
        let header = String::from_utf8_lossy(&rest[..end]).into_owned();
        rest = &rest[end + 1..];
        let mut words = header.split(' ');
        let (_, kind, size) = (words.next(), words.next(), words.next());
        let Some(size) = size.and_then(|s| s.parse::<usize>().ok()) else {
            continue;
        };
        let Some(data) = rest.get(..size) else {
            break;
        };
        rest = rest.get(size..).unwrap_or_default();
        rest = rest.strip_prefix(b"\n").unwrap_or(rest);
        if kind != Some("blob") {
            continue;
        }
        let binary = analyzer::is_binary(data);
        let mut branches = HashMap::new();
        if !binary {
            let text = String::from_utf8_lossy(data);
            for &lang in langs.get(oid).into_iter().flatten() {
                branches.insert(lang, analyzer::branch_points(&text, lang));
            }
        }
        blobs.insert(
            oid,
            Blob {
                binary,
                lines: if binary { 0 } else { analyzer::lines(data) },
                branches,
            },
        );
    }
    Ok(blobs)
}

/// What the test lane recorded for each sample's tree.
fn verdicts(repo: &Path, plan: &Plan) -> Result<Vec<Option<Verdict>>, String> {
    if plan.samples.is_empty() {
        return Ok(Vec::new());
    }
    let revs: Vec<String> = plan
        .samples
        .iter()
        .map(|s| format!("{}^{{tree}}", s.hash))
        .collect();
    let mut args = vec!["rev-parse"];
    args.extend(revs.iter().map(String::as_str));
    let trees: Vec<String> = git::run(Some(repo), &args)?
        .lines()
        .map(|l| l.trim().to_ascii_lowercase())
        .collect();
    let results = git::read_results(repo, git::DEFAULT_TEST)?;
    Ok((0..plan.samples.len())
        .map(|k| trees.get(k).and_then(|t| results.get(t)).copied())
        .collect())
}
