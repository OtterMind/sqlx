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
    Postgresql,
    Oracle,
    Sqlserver,
}

impl std::str::FromStr for Database {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mysql" => Ok(Self::Mysql),
            "postgresql" | "postgres" | "pgsql" => Ok(Self::Postgresql),
            "oracle" => Ok(Self::Oracle),
            "sqlserver" | "mssql" => Ok(Self::Sqlserver),
            _ => Err("expected mysql, postgresql, oracle, or sqlserver".into()),
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
    pub fn validate(&self) -> Result<()> {
        if self.host.trim().is_empty() || self.port == 0 {
            bail!("host and nonzero port are required");
        }
        if !matches!(self.tls.as_str(), "disable" | "verify-full") {
            bail!("tls must be disable or verify-full");
        }
        if self.host.contains([';', '/', '\\', '\n', '\r', '\0']) {
            bail!("invalid host");
        }
        if self.database_type == Database::Oracle && self.service.trim().is_empty() {
            bail!("Oracle requires --service");
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

#[derive(Debug, Serialize, Deserialize)]
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
        }
    }
    text
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
