"""Archive one completed v2 full run and independently approved immutable evidence."""
from pathlib import Path
import sys,os,subprocess,json,hashlib,gzip,re,tempfile,shutil
repo=Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb');os.chdir(repo)
if len(sys.argv)!=5:raise SystemExit('Usage: packager full-results assembly reviews executed-runner')
full_path,assembly_path,reviews_path,runner_path=map(Path,sys.argv[1:])
snapshots={}
def snapshot(path):
    key=Path(path).absolute()
    if key not in snapshots:
        snapshots[key]=key.read_bytes()
    return snapshots[key]
full_bytes=snapshot(full_path);assembly_bytes=snapshot(assembly_path);reviews_bytes=snapshot(reviews_path);runner_bytes=snapshot(runner_path)
full=json.loads(full_bytes);assembly=json.loads(assembly_bytes);reviews=json.loads(reviews_bytes);sha=full['commit'];tree=full['tree']
def git(*args):return subprocess.check_output(['git',*args],text=True).strip()
def digest(data):return hashlib.sha256(data).hexdigest()
def require(ok,why):
    if not ok:raise SystemExit(why)
clean=dict(head=sha,tree=tree,status='')
require(git('rev-parse','HEAD')==sha and not git('status','--porcelain=v1','--untracked-files=all'),'Tested candidate must be checked out cleanly')
labels={'fmt','check','clippy','build-tests','recording-outcomes','recording-resolver','runtime-gitcraft_os-lib','runtime-gitcraft-lib','runtime-gitcraft-main','runtime-checkpoint_cli-checkpoint_cli','runtime-mcp_stdio-mcp_stdio','runtime-standalone_cli-standalone_cli','acl-save','acl-index','palette-isolated','doc-tests'}
require(full.get('status')=='passed' and full.get('final_state')==clean,'Full run did not finish cleanly')
require(len(full['checks'])==16 and {x['label'] for x in full['checks']}==labels,'Expected exactly sixteen unique gates')
require(all(x['exit']==0 and x['before']==clean and x['after']==clean for x in full['checks']),'A gate failed or changed candidate state')
require(digest(runner_bytes)==full['runner_sha256'],'Executed runner changed')
prior_v2_path=Path(full['resumed_from']['results_path']);prior_v2_bytes=snapshot(prior_v2_path);prior_v2=json.loads(prior_v2_bytes)
require(digest(prior_v2_bytes)==full['resumed_from']['results_sha256'],'Initial v2 results changed')
prior_v2_runner=snapshot(full['resumed_from']['runner_path'])
require(digest(prior_v2_runner)==full['resumed_from']['runner_sha256']==prior_v2['runner_sha256'],'Initial v2 runner changed')
prior_passed=[x for x in prior_v2['checks'] if x.get('exit')==0]
require(len(prior_passed)==12,'Expected twelve inherited successful v2 gates')
for original in prior_passed:
    inherited=next((x for x in full['checks'] if x['label']==original['label']),None)
    require(inherited is not None and {k:v for k,v in inherited.items() if k not in ['runner_sha256','inherited_from_results_sha256']}==original,'Inherited gate record changed')
    require(inherited['runner_sha256']==prior_v2['runner_sha256'] and inherited['inherited_from_results_sha256']==digest(prior_v2_bytes),'Inherited gate provenance mismatch')
require(len([x for x in full['checks'] if x['runner_sha256']==full['runner_sha256']])==4,'Expected four completion gates')

require(len(full['artifacts'])==6 and len({x['executable'] for x in full['artifacts']})==6 and full.get('application'),'Harness/application inventory incomplete')
require(assembly['final']==sha and assembly['accepted_tree']==tree and assembly['exact_tree_match'],'Assembly targets another candidate')
require(git('rev-parse',assembly['accepted']+'^{tree}')==tree,'Accepted and tested trees differ')
expected_identity='Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>'
for unit in assembly['units']:
    require(git('show','-s','--format=%an <%ae>',unit['commit'])==expected_identity,'Unexpected unit author')
    require(git('rev-parse',unit['commit']+'^{tree}')==unit['tree'] and unit['remote']==unit['commit'],'Unit provenance mismatch')
    require(set(git('diff-tree','--no-commit-id','--name-only','-r',unit['commit']).splitlines())==set(unit['paths']),'Unit path list differs from immutable commit')
