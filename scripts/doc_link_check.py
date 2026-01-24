import os
import re
import sys

bad=[]
for root,_,files in os.walk("docs"):
    for fn in files:
        if not fn.endswith('.md'):
            continue
        path = os.path.join(root, fn)
        with open(path, 'r', encoding='utf-8') as fh:
            text = fh.read()
        for m in re.findall(r'\]\(([^)]+)\)', text):
            if m.startswith('http://') or m.startswith('https://') or m.startswith('#') or m.startswith('mailto:'):
                continue
            fp = m.split('#')[0]
            if not fp:
                continue
            candidate = os.path.normpath(os.path.join(root, fp))
            if not os.path.exists(candidate):
                bad.append((path, m, candidate))

if bad:
    print('Missing internal links:')
    for p,m,c in bad:
        print(f"{p} -> {m} -> {c}")
    sys.exit(1)
else:
    print('All internal doc links resolved OK')
