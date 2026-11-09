"""Archive a completed immutable follow-up run without modifying historical evidence."""
from pathlib import Path
import subprocess, json, hashlib, gzip, tempfile, shutil, os, sys, re
REPO = Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb')
TESTED = '1e48ab83f98a8c34dc536408ee31915707365bcf'
ROOT = REPO/'docs/validation/pr184-safety-reviews-20261109'
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
runner = snapshot('/tmp/gitcraft-pr184-round4-full-run.py')
verifier = snapshot('/tmp/gitcraft-pr184-round4-inheritance.py')
require(digest(runner) == full['runner_sha256'], 'Executed runner differs')
binding_bytes = snapshot('/tmp/gitcraft_pr184_approval_binding.py')
require(digest(binding_bytes) == 'fb057098987d40161211fb593084ee19cca19758a7269cc1582d1c1071336b7f', 'Reviewed approval binding helper differs')
bindings_namespace = {}
exec(compile(binding_bytes, 'gitcraft_pr184_approval_binding.py', 'exec'), bindings_namespace)
approval_tasks = bindings_namespace['validate_approvals'](reviews, sha, tree, digest(full_bytes), digest(runner), digest(snapshot(__file__)), digest(verifier))
plan = [('Explain line history in confirmed unborn repositories', ['src/git/line_log.rs']), ('Refresh failed file reads and configuration-sensitive blame', ['src/repo/file_view/mod.rs', 'src/repo/file_view/tests.rs', 'src/repo/load.rs', 'src/repo/messages.rs', 'src/repo/navigate.rs']), ('Protect replayed and recursive pulls before dispatch', ['src/repo/operations.rs']), ('Serialize local repository aliases by retained filesystem identity', ['src/repo/tests.rs', 'src/tasks.rs', 'src/tray/watch.rs', 'src/tray/watch_tests.rs']), ('Preserve excluded replacements of deleted tracked paths', ['src/repo/partial_stash/mod.rs', 'src/repo/partial_stash/tests.rs', 'locales/en-US/repo-workflows.ftl', 'locales/de/repo-workflows.ftl'])]
assembly_review_task = bindings_namespace['validate_assembly_provenance'](assembly_bytes, reviews_bytes, git, '71ab301cf9bf976dab39956f1f22b66ab7d7f1c6', 'ab46c8001068e44bad9d2a8f4b5ea8e94a740c77', sha, tree, plan)
source_map = git('ls-tree', '-r', '-z', '--full-tree', sha)
require(source_map == git('ls-tree', '-r', '-z', '--full-tree', assembly['accepted']) and digest(source_map) == assembly['complete_path_mode_blob_map_sha256'], 'Accepted source map differs')
source_checker = snapshot('/tmp/gitcraft_pr184_source_integrity.py')
require(digest(source_checker) == full['source_checker_sha256'], 'Executed source checker differs')
observations = full['source_observations']
require(len(observations) == 34 and all(x['commit'] == sha and not x['errors'] and x['expected_map_sha256'] == x['observed_map_sha256'] == digest(source_map) and x['entries'] == len(source_map.split(b'\0'))-1 for x in observations), 'Missing or mismatching tracked-source observations')
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
pending = {}; manifest = dict(assembly_sha256=digest(assembly_bytes), tested_commit=sha, tested_tree=tree, files=[], logs=[], full_results_sha256=digest(full_bytes), executed_runner_sha256=digest(runner), source_map_sha256=digest(source_map), application=app)
def put(path, data):
    require(path not in pending, 'Duplicate archive path')
    pending[path] = data
    manifest['files'].append(dict(path=path, sha256=digest(data)))
