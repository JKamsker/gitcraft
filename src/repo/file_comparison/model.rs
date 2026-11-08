//! Pairwise Git alignment is composed around the index, keeping all three versions visible.
use crate::git::file_comparison::{self as files, Lens, Version};
use crate::repo::diff::{self, DiffLine, Kind, Pick};
use std::collections::BTreeSet;
use std::process::Command;

#[derive(Clone)]
pub(super) struct Boundary {
    pub lines: Vec<DiffLine>,
    pub pairs: Vec<(Option<usize>, Option<usize>)>,
    pub header: String,
    pub full: String,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    pub reason: Option<String>,
}
impl Boundary {
    pub fn writable(&self) -> bool {
        self.reason.is_none()
            && (self.old_mode.is_some() || self.new_mode.is_some())
            && [self.old_mode.as_deref(), self.new_mode.as_deref()]
                .into_iter()
                .all(|mode| matches!(mode, None | Some("100644" | "100755" | "120000")))
    }
    pub fn changed(&self) -> BTreeSet<usize> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| matches!(l.kind, Kind::Add | Kind::Del).then_some(i))
            .collect()
    }
    pub fn patch(&self, path: &str, pick: Pick, chosen: &BTreeSet<usize>) -> Option<String> {
        if !self.writable() || chosen.is_empty() || !chosen.is_subset(&self.changed()) {
            return None;
        }
        let whole = *chosen == self.changed();
        if whole {
            return Some(self.full.clone());
        }
        let header = if !whole
            && ((pick == Pick::Unstage && self.old_mode.is_none())
                || (pick == Pick::Stage && self.new_mode.is_none()))
        {
            let mode = self.old_mode.as_deref().or(self.new_mode.as_deref());
            files::patch_header(path, mode, mode)
        } else {
            self.header.clone()
        };
        diff::partial_patch(&header, &self.lines, chosen, pick)
    }
}
pub(super) struct Pair {
    pub labels: [String; 2],
    pub versions: [Version; 2],
    pub boundary: Boundary,
}
#[derive(Clone)]
pub(super) struct Row {
    pub text: [Option<String>; 3],
    pub changes: [BTreeSet<usize>; 2],
    pub undoes: bool,
}
pub(super) struct LensData {
    pub snapshot: Lens,
    pub boundaries: [Boundary; 2],
    pub rows: Vec<Row>,
    pub undoing_staged: bool,
}

pub(super) fn prepare_lens(snapshot: Lens, active: &dyn Fn() -> bool) -> Result<LensData, String> {
    let [head, index, work] = &snapshot.versions;
    let left = boundary(&snapshot.spec.path, head, index, active)?;
    let right = boundary(&snapshot.spec.path, index, work, active)?;
    let rows = align(head, index, work, &left, &right);
    let undoing_staged = head.mode == work.mode
        && head.bytes == work.bytes
        && (head.mode != index.mode || head.bytes != index.bytes);
    Ok(LensData {
        snapshot,
        boundaries: [left, right],
        rows,
        undoing_staged,
    })
}

pub(super) fn boundary(
    path: &str,
    before: &Version,
    after: &Version,
    active: &dyn Fn() -> bool,
) -> Result<Boundary, String> {
    let header = files::patch_header(path, before.mode.as_deref(), after.mode.as_deref());
    let reason = before
        .warning
        .clone()
        .or_else(|| after.warning.clone())
        .or_else(|| {
            (before.text().is_none() || after.text().is_none()).then(|| {
                crate::i18n::text("repo-workflows-file_comparison-boundary-26efe746").into()
            })
        });
    if reason.is_some() {
        return Ok(Boundary {
            lines: Vec::new(),
            pairs: Vec::new(),
            full: String::new(),
            header,
            old_mode: before.mode.clone(),
            new_mode: after.mode.clone(),
            reason,
        });
    }
    let a = before.text().unwrap_or_default();
    let b = after.text().unwrap_or_default();
    if a.split_inclusive('\n').count() > 20_000 || b.split_inclusive('\n').count() > 20_000 {
        return Err(crate::i18n::text("repo-workflows-file_comparison-boundary-e44610d7").into());
    }
    let body = if before.bytes == after.bytes {
        String::new()
    } else {
        let temp = files::Scratch::new()?;
        let old = temp.0.join("before");
        let new = temp.0.join("after");
        std::fs::write(&old, &before.bytes)
            .and_then(|_| std::fs::write(&new, &after.bytes))
            .map_err(|e| e.to_string())?;
        let mut command = Command::new("git");
        crate::shell_path::apply(&mut command);
        // This is a pure byte comparison of two application-owned files, never repository
        // discovery, attributes, textconv, a filter, an encrypted renderer or a user program.
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GIT_") {
                command.env_remove(key);
            }
        }
        let empty = temp.0.join("empty-config");
        std::fs::write(&empty, "").map_err(|e| e.to_string())?;
        command
            .current_dir(&temp.0)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &empty)
            .env("GIT_ATTR_NOSYSTEM", "1")
            .env("GIT_CEILING_DIRECTORIES", &temp.0)
            .args([
                "--no-lazy-fetch",
                "diff",
                "--no-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--no-renames",
                "--diff-algorithm=histogram",
                "--unified=20000",
                "--",
            ])
            .arg(old)
            .arg(new);
        let output = crate::processes::run_while(
            command,
            crate::processes::Options {
                timeout: Some(std::time::Duration::from_secs(20)),
                output_limit: 8 * 1024 * 1024,
                stderr_limit: Some(64 * 1024),
                ..Default::default()
            },
            active,
        )?
        .complete()?;
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Err(
                crate::i18n::text("repo-workflows-file_comparison-boundary-b861cbb0").into(),
            );
        }
        let text = String::from_utf8(output.stdout)
            .map_err(|_| crate::i18n::text("repo-workflows-file_comparison-boundary-5421cc1b"))?;
        let start = text.find("@@ ").ok_or(crate::i18n::text(
            "repo-workflows-file_comparison-boundary-6732715f",
        ))?;
        text[start..].to_owned()
    };
    let full = format!("{header}{body}");
    let lines = diff::parse(&full);
    let pairs = diff::comparison_pairs(&lines);
    Ok(Boundary {
        lines,
        pairs,
        header,
        full,
        old_mode: before.mode.clone(),
        new_mode: after.mode.clone(),
        reason: None,
    })
}

