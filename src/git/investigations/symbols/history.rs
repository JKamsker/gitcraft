//! Symbol history reads, matching, and boundary interpretation.

use super::*;

#[derive(Clone, Debug)]
pub(crate) enum Answer {
    Choices(Vec<Symbol>),
    History(Biography),
}

#[derive(Clone, Debug, Default)]
pub(super) struct Changes {
    pub(super) old: BTreeSet<String>,
    pub(super) new: BTreeSet<String>,
    pub(super) edges: Vec<(String, String)>,
    pub(super) limited: bool,
}

pub(super) struct Trace<'a> {
    pub(super) reader: Reader<'a>,
    pub(super) files: BTreeMap<(String, String), Arc<Vec<Symbol>>>,
    pub(super) changes: BTreeMap<String, Changes>,
    pub(super) bytes: usize,
}

pub(super) fn path_ok(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|p| !matches!(p, "" | "." | ".." | ".git"))
}

pub(super) fn fingerprints(tokens: &[String]) -> BTreeSet<String> {
    if tokens.len() < 3 {
        return tokens.iter().cloned().collect();
    }
    tokens.windows(3).map(|w| w.join("\0")).collect()
}

pub(super) fn similarity(a: &Symbol, b: &Symbol) -> f32 {
    let a = fingerprints(&a.tokens);
    let b = fingerprints(&b.tokens);
    if a.is_empty() || b.is_empty() {
        return 0.;
    }
    (2 * a.intersection(&b).count()) as f32 / (a.len() + b.len()) as f32
}

pub(super) fn choose(
    current: &Symbol,
    candidates: &[Symbol],
) -> Result<Option<(Symbol, u8)>, String> {
    let same = candidates
        .iter()
        .filter(|s| s.path == current.path && s.qualified == current.qualified)
        .collect::<Vec<_>>();
    match same.as_slice() {
        [only] => {
            return Ok(Some((
                (*only).clone(),
                (similarity(current, only) * 100.).round() as u8,
            )));
        }
        [] => {}
        _ => {
            return Err(crate::i18n::text(
                "backend-more-than-one-function-has-this-path-and-qualified-name-structura-513daf",
            )
            .into());
        }
    }
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
        return if candidates.is_empty() {
            Ok(None)
        } else {
            Err(crate::i18n::text(
                "backend-no-unambiguous-structural-identity-survived-this-transition-body-9f6beb",
            )
            .into())
        };
    };
    if ranked
        .get(1)
        .is_some_and(|other| ((other.1 + other.2) - (score + bonus)).abs() < 0.04)
    {
        return Err(crate::i18n::text(
            "backend-two-functions-have-similarly-strong-bodies-lineage-is-ambiguous-o-7c9c12",
        )
        .into());
    }
    Ok(Some(((*best).clone(), (score * 100.).round() as u8)))
}

pub(super) fn rename_path(bytes: &[u8]) -> Result<&str, &'static str> {
    std::str::from_utf8(bytes).map_err(|_| crate::i18n::text("backend-symbol-rename-not-utf8"))
}

