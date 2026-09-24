#!/usr/bin/env bash
set -euo pipefail
kind="${1:?oracle, sqlserver, clickhouse, trino, h2 or another JDBC engine required}"
mkdir -p target/debug
# The runner version follows Cargo.toml; these fixtures only stage the jars the CLI loads in dev mode.
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
cp "java/jdbc/target/sqlx-jdbc-$version.jar" target/debug/sqlx-jdbc.jar
case "$kind" in
  oracle)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/oracle/database/jdbc/ojdbc11/23.6.0.24.10/ojdbc11-23.6.0.24.10.jar' -o target/debug/ojdbc.jar
    docker run -d --name sqlx-jdbc-oracle -e ORACLE_PASSWORD=SQLX_Test_Only_12345 -p 127.0.0.1:21521:1521 gvenzl/oracle-free:23-slim
    ;;
  sqlserver)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/microsoft/sqlserver/mssql-jdbc/12.10.1.jre11/mssql-jdbc-12.10.1.jre11.jar' -o target/debug/mssql-jdbc.jar
    docker run -d --name sqlx-jdbc-sqlserver -e ACCEPT_EULA=Y -e MSSQL_SA_PASSWORD=SQLX_Test_Only_12345 -p 127.0.0.1:21433:1433 mcr.microsoft.com/mssql/server:2022-latest
    ;;
  # ClickHouse, Trino and TDengine run from tests/compose.yaml, so these cases only stage their drivers.
  clickhouse)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/clickhouse/clickhouse-jdbc/0.9.0/clickhouse-jdbc-0.9.0-all.jar' -o target/debug/clickhouse-jdbc.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/slf4j/slf4j-api/2.0.16/slf4j-api-2.0.16.jar' -o target/debug/slf4j-api.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/slf4j/slf4j-nop/2.0.16/slf4j-nop-2.0.16.jar' -o target/debug/slf4j-nop.jar
    ;;
  trino)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/io/trino/trino-jdbc/476/trino-jdbc-476.jar' -o target/debug/trino-jdbc.jar
    ;;
  h2)
    # H2 is embedded: the jar is all this fixture needs, and the tests open a local file.
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/h2database/h2/2.5.250/h2-2.5.250.jar' -o target/debug/h2.jar
    ;;
  dameng)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/dameng/DmJdbcDriver18/8.1.3.140/DmJdbcDriver18-8.1.3.140.jar' -o target/debug/dm-jdbc.jar
    ;;
  kingbase)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/cn/com/kingbase/kingbase8/9.0.1.jre7/kingbase8-9.0.1.jre7.jar' -o target/debug/kingbase8-jdbc.jar
    ;;
  opengauss)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/opengauss/opengauss-jdbc/6.0.0-b041-og/opengauss-jdbc-6.0.0-b041-og.jar' -o target/debug/opengauss-jdbc.jar
    ;;
  tdengine)
    # The bundled jar carries the driver's own dependencies; the bundle has no slf4j binding.
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/taosdata/jdbc/taos-jdbcdriver/3.6.3/taos-jdbcdriver-3.6.3-dist.jar' -o target/debug/taos-jdbcdriver.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/slf4j/slf4j-nop/2.0.16/slf4j-nop-2.0.16.jar' -o target/debug/slf4j-nop.jar
    ;;
  # Presto, Hive, Kylin and XuguDB run from tests/compose.yaml, so these cases stage their drivers.
  presto)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/facebook/presto/presto-jdbc/0.293/presto-jdbc-0.293.jar' -o target/debug/presto-jdbc.jar
    ;;
  hive)
    # The standalone jar is the only self-contained Hive driver; its slf4j 1.7 API needs a binding.
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/apache/hive/hive-jdbc/4.0.1/hive-jdbc-4.0.1-standalone.jar' -o target/debug/hive-jdbc.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/slf4j/slf4j-nop/1.7.36/slf4j-nop-1.7.36.jar' -o target/debug/slf4j-nop.jar
    ;;
  kylin)
    # The driver needs JAXB, which the JDK dropped in Java 11, and an slf4j 1.7 binding.
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/apache/kylin/kylin-jdbc/5.0.3/kylin-jdbc-5.0.3.jar' -o target/debug/kylin-jdbc.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/jakarta/xml/bind/jakarta.xml.bind-api/2.3.3/jakarta.xml.bind-api-2.3.3.jar' -o target/debug/jakarta.xml.bind-api.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/glassfish/jaxb/jaxb-runtime/2.3.9/jaxb-runtime-2.3.9.jar' -o target/debug/jaxb-runtime.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/sun/istack/istack-commons-runtime/4.1.2/istack-commons-runtime-4.1.2.jar' -o target/debug/istack-commons-runtime.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/jakarta/activation/jakarta.activation-api/1.2.2/jakarta.activation-api-1.2.2.jar' -o target/debug/jakarta.activation-api.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/glassfish/jaxb/txw2/2.3.9/txw2-2.3.9.jar' -o target/debug/txw2.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/slf4j/slf4j-nop/1.7.36/slf4j-nop-1.7.36.jar' -o target/debug/slf4j-nop.jar
    ;;
  xugu)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/xugudb/xugu-jdbc/12.3.4/xugu-jdbc-12.3.4.jar' -o target/debug/xugu-jdbc.jar
    ;;
  # The vendors below do not allow redistribution, so their driver has to come from the vendor.
  # Db2 and Informix publish theirs on Maven Central, which is enough for a local fixture.
  db2)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/ibm/db2/jcc/12.1.0.0/jcc-12.1.0.0.jar' -o target/debug/db2-jcc.jar
    ;;
  informix)
    # The 15.x driver fails inside its own ASF layer against an Informix 14.10 server (a null
    # pointer, surfaced as "An unexpected error occurred"); the 4.50 line connects.
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/ibm/informix/jdbc/4.50.14/jdbc-4.50.14.jar' -o target/debug/informix-jdbc.jar
    ;;
  sundb)
    # SUNDB runs the Goldilocks engine; the driver lives in the vendor image.
    docker cp "$(docker create sundb/sundb_standlone:01):/goldilocks_home/lib/goldilocks8.jar" target/debug/goldilocks8.jar
    ;;
  gbase8s)
    # GBase 8s is commercial, so its driver comes from an installed instance rather than a registry.
    # The vendor ships a wrapper jar whose inner ifxjdbc.jar is the file the JVM can load.
    : "${SQLX_TEST_GBASE8S_DRIVER:?set SQLX_TEST_GBASE8S_DRIVER to the vendor jdbc jar or wrapper jar}"
    cp "$SQLX_TEST_GBASE8S_DRIVER" target/debug/gbasedbt-provided.jar
    if unzip -l target/debug/gbasedbt-provided.jar ifxjdbc.jar >/dev/null 2>&1; then
      (cd target/debug && unzip -o -q gbasedbt-provided.jar ifxjdbc.jar && mv ifxjdbc.jar gbasedbt-jdbc.jar)
    else
      mv target/debug/gbasedbt-provided.jar target/debug/gbasedbt-jdbc.jar
    fi
    ;;
  *) echo 'Unsupported fixture' >&2; exit 1 ;;
esac
