#!/usr/bin/env python3
import argparse,hashlib,importlib.util,json,sys
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent))
import package
def load_shared():
    # The packaging script is named package-shared.py, which is not importable by name.
    spec=importlib.util.spec_from_file_location('package_shared',Path(__file__).resolve().parent/'package-shared.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    return module
package_shared=load_shared()
PLATFORMS=['macos-arm64','macos-x64','windows-x64','linux-arm64','linux-x64']
DEFAULT_VERSION='0.1.17'
def required_components():
    """Exactly the components the packaging scripts emit: per-platform plus once-for-all."""
    per_platform=set(package.PLATFORM_KINDS)|set(package_shared.PLATFORM_KINDS)
    once=set(package_shared.SHARED_KINDS)|set(package_shared.VENDOR_KINDS)
    return {f'{kind}:{platform}' for kind in per_platform for platform in PLATFORMS}|{f'{kind}:any' for kind in once}
def main():
    p=argparse.ArgumentParser();p.add_argument('directory',type=Path);p.add_argument('--version',default=DEFAULT_VERSION);a=p.parse_args();components={}
    for path in sorted(a.directory.glob('metadata-*.json')):
        fragment=json.loads(path.read_text())
        if set(components)&set(fragment):raise ValueError('duplicate manifest components')
        components.update(fragment)
    required=required_components()
    if set(components)!=required:raise ValueError(f'incomplete platform manifest: {sorted(required-set(components))}')
    (a.directory/'manifest.json').write_text(json.dumps(dict(schema_version=1,components=components),indent=2)+'\n')
    (a.directory/'release-version.txt').write_text(a.version+'\n')
    checksums=[]
    for path in sorted(a.directory.iterdir()):
        if path.is_file() and not path.name.startswith('metadata-') and path.name!='SHA256SUMS':checksums.append(f'{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}')
    (a.directory/'SHA256SUMS').write_text('\n'.join(checksums)+'\n')
    print(f'Validated {len(components)} manifest entries across all five platforms')
if __name__=='__main__':main()
