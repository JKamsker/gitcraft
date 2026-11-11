// Transfer. Included to preserve libtest shard names.
/// Here to here: every kind of change arrives as it was, and the source is as it was.
#[test]
fn work_arrives_exactly_and_the_source_is_untouched() -> Result<(), String> {
    let _one = serial();
    let root = scratch("local")?;
    let (src, target) = source_with_work(&root)?;
    let before = Untouched::of(&src)?;
    let plan = plan_to(&src, &target, &Machine::Here);
    let landed = run(&plan, &job())?;
    assert_eq!(landed.folder, plan.folder);
    assert!(landed.size > 0);
    assert!(landed.notes.is_empty(), "{:?}", landed.notes);
    assert_arrived(&src, &landed.folder)?;
    before.check()?;
    no_bundle_left();
    // The new branch follows what the remote of the same project calls it, if anything.
    assert!(git(&target, &["for-each-ref", "refs/gitcraft"])?.is_empty());
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// Teleport uses the same local excludes when the selected folder is inside the target.
#[test]
fn an_internal_teleport_worktree_is_ignored() -> Result<(), String> {
    let _one = serial();
    let root = scratch("internal-worktree")?;
    let (src, target) = source_with_work(&root)?;
    // The target can already have changes, including CRLF from Windows' clone defaults.
    // Verify those stay visible while the new nested checkout is ignored.
    write(&target.join("existing-note.txt"), b"keep this visible\n")?;
    let target_before = state_of(&target)?;
    let mut plan = plan_to(&src, &target, &Machine::Here);
    plan.folder = target.join(".worktrees/feature/x");
    let landed = run(&plan, &job())?;
    assert_arrived(&src, &landed.folder)?;
    assert!(
        git(&target, &["check-ignore", "--", ".worktrees/feature/x/"])?
            .contains(".worktrees/feature/x/")
    );
    assert_eq!(state_of(&target)?, target_before);
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// An overriding .gitignore rule stops Teleport before it creates or moves a branch.
#[test]
fn an_internal_teleport_worktree_checks_ignore_overrides() -> Result<(), String> {
    let _one = serial();
    let root = scratch("internal-worktree-negation")?;
    let (src, target) = source_with_work(&root)?;
    write(&target.join(".gitignore"), b"!.worktrees/feature/x/\n")?;
    let mut plan = plan_to(&src, &target, &Machine::Here);
    plan.folder = target.join(".worktrees/feature/x");
    let error = run(&plan, &job())
        .err()
        .map(String::from)
        .ok_or("Expected an ignore error")?;
    assert!(error.contains(".gitignore"), "{error}");
    assert!(!plan.folder.join(".git").exists());
    assert!(git(&target, &["show-ref", "--verify", "refs/heads/feature/x"]).is_err());
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// To another machine and back from one, through its shells: the same, exactly.
#[cfg(unix)]
#[test]
fn work_arrives_exactly_through_a_shell_both_ways() -> Result<(), String> {
    let _one = serial();
    let loopback = Machine::There(crate::remote::Host::Ssh(crate::remote::LOOPBACK.to_owned()));
    // Here → there, with a file large enough to go in several pieces, which no compression
    // makes smaller.
    let root = scratch("to-there")?;
    let (src, target) = source_with_work(&root)?;
    write(
        &src.join("untracked dir/large.bin"),
        &noise(protocol::CHUNK * 2 + 12345),
    )?;
    let before = Untouched::of(&src)?;
    let plan = plan_to(&src, &target, &loopback);
    let landed = run(&plan, &job())?;
    assert!(
        landed.size > (protocol::CHUNK * 2) as u64,
        "{}",
        landed.size
    );
    assert!(crate::remote::is_remote(&landed.folder));
    assert_arrived(&src, &landed.folder)?;
    before.check()?;
    no_bundle_left();
    let _ = std::fs::remove_dir_all(&root);

    // There → here.
    let root = scratch("from-there")?;
    let (src, target) = source_with_work(&root)?;
    let before = Untouched::of(&src)?;
    let mut plan = plan_to(&src, &target, &Machine::Here);
    plan.source = loopback.path(&src.to_string_lossy());
    let landed = run(&plan, &job())?;
    assert!(!crate::remote::is_remote(&landed.folder));
    assert_arrived(&src, &landed.folder)?;
    before.check()?;
    no_bundle_left();
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// No clone there: a repository is made from the bundle, with the source's remotes, the work in
/// its own working tree.
#[cfg(unix)]
#[test]
fn a_repository_is_made_from_the_bundle() -> Result<(), String> {
    let _one = serial();
    let loopback = Machine::There(crate::remote::Host::Ssh(crate::remote::LOOPBACK.to_owned()));
    let root = scratch("create")?;
    let (src, _) = source_with_work(&root)?;
    let before = Untouched::of(&src)?;
    let made = loopback.path(&root.join("made").to_string_lossy());
    let plan = Plan {
        source: src.clone(),
        destination: Destination::Create(made.clone()),
        branch: Some("feature/x".to_owned()),
        folder: PathBuf::new(),
        confirm_above: LARGE,
    };
    let landed = run(&plan, &job())?;
    assert_eq!(here(&landed.folder), root.join("made"));
    assert_arrived(&src, &landed.folder)?;
    assert_eq!(
        git(&root.join("made"), &["remote", "get-url", "origin"])?.trim(),
        "https://example.invalid/team/app.git"
    );
    // The whole history came, not just the branch's last commit.
    assert_eq!(
        git(&root.join("made"), &["rev-list", "--count", "HEAD"])?.trim(),
        "2"
    );
    before.check()?;
    no_bundle_left();
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// A detached checkout arrives detached on its commit.
#[test]
fn a_detached_checkout_arrives_detached() -> Result<(), String> {
    let _one = serial();
    let root = scratch("detached")?;
    let (src, target) = source_with_work(&root)?;
    git(&src, &["checkout", "-q", "--detach"])?;
    let mut plan = plan_to(&src, &target, &Machine::Here);
    plan.branch = None;
    plan.folder = worktree_folder(&target, None, "abcdef0123");
    let landed = run(&plan, &job())?;
    let landed = here(&landed.folder);
    assert!(git(&landed, &["symbolic-ref", "-q", "HEAD"]).is_err());
    assert_eq!(state_of(&landed)?, state_of(&src)?);
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// A bundle that is damaged on the way is refused there, and what the landing began (the
/// worktree, the branch) is taken back; the source is untouched.
#[test]
fn a_corrupt_bundle_is_refused_and_cleaned_up() -> Result<(), String> {
    let _one = serial();
    let root = scratch("corrupt")?;
    let (src, target) = source_with_work(&root)?;
    let before = Untouched::of(&src)?;
    let target_refs = git(&target, &["for-each-ref"])?;
    let plan = plan_to(&src, &target, &Machine::Here);
    let info = inspect(&src)?;
    let want = land::Wanted {
        branch: Some("feature/x"),
        folder: &plan.folder,
    };
    for damage in [Damage::Header, Damage::Pack, Damage::Cut] {
        let mut landing = land::Target::prepare(&plan.destination, &info, &want)?;
        let mut snap = source::Snapshot::take(&info, &new_id())?;
        let mut bundle = snap.bundle(&landing.tips()?)?;
        snap.clean_up()?;
        damage.apply(&mut bundle);
        let upload = files::Upload::start(&Machine::Here, &new_id(), &bundle)?;
        let landed = landing.land(upload.there(), &info, &snap);
        let e = landed.err().unwrap_or_default();
        assert!(e.contains("The bundle"), "{damage:?}: {e}");
        drop(upload);
        drop(landing);
        assert!(!plan.folder.exists(), "{damage:?}: the worktree stayed");
        assert_eq!(git(&target, &["for-each-ref"])?, target_refs, "{damage:?}");
        assert_eq!(
            git(&target, &["worktree", "list", "--porcelain"])?
                .matches("worktree ")
                .count(),
            1
        );
    }
    before.check()?;
    no_bundle_left();
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum Damage {
    /// The first line, which says what it is.
    Header,
    /// A byte in the middle of the pack.
    Pack,
    /// The last part never arrived.
    Cut,
}

impl Damage {
    fn apply(self, bundle: &mut Vec<u8>) {
        let len = bundle.len();
        match self {
            Damage::Header => bundle[2] ^= 0x20,
            Damage::Pack => bundle[len - 30] ^= 0xff,
            Damage::Cut => bundle.truncate(len - 40),
        }
    }
}
