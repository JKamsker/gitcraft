use super::*;

/// [`load`], or with `lines` only the part of a commit's diff that changed those lines of the
/// file (as they are in the commit), as `git log -L` shows it.
pub fn load_lines(
    repo: &Path,
    key: &DiffKey,
    lines: Option<&[git::LineRange]>,
    opts: DiffOptions,
    dark: bool,
) -> Result<DiffData, String> {
    match (lines, key) {
        (Some(ranges), DiffKey::Commit { hash, path }) => {
            let text = git::line_diff(repo, hash, path, ranges)?;
            // Not the commit's patch: nothing of it is picked to revert.
            // Line history always uses raw source lines, including its initial log query.
            // Decryption remains an explicit whole-file comparison in the private owner.
            let mut data = prepare(repo, key, path, &text, false, true, dark);
            data.sops = encrypted::probe_checked(repo, key) == Some(true);
            Ok(data)
        }
        _ => load(repo, key, opts, dark),
    }
}

/// Reads and prepares the diff for `key`.
pub fn load(repo: &Path, key: &DiffKey, opts: DiffOptions, dark: bool) -> Result<DiffData, String> {
    let at = Instant::now();
    let recognized = encrypted::probe_checked(repo, key);
    crate::timing::step("SOPS probe", at);
    let sops = recognized == Some(true);
    let opts = DiffOptions {
        no_textconv: opts.no_textconv || recognized != Some(false),
        ..opts
    };
    let patch = |opts: DiffOptions| -> Result<(String, String), String> {
        Ok(match key {
            DiffKey::Commit { hash, path } => {
                (git::file_diff(repo, hash, path, opts)?, path.clone())
            }
            DiffKey::Work(file) => (git::work_diff(repo, file, opts)?, file.path.clone()),
            DiffKey::Range { from, to, path } => (
                git::range_file_diff(repo, from, to.as_deref(), path, opts)?,
                path.clone(),
            ),
            DiffKey::Remerge {
                hash,
                path,
                old_path,
                ..
            } => (
                git::remerge_file_diff(repo, hash, path, old_path.as_deref(), opts)?,
                path.clone(),
            ),
        })
    };
    // A textconv driver that fails (Git for Windows maps `*.docx` and `*.pdf` to its own
    // `astextplain`, which refuses some files) fails the whole diff: the file's own bytes
    // are diffed instead, so it is shown as a binary rather than not at all.
    let at = Instant::now();
    let (text, path) = match patch(opts) {
        Err(e) if !opts.no_textconv => {
            crate::timing::log("textconv failed: diffed again without it");
            patch(DiffOptions {
                no_textconv: true,
                ..opts
            })
            .map_err(|_| e)?
        }
        done => done?,
    };
    crate::timing::step(
        &format!(
            "patch ({} KiB, {} lines, {})",
            text.len() / 1024,
            text.lines().count(),
            crate::timing::kind_of(&path)
        ),
        at,
    );
    let pickable = !opts.ignore_ws;
    let at = Instant::now();
    let mut data = prepare(repo, key, &path, &text, pickable, opts.whole_file, dark);
    crate::timing::step("prepared", at);
    data.sops = sops;
    Ok(data)
}