pub(super) fn boundary_event(
    raw_commit: &[u8],
    traversable_parents: &[u8],
) -> Result<(&'static str, bool), String> {
    let headers = raw_commit
        .split(|b| *b == b'\n')
        .take_while(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if !headers
        .first()
        .and_then(|line| line.strip_prefix(b"tree "))
        .is_some_and(|tree| std::str::from_utf8(tree).is_ok_and(oid))
    {
        return Err(crate::i18n::text(
            "backend-invalid-stored-commit-header-at-symbol-boundary-6a933f",
        )
        .into());
    }
    let mut parents = 0;
    for line in headers {
        if let Some(parent) = line.strip_prefix(b"parent ") {
            if !std::str::from_utf8(parent).is_ok_and(oid) {
                return Err(
                    crate::i18n::text("backend-invalid-stored-commit-parent-d31683").into(),
                );
            }
            parents += 1;
        }
    }
    Ok(if parents == 0 {
        ("Created", false)
    } else if traversable_parents.iter().all(u8::is_ascii_whitespace) {
        ("Earlier shallow boundary (creation unproven)", true)
    } else {
        ("Earlier boundary (creation unproven)", true)
    })
}

pub(super) fn display_boundary(boundary: &str) -> &str {
    let id = match boundary {
        "Created" => "backend-created-d70b9e",
        "Earlier shallow boundary (creation unproven)" => {
            "backend-earlier-shallow-boundary-creation-unproven-4afdd7"
        }
        "Earlier boundary (creation unproven)" => {
            "backend-earlier-boundary-creation-unproven-801381"
        }
        _ => return boundary,
    };
    crate::i18n::text(id)
}

pub(super) fn event(before: &Symbol, after: &Symbol) -> String {
    let mut events = Vec::new();
    if before.path != after.path {
        events.push(crate::i18n::text("backend-moved-b11c90"));
    }
    if before.qualified != after.qualified {
        events.push(crate::i18n::text("backend-renamed-05487a"));
    }
    if before.tokens != after.tokens {
        events.push(crate::i18n::text("backend-body-edited-719318"));
    } else if before.body != after.body {
        events.push(crate::i18n::text("backend-reformatted-10c026"));
    }
    if events.is_empty() {
        crate::i18n::text("backend-selected-version-f988b6").into()
    } else {
        events.join(" · ")
    }
}

pub(super) fn splits(before: &Symbol, primary: &Symbol, all: &[Symbol]) -> Vec<Symbol> {
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
        .filter(|s| {
            s.path != primary.path || s.first != primary.first || s.qualified != primary.qualified
        })
        .filter(|s| {
            let fp = fingerprints(&s.tokens);
            let overlap = old.intersection(&fp).count();
            let union = covered.union(&fp).filter(|p| old.contains(*p)).count();
            overlap >= 6
                && overlap * 100 >= fp.len() * 55
                && union * 100 >= old.len() * 65
                && old
                    .intersection(&fp)
                    .filter(|p| !primary_fp.contains(*p))
                    .count()
                    >= 6
                && old
                    .intersection(&fp)
                    .filter(|p| !primary_fp.contains(*p))
                    .count()
                    * 100
                    >= fp.len() * 30
        })
        .take(8)
        .cloned()
        .collect()
}

impl Trace<'_> {
    pub(super) fn file(&mut self, at: &str, path: &str) -> Result<Arc<Vec<Symbol>>, String> {
        let key = (at.to_owned(), path.to_owned());
        if let Some(symbols) = self.files.get(&key) {
            return Ok(symbols.clone());
        }
        if !self.reader.active() {
            return Err("Symbol history cancelled or reached its time budget".into());
        }
        if self.files.len() >= 1500 {
            return Err(crate::i18n::text(
                "backend-symbol-history-reached-1500-file-versions-reduce-the-history-dept-44674d",
            )
            .into());
        }
        if !path_ok(path) {
            return Err(crate::i18n::text("backend-unsafe-symbol-history-path-2d73ae").into());
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
        let fields = tree.split(|b| *b == b'\t').next().ok_or(crate::i18n::text(
            "backend-invalid-symbol-file-metadata-19117c",
        ))?;
        let fields = std::str::from_utf8(fields)
            .map_err(|_| crate::i18n::text("backend-invalid-symbol-metadata-d188d5"))?
            .split_whitespace()
            .collect::<Vec<_>>();
        let [mode, kind, blob] = fields.as_slice() else {
            return Err(crate::i18n::text("backend-invalid-symbol-file-metadata-19117c").into());
        };
        if !matches!(*mode, "100644" | "100755") || *kind != "blob" || !oid(blob) {
            return Err(crate::i18n::text(
                "backend-symbol-history-supports-regular-source-files-only-ed227c",
            )
            .into());
        }
        let size = self.reader.run(&["cat-file", "-s", blob], vec![], &[0])?;
        let size = std::str::from_utf8(&size)
            .map_err(|_| crate::i18n::text("backend-invalid-source-size-32e774"))?
            .trim()
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-source-size-32e774"))?;
        if size > 2 * 1024 * 1024 || self.bytes.saturating_add(size) > 64 * 1024 * 1024 {
            return Err(crate::i18n::text(
                "backend-symbol-history-reached-2-mib-file-or-64-mib-total-source-budget-67d2a8",
            )
            .into());
        }
        let bytes = self.reader.run(&["cat-file", "blob", blob], vec![], &[0])?;
        if bytes.len() != size || bytes.contains(&0) {
            return Err(crate::i18n::text(
                "backend-binary-or-incomplete-source-is-not-parsed-5f25dc",
            )
            .into());
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            crate::i18n::text("backend-symbol-history-requires-utf-8-source-5761c0")
        })?;
        self.bytes += size;
        let symbols = Arc::new(syntax::parse(path, text, blob, &|| self.reader.active())?);
        self.files.insert(key, symbols.clone());
        Ok(symbols)
    }
    pub(super) fn diff(&mut self, parent: &str, child: &str) -> Result<Changes, String> {
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
            let path = std::str::from_utf8(
                fields
                    .get(i)
                    .ok_or(crate::i18n::text("backend-truncated-file-change-f4e8b8"))?,
            )
            .map_err(|_| crate::i18n::text("backend-non-utf-8-file-history-4d9013"))?;
            i += 1;
            if !path_ok(path) {
                changes.limited = true;
                continue;
            }
            if matches!(status.first(), Some(b'R' | b'C')) {
                let next = rename_path(
                    fields
                        .get(i)
                        .ok_or(crate::i18n::text("backend-truncated-rename-d463db"))?,
                )?;
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
    pub(super) fn candidates(
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
                return Err(crate::i18n::text(
                    "backend-too-many-structural-function-candidates-c6b12a",
                )
                .into());
            }
        }
        Ok(out)
    }
    pub(super) fn step(
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
            return Err(crate::i18n::text("backend-invalid-symbol-event-metadata-c646cc").into());
        }
        Ok(Step {
            commit: commit.into(),
            author: String::from_utf8_lossy(fields[0])
                .chars()
                .take(200)
                .collect(),
            time: std::str::from_utf8(fields[1])
                .map_err(|_| crate::i18n::text("backend-invalid-event-time-719624"))?
                .parse()
                .map_err(|_| crate::i18n::text("backend-invalid-event-time-719624"))?,
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
