from pathlib import Path
root=Path('/workspace/work/review-file-comparison-146a2e3/source')
p=root/'src/git/file_comparison/tests.rs'
p.write_text(p.read_text()+r'''
#[test]
fn reviewer_actual_lens_downloads_missing_attributes() -> Result<(), String> {
    let source=Repo::new()?;
    source.write("a",b"plain\n")?;source.write(".gitattributes",b"a text eol=lf\n")?;source.commit()?;
    source.git(&["config","uploadpack.allowFilter","true"])?;
    let partial=Repo(source.0.join("partial-review"));
    let out=source.command(&["clone","--no-checkout","--no-local","--filter=blob:none"])
        .arg(&source.0).arg(&partial.0).output().map_err(|e|e.to_string())?;
    if !out.status.success(){return Err(String::from_utf8_lossy(&out.stderr).into());}
    partial.git(&["--no-lazy-fetch","read-tree","HEAD"])?;
    // The selected blob is available; only the governing attributes blob is promised.
    let content=source.git(&["rev-parse","HEAD:a"])?;
    let unpack=source.command(&["cat-file","blob",&content]).output().map_err(|e|e.to_string())?;
    let mut hash=partial.command(&["--no-lazy-fetch","hash-object","-w","--stdin"])
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().map_err(|e|e.to_string())?;
    use std::io::Write;
    hash.stdin.take().ok_or("stdin")?.write_all(&unpack.stdout).map_err(|e|e.to_string())?;
    assert!(hash.wait_with_output().map_err(|e|e.to_string())?.status.success());
    partial.write("a",b"plain\n")?;
    let attr=source.git(&["rev-parse","HEAD:.gitattributes"])?;
    let before=partial.command(&["--no-lazy-fetch","cat-file","-e",&attr]).output().map_err(|e|e.to_string())?.status.success();
    let lens=partial.lens("a")?;
    let after=partial.command(&["--no-lazy-fetch","cat-file","-e",&attr]).output().map_err(|e|e.to_string())?.status.success();
    let report=serde_json::json!({"production":"read_lens","attribute_blob_missing_before":!before,"attribute_blob_present_after":after,"lens_conversion_warning":lens.staging_warning});
    std::fs::write("/workspace/work/review-file-comparison-146a2e3/production-attributes.json",serde_json::to_vec_pretty(&report).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    assert!(!before);assert!(after);
    Ok(())
}
''')
p=root/'src/git/file_comparison/transaction.rs'
p.write_text(p.read_text()+r'''
#[cfg(all(test, unix))]
mod reviewer_transaction_tests {
    use super::*;
    use crate::git::file_comparison::tests::Repo;
    use std::process::Command;
    #[test]
    fn reviewer_remote_locked_transaction_ignores_configuration_drift() -> Result<(),String> {
        const CHILD:&str="GITCRAFT_REVIEW_REMOTE_TRANSACTION";
        const NAME:&str="git::file_comparison::transaction::reviewer_transaction_tests::reviewer_remote_locked_transaction_ignores_configuration_drift";
        if let Ok(path)=std::env::var(CHILD) {
            let encoded=crate::remote::encode(&crate::remote::Host::Ssh("review-fixture.invalid".into()),&path);
            let target=crate::remote::parse(&encoded).ok_or("target")?;
            let spec=LensSpec{path:"a".into(),previous:None};
            let lens=read_lens(&encoded,&spec,&||true)?;
            // This is exactly the initial production check in apply_lens_patch. The
            // following change models drift before its separate remote lock round trip.
            verify(&encoded,&lens)?;
            std::fs::write(std::path::Path::new(&path).join(".gitattributes"),b"a text eol=lf\n").map_err(|e|e.to_string())?;
            let patch=patch_header("a",Some("100644"),Some("100644"))+"@@ -1 +1 @@\n-old\n+new\r\n";
            let result=remote_apply(&target,&lens,&patch,false);
            let current=read_lens(&encoded,&spec,&||true)?;
            let report=serde_json::json!({"production":"initial verify then remote_apply locked transaction","conversion_changed_after_initial_verification":current.staging_warning.is_some(),"stale_change_refused":result.is_err(),"index_bytes":current.versions[1].bytes,"working_bytes":current.versions[2].bytes,"error":result.as_ref().err()});
            std::fs::write("/workspace/work/review-file-comparison-146a2e3/production-remote-drift.json",serde_json::to_vec_pretty(&report).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
            assert!(result.is_ok(),"{result:?}");assert!(current.staging_warning.is_some());assert_eq!(current.versions[1].bytes,b"new\r\n");
            return Ok(());
        }
        let r=Repo::new()?;r.write("a",b"old\n")?;r.commit()?;r.write("a",b"new\r\n")?;
        let bin=r.0.join("fixture-bin");std::fs::create_dir(&bin).map_err(|e|e.to_string())?;
        let ssh=bin.join("ssh");
        // Intercept timeout cleanup too: the fixture must never run a host PID script.
        std::fs::write(&ssh,"#!/bin/sh\nexec /bin/sh -c 'ps() { return 125; }; kill() { return 125; }; . /dev/stdin'\n").map_err(|e|e.to_string())?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&ssh,std::fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
        for (name,body) in [("ps","exit 125"),("kill","exit 125")] {
            let file=bin.join(name);std::fs::write(&file,format!("#!/bin/sh\n{body}\n")).map_err(|e|e.to_string())?;
            std::fs::set_permissions(file,std::fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
        }
        let paths=std::env::join_paths([bin,std::path::PathBuf::from("/usr/bin"),std::path::PathBuf::from("/bin")]).map_err(|e|e.to_string())?;
        let out=Command::new(std::env::current_exe().map_err(|e|e.to_string())?).args(["--exact",NAME,"--nocapture"])
            .env(CHILD,&r.0).env("PATH",paths).env_remove("GITCRAFT_TEST_SSH").env_remove("GITCRAFT_TEST_WSL").output().map_err(|e|e.to_string())?;
        assert!(out.status.success(),"{} {}",String::from_utf8_lossy(&out.stdout),String::from_utf8_lossy(&out.stderr));
        Ok(())
    }
}
''')
