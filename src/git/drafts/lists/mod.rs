//! File changelists keep independent messages and raw staged/work layers. Publication
//! uses a private index, an owned native index exchange, and one HEAD/state ref transaction.
mod index;
#[cfg(test)]
mod tests;
use super::working_transaction::Contents;
use super::*;
use std::sync::Arc;
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Lists {
    pub lists: Vec<List>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct List {
    pub id: String,
    pub name: String,
    pub message: String,
    pub paths: BTreeSet<String>,
    pub shelf: Option<Shelf>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Shelf {
    #[serde(default)]
    access: native_files::Policies,
    permissions: BTreeMap<String, u32>,
    base: BTreeMap<String, Option<Entry>>,
    index: BTreeMap<String, Option<Entry>>,
    work: BTreeMap<String, Option<Entry>>,
}
#[derive(Clone)]
pub struct View {
    pub lists: Lists,
    pub paths: Vec<String>,
    pub version: String,
    pub reference: String,
    pub(super) source: Arc<Source>,
    pub(super) work: Arc<Contents>,
    pub(super) policies: Arc<native_files::Policies>,
    pub(super) head: Arc<BTreeMap<String, Entry>>,
}
pub(super) fn entries(
    ctx: &Context,
    rev: &str,
    active: &dyn Fn() -> bool,
) -> Result<BTreeMap<String, Entry>, String> {
    let raw = ctx.run(&["ls-tree", "-rz", rev], vec![], &[0], active)?;
    let mut out = BTreeMap::new();
    for row in raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let row = std::str::from_utf8(row)
            .map_err(|_| crate::i18n::text("backend-changelist-paths-must-be-utf-8-4d79f3"))?;
        let (info, path) = row
            .split_once('\t')
            .ok_or(crate::i18n::text("backend-invalid-tree-c5c836"))?;
        valid_path(path)?;
        let p = info.split_whitespace().collect::<Vec<_>>();
        if p.len() != 3 {
            return Err(crate::i18n::text("backend-invalid-tree-entry-8d7b6f").into());
        }
        out.insert(
            path.into(),
            Entry {
                mode: p[0].into(),
                oid: p[2].into(),
            },
        );
        if out.len() > MAX_INDEX {
            return Err(crate::i18n::text("backend-tree-exceeds-20000-paths-68cadd").into());
        }
    }
    Ok(out)
}
fn state(ctx: &Context, active: &dyn Fn() -> bool) -> Result<(String, String, Lists), String> {
    super::publication::guard(ctx)?;
    let reference = format!("refs/gitcraft/changelists/{}/state", ctx.id);
    if !ctx
        .source(&["symbolic-ref", "-q", &reference], vec![], &[0, 1], active)?
        .is_empty()
    {
        return Err(crate::i18n::text(
            "backend-changelist-state-must-be-a-direct-application-ref-no-state-write-6592ce",
        )
        .into());
    }
    let old = String::from_utf8(ctx.source(
        &["rev-parse", "--verify", "-q", &reference],
        vec![],
        &[0, 1],
        active,
    )?)
    .map_err(|_| crate::i18n::text("backend-invalid-list-ref-bd1015"))?
    .trim()
    .to_owned();
    let lists = if old.is_empty() {
        Lists::default()
    } else {
        let spec = format!("{old}:lists.json");
        let size = ctx
            .text(&["cat-file", "-s", &spec], vec![], active)?
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-lists-size-334524"))?;
        if size > 2 * 1024 * 1024 {
            return Err(
                crate::i18n::text("backend-changelist-registry-exceeds-2-mib-cd36e1").into(),
            );
        }
        serde_json::from_slice(&ctx.run(&["cat-file", "blob", &spec], vec![], &[0], active)?)
            .map_err(|_| crate::i18n::text("backend-invalid-changelist-registry-8193c3"))?
    };
    Ok((reference, old, lists))
}
/// The allowance is decided before invoking a reader, not after retaining its bytes.
/// Synthetic tests exercise this contract with lengths, without allocating payloads.
pub(super) struct SnapshotBudget {
    remaining: usize,
}
impl SnapshotBudget {
    pub(super) fn from_sizes(sizes: impl IntoIterator<Item = usize>) -> Result<Self, String> {
        let mut out = Self {
            remaining: TOTAL_LIMIT,
        };
        for size in sizes {
            out.remaining = out.remaining.checked_sub(size).ok_or(crate::i18n::text(
                "backend-working-snapshot-exceeds-32-mib-71d863",
            ))?;
        }
        Ok(out)
    }
    pub(super) fn allowance(&self) -> Result<usize, String> {
        if self.remaining == 0 {
            Err(crate::i18n::text(
                "backend-working-snapshot-exhausted-its-32-mib-allowance-before-the-next-f-c0be03",
            )
            .into())
        } else {
            Ok(self.remaining.min(FILE_LIMIT))
        }
    }
    pub(super) fn retain(&mut self, size: usize) -> Result<(), String> {
        if size > self.remaining.min(FILE_LIMIT) {
            return Err(crate::i18n::text(
                "backend-file-reader-exceeded-its-remaining-snapshot-allowance-4576a2",
            )
            .into());
        }
        self.remaining = self.remaining.checked_sub(size).ok_or(crate::i18n::text(
            "backend-working-snapshot-overflow-8e6d8f",
        ))?;
        Ok(())
    }
}
pub fn read(repo: &Path, active: &dyn Fn() -> bool) -> Result<View, String> {
    let ctx = Context::new(repo, active)?;
    let (reference, version, lists) = state(&ctx, active)?;
    let now = source(&ctx, active)?;
    let head = entries(
        &ctx,
        now.head.as_deref().ok_or(crate::i18n::text(
            "backend-make-an-initial-commit-before-using-changelists-e503e2",
        ))?,
        active,
    )?;
    let index_tree = tree(&ctx, &now.index, active)?;
    cached_index(&ctx, &now, active)?;
    let mut paths = changed(&ctx, &now, &index_tree, active)?;
    paths.extend(
        now.index
            .keys()
            .chain(head.keys())
            .filter(|p| now.index.get(*p) != head.get(*p))
            .cloned(),
    );
    paths.extend(untracked_paths(ctx.source(
        &["ls-files", "--others", "--exclude-standard", "-z"],
        vec![],
        &[0],
        active,
    )?)?);
    for list in &lists.lists {
        paths.extend(list.paths.iter().cloned());
    }
    if paths.len() > MAX_FILES {
        return Err(crate::i18n::text(
            "backend-changelists-support-up-to-2000-changed-paths-f28952",
        )
        .into());
    }
    let mut work = BTreeMap::new();
    let mut budget = SnapshotBudget::from_sizes([])?;
    for p in &paths {
        super::restore::included(&ctx, &now, p, active)?;
        let mode = now.index.get(p).map_or("100644", |e| e.mode.as_str());
        if !matches!(mode, "100644" | "100755") {
            return Err(crate::i18n::message(
                "backend-changelists-require-regular-files-p-5d56fd",
                &[("pointer", (p).to_string().into())],
            ));
        }
        let limit = budget.allowance()?;
        let data = read_file_limited(&ctx, p, mode, limit, active)?;
        budget.retain(data.as_ref().map_or(0, |(_, b)| b.len()))?;
        work.insert(p.clone(), data);
    }
    if source(&ctx, active)? != now {
        return Err(
            crate::i18n::text("backend-index-moved-while-reading-lists-reload-5ea823").into(),
        );
    }
    Ok(View {
        lists,
        paths: paths.into_iter().collect(),
        reference,
        version,
        source: Arc::new(now),
        policies: Arc::new(native_files::capture_policies(repo, &work, active)?),
        work: Arc::new(work),
        head: Arc::new(head),
    })
}
fn verify(ctx: &Context, view: &View, work: bool, active: &dyn Fn() -> bool) -> Result<(), String> {
    if state(ctx, active)?.1 != view.version || source(ctx, active)? != *view.source {
        return Err(crate::i18n::text(
            "backend-changelists-head-or-staged-contents-changed-reopen-the-lists-f336ff",
        )
        .into());
    }
    if work {
        if native_files::capture_policies(&ctx.repo, &view.work, active)? != *view.policies {
            return Err(crate::i18n::text(
                "backend-working-file-full-access-policy-changed-since-preview-5345af",
            )
            .into());
        }
        for (p, expected) in view.work.iter() {
            if read_file(
                ctx,
                p,
                expected.as_ref().map_or("100644", |(m, _)| m),
                active,
            )? != *expected
            {
                return Err(crate::i18n::message(
                    "backend-p-changed-since-preview-reopen-the-lists-520f7d",
                    &[("pointer", (p).to_string().into())],
                ));
            }
        }
    }
    Ok(())
}

mod shelve;
#[cfg(test)]
use shelve::require_restore_ownership;
pub use shelve::shelve;

mod commit;
pub use commit::commit;
pub(super) use commit::data;
use commit::final_index;
pub(super) use commit::raw_entries;
#[cfg(test)]
use commit::selected_index_records;

mod publication;
pub use publication::Edit;
pub(super) use publication::direct_ref_guard;
pub(super) use publication::direct_update;
pub(super) use publication::direct_verify;
pub use publication::edit;
use publication::publish;
