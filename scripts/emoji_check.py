import os, sys
emoji_found = []
for root, _, files in os.walk('docs'):
    for fn in files:
        if not fn.endswith('.md'): continue
        path = os.path.join(root, fn)
        with open(path, 'r', encoding='utf-8') as fh:
            text = fh.read()
        for i,ch in enumerate(text):
            cp = ord(ch)
            # Basic emoji ranges (not exhaustive)
            if (0x1F300 <= cp <= 0x1F5FF) or (0x1F600 <= cp <= 0x1F64F) or (0x1F680 <= cp <= 0x1F6FF) or (0x2600 <= cp <= 0x26FF):
                emoji_found.append((path, i, ch))

if emoji_found:
    print('Found emoji characters in documentation:')
    for p,i,ch in emoji_found:
        print(f"{p}:{i}: {repr(ch)}")
    sys.exit(1)
else:
    print('No emoji characters found in docs (checked common emoji ranges).')
