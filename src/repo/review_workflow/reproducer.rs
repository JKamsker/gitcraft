//! Generated Rust tests are inert until explicitly run inside a constrained Docker container.
//! The container receives no network, host credentials, or writable host mount.
use super::{
    contracts::{self, Observation},
    echo,
    tools::ToolsAct,
};
use crate::{
    ai, git,
    repo::{Action, DiffKey, RepoTab},
    settings::Settings,
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
#[path = "reproducer_docker.rs"]
mod containers;
#[path = "reproducer_source.rs"]
mod materialize;
static IDS: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Case {
    base: String,
    head: String,
    path: String,
    line: String,
    objection: String,
    source: String,
    image: String,
    approved: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Report {
    case: Case,
    base: Observation,
    head: Observation,
    cleanup: Vec<String>,
}
#[derive(Default)]
pub(crate) struct State {
    case: Case,
    generated: Option<Case>,
    request: u64,
    busy: bool,
    cancel: ai::Cancel,
    stop: crate::tasks::Cancellation,
    error: Option<String>,
    report: Option<Report>,
}
impl State {
    fn accept(
        &mut self,
        request: u64,
        input: &Case,
        result: Result<(Option<Case>, Option<Report>), String>,
    ) {
        if request != self.request {
            return;
        }
        self.busy = false;
        if self.cancel.is_cancelled() {
            return;
        }
        match result {
            Ok((case, report)) => {
                if let Some(mut case) = case {
                    case.approved = false;
                    if &self.case == input {
                        self.case = case;
                    } else {
                        self.generated = Some(case);
                        self.error = Some("Generated result belongs to the earlier form. Your edits were preserved; inspect and explicitly use the result below if wanted.".into());
                    }
                }
                if report.is_some() {
                    self.report = report;
                }
            }
            Err(e) => self.error = Some(e),
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.stop.cancel();
    }
}
#[derive(Clone)]
pub(crate) enum Act {
    Generate(Case),
    Run(Case),
    Stop,
    Load,
    UseSelection,
    UseGenerated,
}
impl RepoTab {
    pub(in crate::repo) fn reproducer_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Stop) {
            self.review_tools.reproducer.cancel.cancel();
            self.review_tools.reproducer.stop.cancel();
            self.review_tools.reproducer.request += 1;
            self.review_tools.reproducer.busy = false;
            self.review_tools.reproducer.case.approved = false;
            self.review_tools.reproducer.error = Some("Stop requested. Bounded cleanup and partial observations are preserved; load preserved observations after the worker finishes.".into());
            return;
        }
        if matches!(act, Act::UseGenerated) {
            let s = &mut self.review_tools.reproducer;
            if let Some(mut generated) = s.generated.take() {
                generated.approved = false;
                s.case = generated;
            }
            return;
        }
        if matches!(act, Act::UseSelection) {
            let c = &mut self.review_tools.reproducer.case;
            c.approved = false;
            if let Some(v) = &self.diff {
                match &v.key {
                    DiffKey::Range {
                        from,
                        to: Some(to),
                        path,
                    } => {
                        c.base = from.clone();
                        c.head = to.clone();
                        c.path = path.clone();
                    }
                    DiffKey::Commit { hash, path } => {
                        c.base = format!("{hash}^");
                        c.head = hash.clone();
                        c.path = path.clone();
                    }
                    _ => {}
                }
            } else if let Some(v) = &self.pr_view
                && let Some(Ok(f)) = &v.files
            {
                c.base = f.from.clone();
                c.head = f.to.clone();
            }
            return;
        }
        let s = &mut self.review_tools.reproducer;
        if s.busy {
            return;
        }
        s.busy = true;
        s.request += 1;
        let request = s.request;
        let input = s.case.clone();
        s.error = None;
        s.cancel = ai::Cancel::new();
        s.stop = self.tx.cancel.child();
        let (repo, settings, cancel, stop) = (
            self.path.clone(),
            Settings::get(ctx).ai,
            s.cancel.clone(),
            s.stop.clone(),
        );
        self.then_or(ctx,true,Err,move|| crate::tasks::with_cancellation(stop.clone(), || {
      match act{
        Act::Generate(mut case)=>{
           case.base=contracts::resolve(&repo,&case.base)?;case.head=contracts::resolve(&repo,&case.head)?;
           let patch=git::run(Some(&repo),&["--literal-pathspecs","diff","--no-ext-diff","--no-textconv","--no-color",&case.base,&case.head,"--",&case.path])?;
           if patch.len()>100_000{return Err("Select a smaller file diff before generating a reproducer".into());}
           let manifest=git::run(Some(&repo),&["show",&format!("{}:Cargo.toml",case.head)])?;
           let prompt=format!("Write ONLY a self-contained Rust integration test file for tests/gitcraft_reproducer.rs, to test the concrete objection below against BOTH revisions using exactly the same test. Use existing public APIs and existing dependencies. No commands, new dependencies, filesystem/network side effects, or Markdown. It runs offline in a container. Do not claim a bug: the human judges the observations. Objection: {}\nManifest:\n{}\nDiff of {} at line {}:\n{}",case.objection,manifest,case.path,case.line,patch);
           case.source=echo::model(&repo,&settings,&prompt,&cancel)?;case.approved=false;
           if case.source.len()>64*1024{return Err("The model produced more than 64KiB; narrow the objection".into());}
           materialize::save(&repo,"reproducer-cases","latest",&case)?;Ok((Some(case),None))
        },
        Act::Run(case)=>run(&repo,case,&stop).map(|report|(None,Some(report))),
        Act::Load=>{
            let mut report = materialize::load::<Report>(&repo,"reproducer-evidence","latest")?;
            if let Some(cleanup) = materialize::load::<containers::Cleanup>(&repo,"reproducer-cleanup","latest")? && cleanup.result.starts_with("Cleanup needs attention") {
                let note = format!("Latest independent cleanup {} ({}) · {}", cleanup.name, cleanup.id.as_deref().unwrap_or("unknown ID"), cleanup.result);
                if let Some(report) = &mut report { if !report.cleanup.contains(&note) { report.cleanup.push(note); } }
                else { return Err(note); }
            }
            Ok((materialize::load::<Case>(&repo,"reproducer-cases","latest")?, report))
        },_=>Ok((None,None))
      }
   }),move |tab,result,_| { tab.review_tools.reproducer.accept(request, &input, result); });
    }
    pub(in crate::repo) fn reproducer_badge(&self, ui: &mut egui::Ui, key: &DiffKey) {
        if let Some(report) = &self.review_tools.reproducer.report
            && let DiffKey::Range {
                from,
                to: Some(to),
                path,
            } = key
            && from == &report.case.base
            && to == &report.case.head
            && path == &report.case.path
        {
            ui.group(|ui|{ui.label(crate::i18n::message("repo-workflows-review-message-reproducer-at-line-line-base-exit-base-head-ex-0e54cd", &[("line", (report.case.line.as_str()).into()), ("base", (exit_code(report.base.code)).into()), ("head", (exit_code(report.head.code)).into())]));egui::CollapsingHeader::new(crate::i18n::text("repo-workflows-review-label-base-and-head-observations-e69344")).id_salt("repo-workflows-review-label-base-and-head-observations-e69344").show(ui,|ui|show_report(ui,report));});
        }
    }
}
#[cfg(test)]
#[path = "reproducer/fluent_review_tests.rs"]
mod fluent_review_tests;
#[cfg(test)]
#[path = "reproducer/tests.rs"]
mod tests;

mod execution;
use execution::run;

mod presentation;
pub(super) use presentation::show;
use presentation::{exit_code, show_report};
