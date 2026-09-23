#!/usr/bin/env python3
"""Package the JDBC runner, licensed JDBC artifacts, Skill and default UI plugin; pin upstream JRE assets."""
import argparse,hashlib,io,json,time,urllib.parse,urllib.request,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
# Components that exist once per platform, in this script's case the pinned JRE.
PLATFORM_KINDS=('java',)
# Components that exist once for all platforms.
SHARED_KINDS=('jdbc','skill','ui-default')
# Licensed vendor drivers, one component per engine the JDBC worker can reach.
VENDORS={
    'oracle':dict(files=[('https://repo.maven.apache.org/maven2/com/oracle/database/jdbc/ojdbc11/23.6.0.24.10/ojdbc11-23.6.0.24.10.jar','ojdbc.jar')],entry='ojdbc.jar',license_url='https://www.oracle.com/downloads/licenses/oracle-free-license.html',license_from_jar='META-INF/license.txt'),
    'sqlserver':dict(files=[('https://repo.maven.apache.org/maven2/com/microsoft/sqlserver/mssql-jdbc/12.10.1.jre11/mssql-jdbc-12.10.1.jre11.jar','mssql-jdbc.jar')],entry='mssql-jdbc.jar',license_url='https://raw.githubusercontent.com/microsoft/mssql-jdbc/v12.10.1/LICENSE'),
    # The all-in-one ClickHouse driver still needs a logging API, so the component ships both.
    'clickhouse':dict(files=[('https://repo.maven.apache.org/maven2/com/clickhouse/clickhouse-jdbc/0.9.0/clickhouse-jdbc-0.9.0-all.jar','clickhouse-jdbc.jar'),
                             ('https://repo.maven.apache.org/maven2/org/slf4j/slf4j-api/2.0.16/slf4j-api-2.0.16.jar','slf4j-api.jar'),
                             ('https://repo.maven.apache.org/maven2/org/slf4j/slf4j-nop/2.0.16/slf4j-nop-2.0.16.jar','slf4j-nop.jar')],
                      entry='clickhouse-jdbc.jar',license_url='https://raw.githubusercontent.com/ClickHouse/clickhouse-java/main/LICENSE',
                      extra_licenses={'LICENSE-slf4j.txt':('slf4j-api.jar','META-INF/LICENSE.txt')}),
    'trino':dict(files=[('https://repo.maven.apache.org/maven2/io/trino/trino-jdbc/476/trino-jdbc-476.jar','trino-jdbc.jar')],entry='trino-jdbc.jar',license_url='https://raw.githubusercontent.com/trinodb/trino/master/LICENSE'),
    # H2 is an embedded Java database that also serves TCP; the driver comes from Maven Central and
    # is dual licensed under the MPL 2.0 or the EPL 1.0.
    'h2':dict(files=[('https://repo.maven.apache.org/maven2/com/h2database/h2/2.5.250/h2-2.5.250.jar','h2.jar')],entry='h2.jar',license_url='https://raw.githubusercontent.com/h2database/h2database/master/LICENSE.txt'),
    # Dameng and KingbaseES publish their JDBC drivers on Maven Central under the Apache license.
    'dameng':dict(files=[('https://repo.maven.apache.org/maven2/com/dameng/DmJdbcDriver18/8.1.3.140/DmJdbcDriver18-8.1.3.140.jar','dm-jdbc.jar')],entry='dm-jdbc.jar',license_url='https://repo1.maven.org/maven2/com/dameng/DmJdbcDriver18/8.1.3.140/DmJdbcDriver18-8.1.3.140.pom'),
    'kingbase':dict(files=[('https://repo.maven.apache.org/maven2/cn/com/kingbase/kingbase8/9.0.1.jre7/kingbase8-9.0.1.jre7.jar','kingbase8-jdbc.jar')],entry='kingbase8-jdbc.jar',license_url='https://repo1.maven.org/maven2/cn/com/kingbase/kingbase8/9.0.1.jre7/kingbase8-9.0.1.jre7.pom'),
    'opengauss':dict(files=[('https://repo.maven.apache.org/maven2/org/opengauss/opengauss-jdbc/6.0.0-b041-og/opengauss-jdbc-6.0.0-b041-og.jar','opengauss-jdbc.jar')],entry='opengauss-jdbc.jar',license_url='https://raw.githubusercontent.com/opengauss-mirror/openGauss-connector-jdbc/master/LICENSE'),
    # The TDengine RESTful driver ships as one bundled jar (its own dependencies included) and
    # needs an slf4j binding, because the bundle carries the slf4j API without a provider.
    'tdengine':dict(files=[('https://repo.maven.apache.org/maven2/com/taosdata/jdbc/taos-jdbcdriver/3.6.3/taos-jdbcdriver-3.6.3-dist.jar','taos-jdbcdriver.jar'),
                           ('https://repo.maven.apache.org/maven2/org/slf4j/slf4j-nop/2.0.16/slf4j-nop-2.0.16.jar','slf4j-nop.jar')],
                    entry='taos-jdbcdriver.jar',license_url='https://raw.githubusercontent.com/taosdata/taos-connector-jdbc/main/LICENSE',
                    extra_licenses={'LICENSE-bundled.txt':('taos-jdbcdriver.jar','META-INF/LICENSE'),
                                    'NOTICE-bundled.txt':('taos-jdbcdriver.jar','META-INF/NOTICE'),
                                    'LICENSE-slf4j.txt':('slf4j-nop.jar','META-INF/LICENSE.txt')}),
}
VENDOR_KINDS=tuple(VENDORS)
def get(url):
    for attempt in range(3):
        try:
            with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'Mozilla/5.0 OtterMind-SQLX-release'}),timeout=30) as r:return r.read()
        except (OSError, urllib.error.URLError):
            if attempt==2:raise
            time.sleep(attempt+1)
