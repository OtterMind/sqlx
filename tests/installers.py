#!/usr/bin/env python3
"""Validate the bootstrap installer using an actual CLI archive and a local release server."""
import functools,hashlib,http.server,json,os,platform,subprocess,tempfile,threading,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*args):pass
def main():
    with tempfile.TemporaryDirectory(prefix='sqlx-installer-') as tmp:
        root=Path(tmp);web=root/'web';version='0.1.0';release=web/'download'/('v'+version);release.mkdir(parents=True)
        latest=web/'latest/download';latest.mkdir(parents=True);(latest/'release-version.txt').write_text(version+'\n')
        suffix='.exe' if os.name=='nt' else ''
        system='windows' if os.name=='nt' else 'macos' if platform.system()=='Darwin' else 'linux'
        arch='arm64' if platform.machine().lower() in ['arm64','aarch64'] else 'x64'
        asset=f'sqlx-{system}-{arch}.zip';binary=ROOT/'target/debug'/('sqlx'+suffix)
        with zipfile.ZipFile(release/asset,'w',zipfile.ZIP_DEFLATED) as z:z.write(binary,'sqlx'+suffix)
        digest=hashlib.sha256((release/asset).read_bytes()).hexdigest()
        (release/'SHA256SUMS').write_text(f'{digest}  {asset}\n')
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Quiet,directory=str(web)))
        thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
        try:
            env=os.environ.copy();env['SQLX_RELEASE_BASE']=f'http://127.0.0.1:{server.server_port}';env['SQLX_INSTALL_DIR']=str(root/'bin');env.pop('SQLX_VERSION',None)
            cmd=['pwsh','-NoProfile','-File',str(ROOT/'scripts/install.ps1')] if os.name=='nt' else ['sh',str(ROOT/'scripts/install.sh')]
            def install(ok):
                result=subprocess.run(cmd,env=env,capture_output=True,text=True,timeout=60)
                assert (result.returncode==0)==ok,(result.stdout,result.stderr)
            install(True)
            installed=root/'bin'/('sqlx'+suffix)
            assert '(OtterMind/sqlx)' in subprocess.check_output([str(installed),'--version'],text=True)
            original=installed.read_bytes();install(True);assert installed.read_bytes()==original
            (release/'SHA256SUMS').write_text('0'*64+'  '+asset+'\n');install(False);assert installed.read_bytes()==original
            (release/'SHA256SUMS').write_text(f'{digest}  {asset}\n')
            installed.write_bytes(b'unrelated user file');install(False);assert installed.read_bytes()==b'unrelated user file'
            print('installer: verified install, repeat install, checksum rejection and preservation of unrelated files passed')
        finally:server.shutdown();server.server_close();thread.join()
if __name__=='__main__':main()
