"""Recording only: every git/rm call in the actual CLEAR script is a shell function.
No repository, workspace, helper, target process, filesystem mutation or signal is used.
"""
import pathlib, re, subprocess, unittest
source=pathlib.Path('src/parallel_bisect/workspace.rs').read_text()
script=re.search(r'const CLEAR: &str = r#"(.*?)"#;',source,re.S).group(1)
base='/__gitcraft_recording__/.cache/gitcraft/bisect/run-000000000000'
stubs=r'''
removed=0
refs_removed=0
git() {
  printf 'RECORDED git %s\n' "$*" >&2
  case "$*" in
    '-c core.quotePath=false worktree list --porcelain')
      [ "$removed" = 1 ] || printf 'worktree %s/worktree\n' "$1_BASE";;
    'worktree remove '* ) [ "$FAIL" != worktree ] || return 37; removed=1;;
    'rev-parse --git-common-dir') printf '/__gitcraft_recording__/common\n';;
    'for-each-ref --format=%(refname) %(objectname) refs/gitcraft-bisect/')
      [ "$refs_removed" = 1 ] || printf 'refs/gitcraft-bisect/sent-test 0123456789abcdef\n';;
    'for-each-ref --format=%(refname) refs/gitcraft-bisect/')
      [ "$refs_removed" = 1 ] || printf 'refs/gitcraft-bisect/sent-test\n';;
    'update-ref -d '*) [ "$FAIL" != refs ] || return 39; refs_removed=1;;
    *) printf 'unexpected command\n' >&2; return 90;;
  esac
  return 0
}
rm() { printf 'RECORDED rm %s\n' "$*" >&2; [ "$FAIL" != removal ]; }
'''.replace('"$1_BASE"', '"'+base+'"')
class PureCommands(unittest.TestCase):
    def test_actual_clear_never_succeeds_after_failed_mutation(self):
        for failure, detail in [('worktree','cannot remove worktree'),('removal','cannot remove workspace'),('refs','cannot remove unchanged ref')]:
            result=subprocess.run(['/bin/sh','-c', 'FAIL='+failure+'\n'+stubs+script,'recording',base,'424242'], input=b'',capture_output=True)
            self.assertNotEqual(result.returncode,0,result.stderr)
            self.assertIn(detail.encode(),result.stderr)
            self.assertIn(b'endpoint artifacts retained',result.stderr)
            if failure=='worktree': self.assertNotIn(b'RECORDED rm',result.stderr)
    def test_recorded_success_requires_all_mutations_and_stale_ref_guard(self):
        result=subprocess.run(['/bin/sh','-c','FAIL=none\n'+stubs+script,'recording',base,'424242'],input=b'',capture_output=True)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertIn(b'update-ref -d refs/gitcraft-bisect/sent-test 0123456789abcdef',result.stderr)
unittest.main()
