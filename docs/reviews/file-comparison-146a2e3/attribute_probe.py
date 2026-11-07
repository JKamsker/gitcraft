from pathlib import Path
import subprocess,json,os
root=Path('/workspace/work/review-file-comparison-146a2e3/attribute-fixture');root.mkdir(exist_ok=True)
env=os.environ.copy()
for key in list(env):
 if key.startswith('GIT_'):env.pop(key)
env.update(GIT_CONFIG_NOSYSTEM='1',GIT_CONFIG_GLOBAL='/dev/null',GIT_ALLOW_PROTOCOL='file')
def run(repo,*args,input=None,check=True):
 p=subprocess.run(['git','-C',str(repo),*args],input=input,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env)
 if check and p.returncode:raise RuntimeError((args,p.stderr.decode(errors='replace')))
 return p
src=root/'source';src.mkdir(exist_ok=True)
run(src,'init','-q','--template=','-b','main');run(src,'config','user.name','Fixture');run(src,'config','user.email','fixture@example.invalid');run(src,'config','commit.gpgsign','false');run(src,'config','uploadpack.allowFilter','true')
(src/'file.txt').write_bytes(b'plain\n');(src/'.gitattributes').write_text('file.txt text eol=lf\n');run(src,'add','.');run(src,'commit','-qm','Base')
p=root/'partial';run(root,'clone','--no-checkout','--no-local','--filter=blob:none',str(src),str(p))
run(p,'--no-lazy-fetch','read-tree','HEAD')
run(p,'--no-lazy-fetch','hash-object','-w','--stdin',input=b'plain\n');(p/'file.txt').write_bytes(b'plain\n')
attr=run(src,'rev-parse','HEAD:.gitattributes').stdout.strip().decode()
before=run(p,'--no-lazy-fetch','cat-file','-e',attr,check=False).returncode
exact=['--literal-pathspecs','check-attr','-z','filter','working-tree-encoding','eol','text','--','file.txt']
# Exact argv from production lens::attributes, preserving the missing working-tree attributes fallback.
out=run(p,*exact)
after=run(p,'--no-lazy-fetch','cat-file','-e',attr,check=False).returncode
report={'git':run(root,'--version').stdout.decode().strip(),'production_argv':exact,'attribute_blob_missing_before':before!=0,'attribute_blob_present_after':after==0,'attributes_response':out.stdout.decode(),'stderr':out.stderr.decode()}
(root.parent/'attribute-probe.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
