#![allow(clippy::unwrap_used)]
use super::{model, run};
fn workflow() -> model::Workflow {
    model::parse(
        ".github/workflows/test.yml".into(),
        r#"
name: A workflow
on:
  push:
  workflow_dispatch:
    inputs:
      greeting:
        type: string
jobs:
  build:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        os: [ubuntu, windows]
        rust: [stable, nightly]
        exclude:
          - {os: windows, rust: nightly}
        include:
          - {os: ubuntu, extra: yes}
          - {os: macos, rust: beta}
    steps: [{run: 'echo "${{ matrix.rust }}"'}]
  test:
    needs: build
    runs-on: ubuntu-latest
    strategy:
      matrix:
        package: [one, two]
    steps: [{run: 'echo test'}]
  unrelated:
    steps: [{run: 'echo must-not-run'}]
"#
        .into(),
    )
    .unwrap()
}
#[test]
fn event_job_and_exact_matrix_snapshot_preserve_needed_jobs() {
    let w = workflow();
    assert_eq!(w.events, vec!["push", "workflow_dispatch"]);
    let entries = w.matrices("build").unwrap();
    assert_eq!(entries.len(), 4);
    assert!(
        !entries
            .iter()
            .any(|v| v["os"].as_str() == Some("windows") && v["rust"].as_str() == Some("nightly"))
    );
    assert_eq!(
        entries
            .iter()
            .filter(|v| v["extra"].as_str() == Some("yes"))
            .count(),
        2
    );
    let selected = &w.matrices("test").unwrap()[1];
    let snapshot = w
        .snapshot("workflow_dispatch", "test", selected, "owned-unique")
        .unwrap();
    let snapshot = model::parse(w.file.clone(), snapshot).unwrap();
    assert!(!snapshot.jobs.contains(&"unrelated".into()));
    assert_eq!(snapshot.matrices("test").unwrap(), vec![selected.clone()]);
    assert_eq!(snapshot.matrices("build").unwrap().len(), 4);
    assert!(
        w.snapshot("pull_request", "test", selected, "nonce")
            .is_err()
    );
}
#[test]
fn unsafe_and_dynamic_workflows_fail_explicitly() {
    assert!(
        model::parse("w.yml".into(), "on: push\njobs: {a: &a {needs: *a}}".into())
            .unwrap_err()
            .contains("aliases")
    );
    let w = model::parse(
        "w.yml".into(),
        "on: push\njobs: {a: {strategy: {matrix: '${{ fromJSON(needs.x.outputs.matrix) }}'}}}"
            .into(),
    )
    .unwrap();
    assert!(w.matrices("a").unwrap_err().contains("Expression"));
    let w = model::parse(
        "w.yml".into(),
        "on: push\njobs: {a: {needs: b}, b: {needs: a}}".into(),
    )
    .unwrap();
    assert!(w.dependencies("a").unwrap_err().contains("Cyclic"));
    assert!(model::parse("w.yml".into(), "[".repeat(100)).is_err());
}
fn selection() -> run::Selection {
    let w = workflow();
    let matrix = w.matrices("test").unwrap()[1].clone();
    run::Selection {
        workflow: w,
        context: super::context::Context::capture(
            "fork",
            &[crate::git::Remote {
                name: "fork".into(),
                url: "https://github.com/example/fork.git".into(),
            }],
        )
        .unwrap(),
        event: "push".into(),
        job: "test".into(),
        matrix,
        payload: r#"{"inputs":{"greeting":"hello"}}"#.into(),
        platforms: "ubuntu-latest=trusted/image:version".into(),
        architecture: "linux/amd64".into(),
        minutes: 5,
    }
}
#[test]
fn recording_command_cannot_run_other_workflows_or_lend_ambient_secrets() {
    let s = selection();
    let private = std::env::temp_dir().join("owned private");
    let args =
        run::arguments(&std::env::temp_dir().join("repo with spaces"), &private, &s).unwrap();
    assert_eq!(args[0], "push");
    assert!(args.windows(2).any(|a| a == ["--remote-name", "fork"]));
    assert!(args.windows(2).any(|a| a == ["--job", "test"]));
    assert!(args.windows(2).any(|a| a
        == [
            "--workflows",
            private.join("workflow.yml").to_string_lossy().as_ref()
        ]));
    for flag in ["--env-file", "--secret-file", "--input-file", "--var-file"] {
        assert!(
            args.windows(2)
                .any(|a| a == [flag, private.join("empty").to_string_lossy().as_ref()])
        );
    }
    assert!(args.contains(&"--container-daemon-socket=-".into()));
    assert!(
        !args
            .iter()
            .any(|a| a == "--secret" || a == "--matrix" || a == "--bind")
    );
    let mut s = selection();
    s.platforms = "ubuntu-latest=-self-hosted".into();
    assert!(
        run::arguments(
            std::path::Path::new("/repo"),
            std::path::Path::new("/own"),
            &s
        )
        .is_err()
    );
    s.platforms = "ubuntu-latest=image".into();
    s.payload = "[]".into();
    assert!(
        run::arguments(
            std::path::Path::new("/repo"),
            std::path::Path::new("/own"),
            &s
        )
        .is_err()
    );
}
#[test]
fn unicode_output_is_bounded_without_panics() {
    let mut log = run::Log::default();
    log.append("é".repeat(2 << 20).as_bytes());
    assert!(log.truncated);
    assert_eq!(log.text.len(), 2 << 20);
}
#[test]
fn local_action_is_available_without_a_forge_and_output_is_reachable() {
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let mut tab = super::RepoTab::new_deferred(
        std::path::PathBuf::from("/nonexistent-gitcraft-local-ci-test"),
        &ctx,
    );
    assert!(tab.hosting.is_none());
    assert!(
        tab.palette_items(&ctx)
            .iter()
            .any(|item| item.label == "Run Actions locally…")
    );
    tab.apply(super::Action::LocalCi(super::Act::Open), &ctx);
    assert!(tab.ci_view.is_some());
    tab.local_ci.provenance = Some("LOCAL · own worktree".into());
    tab.local_ci
        .log
        .lock()
        .unwrap()
        .append(b"test local output");
    let out = ctx.run_ui(egui::RawInput::default(), |ui| {
        tab.ci_view_ui(ui, &mut Vec::new());
    });
    let text = out
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Text(t) = &s.shape {
                Some(t.galley.text())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Local Actions"), "{text}");
    assert!(text.contains("test local output"), "{text}");
    assert!(text.contains("LOCAL · own worktree"), "{text}");
    out.drop_without_applying_deltas();
}

#[test]
fn include_only_entries_remain_distinct() {
    let w = model::parse(
        "w.yml".into(),
        "on: push\njobs: {a: {strategy: {matrix: {include: [{os: linux}, {os: macos}]}}}}".into(),
    )
    .unwrap();
    assert_eq!(w.matrices("a").unwrap().len(), 2);
}
#[test]
fn switching_profile_cancels_pending_and_rejects_its_answer() {
    let mut state = super::State::default();
    let (tx, reply) = std::sync::mpsc::channel();
    let scope = crate::tasks::Scope::default();
    let token = scope.0.clone();
    let mut context = crate::connections::current();
    context.error = Some("different-account".into());
    state.pending = Some(super::Pending {
        reply,
        scope,
        context,
    });
    tx.send(super::Reply::Finished(Ok(
        "old success must not appear".into()
    )))
    .unwrap();
    state.poll(&egui::Context::default());
    assert!(token.is_cancelled());
    assert!(state.pending.is_none());
    assert!(
        state
            .result
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap_err()
            .contains("Profile changed")
    );
}
#[cfg(unix)]
#[test]
#[ignore = "owned fake-tool process gate; run only after preserving and verifying this unit"]
fn owned_fake_act_records_selection_and_stale_source_refuses_execution() {
    use std::os::unix::fs::PermissionsExt;
    let root = run::Scratch::new().unwrap();
    let path = root.0.join("worktree with spaces");
    std::fs::create_dir_all(path.join(".github/workflows")).unwrap();
    let s = selection();
    std::fs::write(path.join(&s.workflow.file), &s.workflow.source).unwrap();
    // Existing ignored owned-child gate now also needs a literal configured remote.
    // No init child is needed to prepare this private minimal repository.
    std::fs::create_dir_all(path.join(".git/refs/heads")).unwrap();
    std::fs::create_dir_all(path.join(".git/objects")).unwrap();
    std::fs::write(path.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(
        path.join(".git/config"),
        r#"[core]
 repositoryformatversion = 0
[remote "fork"]
 url = https://github.com/example/fork.git
"#,
    )
    .unwrap();
    let fake = root.0.join("recording act");
    std::fs::write(
        &fake,
        r#"#!/bin/sh
printf '%s\n' "$@"
printf 'recorded stderr\n' >&2
"#,
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
    let log = std::sync::Arc::new(std::sync::Mutex::new(run::Log::default()));
    assert!(
        run::execute_with(
            path.clone(),
            s,
            std::sync::Arc::clone(&log),
            egui::Context::default(),
            fake.as_os_str()
        )
        .is_ok()
    );
    let text = log.lock().unwrap().text.clone();
    assert!(text.contains("--job\ntest"));
    assert!(text.contains("--eventpath"));
    assert!(text.contains("recorded stderr"));
    std::fs::write(
        path.join(".github/workflows/test.yml"),
        "changed after selection",
    )
    .unwrap();
    let error = run::execute_with(
        path,
        selection(),
        log,
        egui::Context::default(),
        fake.as_os_str(),
    )
    .unwrap_err();
    assert!(error.contains("changed since selection"));
}

#[test]
fn matrix_extra_include_rows_are_not_mutated_by_later_includes() {
    let source = "on: push\njobs: {a: {strategy: {matrix: {os: [linux, windows], exclude: [{os: linux}], include: [{os: linux, foo: one}, {bar: two}]}}}}";
    let w = model::parse("w.yml".into(), source.into()).unwrap();
    let entries = w.matrices("a").unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["os"].as_str(), Some("windows"));
    assert_eq!(entries[0]["bar"].as_str(), Some("two"));
    assert_eq!(entries[1]["os"].as_str(), Some("linux"));
    assert_eq!(entries[1]["foo"].as_str(), Some("one"));
    assert!(matches!(entries[1]["bar"], yaml_rust2::Yaml::BadValue));
    let selected = model::parse(
        "w.yml".into(),
        w.snapshot("push", "a", &entries[1], "extra").unwrap(),
    )
    .unwrap();
    assert_eq!(selected.matrices("a").unwrap(), vec![entries[1].clone()]);
    let w = model::parse("w.yml".into(), "on: push\njobs: {a: {strategy: {matrix: {fruit: [apple, pear], animal: [cat, dog], include: [{color: green}, {color: pink, animal: cat}, {fruit: apple, shape: circle}, {fruit: banana}, {fruit: banana, animal: cat}]}}}}".into()).unwrap();
    let entries = w.matrices("a").unwrap();
    assert_eq!(entries.len(), 6);
    assert_eq!(entries[0]["color"].as_str(), Some("pink"));
    assert_eq!(entries[1]["color"].as_str(), Some("green"));
    assert_eq!(entries[4]["fruit"].as_str(), Some("banana"));
    assert!(matches!(entries[4]["animal"], yaml_rust2::Yaml::BadValue));
    assert_eq!(entries[5]["animal"].as_str(), Some("cat"));
}

// Source grouping retains the established qualified test names.
include!("tests/context.rs");
