import os

replacements = {
    '⚠️': 'WARNING:',
    '⚠': 'WARNING:',
    '❌': '',
    '✅': '',
    '🔴': '',
    '🟢': '',
    '🔮': '',
    '🚀': '',
    '🔵': '',
}

modified = []
for root, _, files in os.walk('docs'):
    for fn in files:
        if not fn.endswith('.md'):
            continue
        path = os.path.join(root, fn)
        with open(path, 'r', encoding='utf-8') as fh:
            text = fh.read()
        new_text = text
        for k, v in replacements.items():
            new_text = new_text.replace(k, v)
        # Normalize multiple spaces introduced by removals (simple heuristic)
        new_text = new_text.replace('  ', ' ')
        if new_text != text:
            with open(path, 'w', encoding='utf-8') as fh:
                fh.write(new_text)
            modified.append(path)

print('Modified files:')
for p in modified:
    print(' -', p)
print('Done')
