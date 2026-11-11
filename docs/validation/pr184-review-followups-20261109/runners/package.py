"""Archive a completed immutable follow-up run without modifying historical evidence."""
from pathlib import Path
import subprocess, json, hashlib, gzip, tempfile, shutil, os, sys, re
REPO = Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb')
TESTED = '0d651f0279021bba63764f6725a337223408a879'
ROOT = REPO/'docs/validation/pr184-review-followups-20261109'
os.chdir(REPO)
def require(ok, why):
    if not ok:
        raise SystemExit(why)
def digest(data):
    return hashlib.sha256(data).hexdigest()
snapshots = {}
def snapshot(path):
    path = Path(path).absolute()
    if path not in snapshots:
        snapshots[path] = path.read_bytes()
    return snapshots[path]
def git(*args):
    return subprocess.check_output(['git', *args])
require(len(sys.argv) == 4, 'Pass full-results, assembly and reviews JSON paths')
full_bytes, assembly_bytes, reviews_bytes = [snapshot(p) for p in sys.argv[1:]]
full, assembly, reviews = [json.loads(b) for b in [full_bytes, assembly_bytes, reviews_bytes]]
sha = full['commit']; tree = full['tree']; clean = dict(head=sha, tree=tree, status='')
require(sha == TESTED and git('rev-parse', 'HEAD').decode().strip() == sha and not git('status', '--porcelain=v1', '--untracked-files=all').strip(), 'Expected clean tested candidate')
require(full.get('status') == 'passed' and full.get('final_state') == clean, 'Incomplete run')
labels = {'fmt','check','clippy','build-tests','recording-outcomes','recording-resolver','runtime-gitcraft_os-lib','runtime-gitcraft-lib','runtime-gitcraft-main','runtime-checkpoint_cli-checkpoint_cli','runtime-mcp_stdio-mcp_stdio','runtime-standalone_cli-standalone_cli','acl-save','acl-index','palette-isolated','doc-tests'}
require(len(full['checks']) == 16 and {x['label'] for x in full['checks']} == labels, 'Expected sixteen unique gates')
require(all(x['exit'] == 0 and x['before'] == x['after'] == clean for x in full['checks']), 'Unsuccessful or dirty gate')
runner = snapshot('/tmp/gitcraft-pr184-review-run.py')
verifier = snapshot('/tmp/gitcraft-pr184-round2-inheritance.py')
require(digest(runner) == full['runner_sha256'], 'Executed runner differs')
exact = [x for x in reviews if x.get('reviewed_commit') == sha and x.get('reviewed_tree') == tree and x.get('verdict') == 'PASS']
source = [x for x in exact if x.get('scope') == 'source']
execution = [x for x in exact if x.get('scope') == 'execution_evidence' and x.get('full_results_sha256') == digest(full_bytes) and x.get('executed_runner_sha256') == digest(runner)]
require(any(a['task_id'] != b['task_id'] for a in source for b in execution), 'Independent source and exact evidence approvals required')
require(any(x.get('scope') == 'inheritance_verifier' and x.get('script_sha256') == digest(verifier) for x in exact), 'Exact verifier design approval required')
require(any(x.get('scope') == 'evidence_packaging' and x.get('script_sha256') == digest(snapshot(__file__)) for x in exact), 'Exact packager design approval required')
require(assembly['final'] == sha and assembly['accepted_tree'] == tree and assembly['exact_tree_match'], 'Assembly mismatch')
parent = assembly['base']
for unit in assembly['units']:
    require(unit['parent'] == parent and git('rev-parse', unit['commit']+'^').decode().strip() == parent, 'Assembly ancestry mismatch')
    require(unit['remote'] == unit['commit'] and git('rev-parse', unit['commit']+'^{tree}').decode().strip() == unit['tree'], 'Assembly provenance mismatch')
    require(git('show', '-s', '--format=%an <%ae>', unit['commit']).decode().strip() == 'Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>', 'Unexpected author')
    require(set(git('diff-tree', '--no-commit-id', '--name-only', '-r', unit['commit']).decode().splitlines()) == set(unit['paths']), 'Assembly paths differ')
    parent = unit['commit']
