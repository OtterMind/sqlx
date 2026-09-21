#!/usr/bin/env python3
"""Exercise the real downloader and Skill installer against isolated local release assets, and verify the shipped Skill source contract."""
import argparse
import functools
import hashlib
import http.server
import json
import os
import platform
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import zipfile

ROOT=Path(__file__).resolve().parents[1]
class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args): pass
class FlakyHandler(QuietHandler):
    """Cut the first response for a path short so the downloader must retry.

    Dropping the connection instead would let the HTTP client replay the request
    internally, which hides the retry this test is meant to observe.
    """
    truncate={}
    def do_GET(self):
        remaining=FlakyHandler.truncate.get(self.path,0)
        target=Path(self.translate_path(self.path))
        if remaining>0 and target.is_file():
            FlakyHandler.truncate[self.path]=remaining-1
            body=target.read_bytes()
            self.send_response(200)
            self.send_header('Content-Type','application/octet-stream')
            self.send_header('Content-Length',str(len(body)))
            self.end_headers()
            self.wfile.write(body[:max(1,len(body)//2)])
            self.wfile.flush()
            self.close_connection=True
            return
        return super().do_GET()
def host_platform():
    os_name={'darwin':'macos','linux':'linux','win32':'windows'}[sys.platform]
    machine=platform.machine().lower()
    return f"{os_name}-{'arm64' if machine in ('arm64','aarch64') else 'x64'}"

def exercise(cli):
    with tempfile.TemporaryDirectory(prefix='sqlx-distribution-') as tmp:
        root=Path(tmp);web=root/'web';web.mkdir();data=root/'data';target=root/'agent-skills/sqlx'
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(FlakyHandler,directory=str(web)))
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        base=f'http://127.0.0.1:{server.server_port}'
        def package(version,text,entries=None):
            asset=web/f'skill-{version}.zip'
            with zipfile.ZipFile(asset,'w') as z:
                for name,body in (entries or {'SKILL.md':f'---\nname: sqlx\ndescription: Test database skill\n---\n{text}\n','references/mysql.md':'# MySQL\nSELECT 1\n'}).items(): z.writestr(name,body)
            return dict(version=version,url=f'{base}/{asset.name}',sha256=hashlib.sha256(asset.read_bytes()).hexdigest(),archive='zip',entrypoint='SKILL.md',cli_compat='>=0.1.1, <0.2.0',protocol_version=1)
        def manifest(asset,name='manifest.json'):
            (web/name).write_text(json.dumps(dict(schema_version=1,components={'skill:any':asset})))
        def call(*args,ok=True,manifest_name='manifest.json',with_log=False):
            env=os.environ.copy();env.pop('SQLX_WORKER_DIR',None)
            result=subprocess.run([str(cli),'--data-dir',str(data),'--manifest',f'{base}/{manifest_name}',*args],env=env,text=True,capture_output=True,timeout=60)
            value=json.loads(result.stdout)
            assert (result.returncode==0)==ok,value
            return (value,result.stderr) if with_log else value
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
            suffix='.exe' if os.name=='nt' else ''
            plat=host_platform()
            def worker(name):
                entry=name+suffix;asset=web/f'{entry}-{plat}.zip'
                with zipfile.ZipFile(asset,'w') as z: z.writestr(entry,'fake worker\n')
                return dict(version='0.1.0',url=f'{base}/{asset.name}',sha256=hashlib.sha256(asset.read_bytes()).hexdigest(),archive='zip',entrypoint=entry,cli_compat='>=0.1.1, <0.2.0',protocol_version=1)
            mysql=worker('sqlx-driver-mysql')
            (web/'prefetch.json').write_text(json.dumps(dict(schema_version=1,components={f'mysql:{plat}':mysql})))
            value,log=call('prefetch','mysql',manifest_name='prefetch.json',with_log=True)
            assert value['data']['downloaded']==1 and value['data']['failed']==[],value
            assert value['data']['components'][0]['status']=='downloaded',value
            assert 'Downloading mysql 0.1.0' in log,log
            assert 'Downloaded mysql 0.1.0' in log and '/s' in log,log
            assert (data/'drivers/mysql'/plat/'0.1.0'/('sqlx-driver-mysql'+suffix)).is_file()
            value,log=call('prefetch','mysql',manifest_name='prefetch.json',with_log=True)
            assert value['data']['already_installed']==1 and value['data']['downloaded']==0,value
            assert 'Downloading' not in log,log
            postgres=worker('sqlx-driver-postgres')
            (web/'retry.json').write_text(json.dumps(dict(schema_version=1,components={f'mysql:{plat}':mysql,f'postgres:{plat}':postgres})))
            key='/'+postgres['url'].rsplit('/',1)[-1]
            FlakyHandler.truncate[key]=1
            value,log=call('prefetch','postgres',manifest_name='retry.json',with_log=True)
            assert FlakyHandler.truncate[key]==0,f'the test server never truncated a response for {key}'
            assert value['data']['downloaded']==1,value
            assert 'retrying in 2s (1/3)' in log,log
            assert 'Downloaded postgres 0.1.0' in log,log
            for args,fragment in ((['prefetch','sqlite'],'invalid value'),(['prefetch'],'required')):
                result=subprocess.run([str(cli),*args],env=os.environ.copy(),text=True,capture_output=True,timeout=20)
                assert result.returncode!=0 and fragment in result.stderr,result.stderr
            print('distribution: download, checksum, archive traversal rejection, compatibility, Skill install/update, local edits, prefetch reuse, retry and progress output passed')
        finally:server.shutdown();server.server_close();thread.join()

def check_skill_source():
    """Verify the Skill shipped by releases still carries the Agent approval contract."""
    skill=ROOT/'skills/sqlx'
    def read(name):return (skill/name).read_text(encoding='utf-8').replace('\r\n','\n')
    text=read('SKILL.md');reference=read('references/local-ui.md')
    assert text.startswith('---\nname: sqlx\n'),'SKILL.md frontmatter name changed'
    clauses=[
        '### Approval before state-changing SQL',
        'classify the whole batch as read-only, state-changing, or unknown',
        'If the user has not explicitly authorized that operation and scope, pause and ask for confirmation',
        'Establish the blast radius with read-only SQL before asking',
        'The same approval gate applies to `--view`',
        'A one-time approval does not authorize future reruns',
        'This is an Agent workflow rule, not a database permission mechanism',
        '## Downloads on first use',
        'sqlx prefetch <component>',
        'retried up to three times',
        'components that are already installed are reused',
    ]
    missing=[clause for clause in clauses if clause not in text]
    assert not missing,f'SKILL.md no longer states the approval contract: {missing}'
    assert "The Skill's approval gate therefore applies on the agent side" in reference,'references/local-ui.md no longer applies the approval gate to refresh'
    databases=('mysql','mariadb','tidb','postgresql','cockroachdb','yugabytedb','oracle','sqlserver','clickhouse','trino','starrocks','doris')
    for database in databases:
        assert (skill/'references'/f'{database}.md').is_file(),f'missing database reference references/{database}.md'
        assert f'(references/{database}.md)' in text,f'SKILL.md does not link references/{database}.md'
    assert not (skill/'references'/'additional-databases.md').exists(),'databases must be documented in one reference file each'
    documents=[('SKILL.md',text)]+[
        (f'references/{path.name}',read(f'references/{path.name}'))
        for path in sorted((skill/'references').glob('*.md'))
    ]
    for name,body in documents:
        assert body.count('```')%2==0,f'unbalanced code fences in {name}'
        trailing=[number for number,line in enumerate(body.splitlines(),1) if line.rstrip()!=line]
        assert not trailing,f'trailing whitespace in {name} at {trailing}'
    print(f'skill source: approval contract clauses, {len(databases)} per-database references and formatting passed')

def check_release_contract():
    """The release manifest gate must accept exactly the components the packagers emit.

    Both packagers keep their component names in module constants that `scripts/manifest.py`
    imports, so a new component cannot reach a release without the gate knowing about it.
    """
    sys.path.insert(0,str(ROOT/'scripts'))
    import manifest,package
    package_shared=manifest.package_shared
    per_platform=set(package.PLATFORM_KINDS)|set(package_shared.PLATFORM_KINDS)
    once=set(package_shared.SHARED_KINDS)|set(package_shared.VENDOR_KINDS)
    assert {'clickhouse','trino'}<=once,'the JDBC worker engines must ship a vendor component'
    with tempfile.TemporaryDirectory(prefix='sqlx-manifest-') as tmp:
        directory=Path(tmp)
        shared={f'{kind}:any':{'version':'0.1.10'} for kind in once}
        (directory/'metadata-shared.json').write_text(json.dumps(shared))
        for platform in manifest.PLATFORMS:
            (directory/f'metadata-{platform}.json').write_text(
                json.dumps({f'{kind}:{platform}':{'version':'0.1.10'} for kind in per_platform}))
        def run(*extra):
            sys.argv=['manifest.py',str(directory),*extra]
            manifest.main()
        run()
        produced=json.loads((directory/'manifest.json').read_text())['components']
        assert set(produced)==manifest.required_components(),'the manifest must carry every required component'
        assert (directory/'SHA256SUMS').is_file() and (directory/'release-version.txt').read_text()=='0.1.10\n'
        (directory/'metadata-extra.json').write_text(json.dumps({'duckdb:any':{'version':'0.1.10'}}))
        try:
            run()
        except ValueError as error:
            assert 'incomplete platform manifest' in str(error),error
        else:
            raise AssertionError('an undeclared component must fail the manifest gate')
    print(f'release contract: {len(per_platform)} per-platform and {len(once)} shared components stay in sync with the packagers')


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--cli',type=Path,default=ROOT/'target/debug'/('sqlx.exe' if os.name=='nt' else 'sqlx'));args=parser.parse_args();check_skill_source();check_release_contract();exercise(args.cli)
