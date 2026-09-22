#!/usr/bin/env python3
"""Real database tests. Uses only the dedicated SQLX fixtures, never saved user datasources."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]

def run(cli, data, workers, *args, payload=None, success=True):
    result = subprocess.run([str(cli), '--data-dir', str(data), '--worker-dir', str(workers), *args], input=None if payload is None else json.dumps(payload), text=True, capture_output=True, timeout=180)
    try:
        value = json.loads(result.stdout)
    except ValueError:
        raise AssertionError(f'invalid CLI JSON; exit={result.returncode}; stderr={result.stderr[-1000:]}') from None
    if success:
        assert result.returncode == 0 and value['success'], value
    else:
        assert result.returncode != 0 and not value['success'], value
    return value

def exercise(cli, workers, kind, port, username):
    with tempfile.TemporaryDirectory(prefix='sqlx-test-') as temp:
        data = Path(temp)/'data'
        connection = dict(database_type=kind, host='127.0.0.1', port=port, database='sqlx_test', username=username, password='sqlx_test_only_password', tls='disable')
        run(cli,data,workers,'datasource','add','--name','fixture','--connection-stdin',payload=connection)
        run(cli,data,workers,'datasource','test','--id','fixture')
        create = 'CREATE TEMPORARY TABLE sqlx_values (id BIGINT, amount DECIMAL(30,4), label VARCHAR(100))'
        statements=[create,"INSERT INTO sqlx_values VALUES (9007199254740993,123.4500,'你好 SQLX')",'SELECT id AS duplicate, id AS duplicate, amount, label FROM sqlx_values']
        args=['sql','execute','--datasource','fixture']
        for sql in statements: args += ['--command',sql]
        result=run(cli,data,workers,*args)
        row=next(e for e in result['events'] if e['event']=='row' and e['index']==2)
        assert row['values']==['9007199254740993','9007199254740993','123.4500','你好 SQLX'],row
        columns=next(e for e in result['events'] if e['event']=='columns' and e['index']==2)
        assert columns['columns'][0]['name']==columns['columns'][1]['name']=='duplicate'
        result=run(cli,data,workers,'sql','execute','--datasource','fixture','--command','SELECT 1','--command','SELECT missing_column FROM missing_table','--command','SELECT 2',success=False)
        assert any(e['event']=='skipped' and e['index']==2 for e in result['events']),result
        # The preceding temporary table must not survive into a new CLI call.
        run(cli,data,workers,'sql','execute','--datasource','fixture','--command','SELECT * FROM sqlx_values',success=False)
        result=run(cli,data,workers,'sql','execute','--datasource','fixture','--command',"SELECT repeat('x', 1100000)")
        assert len(next(e for e in result['events'] if e['event']=='row')['values'][0])==1100000
        if kind=='postgresql':
            result=run(cli,data,workers,'sql','execute','--datasource','fixture','--command','SELECT n FROM generate_series(1,100005) n')
            assert sum(e['event']=='row' for e in result['events'])==100005
        else:
            sqls=['DROP PROCEDURE IF EXISTS sqlx_multi','CREATE PROCEDURE sqlx_multi() BEGIN SELECT 1; SELECT 2; END','CALL sqlx_multi()','DROP PROCEDURE sqlx_multi']
            args=['sql','execute','--datasource','fixture']
            for sql in sqls:args+=['--command',sql]
            result=run(cli,data,workers,*args)
            assert len([e for e in result['events'] if e['event']=='row' and e['index']==2])==2
        print(f'{kind}: connection, same-connection batch, precision, duplicate columns, first-error stop, session closure and complete output passed')

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--bin-dir',type=Path,default=ROOT/'target/debug');options=parser.parse_args()
    cli=options.bin_dir/('sqlx.exe' if os.name=='nt' else 'sqlx')
    exercise(cli,options.bin_dir,'mysql',int(os.getenv('SQLX_TEST_MYSQL_PORT','23306')),'root')
    exercise(cli,options.bin_dir,'postgresql',int(os.getenv('SQLX_TEST_POSTGRES_PORT','25432')),'postgres')
