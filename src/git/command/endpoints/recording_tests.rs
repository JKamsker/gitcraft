//! Recording-only checks: no Git executable, child process, filesystem repo or transport.
use super::*;

#[derive(Default)]
struct Recording {
    prefix: Vec<String>,
    calls: Vec<Vec<String>>,
    b_revision: u32,
    a_revision: u32,
    config_revision: u32,
}
impl Recording {
    fn with_prefix(prefix: &[&str]) -> Self {
        Self {
            prefix: prefix.iter().map(|s| (*s).into()).collect(),
            ..Self::default()
        }
    }
    fn read(&mut self, args: &[&str]) -> Result<String, String> {
        self.calls.push(args.iter().map(|s| (*s).into()).collect());
        // Both selected A and redirected B have origin; dropping context must
        // return a plausible but wrong endpoint, rather than a missing remote.
        let context = args.starts_with(&self.prefix.iter().map(String::as_str).collect::<Vec<_>>());
        let b = context && !self.prefix.is_empty();
        let query = if context {
            &args[self.prefix.len()..]
        } else {
            args
        };
        let repository = if b { "B" } else { "A" };
        let revision = if b { self.b_revision } else { self.a_revision };
        let override_url = b
            && self
                .prefix
                .iter()
                .any(|s| s.contains("remote.origin.pushurl="));
        match query {
            ["remote"] => Ok("origin\n".into()),
            ["remote", "get-url", "--all", "origin"] => {
                Ok(format!("https://{repository}.invalid/fetch-{revision}\n"))
            }
            ["remote", "get-url", "--push", "--all", "origin"] => Ok(if override_url {
                "https://override.invalid/push\n".into()
            } else {
                format!("https://{repository}.invalid/push-{revision}\n")
            }),
            ["symbolic-ref", "--quiet", "--short", "HEAD"] => Ok("main\n".into()),
            ["config", "--get", "branch.main.pushRemote"] => Ok(String::new()),
            ["config", "--get", "remote.pushDefault"] => Ok(String::new()),
            ["config", "--get", "branch.main.remote"] => Ok("origin\n".into()),
            ["config", "--null", "--get-regexp", pattern]
                if *pattern == r"^url\..*\.(insteadof|pushinsteadof)$" =>
            {
                Ok(format!(
                    "url.https://{repository}.invalid/.insteadof\nshort:\0"
                ))
            }
            ["config", "--null", "--get-regexp", _] => Ok(format!(
                "remote.origin.url\nhttps://{repository}.invalid/{revision}\0push.default\n{}\0",
                self.config_revision
            )),
            _ => Err(format!("unexpected metadata query: {args:?}")),
        }
    }
    fn capture(&mut self, operation: &[&str]) -> Result<RouteBinding, String> {
        let owned = self.prefix.clone();
        let args = owned
            .iter()
            .map(String::as_str)
            .chain(operation.iter().copied())
            .collect::<Vec<_>>();
        let binding = RouteBinding::capture_with(&args, &mut |q| self.read(q))?;
        // Every query, including the final snapshot, keeps exact prefix order.
        for call in &self.calls {
            assert!(call.starts_with(&self.prefix), "lost context: {call:?}");
            let query = &call[self.prefix.len()..];
            assert!(matches!(
                query[0].as_str(),
                "remote" | "symbolic-ref" | "config"
            ));
        }
        Ok(binding)
    }
}

#[test]
fn repository_b_origin_wins_over_selected_a_for_all_metadata_queries() -> Result<(), String> {
    for prefix in [
        vec!["--git-dir", "/target-B/.git"],
        vec!["--git-dir=/target-B/.git"],
        vec!["-C", "/target-B"],
        vec!["-C/target-B"],
        vec!["--work-tree", "/target-B", "--git-dir", "/target-B/.git"],
        vec!["--work-tree=/target-B", "--git-dir=/target-B/.git"],
        vec!["-C", "/parent", "-C", "target-B", "--namespace", "space"],
        vec!["--bare", "--git-dir=/target-B/.git", "--namespace=space"],
    ] {
        for operation in [
            vec!["push", "origin"],
            vec!["fetch", "origin"],
            vec!["pull", "origin"],
            vec!["push"],
            vec!["fetch"],
            vec!["pull"],
            vec!["fetch", "--all"],
        ] {
            let mut recording = Recording::with_prefix(&prefix);
            let binding = recording.capture(&operation)?;
            let direction = if operation[0] == "push" {
                "push"
            } else {
                "fetch"
            };
            assert_eq!(binding.urls(), [format!("https://B.invalid/{direction}-0")]);
            assert!(binding.configuration.contains("B.invalid"));
            assert!(!binding.configuration.contains("A.invalid"));
        }
    }
    Ok(())
}

