//! Inert supplied-state checks. No Git, processes, filesystem, native UI or discovery.
use super::*;
use crate::i18n::set_language;
use std::path::Path;

#[test]
fn guard_display_push_keeps_raw_guard_and_auth_receipt_classification() -> Result<(), String> {
    let reached = std::cell::Cell::new(false);
    let raw = crate::push_preflight::valid_ref(Path::new("inert"), "incomplete", &|| {
        reached.set(true);
        false
    })
    .err()
    .ok_or("expected an error")?;
    if reached.get() {
        return Err("the incomplete-ref guard must not reach Git".into());
    }
    let receipt =
        crate::push_preflight::remember_auth_endpoint("https://example.invalid/team/repo");
    let raw_receipt = receipt.clone();
    for (locale, expected) in [
        (
            "en-US",
            "Choose one complete destination ref, such as refs/heads/main.",
        ),
        (
            "de",
            "Wähle eine vollständige Zielreferenz, etwa refs/heads/main.",
        ),
    ] {
        set_language(locale);
        assert_eq!(diagnostic(&raw), expected);
        assert_eq!(
            raw,
            "Choose one complete destination ref, such as refs/heads/main."
        );
        assert!(crate::push_preflight::is_reviewed_auth_error(&receipt));
        assert_eq!(diagnostic(&receipt), raw_receipt);
        assert_eq!(crate::push_preflight::APPROVED, "\0push-preflight");
        assert_eq!(crate::push_preflight::FIX, "\0push-preflight-fix");
    }
    assert_eq!(
        crate::push_preflight::take_auth_endpoint(&receipt).as_deref(),
        Some("https://example.invalid/team/repo")
    );
    assert_eq!(
        reviewed_auth_failure(),
        "Die Authentifizierung für das geprüfte Push-Ziel ist fehlgeschlagen."
    );
    assert_eq!(receipt, raw_receipt);
    set_language("en-US");

    Ok(())
}

#[test]
fn guard_display_push_supplied_findings_and_uncertain_recovery_keep_opaque_fields() {
    use crate::push_preflight::{Finding, Kind};
    let finding = Finding {
        kind: Kind::Secret,
        commit: "0123456789abcdef".into(),
        path: "src/12345 key.rs".into(),
        detail: "Likely secret at line 12345; value withheld.".into(),
    };
    let history = "Force push would drop 12345 remote commits, 2345 by other authors. Based on the fetched target; the lease must still match.";
    let directory = "/owned/12345/EXPECT { $detail }";
    let uncertain = format!(
        "The remote repair outcome is unknown. It may still be running; its owned guard and recovery remain in {directory}. Recheck after the host finishes; no replacement or cleanup was dispatched."
    );
    for (locale, number, other) in [("en-US", "12,345", "2,345"), ("de", "12.345", "2.345")] {
        set_language(locale);
        assert!(diagnostic(&finding.detail).contains(number));
        let shown = diagnostic(history);
        assert!(shown.contains(number) && shown.contains(other));
        assert!(diagnostic(&uncertain).contains(directory));
        assert!(diagnostic("5.0 MiB Git blob; consider Git LFS.").contains("5.0 MiB"));
        assert!(
            diagnostic("Author email differs from the profile policy for host/12345/repo.")
                .contains("host/12345/repo")
        );
        assert_eq!(
            finding.detail,
            "Likely secret at line 12345; value withheld."
        );
        assert_eq!(finding.commit, "0123456789abcdef");
        assert_eq!(finding.path, "src/12345 key.rs");
        assert_eq!(finding.kind, Kind::Secret);
        assert_eq!(
            diagnostic("Likely secret at line +12; value withheld."),
            "Likely secret at line +12; value withheld."
        );
        assert_eq!(
            diagnostic("Likely secret at line text; value withheld."),
            "Likely secret at line text; value withheld."
        );
        assert!(target("/owned/12345/repo").ends_with("/owned/12345/repo"));
        assert_eq!(path("src/a\nb.rs"), "src/a\\nb.rs");
        assert_eq!(path("[sensitive path]"), "[sensitive path]");
        assert_eq!(
            target_label("Local repository: /owned/12345/repo"),
            target("/owned/12345/repo")
        );
    }
    set_language("en-US");
}

