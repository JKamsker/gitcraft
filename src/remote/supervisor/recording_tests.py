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
