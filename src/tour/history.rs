//! Read bounded commit history and represent observed dates.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Day(i64);

impl Day {
    pub(super) fn from_unix_days(n: i64) -> Option<Self> {
        Some(Self(n))
    }
}

impl std::fmt::Display for Day {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Gregorian civil date from a Unix day (Howard Hinnant's era decomposition).
        let z = self.0 + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = mp + if mp < 10 { 3 } else { -9 };
        let y = y + i64::from(m <= 2);
        write!(f, "{y:04}-{m:02}-{d:02} UTC")
    }
}

pub(crate) struct History {
    pub commits: Vec<ObservedCommit>,
    pub more: bool,
    pub shallow: bool,
    pub bytes_limited: bool,
}

pub(crate) struct ObservedCommit {
    pub(super) hash: String,
    pub(super) name: String,
    pub(super) email: String,
    pub(super) committer: Day,
    pub(super) paths: Vec<Vec<u8>>,
}

pub(super) fn read_history(
    repo: &Path,
    active: &dyn Fn() -> bool,
    progress: &AtomicUsize,
) -> Result<History, String> {
    let bytes = run(
        repo,
        &[
            "--no-lazy-fetch",
            "log",
            "--all",
            "HEAD",
            "--max-count=5001",
            "-z",
            "--format=%H%x00%an%x00%ae%x00%ct",
            "--no-show-signature",
        ],
        None,
        active,
    )?;
    let fields: Vec<_> = bytes.split(|b| *b == 0).collect();
    let mut commits = Vec::new();
    for row in fields.chunks(4) {
        if row.len() < 4 {
            continue;
        }
        let hash = String::from_utf8_lossy(row[0]).trim().to_owned();
        if !matches!(hash.len(), 40 | 64) || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid tour history record".into());
        }
        let timestamp = String::from_utf8_lossy(row[3])
            .parse::<i64>()
            .map_err(|_| "Invalid commit date")?;
        commits.push(ObservedCommit {
            hash,
            name: String::from_utf8_lossy(row[1]).into_owned(),
            email: String::from_utf8_lossy(row[2]).into_owned(),
            committer: Day(timestamp.div_euclid(86400)),
            paths: Vec::new(),
        });
    }
    let more = commits.len() > 5000;
    commits.truncate(5000);
    let ids: BTreeMap<_, _> = commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.clone(), i))
        .collect();
    // A NUL record with an empty predecessor denotes a commit hash; paths cannot be empty.
    let names = run(
        repo,
        &[
            "--no-lazy-fetch",
            "log",
            "--all",
            "HEAD",
            "--max-count=5000",
            "--since=90 days ago",
            "--format=%x00%H",
            "--name-only",
            "--no-renames",
            "-z",
            "--no-show-signature",
        ],
        None,
        active,
    )?;
    let mut next_hash = false;
    let mut current = None;
    for field in names.split(|b| *b == 0) {
        if field.is_empty() {
            next_hash = true;
            continue;
        }
        if next_hash {
            current = std::str::from_utf8(field)
                .ok()
                .and_then(|s| ids.get(s).copied());
            next_hash = false;
        } else if let Some(i) = current {
            let path = field.strip_prefix(b"\n").unwrap_or(field);
            if !path.is_empty() {
                commits[i].paths.push(path.to_vec());
            }
        }
    }
    progress.store(commits.len(), std::sync::atomic::Ordering::Relaxed);
    let shallow = run(
        repo,
        &["rev-parse", "--is-shallow-repository"],
        None,
        active,
    )?
    .starts_with(b"true");
    Ok(History {
        commits,
        more,
        shallow,
        bytes_limited: false,
    })
}