#[test]
fn guard_display_authorship_preserves_source_algorithm_and_branch_counts() -> Result<(), String> {
    use crate::authorship::{self, Source, Who, branch::Figures};
    // Actual precedence: the agent's author account takes precedence over a co-author trailer.
    let attribution = authorship::attribute(
        "Claude",
        "noreply@anthropic.com",
        "Co-authored-by: Copilot <copilot@github.com>",
        None,
        None,
    )
    .ok_or("missing test value")?;
    assert_eq!(attribution.who, Who::Agent);
    assert_eq!(attribution.source, Source::Author);
    let raw = attribution.describe();
    let figures = Figures {
        commits: 12345,
        agent: 9000,
        mixed: 1000,
        human: 1000,
        unknown: 1345,
        edited: 900,
        partial: true,
    };
    let before = figures.clone();
    let raw_headline = figures.headline("refs/heads/12345");
    for (locale, number, who) in [
        ("en-US", "12,345", "Agent-written"),
        ("de", "12.345", "Von einem Agenten geschrieben"),
    ] {
        set_language(locale);
        let description = attribution.display_description();
        assert!(description.contains(who) && description.contains("Claude"));
        let shown = figures.display_headline("refs/heads/12345");
        assert!(shown.contains(number));
        assert!(shown.contains("refs/heads/12345"));
        assert!(shown.contains("10%"));
        assert_eq!(attribution.describe(), raw);
        assert_eq!(figures.headline("refs/heads/12345"), raw_headline);
        assert_eq!(figures, before);
        assert_eq!(
            attribution.source.label(),
            "an agent's account as the author"
        );
        let error = authorship::load::one(Path::new("inert"), "-12345")
            .err()
            .ok_or("expected an error")?;
        assert_eq!(error, "-12345 is not a commit");
        assert!(authorship::display::diagnostic(&error).contains("-12345"));
    }
    for source in [
        Source::GitAi,
        Source::AgentTrace,
        Source::Author,
        Source::Trailer,
    ] {
        for who in [Who::Agent, Who::Mixed, Who::Human, Who::Unknown] {
            for has_lines in [false, true] {
                let mut a = attribution.clone();
                a.source = source;
                a.who = who;
                if has_lines {
                    a.lines.insert("source.rs".into(), vec![(1, 2, who)]);
                }
                for with_agents in [false, true] {
                    if !with_agents {
                        a.agents.clear();
                    } else {
                        a.agents = vec!["Opaque 12345 { $source }".into()];
                    }
                    for locale in ["en-US", "de"] {
                        set_language(locale);
                        assert!(!a.display_description().contains("interaction-guards-"));
                        if with_agents {
                            assert!(a.display_description().contains("Opaque 12345 { $source }"));
                        }
                        assert_eq!(a.line("source.rs", 1), who);
                    }
                }
            }
        }
    }
    set_language("en-US");

    Ok(())
}

#[test]
fn guard_display_drift_uses_supplied_report_without_changing_task_or_prompt() -> Result<(), String>
{
    use crate::drift::{self, Dependency, Folder, Report};
    let mut report = Report {
        files: vec!["src/12345.rs".into()],
        folders: vec![Folder {
            path: "src".into(),
            files: vec!["src/12345.rs".into()],
            related: false,
        }],
        dependencies: vec![Dependency {
            manifest: "Cargo.toml".into(),
            name: "Opaque 12345".into(),
        }],
        changed_lines: 12345,
        formatting_lines: 2469,
        judged: true,
        ..Default::default()
    };
    let original = report.clone();
    let prompt = drift::llm::prompt("Task { $source } 12345", &report);
    let raw = drift::measure(Path::new("inert"), "-12345", "task")
        .err()
        .ok_or("expected an error")?;
    assert_eq!(raw, "-12345 is not a revision");
    for (locale, phrase, single) in [
        ("en-US", "20% formatting only", "one folder"),
        ("de", "20% reine Formatierung", "einem Ordner"),
    ] {
        set_language(locale);
        let headline = report.display_headline();
        assert!(
            headline.contains(phrase) && headline.contains(single),
            "{headline}"
        );
        assert!(drift::display::diagnostic(&raw).contains("-12345"));
        assert_eq!(
            drift::llm::prompt("Task { $source } 12345", &report),
            prompt
        );
        assert_eq!(report, original);
        // Issue numbers are identities, unlike timeout seconds.
        let detail = "provider: EXPECT { $seconds } 12345\nunknown diagnostic";
        assert!(
            drift::display::diagnostic(&format!("Could not read issue #12345: {detail}"))
                .contains("#12345")
        );
        assert!(
            drift::display::diagnostic(&format!("Could not run codex: {detail}")).contains(detail)
        );
        let timeout = drift::display::diagnostic("codex took longer than 12345 s");
        assert!(timeout.contains(if locale == "de" { "12.345" } else { "12,345" }));
    }
    report.judged = false;
    set_language("de");
    assert!(!report.display_headline().contains("Ordner"));
    set_language("en-US");

    Ok(())
}

