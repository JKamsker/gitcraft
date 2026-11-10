use super::*;
use crate::git::file_comparison::tests::Repo;
use std::os::unix::fs::PermissionsExt;
fn run(r: &Repo, script: &str, bin: Option<&Path>) -> Result<std::process::Output, String> {
    let mut cmd = std::process::Command::new("/bin/sh");
    cmd.args(["-c", script]).current_dir(&r.0);
    if let Some(bin) = bin {
        cmd.env(
            "PATH",
            format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );
    }
    crate::processes::run(
        cmd,
        crate::processes::Options {
            timeout: Some(std::time::Duration::from_secs(20)),
            ..Default::default()
        },
    )?
    .complete()
}
#[test]
fn owned_remote_program_preserves_foreign_locks_and_applies_native_sharing_policy()
-> Result<(), String> {
    for case in ["0600", "group", "acl", "foreign-file", "foreign-link"] {
        let r = Repo::new()?;
        let path = "a ' $(literal) ü";
        r.write(path, "before\n")?;
        r.write("unrelated", "unchanged\n")?;
        r.commit()?;
        let sharing = if case == "group" { "group" } else { "0600" };
        r.git(&["config", "core.sharedRepository", sharing])?;
        // A no-op add can retain the old index inode/mode. Force native writes
        // before taking the reference policy, just as the real mutation will.
        r.write(path, "native reference rewrite\n")?;
        r.git(&["add", path])?;
        r.write(path, "before\n")?;
        r.git(&["add", path])?;
        #[cfg(target_os = "macos")]
        if case == "acl" {
            exacl::setfacl(
                &[r.0.join(".git/index")],
                &[exacl::AclEntry::allow_user(
                    "nobody",
                    exacl::Perm::READ,
                    None,
                )],
                None,
            )
            .map_err(|e| e.to_string())?;
        }
        #[cfg(target_os = "macos")]
        let expected_acl = exacl::getfacl(r.0.join(".git/index"), exacl::AclOption::SYMLINK_ACL)
            .map_err(|e| e.to_string())?;
        let expected = std::fs::metadata(r.0.join(".git/index"))
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o777;
        r.write(path, "after\n")?;
        let lens = r.lens(path)?;
        let patch =
            patch_header(path, Some("100644"), Some("100644")) + "@@ -1 +1 @@\n-before\n+after\n";
        let mut bin = None;
        if case.starts_with("foreign") {
            let tools = r.0.join("tools");
            std::fs::create_dir(&tools).map_err(|e| e.to_string())?;
            let real = std::process::Command::new("/bin/sh")
                .args(["-c", "command -v git"])
                .output()
                .map_err(|e| e.to_string())?;
            let real = String::from_utf8(real.stdout).map_err(|e| e.to_string())?;
            let lock = r.0.join(".git/index.lock");
            let displaced = r.0.join("displaced-lock");
            let sentinel = r.0.join("sentinel");
            std::fs::write(&sentinel, b"outside-original").map_err(|e| e.to_string())?;
            let replace = if case == "foreign-link" {
                format!(
                    "ln -s {} {}",
                    crate::remote::sh_quote(&sentinel.to_string_lossy()),
                    crate::remote::sh_quote(&lock.to_string_lossy())
                )
            } else {
                format!(
                    "printf FOREIGN-LOCK-BYTES > {}",
                    crate::remote::sh_quote(&lock.to_string_lossy())
                )
            };
            let wrapper = format!(
                "#!/bin/sh\n{} \"$@\"\nrc=$?\ncase \"$*\" in *'apply --cached'*) case \"$GIT_INDEX_FILE\" in {}/.git/gitcraft-index-recovery-*/candidate) mv {} {} && {};; esac;; esac\nexit \"$rc\"\n",
                crate::remote::sh_quote(real.trim()),
                crate::remote::sh_quote(&r.0.to_string_lossy()),
                crate::remote::sh_quote(&lock.to_string_lossy()),
                crate::remote::sh_quote(&displaced.to_string_lossy()),
                replace
            );
            let exe = tools.join("git");
            std::fs::write(&exe, wrapper).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
            bin = Some(tools);
        }
        let out = run(
            &r,
            &remote_apply_script(&lens, &patch, false)?,
            bin.as_deref(),
        )?;
        if case.starts_with("foreign") {
            assert!(
                !out.status.success(),
                "{case}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(std::fs::symlink_metadata(r.0.join(".git/index.lock")).is_ok());
            assert_eq!(
                std::fs::read(r.0.join("sentinel")).map_err(|e| e.to_string())?,
                b"outside-original"
            );
            assert_eq!(r.git(&["show", &format!(":{path}")])?, "before");
            if case == "foreign-file" {
                assert_eq!(
                    std::fs::read(r.0.join(".git/index.lock")).map_err(|e| e.to_string())?,
                    b"FOREIGN-LOCK-BYTES"
                );
            }
        } else {
            assert!(
                out.status.success(),
                "{case}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(r.git(&["show", &format!(":{path}")])?, "after");
            assert_eq!(
                std::fs::metadata(r.0.join(".git/index"))
                    .map_err(|e| e.to_string())?
                    .permissions()
                    .mode()
                    & 0o777,
                expected
            );
            assert!(!r.0.join(".git/index.lock").exists());
            #[cfg(target_os = "macos")]
            assert_eq!(
                exacl::getfacl(r.0.join(".git/index"), exacl::AclOption::SYMLINK_ACL)
                    .map_err(|e| e.to_string())?,
                expected_acl
            );
        }
        assert_eq!(r.git(&["show", ":unrelated"])?, "unchanged");
        assert_eq!(
            std::fs::read(r.0.join(path)).map_err(|e| e.to_string())?,
            b"after\n"
        );
    }
    let r = Repo::new()?;
    r.git(&["config", "core.sharedRepository", "0600"])?;
    r.write("new", "new\n")?;
    let lens = r.lens("new")?;
    let patch = patch_header("new", None, Some("100644")) + "@@ -0,0 +1 @@\n+new\n";
    let out = run(&r, &remote_apply_script(&lens, &patch, false)?, None)?;
    assert!(
        out.status.success(),
        "absent index: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(r.git(&["show", ":new"])?, "new");
    assert_eq!(
        std::fs::metadata(r.0.join(".git/index"))
            .map_err(|e| e.to_string())?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    Ok(())
}
#[cfg(target_os = "macos")]
#[test]
fn stock_darwin_md5_is_accepted_without_gnu_md5sum() -> Result<(), String> {
    let r = Repo::new()?;
    let tools = r.0.join("tools");
    std::fs::create_dir(&tools).map_err(|e| e.to_string())?;
    for (program, path) in [("md5", "/sbin/md5"), ("perl", "/usr/bin/perl")] {
        std::os::unix::fs::symlink(path, tools.join(program)).map_err(|e| e.to_string())?;
    }
    let idx = r.0.join("index-source");
    let sentinel = r.0.join("outside");
    r.write("outside", "outside bytes must not be read")?;
    for case in ["regular", "missing", "symlink", "oversize"] {
        if std::fs::symlink_metadata(&idx).is_ok() {
            std::fs::remove_file(&idx).map_err(|e| e.to_string())?;
        }
        match case {
            "regular" => std::fs::write(&idx, b"abc").map_err(|e| e.to_string())?,
            "symlink" => std::os::unix::fs::symlink(&sentinel, &idx).map_err(|e| e.to_string())?,
            "oversize" => std::fs::write(&idx, vec![0; INDEX_LIMIT as usize + 1])
                .map_err(|e| e.to_string())?,
            _ => (),
        }
        let script = format!(
            "idx={}\nd={}\n{}\nprintf '%s' \"$actual\"",
            crate::remote::sh_quote(&idx.to_string_lossy()),
            crate::remote::sh_quote(&r.0.to_string_lossy()),
            remote_stamp_read()
        );
        let mut cmd = std::process::Command::new("/bin/sh");
        cmd.args(["-c", &script]).env("PATH", &tools);
        let out = crate::processes::run(cmd, crate::processes::Options::default())?.complete()?;
        if case == "regular" {
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.stdout, b"900150983cd24fb0d6963f7d28e17f72");
        } else if case == "missing" {
            assert!(out.status.success());
            assert_eq!(out.stdout, b"missing");
        } else {
            assert!(!out.status.success());
        }
    }
    assert_eq!(
        std::fs::read(&sentinel).map_err(|e| e.to_string())?,
        b"outside bytes must not be read"
    );
    Ok(())
}
