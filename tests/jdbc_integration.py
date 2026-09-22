#!/usr/bin/env python3
"""Run the actual CLI/JDBC path against an isolated Oracle or SQL Server fixture."""
import argparse,json,os,subprocess,sys,tempfile,time
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parent))
import contract

ROOT=Path(__file__).resolve().parents[1]
def exercise(kind,bin_dir):
    cli=bin_dir/'sqlx'
    password='SQLX_Test_Only_12345'
    connection=dict(database_type=kind,host='127.0.0.1',port=21521 if kind=='oracle' else 21433,database='master' if kind=='sqlserver' else '',service='FREEPDB1' if kind=='oracle' else '',username='system' if kind=='oracle' else 'sa',password=password,tls='disable')
    with tempfile.TemporaryDirectory(prefix='sqlx-jdbc-test-') as tmp:
        def call(*args,ok=True,payload=None):
            out=subprocess.run([str(cli),'--data-dir',tmp,'--worker-dir',str(bin_dir),*args],input=json.dumps(payload) if payload else None,text=True,capture_output=True,timeout=90)
            value=json.loads(out.stdout)
            if ok:assert out.returncode==0 and value['success'],value
            return out.returncode,value
        call('datasource','add','--name','fixture','--connection-stdin',payload=connection)
        for attempt in range(60):
            code,result=call('datasource','test','--id','fixture',ok=False)
            if code==0:break
            if attempt==59:raise AssertionError(result)
            time.sleep(3)
        if kind=='oracle':
            statements=['CREATE GLOBAL TEMPORARY TABLE SQLX_TEST_VALUES (id NUMBER(19), amount NUMBER(30,4), label VARCHAR2(100)) ON COMMIT PRESERVE ROWS',"INSERT INTO SQLX_TEST_VALUES VALUES (9007199254740993,123.4500,'hello')",'SELECT id AS DUP, id AS DUP, amount, label FROM SQLX_TEST_VALUES','TRUNCATE TABLE SQLX_TEST_VALUES','DROP TABLE SQLX_TEST_VALUES']
        else:
            statements=['CREATE TABLE #sqlx_values (id BIGINT, amount DECIMAL(30,4), label NVARCHAR(100))',"INSERT INTO #sqlx_values VALUES (9007199254740993,123.4500,'hello')",'SELECT id AS DUP,id AS DUP,amount,label FROM #sqlx_values']
        args=['sql','execute','--datasource','fixture']
        for statement in statements:args+=['--command',statement]
        _,result=call(*args)
        rows=contract.rows(result,2)
        assert len(rows)==1 and rows[0][:2]==['9007199254740993','9007199254740993'],rows
        from decimal import Decimal
        assert Decimal(rows[0][2])==Decimal('123.4500')
        columns=contract.columns(result,2)
        assert columns[0][0]==columns[1][0],columns
        query='SELECT 1 FROM dual' if kind=='oracle' else 'SELECT 1'
        code,result=call('sql','execute','--datasource','fixture','--command',query,'--command','SELECT * FROM SQLX_MISSING_TABLE','--command',query,ok=False)
        assert code!=0 and contract.skipped(result)==[2],result
        if kind=='sqlserver':
            _,result=call('sql','execute','--datasource','fixture','--command','SELECT 1 AS value; SELECT 2 AS value')
            # One statement that returns two result sets numbers the second one.
            assert contract.rows(result,0,0)==[['1']] and contract.rows(result,0,1)==[['2']],result
        print(f'{kind}: actual JDBC load, connection, same-session batch, DDL/DML/query, numeric precision, duplicate columns and first-error stop passed')

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('kind',choices=['oracle','sqlserver']);parser.add_argument('--bin-dir',type=Path,default=ROOT/'target/debug');args=parser.parse_args();exercise(args.kind,args.bin_dir)