#[test]
fn guard_display_fence_uses_actual_pure_validation_and_preserves_pathspecs() -> Result<(), String> {
    use crate::fence::{self, Pattern};
    let validation = Pattern::check("!src/12345").ok_or("missing test value")?;
    for locale in ["en-US", "de"] {
        set_language(locale);
        let shown = fence::display::diagnostic(&validation);
        assert!(shown.contains("!src/12345"));
        assert_eq!(
            Pattern::check("!src/12345").as_deref(),
            Some(validation.as_str())
        );
        let wrapped = format!("Could not save the agent fence: {validation}");
        let displayed = fence::display::diagnostic(&wrapped);
        assert!(displayed.contains("!src/12345"));
        if locale == "de" {
            assert!(!displayed.contains("a fence takes"));
        }
        let payload = "provider { $detail } 12345: no translation";
        let wrapped = format!("Could not read the agent fence: {payload}");
        assert!(fence::display::diagnostic(&wrapped).contains(payload));
        assert_eq!(
            Pattern::parse("/src/12345")
                .ok_or("missing test value")?
                .pathspecs(),
            [":(glob,icase)src/12345", ":(glob,icase)src/12345/**"]
        );
    }
    set_language("en-US");

    Ok(())
}

#[test]
fn guard_display_collision_retains_raw_status_and_old_git_matching() -> Result<(), String> {
    use crate::collision::{self, Matrix};
    let raw = collision::simulate(Path::new("inert"), "-12345", "refs/heads/12345")
        .err()
        .ok_or("expected an error")?;
    let old_git = "The collision matrix needs git 2.38 or later (git merge-tree --write-tree)";
    let matrix = Matrix {
        with_target: vec![Err(old_git.into())],
        ..Default::default()
    };
    for locale in ["en-US", "de"] {
        set_language(locale);
        assert!(collision::display::diagnostic(&raw).contains("-12345"));
        assert_eq!(matrix.first_error(), Some(old_git));
        assert!(
            matrix
                .first_error()
                .ok_or("missing test value")?
                .contains("needs git")
        );
        assert!(collision::display::diagnostic(old_git).contains("git merge-tree --write-tree"));
        assert_eq!(collision::STOPPED, "Stopped");
        assert_eq!(
            collision::display::diagnostic(collision::STOPPED),
            if locale == "de" {
                "Angehalten"
            } else {
                "Stopped"
            }
        );
        let payload = "provider: unknown option { $a } 12345";
        let raw =
            format!("Could not merge refs/heads/12345 with refs/heads/23456 in memory: {payload}");
        let shown = collision::display::diagnostic(&raw);
        assert!(
            shown.contains("refs/heads/12345")
                && shown.contains("refs/heads/23456")
                && shown.contains(payload)
        );
    }
    set_language("en-US");

    Ok(())
}

