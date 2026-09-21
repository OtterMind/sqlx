#!/usr/bin/env bash
set -euo pipefail
kind="${1:?oracle, sqlserver, clickhouse or trino required}"
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
  # ClickHouse and Trino run from tests/compose.yaml, so these cases only stage their drivers.
  clickhouse)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/clickhouse/clickhouse-jdbc/0.9.0/clickhouse-jdbc-0.9.0-all.jar' -o target/debug/clickhouse-jdbc.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/slf4j/slf4j-api/2.0.16/slf4j-api-2.0.16.jar' -o target/debug/slf4j-api.jar
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/org/slf4j/slf4j-nop/2.0.16/slf4j-nop-2.0.16.jar' -o target/debug/slf4j-nop.jar
    ;;
  trino)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/io/trino/trino-jdbc/476/trino-jdbc-476.jar' -o target/debug/trino-jdbc.jar
    ;;
  *) echo 'Unsupported fixture' >&2; exit 1 ;;
esac
