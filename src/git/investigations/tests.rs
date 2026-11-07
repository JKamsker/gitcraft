use super::{views::*,*};
use std::{path::PathBuf,process::Command};
pub(crate) struct Repo(pub PathBuf);
impl Repo{
 pub fn new()->Result<Self,String>{let n=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();let r=Self(std::env::temp_dir().join(format!("gitcraft-graph-views-{}-{n}",std::process::id())));std::fs::create_dir(&r.0).map_err(|e|e.to_string())?;for args in [vec!["init","-q","-b","main"],vec!["config","user.name","View tester"],vec!["config","user.email","me[exact]@test.invalid"],vec!["config","commit.gpgSign","false"]]{r.git(&args)?;}Ok(r)}
 pub fn git(&self,args:&[&str])->Result<String,String>{let o=Command::new("git").arg("-C").arg(&self.0).args(args).output().map_err(|e|e.to_string())?;if o.status.success(){Ok(String::from_utf8_lossy(&o.stdout).trim_end().into_owned())}else{Err(String::from_utf8_lossy(&o.stderr).into_owned())}}
 pub fn commit(&self,email:&str,time:i64)->Result<String,String>{let date=format!("@{time} +0000");let o=Command::new("git").arg("-C").arg(&self.0).args(["commit","-q","--allow-empty","-m","fixture"]).env("GIT_AUTHOR_EMAIL",email).env("GIT_AUTHOR_DATE",&date).env("GIT_COMMITTER_DATE",&date).output().map_err(|e|e.to_string())?;if !o.status.success(){return Err(String::from_utf8_lossy(&o.stderr).into_owned());}self.git(&["rev-parse","HEAD"])}
}
impl Drop for Repo{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn p(kind:Kind)->Preset{Preset{name:"View".into(),kind,days:1}}
#[test]
fn all_four_relations_use_pinned_merge_ancestry_exact_authors_and_stay_read_only()->Result<(),String>{
 let r=Repo::new()?;let me="me[exact]@test.invalid";let base=r.commit(me,1_700_000_000)?;r.git(&["branch","feature"])?;let main=r.commit("someone@test.invalid",1_700_500_000)?;r.git(&["checkout","-q","feature"])?;let feature=r.commit(me,1_700_590_000)?;r.git(&["checkout","-q","main"])?;r.git(&["merge","--no-ff","-qm","merge","feature"])?;let merge=r.git(&["rev-parse","HEAD"])?;
 let scope=Scope{tips:vec![merge.clone()],remotes:vec![base.clone()],email:me.into(),default_branch:Some(("main".into(),merge.clone())),now:1_700_600_000};
 let before=r.git(&["show-ref"])?;let index=std::fs::read(r.0.join(".git/index")).unwrap_or_default();
 assert_eq!(query(&r.0,&p(Kind::MyUnpushed),&scope,&||true)?.commits,BTreeSet::from([feature.clone(),merge.clone()]));
 assert_eq!(query(&r.0,&p(Kind::RecentAncestors),&scope,&||true)?.commits,BTreeSet::from([base.clone(),main.clone(),feature.clone(),merge.clone()]));
 assert_eq!(query(&r.0,&p(Kind::Descendants{anchor:feature.clone()}),&scope,&||true)?.commits,BTreeSet::from([feature,merge.clone()]));
 assert_eq!(query(&r.0,&p(Kind::MergesInto{branch:"main".into()}),&scope,&||true)?.commits,BTreeSet::from([merge.clone()]));
 assert_eq!(before,r.git(&["show-ref"])?);assert_eq!(index,std::fs::read(r.0.join(".git/index")).unwrap_or_default());
 r.git(&["update-ref","refs/heads/main",&base])?;
 assert_eq!(query(&r.0,&p(Kind::MergesInto{branch:"main".into()}),&scope,&||true)?.commits,BTreeSet::from([merge]));
 let page=super::page(&r.0,&scope.tips,0,git::LOG_PAGE,&||true)?;assert_eq!(page.len(),4);assert!(page.iter().any(|c|c.hash==main));Ok(())
}
#[test]
fn cancellation_empty_scope_invalid_ids_and_stale_branch_names_are_explicit()->Result<(),String>{let r=Repo::new()?;let head=r.commit("me@test.invalid",1_700_000_000)?;let scope=Scope{tips:vec![head],remotes:vec![],email:"me@test.invalid".into(),default_branch:None,now:1_700_000_000};assert!(query(&r.0,&p(Kind::MyUnpushed),&scope,&||false).is_err());assert!(query(&r.0,&p(Kind::MergesInto{branch:"main".into()}),&scope,&||true).is_err());let empty=Scope{tips:vec![],..scope.clone()};assert!(query(&r.0,&p(Kind::MyUnpushed),&empty,&||true)?.commits.is_empty());let invalid=Scope{tips:vec!["--all".into()],..scope};assert!(query(&r.0,&p(Kind::MyUnpushed),&invalid,&||true).is_err());assert!(super::page(&r.0,&invalid.tips,0,1,&||true).is_err());Ok(())}
#[cfg(unix)]
#[test]
fn shallow_partial_views_and_pages_do_not_download_or_run_configured_helpers()->Result<(),String>{
 use std::os::unix::fs::PermissionsExt;let r=Repo::new()?;std::fs::write(r.0.join("file ü '$(quoted)"),b"raw blob").map_err(|e|e.to_string())?;r.git(&["add","."])?;r.commit("me@test.invalid",1_700_000_000)?;let head=r.commit("me@test.invalid",1_700_000_001)?;let blob=r.git(&["rev-parse","HEAD:file ü '$(quoted)"])?;r.git(&["config","uploadpack.allowFilter","true"])?;
 let path=r.0.join(".git/partial");let dest=path.to_str().ok_or("path")?;r.git(&["clone","--no-local","--filter=blob:none","--depth=1","--no-checkout","--",r.0.to_str().ok_or("path")?,dest])?;
 let marker=r.0.join(".git/helper-ran");let helper=r.0.join(".git/helper");std::fs::write(&helper,format!("#!/bin/sh\ntouch {}\nexit 1\n",crate::remote::sh_quote(marker.to_str().ok_or("path")?))).map_err(|e|e.to_string())?;std::fs::set_permissions(&helper,std::fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
 git::run(Some(&path),&["config","remote.origin.uploadpack",helper.to_str().ok_or("path")?])?;git::run(Some(&path),&["config","core.fsmonitor",helper.to_str().ok_or("path")?])?;
 let before=git::run(Some(&path),&["count-objects","-v"])?;let refs=git::run(Some(&path),&["show-ref"])?;
 let scope=Scope{tips:vec![head],remotes:vec![],email:"me@test.invalid".into(),default_branch:None,now:1_700_000_002};assert_eq!(query(&path,&p(Kind::MyUnpushed),&scope,&||true)?.commits.len(),1);assert_eq!(super::page(&path,&scope.tips,0,git::LOG_PAGE,&||true)?.len(),1);
 let promised=Scope{tips:vec![blob.clone()],..scope};assert!(query(&path,&p(Kind::MyUnpushed),&promised,&||true).is_err());assert!(!marker.exists());assert_eq!(before,git::run(Some(&path),&["count-objects","-v"])?);assert_eq!(refs,git::run(Some(&path),&["show-ref"])?);assert!(!path.join(".git/index").exists());
 assert!(git::run(Some(&path),&["cat-file","-e",&blob]).is_err());assert!(marker.exists(),"Positive control must call configured fetch helper");Ok(())
}