map_bytes=subprocess.check_output(['git','ls-tree','-r','-z','--full-tree',sha]);accepted_map=subprocess.check_output(['git','ls-tree','-r','-z','--full-tree',assembly['accepted']])
require(map_bytes==accepted_map and digest(map_bytes)==assembly['complete_path_mode_blob_map_sha256'],'Complete source maps differ')
exact=[x for x in reviews if x.get('reviewed_commit')==sha and x.get('reviewed_tree')==tree and x.get('verdict')=='PASS' and x.get('task_id') and x.get('reviewer')]
source=[x for x in exact if x.get('scope')=='source'];execution=[x for x in exact if x.get('scope')=='execution_evidence' and x.get('full_results_sha256')==digest(full_bytes) and x.get('executed_runner_sha256')==full['runner_sha256']]
require(source and execution and any(a['task_id']!=b['task_id'] for a in source for b in execution),'Distinct exact-candidate source and evidence approvals required')
verifier_bytes=snapshot('/tmp/gitcraft-pr184-inheritance.py')
require(any(x.get('scope')=='inheritance_verifier' and x.get('script_sha256')==digest(verifier_bytes) for x in exact),'Exact reviewed inheritance verifier approval required')
expected_artifacts={('gitcraft_os',('lib',),'crates/gitcraft-os/src/lib.rs'),('gitcraft',('lib',),'src/lib.rs'),('gitcraft',('bin',),'src/main.rs'),('checkpoint_cli',('test',),'tests/checkpoint_cli.rs'),('mcp_stdio',('test',),'tests/mcp_stdio.rs'),('standalone_cli',('test',),'tests/standalone_cli.rs')}
require({(x['name'],tuple(x['kind']),x['source']) for x in full['artifacts']}==expected_artifacts,'Unexpected harness identities')
app=full['application'];library=next(x for x in full['artifacts'] if x['source']=='src/lib.rs')
bindings={}
for artifact in full['artifacts']:
    label='runtime-'+artifact['name']+('-lib' if artifact['source'].endswith('/lib.rs') else '-'+Path(artifact['source']).stem)
    bindings[label]=artifact
for label in ['recording-outcomes','recording-resolver','acl-save','acl-index','palette-isolated']:bindings[label]=library
require(len(bindings)==11,'Expected eleven gates with executable bindings')
prefix=['bwrap','--unshare-pid','--die-with-parent','--bind','/','/','--dev','/dev','--proc','/proc','--']
for entry in full['checks']:
    data=snapshot(entry['log']);require(digest(data)==entry['sha256'],'Changed raw log '+entry['label'])
    if entry['label'] in bindings:
        artifact=bindings[entry['label']]
        if entry['label'] in ['acl-save','acl-index']:
            require(entry.get('containment')=='native-owned-ACL-metadata-fixture' and entry['command'][0]==artifact['executable'],'Wrong native ACL fixture command '+entry['label'])
        else:
            require(entry['command'][:len(prefix)]==prefix and len(entry['command'])>len(prefix) and entry['command'][len(prefix)]==artifact['executable'],'Wrong command executable '+entry['label'])
        require(entry.get('binary_sha256_before')==entry.get('binary_sha256_after')==artifact['sha256'],'Missing or wrong harness binding '+entry['label'])
        require(entry.get('application_sha256_before')==entry.get('application_sha256_after')==app['sha256'],'Missing or wrong application binding '+entry['label'])
for artifact in full['artifacts']:
    require(digest(Path(artifact['executable']).read_bytes())==artifact['sha256'],'Current harness differs from tested one')
require(digest(Path(app['executable']).read_bytes())==app['sha256'],'Current application differs from tested one')
root=repo/'docs/validation/pr184-followups-20261109';require(not root.exists(),'Preserve an existing evidence package')
pending={}
manifest=dict(tested_commit=sha,tested_tree=tree,accepted_commit=assembly['accepted'],full_run_runner='v2-plus-native-ACL-completion',full_gate_count=16,application=app,complete_path_mode_blob_map_sha256=digest(map_bytes),execution_description='Twelve successful v2 gates plus four completion gates on the unchanged candidate. ACL metadata fixtures run natively after the one-UID namespace rejected their named-UID ACL. The library and executable suites were not rerun.',files=[],logs=[],inheritance_exclusion='docs/validation/pr184-followups-20261109/',limitations=['Task scheduling exclusion is per pool/process.','Remote mutation tests are synthetic, not live SSH/WSL validation.','Inert UI tests do not prove native picker or screen-reader integration.','Real supplementary-group drift is conditional within the ACL test; a passing summary alone does not establish that branch executed.','Conservative blame and line-history invalidation remains; display continuity and narrower invalidation are deferred UX/performance improvements.'])
def put(relative,data):
    require(relative not in pending,'Duplicate archive path '+relative)
    pending[relative]=data;manifest['files'].append(dict(path=relative,sha256=digest(data)))
