#!/usr/bin/env python3
"""Exercise durable analytics with a real MySQL datasource, in an isolated SQLX root.

Usage: python3 tests/analytics_api.py --source-dir /path/to/mysql-demo-data
The fixture datasource is read-only. No fixture SQL data is modified.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-dir', type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='sqlx-analytics-test-') as folder:
        data = Path(folder)
        for name in ['master.key', 'datasources.enc']:
            shutil.copyfile(args.source_dir/name, data/name); os.chmod(data/name, 0o600)
        env = {**os.environ, 'SQLX_DATA_DIR':folder, 'SQLX_WORKER_DIR':str(ROOT/'target/debug')}
        cli = ROOT/'target/debug/sqlx'
        def command(*args):
            p = subprocess.run([str(cli),'--no-open',*args],env=env,capture_output=True,text=True,check=True)
            return json.loads(p.stdout).get('data')
        command('ui','plugin','install','--path',str(ROOT/'ui/dist'))
        command('ui','plugin','use','default')
        def start():
            command('ui'); return json.loads((data/'ui/state.json').read_text())
        state = start(); opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        def api(path, body=None, expected=200):
            request = urllib.request.Request(state['origin']+'/api'+path,data=None if body is None else json.dumps(body).encode(),headers={'Authorization':'Bearer '+state['token'],'Content-Type':'application/json'})
            try:
                with opener.open(request,timeout=30) as response: status=response.status; value=json.load(response)
            except urllib.error.HTTPError as error: status=error.code; value=json.load(error)
            assert status==expected,(path,status,value)
            return value
        def catalog(): return api('/analytics')
        datasource = command('datasource','list')['datasources'][0]['id']
        def chart(sql,name='Contract fixture'):
            return api('/analytics/charts',dict(id=str(uuid.uuid4()),name=name,datasource_id=datasource,statements=[sql],statement=0,result=0,snapshot_id=None,history=[],revision=0,spec=dict(chart_type='Table',dimension=None,group_by=[],metrics=[],stack=False,line_style='straight')))
        def run(c, expected='completed'):
            request_id=str(uuid.uuid4())
            first=api('/analytics/charts/'+c['id']+'/run',dict(request_id=request_id))
            replay=api('/analytics/charts/'+c['id']+'/run',dict(request_id=request_id))
            assert replay['request_id']==first['request_id']
            for _ in range(100):
                cat=catalog(); r=next(v for v in cat['runs'] if v['request_id']==request_id)
                if r['status']!='running':
                    assert r['status']==expected,r
                    return next(v for v in cat['charts'] if v['id']==c['id'])
                time.sleep(.05)
            raise AssertionError('query did not finish')
        try:
            c=run(chart("SELECT 'Jan' AS month,NULL AS region,'a / b' AS channel,10.25 AS revenue,1.5 AS rate UNION ALL SELECT 'Feb',NULL,'a / b',12.75,1.6 UNION ALL SELECT 'Jan','','a / b',8,1.4"))
            head=c['snapshot_id']; meta=api('/analytics/snapshots/'+head)
            rows=api('/analytics/snapshots/'+head+'/rows?statement=0&result=0&limit=200')['rows']
            assert len(rows)==3 and rows[0][1] is None and rows[2][1]=='',rows
            metric=lambda field,unit='',axis='left',kind=None:dict(field=field,label=field,unit=unit,axis=axis,kind=kind)
            spec=dict(chart_type='Combo',dimension='month',group_by=['region','channel'],metrics=[metric('revenue','CNY',kind='Column'),metric('rate','%','right','Line')],stack=True,line_style='smooth')
            rich=api('/analytics/charts',{**c,'name':'Mixed multidimensional','spec':spec})
            board=api('/analytics/dashboards',dict(id=str(uuid.uuid4()),name='Persistent fixture',description='',charts=[dict(chart_id=rich['id'],x=0,y=0,w=6,h=4)],revision=0))
            stale=dict(board); board['name']='Saved version'; board=api('/analytics/dashboards',board)
            api('/analytics/dashboards',stale,400)
            # Running a chart publishes a new head and keeps the previous one in history.
            refreshed=run(c)
            assert refreshed['snapshot_id']!=head and head in refreshed['history']
            assert api('/analytics/snapshots/'+head+'/rows?statement=0&result=0')['rows']==rows
            failing=run(chart('SELECT missing_column FROM sales'),'failed'); assert failing['snapshot_id'] is None
            # A failed run preserves the preceding complete snapshot.
            changed=api('/analytics/charts',{**refreshed,'statements':['SELECT missing_column FROM sales']})
            assert run(changed,'failed')['snapshot_id']==refreshed['snapshot_id']
            # A chart can be edited without losing the snapshot it already shows.
            renamed=api('/analytics/charts',{**changed,'name':'Renamed fixture'})
            assert renamed['snapshot_id']==refreshed['snapshot_id']
            dup=run(chart("SELECT 'Jan' AS month,1 AS revenue UNION ALL SELECT 'Jan',2"))
            invalid=dict(id=str(uuid.uuid4()),name='Duplicate grain',datasource_id=datasource,statements=['SELECT 1'],statement=0,result=0,snapshot_id=dup['snapshot_id'],history=[],revision=0,spec=dict(chart_type='Column',dimension='month',group_by=[],metrics=[metric('revenue')],stack=False,line_style='straight'))
            api('/analytics/charts',invalid,400)
            invalid['spec']['chart_type']='Statistics';invalid['spec']['dimension']=None;api('/analytics/charts',invalid,400)
            # Query history lists page results (with their label), separate from chart snapshots.
            assert api('/home')['results']==[]
            view_id=str(uuid.uuid4())
            api('/results',dict(request_id=view_id,datasource=datasource,statements=['SELECT 7 AS sales_rows'],description='Row count'))
            for _ in range(100):
                view=api('/results/'+view_id)
                if view['status']=='completed': break
                time.sleep(.05)
            assert view['status']=='completed',view
            assert view['description']=='Row count'
            listed=next(v for v in api('/home')['results'] if v['id']==view_id)
            assert listed['name']=='Row count',listed
            shutil.rmtree(data/'results'/view_id)
            command('ui','stop')
            for _ in range(100):
                if not (data/'ui/state.json').exists(): break
                time.sleep(.05)
            state=start()
            reopened=catalog();assert next(v for v in reopened['dashboards'] if v['id']==board['id'])['name']=='Saved version'
            assert api('/analytics/snapshots/'+head+'/rows?statement=0&result=0')['rows']==rows
            assert api('/home')['results']==[]
            assert next(v for v in reopened['charts'] if v['id']==c['id'])['snapshot_id']==refreshed['snapshot_id']
            print('Analytics API passed: real MySQL, immutable complete rows, dual-axis grouping, request dedupe, revision conflicts, failed run, snapshot history, per-chart SQL and labelled query history.')
        finally:
            command('ui','stop')
            for _ in range(100):
                if not (data/'ui/state.json').exists(): break
                time.sleep(.05)

if __name__=='__main__': main()
