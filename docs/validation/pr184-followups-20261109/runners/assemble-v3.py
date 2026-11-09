from pathlib import Path
import os,subprocess,tempfile,json,hashlib
repo=Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb');os.chdir(repo)
def git(*args,**kwargs):return subprocess.check_output(['git',*args],**kwargs)
def rev(x):return git('rev-parse',x,text=True).strip()
base='5988524c23e43d8862225e44b09fbc104a735cc6';accepted=rev(os.environ['GITCRAFT_ACCEPTED_SHA']);branch='fix/pr184-final-followups-v3-20261109';ref='refs/heads/'+branch
if git('status','--porcelain',text=True).strip():raise SystemExit('Accepted worktree must be clean')
if subprocess.run(['git','show-ref','--verify','--quiet',ref]).returncode==0:raise SystemExit('Preserve existing assembly branch; do not overwrite')
author='Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>'
index=Path(tempfile.mktemp(prefix='gitcraft-pr184-assembly-index-'))
env=dict(os.environ,GIT_INDEX_FILE=str(index),GIT_AUTHOR_NAME='Jonas Kamsker',GIT_AUTHOR_EMAIL='11245306+JKamsker@users.noreply.github.com',GIT_COMMITTER_NAME='Jonas Kamsker',GIT_COMMITTER_EMAIL='11245306+JKamsker@users.noreply.github.com')
def indexgit(*args,**kwargs):return git(*args,env=env,**kwargs)
groups=[
 ('Preserve file-view generations across supported Rust toolchains',accepted,['src/repo/file_view/mod.rs'],'e16441b23d84ffd2d54bc9f409ae0ba4ead35983'),
 ('Submit forms only from eligible fields or explicit primary actions',accepted,['src/a11y/mod.rs','src/a11y/enter_tests.rs','src/app/clone.rs','src/app/askpass_dialog.rs','src/repo/dialogs/branch.rs','src/repo/dialogs/enter_tests.rs','src/repo/dialogs/fold.rs','src/repo/dialogs/mod.rs','src/repo/dialogs/rehearse.rs','src/repo/dialogs/test_commits.rs','src/repo/fold/tests.rs'],'c01b841d, 1e7116d9, 73b8be6d, 6179204f, 9a804356, 6dc16d53, 175f5daf; latest accepted fixtures and lint scope; accessibility action repair 6f21540ef464f1c27edaf6d50881bda1fbd86086; Fold dispatcher fixture 7346090eb38f4332774a06473a02d60262585f32'),
 ('Warn about URL-shaped outgoing-author rules in portable previews',accepted,['src/app/setup/mod.rs','src/app/setup/localization_tests.rs','src/portability/tests.rs','locales/de/interaction.ftl','locales/en-US/interaction.ftl'],'1c5babc69ac757b8360130ef089c36b3f4669a1d'),
 ('Select outgoing-author rules by matching namespace specificity',accepted,['src/settings/profiles.rs','src/push_preflight/tests.rs'],'e5d67c559e1bc498e47e76ea76679215c25d8ae5'),
 ('Classify saved pull options before protecting ignored files','ed6428f689623795d9e767a74285e288a3a923f8',['src/repo/operations.rs'],'ec2ffbc87cf31976ed7dcd304c02a3745eb39af6; integrated ed6428f689623795d9e767a74285e288a3a923f8'),
 ('Serialize remote mutations without changing repository identity',accepted,['src/tasks.rs','src/repo/load.rs','src/repo/lifecycle.rs','src/repo/tests.rs','src/tray/watch_tests.rs'],'c5415af84335672d206006191d0e87d9b51f8d0a; eb38e6d6 test-only lint scope'),
 ('Revalidate destructive stash safety and supported undo at execution',accepted,['src/repo/partial_stash/mod.rs','src/repo/partial_stash/tests.rs','src/repo/dialogs/stash.rs','src/repo/operations.rs','locales/de/repo-workflows.ftl','locales/en-US/repo-workflows.ftl'],'ad0a0bfc, dceb7005, f8525f4a; integrated e15af602; clean no-op ef24a624; latest test lint scope'),
 ('Preserve commit-bound remote scheduler validation evidence',accepted,git('diff','--name-only',base,accepted,'--','docs/validation/pr184-remote-mutations-20261109',text=True).splitlines(),'c5415af84335672d206006191d0e87d9b51f8d0a; lossless compression c5214bf827857ce3c6d1a3b92df4e807757c9a78')
]
expected=set(git('diff','--name-only',base,accepted,text=True).splitlines())
covered={p for _,_,paths,_ in groups for p in paths}
if covered!=expected:raise SystemExit('Unassigned/missing paths: '+repr((expected-covered,covered-expected)))
manifest=dict(base=base,accepted=accepted,accepted_tree=rev(accepted+'^{tree}'),branch=branch,units=[])
path=Path('/tmp/gitcraft-pr184-assembly-'+accepted[:8]+'.json')
def save():path.write_text(json.dumps(manifest,indent=2)+'\n')
def stage(source,paths):
    rows=git('ls-tree','-z',source,'--',*paths).split(b'\0')
    found=set();data=b''
    for row in rows:
        if not row:continue
        meta,name=row.split(b'\t',1);mode,kind,blob=meta.split()
        if kind!=b'blob':raise SystemExit('Expected blob: '+repr(row))
        found.add(name.decode());data+=mode+b' '+blob+b'\t'+name+b'\0'
    if found!=set(paths):raise SystemExit('Missing immutable input '+repr(set(paths)-found))
    indexgit('update-index','-z','--index-info',input=data)
indexgit('read-tree',base)
parent=base
for i,(title,source,paths,provenance) in enumerate(groups):
    stage(source,paths)
    if i==1:stage('6179204f0cade63fd200d9530abaee31a72d586f',['src/repo/dialogs/stash.rs'])
    tree=indexgit('write-tree',text=True).strip()
    message=title+'\n\nAccepted combined implementation: '+accepted+'\nProvenance: '+provenance+'\nStatus: WIP pending exact assembled-candidate validation and independent review.\n'
    commit=indexgit('-c','commit.gpgsign=false','commit-tree',tree,'-p',parent,input=message,text=True).strip()
    actual=git('show','-s','--format=%an <%ae>',commit,text=True).strip()
    if actual!=author:raise SystemExit('Unexpected author '+actual)
    git('update-ref',ref,commit)
    subprocess.run(['git','push','origin',ref+':'+ref],check=True)
    remote=git('ls-remote','origin',ref,text=True).split()[0]
    if remote!=commit:raise SystemExit('Remote does not match '+commit)
    manifest['units'].append(dict(title=title,source=source,paths=paths+(['src/repo/dialogs/stash.rs'] if i==1 else []),intermediate_paths={'src/repo/dialogs/stash.rs':'6179204f0cade63fd200d9530abaee31a72d586f'} if i==1 else {},provenance=provenance,commit=commit,tree=tree,remote=remote,author=actual))
    save();print(title,commit,flush=True);parent=commit
actual_map=git('ls-tree','-r','-z','--full-tree',parent)
accepted_map=git('ls-tree','-r','-z','--full-tree',accepted)
if actual_map!=accepted_map:raise SystemExit('Final complete path/mode/blob map differs')
if rev(parent+'^{tree}')!=manifest['accepted_tree']:raise SystemExit('Final tree differs')
manifest.update(final=parent,complete_path_mode_blob_map_sha256=hashlib.sha256(actual_map).hexdigest(),exact_tree_match=True)
save();index.unlink();print('ASSEMBLED',parent,'EXACT TREE',manifest['accepted_tree'],flush=True)
