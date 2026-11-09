"""Preserve three reviewed feature units with exclusive temporary files and exact tree equality."""
from pathlib import Path
import os, subprocess, tempfile, json, hashlib
repo = Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb')
os.chdir(repo)
base = 'a437a10af4c90a709a24ee7831a4fa7ca85a55ee'
accepted = 'cad719996254322bb453ca8d2a98db7478add6ae'
branch = 'fix/pr184-remaining-final-20261109'
ref = 'refs/heads/' + branch
identity = 'Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>'
def git(*args, **kwargs):
    return subprocess.check_output(['git', *args], **kwargs)
def rev(value):
    return git('rev-parse', value, text=True).strip()
def require(value, message):
    if not value:
        raise SystemExit(message)
require(rev('HEAD') == accepted and not git('status', '--porcelain=v1', '--untracked-files=all', text=True).strip(), 'Expected clean accepted source')
require(subprocess.run(['git', 'show-ref', '--verify', '--quiet', ref]).returncode == 1, 'Preserve existing final branch')
groups = [
    ('Warn about suspicious imported author rules', ['src/app/setup/mod.rs','src/app/setup/localization_tests.rs','locales/en-US/interaction.ftl','locales/de/interaction.ftl'], ['a6685c1f']),
    ('Refuse changed stash selections before dispatch', ['src/repo/partial_stash/mod.rs','src/repo/partial_stash/tests.rs','locales/en-US/repo-workflows.ftl','locales/de/repo-workflows.ftl'], ['67d0f696']),
    ('Keep blame visible during refresh', ['src/repo/file_view/mod.rs','src/repo/file_view/blame.rs','src/repo/file_view/tests.rs','src/repo/messages.rs'], ['51cfbabb']),
    ('Refresh line history from pinned relevant inputs', ['src/git/line_log.rs','src/git/mod.rs','src/repo/file_view/mod.rs','src/repo/file_view/line_history.rs','src/repo/file_view/tests.rs','src/repo/line_history_tests.rs','src/repo/messages.rs'], ['9c89f3e0','61d26f04',accepted]),
]
require({p for _, paths, _ in groups for p in paths} == set(git('diff', '--name-only', base, accepted, text=True).splitlines()), 'Feature map mismatch')
manifest = dict(base=base, accepted=accepted, accepted_tree=rev(accepted+'^{tree}'), branch=branch, units=[])
with tempfile.NamedTemporaryFile(mode='w+', prefix='gitcraft-pr184-remaining-assembly-', suffix='.json', delete=False) as output:
    manifest_path = output.name
    def save():
        output.seek(0)
        json.dump(manifest, output, indent=2)
        output.write('\n')
        output.truncate()
        output.flush()
        os.fsync(output.fileno())
    save()
    print(manifest_path, flush=True)
    with tempfile.TemporaryDirectory(prefix='gitcraft-pr184-remaining-index-') as index_dir:
        env = dict(os.environ, GIT_INDEX_FILE=str(Path(index_dir)/'index'), GIT_AUTHOR_NAME='Jonas Kamsker', GIT_AUTHOR_EMAIL='11245306+JKamsker@users.noreply.github.com', GIT_COMMITTER_NAME='Jonas Kamsker', GIT_COMMITTER_EMAIL='11245306+JKamsker@users.noreply.github.com')
        git('read-tree', base, env=env)
        parent = base
        for title, paths, provenance in groups:
            source = '51cfbabb' if title == 'Keep blame visible during refresh' else accepted
            rows = git('ls-tree', '-z', source, '--', *paths).split(b'\0')
            data = b''
            found = set()
            for row in rows:
                if not row:
                    continue
                meta, path = row.split(b'\t', 1)
                mode, kind, blob = meta.split()
                require(kind == b'blob', 'Expected immutable source blob')
                data += mode + b' ' + blob + b'\t' + path + b'\0'
                found.add(path.decode())
            require(found == set(paths), 'Missing source paths')
            git('update-index', '-z', '--index-info', input=data, env=env)
            tree = git('write-tree', env=env, text=True).strip()
            message = title + '\n\nAccepted implementation: ' + accepted + '\nProvenance: ' + ', '.join(provenance) + '\nStatus: WIP pending combined validation and final exact-source review.\n'
            commit = git('-c', 'commit.gpgsign=false', 'commit-tree', tree, '-p', parent, input=message, env=env, text=True).strip()
            require(git('show', '-s', '--format=%an <%ae>', commit, text=True).strip() == identity, 'Wrong author')
            require(git('show', '-s', '--format=%cn <%ce>', commit, text=True).strip() == identity, 'Wrong committer')
            git('update-ref', ref, commit, '' if not manifest['units'] else parent)
            subprocess.run(['git', 'push', 'origin', ref+':'+ref], check=True)
            remote = git('ls-remote', 'origin', ref, text=True).split()[0]
            require(remote == commit, 'Remote does not match')
            manifest['units'].append(dict(title=title, paths=paths, source=rev(source), provenance=provenance, parent=parent, commit=commit, tree=tree, remote=remote, author=identity))
            save()
            print(title, commit, flush=True)
            parent = commit
        complete_map = git('ls-tree', '-r', '-z', '--full-tree', parent)
        require(complete_map == git('ls-tree', '-r', '-z', '--full-tree', accepted), 'Final source map differs')
        require(rev(parent+'^{tree}') == manifest['accepted_tree'], 'Final tree differs')
        manifest.update(final=parent, exact_tree_match=True, complete_path_mode_blob_map_sha256=hashlib.sha256(complete_map).hexdigest())
        save()
print('ASSEMBLED', parent, 'EXACT TREE', manifest['accepted_tree'], flush=True)