/// The diff `text` of `path` read into lines, coloured and paired. `pickable`: hunks and lines
/// of it can be picked on their own (it is the patch git would apply).
pub(super) fn prepare(
    repo: &Path,
    key: &DiffKey,
    path: &str,
    text: &str,
    pickable: bool,
    whole_file: bool,
    dark: bool,
) -> DiffData {
    let path = path.to_owned();
    let mut lines = parse(text);
    if let DiffKey::Remerge { conflicts, .. } = key {
        lines = remerge::label(conflicts, lines, !pickable);
    }
    // Read only when the diff doesn't show the whole file already.
    let mut at = Instant::now();
    let context = (!whole_file && !is_image(&path))
        .then(|| context::read(repo, key, &lines, dark))
        .flatten();
    crate::timing::step("  file around the hunks", at);
    at = Instant::now();
    if context
        .as_ref()
        .and_then(|c| c.gaps.last())
        .is_some_and(|g| g.trailing && g.hidden() > 0)
    {
        lines.push(context::fold_row());
    }
    let header = patch_header(text);
    let picks = pickable && can_pick(&header, &lines);
    let partial =
        picks && matches!(key, DiffKey::Work(f) if matches!(f.area, Area::Staged | Area::Unstaged));
    // Not a submodule's change, which `git apply` would report done and leave as it was.
    let revertable = picks
        && matches!(key, DiffKey::Commit { .. })
        && git::check_revertable(text, &path).is_ok();
    let new_file = is_new_file(text);
    let lfs = lfs_pair(&lines);
    // An image in LFS: git has only its pointer.
    let image = if is_image(&path) && lfs.is_none() {
        Some(load_images(repo, key, &path, old_name(text).as_deref()))
    } else {
        None
    };
    let binary = (image.is_none() && lfs.is_none() && binary::is_binary_patch(text))
        .then(|| binary::load_pair(repo, key, &path, images::renamed_from(text)));
    // A diff of what the file's textconv made of it: its lines aren't the file's. Not for a
    // symbolic link: git diffs those with its default driver, whatever the attribute says.
    let textconv = (binary.is_none() && !lines.is_empty() && !is_symlink_patch(text))
        .then(|| binary::textconv_for(repo, &path))
        .flatten();
    let partial = partial && textconv.is_none();
    let compare = image
        .as_ref()
        .and_then(|pair| image_compare::compare(repo, key, &path, pair));
    let notebook = (notebook::is_notebook(&path) && lfs.is_none())
        .then(|| notebook::load(repo, key, &path, images::renamed_from(text)))
        .flatten();
    crate::timing::step("  images, binary, textconv, notebook", at);
    at = Instant::now();
    let mut spans = highlight_sides(&path, &lines, dark);
    crate::timing::step("  highlighted", at);
    at = Instant::now();
    let moves = find_moves(&lines);
    let (mut changed, split) = pair_lines(&lines);
    crate::timing::step("  moved and paired lines", at);
    let mut uncut = std::collections::HashMap::new();
    for (i, (line, spans)) in lines.iter_mut().zip(spans.iter_mut()).enumerate() {
        if let Some(whole) = cap_line(&mut line.text, spans.as_mut()) {
            uncut.insert(i, whole);
        }
    }
    for range in changed.iter_mut().flatten() {
        *range = (range.0.min(MAX_LINE_CHARS), range.1.min(MAX_LINE_CHARS));
    }
    let longest = lines
        .iter()
        .map(|l| l.text.chars().count())
        .max()
        .unwrap_or(0);
    let authors = line_authors(repo, key, &path, &lines);
    let line_counts = count_lines(&lines);
    let weakened = weakened::by_hunk_line(text, &lines);
    DiffData {
        line_counts,
        sops: false,
        lines,
        spans,
        changed,
        split,
        image,
        compare,
        lfs,
        binary,
        textconv,
        notebook,
        header,
        partial,
        revertable,
        moves,
        context,
        new_file,
        longest,
        source: {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut h);
            h.finish()
        },
        geometry_revision: 0,
        uncut,
        weakened,
        authors,
        map: Default::default(),
        hunks: Default::default(),
    }
}

/// The moved blocks of a two-sided diff; a combined one (a conflict's) has no line numbers to
/// tell its sides by.
pub(super) fn find_moves(lines: &[DiffLine]) -> moved::Moves {
    let numbered = lines.iter().all(|l| match l.kind {
        Kind::Add => l.new.is_some(),
        Kind::Del => l.old.is_some(),
        _ => true,
    });
    if numbered {
        moved::detect(lines)
    } else {
        moved::Moves::default()
    }
}

