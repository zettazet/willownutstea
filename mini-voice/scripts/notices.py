"""Developer-only: collect license texts from the checksummed Cargo source cache.
Run after cargo build/fetch. No network calls or credential access.
"""
import json
import os
from pathlib import Path
import sys
import tomllib

root = Path(__file__).resolve().parents[1]
output = Path(sys.argv[1]) if len(sys.argv) > 1 else root / 'dist/THIRD-PARTY-NOTICES.txt'
cache = Path(os.environ.get('CARGO_HOME', str(Path.home() / '.cargo'))) / 'registry/src'
lock = tomllib.loads((root / 'Cargo.lock').read_text())
chunks = ['Third-party source license notices for goyvoice.\n'
          'This conservative inventory includes downloaded build/test dependencies.\n'
          'Platform system DLLs are supplied by Windows. SQLite is public domain.\n']
missing = []
for pkg in lock['package']:
    if not pkg.get('source', '').startswith('registry+'):
        continue
    paths = list(cache.glob(f"*/{pkg['name']}-{pkg['version']}"))
    if not paths:
        continue  # Other-platform packages are not necessarily downloaded/bundled.
    source = paths[0]
    manifest = tomllib.loads((source / 'Cargo.toml').read_text())['package']
    chunks.append(f"\n{'=' * 72}\n{pkg['name']} {pkg['version']}\nLicense: {manifest.get('license', 'see source')}\nSource: {manifest.get('repository', pkg['source'])}\n")
    texts = []
    for p in sorted(source.rglob('*')):
        name = p.name.lower()
        if p.is_file() and (name.startswith(('license', 'copying', 'copyright')) or name.endswith('.license')):
            if p.stat().st_size < 2_000_000:
                texts.append(f"\n--- {p.relative_to(source)} ---\n{p.read_text(errors='replace')}")
    if not texts:
        for p in sorted((root / 'licenses').glob(pkg['name'] + '-*.txt')):
            texts.append(f'\n--- vendored upstream notice: {p.name} ---\n{p.read_text()}')
    if not texts and manifest.get('license') != 'Unlicense':
        missing.append(pkg['name'])
    chunks.extend(texts)
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text('\n'.join(chunks), encoding='utf-8')
print(f'Wrote {output}; {output.stat().st_size} bytes')
if missing:
    print('License text review required for:', ', '.join(missing))
    sys.exit(1)
