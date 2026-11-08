//! Exact file/patch review receipts with explicit dependency witnesses and conservative
//! Rust static call analysis. Unresolved dependency edges remain visibly unchecked.
use super::{rust_dependencies, store};
use crate::{
    git,
    pull_request::{PrDetails, PrFiles},
    repo::{Action, RepoTab},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
#[derive(Clone, Serialize, Deserialize, Default)]
struct Marks(BTreeMap<String, Mark>);
#[derive(Clone, Serialize, Deserialize)]
struct Mark {
    base: String,
    head: String,
    blob: Option<String>,
    patch: String,
    dependencies: BTreeMap<String, Option<String>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Status {
    Unreviewed,
    Unchanged,
    Direct,
    Dependency(Vec<String>),
}
#[derive(Clone)]
struct Row {
    path: String,
    status: Status,
    reviewed: Option<(String, String)>,
    dependencies_checked: bool,
    rust: Option<rust_dependencies::Report>,
}
#[derive(Default)]
pub(crate) struct Freshness {
    request: u64,
    rows: Option<Result<Vec<Row>, String>>,
    dependencies: String,
}
#[derive(Clone)]
pub(crate) enum FreshnessAct {
    Refresh,
    Mark {
        path: String,
        head: String,
        base: String,
        dependencies: Vec<String>,
    },
}
fn key(d: &PrDetails) -> String {
    format!("{}#{}", d.url, d.number)
}
fn blob(repo: &Path, rev: &str, path: &str) -> Result<Option<String>, String> {
    let out = git::run(
        Some(repo),
        &[
            "--literal-pathspecs",
            "ls-tree",
            "--format=%(objecttype) %(objectname)",
            rev,
            "--",
            path,
        ],
    )?;
    Ok(out
        .lines()
        .find_map(|l| l.strip_prefix("blob ").map(str::to_owned)))
}
fn fingerprint(
    repo: &Path,
    base: &str,
    head: &str,
    path: &str,
    dependencies: &[String],
) -> Result<Mark, String> {
    let patch = git::run(
        Some(repo),
        &[
            "--literal-pathspecs",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            base,
            head,
            "--",
            path,
        ],
    )?;
    Ok(Mark {
        base: base.into(),
        head: head.into(),
        blob: blob(repo, head, path)?,
        patch: format!("{:x}", md5::compute(patch)),
        dependencies: dependencies
            .iter()
            .map(|p| Ok((p.clone(), blob(repo, head, p)?)))
            .collect::<Result<_, String>>()?,
    })
}
fn classify(old: &Mark, now: &Mark) -> Status {
    if old.blob != now.blob || old.patch != now.patch {
        Status::Direct
    } else {
        let changed: Vec<_> = old
            .dependencies
            .iter()
            .filter(|(p, b)| now.dependencies.get(*p) != Some(*b))
            .map(|(p, _)| p.clone())
            .collect();
        if changed.is_empty() {
            Status::Unchanged
        } else {
            Status::Dependency(changed)
        }
    }
}
fn rows(repo: &Path, d: &PrDetails, f: &PrFiles) -> Result<Vec<Row>, String> {
    let marks = store::load::<Marks>(repo, "freshness", &key(d))?.unwrap_or_default();
    let mut paths: std::collections::BTreeSet<_> = f.files.iter().map(|f| f.path.clone()).collect();
    paths.extend(marks.0.keys().cloned());
    let mut indexes = BTreeMap::new();
    paths
        .into_iter()
        .map(|path| {
            let old = marks.0.get(&path);
            let mut status = match old {
                None => Status::Unreviewed,
                Some(old) => classify(
                    old,
                    &fingerprint(
                        repo,
                        &f.from,
                        &f.to,
                        &path,
                        &old.dependencies.keys().cloned().collect::<Vec<_>>(),
                    )?,
                ),
            };
            let rust = if path.ends_with(".rs") {
                if let Some(old) = old {
                    for rev in [&old.head, &f.to] {
                        if !indexes.contains_key(rev) {
                            indexes.insert(rev.clone(), if indexes.len() >= 8 { Err("More than eight receipt revisions: further Rust dependency snapshots remain unchecked".into()) } else { rust_dependencies::load(repo, rev) });
                        }
                    }
                    match (indexes.get(&old.head), indexes.get(&f.to)) {
                        (Some(Ok(before)), Some(Ok(now))) => {
                            Some(rust_dependencies::compare(before, now, &path))
                        }
                        (Some(Err(e)), _) | (_, Some(Err(e))) => Some(rust_dependencies::Report {
                            limitations: vec![e.clone()],
                            ..Default::default()
                        }),
                        _ => None,
                    }
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(rust) = &rust
                && !rust.witnesses.is_empty()
                && status == Status::Unchanged
            {
                status = Status::Dependency(rust.witnesses.clone());
            }
            Ok(Row {
                rust,
                path,
                status,
                reviewed: old.map(|m| (m.base.clone(), m.head.clone())),
                dependencies_checked: old.is_some_and(|m| !m.dependencies.is_empty()),
            })
        })
        .collect()
}
impl RepoTab {
    pub(in crate::repo) fn load_review_freshness(&mut self, ctx: &egui::Context) {
        let Some(v) = &mut self.pr_view else { return };
        let (Some(Ok(d)), Some(Ok(f))) = (&v.details, &v.files) else {
            return;
        };
        let (d, f, path, request) = (
            d.clone(),
            f.clone(),
            self.path.clone(),
            v.freshness.request + 1,
        );
        v.freshness.request = request;
        v.freshness.rows = None;
        let number = v.number;
        let pr_request = self.pr_requests;
        self.then_or(
            ctx,
            false,
            Err,
            move || rows(&path, &d, &f),
            move |tab, result, _| {
                if tab.pr_requests != pr_request {
                    return;
                }
                if let Some(v) = tab
                    .pr_view
                    .as_mut()
                    .filter(|v| v.number == number && v.freshness.request == request)
                {
                    v.freshness.rows = Some(result);
                }
            },
        );
    }
    pub(in crate::repo) fn freshness_act(&mut self, act: FreshnessAct, ctx: &egui::Context) {
        let FreshnessAct::Mark {
            path,
            head,
            base,
            dependencies,
        } = act
        else {
            self.load_review_freshness(ctx);
            return;
        };
        let Some(v) = &self.pr_view else { return };
        let (Some(Ok(d)), Some(Ok(f))) = (&v.details, &v.files) else {
            return;
        };
        if d.head_oid != head || f.to != head || f.from != base {
            return;
        }
        let (d, repo) = (d.clone(), self.path.clone());
        self.then_or(
            ctx,
            true,
            Err,
            move || {
                let mut marks =
                    store::load::<Marks>(&repo, "freshness", &key(&d))?.unwrap_or_default();
                for (role, rev) in [("base", &base), ("head", &head)] {
                    git::run(
                        Some(&repo),
                        &[
                            "update-ref",
                            &format!(
                                "refs/gitcraft/freshness-inputs/{:x}/{:x}/{role}",
                                md5::compute(key(&d)),
                                md5::compute(&path)
                            ),
                            rev,
                        ],
                    )?;
                }
                marks.0.insert(
                    path.clone(),
                    fingerprint(&repo, &base, &head, &path, &dependencies)?,
                );
                store::save(&repo, "freshness", &key(&d), &marks)
            },
            |tab, result, ctx| {
                if let Err(e) = result {
                    tab.status = Some((format!("Review receipt: {e}"), true));
                }
                tab.load_review_freshness(ctx);
            },
        );
    }
}
pub(in crate::repo) fn show(
    ui: &mut egui::Ui,
    state: &mut Freshness,
    files: &PrFiles,
    acts: &mut Vec<Action>,
) {
    egui::CollapsingHeader::new(crate::i18n::text("repo-workflows-review-label-review-freshness-619aab")).id_salt("repo-workflows-review-label-review-freshness-619aab").default_open(false).show(ui,|ui|{
        ui.label(crate::i18n::text("repo-workflows-review-label-exact-blob-and-patch-receipts-rust-syntax-anal-d8aa92"));
        ui.horizontal(|ui|{ui.label(crate::i18n::text("repo-workflows-review-label-dependency-paths-for-the-next-receipt-one-per-e53c2e"));if ui.button(crate::i18n::text("repo-workflows-review-label-refresh-receipts-d3782d")).clicked(){acts.push(Action::Freshness(FreshnessAct::Refresh));}});
        ui.text_edit_multiline(&mut state.dependencies);
        let Some(rows)=&state.rows else{ui.add(crate::spinner::Spinner::new());return};
        match rows {Err(e)=>{ui.label(crate::repo::messages::display_status(e));},Ok(rows)=>for row in rows{
            ui.horizontal_wrapped(|ui|{
                ui.monospace(&row.path);
                ui.label(match &row.status{Status::Unreviewed=>crate::i18n::text("repo-workflows-review-label-not-reviewed-dc9250").into(),Status::Unchanged=>crate::i18n::text("repo-workflows-review-label-unchanged-since-reviewed-c9f208").into(),Status::Direct=>crate::i18n::text("repo-workflows-review-label-changed-directly-blob-or-patch-b8e844").into(),Status::Dependency(paths)=>crate::i18n::message("repo-workflows-review-message-text-unchanged-dependencies-changed-paths-fab840", &[("paths", (paths.join(", ")).into())])});
                if !row.dependencies_checked && row.rust.is_none(){ui.weak(crate::i18n::text("repo-workflows-review-label-dependencies-unchecked-144460"));}
                if let Some(rust)=&row.rust {
                    ui.weak(crate::i18n::message("repo-workflows-review-message-count-one-one-unchanged-rust-symbol-inspected-da406d", &[("count", (rust.checked).into())]));
                    for witness in &rust.witnesses {ui.label(crate::i18n::message("repo-workflows-review-message-dependency-freshness-witness-25c9d7", &[("witness", (crate::repo::messages::display_status(witness)).into())]));}
                    for limitation in &rust.limitations {ui.weak(crate::i18n::message("repo-workflows-review-message-unchecked-detail-5176bf", &[("detail", (crate::repo::messages::display_status(limitation)).into())]));}
                }
                if let Some((base,head))=&row.reviewed{ui.weak(crate::i18n::message("repo-workflows-review-message-receipt-base-head-433a2f", &[("base", (crate::repo::short(base)).into()), ("head", (crate::repo::short(head)).into())]));}
                if ui.button(crate::i18n::text("repo-workflows-review-label-mark-file-reviewed-7e0e78")).clicked(){acts.push(Action::Freshness(FreshnessAct::Mark{path:row.path.clone(),base:files.from.clone(),head:files.to.clone(),dependencies:state.dependencies.lines().map(str::trim).filter(|s|!s.is_empty()).map(str::to_owned).collect()}));}
            });
        }}
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_receipts_distinguish_direct_dependency_and_unknown() -> Result<(), String> {
        let r = git::stack::test_repo::TestRepo::new("freshness")?;
        r.commit("caller.rs", "callee()\n", "caller")?;
        let old = r.commit("callee.rs", "ok\n", "callee")?;
        let base = r.tip("HEAD~2")?;
        let a = fingerprint(r.path(), &base, &old, "caller.rs", &["callee.rs".into()])?;
        let head = r.commit("callee.rs", "err\n", "behavior changed")?;
        let b = fingerprint(r.path(), &base, &head, "caller.rs", &["callee.rs".into()])?;
        assert_eq!(
            classify(&a, &b),
            Status::Dependency(vec!["callee.rs".into()])
        );
        let head = r.commit("caller.rs", "callee()?\n", "caller changed")?;
        assert_eq!(
            classify(
                &a,
                &fingerprint(r.path(), &base, &head, "caller.rs", &["callee.rs".into()])?
            ),
            Status::Direct
        );
        assert_eq!(classify(&a, &a), Status::Unchanged);
        Ok(())
    }
}
#[cfg(test)]
mod fluent_review_tests {
    use super::*;
    use crate::repo::review_workflow::presentation_tests::{click, paint};

    #[test]
    fn fluent_review_freshness_actual_receipt_repaints_without_recomputing_generations()
    -> Result<(), String> {
        let ctx = egui::Context::default();
        let files = PrFiles {
            from: "base{opaque}".into(),
            to: "head{opaque}".into(),
            files: vec![],
        };
        let mut state = Freshness {
            request: 9,
            rows: Some(Ok(vec![Row {
                path: "src/a{raw}.rs".into(),
                status: Status::Unchanged,
                reviewed: Some((files.from.clone(), files.to.clone())),
                dependencies_checked: true,
                rust: Some(rust_dependencies::Report {
                    checked: 1,
                    ..Default::default()
                }),
            }])),
            dependencies: "src/dependency{raw}.rs".into(),
        };
        let mut acts = vec![];
        for (locale, label, plural) in [
            (
                "en-US",
                "Review freshness",
                "One unchanged Rust symbol inspected",
            ),
            (
                "de",
                "Aktualität des Reviews",
                "Ein unverändertes Rust-Symbol untersucht",
            ),
        ] {
            crate::i18n::set_language(locale);
            // Expand the actual localized header (same stable ID in both languages).
            if locale == "en-US" {
                click(&ctx, label, |ui| show(ui, &mut state, &files, &mut acts))?;
            }
            let text = paint(&ctx, |ui| show(ui, &mut state, &files, &mut acts));
            assert!(
                text.contains(plural) && text.contains("src/a{raw}.rs"),
                "{text}"
            );
        }
        assert_eq!(state.request, 9);
        click(&ctx, "Datei als geprüft markieren", |ui| {
            show(ui, &mut state, &files, &mut acts)
        })?;
        assert!(
            matches!(acts.as_slice(), [Action::Freshness(FreshnessAct::Mark { path, base, head, dependencies })] if path == "src/a{raw}.rs" && base == &files.from && head == &files.to && dependencies == &["src/dependency{raw}.rs"])
        );
        Ok(())
    }
}
