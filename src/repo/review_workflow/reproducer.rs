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
fn token() -> String {
    format!(
        "{:x}",
        md5::compute(format!(
            "{}:{}:{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            IDS.fetch_add(1, Ordering::Relaxed)
        ))
    )
}
fn run(repo: &Path, mut case: Case, stop: &crate::tasks::Cancellation) -> Result<Report, String> {
    if !case.approved {
        return Err("Inspect the source and explicitly approve sandbox execution first".into());
    }
    if case.source.is_empty() || case.source.len() > 64 * 1024 {
        return Err("The generated integration test must be 1–65536 bytes".into());
    }
    case.base = materialize::resolve(repo, &case.base)?;
    case.head = materialize::resolve(repo, &case.head)?;
    case.image = containers::image(repo, &case.image)?;
    for rev in [&case.base, &case.head] {
        materialize::text(
            repo,
            &[
                "update-ref",
                &format!("refs/gitcraft/reproducer-inputs/{rev}"),
                rev,
            ],
        )?;
    }
    let base_tree = materialize::text(repo, &["rev-parse", &format!("{}^{{tree}}", case.base)])?
        .trim()
        .to_owned();
    let head_tree = materialize::text(repo, &["rev-parse", &format!("{}^{{tree}}", case.head)])?
        .trim()
        .to_owned();
    let (base, base_cleanup) = containers::run_one(repo, &case.base, &base_tree, &case, &token())?;
    let mut cleanup = vec![base_cleanup];
    let head = if stop.is_cancelled() {
        Observation {
            commit: case.head.clone(),
            tree: head_tree.clone(),
            command: "No head run after cancellation".into(),
            code: None,
            stopped: true,
            lines: vec!["Stopped after base; no head comparison was made".into()],
            machine: base.machine.clone(),
        }
    } else {
        match containers::run_one(repo, &case.head, &head_tree, &case, &token()) {
            Ok((head, evidence)) => {
                cleanup.push(evidence);
                head
            }
            Err(error) => Observation {
                commit: case.head.clone(),
                tree: head_tree,
                command: "Head source preparation did not complete".into(),
                code: None,
                stopped: true,
                lines: vec![error],
                machine: base.machine.clone(),
            },
        }
    };
    let report = Report {
        case,
        base,
        head,
        cleanup: cleanup
            .into_iter()
            .filter(|e| e.result.starts_with("Cleanup needs attention"))
            .map(|e| {
                format!(
                    "{} ({}) · {}",
                    e.name,
                    e.id.as_deref().unwrap_or("unknown ID"),
                    e.result
                )
            })
            .collect(),
    };
    // Stop cancels execution, not recording its partial observations and bounded cleanup.
    crate::tasks::with_cancellation(crate::tasks::Cancellation::default(), || {
        let text = serde_json::to_string(&report).map_err(|e| e.to_string())?;
        materialize::save(
            repo,
            "reproducer-evidence",
            &format!("{:x}", md5::compute(text)),
            &report,
        )?;
        materialize::save(repo, "reproducer-evidence", "latest", &report)
    })?;
    Ok(report)
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
fn exit_code(code: Option<i32>) -> String {
    code.map(crate::i18n::number).unwrap_or_else(|| {
        crate::i18n::text("repo-workflows-review-label-unknown-b23a6a").to_owned()
    })
}
fn show_report(ui: &mut egui::Ui, r: &Report) {
    ui.label(crate::i18n::message(
        "repo-workflows-review-message-pinned-path-line-base-head-b813c6",
        &[
            ("path", (r.case.path.as_str()).into()),
            ("line", (r.case.line.as_str()).into()),
            ("base", (crate::repo::short(&r.case.base)).into()),
            ("head", (crate::repo::short(&r.case.head)).into()),
        ],
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-exit-code-differences-can-include-build-failur-324428",
    ));
    ui.columns(2, |cols| {
        cols[0].strong(crate::i18n::text("repo-workflows-review-label-base-7b4736"));
        contracts::show_observation(&mut cols[0], &r.base);
        cols[1].strong(crate::i18n::text("repo-workflows-review-label-head-9bb0aa"));
        contracts::show_observation(&mut cols[1], &r.head);
    });
    for error in &r.cleanup {
        ui.label(crate::i18n::message(
            "repo-workflows-review-message-cleanup-needs-attention-detail-3403ce",
            &[(
                "detail",
                (crate::repo::messages::display_status(error)).into(),
            )],
        ));
    }
}
pub(super) fn show(ui: &mut egui::Ui, s: &mut State, acts: &mut Vec<Action>) {
    ui.heading(crate::i18n::text(
        "repo-workflows-review-label-ai-objection-concrete-reproducer-55420a",
    ));
    ui.label(crate::i18n::text(
        "repo-workflows-review-label-start-with-a-rust-integration-test-generation-6cf14f",
    ));
    ui.horizontal(|ui| {
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-use-open-diff-pr-revisions-9fc2d9",
            ))
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::UseSelection)));
        }
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-load-preserved-case-and-observations-2177f5",
            ))
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::Load)));
        }
    });
    let before = s.case.clone();
    let c = &mut s.case;
    for (text, hint) in [
        (
            &mut c.base,
            crate::i18n::text("repo-workflows-review-label-base-revision-7438b8"),
        ),
        (
            &mut c.head,
            crate::i18n::text("repo-workflows-review-label-head-revision-5887ea"),
        ),
        (
            &mut c.path,
            crate::i18n::text("repo-workflows-review-label-file-path-2fb6d3"),
        ),
        (
            &mut c.line,
            crate::i18n::text("repo-workflows-review-label-pinned-hunk-line-ba3843"),
        ),
    ] {
        ui.add(egui::TextEdit::singleline(text).hint_text(hint));
    }
    ui.add(
        egui::TextEdit::multiline(&mut c.objection).hint_text(crate::i18n::text(
            "repo-workflows-review-label-concrete-objection-to-reproduce-f74adb",
        )),
    );
    if ui
        .add_enabled(
            !s.busy,
            egui::Button::new(crate::i18n::text(
                "repo-workflows-review-label-show-me-generate-a-test-with-selected-ai-dd497a",
            )),
        )
        .clicked()
    {
        acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::Generate(
            c.clone(),
        ))));
    }
    ui.add(
        egui::TextEdit::multiline(&mut c.source)
            .code_editor()
            .desired_rows(12)
            .desired_width(f32::INFINITY)
            .hint_text(crate::i18n::text(
                "repo-workflows-review-label-rust-integration-test-source-editable-adba55",
            )),
    );
    if c.image.is_empty() {
        c.image = "rust:1.95.0".into();
    }
    ui.add(
        egui::TextEdit::singleline(&mut c.image).hint_text(crate::i18n::text(
            "repo-workflows-review-label-preloaded-rust-docker-image-or-digest-70313d",
        )),
    );
    ui.weak(crate::i18n::text(
        "repo-workflows-review-label-source-is-streamed-from-raw-git-trees-blobs-wi-96131a",
    ));
    ui.weak(crate::i18n::text(
        "repo-workflows-review-label-docker-must-be-available-on-the-repository-s-m-286675",
    ));
    let mut content = c.clone();
    content.approved = before.approved;
    if content != before {
        c.approved = false;
    }
    ui.checkbox(
        &mut c.approved,
        crate::i18n::text(
            "repo-workflows-review-label-i-inspected-this-source-and-explicitly-approve-d28ee6",
        ),
    );
    if ui
        .add_enabled(
            !s.busy && c.approved,
            egui::Button::new(crate::i18n::text(
                "repo-workflows-review-label-run-identical-test-on-base-and-head-in-sandbox-813afc",
            )),
        )
        .clicked()
    {
        acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::Run(
            c.clone(),
        ))));
    }
    if s.busy {
        ui.add(crate::spinner::Spinner::new());
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-stop-generation-sandbox-run-350bbf",
            ))
            .clicked()
        {
            acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::Stop)));
        }
    }
    if let Some(e) = &s.error {
        ui.label(crate::repo::messages::display_status(e));
    }
    if let Some(generated) = &s.generated {
        egui::CollapsingHeader::new(crate::i18n::text("repo-workflows-review-label-generated-result-for-the-earlier-form-ebc96e")).id_salt("repo-workflows-review-label-generated-result-for-the-earlier-form-ebc96e").show(ui, |ui| {
            ui.label(format!(
                "{}:{} · {} → {}",
                generated.path, generated.line, generated.base, generated.head
            ));
            ui.monospace(&generated.source);
            if ui
                .button(crate::i18n::text("repo-workflows-review-label-use-this-generated-case-approval-required-agai-ee18e0"))
                .clicked()
            {
                acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::UseGenerated)));
            }
        });
    }
    if let Some(r) = &s.report {
        show_report(ui, r);
        if ui
            .button(crate::i18n::text(
                "repo-workflows-review-label-open-pinned-comparison-85160d",
            ))
            .clicked()
        {
            acts.push(Action::OpenDiff(DiffKey::Range {
                from: r.case.base.clone(),
                to: Some(r.case.head.clone()),
                path: r.case.path.clone(),
            }));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "R2 runtime hold: requires Docker and an explicitly preloaded Rust image; authored, not executed"]
    fn r2_runtime_same_integration_test_observes_base_failure_head_success_without_host_checkout()
    -> Result<(), String> {
        let image = std::env::var("GITCRAFT_REPRODUCER_TEST_IMAGE").map_err(
            |_| "Set GITCRAFT_REPRODUCER_TEST_IMAGE to an explicitly preloaded Rust image",
        )?;
        let r = git::stack::test_repo::TestRepo::new("reproducer-contained-observations")?;
        r.commit(
            "Cargo.toml",
            r#"[package]
name="fixture"
version="0.1.0"
edition="2021"
"#,
            "manifest",
        )?;
        let base = r.commit(
            "src/lib.rs",
            "pub fn behavior() -> bool { false }\n",
            "base",
        )?;
        let head = r.commit("src/lib.rs", "pub fn behavior() -> bool { true }\n", "head")?;
        let original = std::fs::read(r.path().join("src/lib.rs")).map_err(|e| e.to_string())?;
        let report = run(
            r.path(),
            Case {
                base,
                head: head.clone(),
                path: "src/lib.rs".into(),
                source: "#[test] fn concrete() { assert!(fixture::behavior()); }".into(),
                image,
                approved: true,
                ..Default::default()
            },
            &crate::tasks::Cancellation::default(),
        )?;
        assert_eq!(report.base.code, Some(101));
        assert_eq!(report.head.code, Some(0));
        assert!(report.cleanup.is_empty(), "{:?}", report.cleanup);
        assert_eq!(r.tip("HEAD")?, head);
        assert_eq!(
            std::fs::read(r.path().join("src/lib.rs")).map_err(|e| e.to_string())?,
            original
        );
        Ok(())
    }
    #[test]
    fn r2_pure_delayed_ai_completion_preserves_edited_form_and_cancelled_approval() {
        let original = Case {
            objection: "old objection".into(),
            source: "old test".into(),
            ..Default::default()
        };
        let generated = Case {
            source: "model test".into(),
            approved: true,
            ..original.clone()
        };
        let mut state = State {
            case: original.clone(),
            request: 1,
            busy: true,
            generated: None,
            cancel: Default::default(),
            stop: Default::default(),
            error: None,
            report: None,
        };
        state.case.objection = "edited objection".into();
        state.case.source = "handwritten test".into();
        state.accept(1, &original, Ok((Some(generated.clone()), None)));
        assert_eq!(state.case.source, "handwritten test");
        assert_eq!(state.case.objection, "edited objection");
        assert!(
            state
                .generated
                .as_ref()
                .is_some_and(|result| !result.approved)
        );
        state.request = 2;
        state.case.approved = false;
        state.accept(1, &original, Ok((Some(generated.clone()), None)));
        assert_eq!(state.case.source, "handwritten test");
        state.cancel.cancel();
        state.accept(2, &original, Ok((Some(generated), None)));
        assert_eq!(state.case.source, "handwritten test");
        assert!(!state.case.approved);
    }
}
#[cfg(test)]
mod fluent_review_tests {
    use super::*;
    use crate::repo::review_workflow::presentation_tests::{click, paint};

