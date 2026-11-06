//! Teleports between two real repositories standing in for two machines: both here, and (on
//! Unix) one of them behind the loopback SSH host, whose shells are `sh` here, so that the
//! work goes through the very shells, framing and `base64 -d` another machine is reached by.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::*;
use crate::git;

/// One at a time: they look for leftovers in folders all of them use.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

fn scratch(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-tp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    // Its real path: `/tmp` is a link on macOS, and git prints real paths. Not in the `\\?\`
    // form Windows gives it, which git doesn't take.
    let real = std::fs::canonicalize(&dir).map_err(|e| e.to_string())?;
    let shown = real.to_string_lossy();
    Ok(match shown.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => real,
    })
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    git::run(Some(dir), args)
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

#[cfg(unix)]
fn set_exec(path: &Path, exec: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if exec { 0o755 } else { 0o644 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())
}

fn configure(dir: &Path) -> Result<(), String> {
    for (key, value) in [
        ("user.name", "t"),
        ("user.email", "t@t"),
        ("commit.gpgSign", "false"),
        ("core.autocrlf", "false"),
    ] {
        git(dir, &["config", key, value])?;
    }
    Ok(())
}

const BINARY: &[u8] = &[0, 159, 146, 150, 255, 0, b'\n', 13, 10, 0, 7];

/// A repository with one commit on `main`, then a branch `feature/x` with one more, which the
/// clone made in between doesn't have; then changes of every kind: staged, unstaged, both in
/// one file, deleted, renamed, new, untracked, ignored, binary, executable, intent to add,
/// names with spaces and letters outside ASCII.
fn source_with_work(root: &Path) -> Result<(PathBuf, PathBuf), String> {
    let src = root.join("source");
    std::fs::create_dir_all(&src).map_err(|e| e.to_string())?;
    git(&src, &["init", "-q", "-b", "main"])?;
    configure(&src)?;
    write(&src.join("a.txt"), b"one\ntwo\nthree\n")?;
    write(&src.join("b.bin"), BINARY)?;
    write(&src.join("script.sh"), b"#!/bin/sh\necho hi\n")?;
    write(&src.join("dir with space/\u{fc}ber file.txt"), b"umlaut\n")?;
    write(&src.join("gone staged.txt"), b"x\n")?;
    write(&src.join("gone unstaged.txt"), b"y\n")?;
    write(&src.join("old name.txt"), b"renamed content\nline two\n")?;
    write(&src.join(".gitignore"), b"build/\n")?;
    #[cfg(unix)]
    {
        set_exec(&src.join("script.sh"), true)?;
        std::os::unix::fs::symlink("a.txt", src.join("link")).map_err(|e| e.to_string())?;
    }
    git(&src, &["add", "-A"])?;
    git(&src, &["commit", "-qm", "first"])?;
    git(
        &src,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/team/app.git",
        ],
    )?;
    let target = root.join("target");
    git(
        root,
        &[
            "clone",
            "-q",
            &src.to_string_lossy(),
            &target.to_string_lossy(),
        ],
    )?;
    configure(&target)?;
    git(
        &target,
        &[
            "remote",
            "set-url",
            "origin",
            "git@example.invalid:team/app",
        ],
    )?;
    git(&src, &["checkout", "-q", "-b", "feature/x"])?;
    write(&src.join("feature.txt"), b"feature work\n")?;
    git(&src, &["add", "feature.txt"])?;
    git(&src, &["commit", "-qm", "feature"])?;

    // Staged, then changed again on top.
    write(&src.join("a.txt"), b"one\nTWO\nthree\n")?;
    git(&src, &["add", "a.txt"])?;
    write(&src.join("a.txt"), b"one\nTWO\nthree\nfour\n")?;
    git(&src, &["rm", "-q", "gone staged.txt"])?;
    std::fs::remove_file(src.join("gone unstaged.txt")).map_err(|e| e.to_string())?;
    git(&src, &["mv", "old name.txt", "new name.txt"])?;
    write(&src.join("staged new \u{2713}.txt"), b"brand new\n")?;
    git(&src, &["add", "staged new \u{2713}.txt"])?;
    let mut changed = BINARY.to_vec();
    changed.extend_from_slice(&[1, 2, 3, 0, 255]);
    write(&src.join("b.bin"), &changed)?;
    #[cfg(unix)]
    set_exec(&src.join("script.sh"), false)?;
    write(&src.join("untracked file.txt"), b"not added\n")?;
    write(&src.join("untracked dir/nested.bin"), BINARY)?;
    write(&src.join("build/out.o"), b"ignored\n")?;
    write(&src.join("intent.txt"), b"to be added\n")?;
    git(&src, &["add", "-N", "intent.txt"])?;
    // Untracked, and matched by the pattern the next file's name is.
    write(&src.join("intentional.txt"), b"untracked\n")?;
    #[cfg(unix)]
    {
        write(&src.join("intent*.txt"), b"a name like a pattern\n")?;
        git(&src, &["--literal-pathspecs", "add", "-N", "intent*.txt"])?;
    }
    Ok((src, target))
}

