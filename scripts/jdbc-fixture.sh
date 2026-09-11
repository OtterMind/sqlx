#!/usr/bin/env bash
set -euo pipefail
kind="${1:?oracle or sqlserver required}"
mkdir -p target/debug
cp java/jdbc/target/sqlx-jdbc-0.1.3.jar target/debug/sqlx-jdbc.jar
case "$kind" in
  oracle)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/oracle/database/jdbc/ojdbc11/23.6.0.24.10/ojdbc11-23.6.0.24.10.jar' -o target/debug/ojdbc.jar
    docker run -d --name sqlx-jdbc-oracle -e ORACLE_PASSWORD=SQLX_Test_Only_12345 -p 127.0.0.1:21521:1521 gvenzl/oracle-free:23-slim
    ;;
  sqlserver)
    curl --fail --location --retry 3 'https://repo.maven.apache.org/maven2/com/microsoft/sqlserver/mssql-jdbc/12.10.1.jre11/mssql-jdbc-12.10.1.jre11.jar' -o target/debug/mssql-jdbc.jar
    docker run -d --name sqlx-jdbc-sqlserver -e ACCEPT_EULA=Y -e MSSQL_SA_PASSWORD=SQLX_Test_Only_12345 -p 127.0.0.1:21433:1433 mcr.microsoft.com/mssql/server:2022-latest
    ;;
  *) echo 'Unsupported fixture' >&2; exit 1 ;;
esac