#[test]
fn option_values_named_fetch_and_push_remain_context() -> Result<(), String> {
    for value in ["fetch", "push"] {
        for prefix in [
            vec!["-C", value],
            vec!["--git-dir", value],
            vec!["--work-tree", value],
            vec!["--namespace", value],
            vec!["-c", value],
        ] {
            let binding = Recording::with_prefix(&prefix).capture(&["push", "origin"])?;
            assert_eq!(binding.urls(), ["https://B.invalid/push-0"]);
        }
        for prefix in [
            format!("-C{value}"),
            format!("--git-dir={value}"),
            format!("--work-tree={value}"),
            format!("--namespace={value}"),
            format!("-c{value}"),
        ] {
            let binding = Recording::with_prefix(&[&prefix]).capture(&["fetch", "origin"])?;
            assert_eq!(binding.urls(), ["https://B.invalid/fetch-0"]);
        }
    }
    Ok(())
}

#[test]
fn command_config_and_literal_rewrites_use_the_same_context() -> Result<(), String> {
    for prefix in [
        vec![
            "-C",
            "/target-B",
            "-c",
            "remote.origin.pushurl=https://override.invalid/push",
        ],
        vec![
            "-C/target-B",
            "-cremote.origin.pushurl=https://override.invalid/push",
        ],
    ] {
        let binding = Recording::with_prefix(&prefix).capture(&["push", "origin"])?;
        assert_eq!(binding.urls(), ["https://override.invalid/push"]);
    }
    let binding = Recording::with_prefix(&[
        "-C",
        "/target-B",
        "-c",
        "url.https://B.invalid/.insteadOf=short:",
    ])
    .capture(&["push", "short:repo"])?;
    assert_eq!(binding.urls(), ["https://B.invalid/repo"]);
    Ok(())
}

#[test]
fn snapshots_detect_b_changes_and_ignore_unqueried_a() -> Result<(), String> {
    let mut recording = Recording::with_prefix(&["--git-dir", "/target-B/.git"]);
    let original = recording.capture(&["push", "origin"])?;
    recording.a_revision = 1;
    original.compare_snapshot(&recording.capture(&["push", "origin"])?)?;
    recording.b_revision = 1;
    assert!(
        original
            .compare_snapshot(&recording.capture(&["push", "origin"])?)
            .is_err()
    );
    // A config-only change in the same queried context invalidates the proof.
    recording.b_revision = 0;
    recording.config_revision = 1;
    let config_only = recording.capture(&["push", "origin"])?;
    assert_eq!(original.urls(), config_only.urls());
    assert!(original.compare_snapshot(&config_only).is_err());
    Ok(())
}

#[test]
fn unproven_envelopes_fail_before_any_query() {
    for args in [
        vec!["--unknown", "fetch", "push", "origin"],
        vec!["--git-dir"],
        vec!["-C"],
        vec!["--work-tree"],
        vec!["-c"],
        vec!["--namespace"],
        vec!["--config-env"],
        vec![
            "--config-env",
            "remote.origin.url=FETCH_URL",
            "push",
            "origin",
        ],
        vec!["--config-env=remote.origin.url=PUSH_URL", "fetch", "origin"],
        vec!["--paginate", "push", "origin"],
        vec!["-p", "push", "origin"],
        vec!["--super-prefix=fetch", "push", "origin"],
        vec!["\0internal", "push"],
        vec!["-C", "push"],
        vec!["--git-dir=push"],
    ] {
        let mut called = false;
        assert!(
            RouteBinding::capture_with(&args, &mut |_| {
                called = true;
                Ok(String::new())
            })
            .is_err(),
            "{args:?}"
        );
        assert!(!called, "queried an unproven envelope: {args:?}");
    }
}

#[test]
fn plain_commands_keep_selected_repository_and_legacy_missing_remote_error() -> Result<(), String> {
    let binding = Recording::default().capture(&["push", "origin"])?;
    assert_eq!(binding.urls(), ["https://A.invalid/push-0"]);
    assert!(operation_endpoints_with(&["push"], &mut |_| Ok(String::new())).is_err());
    assert!(
        operation_endpoints_with(&["status"], &mut |_| Err(
            "non-remote operation queried".into()
        ))?
        .is_empty()
    );
    Ok(())
}
