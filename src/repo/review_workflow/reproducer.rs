//! Generated Rust tests are inert until explicitly run inside a constrained Docker container.
//! The container receives no network, host credentials, or writable host mount.
use super::{
    contracts::{self, Observation},
    echo, store,
    tools::ToolsAct,
};
use crate::{
    ai, git,
    repo::{Action, DiffKey, RepoTab},
    settings::Settings,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
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
    stop: git::RunStop,
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
        self.stop.stop();
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
/// Pure recording plan, validated before any process can be started.
struct Sandbox {
    name: String,
    token: String,
    run: String,
    cleanup: String,
}
fn sandbox(
    source_dir: &str,
    image: &str,
    source: &str,
    name: &str,
    token: &str,
) -> Result<Sandbox, String> {
    if source_dir.contains([',', '\n', '\r']) {
        return Err("Docker bind paths containing commas or newlines are unsupported".into());
    }
    if image.is_empty()
        || image.starts_with('-')
        || !image
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:@-".contains(c))
    {
        return Err("Choose a locally installed Rust container image by name or digest".into());
    }
    if !name.starts_with("gitcraft-reproducer-")
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || !token.bytes().all(|b| b.is_ascii_hexdigit())
        || token.len() != 32
    {
        return Err("Invalid owned-container identity".into());
    }
    if source.is_empty() || source.len() > 64 * 1024 {
        return Err("The generated Rust integration test must be 1–65536 bytes".into());
    }
    let q = crate::remote::sh_quote;
    let encoded = base64::engine::general_purpose::STANDARD.encode(source);
    // timeout lives INSIDE the isolated PID namespace. It cannot select a host process.
    let body = "set -eu; mkdir -p /work/project /work/cargo; for cache in registry git; do if [ -d /usr/local/cargo/$cache ]; then cp -R /usr/local/cargo/$cache /work/cargo/; fi; done; cp -R /source/. /work/project/; rm -f /work/project/.git; cd /work/project; mkdir -p tests; base64 -d > tests/gitcraft_reproducer.rs; exec cargo test --offline --test gitcraft_reproducer -- --nocapture";
    let inside = format!(
        "exec timeout --signal=TERM --kill-after=5s 120s sh -c {}",
        q(body)
    );

    let words = vec![
        "docker".to_owned(),
        "run".into(),
        "--rm".into(),
        "--pull=never".into(),
        "--name".into(),
        name.into(),
        "--label".into(),
        format!("gitcraft.owner={token}"),
        "--network=none".into(),
        "--read-only".into(),
        "--user=65534:65534".into(),
        "--cap-drop=ALL".into(),
        "--security-opt=no-new-privileges".into(),
        "--pids-limit=128".into(),
        "--memory=1g".into(),
        "--cpus=2".into(),
        "--tmpfs".into(),
        "/work:rw,exec,nosuid,nodev,size=1g,mode=1777".into(),
        "--mount".into(),
        format!("type=bind,src={source_dir},dst=/source,readonly"),
        "--env".into(),
        "CARGO_HOME=/work/cargo".into(),
        "--env".into(),
        "RUSTUP_HOME=/usr/local/rustup".into(),
        "-i".into(),
        image.into(),
        "sh".into(),
        "-c".into(),
        inside,
    ];
    let run = format!(
        "printf %s {} | {}",
        q(&encoded),
        words.iter().map(|s| q(s)).collect::<Vec<_>>().join(" ")
    );
    let cleanup = format!(
        "owner=$(docker inspect --format {} {} 2>/dev/null) || exit 0; [ \"$owner\" = {} ] || exit 73; docker rm --force {}",
        q("{{ index .Config.Labels \"gitcraft.owner\" }}"),
        q(name),
        q(token),
        q(name)
    );
    Ok(Sandbox {
        name: name.into(),
        token: token.into(),
        run,
        cleanup,
    })
}
struct OwnedContainer {
    repo: PathBuf,
    plan: Sandbox,
    removed: bool,
}
impl OwnedContainer {
    fn cleanup(&mut self) -> Result<(), String> {
        let out = git::run_shell(&self.repo, &self.plan.cleanup)?;
        if out.code == 0 {
            self.removed = true;
            Ok(())
        } else {
            Err(format!(
                "Container {} (owner {}) cleanup exited {}",
                self.plan.name, self.plan.token, out.code
            ))
        }
    }
}
impl Drop for OwnedContainer {
    fn drop(&mut self) {
        if self.removed {
            return;
        }
        let (repo, script) = (self.repo.clone(), self.plan.cleanup.clone());
        // A cancelled tab cannot launch its own cleanup. Schedule exact-owner cleanup on the
        // shared pool with an independent token. The container also has a 125s internal limit.
        if let Ok(pool) = crate::tasks::pool() {
            let _ = pool.submit(
                crate::tasks::Cancellation::default(),
                crate::tasks::Priority::Interactive,
                Some(repo.clone()),
                Box::new(move || {
                    let _ = git::run_shell(&repo, &script);
                }),
            );
        }
    }
}
fn run_one(
    repo: &Path,
    rev: &str,
    case: &Case,
    stop: &git::RunStop,
) -> Result<(Observation, Option<String>), String> {
    let scratch = git::Scratch::add_empty(repo, "test", rev)?;
    scratch.checkout(rev)?;
    let token = format!(
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
    );
    let name = format!("gitcraft-reproducer-{token}");
    let plan = sandbox(
        &crate::remote::git_path(&scratch.path),
        &case.image,
        &case.source,
        &name,
        &token,
    )?;
    let mut owned = OwnedContainer {
        repo: repo.to_owned(),
        plan,
        removed: false,
    };
    let tree = git::run(Some(repo), &["rev-parse", &format!("{rev}^{{tree}}")])?
        .trim()
        .to_owned();
    let result = contracts::observe(repo, rev, &tree, &owned.plan.run, stop);
    let cleanup = owned.cleanup().err();
    let scratch_cleanup = scratch.remove().err();
    let mut observation = result?;
    // Preserve a readable command identity, rather than presenting encoded test source as a
    // user command. The complete source and immutable image input are in the same report.
    observation.command = format!(
        "Docker {} · cargo test --offline --test gitcraft_reproducer",
        case.image
    );
    Ok((observation, cleanup.or(scratch_cleanup)))
}
fn run(repo: &Path, mut case: Case, stop: &git::RunStop) -> Result<Report, String> {
    if !case.approved {
        return Err("Inspect the source and explicitly approve sandbox execution first".into());
    }
    case.base = contracts::resolve(repo, &case.base)?;
    case.head = contracts::resolve(repo, &case.head)?;
    // Validate all user-controlled plan inputs before any container/worktree side effects.
    sandbox(
        "/recording-only/source",
        &case.image,
        &case.source,
        "gitcraft-reproducer-recording",
        "0123456789abcdef0123456789abcdef",
    )?;
    let inspection = git::run_shell(
        repo,
        &format!(
            "docker image inspect --format '{{{{.Id}}}}' {}",
            crate::remote::sh_quote(&case.image)
        ),
    )?;
    let image = String::from_utf8(inspection.stdout).map_err(|e| e.to_string())?;
    let image = image.trim();
    if inspection.code != 0
        || !image
            .strip_prefix("sha256:")
            .is_some_and(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("The selected image is not available locally. Preload a Rust image before running a sandbox; gitcraft never pulls images.".into());
    }
    case.image = image.to_owned();
    for (role, rev) in [("base", &case.base), ("head", &case.head)] {
        git::run(
            Some(repo),
            &[
                "update-ref",
                &format!(
                    "refs/gitcraft/reproducer-inputs/{:x}/{role}",
                    md5::compute(format!("{}:{}:{}", case.base, case.head, case.source))
                ),
                rev,
            ],
        )?;
    }
    let (base, base_cleanup) = run_one(repo, &case.base, &case, stop)?;
    if stop.stopped() {
        return Err("Stopped after base execution; no head comparison was made".into());
    }
    let (head, head_cleanup) = run_one(repo, &case.head, &case, stop)?;
    let report = Report {
        case,
        base,
        head,
        cleanup: base_cleanup.into_iter().chain(head_cleanup).collect(),
    };
    let text = serde_json::to_string(&report).map_err(|e| e.to_string())?;
    store::save(
        repo,
        "reproducer-evidence",
        &format!("{:x}", md5::compute(text)),
        &report,
    )?;
    store::save(repo, "reproducer-evidence", "latest", &report)?;
    Ok(report)
}
impl RepoTab {
    pub(in crate::repo) fn reproducer_act(&mut self, act: Act, ctx: &egui::Context) {
        if matches!(act, Act::Stop) {
            self.review_tools.reproducer.cancel.cancel();
            self.review_tools.reproducer.stop.stop();
            self.review_tools.reproducer.request += 1;
            self.review_tools.reproducer.busy = false;
            self.review_tools.reproducer.case.approved = false;
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
        s.stop = git::RunStop::default();
        let (repo, settings, cancel, stop) = (
            self.path.clone(),
            Settings::get(ctx).ai,
            s.cancel.clone(),
            s.stop.clone(),
        );
        self.then_or(ctx,true,Err,move||{
      match act{
        Act::Generate(mut case)=>{
           case.base=contracts::resolve(&repo,&case.base)?;case.head=contracts::resolve(&repo,&case.head)?;
           let patch=git::run(Some(&repo),&["--literal-pathspecs","diff","--no-ext-diff","--no-textconv","--no-color",&case.base,&case.head,"--",&case.path])?;
           if patch.len()>100_000{return Err("Select a smaller file diff before generating a reproducer".into());}
           let manifest=git::run(Some(&repo),&["show",&format!("{}:Cargo.toml",case.head)])?;
           let prompt=format!("Write ONLY a self-contained Rust integration test file for tests/gitcraft_reproducer.rs, to test the concrete objection below against BOTH revisions using exactly the same test. Use existing public APIs and existing dependencies. No commands, new dependencies, filesystem/network side effects, or Markdown. It runs offline in a container. Do not claim a bug: the human judges the observations. Objection: {}\nManifest:\n{}\nDiff of {} at line {}:\n{}",case.objection,manifest,case.path,case.line,patch);
           case.source=echo::model(&repo,&settings,&prompt,&cancel)?;case.approved=false;
           if case.source.len()>64*1024{return Err("The model produced more than 64KiB; narrow the objection".into());}
           store::save(&repo,"reproducer-cases","latest",&case)?;Ok((Some(case),None))
        },
        Act::Run(case)=>run(&repo,case,&stop).map(|report|(None,Some(report))),
        Act::Load=>Ok((store::load::<Case>(&repo,"reproducer-cases","latest")?,store::load::<Report>(&repo,"reproducer-evidence","latest")?)),_=>Ok((None,None))
      }
   },move |tab,result,_| { tab.review_tools.reproducer.accept(request, &input, result); });
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
            ui.group(|ui|{ui.label(format!("Reproducer at line {} · base exit {:?}, head exit {:?}. Inspect observed output; correctness is your judgment.",report.case.line,report.base.code,report.head.code));egui::CollapsingHeader::new("Base and head observations").show(ui,|ui|show_report(ui,report));});
        }
    }
}
fn show_report(ui: &mut egui::Ui, r: &Report) {
    ui.label(format!(
        "Pinned {}:{} · {}..{}",
        r.case.path,
        r.case.line,
        crate::repo::short(&r.case.base),
        crate::repo::short(&r.case.head)
    ));
    ui.label("Exit-code differences can include build failures. They are observations, not an automatic defect verdict.");
    ui.columns(2, |cols| {
        cols[0].strong("Base");
        contracts::show_observation(&mut cols[0], &r.base);
        cols[1].strong("Head");
        contracts::show_observation(&mut cols[1], &r.head);
    });
    for error in &r.cleanup {
        ui.label(format!("Cleanup needs attention: {error}"));
    }
}
pub(super) fn show(ui: &mut egui::Ui, s: &mut State, acts: &mut Vec<Action>) {
    ui.heading("AI objection → concrete reproducer");
    ui.label("Start with a Rust integration test. Generation uses the AI tool you selected in Settings and only runs when invoked. Inspect or edit its source; execution is a separate explicit action.");
    ui.horizontal(|ui| {
        if ui.button("Use open diff / PR revisions").clicked() {
            acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::UseSelection)));
        }
        if ui.button("Load preserved case and observations").clicked() {
            acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::Load)));
        }
    });
    let before = s.case.clone();
    let c = &mut s.case;
    for (text, hint) in [
        (&mut c.base, "Base revision"),
        (&mut c.head, "Head revision"),
        (&mut c.path, "File path"),
        (&mut c.line, "Pinned hunk line"),
    ] {
        ui.add(egui::TextEdit::singleline(text).hint_text(hint));
    }
    ui.add(
        egui::TextEdit::multiline(&mut c.objection).hint_text("Concrete objection to reproduce"),
    );
    if ui
        .add_enabled(
            !s.busy,
            egui::Button::new("Show me: generate a test with selected AI"),
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
            .hint_text("Rust integration test source (editable)"),
    );
    if c.image.is_empty() {
        c.image = "rust:1.95.0".into();
    }
    ui.add(
        egui::TextEdit::singleline(&mut c.image).hint_text("Preloaded Rust Docker image or digest"),
    );
    ui.weak("Docker must be available on the repository's machine. Images are never pulled. The container has no network, no writable host mount, an unprivileged UID, resource limits, and a 120s internal timeout. Dependencies must already be vendored in the repository or preloaded in your image; cargo uses --offline.");
    let mut content = c.clone();
    content.approved = before.approved;
    if content != before {
        c.approved = false;
    }
    ui.checkbox(
        &mut c.approved,
        "I inspected this source and explicitly approve running it in the restricted container",
    );
    if ui
        .add_enabled(
            !s.busy && c.approved,
            egui::Button::new("Run identical test on base and head in sandbox"),
        )
        .clicked()
    {
        acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::Run(
            c.clone(),
        ))));
    }
    if s.busy {
        ui.add(crate::spinner::Spinner::new());
        if ui.button("Stop generation / sandbox run").clicked() {
            acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::Stop)));
        }
    }
    if let Some(e) = &s.error {
        ui.label(e);
    }
    if let Some(generated) = &s.generated {
        egui::CollapsingHeader::new("Generated result for the earlier form").show(ui, |ui| {
            ui.label(format!(
                "{}:{} · {} → {}",
                generated.path, generated.line, generated.base, generated.head
            ));
            ui.monospace(&generated.source);
            if ui
                .button("Use this generated case (approval required again)")
                .clicked()
            {
                acts.push(Action::ReviewTools(ToolsAct::Reproducer(Act::UseGenerated)));
            }
        });
    }
    if let Some(r) = &s.report {
        show_report(ui, r);
        if ui.button("Open pinned comparison").clicked() {
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
        assert!(!state.generated.as_ref().expect("separate result").approved);
        state.request = 2;
        state.case.approved = false;
        state.accept(1, &original, Ok((Some(generated.clone()), None)));
        assert_eq!(state.case.source, "handwritten test");
        state.cancel.cancel();
        state.accept(2, &original, Ok((Some(generated), None)));
        assert_eq!(state.case.source, "handwritten test");
        assert!(!state.case.approved);
    }
    #[test]
    fn recorded_sandbox_plan_contains_real_containment_and_exact_owner_cleanup()
    -> Result<(), String> {
        let token = "0123456789abcdef0123456789abcdef";
        let s = sandbox(
            "/tmp/owned source",
            "rust:1.95.0",
            "#[test] fn repro() {}",
            "gitcraft-reproducer-owned",
            token,
        )?;
        for required in [
            "--network=none",
            "--read-only",
            "--user=65534:65534",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--pids-limit=128",
            "--memory=1g",
            "--pull=never",
            "readonly",
            "cargo test --offline",
            "timeout --signal=TERM",
        ] {
            assert!(s.run.contains(required), "missing {required}");
        }
        assert!(s.cleanup.contains("gitcraft.owner"));
        assert!(s.cleanup.contains(token));
        assert!(s.cleanup.contains("gitcraft-reproducer-owned"));
        assert!(!s.cleanup.contains("ps "));
        assert!(!s.cleanup.contains("kill "));
        assert!(
            sandbox(
                "/tmp/a,b",
                "rust:1.95.0",
                "x",
                "gitcraft-reproducer-owned",
                token
            )
            .is_err()
        );
        assert!(
            sandbox(
                "/tmp/a",
                "--privileged",
                "x",
                "gitcraft-reproducer-owned",
                token
            )
            .is_err()
        );
        assert!(sandbox("/tmp/a", "rust:1.95.0", "x", "unowned", token).is_err());
        Ok(())
    }
}
