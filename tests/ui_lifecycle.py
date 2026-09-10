#!/usr/bin/env python3
"""Cross-platform detached UI startup, authenticated reuse and cleanup."""
import json,os,subprocess,tempfile,time,urllib.error,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def main():
    with tempfile.TemporaryDirectory(prefix='sqlx-ui-lifecycle-') as directory:
        data=Path(directory);cli=ROOT/'target/debug'/('sqlx.exe' if os.name=='nt' else 'sqlx')
        env=os.environ.copy();env.update(SQLX_DATA_DIR=str(data),SQLX_WORKER_DIR=str(ROOT/'target/debug'))
        def call(*args):
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
            first=call('ui');assert '#token=' in first['url']
            state=json.loads((data/'ui/state.json').read_text())
            assert call('ui','status')['pid']==state['pid']
            call('ui');assert json.loads((data/'ui/state.json').read_text())['instance']==state['instance']
            http=urllib.request.build_opener(urllib.request.ProxyHandler({}))
            try:http.open(state['origin']+'/api/home')
            except urllib.error.HTTPError as e:assert e.code==403
            else:raise AssertionError('unauthenticated access succeeded')
            stop();assert call('ui','status')['status']=='stopped'
            call('ui');assert json.loads((data/'ui/state.json').read_text())['instance']!=state['instance']
            print('UI lifecycle: detached startup, authenticated reuse, denied anonymous API and clean stop/restart passed')
        finally:stop()
if __name__=='__main__':main()