#[test]
fn guard_display_weakened_uses_actual_scan_flags_and_preserves_source_and_numeric_lexemes()
-> Result<(), String> {
    use crate::weakened::{self, Reason};
    let patch = "diff --git a/tests/a.rs b/tests/a.rs\n--- a/tests/a.rs\n+++ b/tests/a.rs\n@@ -12345,2 +12345,2 @@\n-assert!(result);\n-assert!(other);\n+#[ignore]\n+assert!(true);\n";
    let files = weakened::scan_patch(patch);
    assert_eq!(files.len(), 1);
    let flags = &files[0];
    let before = flags.clone();
    assert!(
        flags
            .flags
            .iter()
            .any(|f| f.reason == Reason::RemovedAssertions)
    );
    assert!(flags.flags.iter().any(|f| f.reason == Reason::SkipsTests));
    for (locale, number, label) in [
        ("en-US", "12,345", "Removes assertions"),
        ("de", "12.345", "Entfernt Assertions"),
    ] {
        set_language(locale);
        let shown = flags.display_summary();
        assert!(shown.contains(label) && shown.contains(number) && shown.contains("#[ignore]"));
        assert_eq!(flags, &before);
        assert!(
            flags
                .flags
                .iter()
                .any(|f| f.evidence.contains("old line 12345"))
        );
        let patch = "diff --git a/tests/a.rs b/tests/a.rs\n--- a/tests/a.rs\n+++ b/tests/a.rs\n@@ -1,1 +1,1 @@\n-assert!(approx(a, b, epsilon=0.001));\n+assert!(approx(a, b, epsilon=0.01));\n";
        let tolerance = weakened::scan_patch(patch);
        if let Some(flag) = tolerance
            .iter()
            .flat_map(|f| &f.flags)
            .find(|f| f.reason == Reason::LoosenedTolerance)
        {
            assert!(flag.display_evidence().contains("0.001 → 0.01"));
        } else {
            return Err("real tolerance scanner must supply the evidence".into());
        }
    }
    set_language("en-US");

    Ok(())
}

#[test]
fn guard_display_unknown_diagnostics_are_opaque_in_every_family() {
    let unknown = "user/provider diagnostic: 12345 { $detail }\nEXPECT APPROVED --no-verify";
    for locale in ["en-US", "de"] {
        set_language(locale);
        for show in [
            diagnostic,
            crate::authorship::display::diagnostic,
            crate::drift::display::diagnostic,
            crate::fence::display::diagnostic,
            crate::collision::display::diagnostic,
            crate::weakened::display::diagnostic,
        ] {
            assert_eq!(show(unknown), unknown);
        }
        // Source/config text is opaque even if it contains an exact application diagnostic.
        let raw = "Could not read who wrote the commits: Stopped\nChoose a remote before pushing this branch.";
        assert!(
            crate::authorship::display::diagnostic(raw)
                .ends_with("Stopped\nChoose a remote before pushing this branch.")
        );
    }
    set_language("en-US");
}

#[test]
fn guard_display_supplied_fence_target_keeps_delimiters_and_provider_payload_whole() {
    let target = "/owned/12345 against the agent fence: user/path";
    let payload = "provider: against the agent fence: { $detail } 12345\nunknown result";
    let raw = format!("Could not check {target} against the agent fence: {payload}");
    for locale in ["en-US", "de"] {
        set_language(locale);
        let shown = crate::fence::display::check_diagnostic(&raw, target);
        assert!(shown.contains(target) && shown.ends_with(payload));
        // Unscoped text cannot prove where the target ends, so remains opaque.
        assert_eq!(crate::fence::display::diagnostic(&raw), raw);
        assert_eq!(
            crate::fence::display::check_diagnostic(&raw, "other-target"),
            raw
        );
        for unknown in [
            "provider failed: arbitrary 12345",
            "provider took longer than 12345 s",
        ] {
            assert_eq!(crate::drift::display::diagnostic(unknown), unknown);
        }
        assert_eq!(
            crate::fence::display::diagnostic("Could not read user: path: provider detail"),
            "Could not read user: path: provider detail"
        );
        let nested = format!(
            "Could not check {target} against the agent fence: there is no main branch to compare with"
        );
        let shown = crate::fence::display::check_diagnostic(&nested, target);
        assert!(shown.contains(target));
        if locale == "de" {
            assert!(!shown.contains("there is no main branch"));
        }
    }
    set_language("en-US");
}