def archive_logs(prefix,results):
    for entry in results['checks']:
        data=snapshot(entry['log']);require(digest(data)==entry['sha256'],'Changed log '+entry['label'])
        relative='logs/'+prefix+'-'+entry['label']+'.log.gz';packed=gzip.compress(data,mtime=0);put(relative,packed)
        manifest['logs'].append(dict(run=prefix,gate=entry['label'],exit=entry['exit'],path=relative,raw_sha256=digest(data),archive_sha256=digest(packed)))
put('full-results.json',full_bytes);put('assembly.json',assembly_bytes);put('reviews.json',reviews_bytes);put('runners/complete-gates.py',runner_bytes);put('runners/full-run-v2.py',prior_v2_runner);put('runners/assemble-v3.py',snapshot('/tmp/gitcraft-pr184-assemble-v3.py'));put('runners/verify-inheritance.py',snapshot('/tmp/gitcraft-pr184-inheritance.py'));put('runners/package-evidence.py',snapshot(__file__))
put('historical/initial-v2-results.json',prior_v2_bytes)
for failed in full['failed_attempts']:
    data=snapshot(failed['log']);require(digest(data)==failed['sha256'],'Initial failed ACL log changed')
    packed=gzip.compress(data,mtime=0);relative='logs/initial-v2-failed-'+failed['label']+'.log.gz';put(relative,packed);manifest['logs'].append(dict(run='initial-v2-failed',gate=failed['label'],exit=failed['exit'],path=relative,raw_sha256=digest(data),archive_sha256=digest(packed)))
put('tested-path-mode-blob-map.bin.gz',gzip.compress(map_bytes,mtime=0));put('tested-path-mode-blob-map.txt',subprocess.check_output(['git','ls-tree','-r','--full-tree',sha]));archive_logs('full',full)
ignored=[]
for entry in full['checks']:
    for name,reason in re.findall(r'^test (.+?) \.\.\. ignored(?:, (.*))?$',snapshot(entry['log']).decode(),re.M):ignored.append(dict(gate=entry['label'],name=name,reason=reason or 'No reason printed'))
expected_ignored=sum(x['ignored'] for e in full['checks'] for x in e.get('test_summaries',[]))
require(len(ignored)==expected_ignored==24 and len({x['name'] for x in ignored})==24,'Expected exactly twenty-four distinct ignored tests')
for entry in full['checks']:
    require(sum(x['gate']==entry['label'] for x in ignored)==sum(x['ignored'] for x in entry.get('test_summaries',[])),'Ignored enumeration differs for '+entry['label'])
put('ignored-tests.json',(json.dumps(ignored,indent=2)+'\n').encode());manifest['ignored_test_count']=len(ignored)
old_path=Path('/tmp/gitcraft-pr184-final-049a29c0-8lj0ptpc/results.json');old_bytes=snapshot(old_path);old=json.loads(old_bytes);old_lib=next(x for x in old['checks'] if x['label']=='runtime-gitcraft-lib')
require(old['commit']=='049a29c06adf3d1afe8cdd5f2639873c4e8e87d6' and old_lib['exit']!=0,'Historical failure mismatch')
put('historical/failed-049-results.json',old_bytes);old_runner=snapshot('/tmp/gitcraft-pr184-final-run-v1.py');require(digest(old_runner)==old['runner_sha256'],'Historical runner changed');put('historical/full-run-v1.py',old_runner);archive_logs('failed049',old)
rows=[]
for entry in full['checks']:
    counts=entry.get('test_summaries',[]);detail='; '.join(f"{x['passed']} passed, {x['failed']} failed, {x['ignored']} ignored, {x['filtered']} filtered" for x in counts) or 'exit 0';rows.append('| '+entry['label']+' | '+str(entry['exit'])+' | '+detail+' |')
