"""R3-B1 production-helper regression cases. Every directory operation is injected;
no filesystem, process, socket, lock or signal operations are performed."""
import importlib.util,unittest
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('owner','src/remote/supervisor/owner.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
OWNER=dict(v=1,nonce='a',path=m.ROOT+'b'*32,token='c'*64)
PROOF=m.encode(m.receipt(OWNER,'stopped',0))
class Fixture:
    def __init__(self,terminal=PROOF,present=True):
        self.files=dict(input=b'data',control=b'endpoint') if present else {}
        if terminal is not None:self.files['terminal']=terminal
        self.events=[];self.refuse=None;self.missing=False
    def pin(self,path):
        self.events.append('pin')
        if self.missing:raise FileNotFoundError('already absent')
        return self
    def __enter__(self):return self
    def __exit__(self,*args):pass
    def receipt(self):
        self.events.append('read-current')
        if 'terminal' not in self.files:raise FileNotFoundError('missing terminal')
        return self.files['terminal']
    def artifacts_absent(self):self.events.append('check-absent');return not self.files
    def unlink(self,name):
        self.events.append('unlink-'+name)
        if self.refuse==name:raise PermissionError('recorded retirement failure')
        self.files.pop(name,None)
    def retire(self):
        with patch.object(m,'PinnedDirectory',self.pin):m.retire_proven(OWNER,PROOF)
class CachedRetirement(unittest.TestCase):
    def test_foreign_or_changed_pinned_receipt_refuses_every_deletion(self):
        control=Fixture(m.encode(dict(m.receipt(OWNER,'stopped',0),token='f'*64)))
        with self.assertRaises(ValueError):m.retire(OWNER,control.pin)
        self.assertEqual(control.events,['pin','read-current'])
        self.assertEqual(len(control.files),3)
        for terminal in [m.encode(dict(m.receipt(OWNER,'stopped',0),token='f'*64)),m.encode(m.receipt(OWNER,'stopped',7)),b'{']:
            f=Fixture(terminal);original=f.files.copy()
            with self.assertRaises(ValueError):f.retire()
            self.assertEqual(f.files,original);self.assertEqual(f.events,['pin','read-current'])
    def test_missing_terminal_with_present_artifacts_refuses_and_retains(self):
        f=Fixture(None);original=f.files.copy()
        with self.assertRaises(ValueError):f.retire()
        self.assertEqual(f.files,original);self.assertEqual(f.events,['pin','read-current','check-absent'])
    def test_absent_directory_is_acknowledged_without_any_removal(self):
        f=Fixture();f.missing=True;f.retire();self.assertEqual(f.events,['pin'])
    def test_absent_terminal_and_all_artifacts_never_call_unlink(self):
        f=Fixture(None,False);f.retire();self.assertEqual(f.events,['pin','read-current','check-absent'])
    def test_authenticated_match_and_repeated_retirement_are_idempotent(self):
        f=Fixture();f.retire();self.assertEqual(f.files,{})
        self.assertEqual(f.events,['pin','read-current','unlink-input','unlink-control','unlink-terminal'])
        f.events=[];f.retire();self.assertEqual(f.events,['pin','read-current','check-absent'])
    def test_partial_retirement_retry_reauthenticates_current_terminal(self):
        f=Fixture();f.refuse='control'
        with self.assertRaises(PermissionError):f.retire()
        self.assertIn('terminal',f.files);self.assertNotIn('input',f.files)
        f.refuse=None;f.events=[];f.retire();self.assertEqual(f.files,{})
        self.assertEqual(f.events[:2],['pin','read-current'])
    def test_foreign_directory_failure_keeps_external_manifest_and_record(self):
        f=Fixture(m.encode(dict(m.receipt(OWNER,'stopped',0),token='f'*64)))
        generation='d'*64;name=generation+'-mutation-'+'e'*32+'.owner'
        record=m.encode(dict(v=3,anchor='original',generation=generation,kind='mutation',endpoint=OWNER))
        entry=dict(name=name,record=record,receipt=PROOF,owner=OWNER,present=True)
        storage={name:record};events=[]
        class Ledger(m.RecoveryLedger):
            def __init__(self):self.grant=dict(anchor='original',generation=generation);self.own=generation+'-mutation-'+'f'*32+'.owner';self.entries={name:entry};self.manifests=[]
            def active(self):pass
            def verify(self):events.append('verify')
            def publish(self,name,data,limit):storage[name]=data;events.append('manifest')
            def remove(self,*args):events.append('prune');raise AssertionError('must retain evidence')
        with patch.object(m,'PinnedDirectory',f.pin):
            with self.assertRaises(ValueError):Ledger().complete()
        self.assertEqual(events,['verify','manifest']);self.assertEqual(storage[name],record)
        self.assertEqual(sum(key.endswith('.cleaned') for key in storage),1)
        self.assertFalse(any(e.startswith('unlink-') for e in f.events))
if __name__=='__main__':unittest.main()
