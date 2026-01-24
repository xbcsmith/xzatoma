import os

for root, _, files in os.walk('docs'):
    for fn in files:
        if not fn.endswith('.md'): continue
        path = os.path.join(root, fn)
        with open(path, 'r', encoding='utf-8') as fh:
            text = fh.read()
        new_text = text.replace('_(coming soon)_', '')
        if new_text != text:
            with open(path, 'w', encoding='utf-8') as fh:
                fh.write(new_text)
            print('Updated:', path)
print('done')