readme=f'''# PR 184 follow-up validation

Tested commit: `{sha}`. Tree: `{tree}`. Accepted source: `{assembly['accepted']}`.

All sixteen gates in [full-results.json](full-results.json) passed on the exact clean candidate. Twelve successful gates came from [v2](runners/full-run-v2.py); [completion](runners/complete-gates.py) verified those records and executable hashes, then ran the remaining four gates. The initial contained ACL-save attempt failed because the namespace mapped only one UID while the fixture names UID+1; its results and failure log are preserved separately. The two ACL metadata fixtures therefore ran natively on owned temporary files. The library and other executable suites ran once and were not repeated. Six test harnesses and the separate application executable were identified from Cargo JSON and hashed before/after runtime gates. Every gate records clean before/after states, raw-log hashes and exact commands; final cleanliness is enforced. No integration-only supplement was executed for this candidate; its separate application executable was hash-checked throughout both stages.

| Gate | Exit | Recorded summary |
|---|---:|---|
'''+ '\n'.join(rows)+f'''

The main harness legitimately has zero tests. Recording tests overlap library coverage and must not be counted as distinct additional tests. The three separately executed ACL/index/palette cases are filtered out of the parallel library gate and each required to pass by exact name with exactly one executed test. [ignored-tests.json](ignored-tests.json) enumerates all {len(ignored)} ignored tests and their recorded reasons; they are separate from those three filtered cases. Documentation-test summaries are preserved in the raw log; no nonzero documentation-test count is claimed.

The compiled runtime suites, recording gates and isolated palette case ran inside a separate PID namespace with private proc/dev mounts. ACL metadata fixtures ran natively because their named UID cannot be represented in that namespace; they do not perform process-control tests. Recording-only safety gates passed before the process test suites. No host-wide signal selection was used. The ACL test's real supplementary-group branch is conditional; a passing summary alone does not establish that branch ran. Execution identity and groups are recorded.

[reviews.json](reviews.json) identifies each reviewer, exact commit/tree, scope and verdict. Distinct independent source and execution-evidence reviews passed for the tested candidate. Reviewers inspected source and parent-produced records; they did not rerun tests unless their record expressly says so.

[assembly.json](assembly.json) binds eight follow-ups descending from published `5988524c23e43d8862225e44b09fbc104a735cc6`: seven feature commits and historical scheduler evidence. Every author matches the required identity. The complete accepted and tested path/mode/blob maps are equal. [The binary map](tested-path-mode-blob-map.bin.gz) decompresses to the exact NUL-delimited bytes whose SHA-256 is recorded; [the readable map](tested-path-mode-blob-map.txt) has its own file hash.

The prior `049a29c0` run is archived separately under `historical/` and `logs/failed049-*`. It failed with 4,952 passed, one failed, 24 ignored and three filtered, stopping at the library gate; later gates did not run. Its sole failure expected unfocused Enter to submit Fold. Accepted source `7346090e` corrected that production-dispatcher test to require explicit primary activation; this candidate's complete run passed. None of the prior failed run's gates is accepted as validation of the changed application.

Historical remote-scheduler archives remain bound to implementation `28e6a8b1`; current `tasks.rs` adds a documented test-module lint allowance. Older original full-suite evidence under `adversarial-review-20261109` remains historical. This package supplies validation for the current changed application.

Limits: scheduling exclusion is per pool/process; remote tests are synthetic; inert egui tests do not prove live SSH/WSL, native-picker or screen-reader behavior. Conservative blame/line-history refresh remains; display continuity and narrower invalidation are deferred UX/performance work.

[manifest.json](manifest.json) records raw and zero-mtime gzip hashes for all logs, hashes all archived files, and binds the tested source map and application binary. Original absolute paths identify the execution machine; use the manifest's log mapping for portable archived files. For example, `gzip -dc logs/full-runtime-gitcraft-lib.log.gz` reproduces the library log.

This evidence-only successor inherits the tested application gates only after a post-commit comparison proves every path/mode/blob outside `docs/validation/pr184-followups-20261109/` is unchanged. The external inheritance verification record names both commit SHAs, both tree IDs and map digests, and requires only additions under the stated prefix; no new execution on that documentation successor is claimed.
'''
put('README.md',readme.encode())
staging=Path(tempfile.mkdtemp(prefix='.pr184-evidence-',dir=root.parent))
try:
    for relative,data in pending.items():
        target=staging/relative;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
    (staging/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    require(not root.exists(),'Preserve a concurrently created evidence package')
    os.rename(staging,root)
finally:
    shutil.rmtree(staging,ignore_errors=True)
print(root)
