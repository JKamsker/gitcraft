from pathlib import Path
import importlib.util,unittest,types,json
from collections import Counter
from unittest.mock import patch
repo=Path('/home/jonas/.t3/worktrees/gitcraft/t3-572729bb');spec=importlib.util.spec_from_file_location('shards',repo/'ci/test-shards.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
names=['standalone::save::acl','standalone::model::merge','other::test','git::alpha','repo::zulu']+[n for group in m.ISOLATED.values() for n in group]
def inventory(binaries,filters):
 includes=[];skips=[];exact='--exact' in filters;it=iter(filters)
 for x in it:
  if x=='--skip':skips.append(next(it))
  elif not x.startswith('--'):includes.append(x)
 return Counter({n:1 for n in names if (not includes or any(n==i if exact else i in n for i in includes)) and not any(s in n for s in skips)})
class Tests(unittest.TestCase):
 def test_core_runs_serial_once_after_parallel_without_dropping_tests(self):
  calls=[]
  def run(args,**kw):calls.append(args);return types.SimpleNamespace(returncode=0)
  with patch.object(m,'inventory',inventory),patch.object(m.subprocess,'run',run):self.assertEqual(m.run_shard([(Path('fixture'),repo)],'core'),0)
  self.assertEqual(sum('standalone::' in c and '--test-threads=1' in c for c in calls),1)
  self.assertIn('--skip',calls[0]);self.assertNotIn('--test-threads=1',calls[0]);self.assertEqual(calls[1],['fixture','--nocapture','standalone::','--test-threads=1'])
  full=inventory([],[]);assigned=Counter()
  for shard,filters in m.SHARDS.items():assigned.update(inventory([],filters));
  with patch.object(m,'inventory',inventory):
   for shard in m.SHARDS:assigned.update(m.isolated_inventory([],shard));assigned.update(m.serial_inventory([],shard))
  self.assertEqual(assigned,full)
 def test_serial_failure_prevents_success(self):
  def run(args,**kw):return types.SimpleNamespace(returncode=17 if '--test-threads=1' in args else 0)
  with patch.object(m,'inventory',inventory),patch.object(m.subprocess,'run',run):self.assertEqual(m.run_shard([(Path('fixture'),repo)],'core'),17)
 def test_empty_serial_group_refuses(self):
  with patch.object(m,'inventory',lambda *_:Counter()):
   with self.assertRaises(RuntimeError):m.serial_inventory([],'core')
 def test_serial_overlap_refuses(self):
  with patch.object(m,'inventory',inventory),patch.dict(m.SERIAL_GROUPS,{'core':['standalone::','standalone::save']}):
   with self.assertRaises(RuntimeError):m.serial_inventory([],'core')
program=unittest.main(exit=False)
if not program.result.wasSuccessful():
 raise SystemExit(1)
# Real immutable binary inventory only: --list performs no tests or process-control fixture.
d=json.loads(Path('/tmp/gitcraft-pr184-round5-full-661be439-o954mljl/results.json').read_text());binary=next(x['executable'] for x in d['artifacts'] if x['source']=='src/lib.rs')
m.verify_partition([(Path(binary),repo)])