require(parent == sha and len(assembly['units']) == 3, 'Expected three feature units')
source_map = git('ls-tree', '-r', '-z', '--full-tree', sha)
require(source_map == git('ls-tree', '-r', '-z', '--full-tree', assembly['accepted']) and digest(source_map) == assembly['complete_path_mode_blob_map_sha256'], 'Accepted source map differs')
require(len(full['artifacts']) == 6 and len({x['executable'] for x in full['artifacts']}) == 6, 'Harness inventory mismatch')
expected_artifacts = {('gitcraft_os',('lib',),'crates/gitcraft-os/src/lib.rs'),('gitcraft',('lib',),'src/lib.rs'),('gitcraft',('bin',),'src/main.rs'),('checkpoint_cli',('test',),'tests/checkpoint_cli.rs'),('mcp_stdio',('test',),'tests/mcp_stdio.rs'),('standalone_cli',('test',),'tests/standalone_cli.rs')}
require({(x['name'],tuple(x['kind']),x['source']) for x in full['artifacts']} == expected_artifacts, 'Unexpected harness identities')
artifacts = {x['executable']: x['sha256'] for x in full['artifacts']}; app = full['application']
library = next(x for x in full['artifacts'] if x['source'] == 'src/lib.rs')
bindings = {'runtime-'+x['name']+('-lib' if x['source'].endswith('/lib.rs') else '-'+Path(x['source']).stem): x for x in full['artifacts']}
for label in ['recording-outcomes','recording-resolver','acl-save','acl-index','palette-isolated']:
    bindings[label] = library
require(len(bindings) == 11, 'Expected eleven runtime bindings')
prefix = ['bwrap','--unshare-pid','--die-with-parent','--bind','/','/','--dev','/dev','--proc','/proc','--']
for x in full['checks']:
    require(digest(snapshot(x['log'])) == x['sha256'], 'Raw log differs')
    if x['label'] in bindings:
        artifact = bindings[x['label']]
        if x['label'] in ['acl-save','acl-index']:
            require(x.get('containment') == 'native-owned-ACL-metadata-fixture' and x['command'][0] == artifact['executable'], 'Wrong native fixture command')
        else:
            require(x['command'][:len(prefix)] == prefix and x['command'][len(prefix)] == artifact['executable'], 'Wrong contained executable')
        require(x.get('binary_sha256_before') == x.get('binary_sha256_after') == artifact['sha256'], 'Harness binding differs')
        require(x.get('application_sha256_before') == x.get('application_sha256_after') == app['sha256'], 'Application binding differs')
for path, expected in [*artifacts.items(), (app['executable'], app['sha256'])]:
    with Path(path).open('rb') as stream:
        require(hashlib.file_digest(stream, 'sha256').hexdigest() == expected, 'Executable changed')
pending = {}; manifest = dict(tested_commit=sha, tested_tree=tree, files=[], logs=[], full_results_sha256=digest(full_bytes), executed_runner_sha256=digest(runner), source_map_sha256=digest(source_map), application=app)
def put(path, data):
    require(path not in pending, 'Duplicate archive path')
    pending[path] = data
    manifest['files'].append(dict(path=path, sha256=digest(data)))
put('full-results.json', full_bytes); put('assembly.json', assembly_bytes); put('reviews.json', reviews_bytes)
put('findings.json', snapshot('/tmp/gitcraft-pr184-round2-findings.json'))
put('runners/full-run.py', runner); put('runners/assemble.py', snapshot('/tmp/gitcraft-pr184-round2-assemble.py')); put('runners/verify-inheritance.py', verifier); put('runners/package.py', snapshot(__file__))
put('tested-path-mode-blob-map.bin.gz', gzip.compress(source_map, mtime=0))
ignored = []; rows = []
for x in full['checks']:
    raw = snapshot(x['log']); path = 'logs/'+x['label']+'.log.gz'; compressed = gzip.compress(raw, mtime=0)
    put(path, compressed); manifest['logs'].append(dict(gate=x['label'], path=path, raw_sha256=digest(raw), archive_sha256=digest(compressed)))
    names = re.findall(r'^test (.+?) \.\.\. ignored(?:, (.*))?$', raw.decode(), re.M)
    require(len(names) == sum(s['ignored'] for s in x['test_summaries']), 'Ignored count mismatch')
    ignored.extend(dict(name=n, reason=r, gate=x['label']) for n,r in names)
    detail = '; '.join(f"{s['passed']} passed, {s['failed']} failed, {s['ignored']} ignored, {s['filtered']} filtered" for s in x['test_summaries']) or 'exit 0'
    rows.append(f"| {x['label']} | {x['exit']} | {detail} |")
