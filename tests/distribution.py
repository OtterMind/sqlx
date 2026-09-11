#!/usr/bin/env python3
"""Exercise the real downloader and Skill installer against isolated local release assets."""
import argparse
import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import zipfile

ROOT=Path(__file__).resolve().parents[1]
class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args): pass

def exercise(cli):
    with tempfile.TemporaryDirectory(prefix='sqlx-distribution-') as tmp:
        root=Path(tmp);web=root/'web';web.mkdir();data=root/'data';target=root/'agent-skills/sqlx'
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(QuietHandler,directory=str(web)))
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        base=f'http://127.0.0.1:{server.server_port}'
        def package(version,text,entries=None):
            asset=web/f'skill-{version}.zip'
            with zipfile.ZipFile(asset,'w') as z:
                for name,body in (entries or {'SKILL.md':f'---\nname: sqlx\ndescription: Test database skill\n---\n{text}\n','references/mysql.md':'# MySQL\nSELECT 1\n'}).items(): z.writestr(name,body)
            return dict(version=version,url=f'{base}/{asset.name}',sha256=hashlib.sha256(asset.read_bytes()).hexdigest(),archive='zip',entrypoint='SKILL.md',cli_compat='>=0.1.1, <0.2.0',protocol_version=1)
        def manifest(asset,name='manifest.json'):
            (web/name).write_text(json.dumps(dict(schema_version=1,components={'skill:any':asset})))
        def call(*args,ok=True,manifest_name='manifest.json'):
            env=os.environ.copy();env.pop('SQLX_WORKER_DIR',None)
            result=subprocess.run([str(cli),'--data-dir',str(data),'--manifest',f'{base}/{manifest_name}',*args],env=env,text=True,capture_output=True,timeout=40)
            value=json.loads(result.stdout)
            assert (result.returncode==0)==ok,value
            return value
        try:
            asset=package('0.1.0','version one');manifest(asset)
            call('skill','install','--path',str(target))
            assert (target/'references/mysql.md').is_file()
            assert call('skill','status')['data']['installations'][0]['intact']
            manifest(package('0.1.1','version two'))
            call('skill','update');assert 'version two' in (target/'SKILL.md').read_text()
            (target/'SKILL.md').write_text('user-edited instructions')
            manifest(package('0.1.2','version three'))
            call('skill','update',ok=False);assert (target/'SKILL.md').read_text()=='user-edited instructions'
            assert not call('skill','status')['data']['installations'][0]['intact']
            bad=package('0.1.3','bad');bad['sha256']='0'*64;manifest(bad,'bad.json')
            call('skill','install','--path',str(root/'bad-target'),ok=False,manifest_name='bad.json')
            assert not (root/'bad-target').exists()
            traversal=package('0.1.4','bad',{'../escape':'no','SKILL.md':'no'});manifest(traversal,'traversal.json')
            call('skill','install','--path',str(root/'escape-target'),ok=False,manifest_name='traversal.json')
            assert not list(data.rglob('escape'))
            incompatible=package('0.1.5','future');incompatible['cli_compat']='>=99.0.0';manifest(incompatible,'future.json')
            call('skill','install','--path',str(root/'future'),ok=False,manifest_name='future.json')
            print('distribution: download, checksum, archive traversal rejection, compatibility, Skill install/update and preservation of local edits passed')
        finally:server.shutdown();server.server_close();thread.join()

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--cli',type=Path,default=ROOT/'target/debug'/('sqlx.exe' if os.name=='nt' else 'sqlx'));args=parser.parse_args();exercise(args.cli)
