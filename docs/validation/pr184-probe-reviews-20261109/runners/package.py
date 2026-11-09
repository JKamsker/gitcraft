"""Archive a completed immutable follow-up run without modifying historical evidence."""
from pathlib import Path
import subprocess, json, hashlib, gzip, tempfile, shutil, os, sys, re
REPO = Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb')
TESTED = 'dcd0b82ed672b1fffb485f235f6864d378143c04'
ROOT = REPO/'docs/validation/pr184-probe-reviews-20261109'
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
runner = snapshot('/tmp/gitcraft-pr184-round5-full-run.py')
verifier = snapshot('/tmp/gitcraft-pr184-round5-inheritance.py')
require(digest(runner) == full['runner_sha256'], 'Executed runner differs')
binding_bytes = snapshot('/tmp/gitcraft_pr184_round5_approval_binding.py')
require(digest(binding_bytes) == 'eda770dedb7661c2ca39a233a723ef2434ae2c5c3e0bd29c582dd132a2592934', 'Reviewed approval binding helper differs')
bindings_namespace = {}
exec(compile(binding_bytes, 'gitcraft_pr184_round5_approval_binding.py', 'exec'), bindings_namespace)
approval_tasks = bindings_namespace['validate_approvals'](reviews, sha, tree, digest(full_bytes), digest(runner), digest(snapshot(__file__)), digest(verifier))
plan = [('Keep host-wide author rules free of namespace warnings', ['src/app/setup/mod.rs', 'src/app/setup/localization_tests.rs'], '71304ad5b4abe16c82e596296cb8157d8deada26'), ('Serialize local mutations without synchronous identity lookup', ['src/tasks.rs', 'src/repo/tests.rs', 'src/tray/watch.rs', 'src/repo/messages.rs', 'src/repo/load.rs', 'src/repo/mod.rs', 'src/repo/lifecycle.rs', 'src/repo/partial_stash/tests.rs'], '7fe66c1c88adfc9ad6e84b1d0974a30d5f3ca82f'), ('Protect native pull cleanup and literal submodule update scope', ['src/repo/operations.rs', 'src/repo/keep_submodules.rs', 'src/repo/partial_stash/mod.rs', 'src/repo/partial_stash/tests.rs'], '71304ad5b4abe16c82e596296cb8157d8deada26'), ('Coalesce file probes and retain current pending reads', ['src/repo/file_view/blame.rs', 'src/repo/file_view/line_history.rs', 'src/repo/file_view/mod.rs', 'src/repo/file_view/tests.rs', 'src/repo/line_history_tests.rs', 'src/repo/navigate.rs', 'src/repo/messages.rs', 'src/repo/lifecycle.rs'], '71304ad5b4abe16c82e596296cb8157d8deada26'), ('Verify clone keyboard submission through production widgets', ['src/app/clone.rs'], '71304ad5b4abe16c82e596296cb8157d8deada26')]
assembly_review_task = bindings_namespace['validate_assembly_provenance'](assembly_bytes, reviews_bytes, git, 'c36d436e88508ebfbbbd85818b523ae44f670e3a', '71304ad5b4abe16c82e596296cb8157d8deada26', sha, tree, plan)
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
put('findings.json', snapshot('/tmp/gitcraft-pr184-round5-findings.json'))
put('runners/full-run.py', runner); put('runners/assemble.py', snapshot('/tmp/gitcraft-pr184-round5-assemble.py')); put('runners/verify-inheritance.py', verifier); put('runners/package.py', snapshot(__file__))
put('runners/gitcraft_pr184_round5_approval_binding.py', binding_bytes)
put('runners/gitcraft_pr184_source_integrity.py', source_checker)
put('runners/source-integrity-tests.py', snapshot('/tmp/gitcraft-pr184-source-integrity-tests.py'))
put('runners/approval-binding-tests.py', snapshot('/tmp/gitcraft-pr184-round5-approval-binding-tests.py'))
put('tooling-regressions.log', snapshot('/tmp/gitcraft-pr184-round5-tooling-regressions.log'))
put('provenance-errata.md', snapshot('/tmp/gitcraft-pr184-round5-provenance-errata.md'))
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

Tested commit `{sha}`, tree `{tree}`. [Full results](full-results.json) record sixteen successful gates from one complete run on the exact candidate. [Assembly](assembly.json) preserves five ordered feature commits descending from published `c36d436e88508ebfbbbd85818b523ae44f670e3a`; each was committed and pushed before validation. The complete tested path/mode/blob map equals accepted source.

The fixes accept host-wide author-rule suffixes, serialize all keyed local mutations through a pure Local bucket, guard native pull autostash cleanup, refuse prefixed/TRY stash envelopes, preserve literal selected-submodule scope, coalesce status probes, retry queue BUSY through UI polling, and reconcile pending file/history/blame reads against captured input keys. Clone keyboard regressions exercise production widgets without invoking pickers or clones. [Findings](findings.json) retain every reported review and its disposition.

One global Local mutation bucket deliberately trades independent-repository concurrency for exclusion across aliases, lookup failures and pathname retargeting. Remote mutations retain their separate shared Remote bucket; unkeyed reads remain runnable when worker capacity permits. Scheduling performs no filesystem identity lookup. This provides app-internal exclusion, not immutable repository targeting or coordination with external Git, other app instances, or local/remote paths exposing the same storage. Background reads separately cache fetch ownership.

Native pull cleanup checks conservatively refuse excluded replacements even when native autostash would be disabled. Canonical sidebar submodule updates inspect literal selected gitlinks and descendants; noncanonical option envelopes receive a whole-repository populated-submodule scan rather than guessed scope. Attribution keys include effective Git attributes, symlink-followed configured dependency files through the repository transport, and resolved mailmap blobs. External textconv program state still requires explicit refresh.

Assembly pins each unit's immutable source, including a scheduler-only intermediate snapshot where shared files precede the file-view feature. The final complete tree equals the accepted combined source. Digest-bound independent assembly review covers that exact ordered history.

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
