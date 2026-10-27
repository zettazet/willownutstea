"""Package already-built Linux and GNU Windows artifacts on a Linux build host.
Developer tool; Python 3.11+. Never substitutes stub binaries for missing builds.
"""
from pathlib import Path
import argparse
import datetime
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tarfile
import zipfile

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--objdump', required=True)
parser.add_argument('--mingw-docs', required=True, type=Path)
args = parser.parse_args()
dist = root / 'dist'
source_sums = []
for p in sorted(root.rglob('*')):
    if p.is_file() and not any(x in ('target', 'dist', 'releases', '__pycache__') for x in p.relative_to(root).parts):
        source_sums.append(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.relative_to(root).as_posix()}')
allowed = {'kernel32.dll', 'ole32.dll', 'oleaut32.dll', 'msvcrt.dll',
           'bcryptprimitives.dll', 'advapi32.dll', 'api-ms-win-core-synch-l1-2-0.dll',
           'shell32.dll', 'comctl32.dll', 'comdlg32.dll', 'imm32.dll',
           'bcrypt.dll', 'crypt32.dll', 'gdi32.dll', 'ntdll.dll', 'user32.dll', 'ws2_32.dll'}
for platform in ('windows-x64', 'linux-x64'):
    stage = dist / platform
    stage.mkdir(parents=True, exist_ok=True)
    binaries = ['mini-client.exe', 'mini-server.exe'] if platform.startswith('windows') else ['mini-server']
    for name in binaries:
        source = root / 'target' / ('x86_64-pc-windows-gnu/release' if name.endswith('.exe') else 'release') / name
        if not source.is_file():
            sys.exit(f'Missing real release binary: {source}')
        if name.endswith('.exe'):
            imports = re.findall(r'DLL Name:\s*(\S+)', subprocess.check_output([args.objdump, '-p', str(source)], text=True))
            unexpected = set(x.lower() for x in imports) - allowed
            if unexpected:
                sys.exit(f'Package required runtime DLLs before distribution: {unexpected}')
            print(f'{name}: only Windows system DLL imports verified')
        shutil.copy2(source, stage / name)
        (stage / name).chmod(0o755)
    for name in ('README.md', 'LICENSE', 'server.example.json'):
        shutil.copy2(root / name, stage / name)
    shutil.copytree(root / 'docs', stage / 'docs', dirs_exist_ok=True)
    if platform.startswith('windows'):
        shutil.copy2(root / 'docs/PORTABLE-WINDOWS.md', stage / 'README.md')
    if platform.startswith('windows'):
        shutil.copy2(root / 'scripts/benchmark.ps1', stage / 'benchmark.ps1')
        license_dir = stage / 'compiler-notices'
        license_dir.mkdir(exist_ok=True)
        for name in ('gcc-mingw-w64-x86-64-posix-runtime', 'mingw-w64-common'):
            shutil.copy2(args.mingw_docs / name / 'copyright', license_dir / (name + '.txt'))
        shutil.copy2('/usr/share/common-licenses/GPL-3', license_dir / 'GPL-3.txt')
    subprocess.run([sys.executable, str(root / 'scripts/notices.py'), str(stage / 'THIRD-PARTY-NOTICES.txt')], check=True)
    info = (f'Built {datetime.datetime.now(datetime.timezone.utc).isoformat()}\n'
            f'Platform: {platform}; Rust 1.99.1; optimized release, thin LTO.\n'
            'Windows builds: GNU cross-compiled, only system DLL imports; not executed on Windows.\n'
            'Linux binary requires glibc >= 2.38. No public server is configured.\n'
            'See docs/VALIDATION.md for executed checks and remaining acceptance tests.\n')
    (stage / 'BUILD-INFO.txt').write_text(info)
    (stage / 'SOURCE-SHA256SUMS.txt').write_text('\n'.join(source_sums) + '\n')
    sums = [f'{hashlib.sha256((stage / n).read_bytes()).hexdigest()}  {n}' for n in binaries]
    (stage / 'BINARY-SHA256SUMS.txt').write_text('\n'.join(sums) + '\n')
    if platform.startswith('windows'):
        archive = dist / 'mini-voice-windows-x64.zip'
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
            for p in sorted(stage.rglob('*')):
                if p.is_file():
                    z.write(p, p.relative_to(stage))
        with zipfile.ZipFile(archive) as z:
            assert z.testzip() is None
    else:
        archive = dist / 'mini-server-linux-x64.tar.gz'
        with tarfile.open(archive, 'w:gz') as t:
            t.add(stage, arcname='mini-server-linux-x64')
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(archive.name + '.sha256').write_text(f'{digest}  {archive.name}\n')
    print(f'{archive}: {archive.stat().st_size} bytes; SHA256 {digest}')
