"""Final immutable-candidate gates. Run only after the candidate is committed and pushed."""
from pathlib import Path
import os, subprocess, json, tempfile, hashlib, re, sys, datetime
repo = Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb')
os.chdir(repo)
if len(sys.argv) != 2 or not re.fullmatch(r'[0-9a-f]{40}', sys.argv[1]):
    raise SystemExit('Pass the intended full candidate SHA explicitly')
sha = sys.argv[1]
base = Path(tempfile.mkdtemp(prefix='gitcraft-pr184-round5-full-' + sha[:8] + '-'))
env = dict(os.environ, PKG_CONFIG_PATH='/tmp/gitcraft-pr179-build-deps/root/usr/lib/x86_64-linux-gnu/pkgconfig', CARGO_TARGET_DIR=str(repo/'target'), CARGO_BUILD_JOBS='4', CARGO_PROFILE_TEST_DEBUG='0', PATH='/tmp/gitcraft-git252/install/bin:/tmp/gitcraft-lfs/root/usr/bin:'+os.environ['PATH'], GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_NOSYSTEM='1')
def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()
def digest(path):
    return hashlib.file_digest(Path(path).open('rb'), 'sha256').hexdigest()
source_checker_bytes = Path(__file__).with_name('gitcraft_pr184_source_integrity.py').read_bytes()
source_namespace = {}
exec(compile(source_checker_bytes, 'gitcraft_pr184_source_integrity.py', 'exec'), source_namespace)
manifest = dict(source_checker_sha256=hashlib.sha256(source_checker_bytes).hexdigest(), source_observations=[], commit=sha, tree=git('rev-parse', sha+'^{tree}'), started=datetime.datetime.now(datetime.timezone.utc).isoformat(), runner_sha256=digest(__file__), checks=[], artifacts=[], environment={k:env[k] for k in ['PKG_CONFIG_PATH','CARGO_TARGET_DIR','CARGO_BUILD_JOBS','CARGO_PROFILE_TEST_DEBUG','PATH','GIT_CONFIG_GLOBAL','GIT_CONFIG_NOSYSTEM']}, uid=os.getuid(), gid=os.getgid(), supplementary_groups=os.getgroups(), acl_group_drift_coverage='Conditional within test; a passing summary alone does not establish the real supplementary-group branch ran.')
for cmd in [['rustc','--version'],['cargo','--version'],['git','--version'],['bwrap','--version']]:
    manifest.setdefault('versions', []).append(dict(command=cmd, output=subprocess.check_output(cmd, env=env, text=True).strip()))
def save():
    (base/'results.json').write_text(json.dumps(manifest, indent=2)+'\n')
def state():
    observation = source_namespace['inspect_source'](repo, sha)
    manifest['source_observations'].append(observation)
    status = git('status','--porcelain=v1','--untracked-files=all')
    if Path(__file__).with_name('gitcraft_pr184_source_integrity.py').read_bytes() != source_checker_bytes:
        observation['errors'].append('Source checker changed after execution snapshot')
    if observation['errors']:
        status += '\n' + '\n'.join(observation['errors'])
    return dict(head=git('rev-parse','HEAD'), tree=git('rev-parse','HEAD^{tree}'), status=status)

def require(condition, message):
    if not condition:
        manifest['fatal']=message
        save()
        raise SystemExit(message)
def clean(observed):
    return observed == dict(head=sha, tree=manifest['tree'], status='')
def contained(args):
    return ['bwrap','--unshare-pid','--die-with-parent','--bind','/','/','--dev','/dev','--proc','/proc','--', *args]
def runtime_env():
    scratch=tempfile.mkdtemp(prefix='gitcraft-pr184-final-runtime-')
    return dict(env, TMPDIR=scratch, XDG_CONFIG_HOME=scratch+'/config', XDG_DATA_HOME=scratch+'/data', XDG_CACHE_HOME=scratch+'/cache')
