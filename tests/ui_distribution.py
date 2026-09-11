#!/usr/bin/env python3
"""Install the UI component through the real manifest/downloader, without local-worker overrides."""
import functools,hashlib,http.server,json,os,platform,subprocess,tempfile,threading,time,zipfile
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
        plugin_archive=web/'plugin.zip'
        with zipfile.ZipFile(plugin_archive,'w',zipfile.ZIP_DEFLATED) as z:
            for file in (ROOT/'ui/dist').rglob('*'):
                if file.is_file():z.write(file,str(file.relative_to(ROOT/'ui/dist')))
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Quiet,directory=str(web)));thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        origin=f'http://127.0.0.1:{server.server_port}'
        asset=dict(version='0.1.1',url=origin+'/ui.zip',sha256=hashlib.sha256(archive.read_bytes()).hexdigest(),archive='zip',entrypoint='sqlx-ui'+suffix,cli_compat='>=0.1.1, <0.2.0',protocol_version=1)
        plugin_asset=dict(asset,url=origin+'/plugin.zip',sha256=hashlib.sha256(plugin_archive.read_bytes()).hexdigest(),entrypoint='ui-plugin.json')
        (web/'manifest.json').write_text(json.dumps(dict(schema_version=1,components={f'ui:{os_name}-{arch}':asset,'ui-default:any':plugin_asset})))
        env=os.environ.copy();env.pop('SQLX_WORKER_DIR',None);env.update(SQLX_DATA_DIR=str(data),SQLX_MANIFEST=origin+'/manifest.json')
        # An upgrade must not reuse 0.1.0's indefinitely cached manifest, which has no UI.
        cache=data/'manifests';cache.mkdir(parents=True)
        legacy=cache/(hashlib.sha256(env['SQLX_MANIFEST'].encode()).hexdigest()+'.json')
        legacy.write_text(json.dumps(dict(schema_version=1,components={})))
        cli=ROOT/'target/debug'/('sqlx'+suffix)
        def call(*args):
            result=subprocess.run([str(cli),'--no-open',*args],env=env,capture_output=True,text=True,timeout=40);assert result.returncode==0,result.stdout+result.stderr;return json.loads(result.stdout)['data'],result.stderr
        try:
            _,log=call('ui');assert 'Downloading ui' in log
            assert (data/'engines/ui'/f'{os_name}-{arch}'/'0.1.1'/('sqlx-ui'+suffix)).exists()
            _,log=call('ui');assert 'Downloading' not in log
            assert call('ui','status')[0]['status']=='running'
            plugins=call('ui','plugin','list')[0]['plugins'];assert len(plugins)==1 and plugins[0]['active']
            assert (data/'plugins/ui/default/0.1.1/index.html').exists()
            print('UI download: platform package verified, extracted, launched and reused without worker overrides')
        finally:
            call('ui','stop')
            for _ in range(200):
                if not (data/'ui/state.json').exists():break
                time.sleep(.05)
            server.shutdown();server.server_close();thread.join()
if __name__=='__main__':main()
