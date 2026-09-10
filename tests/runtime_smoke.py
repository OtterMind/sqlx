#!/usr/bin/env python3
"""Exercise on-demand private JRE + JDBC installation from actual packaged assets."""
import functools,http.server,json,os,subprocess,tempfile,threading
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args):pass
def main():
    dist=ROOT/'dist';metadata=json.loads((dist/'metadata-shared.json').read_text())
    with tempfile.TemporaryDirectory(prefix='sqlx-runtime-') as temp:
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Quiet,directory=str(dist)))
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        try:
            base=f'http://127.0.0.1:{server.server_port}'
            for name,asset in metadata.items():
                if not name.startswith('java:'):asset['url']=base+'/'+asset['url'].rsplit('/',1)[1]
            (dist/'runtime-smoke-manifest.json').write_text(json.dumps(dict(schema_version=1,components=metadata)))
            cli=ROOT/'target/debug'/('sqlx.exe' if os.name=='nt' else 'sqlx')
            args=[str(cli),'--data-dir',str(Path(temp)/'data'),'--manifest',base+'/runtime-smoke-manifest.json']
            env=os.environ.copy();env.pop('SQLX_WORKER_DIR',None)
            config=dict(database_type='oracle',host='127.0.0.1',port=1,service='FREEPDB1',username='test',password='not_a_real_password',tls='disable',properties={'oracle.net.CONNECT_TIMEOUT':'2000'})
            add=subprocess.run(args+['datasource','add','--name','fixture','--connection-stdin'],input=json.dumps(config),text=True,capture_output=True,env=env,timeout=20)
            assert add.returncode==0,add.stdout
            result=subprocess.run(args+['datasource','test','--id','fixture'],text=True,capture_output=True,env=env,timeout=360)
            value=json.loads(result.stdout)
            assert not value['success'],value
            assert any(e['event']=='ready' for e in value.get('events',[])),value
            assert any(e['event']=='error' and e['code'].startswith('jdbc.') for e in value['events']),value
            print('runtime: downloaded and verified private JRE, JDBC runner and Oracle driver; JVM executed and returned expected connection refusal')
        finally:
            server.shutdown();server.server_close();thread.join();(dist/'runtime-smoke-manifest.json').unlink(missing_ok=True)
if __name__=='__main__':main()
