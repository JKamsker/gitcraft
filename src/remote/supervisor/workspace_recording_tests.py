"""The twelve design schedules, entering production claim/fence/publication/gate/ledger
branches with synthetic descriptors and injected operations. No child, signal, actual
lock, repository or filesystem mutation. Native counterparts run under Rust `recording`.
"""
import importlib.util, unittest
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('owner','src/remote/supervisor/owner.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
A='a'*64;B='b'*64
class World:
    def __init__(self):
        self.owner={};self.generation=b'';self.events=[];self.identities=['directory','primary','fence','generation'];self.next=0;self.waits=0;self.failure=None
        world=self
        class Locks(m.WorkspaceLocks):
            def __init__(self,root,initialize=False):
                world.next+=1;self.id=world.next;self.root=root;self.initialized=False;self.anchor_id='original';self.files=[(kind,self.id) for kind in ('primary','fence','generation')];self.anchor=None;self.directory=None
            def verify(self): m.workspace_namespace(['directory','primary','fence','generation'],world.identities)
            @staticmethod
            def read(fd,limit): return world.generation
            def open_file(self,name,flags,at=None):
                if world.failure=='collision': raise FileExistsError('record collision')
                world.events.append('record-create');return ('record',self.id)
        self.Locks=Locks
    def flock(self,fd,flags):
        kind,owner=fd
        if flags&m.fcntl.LOCK_UN:
            if self.owner.get(kind)==owner:self.owner.pop(kind)
        elif kind in self.owner and self.owner[kind]!=owner: raise BlockingIOError('owned')
        else:self.owner[kind]=owner
    def close(self,fd):
        kind,owner=fd
        if self.owner.get(kind)==owner:self.owner.pop(kind)
        self.events.append('close-'+kind)
    def wait(self,*args):
        self.waits+=1
        if self.waits>4:raise TimeoutError('recorded fence wait expired')
    def write(self,fd,data):self.generation=bytes(data);self.events.append('generation');return len(data)
    def claim(self,generation=A):
        self.waits=0
        return self.Locks.claim(dict(root='/synthetic',generation=generation),None,5)
    def grant(self,generation=A,mode='dispatch'):return dict(root='/synthetic',anchor='original',generation=generation,mode=mode)
    def __enter__(self):
        self.patches=[patch.object(m.fcntl,'flock',self.flock),patch.object(m.os,'close',self.close),patch.object(m.os,'lseek',lambda *a:0),patch.object(m.os,'ftruncate',lambda *a:None),patch.object(m.os,'write',self.write),patch.object(m.os,'fsync',lambda *a:None),patch.object(m,'workspace_wait',self.wait),patch.object(m.time,'sleep',lambda *a:None)]
        for p in self.patches:p.start()
        return self
    def __exit__(self,*args):
        for p in reversed(self.patches):p.stop()
class Schedules(unittest.TestCase):
    def test_01_failed_claimant_close_has_zero_cleanup_or_dispatch(self):
        with World() as w:
            winner=w.claim();published=w.generation;events=len(w.events)
            with self.assertRaises(BlockingIOError):w.claim(B)
            self.assertEqual(w.generation,published);self.assertEqual(w.owner['primary'],winner.id)
            self.assertTrue(all(e.startswith('close-') for e in w.events[events:]))
            winner.close()
    def test_02_equal_pids_and_clocks_never_override_primary_lock(self):
        with World() as w:
            winner=w.claim()
            for clock in [-999999,0,999999]:
                request=dict(root='/synthetic',generation=B,pid=424242,heartbeat=clock)
                with self.assertRaises(BlockingIOError):w.Locks.claim(request,None,clock)
                self.assertEqual(w.owner['primary'],winner.id)
            winner.close()
    def test_03_queued_old_generation_rejected_at_actual_machine_run(self):
        with World() as w:
            old=w.claim();old.close();new=w.claim(B);effects=[]
            with patch.object(m,'WorkspaceLocks',w.Locks),patch.object(m,'run_inner',lambda *args:effects.append('record-or-target')):
                with self.assertRaises(ValueError):m.run(dict(workspace=w.grant(),record='/synthetic/old.owner',milliseconds=1000),None,True)
            self.assertEqual(effects,[]);new.close()
    def test_04_admitted_mutation_retains_fence_after_parent_disconnect(self):
        with World() as w:
            claim=w.claim();effects=[]
            def operation(req,controller,stream,locks):
                claim.close() # Controller loss releases primary, never the helper fence.
                with self.assertRaises(TimeoutError):w.claim(B)
                effects.append('terminal');return 0,b'',b''
            with patch.object(m,'WorkspaceLocks',w.Locks),patch.object(m,'run_inner',operation):
                m.run(dict(workspace=w.grant(mode='mutation'),record='/synthetic/mutation.owner',milliseconds=1000),None,True)
            self.assertEqual(effects,['terminal']);new=w.claim(B);new.close()
    def test_05_publication_faults_and_lease_loss_never_release_unrecorded_start(self):
        owner=dict(v=1,nonce='a',path=m.ROOT+'b'*32,token='c'*64)
        for failure in ['collision','write','flush','sync','close','none']:
            with World() as w:
                claim=w.claim();locks=w.Locks.fence(w.grant(),None,5);locks.directory=type('Dir',(),{'fd':None})();w.failure=failure;events=[]
                class File:
                    def __enter__(self):return self
                    def write(self,data):
                        events.append('write')
                        if failure=='write':raise OSError('truncated write')
                    def flush(self):
                        events.append('flush')
                        if failure=='flush':raise OSError('flush failed')
                    def fileno(self):return None
                    def __exit__(self,*unused):
                        events.append('close')
                        if failure=='close':raise OSError('close failed')
                def sync(*args):
                    events.append('sync')
                    if failure=='sync':raise OSError('sync failed')
                with patch.object(m.os,'fdopen',lambda *a,**k:File()),patch.object(m.os,'fsync',sync):
                    try:locks.publish('/synthetic/'+A+'-dispatch-'+'d'*32+'.owner',owner);published=True
                    except OSError:published=False
                self.assertEqual(published,failure=='none')
                released=[];gate=m.Handshake(b'hello',5)
                self.assertIsNone(gate.step(True,lambda:0,lambda b:len(b),lambda:released.append('start')))
                self.assertEqual(released,[])
                locks.directory=None;locks.close();claim.close()
    def test_06_stop_of_pending_gate_is_sticky(self):
        frame=m.Handshake(b'identity',5);starts=[]
        for _ in range(4):self.assertIsNone(frame.step(True,lambda:0,lambda b:len(b),lambda:starts.append(True)))
        self.assertEqual(starts,[]);self.assertFalse(frame.released)
    def test_07_publication_inventory_boundary_is_fenced(self):
        with World() as w:
            claim=w.claim();locks=w.Locks.fence(w.grant(),None,5);records=[]
            claim.close()
            for boundary in ['before-record','partial-record','complete-record']:
                with self.assertRaises(TimeoutError):w.claim(B)
                if boundary=='complete-record':records.append('durable-owner')
            locks.close();new=w.claim(B)
            self.assertEqual(records,['durable-owner']);self.assertEqual(w.generation,(B+'\n').encode());new.close()
    def test_08_unknown_records_and_wrong_receipts_authorize_no_retirement(self):
        owner=dict(v=1,nonce='a',path=m.ROOT+'b'*32,token='c'*64)
        for data in [b'',b'{',m.encode(dict(owner,state='stopped',code=0,complete=True,extra=True)),m.encode(dict(owner,state='stopped',code=0,complete=False,token='f'*64))]:
            effects=[]
            with self.assertRaises((ValueError,KeyError)):m.checked_receipt(owner,data);effects.append('retire')
            self.assertEqual(effects,[])
    def test_09_replaced_or_unlinked_lock_and_stale_close_never_split_authority(self):
        for index in range(4):
            with World() as w:
                old=w.claim();w.identities[index]='replacement'
                with self.assertRaises(ValueError):w.Locks.fence(w.grant(),None,5)
                generation=w.generation;old.close();self.assertEqual(w.generation,generation)
                with self.assertRaises(ValueError):w.claim(B)
    def ledger(self,refusal=None):
        owner=dict(v=1,nonce='a',path=m.ROOT+'b'*32,token='c'*64);name=A+'-mutation-'+'d'*32+'.owner'
        record=m.encode(dict(v=3,anchor='original',generation=A,kind='mutation',endpoint=owner));receipt=m.encode(m.receipt(owner,'stopped',0))
        proof=dict(name=name,record=m.base64.b64encode(record).decode(),receipt=m.base64.b64encode(receipt).decode());entry=m.ledger_decode(proof,'original');events=[];storage={name:record,name+'.proof':m.encode(proof)}
        class Ledger(m.RecoveryLedger):
            def __init__(self):self.grant=dict(anchor='original',generation=B);self.own=B+'-mutation-'+'e'*32+'.owner';self.entries={name:dict(entry,present=True)};self.manifests=[]
            def active(self):
                if refusal=='lease':raise EOFError('lease lost')
            def verify(self):events.append('verify')
            def publish(self,name,data,limit):events.append('manifest');storage[name]=data
            def raw_read(self,name,limit):return storage[name]
            def remove(self,name,data,limit):
                events.append('prune-'+name)
                if refusal=='prune' and name.endswith('.proof'):raise PermissionError('partial prune')
                self.assert_equal=storage.pop(name)==data
        return Ledger(),events,storage,record
    def test_10_clear_failure_never_calls_ledger_completion_or_erases_evidence(self):
        # Actual shell command/listing failure branches are the recording/clear_failures.py
        # fixture; here the receipt postcondition rejects each failed target outcome.
        for code,delivery in [(1,True),(0,False),(125,False)]:
            ledger,events,storage,record=self.ledger()
            if m.cleanup_complete_allowed(code,delivery):ledger.complete()
            self.assertEqual(events,[]);self.assertIn(record,storage.values());self.assertEqual(m.delivery_code(code,delivery),125 if not delivery else code)
    def test_11_partial_prune_keeps_complete_manifest_for_retry(self):
        ledger,events,storage,record=self.ledger('prune')
        with patch.object(m,'retire_proven',lambda *a:events.append('retire')):
            with self.assertRaises(PermissionError):ledger.complete()
        manifests=[value for name,value in storage.items() if name.endswith('.cleaned')]
        self.assertEqual(len(manifests),1);self.assertIn(m.base64.b64encode(record),manifests[0]);self.assertIn('retire',events)
    def test_12_retire_refusal_and_shutdown_keep_evidence_and_report_failure(self):
        for failure in ['retire','lease']:
            ledger,events,storage,record=self.ledger(failure)
            def retire(*args):events.append('retire');raise PermissionError('retire refused')
            with patch.object(m,'retire_proven',retire):
                with self.assertRaises((PermissionError,EOFError)):ledger.complete()
            self.assertIn(record,storage.values());self.assertFalse(any(e.startswith('prune') for e in events))
if __name__=='__main__':unittest.main()
