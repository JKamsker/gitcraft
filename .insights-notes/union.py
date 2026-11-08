import re, sys
p = sys.argv[1]
s = open(p).read()
def fix(m):
    lines = m.group(1).splitlines() + m.group(2).splitlines()
    seen = []
    for l in lines:
        if l not in seen: seen.append(l)
    if all(l.startswith('pub mod ') or l.startswith('pub(crate) mod ') or l.startswith('mod ') for l in seen):
        seen.sort()
    return "\n".join(seen) + "\n"
s2 = re.sub(r'<<<<<<< [^\n]*\n(.*?)=======\n(.*?)>>>>>>> [^\n]*\n', fix, s, flags=re.S)
open(p, 'w').write(s2)
