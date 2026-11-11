from pathlib import Path
import copy
import importlib.util
import json
import unittest

spec = importlib.util.spec_from_file_location('bindings', Path(__file__).with_name('gitcraft_pr184_approval_binding.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

class Bindings(unittest.TestCase):
    def fixture(self):
        runner, package, verifier = b'runner', b'package', b'verifier'
        result = json.dumps(dict(commit='commit', tree='tree', status='passed', runner_sha256=m.digest(runner))).encode()
        manifest = dict(full_results_sha256=m.digest(result), executed_runner_sha256=m.digest(runner))
        records = [dict(task_id=scope, scope=scope, verdict='PASS', reviewer=dict(provider='codex', model='same-model', options={'reasoningEffort':'high'}), reviewed_commit='commit', reviewed_tree='tree') for scope in ['source','execution_evidence','evidence_packaging','inheritance_verifier']]
        records[1].update(full_results_sha256=m.digest(result), executed_runner_sha256=m.digest(runner))
        records[2]['script_sha256']=m.digest(package)
        records[3]['script_sha256']=m.digest(verifier)
        return manifest, result, runner, records, package, verifier
    def check(self, fixture):
        manifest, result, runner, records, package, verifier = fixture
        return m.validate_committed_execution(manifest, result, runner, json.dumps(records).encode(), package, verifier, 'commit', 'tree')
    def test_identity_and_independence(self):
        self.check(self.fixture())
        for invalid in [None, '', '   ', 7]:
            f=self.fixture(); f[3][0]['task_id']=invalid
            with self.assertRaises(ValueError): self.check(f)
        for reviewer in [None, {}, dict(provider='',model='m'), dict(provider='p',model=None)]:
            f=self.fixture(); f[3][0]['reviewer']=reviewer
            with self.assertRaises(ValueError): self.check(f)
        f=self.fixture(); f[3][1]['task_id']=f[3][0]['task_id']
        with self.assertRaises(ValueError): self.check(f)
    def test_rewritten_results_and_runner_cannot_inherit_stale_approval(self):
        for refresh_top_level in [False, True]:
            f=list(self.fixture()); f[1]=f[1]+b'\n'
            if refresh_top_level: f[0]['full_results_sha256']=m.digest(f[1])
            with self.assertRaises(ValueError): self.check(f)
        f=list(self.fixture()); f[2]=b'wrong runner'
        with self.assertRaises(ValueError): self.check(f)
        f=list(self.fixture()); r=json.loads(f[1]);r['tree']='other';f[1]=json.dumps(r).encode();f[0]['full_results_sha256']=m.digest(f[1]);f[3][1]['full_results_sha256']=m.digest(f[1])
        with self.assertRaises(ValueError): self.check(f)
        for index in (2,3):
            f=self.fixture();f[3][index]['script_sha256']='wrong'
            with self.assertRaises(ValueError): self.check(f)
    def test_chain_pins_base_order_complete_parents_and_terminal(self):
        identity='Jonas Kamsker <11245306+JKamsker@users.noreply.github.com>'
        plan=[('one',['a']),('two',['b'])]
        units=[dict(title='one',paths=['a'],parent='base',commit='c1',tree='t1',remote='c1'),dict(title='two',paths=['b'],parent='c1',commit='c2',tree='t2',remote='c2')]
        assembly=dict(accepted='accepted',base='base',final='c2',accepted_tree='t2',exact_tree_match=True,units=units)
        for unit in units:unit['source']='accepted'
        objects={u['commit']:dict(parents=[u['parent']],tree=u['tree'],author=identity,committer=identity,paths=u['paths']) for u in units}
        m.validate_chain(assembly,objects,'base','c2','t2',plan,'accepted')
        mutations=[lambda a,o:a.update(accepted='HEAD'),lambda a,o:a['units'][0].update(source='c2'),lambda a,o:a.update(base='substitute'),lambda a,o:a['units'].reverse(),lambda a,o:a['units'].pop(),lambda a,o:a['units'].append(a['units'][0]),lambda a,o:o['c1']['parents'].append('merge-parent'),lambda a,o:a.update(final='c1')]
        for change in mutations:
            a,o=copy.deepcopy(assembly),copy.deepcopy(objects);change(a,o)
            with self.assertRaises(ValueError):m.validate_chain(a,o,'base','c2','t2',plan,'accepted')

class LogBindings(unittest.TestCase):
    def test_refreshed_manifest_cannot_substitute_approved_logs(self):
        import gzip
        raw=b'approved execution log'
        compressed=gzip.compress(raw,mtime=0)
        results=dict(checks=[dict(label='gate',sha256=m.digest(raw))])
        logs=[dict(gate='gate',path='log.gz',raw_sha256=m.digest(raw),archive_sha256=m.digest(compressed))]
        m.validate_log_bindings(results,logs,lambda path:compressed)
        substituted=gzip.compress(b'substituted log',mtime=0)
        changed=copy.deepcopy(logs)
        changed[0].update(raw_sha256=m.digest(b'substituted log'),archive_sha256=m.digest(substituted))
        with self.assertRaises(ValueError):m.validate_log_bindings(results,changed,lambda path:substituted)
        for invalid in [[],logs+logs,[dict(logs[0],gate='wrong')]]:
            with self.assertRaises(ValueError):m.validate_log_bindings(results,invalid,lambda path:compressed)
        with self.assertRaises(ValueError):m.validate_log_bindings(dict(checks=results['checks']*2),logs,lambda path:compressed)

class AssemblyApproval(unittest.TestCase):
    def test_rewritten_committed_assembly_cannot_keep_stale_approval(self):
        original=b'{"accepted":"approved"}'
        record=dict(scope='assembly',verdict='PASS',task_id='assembly-review',reviewer=dict(provider='codex',model='same-model'),reviewed_commit='commit',reviewed_tree='tree',assembly_sha256=m.digest(original))
        m.validate_assembly_approval([record],'commit','tree',original)
        for changed in [b'{"accepted":"HEAD"}',b'{"base":"substituted"}',b'{"provenance":["wrong"]}']:
            with self.assertRaises(ValueError):m.validate_assembly_approval([record],'commit','tree',changed)

class AssemblerCompareAndSwap(unittest.TestCase):
    def test_actual_ref_update_refuses_concurrent_writer_before_push(self):
        import ast
        import types
        assembler=Path(__file__).with_name('assemble.py')
        if not assembler.exists():assembler=Path(__file__).with_name('gitcraft-pr184-round4-assemble.py')
        tree=ast.parse(assembler.read_text())
        updates=[node for node in ast.walk(tree) if isinstance(node,ast.Expr) and isinstance(node.value,ast.Call) and isinstance(node.value.func,ast.Name) and node.value.func.id=='git' and node.value.args and isinstance(node.value.args[0],ast.Constant) and node.value.args[0].value=='update-ref']
        self.assertEqual(len(updates),1)
        blocks=[node.body for node in ast.walk(tree) if isinstance(node,ast.For) and any(item is updates[0] for item in node.body)]
        block=blocks[0];index=block.index(updates[0]);commands=ast.Module(body=block[index:index+2],type_ignores=[])
        code=compile(ast.fix_missing_locations(commands),'actual-assembler-CAS','exec')
        for prior_units,expected_old in [([],''),([{}],'parent')]:
            for collision in [False,True]:
                calls=[]
                def git(*args):
                    calls.append(args)
                    if collision:raise RuntimeError('ref changed concurrently')
                namespace=dict(git=git,ref='refs/heads/fixture',commit='next',parent='parent',manifest={'units':prior_units},subprocess=types.SimpleNamespace(run=lambda *args,**kwargs:calls.append(('push',args))))
                if collision:
                    with self.assertRaises(RuntimeError):exec(code,namespace)
                else:exec(code,namespace)
                self.assertEqual(calls[0],('update-ref','refs/heads/fixture','next',expected_old))
                self.assertEqual(len(calls),1 if collision else 2)

if __name__ == '__main__': unittest.main()
