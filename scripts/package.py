#!/usr/bin/env python3
"""Create separately downloadable CLI/native-worker archives and manifest fragments."""
import argparse,hashlib,json,os,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def archive(output,files):
    with zipfile.ZipFile(output,'w',zipfile.ZIP_DEFLATED) as z:
        for source,name in files:z.write(source,name)
    return hashlib.sha256(output.read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser();p.add_argument('--platform',required=True);p.add_argument('--version',default='0.1.10');p.add_argument('--bin-dir',type=Path,default=ROOT/'target/release');p.add_argument('--output',type=Path,default=ROOT/'dist');a=p.parse_args()
    a.output.mkdir(parents=True,exist_ok=True);records={}
    suffix='.exe' if a.platform.startswith('windows-') else ''
    for kind,binary in [('cli','sqlx'),('mysql','sqlx-driver-mysql'),('postgres','sqlx-driver-postgres'),('ui','sqlx-ui')]:
        source=a.bin_dir/(binary+suffix)
        asset=a.output/f'{binary}-{a.platform}.zip'
        digest=archive(asset,[(source,binary+suffix),(ROOT/'LICENSE','LICENSE'),(ROOT/'NOTICE','NOTICE')])
        records[f'{kind}:{a.platform}']=dict(version=a.version,url=f'https://github.com/OtterMind/sqlx/releases/download/v{a.version}/{asset.name}',sha256=digest,archive='zip',entrypoint=binary+suffix,cli_compat='>=0.1.1, <0.2.0',protocol_version=1)
    (a.output/f'metadata-{a.platform}.json').write_text(json.dumps(records,indent=2)+'\n')
    print(f'Created {len(records)} archives for {a.platform}')
if __name__=='__main__':main()
