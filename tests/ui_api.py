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
        def open_page(url):
            parsed=urllib.parse.urlsplit(url);assert not parsed.fragment,url
            with client.open(url,timeout=30) as response:
                assert response.status==200
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
        def wait_refresh(id):
            for _ in range(200):
                result=request('/results/'+id)
                if result['refresh']['status']!='running':return result
                time.sleep(.05)
            raise AssertionError(result)
        draft_args=['datasource','add','--ui','--name','ui-fixture','--type','postgresql','--host','127.0.0.1','--port','25432','--database','sqlx_test','--tls','disable','--username-env','SQLX_UI_TEST_USER']
        connection=dict(database_type='postgresql',host='127.0.0.1',port=25432,database='sqlx_test',service='',username='postgres',password='sqlx_test_only_password',tls='disable',properties={})
        try:
            command('ui','plugin','install','--path',str(ROOT/'ui/dist'))
            command('ui','plugin','use','default')
            command('ui','plugin','install','--path',str(ROOT/'examples/terminal-ui/dist'))
            setup=command(*draft_args);parsed=urllib.parse.urlsplit(setup['url']);origin=parsed.scheme+'://'+parsed.netloc
            # CLI page URLs are ordinary local paths. Opening them establishes
            # the browser session without a launch credential exchange.
            open_page(setup['url'])
            open_page(command('ui')['url'])
            assert any(cookie.name.startswith('sqlx_ui_') for cookie in jar)
            assert request('/home')['datasources']==[]
            foreign=urllib.request.Request(origin+'/',headers={'Host':'attacker.example'+':'+str(parsed.port)})
            try:
                client.open(foreign,timeout=30)
            except urllib.error.HTTPError as error:
                assert error.code==403
            else:
                raise AssertionError('foreign local page host was accepted')
            id=setup['request_id']
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
            # Saved connections remain visible after the transient setup finishes.
            listed=request('/home')['datasources'];assert len(listed)==1 and listed[0]['id']==source_id
            details=request('/datasources/'+source_id);assert details==listed[0]
            assert not {'username','password','properties'} & details['connection'].keys()
            before=(data/'datasources.enc').read_bytes()
            assert request('/datasources/'+source_id+'/test',{})['connected']
            assert request('/home')['results']==[], 'connection testing executed SQL for a result page'
            draft=request('/datasources/'+source_id+'/edit',{})
            assert request('/setups/'+draft['request_id'])['editing']
            request('/setups/'+draft['request_id']+'/cancel',{})
            assert (data/'datasources.enc').read_bytes()==before, 'read/test/cancel rewrote stored credentials'
            request('/datasources/'+source_id+'/edit',{},extra={'Origin':'https://unrelated.example'},expected=403)
            request('/datasources/'+str(uuid.uuid4()),expected=404)
            request('/setups/'+id,dict(name='ui-fixture',connection=connection,password_action='replace'),expected=409)
            public=command('datasource','show','--id',source_id);assert 'password' not in public['connection']
            for file in data.rglob('*'):
                if file.is_file():assert b'sqlx_test_only_password' not in file.read_bytes(),file.name
            cancelled=command(*[x if x!='ui-fixture' else 'cancelled-fixture' for x in draft_args])
            request('/setups/'+cancelled['request_id']+'/cancel',{})
            assert wait_setup(cancelled['request_id'],'cancelled')['datasource_id'] is None
            assert len(command('datasource','list')['datasources'])==1
            # Editing with an empty password field preserves the existing secret.
            edit=request('/datasources/'+source_id+'/edit',{})
            request('/setups/'+edit['request_id'],dict(name='ui-renamed',connection=dict(connection,password=''),password_action='keep'))
            assert wait_setup(edit['request_id'],'completed')['datasource_id']==source_id
            command('datasource','test','--id',source_id)
            assert request('/home')['datasources'][0]['name']=='ui-renamed'
            # A stale form cannot overwrite a concurrent CLI edit.
            stale=request('/datasources/'+source_id+'/edit',{})
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
                command('ui','plugin','use','terminal')
                assert request('/plugin')['id']=='terminal'
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
                home=command('ui');parsed=urllib.parse.urlsplit(home['url']);origin=parsed.scheme+'://'+parsed.netloc;open_page(home['url'])
                assert request('/results/'+old_id)['status']=='completed'
                assert len(request('/results/'+old_id+'/rows?statement=0&result=0&offset=200&limit=100')['rows'])==51
                error=command('sql','execute','--datasource',source_id,'--sql','SELECT 1','--sql','SELECT * FROM ui_missing_table','--sql','SELECT 2','--view')
                failed=wait_result(error['result_id']);assert failed['status']=='failed';assert any(e['event']=='skipped' for e in failed['events'])
                slow=command('sql','execute','--datasource',source_id,'--sql','SELECT pg_sleep(30)','--view')
                request('/results/'+slow['result_id']+'/cancel',{});assert wait_result(slow['result_id'])['status']=='cancelled'
            finally:command('sql','execute','--datasource',source_id,'--sql',f'DROP SEQUENCE {sequence}')
            # Refresh reexecutes the exact SQL batch, including an explicit write.
            # Repeated request IDs cannot execute that write a second time.
            counter='ui_refresh_'+uuid.uuid4().hex[:10]
            command('sql','execute','--datasource',source_id,'--sql',f'CREATE TABLE {counter} (value integer NOT NULL)','--sql',f'INSERT INTO {counter} VALUES (0)')
            try:
                statements=[f'UPDATE {counter} SET value=value+1 RETURNING value',f'SELECT value, pg_sleep(0.2) FROM {counter}']
                viewed=command('sql','execute','--datasource',source_id,'--sql',statements[0],'--sql',statements[1],'--view')
                rid=viewed['result_id'];assert wait_result(rid)['status']=='completed'
                before_count=len(request('/home')['results'])
                first=str(uuid.uuid4());second=str(uuid.uuid4())
                request('/results/'+rid+'/refresh',{'request_id':first})
                request('/results/'+rid+'/refresh',{'request_id':str(uuid.uuid4())},expected=409)
                refreshed=wait_refresh(rid);assert refreshed['refresh']['status']=='completed'
                assert refreshed['statements']==statements and refreshed['result_id']==rid
                assert request('/results/'+rid+'/rows?statement=0&result=0&offset=0&limit=100')['rows']==[['2']]
                request('/results/'+rid+'/rows?statement=0&result=0&offset=0&limit=100&snapshot=initial',expected=400)
                request('/results/'+rid+'/refresh',{'request_id':second});assert wait_refresh(rid)['refresh']['status']=='completed'
                assert request('/results/'+rid+'/refresh',{'request_id':first})['status']=='completed'
                assert request('/results/'+rid+'/rows?statement=0&result=0&offset=0&limit=100')['rows']==[['3']]
                assert len(request('/home')['results'])==before_count, 'refresh duplicated query history'
                request('/results/'+rid+'/refresh',{'request_id':str(uuid.uuid4())},extra={'Origin':'https://unrelated.example'},expected=403)
                request('/results/'+rid+'/refresh',{'request_id':str(uuid.uuid4())})
                request('/results/'+rid+'/cancel',{})
                cancelled=wait_refresh(rid);assert cancelled['refresh']['status']=='cancelled'
                assert cancelled['snapshot']==second
                command('sql','execute','--datasource',source_id,'--sql',f'DROP TABLE {counter}')
                failed_id=str(uuid.uuid4());request('/results/'+rid+'/refresh',{'request_id':failed_id})
                failed=wait_refresh(rid);assert failed['refresh']['status']=='failed' and failed['snapshot']==second
                assert request('/results/'+rid+'/rows?statement=0&result=0&offset=0&limit=100')['rows']==[['3']]
                assert request('/results/'+rid+'/refresh',{'request_id':failed_id})['status']=='failed'
            finally:command('sql','execute','--datasource',source_id,'--sql',f'DROP TABLE IF EXISTS {counter}')
            command('datasource','remove','--id',source_id)
            assert request('/home')['datasources']==[]
            request('/datasources/'+source_id,expected=404)
            request('/datasources/'+source_id+'/edit',{},expected=404)
            request('/datasources/'+source_id+'/test',{},expected=404)
            print('UI API: authentication, form-only credentials, retry/cancel/edit/CAS, complete indexed pages, no re-execution, restart recovery and cancellation passed')
        finally:
            command('ui','stop')
            for _ in range(200):
                if not (data/'ui/state.json').exists():break
                time.sleep(.05)
if __name__=='__main__':main()
