import os,subprocess,json,pathlib,hashlib,time,re,sys
root=pathlib.Path.cwd()
output=pathlib.Path('/tmp/gitcraft-visual-corrections-final')
output.mkdir(exist_ok=True)
env=os.environ.copy();env.update(TMPDIR='/private/tmp',SHELL='/tmp/gitcraft-integration-tools/validation-shell',CARGO_BUILD_JOBS='2')
env['PATH']='/tmp/gitcraft-integration-tools:'+env['PATH']
commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
if subprocess.check_output(['git','status','--porcelain'],text=True).strip(): raise SystemExit('working tree must be clean')
manifest={'application_commit':commit,'base':'883c14d8','environment':{'TMPDIR':env['TMPDIR'],'SHELL':env['SHELL'],'PATH_prefix':'/tmp/gitcraft-integration-tools','CARGO_BUILD_JOBS':'2'},'checks':[],'groups':[]}
def save(): (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
def run(name,args,kind='checks'):
 log=output/(name+'.log');t=time.monotonic()
 with log.open('w') as f: result=subprocess.run(args,env=env,stdout=f,stderr=subprocess.STDOUT)
 data=log.read_bytes();entry={'name':name,'argv':args,'returncode':result.returncode,'log':log.name,'sha256':hashlib.sha256(data).hexdigest(),'seconds':round(time.monotonic()-t,3)}
 manifest[kind].append(entry);save();print(name,result.returncode,flush=True)
 if kind=='groups':
  found=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',data.decode(errors='replace'))
  entry['counts']=[{'passed':int(a),'failed':int(b),'ignored':int(c)} for a,b,c in found];save()
 return result.returncode
for name,args in [('fmt',['cargo','+1.95.0','fmt','--check']),('clippy-qualified',['cargo','+1.95.0','clippy','--locked','--all-targets','--','-D','warnings','-A','dead_code'])]:
 if run(name,args):sys.exit(1)
run('clippy-strict',['cargo','+1.95.0','clippy','--locked','--all-targets','--','-D','warnings'])
if run('test-build',['cargo','+1.95.0','test','--locked','--lib','--no-run','--message-format=json']):sys.exit(1)
artifacts=[]
for line in (output/'test-build.log').read_text().splitlines():
 try: item=json.loads(line)
 except ValueError:continue
 if item.get('reason')=='compiler-artifact' and item.get('executable') and item.get('target',{}).get('name')=='gitcraft':artifacts.append(item)
assert len(artifacts)==1,artifacts
binary=artifacts[0]['executable'];manifest['test_binary']={'path':binary,'sha256':hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest()};save()
if run('production-build',['cargo','+1.95.0','build','--locked']):sys.exit(1)
filters=['repo::branch_bands::','repo::graph_view::minimap::','git::activity::','repo::history_activity::','git::atlas::','repo::atlas::','git::growth::','repo::growth::','repo::postcards::','git::postcards::','git::visualization_tests::','repo::graph_walk::','repo::celebration::','repo::graph_motion::','repo::history_reader::','audio::tests::','graph::tests::','repo::graph_view::','repo::navigate::tests::','a11y::tests::repo_view::','repo::lifecycle::tests::','repo::graph_order::tests::','repo::authorship_view::','repo::push::tests::']
listing=subprocess.check_output([binary,'--list'],env=env,text=True)
(output/'test-list.log').write_text(listing)
manifest['selected_unique_test_names']=sorted({line.split(': test')[0] for line in listing.splitlines() if ': test' in line and any(f in line for f in filters)})
for i,f in enumerate(filters):
 if run(f'group-{i:02d}',[binary,f,'--nocapture'],'groups'):sys.exit(1)
manifest['all_required_local_gates_pass']=True
manifest['final_application_commit']=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
assert manifest['final_application_commit']==commit
assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip()
save();print('ALL LOCAL GATES PASSED',flush=True)