def main():
    p=argparse.ArgumentParser();p.add_argument('--version',default='0.1.16');p.add_argument('--output',type=Path,default=ROOT/'dist');a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
    records={};base=f'https://github.com/OtterMind/sqlx/releases/download/v{a.version}/'
    def add(name,entries,entrypoint,version=None):
        # Every component must be declared above, so the release manifest gate stays in sync
        # with what this script actually packages.
        if name not in SHARED_KINDS and name not in VENDOR_KINDS:
            raise ValueError(f'{name} is not listed in SHARED_KINDS or VENDORS')
        component_version=version or a.version
        path=a.output/f'{name}-{component_version}.zip'
        with zipfile.ZipFile(path,'w',zipfile.ZIP_DEFLATED) as z:
            for filename,body in entries.items():z.writestr(filename,body)
        # The Skill ships with the CLI, so it requires the same release; the default UI plugin
        # keeps the CLI range of UI API version 1, which starts at 0.1.4.
        compat={'skill':f'>={a.version}, <0.2.0','ui-default':'>=0.1.4, <0.2.0'}.get(name,'>=0.1.1, <0.2.0')
        records[f'{name}:any']=dict(version=component_version,url=base+path.name,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),archive='zip',entrypoint=entrypoint,cli_compat=compat,protocol_version=1)
    add('jdbc',{'sqlx-jdbc.jar':(ROOT/f'java/jdbc/target/sqlx-jdbc-{a.version}.jar').read_bytes(),'LICENSE':(ROOT/'LICENSE').read_bytes(),'NOTICE':(ROOT/'NOTICE').read_bytes()},'sqlx-jdbc.jar')
    skill=ROOT/'skills/sqlx';add('skill',{str(f.relative_to(skill)).replace('\\','/'):f.read_bytes() for f in skill.rglob('*') if f.is_file()},'SKILL.md')
    plugin=ROOT/'ui/dist';add('ui-default',{str(f.relative_to(plugin)).replace('\\','/'):f.read_bytes() for f in plugin.rglob('*') if f.is_file()},'ui-plugin.json',version=json.loads((plugin/'ui-plugin.json').read_text())['version'])
    for name,spec in VENDORS.items():
        entries={};sources=[]
        for url,filename in spec['files']:
            entries[filename]=get(url);sources.append(url)
        if 'license_from_jar' in spec:
            # Preserve the license shipped with this exact driver; the HTML page blocks automated downloads.
            with zipfile.ZipFile(io.BytesIO(entries[spec['entry']])) as jar:license_text=jar.read(spec['license_from_jar'])
        else:license_text=get(spec['license_url'])
        entries['LICENSE.txt']=license_text
        for target,(archive,member) in spec.get('extra_licenses',{}).items():
            with zipfile.ZipFile(io.BytesIO(entries[archive])) as jar:entries[target]=jar.read(member)
        entries['SOURCE.txt']=('\n'.join(sources+[spec['license_url']])+'\n').encode()
        add(name,entries,spec['entry'])
    for platform,os_name,arch in [('macos-arm64','mac','aarch64'),('macos-x64','mac','x64'),('windows-x64','windows','x64'),('linux-arm64','linux','aarch64'),('linux-x64','linux','x64')]:
        params=urllib.parse.urlencode(dict(architecture=arch,image_type='jre',os=os_name,vendor='eclipse'))
        releases=json.loads(get('https://api.adoptium.net/v3/assets/latest/17/hotspot?'+params))
        item=releases[0];binary=item['binary'];package=binary['package'];release=item['release_name']
        folder=release+'-jre'
        entry=folder+('/Contents/Home/bin/java' if os_name=='mac' else '/bin/java.exe' if os_name=='windows' else '/bin/java')
        version=item['version']['semver']
        records[f'java:{platform}']=dict(version=version,url=package['link'],sha256=package['checksum'],archive='zip' if package['name'].endswith('.zip') else 'tar.gz',entrypoint=entry,cli_compat='>=0.1.1, <0.2.0',protocol_version=1)
    (a.output/'metadata-shared.json').write_text(json.dumps(records,indent=2)+'\n')
    print('Packaged JDBC, Skill, default UI plugin and vendor driver assets; pinned five official JRE downloads')
if __name__=='__main__':main()
