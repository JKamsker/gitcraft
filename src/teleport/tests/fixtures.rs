// Fixtures. Included to preserve libtest shard names.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    super::test_serial()
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
#[cfg(unix)]
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