put('full-results.json', full_bytes); put('assembly.json', assembly_bytes); put('reviews.json', reviews_bytes)
put('findings.json', snapshot('/tmp/gitcraft-pr184-round4-findings.json'))
put('runners/full-run.py', runner); put('runners/assemble.py', snapshot('/tmp/gitcraft-pr184-round4-assemble.py')); put('runners/verify-inheritance.py', verifier); put('runners/package.py', snapshot(__file__))
put('runners/gitcraft_pr184_approval_binding.py', binding_bytes)
put('runners/gitcraft_pr184_source_integrity.py', source_checker)
put('runners/source-integrity-tests.py', snapshot('/tmp/gitcraft-pr184-source-integrity-tests.py'))
put('runners/approval-binding-tests.py', snapshot('/tmp/gitcraft-pr184-approval-binding-tests.py'))
put('tooling-regressions.log', snapshot('/tmp/gitcraft-pr184-round4-tooling-regressions.log'))
put('provenance-errata.md', snapshot('/tmp/gitcraft-pr184-round4-provenance-errata.md'))
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
readme = f'''# PR 184 safety review validation

Tested commit `{sha}`, tree `{tree}`. [Full results](full-results.json) record sixteen successful gates from one complete run on the exact candidate. [Assembly](assembly.json) preserves five ordered feature commits descending from published `71ab301cf9bf976dab39956f1f22b66ab7d7f1c6`; each was committed and pushed before validation. The complete tested path/mode/blob map equals accepted source.

The fixes preserve the localized unborn-repository explanation, invalidate failed mutable file reads, preserve current File/History reads across unrelated status updates through captured input keys, refresh configuration-sensitive blame with an independent epoch, protect saved and recursive pulls before dispatch, serialize local metadata aliases by retained filesystem identity, and refuse excluded replacements of staged or unstaged removed paths. Case/Unicode aliases are checked on the repository's machine without trusting host case rules or `core.ignorecase`. Local scheduling lookup failure retains the documented path fallback, which cannot guarantee exclusion across unidentified aliases. Remote keys retain the alias-safe shared bucket. [Findings](findings.json) record review dispositions.

| Gate | Exit | Recorded summary |
|---|---:|---|
'''+'\n'.join(rows)+'''

The three filtered ACL-save/index/palette cases each passed separately by exact name. Recording 3+1 passes overlap the library; both doctest suites contain zero tests. [Ignored tests](ignored-tests.json) enumerate all 24 names and reasons. Other platform-gated cases are outside this Linux inventory.

Recording gates, runtime suites and palette use PID/proc/dev containment. Two exact native ACL/ownership metadata fixtures operate on owned temporary files, including ordinary Git children; no process-control test runs natively. Real supplementary-group drift remains conditional; a passing summary does not establish that branch executed. No host-wide signal selectors were used. This does not establish native Windows/macOS, live SSH/WSL, picker or screen-reader behavior.

[Reviews](reviews.json) distinguish trusted source, digest-bound execution and tool-design approvals. Task IDs must be nonblank, reviewer provider/model identities must be populated, and source/execution tasks must differ. These records are trusted inputs rather than cryptographic reviewer authentication. Archived runner and source-checker bytes match completed execution. All tracked bytes, modes and symlink targets were independently compared with the immutable tree at 34 recorded boundaries; hidden/sparse flags and observation errors refuse validation. Boundary observations do not establish an immutable filesystem throughout execution.

[Manifest](manifest.json) binds every portable file plus compressed/raw log hashes. All gzip timestamps are zero. `gzip -dc logs/runtime-gitcraft-lib.log.gz` reproduces the library log. Absolute paths in original results identify the execution machine; the manifest maps portable logs. The NUL-delimited tested map is archived losslessly. Tooling regression tests exercise hidden source edits, malformed approvals, rewritten results with stale approval, and invalid assembly chains.

[Historical provenance errata](provenance-errata.md) qualify the earlier completion-stage identity as unknown and disclose prior clean-status source-attribution limits. Every historical executed byte and archive remains unchanged; no hidden historical edit or archive corruption is established. Earlier application gates do not validate this changed source.

A documentation-only successor inherits application gates only after the archived verifier establishes sole ancestry, complete tested-map equality outside this new evidence prefix, complete committed archive inventory, approved gate-log correspondence, pinned accepted-source and assembly provenance, file/log hashes, and exact committed execution/source/design approvals. Its external proof identifies both commits, both trees, reviews and evidence digests. No new runtime execution is claimed on that successor.
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
