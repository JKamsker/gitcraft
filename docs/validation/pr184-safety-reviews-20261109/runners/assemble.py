"""Preserve five feature units with exclusive temporary files and exact tree equality."""
from pathlib import Path
import os, subprocess, tempfile, json, hashlib
repo = Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb')
os.chdir(repo)
base = '71ab301cf9bf976dab39956f1f22b66ab7d7f1c6'
accepted = 'ab46c8001068e44bad9d2a8f4b5ea8e94a740c77'
branch = 'fix/pr184-round4-final-v3-20261109'
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
    ('Explain line history in confirmed unborn repositories', ['src/git/line_log.rs'], ['eb12a9d71765478a6d9c94d2d3da07cd9581e955']),
    ('Refresh failed file reads and configuration-sensitive blame', ['src/repo/file_view/mod.rs','src/repo/file_view/tests.rs','src/repo/load.rs','src/repo/messages.rs','src/repo/navigate.rs'], ['6a60ef0518a7caa0ab1a7c3deb68e2d753048fc2','fba88cfccee51a7e912274c885418bdb78c227bd','4dec686beee4e96330398098fd94c22c0c098487','ab46c8001068e44bad9d2a8f4b5ea8e94a740c77']),
    ('Protect replayed and recursive pulls before dispatch', ['src/repo/operations.rs'], ['13a987cc4a2293af0ace0578ae0e01079beebfa7','c3fea67bc5cc5f862154ef8227bdd96d93e07057','8c541cb72bc2efb01b9095ba1ec6fa07c09628c3','47ecee32d0359abc240b95a4c6ff42db2f37a3dc','0fe2c606952deae3f1eea355399d1a64302567a7','e4869367f576b802983e125ac377b0753cf2f270','ab46c8001068e44bad9d2a8f4b5ea8e94a740c77']),
    ('Serialize local repository aliases by retained filesystem identity', ['src/repo/tests.rs','src/tasks.rs','src/tray/watch.rs','src/tray/watch_tests.rs'], ['b13b5a410a1a1dabe0138e09c41444fd9630fd73','a28dfab4145c7a9e8010528b7a30ed5d98dcf26e','8f87fa6619486af3df42a24cf6b728d9be1c3542']),
    ('Preserve excluded replacements of deleted tracked paths', ['src/repo/partial_stash/mod.rs','src/repo/partial_stash/tests.rs','locales/en-US/repo-workflows.ftl','locales/de/repo-workflows.ftl'], ['29792ee901ec37d0bca02e03273d86e3a299633e','09ba75459202913d526fc3961c3fb609c41d2c3b','1dacb09c95e97abc25d66964bef1dcc7a822dddd']),
]
require({p for _, paths, _ in groups for p in paths} == set(git('diff', '--name-only', base, accepted, text=True).splitlines()), 'Feature map mismatch')
manifest = dict(base=base, accepted=accepted, accepted_tree=rev(accepted+'^{tree}'), branch=branch, units=[])
with tempfile.NamedTemporaryFile(mode='w+', prefix='gitcraft-pr184-round4-v3-assembly-', suffix='.json', delete=False) as output:
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
    with tempfile.TemporaryDirectory(prefix='gitcraft-pr184-round4-index-') as index_dir:
        env = dict(os.environ, GIT_INDEX_FILE=str(Path(index_dir)/'index'), GIT_AUTHOR_NAME='Jonas Kamsker', GIT_AUTHOR_EMAIL='11245306+JKamsker@users.noreply.github.com', GIT_COMMITTER_NAME='Jonas Kamsker', GIT_COMMITTER_EMAIL='11245306+JKamsker@users.noreply.github.com')
        git('read-tree', base, env=env)
        parent = base
        for title, paths, provenance in groups:
            source = accepted
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
