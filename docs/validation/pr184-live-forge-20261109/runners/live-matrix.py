from pathlib import Path
import os,json,subprocess,tempfile,hashlib,shlex
repo=Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb');os.chdir(repo)
sha='661be4398e55117cad723ecb0962c6b86f1c1270'
fullpath=Path('/tmp/gitcraft-pr184-live-final-full-progress.log').read_text().splitlines()[0]
full=json.loads((Path(fullpath)/'results.json').read_text());assert full.get('status')=='passed' and full['commit']==sha
binary=next(x for x in full['artifacts'] if x['source']=='src/lib.rs');assert hashlib.file_digest(Path(binary['executable']).open('rb'),'sha256').hexdigest()==binary['sha256']
root=Path(tempfile.mkdtemp(prefix='gitcraft-pr184-live-matrix-'));print(root,flush=True)
result=dict(commit=sha,tree=full['tree'],binary=binary,full_results_sha256=hashlib.sha256((Path(fullpath)/'results.json').read_bytes()).hexdigest(),runner_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),matrices=[])
env=dict(os.environ,PATH='/tmp/gitcraft-git252/install/bin:/tmp/gitcraft-lfs/root/usr/bin:'+os.environ['PATH'],GIT_CONFIG_GLOBAL='/dev/null',GIT_CONFIG_NOSYSTEM='1')
for forgejo,gitea in [('17.0.1','28.3.0'),('15.0.11','1.26.4')]:
 scratch=Path(tempfile.mkdtemp(prefix='gitcraft-pr184-live-owned-'));envfile=scratch/'forges.env'
 assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==sha and not subprocess.check_output(['git','status','--porcelain'],text=True).strip()
 (scratch/'home').mkdir()
 runtime=dict(env,HOME=str(scratch/'home'),GIT_CONFIG_GLOBAL=str(scratch/'gitconfig'),FORGEJO_VERSION=forgejo,GITEA_VERSION=gitea,FORGE_CACHE='/tmp/gitcraft-pr184-live-forge-cache',FORGE_WORK=str(scratch/'servers'),TMPDIR=str(scratch),XDG_CONFIG_HOME=str(scratch/'config'),XDG_DATA_HOME=str(scratch/'data'),XDG_CACHE_HOME=str(scratch/'cache'))
 script='set -eu\nmkdir -p '+shlex.quote(str(scratch/'forgejo-home'))+' '+shlex.quote(str(scratch/'gitea-home'))+'\nHOME='+shlex.quote(str(scratch/'forgejo-home'))+' GIT_CONFIG_GLOBAL='+shlex.quote(str(scratch/'forgejo.gitconfig'))+' ci/forge-server.sh forgejo 3000 '+shlex.quote(str(envfile))+'\nHOME='+shlex.quote(str(scratch/'gitea-home'))+' GIT_CONFIG_GLOBAL='+shlex.quote(str(scratch/'gitea.gitconfig'))+' ci/forge-server.sh gitea 3001 '+shlex.quote(str(envfile))+'\nset -a\n. '+shlex.quote(str(envfile))+'\nset +a\n'+shlex.quote(binary['executable'])+' forge::gitea::live --ignored --nocapture --test-threads=1\n'
 command=['bwrap','--unshare-pid','--die-with-parent','--bind','/','/','--dev','/dev','--proc','/proc','--','bash','-c',script]
 log=root/('forgejo-'+forgejo+'-gitea-'+gitea+'.log')
 with log.open('w') as out:code=subprocess.run(command,env=runtime,stdout=out,stderr=subprocess.STDOUT).returncode
 raw=log.read_bytes();entry=dict(forgejo=forgejo,gitea=gitea,exit=code,command=command,log=str(log),sha256=hashlib.sha256(raw).hexdigest(),containment='PID/proc/dev namespace; owned fresh server work directories; no existing process cleanup')
 result['matrices'].append(entry);(root/'results.json').write_text(json.dumps(result,indent=2)+'\n');print(forgejo,gitea,code,flush=True)
 assert code==0 and b'test result: ok. 6 passed; 0 failed; 0 ignored;' in raw
 assert subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()==sha and not subprocess.check_output(['git','status','--porcelain'],text=True).strip()
 assert hashlib.file_digest(Path(binary['executable']).open('rb'),'sha256').hexdigest()==binary['sha256']
result['status']='passed';(root/'results.json').write_text(json.dumps(result,indent=2)+'\n')
