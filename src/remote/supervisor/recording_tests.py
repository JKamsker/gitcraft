import importlib.util, unittest
spec = importlib.util.spec_from_file_location('owner', 'src/remote/supervisor/owner.py')
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
class Recording:
    def __init__(self, refuse=False): self.events=[]; self.refuse=refuse
    def killpg(self,p,s):
        self.events.append(('group',p,s))
        if self.refuse: raise PermissionError('recorded refusal')
    def kill(self,p,s): self.events.append(('direct',p,s))
    def waitpid(self,p,f): self.events.append(('reap',p,f)); return p,0
class Pure(unittest.TestCase):
    def test_actual_frame_reader_retains_coalesced_ack_and_request_at_every_split(self):
        data=b'A oldnonce\n{"nonce":"next"}\nP ping\n'
        expected=[b'A oldnonce\n',b'{"nonce":"next"}\n',b'P ping\n',None]
        for split in range(len(data)+1):
            chunks=iter([p for p in (data[:split],data[split:]) if p])
            frames=m.Frames()
            self.assertEqual([frames.read(lambda _:next(chunks,b'')) for _ in range(4)],expected)
        frames=m.Frames()
        with self.assertRaises(ValueError): frames.read(lambda _:b'incomplete' if not frames.pending else b'')
    def test_incomplete_output_is_never_success_even_with_clean_target_exit(self):
        self.assertEqual(m.delivery_code(0,False),125)
        self.assertEqual(m.delivery_code(0,True),0)
        self.assertEqual(m.delivery_code(7,True),7)
    def test_retirement_pins_receipt_and_every_removal_to_one_identity(self):
        owner=dict(v=1,nonce='a',path=m.ROOT+'b'*32,token='c'*64)
        events=[]
        class Directory:
            def __init__(self,path): self.identity='original'; events.append(('pin',path))
            def __enter__(self): return self
            def __exit__(self,*unused): events.append(('close',self.identity))
            def receipt(self):
                events.append(('receipt',self.identity))
                # Simulate pathname substitution after validation; the pinned identity stays.
                self.pathname='foreign'
                return m.encode(m.receipt(owner,'stopped',0))
            def unlink(self,name): events.append(('unlink',self.identity,name)); self.pathname='foreign-again'
        m.retire(owner,Directory)
        self.assertEqual([e for e in events if e[0]=='unlink'],[('unlink','original',n) for n in ('input','control','terminal')])
        self.assertEqual(events[-1],('close','original'))
        events.clear()
        class Missing(Directory):
            def receipt(self): raise ValueError('missing atomic receipt')
        with self.assertRaises(ValueError): m.retire(owner,Missing)
        self.assertFalse(any(e[0]=='unlink' for e in events))
    def test_forwarding_backpressure_never_blocks_control_and_is_bounded(self):
        def blocked(fd, data): raise BlockingIOError()
        forward = m.Forwarder(blocked)
        self.assertTrue(forward.add(0, b'abcdef'))
        self.assertTrue(forward.flush())
        self.assertEqual(forward.pending[0], b'abcdef')
        calls=[]
        forward.write=lambda fd,data: calls.append((fd,bytes(data))) or 2
        self.assertTrue(forward.flush())
        self.assertEqual(forward.pending[0], b'cdef')
        self.assertEqual(calls, [(1,b'abcdef')])
        old=m.MAX
        try:
            m.MAX=8
            self.assertFalse(forward.add(0,b'x'*16))
            self.assertEqual(len(forward.pending[0]),8)
        finally: m.MAX=old
    def test_live_anchor_signal_order(self):
        r=Recording(); m.finish_owned(424242,True,r,lambda _:None)
        self.assertEqual([e[0] for e in r.events],['group','group','reap'])
        self.assertTrue(all(e[1]==424242 for e in r.events))
    def test_before_group_ack_only_the_unreleased_anchor_is_stopped(self):
        r=Recording(); m.finish_owned(424242,False,r,lambda _:None)
        self.assertEqual([e[0] for e in r.events],['direct','reap'])
    def test_refused_group_never_acknowledges_success(self):
        r=Recording(True)
        with self.assertRaises(PermissionError): m.finish_owned(424242,True,r,lambda _:None)
        self.assertEqual([e[0] for e in r.events],['group','direct','reap'])
unittest.main()
