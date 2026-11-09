"""Complete unchanged-candidate gates after an unmapped-UID ACL fixture failure."""
from pathlib import Path
import os,sys,subprocess,json,tempfile,hashlib,re,datetime
repo=Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb');os.chdir(repo)
prior_path=Path(sys.argv[1]);prior_bytes=prior_path.read_bytes();prior=json.loads(prior_bytes);sha=prior['commit'];tree=prior['tree']
root=Path(tempfile.mkdtemp(prefix='gitcraft-pr184-complete-'+sha[:8]+'-'))
def digest(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def state():
    def git(*args):return subprocess.check_output(['git',*args],text=True).strip()
    return dict(head=git('rev-parse','HEAD'),tree=git('rev-parse','HEAD^{tree}'),status=git('status','--porcelain=v1','--untracked-files=all'))
clean=dict(head=sha,tree=tree,status='');runner_hash=digest(__file__)
full={k:v for k,v in prior.items() if k not in ['checks','fatal','finished','status','final_state']}
full.update(runner_sha256=runner_hash,resumed_from=dict(results_path=str(prior_path),results_sha256=hashlib.sha256(prior_bytes).hexdigest(),runner_path='/tmp/gitcraft-pr184-final-run.py',runner_sha256=prior['runner_sha256'],reason='PID wrapper maps only one UID; ACL-save fixture needs a named entry for UID+1 and fails EINVAL inside that user namespace.'),checks=[],failed_attempts=[x for x in prior['checks'] if x.get('exit')!=0],completion_started=datetime.datetime.now(datetime.timezone.utc).isoformat(),execution_description='Twelve v2 gates inherited after hash/state verification; four completion gates executed. ACL metadata fixtures run natively; no library or other compiled-suite rerun.')
def save():(root/'results.json').write_text(json.dumps(full,indent=2)+'\n')
def require(ok,message):
    if not ok:full['fatal']=message;save();raise SystemExit(message)
print(root,flush=True)
require(sha=='1eee7e703ce629441eee166b2718021394c9fe09' and state()==clean,'Unexpected or dirty candidate')
expected_prior={'fmt','check','clippy','build-tests','recording-outcomes','recording-resolver','runtime-gitcraft_os-lib','runtime-gitcraft-lib','runtime-gitcraft-main','runtime-checkpoint_cli-checkpoint_cli','runtime-mcp_stdio-mcp_stdio','runtime-standalone_cli-standalone_cli'}
passed=[x for x in prior['checks'] if x.get('exit')==0]
require(len(passed)==12 and {x['label'] for x in passed}==expected_prior,'Expected exactly twelve completed successful gates')
require(len(full['failed_attempts'])==1 and full['failed_attempts'][0]['label']=='acl-save','Unexpected prior failure')
require(digest(full['resumed_from']['runner_path'])==prior['runner_sha256'],'Prior executed runner changed')
for entry in prior['checks']:require(digest(entry['log'])==entry['sha256'],'Prior log changed: '+entry['label'])
for entry in passed:
    require(entry['before']==entry['after']==clean,'Prior gate source state differs')
    full['checks'].append(dict(entry,runner_sha256=prior['runner_sha256'],inherited_from_results_sha256=full['resumed_from']['results_sha256']))
artifacts={x['executable']:x['sha256'] for x in full['artifacts']};app=full['application'];library=next(x['executable'] for x in full['artifacts'] if x['source']=='src/lib.rs')
def binaries_unchanged():return all(digest(path)==expected for path,expected in artifacts.items()) and digest(app['executable'])==app['sha256']
require(binaries_unchanged(),'Previously tested executable changed')
env=dict(os.environ,**prior['environment']);save()
def runtime_env():
    scratch=tempfile.mkdtemp(prefix='gitcraft-pr184-completion-runtime-');return dict(env,TMPDIR=scratch,XDG_CONFIG_HOME=scratch+'/config',XDG_DATA_HOME=scratch+'/data',XDG_CACHE_HOME=scratch+'/cache')
def contained(args):return ['bwrap','--unshare-pid','--die-with-parent','--bind','/','/','--dev','/dev','--proc','/proc','--',*args]
def run(label,args,binary=None,test=None,native_acl=False):
    entry=dict(label=label,command=args,before=state(),runner_sha256=runner_hash,containment='native-owned-ACL-metadata-fixture' if native_acl else 'PID/proc/dev namespace' if binary else 'cargo-doc-tests')
    full['checks'].append(entry);save();require(entry['before']==clean,'Candidate changed before '+label)
    if binary:
        entry.update(binary_sha256_before=digest(binary),application_sha256_before=digest(app['executable']))
        require(entry['binary_sha256_before']==artifacts[binary] and entry['application_sha256_before']==app['sha256'],'Executable changed before '+label)
    log=root/(label+'.log')
    with log.open('w') as output:entry['exit']=subprocess.run(args,env=runtime_env(),stdout=output,stderr=subprocess.STDOUT).returncode
    entry.update(log=str(log),sha256=digest(log),after=state())
    entry['test_summaries']=[dict(zip(['passed','failed','ignored','measured','filtered'],map(int,m))) for m in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;',log.read_text())]
    if binary:entry.update(binary_sha256_after=digest(binary),application_sha256_after=digest(app['executable']))
    save();require(entry['exit']==0 and entry['after']==clean,'Gate failed or source changed: '+label)
    if binary:
        require(entry['binary_sha256_before']==entry['binary_sha256_after']==artifacts[binary] and entry['application_sha256_before']==entry['application_sha256_after']==app['sha256'],'Executable changed during '+label)
        require(len(entry['test_summaries'])==1 and entry['test_summaries'][0]['passed']==1 and entry['test_summaries'][0]['failed']==0 and entry['test_summaries'][0]['ignored']==0,'Expected one successful test: '+label)
        require(re.search(r'^test '+re.escape(test)+r' \.\.\. ok$',log.read_text(),re.M) is not None,'Exact test did not execute: '+label)
    print(label,entry['exit'],entry['test_summaries'],flush=True)
for label,test in [('acl-save','standalone::save::tests::acl_only_tightening_during_each_save_boundary_refuses_stale_policy_and_retains_recovery'),('acl-index','git::file_comparison::index_transaction::tests::index_owner_comparison_and_real_group_drift_refuse_stale_publication')]:run(label,[library,test,'--exact','--test-threads=1'],binary=library,test=test,native_acl=True)
test='app::palette::tests::thousands_of_branches_open_and_filter_quickly';run('palette-isolated',contained([library,test,'--exact','--test-threads=1']),binary=library,test=test)
run('doc-tests',['cargo','test','--locked','--doc'])
require(binaries_unchanged(),'A previously tested executable changed during completion')
full['final_state']=state();require(full['final_state']==clean and len(full['checks'])==16,'Final state or gate inventory invalid')
full['finished']=datetime.datetime.now(datetime.timezone.utc).isoformat();full['status']='passed';save()