/// Who wrote each added line of a commit's diff, as the commit records it (git-ai notes, Agent
/// Trace, its author or trailers); nothing for other diffs or a commit that records nothing.
pub(super) fn line_authors(
    repo: &Path,
    key: &DiffKey,
    path: &str,
    lines: &[DiffLine],
) -> Vec<Option<crate::authorship::Who>> {
    let DiffKey::Commit { hash, .. } = key else {
        return Vec::new();
    };
    let Some(a) = crate::authorship::load::one(repo, hash).ok().flatten() else {
        return Vec::new();
    };
    lines
        .iter()
        .map(|l| match (l.kind, l.new) {
            (Kind::Add, Some(n)) => Some(a.line(path, n)),
            _ => None,
        })
        .collect()
}

/// Whether the patch is of a symbolic link (mode 120000 before its first hunk, no file mode).
pub(super) fn is_symlink_patch(text: &str) -> bool {
    let modes: Vec<&str> = text
        .split_terminator('\n')
        .take_while(|l| !l.starts_with("@@"))
        .filter(|l| l.starts_with("index ") || l.contains(" mode "))
        .filter_map(|l| l.rsplit(' ').next())
        .filter(|m| m.len() == 6 && m.bytes().all(|c| c.is_ascii_digit()))
        .collect();
    modes.contains(&"120000") && modes.iter().all(|m| *m == "120000")
}

/// The lines `lines` adds and removes; `None` when it has none to count (a binary file, one
/// git doesn't diff).
pub(super) fn count_lines(lines: &[DiffLine]) -> (Option<u32>, Option<u32>) {
    if lines.iter().all(|l| l.kind == Kind::Meta) {
        return (None, None);
    }
    let count = |kind| {
        let n = lines.iter().filter(|l| l.kind == kind).count();
        Some(u32::try_from(n).unwrap_or(u32::MAX))
    };
    (count(Kind::Add), count(Kind::Del))
}

/// Whether hunks and lines of this diff can be picked on their own (it isn't a `-w` diff,
/// whose patch wouldn't apply: see [`prepare`]).
pub(super) fn can_pick(header: &str, lines: &[DiffLine]) -> bool {
    // New, deleted, renamed and binary files, truncated diffs and text that isn't UTF-8 only
    // go whole.
    !header.is_empty()
        && ![
            "new file mode",
            "deleted file mode",
            "rename from",
            "copy from",
            "Binary files",
        ]
        .iter()
        .any(|h| header.contains(h))
        && lines
            .iter()
            .all(|l| l.kind != Kind::Meta || l.raw.starts_with('\\'))
        // Not UTF-8 (a Latin-1 file…): the text was read with its odd bytes replaced, and a
        // patch made from it would write the replacements into the added lines.
        && !lines.iter().any(|l| l.raw.contains(char::REPLACEMENT_CHARACTER))
}

/// Syntax colours per line. The old and the new side are each coloured as one stream, so
/// multi-line constructs work.
pub(super) fn highlight_sides(path: &str, lines: &[DiffLine], dark: bool) -> Vec<Option<Spans>> {
    let mut spans = vec![None; lines.len()];
    let old_side: Vec<usize> = (0..lines.len())
        .filter(|&i| matches!(lines[i].kind, Kind::Context | Kind::Del))
        .collect();
    let new_side: Vec<usize> = (0..lines.len())
        .filter(|&i| matches!(lines[i].kind, Kind::Context | Kind::Add))
        .collect();
    if let Some(old) =
        highlight::highlight(path, old_side.iter().map(|&i| lines[i].text.as_str()), dark)
    {
        for (k, &i) in old_side.iter().enumerate() {
            if lines[i].kind == Kind::Del {
                spans[i] = old.get(k).cloned();
            }
        }
    }
    if let Some(new) =
        highlight::highlight(path, new_side.iter().map(|&i| lines[i].text.as_str()), dark)
    {
        for (k, &i) in new_side.iter().enumerate() {
            spans[i] = new.get(k).cloned();
        }
    }
    spans
}
