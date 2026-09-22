#!/usr/bin/env python3
"""Install the UI component through the real manifest/downloader, without local-worker overrides."""
import functools,hashlib,http.cookiejar,http.server,json,os,platform,subprocess,tempfile,threading,time,urllib.parse,urllib.request,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args):pass
def main():
    with tempfile.TemporaryDirectory(prefix='sqlx-ui-download-') as directory:
        root=Path(directory);web=root/'web';web.mkdir();data=root/'data';suffix='.exe' if os.name=='nt' else ''
        os_name='windows' if os.name=='nt' else 'macos' if platform.system()=='Darwin' else 'linux';arch='arm64' if platform.machine().lower() in ['arm64','aarch64'] else 'x64'
        archive=web/'ui.zip'
        with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED) as z:z.write(ROOT/'target/debug'/('sqlx-ui'+suffix),'sqlx-ui'+suffix)
        plugin_version=json.loads((ROOT/'ui/dist/ui-plugin.json').read_text())['version']
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Quiet,directory=str(web)));thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        origin=f'http://127.0.0.1:{server.server_port}'
        asset=dict(version='0.1.2',url=origin+'/ui.zip',sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),archive='zip',entrypoint='sqlx-ui'+suffix,cli_compat='>=0.1.1, <0.2.0',protocol_version=1)
        def publish(version):
            """Publish the default plugin at one version, the way a release does."""
            with zipfile.ZipFile(web/'plugin.zip','w',zipfile.ZIP_DEFLATED) as z:
                for file in (ROOT/'ui/dist').rglob('*'):
                    if not file.is_file():continue
                    body=file.read_bytes()
                    if file.name=='ui-plugin.json':body=json.dumps(dict(json.loads(body),version=version)).encode()
                    z.writestr(str(file.relative_to(ROOT/'ui/dist')),body)
            plugin=dict(asset,version=version,url=origin+'/plugin.zip',sha256=hashlib.sha256((web/'plugin.zip').read_bytes()).hexdigest(),entrypoint='ui-plugin.json')
            (web/'manifest.json').write_text(json.dumps(dict(schema_version=1,components={f'ui:{os_name}-{arch}':asset,'ui-default:any':plugin})))
            for cached in (data/'manifests').glob('*.json'):cached.unlink()  # A new release publishes a new manifest.
        publish(plugin_version)
        env=os.environ.copy();env.pop('SQLX_WORKER_DIR',None);env.update(SQLX_DATA_DIR=str(data),SQLX_MANIFEST=origin+'/manifest.json')
        # An upgrade must not reuse 0.1.0's indefinitely cached manifest, which has no UI.
        cache=data/'manifests';cache.mkdir(parents=True)
        legacy=cache/(hashlib.sha256(env['SQLX_MANIFEST'].encode()).hexdigest()+'.json')
        legacy.write_text(json.dumps(dict(schema_version=1,components={})))
        cli=ROOT/'target/debug'/('sqlx'+suffix)
        def call(*args):
            result=subprocess.run([str(cli),'--no-open',*args],env=env,capture_output=True,text=True,timeout=40);assert result.returncode==0,result.stdout+result.stderr;return json.loads(result.stdout)['data'],result.stderr
        def defaults():
            return [p for p in call('ui','plugin','list')[0]['plugins'] if p['manifest']['id']=='default']
        try:
            _,log=call('ui');assert 'Downloading ui' in log
            assert (data/'engines/ui'/f'{os_name}-{arch}'/'0.1.2'/('sqlx-ui'+suffix)).exists()
            _,log=call('ui');assert 'Downloading' not in log
            assert call('ui','status')[0]['status']=='running'
            plugins=call('ui','plugin','list')[0]['plugins'];assert len(plugins)==1 and plugins[0]['active']
            assert (data/'plugins/ui/default'/plugin_version/'index.html').exists()
            # The default interface belongs to the release: a newer release replaces it and keeps
            # the previous directory installed.
            bump=lambda step:plugin_version.rsplit('.',1)[0]+'.'+str(int(plugin_version.rsplit('.',1)[1])+step)
            publish(bump(1))
            _,log=call('ui');assert 'Downloading ' in log
            installed=sorted(p['manifest']['version'] for p in defaults())
            assert installed==sorted([plugin_version,bump(1)]),installed
            assert [p['manifest']['version'] for p in defaults() if p['active']]==[bump(1)]
            assert (data/'plugins/ui/default'/plugin_version/'index.html').exists()
            assert (data/'plugins/ui/default'/bump(1)/'index.html').exists()
            # The service that was already running serves the replaced interface.
            launch=call('ui')[0]['url'];page_url=urllib.parse.urlsplit(launch)
            page_origin=page_url.scheme+'://'+page_url.netloc
            client=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))
            with client.open(urllib.request.Request(page_origin+'/',headers={'Origin':page_origin,'X-SQLX-UI':'1'}),timeout=10) as response:page=response.read()
            assert f'/_ui/default/{bump(1)}/'.encode() in page,page[:300]
            _,log=call('ui');assert 'Downloading' not in log
            # A plugin the user installed keeps the version they selected.
            call('ui','plugin','install','--path',str(ROOT/'examples/terminal-ui/dist'))
            call('ui','plugin','use','terminal')
            publish(bump(2))
            call('ui')
            plugins=call('ui','plugin','list')[0]['plugins']
            assert [p['manifest']['id'] for p in plugins if p['active']]==['terminal'],plugins
            assert not (data/'plugins/ui/default'/bump(2)).exists()
            print('UI download: platform package verified, extracted, launched, and the release default interface replaced without worker overrides')
        finally:
            call('ui','stop')
            for _ in range(200):
                if not (data/'ui/state.json').exists():break
                time.sleep(.05)
            server.shutdown();server.server_close();thread.join()
if __name__=='__main__':main()