/// Files by their path: their bytes, whether executable, whether a link.
type Files = BTreeMap<String, (Vec<u8>, bool, bool)>;

/// Every file under `dir` but `.git` (and ignored `build/`): its bytes, whether it is
/// executable, whether it is a link.
fn files_of(dir: &Path) -> Result<Files, String> {
    fn walk(base: &Path, dir: &Path, out: &mut Files) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let name = path
                .strip_prefix(base)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            if name == ".git" || name == "build" {
                continue;
            }
            let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if meta.is_dir() {
                walk(base, &path, out)?;
                continue;
            }
            let link = meta.file_type().is_symlink();
            let bytes = if link {
                std::fs::read_link(&path)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .into_owned()
                    .into_bytes()
            } else {
                std::fs::read(&path).map_err(|e| e.to_string())?
            };
            #[cfg(unix)]
            let exec = {
                use std::os::unix::fs::PermissionsExt;
                !link && meta.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let exec = false;
            out.insert(name, (bytes, exec, link));
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

/// What git says of a working tree: its index entries and its status, renames included.
fn state_of(dir: &Path) -> Result<(String, String), String> {
    let index = git(dir, &["ls-files", "-s"])?;
    let status = git(
        dir,
        &["status", "--porcelain=v1", "--untracked-files=all", "-z"],
    )?;
    Ok((index, status))
}

/// What the source must still be afterwards, byte for byte: index file, refs, files.
struct Untouched {
    dir: PathBuf,
    index: Vec<u8>,
    refs: String,
    head: String,
    files: Files,
}

impl Untouched {
    fn of(dir: &Path) -> Result<Self, String> {
        Ok(Self {
            dir: dir.to_path_buf(),
            index: std::fs::read(dir.join(".git/index")).map_err(|e| e.to_string())?,
            refs: git(dir, &["for-each-ref"])?,
            head: std::fs::read_to_string(dir.join(".git/HEAD")).map_err(|e| e.to_string())?,
            files: files_of(dir)?,
        })
    }

    fn check(&self) -> Result<(), String> {
        let now = Self::of(&self.dir)?;
        assert!(now.index == self.index, "the source's index changed");
        assert_eq!(now.refs, self.refs, "the source's refs changed");
        assert_eq!(now.head, self.head);
        assert!(now.files == self.files, "the source's files changed");
        assert!(
            !leftover_in(&self.dir.join(".git"), "gitcraft-teleport"),
            "a temporary index stayed"
        );
        Ok(())
    }
}

/// Whether a file whose name starts with `prefix` is in `dir`.
fn leftover_in(dir: &Path, prefix: &str) -> bool {
    std::fs::read_dir(dir).is_ok_and(|d| {
        d.flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with(prefix))
    })
}

/// No bundle of this process left in this machine's temporary folders (here, and the private
/// one the loopback host's files go in).
fn no_bundle_left() {
    let tmp = std::env::temp_dir();
    assert!(!leftover_in(&tmp, "gitcraft-teleport-"), "a bundle stayed");
    let private = std::fs::read_dir(&tmp)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("gitcraft-"))
        .map(|e| e.path())
        .filter(|p| p.is_dir());
    let ours = format!("{}-", std::process::id());
    for dir in private {
        let left = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.starts_with(&ours) && name.ends_with(".bundle")
            });
        assert!(!left, "a bundle stayed in {}", dir.display());
    }
}