#[test]
fn guard_display_actual_ci_deleted_file_snapshot_and_summary_limit_are_localized()
-> Result<(), String> {
    use crate::weakened::{self, Reason};
    let ci = "diff --git a/.github/workflows/ci.yml b/.github/workflows/ci.yml\n--- a/.github/workflows/ci.yml\n+++ b/.github/workflows/ci.yml\n@@ -1,3 +1,1 @@\n jobs:\n-  checks12345:\n-    run: check\n";
    let ci_files = weakened::scan_patch(ci);
    let job = ci_files
        .iter()
        .flat_map(|f| &f.flags)
        .find(|f| f.reason == Reason::CiRemovedJob)
        .ok_or("missing test value")?;
    let deleted = "diff --git a/tests/test_12345.py b/tests/test_12345.py\ndeleted file mode 100644\n--- a/tests/test_12345.py\n+++ /dev/null\n@@ -1,1 +0,0 @@\n-def test_x(): pass\n";
    let files = weakened::scan_patch(deleted);
    let snapshot = "diff --git a/output.snap b/output.snap\ndeleted file mode 100644\n--- a/output.snap\n+++ /dev/null\n@@ -1,1 +0,0 @@\n-source12345\n";
    let snapshots = weakened::scan_patch(snapshot);
    let snap = snapshots
        .iter()
        .flat_map(|f| &f.flags)
        .find(|f| f.reason == Reason::SnapshotRewrite)
        .ok_or("missing test value")?;
    let flags = vec![job.clone(); 7];
    for locale in ["en-US", "de"] {
        set_language(locale);
        let shown = job.display_evidence();
        assert!(shown.contains("checks12345"));
        assert!(files[0].display_summary().contains("tests/test_12345.py"));
        assert!(snap.display_evidence().contains("output.snap"));
        let summary = weakened::display::summary(flags.iter());
        assert_eq!(summary.lines().count(), 7); // heading, five evidence lines, remainder
        assert!(summary.contains(if locale == "de" {
            "und 2 weitere"
        } else {
            "and 2 more"
        }));
        if locale == "de" {
            assert!(shown.contains("den Job"));
        }
        assert!(job.evidence.contains("the job `checks12345`"));
    }
    set_language("en-US");

    Ok(())
}

#[test]
fn guard_display_every_catalog_contract_formats_in_both_languages_without_fallback()
-> Result<(), String> {
    use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
    use std::collections::{BTreeMap, BTreeSet};
    let mut paired = Vec::new();
    for (locale, catalog) in [
        (
            "en-US",
            include_str!("../../../locales/en-US/interaction-guards.ftl"),
        ),
        (
            "de",
            include_str!("../../../locales/de/interaction-guards.ftl"),
        ),
    ] {
        let mut contracts: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        let mut current = "";
        for line in catalog.lines() {
            if let Some((id, _)) = line.split_once(" = ") {
                assert!(id.starts_with("interaction-guards-"));
                current = id;
                assert!(contracts.insert(id, BTreeSet::new()).is_none());
            }
            for suffix in line.split('$').skip(1) {
                let name: String = suffix
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                contracts
                    .get_mut(current)
                    .ok_or("missing test value")?
                    .insert(name);
            }
        }
        let mut bundle =
            FluentBundle::new(vec![locale.parse().map_err(|error| format!("{error:?}"))?]);
        bundle.set_use_isolating(false);
        bundle
            .add_resource(
                FluentResource::try_new(catalog.to_owned())
                    .map_err(|error| format!("{error:?}"))?,
            )
            .map_err(|error| format!("{error:?}"))?;
        // Direct bundles make a broken German message fail instead of using English fallback.
        for (id, names) in &contracts {
            for (number, selector) in [(1u64, "yes"), (12345u64, "no")] {
                let mut args = FluentArgs::new();
                for name in names {
                    match name.as_str() {
                        "line" | "count" | "others" | "seconds" | "megabytes" | "percent"
                        | "agent" | "total" | "commits" | "edited" | "mixed" | "unknown"
                        | "files" | "unrelated_files" | "folders" | "dependencies"
                        | "signatures" | "removed" | "old" => args.set(name, number),
                        "has_agent" | "has_mixed" | "has_unknown" | "partial" | "unrelated" => {
                            args.set(name, selector)
                        }
                        _ => args.set(name, "opaque 12345 { $untouched }"),
                    }
                }
                let mut errors = Vec::new();
                let shown = bundle.format_pattern(
                    bundle
                        .get_message(id)
                        .ok_or("missing test value")?
                        .value()
                        .ok_or("missing test value")?,
                    Some(&args),
                    &mut errors,
                );
                assert!(errors.is_empty(), "{locale} {id}: {errors:?}");
                assert!(!shown.is_empty(), "{locale} {id}");
            }
        }
        paired.push(contracts);
    }
    assert_eq!(paired[0], paired[1]);

    Ok(())
}

