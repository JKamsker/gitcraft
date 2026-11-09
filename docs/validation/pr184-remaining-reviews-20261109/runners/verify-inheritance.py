"""Check application inheritance and every committed archive hash for this follow-up."""
from pathlib import Path
import subprocess, sys, json, hashlib, gzip, tempfile, os
REPO = Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb')
TESTED = '96584b3a69234822ca9f8c299ec13e32929ac2a0'
PREFIX = b'docs/validation/pr184-remaining-reviews-20261109/'
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
record = dict(verdict='PASS', tested_commit=TESTED, tested_tree=manifest['tested_tree'], evidence_commit=successor, evidence_tree=git('rev-parse', successor+'^{tree}').decode().strip(), excluded_prefix=PREFIX.decode(), application_map_equal=True, archive_integrity=True, file_count=len(listed), log_count=len(manifest['logs']), tested_map_sha256=digest(old), evidence_map_sha256=digest(new), manifest_sha256=digest(manifest_bytes), verifier_sha256=digest(verifier), full_results_sha256=digest(blob('full-results.json')), reviews_sha256=digest(blob('reviews.json')), inheritance='Application gates and source approval are inherited; no new runtime execution on this evidence successor is claimed.')
with tempfile.NamedTemporaryFile(mode='w', prefix='gitcraft-pr184-remaining-inheritance-', suffix='.json', delete=False) as output:
    json.dump(record, output, indent=2)
    output.write('\n')
    output.flush()
    os.fsync(output.fileno())
    print(output.name)
