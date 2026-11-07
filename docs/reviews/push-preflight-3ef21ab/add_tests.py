from pathlib import Path
root=Path('/workspace/work/review-push-preflight-3ef21ab/source')
p=root/'src/repo/push_preflight.rs'
s=p.read_text(); at=s.rfind('\n}')
new=r'''
    #[test]
    fn reviewer_production_dispatch_push_matrix() -> Result<(), String> {
        use crate::repo::{OpKind, ops};
        use crate::repo::operations::run_steps;
        let mut rows = Vec::new();
        for case in ["remote_delete", "both_delete_d", "both_delete_D", "undo_both", "redo_both", "remote_tag_delete", "normal_ff", "publish", "force_lease", "branch_context", "annotated_tag", "detached_tag", "gerrit", "create_then_push_tag"] {
            let f=Fixture::new()?;
            let base=f.receiver_head()?;
            f.git(&["branch", "victim"])?;
            f.git(&["push", "-q", "origin", "victim"])?;
            f.git(&["branch", "--set-upstream-to=origin/victim", "victim"])?;
            let lease=format!("--force-with-lease=refs/heads/victim:{base}");
            let remote_delete=ops(&[&["push", &lease, "origin", "--delete", "refs/heads/victim"]]);
            let both=|flag:&str| {
                let here=ops(&[&["branch", flag, "--", "victim"]]);
                if flag=="-d" { [here,remote_delete.clone()].concat() } else {[remote_delete.clone(),here].concat()}
            };
            let mut kind=OpKind::plain();
            let steps=match case {
                "remote_delete" => remote_delete.clone(),
                "both_delete_d" => both("-d"),
                "both_delete_D" | "redo_both" => both("-D"),
                "undo_both" => {
                    let (result,_)=run_steps(&f.repo,&both("-D"),OpKind::plain(),None); result?;
                    ops(&[&["push","origin",&format!("{base}:refs/heads/victim")], &["branch","--","victim",&base], &["branch","--set-upstream-to=origin/victim","--","victim"]])
                },
                "remote_tag_delete" => {f.git(&["tag","release"])?;f.git(&["push","-q","origin","refs/tags/release"])?;ops(&[&["push","origin","--delete","refs/tags/release"]])},
                "normal_ff" => {f.commit("next","next","Next")?;ops(&[&["push"]])},
                "publish" => {f.git(&["checkout","-qb","newbranch"])?;f.commit("next","next","Next")?;ops(&[&["-c","push.default=simple","push","-u","origin","refs/heads/newbranch"]])},
                "force_lease" => {f.advance_receiver()?;f.git(&["fetch","-q","origin"])?;f.commit("next","next","Next")?;ops(&[&["push","--force-with-lease"]])},
                "branch_context" => {f.git(&["checkout","-qb","feature"])?;f.commit("next","next","Next")?;f.git(&["checkout","-q","main"])?;ops(&[&["push","origin","refs/heads/feature:refs/heads/feature"]])},
                "annotated_tag"|"detached_tag" => {f.git(&["tag","-a","-m","Release","release"])?;if case=="detached_tag" {f.git(&["checkout","-q","--detach"])?;}ops(&[&["push","origin","refs/tags/release"]])},
                "gerrit" => {let head=f.commit("next","next","Next\n\nChange-Id: I0123456789abcdef0123456789abcdef01234567")?;crate::gerrit::review_ops("origin","main","",&head,&base)?},
                "create_then_push_tag" => ops(&[&["tag","-a","-m","Release","release"],&["push","origin","refs/tags/release"]]),
                _=>unreachable!(),
            };
            let ctx=egui::Context::default();
            let mut tab=RepoTab::new_deferred(f.repo.clone(),&ctx);
            if case=="undo_both" {
                let entry=UndoEntry{label:case.into(),undo:steps.clone(),redo:both("-D"),heads:None};
                tab.undo_stack.push(entry);tab.undo(&ctx);
            } else if case=="redo_both" {
                let entry=UndoEntry{label:case.into(),undo:vec![],redo:steps.clone(),heads:None};
                tab.redo_stack.push(entry);tab.redo(&ctx);
            } else {tab.run_ops(case,steps.clone(),Refresh::All,kind.clone(),&ctx);}
            // Run the exact local-tag completion path before its preview.
            if case=="create_then_push_tag" {
                let deadline=Instant::now()+Duration::from_secs(20);
                while tab.busy_op.is_some() && Instant::now()<deadline {
                    if let Ok(Msg::OpDone{label,result,refresh,kind,quiet,..})=tab.rx.recv_timeout(Duration::from_secs(1)) {
                        tab.op_done(label,result,refresh,kind,quiet,&ctx);
                    }
                }
            }
            receive(&mut tab,&ctx)?;
            let dialog=tab.dialog.take();
            let (preview,error,control)=match dialog {
                Some(Dialog::PushPreflight{plan,..})=>{
                    let approved=check::approved_ops(*plan);
                    let (r,_)=run_steps(&f.repo,&approved,OpKind::plain(),None);
                    (true,String::new(),r.err().unwrap_or_default())
                },
                Some(Dialog::PushError(error))=>{
                    let (r,_)=run_steps(&f.repo,&steps,OpKind::plain(),None);
                    (false,error,r.err().unwrap_or_default())
                },
                _=>return Err(format!("{case}: unexpected dialog")),
            };
            rows.push(serde_json::json!({"case":case,"preview":preview,"preview_error":error,"production_dispatch_or_raw_git_error":control}));
            assert!(rows.last().and_then(|r|r["production_dispatch_or_raw_git_error"].as_str()).is_some_and(str::is_empty),"{case}: {:?}",rows.last());
        }
        std::fs::write("/workspace/work/review-push-preflight-3ef21ab/matrix.json",serde_json::to_vec_pretty(&rows).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        Ok(())
    }
'''
p.write_text(s[:at]+new+s[at:])
p=root/'src/push_preflight/tests.rs'
p.write_text(p.read_text()+r'''
#[test]
fn reviewer_rejecting_hook_must_run_before_lfs_upload() -> Result<(),String> {
    use std::os::unix::fs::PermissionsExt;
    let f=Fixture::new()?;
    std::fs::write(f.repo.join("asset.bin"),vec![b'x';6*1024*1024]).map_err(|e|e.to_string())?;
    f.git(&["add","asset.bin"])?;f.git(&["commit","-qm","Asset"])?;
    fix(&f.repo,&f.plan(f.profile())?,"lfs")?;
    let marker=f.root.join("hook-ran");
    let hook=f.repo.join(".git/hooks/pre-push");
    std::fs::write(&hook,format!("#!/bin/sh\ntouch {}\nexit 1\n",crate::remote::sh_quote(&marker.to_string_lossy()))).map_err(|e|e.to_string())?;
    std::fs::set_permissions(&hook,std::fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
    let plan=f.plan(f.profile())?;let before=f.receiver_head()?;
    let result=execute(&f.repo,&plan,None);
    let oid=&plan.lfs_objects[0];
    let stored=f.receiver.join("lfs/objects").join(&oid[..2]).join(&oid[2..4]).join(oid);
    let report=serde_json::json!({"push_failed":result.is_err(),"hook_ran":marker.exists(),"remote_ref_unchanged":f.receiver_head()?==before,"lfs_uploaded_despite_reject":stored.exists()});
    std::fs::write("/workspace/work/review-push-preflight-3ef21ab/hook.json",serde_json::to_vec_pretty(&report).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    assert!(result.is_err());assert!(marker.exists());assert_eq!(f.receiver_head()?,before);assert!(stored.exists());
    Ok(())
}
''')
