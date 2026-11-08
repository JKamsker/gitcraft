"""Recording only: every git/rm command in actual CLEAR is a shell function.
All paths are nonexistent /__gitcraft_recording__ names. No repository/file mutation,
helper, target child, filesystem race or signal is exercised.
"""
import pathlib,re,subprocess,unittest
source=pathlib.Path('src/parallel_bisect/workspace.rs').read_text()
script=re.search(r'const CLEAR: &str = r#"(.*?)"#;',source,re.S).group(1)
common='/__gitcraft_recording__/common'
base=common+'/gitcraft-bisect-work'; ledger=common+'/gitcraft-bisect-owners'
generation='a'*64; own=generation+'-mutation-'+'b'*32+'.owner'
proof=('G 1\n'+generation+'\nR 0\n').encode()
stubs=r'''
removed=0
refs_removed=0
git() {
  printf 'RECORDED git %s\n' "$*" >&2
  case "$*" in
    '-c core.quotePath=false worktree list --porcelain')
      [ "$FAIL" != list ] || return 31
      if [ "$removed" = 1 ]; then [ "$FAIL" != verify_worktrees ] || return 32
      else printf 'worktree %s/worktree\n' "$BASE"; fi;;
    'worktree remove '*) [ "$FAIL" != worktree ] || return 37; removed=1;;
    'rev-parse --git-common-dir') [ "$FAIL" != metadata ] || return 38; printf '%s\n' "$COMMON";;
    'for-each-ref --format=%(refname) %(objectname) refs/gitcraft-bisect/')
      [ "$FAIL" != list_refs ] || return 33
      if [ "$FAIL" = foreign ]; then printf 'refs/gitcraft-bisect/foreign/ref 0123456789abcdef\n'
      elif [ "$refs_removed" = 0 ]; then printf 'refs/gitcraft-bisect/%s/sent-test 0123456789abcdef\n' "$GENERATION"; fi;;
    'for-each-ref --format=%(refname) refs/gitcraft-bisect/')
      [ "$FAIL" != verify_refs ] || return 34
      [ "$refs_removed" = 1 ] || printf 'refs/gitcraft-bisect/%s/sent-test\n' "$GENERATION";;
    'update-ref -d '*) [ "$FAIL" != refs ] || return 39; refs_removed=1;;
    *) printf 'unexpected command\n' >&2; return 90;;
  esac
  return 0
}
rm() { printf 'RECORDED rm %s\n' "$*" >&2; [ "$FAIL" != removal ]; }
'''
def run(failure='none',data=proof):
    prefix='\n'.join(k+'='+v for k,v in [('FAIL',failure),('BASE',base),('COMMON',common),('GENERATION',generation)])+'\n'
    return subprocess.run(['/bin/sh','-c',prefix+stubs+script,'recording',base,ledger,own,generation],input=data,capture_output=True)
class PureCommands(unittest.TestCase):
    def test_actual_clear_never_succeeds_after_independent_command_failures(self):
        for failure,detail in [('list','cannot list worktrees'),('list_refs','cannot list bisect refs'),('worktree','cannot remove worktree'),('metadata','cannot resolve worktree metadata'),('refs','cannot remove unchanged ref'),('removal','cannot remove workspace'),('verify_worktrees','cannot verify remaining worktrees'),('verify_refs','cannot verify remaining refs')]:
            result=run(failure)
            self.assertNotEqual(result.returncode,0,result.stderr)
            self.assertIn(detail.encode(),result.stderr)
            self.assertIn(b'external recovery ledger retained',result.stderr)
            if failure in ('list','list_refs','worktree'): self.assertNotIn(b'RECORDED rm',result.stderr)
    def test_foreign_generation_and_invalid_inventory_have_zero_mutations(self):
        for failure,data in [('foreign',proof),('none',b'G 0\nR 0\n'),('none',b'G 1\nold\nR 0\n'),('none',('G 1\n'+generation+'\nR 1\n').encode()),('none',proof+b'extra\n')]:
            result=run(failure,data)
            self.assertNotEqual(result.returncode,0,result.stderr)
            for mutation in (b'RECORDED rm',b'RECORDED git worktree remove',b'RECORDED git update-ref'): self.assertNotIn(mutation,result.stderr)
    def test_success_requires_checked_postconditions_and_stale_oid_ref_guard(self):
        result=run()
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn(('update-ref -d refs/gitcraft-bisect/'+generation+'/sent-test 0123456789abcdef').encode(),result.stderr)
        self.assertIn(('RECORDED rm -rf -- '+base).encode(),result.stderr)
unittest.main()
