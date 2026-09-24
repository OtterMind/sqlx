//! Versioned newline-delimited worker protocol. Credentials are only sent on stdin.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{self, Write},
};

pub const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Database {
    Mysql,
    Mariadb,
    Tidb,
    Greatsql,
    Oceanbase,
    Postgresql,
    Cockroachdb,
    Yugabytedb,
    Opengauss,
    Oracle,
    Sqlserver,
    Clickhouse,
    Trino,
    Starrocks,
    Doris,
    Tdengine,
    Dameng,
    Kingbase,
    Redis,
    Mongodb,
    Sqlite,
    Duckdb,
    H2,
    Presto,
    Hive,
    Kylin,
    Xugu,
    Db2,
    Informix,
    Sundb,
    Gbase8s,
}

impl Database {
    /// Engines that open a local file instead of a network endpoint.
    pub fn is_file_based(self) -> bool {
        matches!(self, Database::Sqlite | Database::Duckdb)
    }

    /// Every engine, in the order the documentation lists them.
    pub const ALL: [Database; 31] = [
        Database::Mysql,
        Database::Mariadb,
        Database::Tidb,
        Database::Greatsql,
        Database::Oceanbase,
        Database::Postgresql,
        Database::Cockroachdb,
        Database::Yugabytedb,
        Database::Opengauss,
        Database::Oracle,
        Database::Sqlserver,
        Database::Clickhouse,
        Database::Trino,
        Database::Starrocks,
        Database::Doris,
        Database::Tdengine,
        Database::Dameng,
        Database::Kingbase,
        Database::Redis,
        Database::Mongodb,
        Database::Sqlite,
        Database::Duckdb,
        Database::H2,
        Database::Presto,
        Database::Hive,
        Database::Kylin,
        Database::Xugu,
        Database::Db2,
        Database::Informix,
        Database::Sundb,
        Database::Gbase8s,
    ];

    /// The name every other component uses for this engine: `--type`, driver components and
    /// prefetch arguments all spell it the same way.
    pub fn name(self) -> &'static str {
        match self {
            Database::Mysql => "mysql",
            Database::Mariadb => "mariadb",
            Database::Tidb => "tidb",
            Database::Greatsql => "greatsql",
            Database::Oceanbase => "oceanbase",
            Database::Postgresql => "postgresql",
            Database::Cockroachdb => "cockroachdb",
            Database::Yugabytedb => "yugabytedb",
            Database::Opengauss => "opengauss",
            Database::Oracle => "oracle",
            Database::Sqlserver => "sqlserver",
            Database::Clickhouse => "clickhouse",
            Database::Trino => "trino",
            Database::Starrocks => "starrocks",
            Database::Doris => "doris",
            Database::Tdengine => "tdengine",
            Database::Dameng => "dameng",
            Database::Kingbase => "kingbase",
            Database::Redis => "redis",
            Database::Mongodb => "mongodb",
            Database::Sqlite => "sqlite",
            Database::Duckdb => "duckdb",
            Database::H2 => "h2",
            Database::Presto => "presto",
            Database::Hive => "hive",
            Database::Kylin => "kylin",
            Database::Xugu => "xugu",
            Database::Db2 => "db2",
            Database::Informix => "informix",
            Database::Sundb => "sundb",
            Database::Gbase8s => "gbase8s",
        }
    }
}

