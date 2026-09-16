#!/usr/bin/env python3
"""Cross-platform detached UI startup, authenticated reuse and cleanup."""
import json,os,socket,subprocess,tempfile,time,urllib.error,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def main():
    with tempfile.TemporaryDirectory(prefix='sqlx-ui-lifecycle-') as directory:
        data=Path(directory);cli=ROOT/'target/debug'/('sqlx.exe' if os.name=='nt' else 'sqlx')
        env=os.environ.copy();env.update(SQLX_DATA_DIR=str(data),SQLX_WORKER_DIR=str(ROOT/'target/debug'))
        def call(*args):
            print("UI lifecycle: " + " ".join(args[:3]), flush=True)
            result=subprocess.run([str(cli),'--no-open',*args],env=env,capture_output=True,text=True,timeout=30)
            assert result.returncode==0,result.stdout+result.stderr
            return json.loads(result.stdout)['data']
        def stop():
            call('ui','stop')
            for _ in range(200):
                if not (data/'ui/state.json').exists():return
                time.sleep(.05)
            raise AssertionError('UI did not exit')
        try:
            call('ui','plugin','install','--path',str(ROOT/'ui/dist'))
            call('ui','plugin','use','default')
            first=call('ui')
            state=json.loads((data/'ui/state.json').read_text());assert first['url']==state['origin']+'/'
            assert call('ui','status')['pid']==state['pid']
            call('ui');assert json.loads((data/'ui/state.json').read_text())['instance']==state['instance']
            http=urllib.request.build_opener(urllib.request.ProxyHandler({}))
            try:http.open(state['origin']+'/api/home',timeout=10)
            except urllib.error.HTTPError as e:assert e.code==403
            else:raise AssertionError('unauthenticated access succeeded')
            stop();assert call('ui','status')['status']=='stopped'
            saved_port=json.loads((data/'ui/port.json').read_text())
            assert saved_port==int(state['origin'].rsplit(':',1)[1])
            # A port conflict must not silently move the user's saved address.
            with socket.socket() as occupied:
                occupied.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)
                occupied.bind(('127.0.0.1',saved_port));occupied.listen()
                failed=subprocess.run([str(cli),'--no-open','ui'],env=env,capture_output=True,text=True,timeout=30)
                assert failed.returncode!=0,failed.stdout
                assert json.loads((data/'ui/port.json').read_text())==saved_port
            assert call('ui')['url']==first['url']
            restarted=json.loads((data/'ui/state.json').read_text())
            assert restarted['instance']!=state['instance']
            assert restarted['origin']==state['origin']
            print('UI lifecycle: detached startup, authenticated reuse, denied anonymous API, stable restart URL and port-conflict handling passed')
        finally:stop()
if __name__=='__main__':main()
