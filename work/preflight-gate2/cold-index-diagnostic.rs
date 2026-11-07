//! Owned read-only Git comparison diagnostic; uses the exact approved bounded runner.
#[path = "../../src/bounded_process.rs"]
mod bounded_process;
use std::{fs, path::{Path,PathBuf}, process::Command, time::Duration};
fn copy_tree(source: &Path, target: &Path) -> Result<(),String> {
    fs::create_dir(target).map_err(|e|e.to_string())?;
    for entry in fs::read_dir(source).map_err(|e|e.to_string())? {
        let entry=entry.map_err(|e|e.to_string())?;let ty=entry.file_type().map_err(|e|e.to_string())?;
        if ty.is_symlink(){return Err("Owned diagnostic fixture unexpectedly contained symlink".into());}
        let dst=target.join(entry.file_name());
        if ty.is_dir(){copy_tree(&entry.path(),&dst)?;}else{fs::copy(entry.path(),dst).map_err(|e|e.to_string())?;}
    }
    Ok(())
}
fn compare(repo:&Path,index:&Path,home:&Path,label:&str,patch:bool)->Result<(),String>{
    let mut command=Command::new("/usr/local/bin/git");command.env_clear().env("PATH","/usr/local/bin:/usr/bin:/bin").env("HOME",home).env("XDG_CONFIG_HOME",home).env("GIT_CONFIG_GLOBAL","/dev/null").env("GIT_CONFIG_NOSYSTEM","1").env("GIT_ATTR_NOSYSTEM","1").env("GIT_NO_LAZY_FETCH","1").env("GIT_TERMINAL_PROMPT","0").env("GIT_INDEX_FILE",index)
      .arg("-C").arg(repo).args(["--no-lazy-fetch","-c","core.hooksPath=","-c","core.fsmonitor=false","-c","core.trustctime=true","-c","core.ignoreStat=false","diff-files"]);
    if patch{command.args(["--exit-code","--patch"]);}else{command.arg("--quiet");}
    command.args(["--no-ext-diff","--no-textconv","--ignore-submodules=none","--"]);
    let output=bounded_process::run_input(command,None,&||true,Duration::from_secs(5),1024*1024,1024*1024)?;
    println!("{label}: patch={patch} status={} stdout={:?} stderr={:?}",output.status,String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));Ok(())
}
fn main()->Result<(),String>{
    let original=PathBuf::from("/workspace/work/push-preflight-gate2-production-files-sha1");
    let repo=original.join("repository ' quoted \\ path");
    let evidence=fs::read_dir(original.join("tmp")).map_err(|e|e.to_string())?.find_map(|e|e.ok().filter(|e|e.file_name().to_string_lossy().starts_with("gitcraft-push-ref-transaction-")).map(|e|e.path())).ok_or("owned evidence not found")?;
    let root=PathBuf::from("/workspace/work/push-preflight-cold-index-diagnostic");fs::create_dir(&root).map_err(|e|e.to_string())?;
    let home=root.join("home");fs::create_dir(&home).map_err(|e|e.to_string())?;
    let before=fs::read(repo.join("file.txt")).map_err(|e|e.to_string())?;
    let actual=fs::read(repo.join(".git/index")).map_err(|e|e.to_string())?;
    println!("actual_index_equals_saved={}",actual==fs::read(evidence.join("before.index")).map_err(|e|e.to_string())?);
    for patch in [false,true]{compare(&repo,&evidence.join("check-before.index"),&home,"original_clean",patch)?;}
    assert_eq!(before,fs::read(repo.join("file.txt")).map_err(|e|e.to_string())?);
    assert_eq!(actual,fs::read(repo.join(".git/index")).map_err(|e|e.to_string())?);
    let copied=root.join("copied-repo");copy_tree(&repo,&copied)?;
    let index=root.join("check-before.index");fs::copy(evidence.join("check-before.index"),&index).map_err(|e|e.to_string())?;
    for patch in [false,true]{compare(&copied,&index,&home,"copied_clean",patch)?;}
    fs::write(copied.join("file.txt"),b"BEFORE\n").map_err(|e|e.to_string())?;
    for patch in [false,true]{compare(&copied,&index,&home,"copied_changed_same_length",patch)?;}
    Ok(())
}
