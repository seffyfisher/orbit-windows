#!/usr/bin/env python3
"""Package a built executable and the corresponding reviewable source tree."""
import argparse, hashlib, json, pathlib, zipfile
from verify_pe import inspect

p=argparse.ArgumentParser()
p.add_argument('exe',type=pathlib.Path)
p.add_argument('output',type=pathlib.Path)
p.add_argument('--verification',type=pathlib.Path)
a=p.parse_args()
root=pathlib.Path(__file__).resolve().parents[1]
a.output.mkdir(parents=True,exist_ok=True)
report={'application':'Orbit','version':'0.1.0','binary':inspect(a.exe),
    'windows_runtime_acceptance':'pending — not run on Windows by the macOS builder',
    'verification':json.loads(a.verification.read_text()) if a.verification else {'status':'not supplied'}}
report_path=a.output/'Orbit-build-report.json'
report_path.write_text(json.dumps(report,indent=2)+'\n')
portable=a.output/'Orbit-Windows-x64-preview.zip'
with zipfile.ZipFile(portable,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    z.write(a.exe,'Orbit-Windows/Orbit.exe')
    for name in ['README.md','WINDOWS-QA.md','LICENSE','THIRD-PARTY-NOTICES.txt']:
        z.write(root/name,'Orbit-Windows/'+name)
    z.write(report_path,'Orbit-Windows/BUILD-REPORT.json')
    for name in ['Install.cmd','Check-Windows-Startup.cmd']:
        # CRLF is intentional for the files a Windows user runs by double-clicking.
        z.writestr('Orbit-Windows/'+name,(root/'packaging'/name).read_text().replace('\r\n','\n').replace('\n','\r\n'))
source=a.output/'Orbit-Source.zip'
with zipfile.ZipFile(source,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for path in sorted(root.rglob('*')):
        rel=path.relative_to(root)
        if not path.is_file() or any(part in {'target','dist','downloads','.git','__pycache__','.DS_Store'} or part.startswith('.env') for part in rel.parts):continue
        z.write(path,pathlib.Path('orbit-windows')/rel)
for path in [portable,source]:
    with zipfile.ZipFile(path) as z:assert z.testzip() is None
checksums=''.join(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n' for path in [portable,source,report_path])
(a.output/'Orbit-SHA256SUMS.txt').write_text(checksums)
print(json.dumps({'portable_zip':str(portable),'source_zip':str(source),'binary_bytes':report['binary']['bytes'],'zip_integrity':'passed'},indent=2))
