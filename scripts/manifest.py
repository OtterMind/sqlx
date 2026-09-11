#!/usr/bin/env python3
import argparse,hashlib,json
from pathlib import Path
def main():
    p=argparse.ArgumentParser();p.add_argument('directory',type=Path);p.add_argument('--version',default='0.1.1');a=p.parse_args();components={}
    for path in sorted(a.directory.glob('metadata-*.json')):
        fragment=json.loads(path.read_text())
        if set(components)&set(fragment):raise ValueError('duplicate manifest components')
        components.update(fragment)
    platforms=['macos-arm64','macos-x64','windows-x64','linux-arm64','linux-x64']
    required={f'{kind}:{platform}' for kind in ['cli','mysql','postgres','ui','java'] for platform in platforms}|{f'{kind}:any' for kind in ['jdbc','oracle','sqlserver','skill','ui-default']}
    if set(components)!=required:raise ValueError(f'incomplete platform manifest: {sorted(required-set(components))}')
    (a.directory/'manifest.json').write_text(json.dumps(dict(schema_version=1,components=components),indent=2)+'\n')
    (a.directory/'release-version.txt').write_text(a.version+'\n')
    checksums=[]
    for path in sorted(a.directory.iterdir()):
        if path.is_file() and not path.name.startswith('metadata-') and path.name!='SHA256SUMS':checksums.append(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}')
    (a.directory/'SHA256SUMS').write_text('\n'.join(checksums)+'\n')
    print(f'Validated {len(components)} manifest entries across all five platforms')
if __name__=='__main__':main()
