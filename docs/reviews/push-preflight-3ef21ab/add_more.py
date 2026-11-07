from pathlib import Path
root=Path('/workspace/work/review-push-preflight-3ef21ab/source')
p=root/'src/repo/push_preflight.rs';s=p.read_text();at=s.rfind('\n}')
p.write_text(s[:at]+r'''
    #[test]
    fn reviewer_failed_ref_lock_repair_changes_worktree_without_undo() -> Result<(),String> {
        use crate::repo::operations::run_steps;
        let f=Fixture::new()?;
        let text="api_key = 'THIS_IS_A_DUMMY_FIXTURE_VALUE_123456789'\n";
        f.commit("credential.txt",text,"Accidental credential")?;
        let before=f.git(&["rev-parse","HEAD"])?;
        let plan=f.plan(f.profile())?;
        assert!(plan.findings.iter().any(|i|i.kind==Kind::Secret));
        std::fs::write(f.repo.join(".git/refs/heads/main.lock"),"another Git process").map_err(|e|e.to_string())?;
        let mut step=check::register(plan);step[0]=check::FIX.into();step.push("secrets".into());
        let kind=super::super::OpKind::Normal(Undo::HeadMove{mode:"--merge"},String::new());
        let (result,kind)=run_steps(&f.repo,&[step],kind,None);
        let ctx=egui::Context::default();let mut tab=RepoTab::new_deferred(f.repo.clone(),&ctx);
        tab.op_done("Repair history".into(),result.clone(),Refresh::All,kind,false,&ctx);
        let report=serde_json::json!({"repair_failed":result.is_err(),"head_unchanged":f.git(&["rev-parse","HEAD"])?==before,"work_file_removed":!f.repo.join("credential.txt").exists(),"status":f.git(&["status","--porcelain"])? ,"undo_entries":tab.undo_stack.len()});
        std::fs::write("/workspace/work/review-push-preflight-3ef21ab/repair-lock.json",serde_json::to_vec_pretty(&report).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        assert!(result.is_err());assert_eq!(f.git(&["rev-parse","HEAD"])?,before);assert!(!f.repo.join("credential.txt").exists());assert!(tab.undo_stack.is_empty());
        Ok(())
    }
''' + s[at:])
p=root/'src/push_preflight/tests.rs'
p.write_text(p.read_text()+r'''
#[test]
fn reviewer_default_https_port_identity_policy() -> Result<(),String> {
    let f=Fixture::new()?;f.commit("next","next","Next")?;
    let mut profile=f.profile();profile.author_hosts.push(crate::settings::profiles::HostAuthor{host:"github.com/personal".into(),name:"Personal".into(),email:"personal@example.org".into()});
    let mut rows=Vec::new();
    for url in ["https://github.com/personal/repo.git","https://github.com:443/personal/repo.git"] {
        f.git(&["remote","set-url","origin",url])?;
        let plan=f.plan(profile.clone())?;
        rows.push(serde_json::json!({"url":url,"expected_email":plan.expected_email,"identity_findings":plan.findings.iter().filter(|i|i.kind==Kind::Identity).count()}));
    }
    std::fs::write("/workspace/work/review-push-preflight-3ef21ab/identity-port.json",serde_json::to_vec_pretty(&rows).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    assert!(rows[0]["identity_findings"].as_u64().is_some_and(|n|n>0));assert_eq!(rows[1]["identity_findings"],0);
    Ok(())
}
''')
