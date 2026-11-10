//! Repository reads and bounded impact assembly.
use super::*;

struct Read {
    items: Vec<Item>,
    masked: Vec<String>,
}

impl Read {
    fn new(text: &str) -> Self {
        Self {
            items: scan::scan(text),
            masked: scan::mask(text).lines().map(str::to_owned).collect(),
        }
    }
}

/// Reads files at revisions in one go, those not there (or too big) left out.
fn read_files(
    repo: &Path,
    wanted: &[(String, String)],
) -> Result<HashMap<(String, String), Read>, String> {
    let texts = git::blobs::read(repo, wanted, MAX_TEXT)?;
    Ok(wanted
        .iter()
        .cloned()
        .zip(texts)
        .filter_map(|(key, text)| Some((key, Read::new(&text?))))
        .collect())
}

/// What the change `range` touched in its Rust files and what refers to it, and its other
/// files as unknown.
pub fn analyze(repo: &Path, range: &Range) -> Result<Impact, String> {
    let (base, head) = (range.base.as_str(), range.head.as_str());
    let changed = git::diff_files(repo, base, Some(head))?;
    let in_diff: HashSet<String> = changed
        .iter()
        .flat_map(|f| std::iter::once(f.path.clone()).chain(f.old_path.clone()))
        .collect();
    let mut partial = Vec::new();
    let mut other = Vec::new();
    for file in changed.iter().filter(|f| !f.path.ends_with(".rs")) {
        if other.len() == MAX_OTHER {
            partial.push(format!(
                "only the first {MAX_OTHER} files in other languages are listed"
            ));
            break;
        }
        let name = file.path.rsplit('/').next().unwrap_or(&file.path);
        let why = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => format!("no analyzer for .{ext} files"),
            _ => "no analyzer for this file".to_owned(),
        };
        other.push(OtherFile {
            path: file.path.clone(),
            status: file.status,
            why,
        });
    }

    let mut files = git::changed_lines::read(repo, base, Some(head), &["*.rs"])?;
    files.retain(|f| !f.binary && (f.path.ends_with(".rs") || f.deleted));
    if files.len() > MAX_CHANGED_FILES {
        partial.push(format!(
            "only {MAX_CHANGED_FILES} of {} changed Rust files are read",
            files.len()
        ));
        files.truncate(MAX_CHANGED_FILES);
    }
    let mut wanted = Vec::new();
    for f in &files {
        if !f.deleted {
            wanted.push((head.to_owned(), f.path.clone()));
        }
        if !f.added {
            let old = f.old_path.clone().unwrap_or_else(|| f.path.clone());
            wanted.push((base.to_owned(), old));
        }
    }
    let mut read = read_files(repo, &wanted)?;
    // Those left unread (too big) have changes in no item known: said, not hidden.
    let unread = wanted.iter().filter(|w| !read.contains_key(*w)).count();
    if unread > 0 {
        partial.push(format!(
            "{unread} versions of changed Rust files weren't read (bigger than {} MB): their \
             changes aren't placed in items",
            MAX_TEXT >> 20
        ));
    }

    let mut symbols = Vec::new();
    let mut rust_files = Vec::new();
    let none = Read {
        items: Vec::new(),
        masked: Vec::new(),
    };
    for f in &files {
        let old = f.old_path.clone().unwrap_or_else(|| f.path.clone());
        let head_read = read
            .get(&(head.to_owned(), f.path.clone()))
            .unwrap_or(&none);
        let base_read = read.get(&(base.to_owned(), old.clone())).unwrap_or(&none);
        let (touched, outside) = touched(&f.hunks, &head_read.items, &base_read.items);
        rust_files.push((f.path.clone(), outside));
        for t in touched {
            let span = |rev: &str, path: &str, item: &Item| Span {
                rev: rev.to_owned(),
                path: path.to_owned(),
                line: item.line,
                start: item.start,
                end: item.end,
            };
            let h = t.head.map(|i| &head_read.items[i]);
            let b = t.base.map(|i| &base_read.items[i]);
            let Some(item) = h.or(b) else {
                continue;
            };
            symbols.push(Symbol {
                kind: item.kind,
                name: item.name.clone(),
                path: item.path.clone(),
                change: t.change(),
                public: item.public,
                test: item.test || test_path(&f.path),
                head: h.map(|i| span(head, &f.path, i)),
                base: b.map(|i| span(base, &old, i)),
                lines: t.lines,
                refs_cut: false,
                same_named: 0,
            });
        }
    }
    if symbols.len() > MAX_SYMBOLS {
        partial.push(format!(
            "only {MAX_SYMBOLS} of {} changed symbols are followed",
            symbols.len()
        ));
        symbols.truncate(MAX_SYMBOLS);
    }

    // The searches: at the head for each, at the base too for what the change removed.
    let mut found: HashMap<(String, String), Vec<Hit>> = HashMap::new();
    for s in &symbols {
        let mut revs = vec![head];
        if s.change == Change::Removed {
            revs.push(base);
        }
        for rev in revs {
            let key = (rev.to_owned(), s.name.clone());
            if let std::collections::hash_map::Entry::Vacant(e) = found.entry(key) {
                e.insert(grep(repo, rev, &s.name)?);
            }
        }
    }
    let mut to_read: Vec<(String, String)> = found
        .iter()
        .flat_map(|((rev, _), hits)| hits.iter().map(move |h| (rev.clone(), h.path.clone())))
        .filter(|key| !read.contains_key(key))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if to_read.len() > MAX_FILES {
        partial.push(format!(
            "only {MAX_FILES} of {} files with references are read; the rest are placed by \
             their path alone",
            to_read.len()
        ));
        to_read.truncate(MAX_FILES);
    }
    read.extend(read_files(repo, &to_read)?);

    let mut sites: Vec<Site> = Vec::new();
    let mut site_index: HashMap<(String, String, String, u32), usize> = HashMap::new();
    let mut refs = Vec::new();
    let mut in_comments = 0;
    let mut cut = 0;
    for (si, s) in symbols.iter_mut().enumerate() {
        let mut revs = vec![head];
        if s.change == Change::Removed {
            revs.push(base);
        }
        let mut kept = 0;
        for rev in revs {
            let Some(hits) = found.get(&(rev.to_owned(), s.name.clone())) else {
                continue;
            };
            // A file whose lines grep stopped at may have more references past them.
            let mut per_file: HashMap<&str, usize> = HashMap::new();
            for hit in hits {
                *per_file.entry(hit.path.as_str()).or_default() += 1;
            }
            if !s.refs_cut && per_file.values().any(|&n| n > MAX_REFS) {
                s.refs_cut = true;
                cut += 1;
            }
            for hit in hits {
                let own = |span: &Option<Span>| {
                    span.as_ref()
                        .is_some_and(|sp| sp.contains(rev, &hit.path, hit.line))
                };
                if own(&s.head) || own(&s.base) {
                    continue;
                }
                let file = read.get(&(rev.to_owned(), hit.path.clone()));
                let mut site = Site {
                    rev: rev.to_owned(),
                    path: hit.path.clone(),
                    label: hit.path.clone(),
                    line: hit.line,
                    test: test_path(&hit.path),
                    in_diff: in_diff.contains(&hit.path),
                    changed: false,
                    read: file.is_some(),
                };
                if let Some(file) = file {
                    let code = (hit.line as usize)
                        .checked_sub(1)
                        .and_then(|i| file.masked.get(i));
                    if !code.is_some_and(|l| has_word(l, &s.name)) {
                        in_comments += 1;
                        continue;
                    }
                    if file.items.iter().any(|it| {
                        it.name == s.name && it.line == hit.line && it.kind != ItemKind::Impl
                    }) {
                        s.same_named += 1;
                        continue;
                    }
                    site.test |= scan::in_test(&file.items, hit.line);
                    if let Some(i) = scan::innermost(&file.items, hit.line) {
                        site.label = file.items[i].path.clone();
                        site.line = file.items[i].line;
                    }
                }
                if kept == MAX_REFS {
                    if !s.refs_cut {
                        s.refs_cut = true;
                        cut += 1;
                    }
                    break;
                }
                kept += 1;
                let key = (
                    site.rev.clone(),
                    site.path.clone(),
                    site.label.clone(),
                    site.line,
                );
                let index = *site_index.entry(key).or_insert_with(|| {
                    sites.push(site);
                    sites.len() - 1
                });
                refs.push(Reference {
                    symbol: si,
                    site: index,
                    rev: rev.to_owned(),
                    path: hit.path.clone(),
                    line: hit.line,
                    text: cut_to(hit.text.trim(), 160),
                });
            }
        }
    }
    if cut > 0 {
        partial.push(format!(
            "{cut} symbols have more than {MAX_REFS} references; only the first are kept"
        ));
    }
    // A site that is itself a changed symbol.
    for site in &mut sites {
        site.changed = symbols.iter().any(|s| {
            [&s.head, &s.base]
                .into_iter()
                .flatten()
                .any(|sp| sp.rev == site.rev && sp.path == site.path && sp.line == site.line)
        });
    }
    Ok(Impact {
        range: range.clone(),
        symbols,
        sites,
        refs,
        other,
        rust_files,
        in_comments,
        partial,
        collected: git::now(),
    })
}
