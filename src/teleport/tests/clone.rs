// Clone. Included to preserve libtest shard names.
/// A clone made for the work: nothing of the clone's own checkout is in the way (its default
/// branch is free to take, moved on to the work's commit), and the work is in its working tree.
#[test]
fn work_arrives_in_a_clone_made_for_it() -> Result<(), String> {
    let _one = serial();
    let root = scratch("clone")?;
    let (src, _) = source_with_work(&root)?;
    let upstream = root.join("upstream.git");
    git(
        &root,
        &[
            "clone",
            "-q",
            "--bare",
            &src.to_string_lossy(),
            &upstream.to_string_lossy(),
        ],
    )?;
    git(&upstream, &["update-ref", "refs/heads/feature/x", "main"])?;
    let before = Untouched::of(&src)?;
    let made = root.join("cloned");
    let plan = Plan {
        source: src.clone(),
        destination: Destination::Clone {
            url: upstream.to_string_lossy().into_owned(),
            path: made.clone(),
        },
        branch: Some("main".to_owned()),
        folder: PathBuf::new(),
        confirm_above: LARGE,
    };
    let landed = run(&plan, &job())?;
    assert_eq!(here(&landed.folder), made);
    assert_eq!(state_of(&made)?, state_of(&src)?);
    // A clone takes this machine's own settings: on Windows (the CI runner's global
    // `core.autocrlf=true`) its text files are checked out with CRLF, as any clone's would be.
    let lf = |files: Files| -> Files {
        files
            .into_iter()
            .map(|(name, (bytes, exec, link))| {
                let bytes = if cfg!(windows) {
                    let mut out = Vec::with_capacity(bytes.len());
                    for (i, &b) in bytes.iter().enumerate() {
                        if !(b == b'\r' && bytes.get(i + 1) == Some(&b'\n')) {
                            out.push(b);
                        }
                    }
                    out
                } else {
                    bytes
                };
                (name, (bytes, exec, link))
            })
            .collect()
    };
    assert!(lf(files_of(&made)?) == lf(files_of(&src)?));
    assert_eq!(
        git(&made, &["symbolic-ref", "--short", "HEAD"])?.trim(),
        "main"
    );
    assert_eq!(
        git(&made, &["rev-parse", "HEAD"])?,
        git(&src, &["rev-parse", "HEAD"])?
    );
    before.check()?;
    no_bundle_left();
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// A teleport into a clone that is cancelled takes the clone back too.
#[test]
fn a_cancelled_teleport_takes_its_clone_back() -> Result<(), String> {
    let _one = serial();
    let root = scratch("clone-cancel")?;
    let (src, _) = source_with_work(&root)?;
    let upstream = root.join("upstream.git");
    git(
        &root,
        &[
            "clone",
            "-q",
            "--bare",
            &src.to_string_lossy(),
            &upstream.to_string_lossy(),
        ],
    )?;
    let made = root.join("cloned");
    let plan = Plan {
        source: src.clone(),
        destination: Destination::Clone {
            url: upstream.to_string_lossy().into_owned(),
            path: made.clone(),
        },
        branch: Some("feature/x".to_owned()),
        folder: PathBuf::new(),
        confirm_above: 0,
    };
    let job = std::sync::Arc::new(job());
    let asked = std::sync::Arc::clone(&job);
    let answer = std::thread::spawn(move || {
        while !matches!(asked.step(), Step::Confirm(_)) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        asked.cancel();
    });
    let failed = run(&plan, &job).err().ok_or("it landed")?;
    let _ = answer.join();
    assert!(failed.cancelled, "{}", failed.message);
    assert!(!made.exists(), "the clone stayed");
    no_bundle_left();
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// What `add -A` wouldn't see, or what would arrive broken, is refused before anything is made:
/// a shallow clone, files git is told to look away from, uncommitted LFS files whose rule is in
/// a folder's own `.gitattributes`.
#[test]
fn what_would_be_lost_is_refused() -> Result<(), String> {
    let _one = serial();
    let root = scratch("lost")?;
    let (src, target) = source_with_work(&root)?;
    let plan = plan_to(&src, &target, &Machine::Here);
    let refused = |plan: &Plan| {
        run(plan, &job())
            .err()
            .map(String::from)
            .unwrap_or_default()
    };

    git(&src, &["update-index", "--assume-unchanged", "a.txt"])?;
    let e = refused(&plan);
    assert!(e.contains("a.txt is marked"), "{e}");
    git(&src, &["update-index", "--no-assume-unchanged", "a.txt"])?;

    write(&src.join("assets/.gitattributes"), b"*.bin filter=lfs\n")?;
    write(&src.join("assets/new.bin"), BINARY)?;
    let before = Untouched::of(&src)?;
    let e = refused(&plan);
    assert!(e.contains("assets/new.bin is kept in Git LFS"), "{e}");
    before.check()?;
    std::fs::remove_dir_all(src.join("assets")).map_err(|e| e.to_string())?;

    let shallow = root.join("shallow");
    let path = src.to_string_lossy().replace('\\', "/");
    let url = if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    };
    git(
        &root,
        &[
            "clone",
            "-q",
            "--depth",
            "1",
            "--branch",
            "feature/x",
            &url,
            &shallow.to_string_lossy(),
        ],
    )?;
    let mut from_shallow = plan.clone();
    from_shallow.source = shallow;
    let e = refused(&from_shallow);
    assert!(e.contains("shallow clone"), "{e}");
    assert!(!plan.folder.exists());
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// A snapshot of a checkout that moved on since it was looked at is refused: its index belongs
/// to another commit.
#[test]
fn a_head_that_moved_is_refused() -> Result<(), String> {
    let _one = serial();
    let root = scratch("moved")?;
    let (src, _) = source_with_work(&root)?;
    let info = inspect(&src)?;
    git(&src, &["commit", "-q", "--allow-empty", "-m", "meanwhile"])?;
    let e = source::Snapshot::take(&info, &new_id())
        .err()
        .unwrap_or_default();
    assert!(e.contains("changed while"), "{e}");
    assert!(git(&src, &["for-each-ref", "refs/gitcraft"])?.is_empty());
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// A folder that appears where the worktree was to go, while the bundle travels, is left as it
/// is: never removed as if the teleport had made it.
#[test]
fn a_folder_that_appears_meanwhile_is_left_alone() -> Result<(), String> {
    let _one = serial();
    let root = scratch("appeared")?;
    let (src, target) = source_with_work(&root)?;
    let plan = plan_to(&src, &target, &Machine::Here);
    let info = inspect(&src)?;
    let want = land::Wanted {
        branch: Some("feature/x"),
        folder: &plan.folder,
    };
    let mut landing = land::Target::prepare(&plan.destination, &info, &want)?;
    let mut snap = source::Snapshot::take(&info, &new_id())?;
    let bundle = snap.bundle(&landing.tips()?)?;
    snap.clean_up()?;
    write(&plan.folder.join("mine.txt"), b"someone else's\n")?;
    let upload = files::Upload::start(&Machine::Here, &new_id(), &bundle)?;
    let e = landing
        .land(upload.there(), &info, &snap)
        .err()
        .unwrap_or_default();
    assert!(e.contains("appeared"), "{e}");
    drop(upload);
    drop(landing);
    assert_eq!(
        std::fs::read(plan.folder.join("mine.txt")).map_err(|e| e.to_string())?,
        b"someone else's\n"
    );
    assert!(git(&target, &["branch", "--list", "feature/x"])?.is_empty());
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}
