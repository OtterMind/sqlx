#!/usr/bin/env python3
"""Real database tests. Uses only the dedicated SQLX fixtures, never saved user datasources."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))
import contract


def run(cli, data, workers, *args, payload=None, success=True):
    return contract.run(cli, data, *args, payload=payload, workers=workers, success=success)


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
        assert list(result)==['results','success'],result
        assert contract.rows(result,2)==[['9007199254740993','9007199254740993','123.4500','你好 SQLX']],result
        # Each engine reports its own type names, shortened to what a caller has to know.
        expected={'mysql':[['duplicate','bigint'],['duplicate','bigint'],['amount','decimal'],['label','varchar']],
                  'postgresql':[['duplicate','int8'],['duplicate','int8'],['amount','numeric'],['label','varchar']]}[kind]
        assert contract.columns(result,2)==expected,result
        assert contract.affected(result,1)=='1' and contract.count(result,1) is None,result
        assert contract.affected(result,0)=='0',result  # MySQL reports a write count for DDL
        # A result set that fits the preview is printed completely and never stored.
        assert contract.stored_file(result,2) is None,result
        result=run(cli,data,workers,'sql','execute','--datasource','fixture','--command','SELECT 1','--command','SELECT missing_column FROM missing_table','--command','SELECT 2',success=False)
        assert contract.skipped(result)==[2] and contract.error(result,1)['outcome']=='failed',result
        assert contract.rows(result,0)==[['1']],result
        # The preceding temporary table must not survive into a new CLI call.
        run(cli,data,workers,'sql','execute','--datasource','fixture','--command','SELECT * FROM sqlx_values',success=False)
        # One very large cell is stored, not printed: the preview stops at its byte budget.
        result=run(cli,data,workers,'sql','execute','--datasource','fixture','--command',"SELECT repeat('x', 1100000)")
        assert contract.rows(result)==[] and contract.count(result)=='1',result
        assert len(json.dumps(result))<1024,result
        stored=contract.stored_file(result)
        assert stored and len(contract.stored_rows(stored)[0][0])==1100000,result
        # The same call with an explicit preview of zero stores the row instead of printing it.
        result=run(cli,data,workers,'sql','execute','--datasource','fixture','--command',"SELECT repeat('y', 10)",'--preview','0')
        assert contract.rows(result)==[] and contract.count(result)=='1',result
        stored=contract.stored_file(result)
        assert stored and contract.stored_rows(stored)==[['y'*10]],result
        assert contract.run(cli,data,'results','rows','--id',result['id'])['data']['rows']==[['y'*10]]
        # Settings decide where results are stored and how many rows are previewed.
        custom = Path(temp)/'custom-results'
        settings = contract.run(cli,data,'setting','set','results-dir',str(custom))['data']
        assert settings=={'key':'results-dir','value':str(custom),'source':'file'},settings
        contract.run(cli,data,'setting','set','preview-rows','2')
        listed = contract.run(cli,data,'setting','list')['data']
        assert {entry['key']:entry['source'] for entry in listed['settings']}=={'preview-rows':'file','results-dir':'file','results-retention-hours':'default'},listed
        result=run(cli,data,workers,'sql','execute','--datasource','fixture','--command','SELECT 1 AS n UNION SELECT 2 UNION SELECT 3')
        assert len(contract.rows(result))==2 and contract.count(result)=='3',result
        assert contract.stored_file(result).startswith(str(custom)+'/'),result
        assert contract.run(cli,data,'results','list')['data']['dir']==str(custom)
        for key in ('results-dir','preview-rows'):
            assert contract.run(cli,data,'setting','unset',key)['data']['source']=='default'
        contract.run(cli,data,'setting','set','results-dir','relative/path',success=False)
        # The raw worker stream is still available for scripts that parse it.
        result=run(cli,data,workers,'sql','execute','--datasource','fixture','--command','SELECT 1','--events')
        assert result['protocol_version']==1 and any(e['event']=='ready' for e in result['events']),result
        if kind=='postgresql':
            result=run(cli,data,workers,'sql','execute','--datasource','fixture','--command','SELECT n FROM generate_series(1,100005) n')
            assert contract.count(result)=='100005',result
            # A large result must stay out of the printed output and live in its stored file.
            printed=json.dumps(result)
            assert len(printed)<4096,f'{len(printed)} bytes for 100005 rows: {printed[:200]}'
            assert len(contract.rows(result))==10,result
            stored=contract.stored_file(result)
            assert stored and sum(1 for _ in open(stored))==100005,stored
            tail=contract.run(cli,data,'results','rows','--id',result['id'],'--offset','100000')
            assert tail['data']['rows'][0]==['100001'] and tail['data']['complete'],tail
        else:
            sqls=['DROP PROCEDURE IF EXISTS sqlx_multi','CREATE PROCEDURE sqlx_multi() BEGIN SELECT 1; SELECT 2; END','CALL sqlx_multi()','DROP PROCEDURE sqlx_multi']
            args=['sql','execute','--datasource','fixture']
            for sql in sqls:args+=['--command',sql]
            result=run(cli,data,workers,*args)
            # A statement with two result sets numbers the second one.
            assert contract.rows(result,2,0)==[['1']] and contract.rows(result,2,1)==[['2']],result
        print(f'{kind}: connection, same-connection batch, precision, duplicate columns, first-error stop, session closure, stored results and complete output passed')

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--bin-dir',type=Path,default=ROOT/'target/debug');options=parser.parse_args()
    cli=options.bin_dir/('sqlx.exe' if os.name=='nt' else 'sqlx')
    exercise(cli,options.bin_dir,'mysql',int(os.getenv('SQLX_TEST_MYSQL_PORT','23306')),'root')
    exercise(cli,options.bin_dir,'postgresql',int(os.getenv('SQLX_TEST_POSTGRES_PORT','25432')),'postgres')
