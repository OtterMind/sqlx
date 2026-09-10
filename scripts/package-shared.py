#!/usr/bin/env python3
"""Package the JDBC runner, licensed JDBC artifacts and the English Skill; pin upstream JRE assets."""
import argparse,hashlib,json,urllib.parse,urllib.request,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def get(url):
    with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'OtterMind-SQLX-release'}),timeout=60) as r:return r.read()
def main():
    p=argparse.ArgumentParser();p.add_argument('--version',default='0.1.0');p.add_argument('--output',type=Path,default=ROOT/'dist');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
    records={};base=f'https://github.com/OtterMind/sqlx/releases/download/v{a.version}/'
    def add(name,entries,entrypoint):
        path=a.output/f'{name}-{a.version}.zip'
        with zipfile.ZipFile(path,'w',zipfile.ZIP_DEFLATED) as z:
            for filename,body in entries.items():z.writestr(filename,body)
        records[f'{name}:any']=dict(version=a.version,url=base+path.name,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),archive='zip',entrypoint=entrypoint,cli_compat='>=0.1.0, <0.2.0',protocol_version=1)
    add('jdbc',{'sqlx-jdbc.jar':(ROOT/f'java/jdbc/target/sqlx-jdbc-{a.version}.jar').read_bytes(),'LICENSE':(ROOT/'LICENSE').read_bytes(),'NOTICE':(ROOT/'NOTICE').read_bytes()},'sqlx-jdbc.jar')
    skill=ROOT/'skills/sqlx';add('skill',{str(f.relative_to(skill)).replace('\\','/'):f.read_bytes() for f in skill.rglob('*') if f.is_file()},'SKILL.md')
    vendors={
        'oracle':('https://repo.maven.apache.org/maven2/com/oracle/database/jdbc/ojdbc11/23.6.0.24.10/ojdbc11-23.6.0.24.10.jar','ojdbc.jar','https://www.oracle.com/downloads/licenses/oracle-free-license.html'),
        'sqlserver':('https://repo.maven.apache.org/maven2/com/microsoft/sqlserver/mssql-jdbc/12.10.1.jre11/mssql-jdbc-12.10.1.jre11.jar','mssql-jdbc.jar','https://raw.githubusercontent.com/microsoft/mssql-jdbc/v12.10.1/LICENSE'),
    }
    for name,(url,filename,license_url) in vendors.items():
        add(name,{filename:get(url),'LICENSE.html' if name=='oracle' else 'LICENSE.txt':get(license_url),'SOURCE.txt':(url+'\n'+license_url+'\n').encode()},filename)
    for platform,os_name,arch in [('macos-arm64','mac','aarch64'),('macos-x64','mac','x64'),('windows-x64','windows','x64'),('linux-arm64','linux','aarch64'),('linux-x64','linux','x64')]:
        params=urllib.parse.urlencode(dict(architecture=arch,image_type='jre',os=os_name,vendor='eclipse'))
        releases=json.loads(get('https://api.adoptium.net/v3/assets/latest/17/hotspot?'+params))
        item=releases[0];binary=item['binary'];package=binary['package'];release=item['release_name']
        folder=release+'-jre'
        entry=folder+('/Contents/Home/bin/java' if os_name=='mac' else '/bin/java.exe' if os_name=='windows' else '/bin/java')
        version=item['version']['semver']
        records[f'java:{platform}']=dict(version=version,url=package['link'],sha256=package['checksum'],archive='zip' if package['name'].endswith('.zip') else 'tar.gz',entrypoint=entry,cli_compat='>=0.1.0, <0.2.0',protocol_version=1)
    (a.output/'metadata-shared.json').write_text(json.dumps(records,indent=2)+'\n')
    print('Packaged JDBC, Skill and vendor driver assets; pinned five official JRE downloads')
if __name__=='__main__':main()
