// Observation. Included to preserve libtest shard names.
struct Recording {
    calls: Vec<Vec<String>>,
    enabled: bool,
    before: String,
    after: String,
    missing_head: bool,
}

impl Probe for Recording {
    fn git(&mut self, args: &[&str]) -> Result<String, String> {
        self.calls.push(args.iter().map(|s| (*s).into()).collect());
        match args {
            ["fetch", "--all", "--prune", "--quiet"] => Ok(String::new()),
            ["remote"] => Ok("fork\norigin\n".into()),
            ["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"] => {
                if self.missing_head {
                    Err("no local remote HEAD".into())
                } else {
                    Ok("refs/remotes/origin/main\n".into())
                }
            }
            ["ls-remote", "--symref", "--", "origin", "HEAD"] => {
                Ok("ref: refs/heads/main\tHEAD\n".into())
            }
            [
                "rev-parse",
                "--verify",
                "--end-of-options",
                "refs/remotes/origin/main^{commit}",
            ] => Ok(self.after.clone()),
            ["rev-list", "--count", range, "--"]
                if *range == format!("{}..{}", self.before, self.after) =>
            {
                Ok("7\n".into())
            }
            _ => Err(format!("unexpected recording argv {args:?}")),
        }
    }
    fn hosting(&mut self) -> Option<forge::Hosting> {
        None
    }
    fn inbox(&mut self, _: &forge::Hosting) -> Result<Vec<InboxPr>, String> {
        Err("not called".into())
    }
    fn ci_failed(&mut self, _: &forge::Hosting, _: &str) -> Result<Option<bool>, String> {
        Err("not called".into())
    }
    fn may_run(&self) -> bool {
        self.enabled
    }
}

#[test]
fn actual_observation_orchestration_records_quiet_fetch_validated_revisions_and_exact_count()
-> Result<(), String> {
    let before = snapshot('a', false, Checks::Passing);
    let mut probe = Recording {
        calls: Vec::new(),
        enabled: true,
        before: "a".repeat(40),
        after: "b".repeat(40),
        missing_head: false,
    };
    let observed = observe(&mut probe, Some(&before))?;
    assert_eq!(observed.advanced, Some(7));
    assert_eq!(probe.calls.len(), 5);
    assert_eq!(probe.calls[0], ["fetch", "--all", "--prune", "--quiet"]);
    probe.enabled = false;
    probe.calls.clear();
    assert!(observe(&mut probe, Some(&before)).is_err());
    assert!(probe.calls.is_empty());
    probe.enabled = true;
    probe.missing_head = true;
    assert_eq!(observe(&mut probe, Some(&before))?.advanced, Some(7));
    assert!(
        probe
            .calls
            .iter()
            .any(|args| args == &["ls-remote", "--symref", "--", "origin", "HEAD"])
    );
    probe.calls.clear();
    probe.after = "--bad-object".into();
    assert!(observe(&mut probe, Some(&before))?.default.is_none());
    assert!(
        !probe
            .calls
            .iter()
            .any(|c| c.first().is_some_and(|s| s == "rev-list"))
    );
    Ok(())
}
