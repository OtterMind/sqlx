#!/usr/bin/env python3
"""Playwright CLI checks for MySQL outages, retained results and UI restarts.

Requires the isolated MySQL fixture on 23306, built binaries and ui/dist.
A test-owned TCP proxy simulates outages without stopping the database.
"""
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import tempfile
import threading
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


class DatabaseProxy:
    def __init__(self):
        self.port = 0
        self.connections = 0
        self.listener = None
        self.thread = None

    def start(self):
        listener = socket.socket()
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(('127.0.0.1', self.port))
        self.port = listener.getsockname()[1]
        listener.listen()
        listener.settimeout(.1)
        self.listener = listener
        self.thread = threading.Thread(target=self.accept, args=(listener,), daemon=True)
        self.thread.start()

    def accept(self, listener):
        while self.listener is listener:
            try:
                client, _ = listener.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            self.connections += 1
            threading.Thread(target=self.forward, args=(client,), daemon=True).start()

    @staticmethod
    def forward(client):
        with client, socket.create_connection(('127.0.0.1', 23306), timeout=5) as upstream:
            while True:
                ready, _, _ = select.select([client, upstream], [], [], 20)
                if not ready:
                    return
                for source in ready:
                    data = source.recv(65536)
                    if not data:
                        return
                    (upstream if source is client else client).sendall(data)

    def stop(self):
        if self.listener:
            listener, self.listener = self.listener, None
            listener.close()
            self.thread.join(timeout=2)


def main():
    proxy = DatabaseProxy()
    proxy.start()
    with tempfile.TemporaryDirectory(prefix='sqlx-refresh-recovery-') as tmp:
        directory = Path(tmp)
        session = 'sqlx-recovery-' + uuid.uuid4().hex[:8]
        env = dict(os.environ, SQLX_DATA_DIR=str(directory/'data'),
                   SQLX_WORKER_DIR=str(ROOT/'target/debug'), SQLX_NO_UPDATE_CHECK='1')

        def cli(*args, input=None):
            run = subprocess.run([str(ROOT/'target/debug/sqlx'), '--no-open', *args],
                                 env=env, input=input, capture_output=True, text=True, timeout=30)
            value = json.loads(run.stdout)
            assert run.returncode == 0 and value['success'], value
            return value.get('data', value)

        def browser(*args):
            run = subprocess.run(['rtk', 'proxy', 'playwright-cli', '-s='+session, *args],
                                 cwd=directory, capture_output=True, text=True, timeout=45)
            assert run.returncode == 0 and '### Error' not in run.stdout, run.stdout[-6000:]+run.stderr
            return run.stdout

        def ready():
            for _ in range(200):
                meta = json.loads(metadata.read_text())
                if meta['status'] not in ['running', 'queued'] and (meta.get('refresh') or {}).get('status') != 'running':
                    return meta
                time.sleep(.05)
            raise AssertionError('Result did not finish')

        def check(case):
            config = dict(case=case, url=url, endpoint='127.0.0.1:'+str(proxy.port))
            code = (ROOT/'tests/ui_refresh_recovery.js').read_text().rstrip().removesuffix(';')
            output = browser('run-code', code.replace('__CONFIG__', json.dumps(config)))
            assert '"passed"' in output, output
            print('PASS', case, flush=True)

        try:
            source = cli('datasource', 'add', '--name', 'Recovery fixture', '--connection-stdin',
                         input=json.dumps(dict(database_type='mysql', host='127.0.0.1', port=proxy.port,
                                               database='sqlx_test', username='root',
                                               password='sqlx_test_only_password', tls='disable')))['id']
            cli('ui', 'plugin', 'install', '--path', str(ROOT/'ui/dist'))
            cli('ui', 'plugin', 'use', 'default')
            result = cli('sql', 'execute', '--datasource', source, '--sql',
                         "SELECT CAST(9007199254740993 AS UNSIGNED) AS exact_value, 'retained' AS label", '--view')
            url = result['url']
            metadata = directory/'data/results'/result['result_id']/'metadata.json'
            assert ready()['status'] == 'completed'
            browser('open', url, '--browser=chrome')
            check('initial')
            proxy.stop()
            check('outage')
            failed = ready()
            assert failed['refresh']['status'] == 'failed'
            assert 'MySQL at 127.0.0.1:' in failed['refresh']['error']
            assert 'Input/output error' not in failed['refresh']['error']
            proxy.start()
            connections = proxy.connections
            check('historical')
            assert proxy.connections == connections, 'Reload or focus replayed SQL'
            check('recover')
            assert ready()['refresh']['status'] == 'completed'
            assert proxy.connections == connections + 1, 'Refresh replayed SQL'
            proxy.stop()
            check('second_outage')
            proxy.start()
            check('recover_on_focus')
            check('auto_refresh_recovery')
            connections = proxy.connections
            previous_snapshot = ready()['snapshot']
            origin = url.split('/result/')[0]
            cli('ui', 'stop')
            assert cli('ui')['url'] == origin+'/'
            check('restart')
            assert ready()['snapshot'] == previous_snapshot and proxy.connections == connections, 'UI restart replayed SQL'
            print('MySQL outage/recovery and SQLX restart: cached data, no replay, error cleanup passed', flush=True)
        finally:
            try:
                browser('close')
                browser('delete-data')
            finally:
                try:
                    cli('ui', 'stop')
                finally:
                    proxy.stop()


if __name__ == '__main__':
    main()