    #[test]
    fn fluent_review_reproducer_actual_form_has_no_execution_or_approval_side_effects()
    -> Result<(), String> {
        let ctx = egui::Context::default();
        let mut state = State::default();
        state.request = 13;
        state.case = Case {
            base: "refs/heads/base".into(),
            head: "refs/heads/head".into(),
            path: "src/{opaque}.rs".into(),
            line: "1234".into(),
            objection: "User objection {raw}".into(),
            source: "#[test] fn opaque() { assert!(true); }".into(),
            image: "rust:1.95.0".into(),
            approved: false,
        };
        let before = state.case.clone();
        let mut acts = vec![];
        for (locale, heading, action) in [
            (
                "en-US",
                "AI objection → concrete reproducer",
                "Run identical test on base and head in sandbox",
            ),
            (
                "de",
                "KI-Einwand → konkreter Reproduktionsfall",
                "Identischen Test auf Basis und Head in der Sandbox ausführen",
            ),
        ] {
            crate::i18n::set_language(locale);
            let text = paint(&ctx, |ui| show(ui, &mut state, &mut acts));
            assert!(text.contains(heading) && text.contains(action), "{text}");
            assert!(
                text.contains("refs/heads/head") && text.contains("src/{opaque}.rs"),
                "{text}"
            );
        }
        assert!(state.case == before && !state.busy && !state.case.approved);
        assert_eq!(state.request, 13);
        assert!(acts.is_empty());
        click(
            &ctx,
            "Identischen Test auf Basis und Head in der Sandbox ausführen",
            |ui| show(ui, &mut state, &mut acts),
        )?;
        assert!(
            acts.is_empty(),
            "unapproved sandbox button must stay disabled"
        );
        click(
            &ctx,
            "Zeig es mir: Test mit ausgewählter KI generieren",
            |ui| show(ui, &mut state, &mut acts),
        )?;
        assert!(
            matches!(acts.as_slice(), [Action::ReviewTools(ToolsAct::Reproducer(Act::Generate(case)))] if case == &before)
        );
        Ok(())
    }
}
