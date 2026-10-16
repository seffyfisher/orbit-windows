#!/usr/bin/env python3
"""Package the actual native installer, its embedded app and matching sources."""
import argparse
import base64
import hashlib
import json
import pathlib
import shutil
import zipfile
from verify_pe import inspect

p = argparse.ArgumentParser()
p.add_argument('app', type=pathlib.Path)
p.add_argument('gift', type=pathlib.Path)
p.add_argument('output', type=pathlib.Path)
p.add_argument('--verification', type=pathlib.Path, required=True)
a = p.parse_args()
root = pathlib.Path(__file__).resolve().parents[1]
a.output.mkdir(parents=True, exist_ok=True)
app_bytes = a.app.read_bytes()
assert a.gift.read_bytes().count(app_bytes) == 1, 'Gift must embed the exact verified app'

installer = a.output / 'Orbit-Gift-0.1.0-x64.exe'
shutil.copy2(a.gift, installer)
report = {
    'application': 'Orbit', 'version': '0.1.0',
    'gift_installer': inspect(installer), 'embedded_application': inspect(a.app),
    'exact_payload_embedding': 'passed: application occurs once byte-for-byte',
    'windows_runtime_acceptance': 'pending: installer and app were not run on Windows',
    'verification': json.loads(a.verification.read_text()),
}
preview = (root / 'gift-installer/preview/index.html').read_text()
# Keep the delivered preview completely offline and usable by double-click on macOS.
data_urls = {}
for name in ['wrapped-parcel.png', 'product-box.png', 'paper-flap.png', 'dedication.png']:
    data_urls[name] = 'data:image/png;base64,' + base64.b64encode(
        (root / 'gift-installer/assets' / name).read_bytes()).decode('ascii')
preview = preview.replace("load('../assets/'+name)", 'load(' + json.dumps(data_urls) + '[name])')
preview = preview.replace('../../../Orbit-Gift-0.1.0-x64.exe', 'Orbit-Gift-0.1.0-x64.exe')
preview_path = a.output / 'Orbit-Gift-Preview.html'
preview_path.write_text(preview)

source_path = a.output / 'Orbit-Source.zip'
with zipfile.ZipFile(source_path, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for path in sorted(root.rglob('*')):
        rel = path.relative_to(root)
        if not path.is_file() or any(part in {'target', 'dist', 'downloads', '.git', '__pycache__', '.DS_Store'}
                                    or part.startswith('.env') for part in rel.parts):
            continue
        z.write(path, pathlib.Path('orbit-windows') / rel)

report_path = a.output / 'Orbit-Gift-build-report.json'
report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + '\n')
archive = a.output / 'Orbit-Gift-Windows-0.1.0-x64.zip'
with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    folder = 'Orbit-Gift/'
    z.write(installer, folder + installer.name)
    z.write(preview_path, folder + preview_path.name)
    z.write(report_path, folder + 'BUILD-REPORT.json')
    z.write(source_path, folder + source_path.name)
    media = a.output / 'Orbit-Gift-Media'
    if media.is_dir():
        for path in sorted(media.iterdir()):
            if path.is_file() and path.suffix.lower() in {'.jpg', '.mp4', '.txt', '.json'}:
                z.write(path, folder + 'Media/' + path.name)
    for source, dest in [
        ('packaging/gift-readme-he.txt', 'Readme-Hebrew.txt'),
        ('LICENSE', 'LICENSE.txt'),
        ('THIRD-PARTY-NOTICES.txt', 'THIRD-PARTY-NOTICES.txt'),
        ('WINDOWS-QA.md', 'WINDOWS-QA.md'),
    ]:
        z.writestr(folder + dest, (root / source).read_text().replace('\r\n', '\n').replace('\n', '\r\n'))
    z.writestr(folder + 'Check-Windows-Startup.cmd', (
        '@echo off\n'
        'if exist "%LOCALAPPDATA%\\Programs\\Orbit\\Orbit.exe" (\n'
        '  "%LOCALAPPDATA%\\Programs\\Orbit\\Orbit.exe" --diagnose\n'
        '  exit /b\n'
        ')\n'
        'echo Install Orbit with the gift installer first.\n'
        'pause\nexit /b 1\n').replace('\n', '\r\n'))

for path in [source_path, archive]:
    with zipfile.ZipFile(path) as z:
        assert z.testzip() is None
checksums = ''.join(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n'
                    for path in [installer, archive, preview_path, source_path, report_path])
(a.output / 'Orbit-Gift-SHA256SUMS.txt').write_text(checksums)
print(json.dumps({'installer': str(installer), 'bytes': installer.stat().st_size,
                  'gift_zip': str(archive), 'archive_integrity': 'passed',
                  'exact_payload': 'passed', 'browser_preview': str(preview_path)}, indent=2))
