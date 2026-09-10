#!/usr/bin/env python3
"""Local UI lifecycle tests against the dedicated PostgreSQL fixture, through public CLI/API contracts."""
import http.cookiejar,json,os,subprocess,tempfile,time,urllib.error,urllib.parse,urllib.request,uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]

def main():
    with tempfile.TemporaryDirectory(prefix='sqlx-ui-api-') as directory:
        data=Path(directory);cli=ROOT/'target/debug'/('sqlx.exe' if os.name=='nt' else 'sqlx')
        env=os.environ.copy();env.update(SQLX_DATA_DIR=str(data),SQLX_WORKER_DIR=str(ROOT/'target/debug'),SQLX_UI_TEST_USER='postgres')
        def command(*args,ok=True):
            result=subprocess.run([str(cli),'--no-open',*args],env=env,capture_output=True,text=True,timeout=40)
            value=json.loads(result.stdout);assert (result.returncode==0)==ok,value;return value.get('data',value)
        jar=http.cookiejar.CookieJar();client=urllib.request.build_opener(urllib.request.ProxyHandler({}),urllib.request.HTTPCookieProcessor(jar))
        origin=''
        def request(path,body=None,extra=None,expected=200):
            headers={'X-SQLX-UI':'1','Origin':origin};headers.update(extra or {})
            if body is not None:headers['Content-Type']='application/json'
            req=urllib.request.Request(origin+'/api'+path,headers=headers,data=None if body is None else json.dumps(body).encode())
            try:
                with client.open(req,timeout=30) as response:code=response.status;raw=response.read()
            except urllib.error.HTTPError as error:code=error.code;raw=error.read()
            assert code==expected,(code,raw[:500]);return json.loads(raw)
        def authorize(url):
            parsed=urllib.parse.urlsplit(url);token=urllib.parse.parse_qs(parsed.fragment)['token'][0]
            request('/session',{'token':token})
            request('/session',{'token':token},expected=403)
        def wait_setup(id,status):
            for _ in range(100):
                result=command('datasource','setup-status','--request-id',id)
                if result['status']==status:return result
                time.sleep(.1)
            raise AssertionError(result)
        def wait_result(id):
            for _ in range(200):
                result=request('/results/'+id)
                if result['status'] not in ['running','queued']:return result
                time.sleep(.1)
            raise AssertionError(result)
        draft_args=['datasource','add','--ui','--name','ui-fixture','--type','postgresql','--host','127.0.0.1','--port','25432','--database','sqlx_test','--tls','disable','--username-env','SQLX_UI_TEST_USER']
        connection=dict(database_type='postgresql',host='127.0.0.1',port=25432,database='sqlx_test',service='',username='postgres',password='sqlx_test_only_password',tls='disable',properties={})
        try:
            setup=command(*draft_args);parsed=urllib.parse.urlsplit(setup['url']);origin=parsed.scheme+'://'+parsed.netloc
            authorize(setup['url']);id=setup['request_id']
            assert command('datasource','list')['datasources']==[]
            form=request('/setups/'+id);assert form['connection']['host']=='127.0.0.1' and 'password' not in form['connection']
            request('/home',extra={'Origin':'https://unrelated.example'},expected=403)
            request('/home',extra={'Host':'attacker.example'},expected=403)
            request('/home',extra={'X-SQLX-UI':'wrong'},expected=403)
            request('/health',expected=403)
            bad=dict(connection,password='bad-test-password')
            request('/setups/'+id,dict(name='ui-fixture',connection=bad,password_action='replace'))
            failed=wait_setup(id,'waiting_for_user');assert failed['error'] and 'bad-test-password' not in json.dumps(failed)
            assert command('datasource','list')['datasources']==[]
            request('/setups/'+id,dict(name='ui-fixture',connection=connection,password_action='replace'))
            saved=wait_setup(id,'completed');source_id=saved['datasource_id'];assert source_id
            request('/setups/'+id,dict(name='ui-fixture',connection=connection,password_action='replace'),expected=409)
            public=command('datasource','show','--id',source_id);assert 'password' not in public['connection']
            for file in data.rglob('*'):
                if file.is_file():assert b'sqlx_test_only_password' not in file.read_bytes(),file.name
            cancelled=command(*[x if x!='ui-fixture' else 'cancelled-fixture' for x in draft_args])
            request('/setups/'+cancelled['request_id']+'/cancel',{})
            assert wait_setup(cancelled['request_id'],'cancelled')['datasource_id'] is None
            assert len(command('datasource','list')['datasources'])==1
            # Editing with an empty password field preserves the existing secret.
            edit=command('datasource','update','--id',source_id,'--ui')
            request('/setups/'+edit['request_id'],dict(name='ui-renamed',connection=dict(connection,password=''),password_action='keep'))
            assert wait_setup(edit['request_id'],'completed')['datasource_id']==source_id
            command('datasource','test','--id',source_id)
            # A stale form cannot overwrite a concurrent CLI edit.
            stale=command('datasource','update','--id',source_id,'--ui')
            command('datasource','update','--id',source_id,'--name','concurrent-name')
            request('/setups/'+stale['request_id'],dict(name='stale-name',connection=dict(connection,password=''),password_action='keep'))
            assert 'changed' in wait_setup(stale['request_id'],'waiting_for_user')['error']
            assert command('datasource','show','--id',source_id)['name']=='concurrent-name'
            # A sequence proves that page reads and reloads do not execute SQL again.
            sequence='ui_once_'+uuid.uuid4().hex[:10]
            command('sql','execute','--datasource',source_id,'--sql',f'CREATE SEQUENCE {sequence}')
            try:
                view=command('sql','execute','--datasource',source_id,'--sql',f"SELECT nextval('{sequence}') AS execution, n AS id, 9007199254740993::bigint AS exact, '<script>alert(1)</script>' AS content FROM generate_series(1,251) n ORDER BY n",'--view')
                metadata=wait_result(view['result_id']);assert metadata['status']=='completed';assert metadata['tables'][0]['rows']==251
                for _ in range(3):
                    page=request('/results/'+view['result_id']+'/rows?statement=0&result=0&offset=200&limit=100');assert len(page['rows'])==51 and page['rows'][0][2]=='9007199254740993'
                check=subprocess.run([str(cli),'sql','execute','--datasource',source_id,'--sql',f'SELECT last_value FROM {sequence}'],env=env,capture_output=True,text=True,check=True)
                rows=[e['values'] for e in json.loads(check.stdout)['events'] if e['event']=='row'];assert rows==[['251']]
                # Internal control retries with the same request ID return existing results.
                state=json.loads((data/'ui/state.json').read_text());admin={'Authorization':'Bearer '+state['token']}
                request('/results',dict(request_id=view['result_id'],datasource=source_id,statements=metadata['statements']),extra=admin)
                old_id=view['result_id'];command('ui','stop')
                for _ in range(100):
                    if not (data/'ui/state.json').exists():break
                    time.sleep(.05)
                home=command('ui');parsed=urllib.parse.urlsplit(home['url']);origin=parsed.scheme+'://'+parsed.netloc;authorize(home['url'])
                assert request('/results/'+old_id)['status']=='completed'
                assert len(request('/results/'+old_id+'/rows?statement=0&result=0&offset=200&limit=100')['rows'])==51
                error=command('sql','execute','--datasource',source_id,'--sql','SELECT 1','--sql','SELECT * FROM ui_missing_table','--sql','SELECT 2','--view')
                failed=wait_result(error['result_id']);assert failed['status']=='failed';assert any(e['event']=='skipped' for e in failed['events'])
                slow=command('sql','execute','--datasource',source_id,'--sql','SELECT pg_sleep(30)','--view')
                request('/results/'+slow['result_id']+'/cancel',{});assert wait_result(slow['result_id'])['status']=='cancelled'
            finally:command('sql','execute','--datasource',source_id,'--sql',f'DROP SEQUENCE {sequence}')
            command('datasource','remove','--id',source_id)
            print('UI API: authentication, form-only credentials, retry/cancel/edit/CAS, complete indexed pages, no re-execution, restart recovery and cancellation passed')
        finally:
            command('ui','stop')
            for _ in range(200):
                if not (data/'ui/state.json').exists():break
                time.sleep(.05)
if __name__=='__main__':main()