/// `len` bytes that don't compress.
fn noise(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

fn job() -> Job {
    Job::new(|| {})
}

/// The plan that sends `src`'s work to a new worktree beside `target` on `machine`.
fn plan_to(src: &Path, target_here: &Path, machine: &Machine) -> Plan {
    let there = target_here.to_string_lossy().replace('\\', "/");
    let repo = machine.path(&there);
    Plan {
        source: src.to_path_buf(),
        folder: worktree_folder(&repo, Some("feature/x"), ""),
        destination: Destination::Repo(repo),
        branch: Some("feature/x".to_owned()),
        confirm_above: LARGE,
    }
}

/// The folder as it is here, for one that is behind the loopback host.
fn here(path: &Path) -> PathBuf {
    PathBuf::from(crate::remote::git_path(path))
}

/// The whole of the work arrived, exactly: the same index, status and files as the source's.
fn assert_arrived(src: &Path, landed: &Path) -> Result<(), String> {
    let landed = here(landed);
    assert_eq!(state_of(&landed)?, state_of(src)?);
    let (theirs, ours) = (files_of(&landed)?, files_of(src)?);
    assert_eq!(
        theirs.keys().collect::<Vec<_>>(),
        ours.keys().collect::<Vec<_>>()
    );
    assert!(theirs == ours, "a file's bytes or mode differ");
    assert_eq!(
        git(&landed, &["rev-parse", "HEAD"])?,
        git(src, &["rev-parse", "HEAD"])?
    );
    assert_eq!(
        git(&landed, &["symbolic-ref", "--short", "HEAD"])?.trim(),
        "feature/x"
    );
    assert!(!landed.join("build").exists(), "an ignored file came along");
    // Still known as added with intent, not as an untracked file.
    let intent = git(
        &landed,
        &[
            "-c",
            "core.quotePath=false",
            "diff-files",
            "--name-only",
            "--diff-filter=A",
        ],
    )?;
    let expected = if cfg!(unix) {
        "intent*.txt\nintent.txt"
    } else {
        "intent.txt"
    };
    assert_eq!(intent.trim(), expected);
    Ok(())
}

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

/// Stopped while it waits for a large bundle to be allowed: nothing is left on either side.
#[test]
fn a_large_bundle_waits_and_a_no_leaves_nothing() -> Result<(), String> {
    let _one = serial();
    let root = scratch("large")?;
    let (src, target) = source_with_work(&root)?;
    let before = Untouched::of(&src)?;
    let target_refs = git(&target, &["for-each-ref"])?;
    let mut plan = plan_to(&src, &target, &Machine::Here);
    plan.confirm_above = 0;
    let job = std::sync::Arc::new(job());
    let asked = std::sync::Arc::clone(&job);
    let answer = std::thread::spawn(move || {
        while !matches!(asked.step(), Step::Confirm(_)) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        asked.answer(false);
    });
    let result = run(&plan, &job);
    let _ = answer.join();
    let failed = result.err().ok_or("it landed")?;
    assert!(failed.cancelled);
    assert_eq!(failed.message, CANCELLED);
    assert!(!plan.folder.exists());
    assert_eq!(git(&target, &["for-each-ref"])?, target_refs);
    before.check()?;
    no_bundle_left();
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// What can't be carried is refused with its reason, before anything is made anywhere.
#[test]
fn what_cant_go_is_refused_with_a_reason() -> Result<(), String> {
    let _one = serial();
    let root = scratch("refused")?;
    let (src, target) = source_with_work(&root)?;
    let plan = plan_to(&src, &target, &Machine::Here);
    let refused = |plan: &Plan| {
        run(plan, &job())
            .err()
            .map(String::from)
            .unwrap_or_default()
    };

    // The branch is checked out there.
    git(&target, &["branch", "feature/x", "main"])?;
    git(&target, &["checkout", "-q", "feature/x"])?;
    let e = refused(&plan);
    assert!(e.contains("is checked out on this machine already"), "{e}");
    // There, with commits the source doesn't have.
    git(&target, &["checkout", "-q", "main"])?;
    git(&target, &["checkout", "-q", "feature/x"])?;
    git(
        &target,
        &["commit", "-q", "--allow-empty", "-m", "only there"],
    )?;
    git(&target, &["checkout", "-q", "main"])?;
    let e = refused(&plan);
    assert!(e.contains("has commits that aren't here"), "{e}");
    git(&target, &["branch", "-q", "-D", "feature/x"])?;
    // A folder that is taken.
    std::fs::create_dir_all(&plan.folder).map_err(|e| e.to_string())?;
    assert!(refused(&plan).contains("is already there"));
    std::fs::remove_dir_all(&plan.folder).map_err(|e| e.to_string())?;
    // A repository of its own inside the work.
    let nested = src.join("untracked dir/inner");
    std::fs::create_dir_all(&nested).map_err(|e| e.to_string())?;
    git(&nested, &["init", "-q"])?;
    configure(&nested)?;
    git(&nested, &["commit", "-q", "--allow-empty", "-m", "inner"])?;
    let before = Untouched::of(&src)?;
    let e = refused(&plan);
    assert!(e.contains("is a repository of its own"), "{e}");
    before.check()?;
    std::fs::remove_dir_all(&nested).map_err(|e| e.to_string())?;
    // A merge with conflicts.
    git(&src, &["reset", "-q", "--hard"])?;
    git(&src, &["clean", "-q", "-f", "-d"])?;
    git(&src, &["checkout", "-q", "-b", "other", "main"])?;
    write(&src.join("feature.txt"), b"other side\n")?;
    git(&src, &["add", "feature.txt"])?;
    git(&src, &["commit", "-qm", "other"])?;
    let _ = git(&src, &["merge", "-q", "feature/x"]);
    let e = refused(&plan);
    assert!(e.contains("merge is in progress"), "{e}");
    assert!(!plan.folder.exists());
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// Submodules aren't carried.
#[test]
fn submodules_are_refused() -> Result<(), String> {
    let _one = serial();
    let root = scratch("submodule")?;
    let (src, target) = source_with_work(&root)?;
    let sha = git(&src, &["rev-parse", "HEAD"])?;
    let entry = format!("160000,{},vendor/lib", sha.trim());
    git(&src, &["update-index", "--add", "--cacheinfo", &entry])?;
    let e = run(&plan_to(&src, &target, &Machine::Here), &job())
        .err()
        .map(String::from)
        .unwrap_or_default();
    assert!(e.contains("submodules"), "{e}");
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// A clone of the same project is found among the folders known on a machine, whatever its
/// remote's URL looks like.
#[test]
fn a_clone_of_the_project_is_found() -> Result<(), String> {
    let _one = serial();
    let root = scratch("find")?;
    let (src, target) = source_with_work(&root)?;
    let other = root.join("other");
    std::fs::create_dir_all(&other).map_err(|e| e.to_string())?;
    git(&other, &["init", "-q"])?;
    let urls: Vec<String> = git::remotes(&src)?.into_iter().map(|r| r.url).collect();
    let found = find_clones(
        &[
            other,
            target.join("dir with space"),
            root.join("nowhere"),
            target.clone(),
        ],
        &urls,
    );
    assert_eq!(found, vec![target]);
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn pieces_cover_the_bundle_once() {
    assert_eq!(protocol::chunks(0, 4), vec![0..0]);
    assert_eq!(protocol::chunks(4, 4), vec![0..4]);
    assert_eq!(protocol::chunks(9, 4), vec![0..4, 4..8, 8..9]);
    assert_eq!(protocol::chunks(3, 0), vec![0..1, 1..2, 2..3]);
}

#[test]
fn the_bundle_is_the_snapshot_without_what_is_there() {
    let base = "a".repeat(40);
    assert_eq!(
        protocol::bundle_revs("refs/gitcraft/teleport/1", std::slice::from_ref(&base)),
        format!("refs/gitcraft/teleport/1\n^{base}\n")
    );
    assert_eq!(protocol::bundle_revs("r", &[]), "r\n");
}

#[test]
fn answers_are_read() {
    let (a, b, c) = ("a".repeat(40), "b".repeat(40), "c".repeat(64));
    // An annotated tag counts as its commit; a line that isn't a hash is dropped.
    let tips = protocol::parse_tips(&format!("{a} \n{b} {c}\n{a} \nnonsense\n"));
    assert_eq!(tips, vec![a.clone(), c.clone()]);
    let present = protocol::parse_present(&format!("{a} commit\n{b} missing\n{c} tag\n"));
    assert_eq!(present, vec![a.clone(), c]);
    let raw = format!(
        ":000000 160000 {z} {a} A\0vendor/x\0:100644 100644 {a} {b} M\0a b.txt\0",
        z = "0".repeat(40)
    );
    assert_eq!(protocol::parse_gitlinks(raw.as_bytes()), vec!["vendor/x"]);
    let attrs = b"big.psd\0filter\0lfs\0a.txt\0filter\0unspecified\0";
    assert_eq!(protocol::parse_lfs_paths(attrs), vec!["big.psd"]);
    assert_eq!(
        protocol::nul_list(&["a b".to_owned(), "\u{fc}".to_owned()]),
        "a b\0\u{fc}\0".as_bytes()
    );
}

#[test]
fn machines_are_listed_once_and_never_the_source_s() {
    use crate::remote::Host;
    let ssh = |h: &str| Machine::There(Host::Ssh(h.to_owned()));
    let known = vec![
        PathBuf::from("ssh://build/home/me/app"),
        PathBuf::from("ssh://other/srv/x"),
        PathBuf::from("/home/me/local"),
    ];
    let list = machines(
        Path::new("ssh://build/home/me/app"),
        &["Ubuntu".to_owned(), "-bad".to_owned()],
        &["build".to_owned(), "-oProxyCommand=x".to_owned()],
        &known,
    );
    assert_eq!(
        list,
        vec![
            Machine::Here,
            Machine::There(Host::Wsl("Ubuntu".to_owned())),
            ssh("other"),
        ]
    );
    assert!(!machines(Path::new("/home/me/app"), &[], &[], &[]).contains(&Machine::Here));
}

#[test]
fn a_worktree_goes_beside_its_repository() {
    assert_eq!(
        worktree_folder(Path::new("ssh://build/home/me/app"), Some("feat/x y"), ""),
        PathBuf::from("ssh://build/home/me/app-feat-x-y")
    );
    assert_eq!(
        worktree_folder(Path::new("/code/app"), None, "0123456789"),
        Path::new("/code").join("app-01234567")
    );
}

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