impl std::str::FromStr for Database {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mysql" => Ok(Self::Mysql),
            "mariadb" => Ok(Self::Mariadb),
            "tidb" => Ok(Self::Tidb),
            "greatsql" => Ok(Self::Greatsql),
            "oceanbase" | "ob" => Ok(Self::Oceanbase),
            "postgresql" | "postgres" | "pgsql" => Ok(Self::Postgresql),
            "cockroachdb" | "cockroach" | "crdb" => Ok(Self::Cockroachdb),
            "yugabytedb" | "yugabyte" | "yb" => Ok(Self::Yugabytedb),
            "opengauss" | "gaussdb" => Ok(Self::Opengauss),
            "oracle" => Ok(Self::Oracle),
            "sqlserver" | "mssql" => Ok(Self::Sqlserver),
            "clickhouse" => Ok(Self::Clickhouse),
            "trino" => Ok(Self::Trino),
            "starrocks" => Ok(Self::Starrocks),
            "doris" => Ok(Self::Doris),
            "tdengine" | "taos" => Ok(Self::Tdengine),
            "dameng" | "dm" => Ok(Self::Dameng),
            "kingbase" | "kingbasees" => Ok(Self::Kingbase),
            "redis" => Ok(Self::Redis),
            "mongodb" | "mongo" => Ok(Self::Mongodb),
            "sqlite" | "sqlite3" => Ok(Self::Sqlite),
            "duckdb" => Ok(Self::Duckdb),
            "h2" => Ok(Self::H2),
            "presto" | "prestodb" => Ok(Self::Presto),
            "hive" => Ok(Self::Hive),
            "kylin" => Ok(Self::Kylin),
            "xugu" | "xugudb" => Ok(Self::Xugu),
            "db2" | "ibmdb2" => Ok(Self::Db2),
            "informix" | "ifx" => Ok(Self::Informix),
            "sundb" => Ok(Self::Sundb),
            "gbase8s" | "gbasedbt" => Ok(Self::Gbase8s),
            _ => Err(
                "expected mysql, mariadb, tidb, greatsql, oceanbase, postgresql, cockroachdb, yugabytedb, opengauss, oracle, sqlserver, clickhouse, trino, starrocks, doris, tdengine, dameng, kingbase, redis, mongodb, sqlite, duckdb, h2, presto, hive, kylin, xugu, db2, informix, sundb, or gbase8s"
                    .into(),
            ),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Connection {
    pub database_type: Database,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub service: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_tls")]
    pub tls: String,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}
fn default_tls() -> String {
    "verify-full".into()
}
impl Connection {
    /// A connection that opens a local file: a file-based engine, or embedded H2 without a host.
    pub fn is_local(&self) -> bool {
        self.database_type.is_file_based()
            || (self.database_type == Database::H2 && self.host.trim().is_empty())
    }
    pub fn validate(&self) -> Result<()> {
        // A local engine opens a file, so it carries a path instead of a host and port.
        if self.is_local() {
            if self.database.trim().is_empty() {
                bail!("{:?} requires the database file", self.database_type);
            }
            if self.database_type.is_file_based()
                && (!self.host.trim().is_empty() || self.port != 0)
            {
                bail!(
                    "{:?} opens a local file and takes no host or port",
                    self.database_type
                );
            }
        } else if self.host.trim().is_empty() || self.port == 0 {
            bail!("host and nonzero port are required");
        }
        if !matches!(self.tls.as_str(), "disable" | "verify-full") {
            bail!("tls must be disable or verify-full");
        }
        if !self.database_type.is_file_based()
            && self.host.contains([';', '/', '\\', '\n', '\r', '\0'])
        {
            bail!("invalid host");
        }
        if self.database_type == Database::Oracle && self.service.trim().is_empty() {
            bail!("Oracle requires --service");
        }
        if self.database_type == Database::Trino {
            // Trino addresses a catalog and an optional schema: --database catalog[.schema].
            let parts: Vec<&str> = self.database.split('.').collect();
            if parts.is_empty()
                || parts.len() > 2
                || parts.iter().any(|part| part.trim().is_empty())
            {
                bail!("Trino requires --database <catalog> or <catalog>.<schema>");
            }
        }
        if self.properties.keys().any(|k| {
            ["user", "username", "password", "passwd"].contains(&k.to_ascii_lowercase().as_str())
        }) {
            bail!("credentials must use the dedicated credential fields");
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub protocol_version: u32,
    pub action: Action,
    pub connection: Connection,
    #[serde(default)]
    pub statements: Vec<String>,
    #[serde(default)]
    pub driver_jars: Vec<String>,
    #[serde(default)]
    pub driver_class: String,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Test,
    Execute,
}
impl Request {
    pub fn read() -> Result<Self> {
        let request: Self = serde_json::from_reader(io::stdin().lock())?;
        if request.protocol_version != VERSION {
            bail!("unsupported protocol version");
        }
        request.connection.validate()?;
        if request.action == Action::Execute
            && (request.statements.is_empty()
                || request.statements.iter().any(|s| s.trim().is_empty()))
        {
            bail!("at least one nonempty SQL statement is required");
        }
        Ok(request)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub database_type: String,
    pub encoding: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Ready {
        protocol_version: u32,
    },
    Connected,
    StatementStart {
        index: usize,
    },
    Columns {
        index: usize,
        result: usize,
        columns: Vec<Column>,
    },
    Row {
        index: usize,
        result: usize,
        values: Vec<Value>,
    },
    ResultEnd {
        index: usize,
        result: usize,
        rows: String,
        affected_rows: Option<String>,
    },
    StatementEnd {
        index: usize,
    },
    Error {
        index: Option<usize>,
        code: String,
        message: String,
        outcome: String,
    },
    Skipped {
        index: usize,
    },
    Complete {
        success: bool,
    },
}

pub struct Emitter<W: Write> {
    writer: W,
}
impl<W: Write> Emitter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
    pub fn send(&mut self, event: Event) -> Result<()> {
        serde_json::to_writer(&mut self.writer, &event)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(())
    }
    pub fn failed(
        &mut self,
        request: &Request,
        index: Option<usize>,
        code: &str,
        message: &str,
        outcome: &str,
    ) -> Result<()> {
        self.send(Event::Error {
            index,
            code: code.into(),
            message: redact(message, &request.connection),
            outcome: outcome.into(),
        })?;
        let skipped_from = index.map_or(0, |i| i + 1);
        for i in skipped_from..request.statements.len() {
            self.send(Event::Skipped { index: i })?;
        }
        self.send(Event::Complete { success: false })
    }
}
pub fn redact(message: &str, connection: &Connection) -> String {
    let mut text = message.to_owned();
    for secret in [&connection.password, &connection.username] {
        if !secret.is_empty() {
            text = redact_secret(&text, secret);
            // URLs carry credentials percent-encoded, so a driver can echo that form instead.
            let encoded = percent_encode(secret);
            if encoded != *secret {
                text = redact_secret(&text, &encoded);
            }
        }
    }
    text
}

/// Percent-encode everything outside the unreserved set, the way a connection URL does.
fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}
/// Replace standalone occurrences of a secret. A secret that is only part of a word or a
/// hostname stays: the password `oracle` must not turn `docs.oracle.com` into
/// `docs.[redacted].com`, and the username `app` must not mangle `application`.
fn redact_secret(text: &str, secret: &str) -> String {
    let boundary = |c: char| !(c.is_alphanumeric() || c == '.');
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while let Some(offset) = text[index..].find(secret) {
        let start = index + offset;
        let end = start + secret.len();
        let standalone = text[..start].chars().next_back().is_none_or(boundary)
            && text[end..].chars().next().is_none_or(boundary);
        out.push_str(&text[index..start]);
        out.push_str(if standalone { "[redacted]" } else { secret });
        index = end;
    }
    out.push_str(&text[index..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn connection(username: &str, password: &str) -> Connection {
        Connection {
            database_type: Database::Oracle,
            host: "127.0.0.1".into(),
            port: 1521,
            database: String::new(),
            service: "FREEPDB1".into(),
            username: username.into(),
            password: password.into(),
            tls: "verify-full".into(),
            properties: std::collections::BTreeMap::new(),
        }
    }
    #[test]
    fn every_engine_name_round_trips() {
        let mut names = std::collections::BTreeSet::new();
        for kind in Database::ALL {
            let name = kind.name();
            assert!(names.insert(name), "{name} is declared twice");
            assert_eq!(
                name.parse::<Database>(),
                Ok(kind),
                "{name} does not parse back"
            );
            let serialized = serde_json::to_value(kind).expect("an engine serializes");
            assert_eq!(serialized, serde_json::Value::String(name.into()));
        }
        assert_eq!(names.len(), Database::ALL.len());
    }

    #[test]
    fn redaction_keeps_hostnames_and_words() {
        let connection = connection("app", "oracle");
        assert_eq!(
            redact(
                "https://docs.oracle.com/error-help/db/ora-17002/",
                &connection
            ),
            "https://docs.oracle.com/error-help/db/ora-17002/"
        );
        assert_eq!(
            redact("the application could not start", &connection),
            "the application could not start"
        );
    }
    #[test]
    fn redaction_covers_percent_encoded_credentials() {
        let connection = connection("app", "p@ss word");
        assert_eq!(
            redact(
                "invalid URI: redis://app:p%40ss%20word@localhost:6379",
                &connection
            ),
            "invalid URI: redis://[redacted]:[redacted]@localhost:6379"
        );
    }
    #[test]
    fn redaction_still_covers_credentials() {
        let connection = connection("app", "s3cret");
        assert_eq!(
            redact(
                "password authentication failed for user \"app\" with s3cret",
                &connection
            ),
            "password authentication failed for user \"[redacted]\" with [redacted]"
        );
        assert_eq!(
            redact("jdbc:mysql://app:s3cret@db:3306/app", &connection),
            "jdbc:mysql://[redacted]:[redacted]@db:3306/[redacted]"
        );
    }
}
