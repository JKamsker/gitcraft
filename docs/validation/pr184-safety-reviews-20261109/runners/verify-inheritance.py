"""Check application inheritance and every committed archive hash for this follow-up."""
from pathlib import Path
import subprocess, sys, json, hashlib, gzip, tempfile, os
REPO = Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb')
TESTED = '1e48ab83f98a8c34dc536408ee31915707365bcf'
PREFIX = b'docs/validation/pr184-safety-reviews-20261109/'
def require(ok, why):
    if not ok:
        raise SystemExit(why)
def git(*args):
    return subprocess.check_output(['git', *args], cwd=REPO)
def digest(data):
    return hashlib.sha256(data).hexdigest()
require(len(sys.argv) == 2, 'Pass the full evidence successor SHA')
successor = sys.argv[1]
require(len(successor) == 40 and all(c in '0123456789abcdef' for c in successor), 'Expected full SHA')
require(git('rev-list', '--parents', '-n', '1', successor).decode().split() == [successor, TESTED], 'Expected sole tested parent')
require(git('rev-parse', 'HEAD').decode().strip() == successor and not git('status', '--porcelain=v1', '--untracked-files=all').strip(), 'Expected clean successor checkout')
old = git('ls-tree', '-r', '-z', '--full-tree', TESTED)
new = git('ls-tree', '-r', '-z', '--full-tree', successor)
rows = [row for row in new.split(b'\0') if row]
added = [row for row in rows if row.split(b'\t', 1)[1].startswith(PREFIX)]
application = b'\0'.join(row for row in rows if not row.split(b'\t', 1)[1].startswith(PREFIX)) + b'\0'
require(added and application == old, 'Application path/mode/blob map changed')
require(not any(row.split(b'\t', 1)[1].startswith(PREFIX) for row in old.split(b'\0') if row), 'Expected newly added evidence prefix')
require(all(row.split(b'\t', 1)[0].split()[:2] == [b'100644', b'blob'] for row in added), 'Only regular evidence additions allowed')
def blob(relative):
    path = Path(relative)
    require(not path.is_absolute() and '..' not in path.parts, 'Unsafe archive path')
    return git('show', successor + ':' + PREFIX.decode() + relative)
verifier = Path(__file__).read_bytes()
require(blob('runners/verify-inheritance.py') == verifier, 'Executing verifier differs from archived reviewed source')
manifest_bytes = blob('manifest.json')
manifest = json.loads(manifest_bytes)
require(manifest['tested_commit'] == TESTED and manifest['tested_tree'] == git('rev-parse', TESTED+'^{tree}').decode().strip(), 'Wrong tested tree')
listed = set()
for entry in manifest['files']:
    require(entry['path'] not in listed, 'Duplicate archive file')
    listed.add(entry['path'])
    require(digest(blob(entry['path'])) == entry['sha256'], 'Archive file hash mismatch: '+entry['path'])
actual = {row.split(b'\t', 1)[1][len(PREFIX):].decode() for row in added}
require(actual == listed | {'manifest.json'}, 'Unlisted or missing archive file')
for entry in manifest['logs']:
    compressed = blob(entry['path'])
    require(digest(compressed) == entry['archive_sha256'], 'Compressed log hash mismatch')
    require(digest(gzip.decompress(compressed)) == entry['raw_sha256'], 'Raw log hash mismatch')
require(gzip.decompress(blob('tested-path-mode-blob-map.bin.gz')) == old, 'Archived tested map differs')
binding_bytes = blob('runners/gitcraft_pr184_approval_binding.py')
require(digest(binding_bytes) == 'fb057098987d40161211fb593084ee19cca19758a7269cc1582d1c1071336b7f', 'Reviewed approval binding helper differs')
bindings_namespace = {}
exec(compile(binding_bytes, 'gitcraft_pr184_approval_binding.py', 'exec'), bindings_namespace)
results_bytes = blob('full-results.json')
runner_bytes = blob('runners/full-run.py')
review_bytes = blob('reviews.json')
approval_tasks = bindings_namespace['validate_committed_execution'](manifest, results_bytes, runner_bytes, review_bytes, blob('runners/package.py'), verifier, TESTED, manifest['tested_tree'])
assembly_bytes = blob('assembly.json')
require(digest(assembly_bytes) == manifest['assembly_sha256'], 'Committed assembly differs from manifest')
plan = [('Explain line history in confirmed unborn repositories', ['src/git/line_log.rs']), ('Refresh failed file reads and configuration-sensitive blame', ['src/repo/file_view/mod.rs', 'src/repo/file_view/tests.rs', 'src/repo/load.rs', 'src/repo/messages.rs', 'src/repo/navigate.rs']), ('Protect replayed and recursive pulls before dispatch', ['src/repo/operations.rs']), ('Serialize local repository aliases by retained filesystem identity', ['src/repo/tests.rs', 'src/tasks.rs', 'src/tray/watch.rs', 'src/tray/watch_tests.rs']), ('Preserve excluded replacements of deleted tracked paths', ['src/repo/partial_stash/mod.rs', 'src/repo/partial_stash/tests.rs', 'locales/en-US/repo-workflows.ftl', 'locales/de/repo-workflows.ftl'])]
assembly_review_task = bindings_namespace['validate_assembly_provenance'](assembly_bytes, review_bytes, git, '71ab301cf9bf976dab39956f1f22b66ab7d7f1c6', 'ab46c8001068e44bad9d2a8f4b5ea8e94a740c77', TESTED, manifest['tested_tree'], plan)
results = json.loads(results_bytes)
bindings_namespace['validate_log_bindings'](results, manifest['logs'], blob)
require(digest(blob('runners/gitcraft_pr184_source_integrity.py')) == results['source_checker_sha256'], 'Archived source checker differs from execution')
record = dict(assembly_review_task=assembly_review_task, assembly_sha256=digest(assembly_bytes), approval_tasks=approval_tasks, approval_binding_sha256=digest(binding_bytes), verdict='PASS', tested_commit=TESTED, tested_tree=manifest['tested_tree'], evidence_commit=successor, evidence_tree=git('rev-parse', successor+'^{tree}').decode().strip(), excluded_prefix=PREFIX.decode(), application_map_equal=True, archive_integrity=True, file_count=len(listed), log_count=len(manifest['logs']), tested_map_sha256=digest(old), evidence_map_sha256=digest(new), manifest_sha256=digest(manifest_bytes), verifier_sha256=digest(verifier), full_results_sha256=digest(blob('full-results.json')), reviews_sha256=digest(blob('reviews.json')), inheritance='Application gates and source approval are inherited; no new runtime execution on this evidence successor is claimed.')
with tempfile.NamedTemporaryFile(mode='w', prefix='gitcraft-pr184-round4-inheritance-', suffix='.json', delete=False) as output:
    json.dump(record, output, indent=2)
    output.write('\n')
    output.flush()
    os.fsync(output.fileno())
    print(output.name)