require(len(ignored) == len({x['name'] for x in ignored}) == 24, 'Expected twenty-four distinct ignored tests')
put('ignored-tests.json', (json.dumps(ignored, indent=2)+'\n').encode())
readme = f'''# PR 184 review follow-up validation

Tested commit `{sha}`, tree `{tree}`. [Full results](full-results.json) record sixteen successful gates on the exact clean candidate. [Assembly](assembly.json) preserves three feature commits descending from `f5919c97`, with the complete accepted source map unchanged. Each unit was pushed before validation.

The fixes cover excluded stash directory entries, tag-name Enter with multiline annotations, and matching/ranking author namespaces with trailing slashes. [Findings and dispositions](findings.json) preserve independent verification of all eight comments. Broader malformed-key advisories and stale-selection diagnostics are deferred UX improvements; stale arguments fail closed before mutation. Remote mutations retain the deliberate global bucket needed to cover aliases. Historical assembly and inheritance runners are preserved unchanged; new assembly uses exclusively created temporary files and the new inheritance verifier also checks committed archive hashes.

| Gate | Exit | Recorded summary |
|---|---:|---|
'''+'\n'.join(rows)+'''

The three filtered ACL-save/index/palette cases each passed separately by exact name. Recording 3+1 tests overlap library coverage. Both doctest suites contain zero tests. [Ignored tests](ignored-tests.json) enumerate all 24 names and reasons; platform-gated cases are outside that Linux inventory.

Recording gates, runtime suites and palette use PID/proc/dev isolation. The two native ACL metadata fixtures operate on owned temporary files, including ordinary Git children; no process-control tests run natively. Real supplementary-group drift is conditional, and a passing summary does not establish that branch executed. No host-wide signal selectors were used. Remote and UI tests do not establish live SSH/WSL, native picker, or real screen-reader behavior.

[Reviews](reviews.json) distinguish source, execution, and verifier-design approvals. The source and digest-bound execution approvals are independent and name this exact tested commit/tree. Reviewers inspected parent-produced evidence rather than rerunning the suite unless explicitly recorded otherwise.

[Manifest](manifest.json) binds every archive file and each raw/compressed log pair. Gzip timestamps are zero; for example `gzip -dc logs/runtime-gitcraft-lib.log.gz` reproduces the library log. Absolute paths in original results identify the execution machine; the manifest maps portable archived logs. The NUL-delimited tested path/mode/blob map is archived losslessly.

Earlier validation remains historical and does not validate these changed application files. The initial test-harness compile error was corrected in accepted source `0f72c535`; no passing execution is attributed to that failed attempt. No earlier failed or partial gate supplies this full run.

A documentation-only successor inherits these gates only after the archived verifier proves sole ancestry, complete application path/mode/blob equality outside this newly added evidence directory, and all committed archive/file/log hashes. Its external proof names both commits, both trees and digests; no new runtime execution is claimed on that successor.
'''
put('README.md', readme.encode())
require(not ROOT.exists(), 'Preserve existing package')
staging = Path(tempfile.mkdtemp(prefix='.pr184-review-package-', dir=ROOT.parent))
try:
    for path, data in pending.items():
        target = staging/path; target.parent.mkdir(parents=True, exist_ok=True); target.write_bytes(data)
    (staging/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    require(not ROOT.exists(), 'Concurrent package exists')
    os.rename(staging, ROOT)
finally:
    shutil.rmtree(staging, ignore_errors=True)
print(ROOT)
