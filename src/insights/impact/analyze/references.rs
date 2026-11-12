//! Place bounded name matches at the exact analyzed revisions and classify their evidence.
use super::*;

pub(super) fn place(
    symbols: &mut [Symbol],
    found: &HashMap<(String, String), Vec<Hit>>,
    read: &HashMap<(String, String), Read>,
    in_diff: &HashSet<String>,
    range: &Range,
    partial: &mut Vec<String>,
) -> (Vec<Site>, Vec<Reference>, u32) {
    let (base, head) = (range.base.as_str(), range.head.as_str());
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
    (sites, refs, in_comments)
}
