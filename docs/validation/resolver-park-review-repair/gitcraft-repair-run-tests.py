import json, os, pathlib, subprocess, sys, time, tempfile
artifacts=[]
for line in pathlib.Path('/tmp/gitcraft-repair-final-build.jsonl').read_text().splitlines():
    try: item=json.loads(line)
    except ValueError: continue
    if item.get('reason')=='compiler-artifact' and item.get('target',{}).get('name')=='gitcraft' and item.get('profile',{}).get('test') and item.get('executable'):
        artifacts.append(item['executable'])
if not artifacts: sys.exit('No final test artifact')
binary=artifacts[-1]
env=os.environ.copy();env['PATH']='/tmp/gitcraft-git250/install/bin:/tmp/gitcraft-lfs/root/usr/bin:'+env['PATH']
prefix=['bwrap','--unshare-pid','--die-with-parent','--bind','/','/','--dev','/dev','--proc','/proc','--',binary]
filters=[
 ('recording-mapping','resolver::process::tests::recording_failure_mapping_and_join_proof_fail_closed'),
 ('recording-service','processes::recording_outcomes::'),
 ('resolver-process','resolver::process::tests::'),
 ('parking','resolver::park::tests::'),
 ('uncertain-retention','repo::resolver::flow_tests::uncertain_cleanup_preserves_workspace_record_and_blocks_apply'),
 ('insights-dispatch','repo::resolver::flow_tests::insights_action_closes_only_the_read_only_resolver_review'),
 ('resolver-flow','repo::resolver::flow_tests::'),
]
results=[]
for name, pattern in filters:
    env['TMPDIR']=tempfile.mkdtemp(prefix='gitcraft-repair-'+name+'-')
    started=time.time(); log=f'/tmp/gitcraft-repair-{name}.log'
    with open(log,'w') as out:
        proc=subprocess.run(prefix+[pattern,'--nocapture','--test-threads=1'],env=env,stdout=out,stderr=subprocess.STDOUT)
    result={'name':name,'pattern':pattern,'code':proc.returncode,'seconds':round(time.time()-started,2),'log':log}
    results.append(result);print(json.dumps(result),flush=True)
    pathlib.Path('/tmp/gitcraft-repair-results.json').write_text(json.dumps({'commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binary':binary,'results':results},indent=2))
    if proc.returncode: sys.exit(proc.returncode)
