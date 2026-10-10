#!/usr/bin/env python3
"""Seed an isolated Docker MySQL fixture and a real SQLX analytics workspace.

No credentials enter arguments, output, source files or the browser. Existing schemas
are refused. Requires locally built CLI, UI, MySQL worker and ui/dist assets.
"""
import argparse
import csv
import datetime
import decimal
import json
import os
from pathlib import Path
import secrets
import subprocess
import time
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[2]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data-dir', type=Path, required=True)
    parser.add_argument('--schema', default='sqlx_chart_demo_20260930')
    parser.add_argument('--container', default='mysql')
    args = parser.parse_args()
    if not args.schema.replace('_', '').isalnum():
        raise ValueError('Schema must contain only letters, digits and underscores')
    args.data_dir.mkdir(parents=True, exist_ok=True)
    os.chmod(args.data_dir, 0o700)
    cli = ROOT / 'target/debug/sqlx'
    env = {**os.environ, 'SQLX_DATA_DIR':str(args.data_dir), 'SQLX_WORKER_DIR':str(ROOT/'target/debug')}
    def command(*argv, input=None):
        process = subprocess.run([str(cli), '--no-open', *argv], env=env, input=input, text=True, capture_output=True, check=True)
        value = json.loads(process.stdout)
        return value.get('data', value)
    subprocess.run([str(cli), '--version'], env=env, check=True)
    inspection = json.loads(subprocess.check_output(['docker','inspect',args.container]))[0]
    settings = dict(v.split('=',1) for v in inspection['Config']['Env'] if '=' in v)
    root_password = settings.get('MYSQL_ROOT_PASSWORD','')
    def mysql(sql):
        process = subprocess.run(['docker','exec','-i',args.container,'sh','-c','IFS= read -r MYSQL_PWD; export MYSQL_PWD; exec mysql -uroot --default-character-set=utf8mb4 -N'], input=root_password+'\n'+sql, capture_output=True, text=True)
        if process.returncode:
            raise RuntimeError('Demo fixture SQL failed: '+process.stderr.replace(root_password,'[redacted]') if root_password else process.stderr)
        return process.stdout
    if mysql(f"SELECT COUNT(*) FROM INFORMATION_SCHEMA.SCHEMATA WHERE SCHEMA_NAME='{args.schema}';").strip() != '0':
        raise RuntimeError('Demo schema already exists; refusing to overwrite it')
    if (args.data_dir/'analytics/catalog.json').exists():
        raise RuntimeError('Analytics catalog already exists; use a new data directory')
    rows = []
    regions = ['华东','华南','华北','西部']; channels = ['官网','App']; products = ['数据库工具','云服务','团队协作']
    for month in range(1,13):
        for r,region in enumerate(regions):
            for c,channel in enumerate(channels):
                for p,product in enumerate(products):
                    visits = 900 + month*55 + r*180 + c*120 + p*75
                    orders = 35 + month*5 + r*8 + c*5 + p*3
                    price = decimal.Decimal('199.90') + p*90 + c*20
                    revenue = price*orders
                    rows.append([f'2026-{month:02}-01',region,channel,product,str(revenue),orders,visits])
    quote = lambda value: "'"+str(value).replace("'","''")+"'"
    values = ',\n'.join('('+','.join(quote(v) for v in row)+')' for row in rows)
    words = [('SQL',320),('数据库',280),('分析',230),('MySQL',200),('可视化',180),('多维',160),('团队',145),('查询',130),('图表',120),('报表',100),('效率',95),('连接',85),('性能',75),('工作台',65),('数据',55),('协作',45)]
    sql = f"""CREATE DATABASE `{args.schema}` CHARACTER SET utf8mb4;
CREATE TABLE `{args.schema}`.sales (month DATE NOT NULL, region VARCHAR(20) NOT NULL, channel VARCHAR(20) NOT NULL, product VARCHAR(30) NOT NULL, revenue DECIMAL(18,2) NOT NULL, orders INT NOT NULL, visits INT NOT NULL, PRIMARY KEY(month,region,channel,product));
INSERT INTO `{args.schema}`.sales VALUES {values};
CREATE TABLE `{args.schema}`.funnel (position INT PRIMARY KEY, stage VARCHAR(40), users INT);
INSERT INTO `{args.schema}`.funnel VALUES (1,'访问',48000),(2,'注册',16800),(3,'试用',8500),(4,'购买',3400),(5,'续费',2100);
CREATE TABLE `{args.schema}`.keywords (word VARCHAR(40) PRIMARY KEY, mentions INT);
INSERT INTO `{args.schema}`.keywords VALUES {','.join('('+quote(w)+','+str(n)+')' for w,n in words)};
"""
    mysql(sql)
    # The fixture account has SELECT on this one schema, no write permissions.
    user = 'sqlx_demo_'+secrets.token_hex(4); password = secrets.token_hex(24)
    mysql(f"CREATE USER '{user}'@'%' IDENTIFIED BY '{password}'; GRANT SELECT ON `{args.schema}`.* TO '{user}'@'%';")
    connection = dict(database_type='mysql',host='127.0.0.1',port=3306,database=args.schema,service='',username=user,password=password,tls='disable',properties={})
    datasource = command('datasource','add','--name','图表演示 MySQL','--connection-stdin', input=json.dumps(connection))
    datasource_id = datasource.get('id') or datasource.get('datasource_id')
    if not datasource_id:
        datasource_id = command('datasource','list')['datasources'][0]['id']
    command('ui','plugin','install','--path',str(ROOT/'ui/dist'))
    command('ui','plugin','use','default')
    page = command('ui')
    state = json.loads((args.data_dir/'ui/state.json').read_text())
    origin = state['origin']
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    def api(path, body=None):
        request = urllib.request.Request(origin+'/api/analytics'+path, data=None if body is None else json.dumps(body).encode(), headers={'Authorization':'Bearer '+state['token'], 'Content-Type':'application/json'})
        with opener.open(request, timeout=30) as response:
            return json.load(response)
    def run_chart(chart_id):
        run = api('/charts/'+chart_id+'/run',dict(request_id=str(uuid.uuid4())))
        for _ in range(300):
            catalog = api(''); current = next(r for r in catalog['runs'] if r['request_id']==run['request_id'])
            if current['status']=='failed': raise RuntimeError(current['error'])
            if current['status']=='completed': return next(v for v in catalog['charts'] if v['id']==chart_id)
            time.sleep(.1)
        raise RuntimeError('Demo query timed out')
    # A chart owns its SQL: each one is created, run and then placed on a board.
    summary_sql = 'SELECT SUM(revenue) AS revenue, SUM(orders) AS orders, SUM(visits) AS visits, ROUND(SUM(orders)*100.0/SUM(visits),2) AS conversion_rate FROM sales'
    trend_sql = "SELECT DATE_FORMAT(month,'%Y-%m') AS month,region,channel,SUM(revenue) AS revenue,SUM(orders) AS orders,SUM(visits) AS visits,ROUND(SUM(orders)*100.0/SUM(visits),2) AS conversion_rate FROM sales GROUP BY month,region,channel ORDER BY month,region,channel"
    monthly_sql = "SELECT DATE_FORMAT(month,'%Y-%m') AS month,SUM(revenue) AS revenue,SUM(orders) AS orders FROM sales GROUP BY month ORDER BY month"
    regional_sql = 'SELECT region,SUM(revenue) AS revenue FROM sales GROUP BY region ORDER BY revenue DESC'
    detail_sql = "SELECT DATE_FORMAT(month,'%Y-%m') AS month,region,channel,product,revenue,orders,visits FROM sales ORDER BY month,region,channel,product"
    funnel_sql = 'SELECT stage,users FROM funnel ORDER BY position'
    cloud_sql = 'SELECT word,mentions FROM keywords ORDER BY mentions DESC'
    def metric(field='revenue', unit='CNY', axis='left', kind=None):
        return dict(field=field,label=field,unit=unit,axis=axis,kind=kind)
    charts = []
    def chart(name, sql, type, dimension=None, groups=None, metrics=None, stack=False, style='straight'):
        c = api('/charts',dict(id=str(uuid.uuid4()),name=name,datasource_id=datasource_id,statements=[sql],statement=0,result=0,snapshot_id=None,history=[],revision=0,spec=dict(chart_type=type,dimension=dimension,group_by=groups or [],metrics=[metric()] if metrics is None else metrics,stack=stack,line_style=style)))
        c = run_chart(c['id'])
        charts.append(c); return c
    revenue = chart('总销售额',summary_sql,'Statistics',metrics=[metric()])
    orders = chart('订单总量',summary_sql,'Statistics',metrics=[metric('orders','单')])
    rate = chart('整体转化率',summary_sql,'Statistics',metrics=[metric('conversion_rate','%')])
    combo = chart('多维销售额 × 转化率 · 双轴',trend_sql,'Combo','month',['region','channel'],[metric(kind='Column'),metric('conversion_rate','%','right','Line')],True)
    line = chart('月度趋势 · 平滑折线',monthly_sql,'Line','month',style='smooth')
    bar = chart('各地区销售额',regional_sql,'Bar','region')
    pie = chart('地区占比 · 饼图',regional_sql,'Pie','region')
    ring = chart('地区占比 · 环形图',regional_sql,'RingPie','region')
    rose = chart('地区销售 · 玫瑰图',regional_sql,'RosePie','region')
    scatter = chart('访问量 × 销售额 · 三维分组',detail_sql,'Scatter','visits',['region','channel','product'])
    table = chart('完整销售明细',detail_sql,'Table',metrics=[])
    fun = chart('从访问到续费',funnel_sql,'Funnel','stage',metrics=[metric('users','人')])
    word = chart('产品反馈关键词',cloud_sql,'WordCloud','word',metrics=[metric('mentions','次')])
    column = chart('月份 × 地区 × 渠道 × 商品 · 24 系列',detail_sql,'Column','month',['region','channel','product'])
    stacked = chart('地区与渠道 · 堆叠柱状图',trend_sql,'Column','month',['region','channel'],stack=True)
    area = chart('多维堆叠面积',trend_sql,'AreaLine','month',['region','channel'],stack=True)
    step = chart('月度趋势 · 阶梯折线',monthly_sql,'Line','month',style='step')
    multibar = chart('多维堆叠条形图',trend_sql,'Bar','month',['region','channel'],stack=True)
    def board(name,description,cells):
        return api('/dashboards',dict(id=str(uuid.uuid4()),name=name,description=description,revision=0,charts=[dict(chart_id=c['id'],x=x,y=y,w=w,h=h) for c,x,y,w,h in cells]))
    main_board = board('销售分析 · MySQL 本地演示','2026 年销售数据 · 12 个月 / 4 地区 / 2 渠道 / 3 商品',[(revenue,0,0,4,3),(orders,4,0,4,3),(rate,8,0,4,3),(combo,0,3,12,5),(line,0,8,6,4),(bar,6,8,6,4),(ring,0,12,4,4),(fun,4,12,4,4),(word,8,12,4,4),(scatter,0,16,6,5),(table,6,16,6,5)])
    gallery = [column,multibar,line,area,scatter,pie,ring,rose,fun,word,revenue,combo,table,stacked,step]
    gallery_board = board('图表与多维类型全集','13 类图表 + 分组 / 堆叠 / 平滑 / 阶梯 / 双轴；所有图表读取真实 MySQL 快照。',[(c,0 if i%2==0 else 6,(i//2)*5,6,5) for i,c in enumerate(gallery)])
    export = args.data_dir/'sales-demo.csv'
    with export.open('w',encoding='utf-8-sig',newline='') as file:
        writer=csv.writer(file);writer.writerow(['month','region','channel','product','revenue','orders','visits']);writer.writerows(rows)
    report=dict(url=origin+'/dashboard/'+main_board['id'],gallery_url=origin+'/dashboard/'+gallery_board['id'],schema=args.schema,tables={'sales':len(rows),'funnel':5,'keywords':len(words)},charts=len(charts),csv=str(export))
    (args.data_dir/'demo.json').write_text(json.dumps(report,ensure_ascii=False,indent=2))
    print(json.dumps(report,ensure_ascii=False,indent=2))

if __name__=='__main__': main()