fn lines(version: &Version) -> Vec<String> {
    version
        .text()
        .unwrap_or_default()
        .split_inclusive('\n')
        .map(str::to_owned)
        .collect()
}
#[derive(Clone)]
struct Side {
    before: Vec<Vec<(String, BTreeSet<usize>)>>,
    at: Vec<Option<(String, BTreeSet<usize>)>>,
    changed: Vec<BTreeSet<usize>>,
}
fn slots(boundary: &Boundary, source: &Version, index: &Version, left: bool) -> Side {
    let index_lines = lines(index);
    let mut result = Side {
        before: vec![Vec::new(); index_lines.len() + 1],
        at: vec![None; index_lines.len()],
        changed: vec![BTreeSet::new(); index_lines.len()],
    };
    if source.bytes == index.bytes {
        for (i, s) in lines(source).into_iter().enumerate() {
            if let Some(at) = result.at.get_mut(i) {
                *at = Some((s, BTreeSet::new()));
            }
        }
        return result;
    }
    let source_lines = lines(source);
    let mut cursor = 0;
    for (old, new) in &boundary.pairs {
        let (outer, central) = if left { (*old, *new) } else { (*new, *old) };
        let outer = outer
            .and_then(|i| boundary.lines.get(i).map(|l| (i, l)))
            .filter(|(_, l)| matches!(l.kind, Kind::Context | Kind::Add | Kind::Del));
        let central = central
            .and_then(|i| boundary.lines.get(i).map(|l| (i, l)))
            .filter(|(_, l)| matches!(l.kind, Kind::Context | Kind::Add | Kind::Del));
        let changed = outer
            .iter()
            .chain(central.iter())
            .filter_map(|(i, l)| matches!(l.kind, Kind::Add | Kind::Del).then_some(*i))
            .collect::<BTreeSet<_>>();
        let text = outer.and_then(|(_, l)| {
            (if left { l.old } else { l.new })
                .and_then(|n| source_lines.get(n.saturating_sub(1) as usize))
                .cloned()
        });
        if let Some((_, middle)) = central {
            let number = if left { middle.new } else { middle.old };
            if let Some(i) = number.and_then(|n| usize::try_from(n.saturating_sub(1)).ok()) {
                cursor = i + 1;
                if let Some(at) = result.at.get_mut(i) {
                    result.changed[i] = changed.clone();
                    *at = text.map(|s| (s, changed));
                }
            }
        } else if let Some(s) = text
            && let Some(slot) = result.before.get_mut(cursor)
        {
            slot.push((s, changed));
        }
    }
    result
}
fn align(
    head: &Version,
    index: &Version,
    work: &Version,
    left: &Boundary,
    right: &Boundary,
) -> Vec<Row> {
    let center = lines(index);
    let a = slots(left, head, index, true);
    let b = slots(right, work, index, false);
    let mut result = Vec::new();
    for i in 0..=center.len() {
        for j in 0..a.before[i].len().max(b.before[i].len()) {
            let l = a.before[i].get(j);
            let r = b.before[i].get(j);
            let text = [l.map(|x| x.0.clone()), None, r.map(|x| x.0.clone())];
            let undoes = text[0].is_some() && text[0] == text[2];
            result.push(Row {
                text,
                changes: [
                    l.map(|x| x.1.clone()).unwrap_or_default(),
                    r.map(|x| x.1.clone()).unwrap_or_default(),
                ],
                undoes,
            });
        }
        if let Some(middle) = center.get(i) {
            let l = a.at[i].as_ref();
            let r = b.at[i].as_ref();
            let text = [
                l.map(|x| x.0.clone()),
                Some(middle.clone()),
                r.map(|x| x.0.clone()),
            ];
            let undoes = text[0] == text[2] && text[0] != text[1];
            result.push(Row {
                text,
                changes: [a.changed[i].clone(), b.changed[i].clone()],
                undoes,
            });
        }
    }
    result
}
