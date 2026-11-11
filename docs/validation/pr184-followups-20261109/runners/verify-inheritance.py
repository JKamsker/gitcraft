"""Verify a committed evidence-only successor without claiming new test execution."""
from pathlib import Path
import sys,subprocess,json,hashlib,re
repo=Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb')
if len(sys.argv)!=3 or any(not re.fullmatch('[0-9a-f]{40}',x) for x in sys.argv[1:]):raise SystemExit('Pass full tested and evidence commit SHAs')
tested,successor=sys.argv[1:];
if tested!='1eee7e703ce629441eee166b2718021394c9fe09':raise SystemExit('Unexpected tested candidate')
prefix=b'docs/validation/pr184-followups-20261109/'
def git(*args):return subprocess.check_output(['git',*args],cwd=repo)
def value(*args):return git(*args).decode().strip()
def digest(data):return hashlib.sha256(data).hexdigest()
def require(ok,message):
    if not ok:raise SystemExit(message)
def mapping(commit):
    raw=git('ls-tree','-r','-z','--full-tree',commit)
    rows=[row for row in raw.split(b'\0') if row]
    included=[row for row in rows if not row.split(b'\t',1)[1].startswith(prefix)]
    excluded=[row.split(b'\t',1)[1].decode() for row in rows if row.split(b'\t',1)[1].startswith(prefix)]
    return raw,b'\0'.join(included)+b'\0',excluded
require(value('rev-list','--parents','-n','1',successor).split()==[successor,tested],'Evidence commit must directly and solely descend from tested commit')
require(value('rev-parse','HEAD')==successor and not value('status','--porcelain=v1','--untracked-files=all'),'Evidence successor must be checked out cleanly')
old,old_application,old_excluded=mapping(tested);new,new_application,new_excluded=mapping(successor)
require(not old_excluded and new_excluded,'Expected a newly added evidence directory')
new_rows=[row for row in new.split(b'\0') if row]
require(all(row.split(b'\t',1)[0].split()[:2]==[b'100644',b'blob'] for row in new_rows if row.split(b'\t',1)[1].startswith(prefix)),'Evidence additions must be regular nonexecutable files')
require(old_application==new_application,'A path/mode/blob outside the evidence directory changed')
changed=[x for x in git('diff-tree','--no-commit-id','--name-only','-r','-z',successor).split(b'\0') if x]
require(changed and all(x.startswith(prefix) for x in changed),'Unexpected changed path')
archived_verifier=git('show',successor+':'+prefix.decode()+'runners/verify-inheritance.py')
require(Path(__file__).read_bytes()==archived_verifier,'Executing verifier differs from archived reviewed source')
manifest=git('show',successor+':'+prefix.decode()+'manifest.json')
data=json.loads(manifest)
require(data['tested_commit']==tested and data['tested_tree']==value('rev-parse',tested+'^{tree}'),'Manifest binds another tested tree')
record=dict(verifier_sha256=digest(Path(__file__).read_bytes()),tested_commit=tested,tested_tree=value('rev-parse',tested+'^{tree}'),evidence_commit=successor,evidence_tree=value('rev-parse',successor+'^{tree}'),excluded_prefix=prefix.decode(),application_path_mode_blob_map_sha256=digest(old_application),same_application_path_mode_blob_map=True,tested_full_map_sha256=digest(old),evidence_full_map_sha256=digest(new),changed_paths=[x.decode() for x in changed],manifest_sha256=digest(manifest),inherited_validation='Application gates and source review are inherited from the tested commit; no new runtime execution is claimed for the evidence-only successor.',checkout_clean=True)
path=Path('/tmp/gitcraft-pr184-inheritance-'+successor+'.json');serialized=json.dumps(record,indent=2)+'\n'
require(not path.exists() or path.read_text()==serialized,'Preserve a differing existing proof')
path.write_text(serialized);print(path)