#[test]
fn guard_display_target_labels_preserve_ambiguous_paths_and_canonical_redaction() {
    let literal_local = "[sensitive path]";
    let literal_ssh = "example.invalid:[sensitive path]";
    let secret_local = "/owned/ghp_abcdefghijklmnop/repo";
    let secret_ssh = "example.invalid:ghp_abcdefghijklmnop/repo";
    let stored_local = "Local repository: [sensitive path]";
    let stored_remote = "example.invalid/[sensitive path]";
    assert_eq!(
        crate::push_preflight::target_label(literal_local),
        stored_local
    );
    assert_eq!(
        crate::push_preflight::target_label(literal_ssh),
        stored_remote
    );
    assert_eq!(
        crate::push_preflight::target_label(secret_local),
        stored_local
    );
    assert_eq!(
        crate::push_preflight::target_label(secret_ssh),
        stored_remote
    );
    for (locale, local_caption) in [
        ("en-US", "Local repository: [sensitive path]"),
        ("de", "Lokales Repository: [sensitive path]"),
    ] {
        set_language(locale);
        // Stored labels have no redaction provenance. Literal and genuine redactions
        // deliberately preserve the same canonical technical marker.
        for raw in [literal_local, secret_local] {
            assert_eq!(target(raw), local_caption);
        }
        for raw in [literal_ssh, secret_ssh] {
            assert_eq!(target(raw), stored_remote);
        }
        assert_eq!(target_label(stored_local), local_caption);
        assert_eq!(target_label(stored_remote), stored_remote);
        let raw_finding =
            format!("Author email differs from the profile policy for {stored_local}.");
        assert!(diagnostic(&raw_finding).contains(local_caption));
        assert!(!target(secret_local).contains("ghp_"));
        assert!(!target(secret_ssh).contains("ghp_"));
        // Path-only projection has the actual source and can localize real redaction.
        assert_eq!(path(literal_local), literal_local);
        assert_eq!(
            path(secret_local),
            if locale == "de" {
                "[sensibler Pfad]"
            } else {
                "[sensitive path]"
            }
        );
    }
    set_language("en-US");
}

#[test]
fn captured_push_leases_render_without_changing_raw_witnesses() {
    let pairs = [
        (
            "The push lease must name this exact ref.",
            "Die Push-Lease muss genau diese Referenz benennen.",
        ),
        (
            "The push lease must name this exact ref and object.",
            "Die Push-Lease muss genau diese Referenz und dieses Objekt benennen.",
        ),
        (
            "The push contains contradictory explicit leases; reopen its original action.",
            "Der Push enthält widersprüchliche explizite Leases; öffne die ursprüngliche Aktion erneut.",
        ),
        (
            "The fetched target differs from the captured push lease; reopen its original action.",
            "Das abgerufene Ziel weicht von der erfassten Push-Lease ab; öffne die ursprüngliche Aktion erneut.",
        ),
        (
            "The captured push lease changed; reopen its original action.",
            "Die erfasste Push-Lease wurde geändert; öffne die ursprüngliche Aktion erneut.",
        ),
        (
            "The reviewed fork source changed; reopen push preflight",
            "Die geprüfte Fork-Quelle wurde geändert; öffne die Push-Vorabprüfung erneut",
        ),
    ];
    for locale in ["en-US", "de", "en-US"] {
        set_language(locale);
        for (raw, german) in pairs {
            let before = raw.to_owned();
            let expected = if locale == "de" { german } else { raw };
            assert_eq!(diagnostic(raw), expected);
            assert_eq!(crate::git::display_error(raw), expected);
            assert_eq!(raw, before);
            let unknown = format!("{raw}\nopaque provider detail: refs/heads/a:b");
            assert_eq!(diagnostic(&unknown), unknown);
            assert_eq!(crate::git::display_error(&unknown), unknown);
        }
    }
}