def run(label, args, custom_env=None, binary=None, expected_passed=None, minimum_passed=None, exact_test=None):
    entry=dict(label=label, command=args, before=state())
    manifest['checks'].append(entry)
    save()
    require(clean(entry['before']), 'Candidate dirty or changed before '+label)
    if binary and manifest.get('application'):
        entry['application_sha256_before']=digest(manifest['application']['executable'])
        require(entry['application_sha256_before']==manifest['application']['sha256'], 'Application binary changed before '+label)
    if binary:
        entry['binary_sha256_before']=digest(binary)
        require(entry['binary_sha256_before']==binary_hashes[binary], 'Binary changed before '+label)
    log=base/(label+'.log')
    try:
        with log.open('w') as out:
            entry['exit']=subprocess.run(args, env=custom_env or env, stdout=out, stderr=subprocess.STDOUT).returncode
    except Exception as error:
        entry['exception']=str(error)
        entry['log']=str(log)
        if log.exists():
            entry['sha256']=digest(log)
        entry['after']=state()
        manifest['fatal']='Gate launch failed: '+label
        save()
        raise
    entry.update(log=str(log), sha256=digest(log), after=state())
    text=log.read_text()
    counts=[dict(zip(['passed','failed','ignored','measured','filtered'], map(int, match))) for match in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;', text)]
    entry['test_summaries']=counts
    if binary and manifest.get('application'):
        entry['application_sha256_after']=digest(manifest['application']['executable'])
    if binary:
        entry['binary_sha256_after']=digest(binary)
    save()
    require(clean(entry['after']), 'Candidate dirty or changed during '+label)
    if binary:
        require(entry['binary_sha256_after']==binary_hashes[binary], 'Binary changed during '+label)
    if binary and manifest.get('application'):
        require(entry['application_sha256_after']==manifest['application']['sha256'], 'Application binary changed during '+label)
    require(entry['exit']==0, 'Gate failed: '+label)
    if expected_passed is not None:
        require(len(counts)==1 and counts[0]['passed']==expected_passed and counts[0]['failed']==0 and counts[0]['ignored']==0, 'Unexpected test count: '+label)
    if minimum_passed is not None:
        require(len(counts)==1 and counts[0]['passed']>=minimum_passed and counts[0]['failed']==0, 'Missing test summary or insufficient count: '+label)
    if exact_test:
        require(re.search(r'^test '+re.escape(exact_test)+r' \.\.\. ok$', text, re.M) is not None, 'Exact test did not run: '+label)
    print(label, entry['exit'], counts, flush=True)
print(str(base), flush=True)
require(clean(state()), 'Expected candidate must be checked out clean')
for label,args in [('fmt',['cargo','fmt','--all','--','--check']),('check',['cargo','check','--locked','--all-targets']),('clippy',['cargo','clippy','--locked','--all-targets','--','-D','warnings']),('build-tests',['cargo','test','--locked','--no-run','--message-format=json'])]:
    run(label,args)
expected={('gitcraft_os',('lib',),'crates/gitcraft-os/src/lib.rs'),('gitcraft',('lib',),'src/lib.rs'),('gitcraft',('bin',),'src/main.rs'),('checkpoint_cli',('test',),'tests/checkpoint_cli.rs'),('mcp_stdio',('test',),'tests/mcp_stdio.rs'),('standalone_cli',('test',),'tests/standalone_cli.rs')}
applications=[]
for line in (base/'build-tests.log').read_text().splitlines():
    try: item=json.loads(line)
    except ValueError: continue
    if item.get('reason')=='compiler-artifact' and item.get('executable') and not item.get('profile',{}).get('test') and item['target']['name']=='gitcraft' and item['target']['kind']==['bin'] and Path(item['target']['src_path'])==repo/'src/main.rs':
        applications.append(dict(executable=item['executable'],sha256=digest(item['executable'])))
    if item.get('reason')=='compiler-artifact' and item.get('executable') and item.get('profile',{}).get('test'):
        target=item['target']; src=str(Path(target['src_path']).relative_to(repo)); executable=item['executable']
        manifest['artifacts'].append(dict(name=target['name'], kind=target['kind'], source=src, executable=executable, sha256=digest(executable)))
require(len(applications)==1, 'Expected exactly one non-test application binary')
manifest['application']=applications[0]
actual=[(x['name'],tuple(x['kind']),x['source']) for x in manifest['artifacts']]
save()
require(len(actual)==6 and set(actual)==expected, 'Unexpected test artifact inventory: '+repr(actual))
binary_hashes={x['executable']:x['sha256'] for x in manifest['artifacts']}
require(len(binary_hashes)==6, 'Duplicate test executables')
library=next(x['executable'] for x in manifest['artifacts'] if x['source']=='src/lib.rs')
for label,pattern,count in [('recording-outcomes','processes::recording_outcomes',3),('recording-resolver','resolver::process::tests::recording_failure_mapping_and_join_proof_fail_closed',1)]:
    run(label,contained([library,pattern,'--test-threads=1']), runtime_env(), binary=library, expected_passed=count)
special=[('acl-save','standalone::save::tests::acl_only_tightening_during_each_save_boundary_refuses_stale_policy_and_retains_recovery'),('acl-index','git::file_comparison::index_transaction::tests::index_owner_comparison_and_real_group_drift_refuse_stale_publication'),('palette-isolated','app::palette::tests::thousands_of_branches_open_and_filter_quickly')]
for item in manifest['artifacts']:
    binary=item['executable']; label='runtime-'+item['name']+('-lib' if item['source'].endswith('/lib.rs') else '-'+Path(item['source']).stem)
    args=[binary,'--test-threads=4']
    if binary==library:
        for _, test in special: args+=['--skip',test]
    run(label,contained(args),runtime_env(),binary=binary,minimum_passed=0 if item['source']=='src/main.rs' else 1)
for label,test in special:
    args=[library,test,'--exact','--test-threads=1']
    run(label,args if label in ['acl-save','acl-index'] else contained(args),runtime_env(),binary=library,expected_passed=1,exact_test=test)
    manifest['checks'][-1]['containment']='native-owned-ACL-metadata-fixture' if label in ['acl-save','acl-index'] else 'PID/proc/dev namespace'
    save()
run('doc-tests',['cargo','test','--locked','--doc'])
manifest['finished']=datetime.datetime.now(datetime.timezone.utc).isoformat()
manifest['final_state']=state()
require(clean(manifest['final_state']), 'Candidate dirty or changed at final observation')
manifest['status']='passed'
save()
