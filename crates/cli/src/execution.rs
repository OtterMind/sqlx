use crate::{
    components::{platform, Components},
    output, settings,
    storage::Datasource,
};
use anyhow::{bail, Context, Result};
use sqlx_protocol::{Action, Connection, Database, Event, Request, VERSION};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio_util::sync::CancellationToken;

pub struct PreparedExecution {
    binary: PathBuf,
    args: Vec<String>,
    request: Request,
}

/// Wire-compatible engines reuse the native workers.
fn native_worker(kind: Database) -> Option<&'static str> {
    match kind {
        Database::Mysql
        | Database::Mariadb
        | Database::Tidb
        | Database::Greatsql
        | Database::Oceanbase
        | Database::Starrocks
        | Database::Doris => Some("mysql"),
        Database::Postgresql | Database::Cockroachdb | Database::Yugabytedb => Some("postgres"),
        Database::Redis => Some("redis"),
        Database::Mongodb => Some("mongodb"),
        Database::Sqlite => Some("sqlite"),
        Database::Duckdb => Some("duckdb"),
        _ => None,
    }
}
struct JdbcDriver {
    component: &'static str,
    jars: &'static [&'static str],
}
fn jdbc_driver(kind: Database) -> Result<JdbcDriver> {
    Ok(match kind {
        Database::Oracle => JdbcDriver {
            component: "oracle",
            jars: &["ojdbc.jar"],
        },
        Database::Sqlserver => JdbcDriver {
            component: "sqlserver",
            jars: &["mssql-jdbc.jar"],
        },
        Database::Clickhouse => JdbcDriver {
            component: "clickhouse",
            jars: &["clickhouse-jdbc.jar", "slf4j-api.jar", "slf4j-nop.jar"],
        },
        Database::Trino => JdbcDriver {
            component: "trino",
            jars: &["trino-jdbc.jar"],
        },
        Database::Opengauss => JdbcDriver {
            component: "opengauss",
            jars: &["opengauss-jdbc.jar"],
        },
        Database::Dameng => JdbcDriver {
            component: "dameng",
            jars: &["dm-jdbc.jar"],
        },
        Database::Kingbase => JdbcDriver {
            component: "kingbase",
            jars: &["kingbase8-jdbc.jar"],
        },
        Database::Tdengine => JdbcDriver {
            component: "tdengine",
            jars: &["taos-jdbcdriver.jar", "slf4j-nop.jar"],
        },
        Database::H2 => JdbcDriver {
            component: "h2",
            jars: &["h2.jar"],
        },
        other => bail!("{other:?} is not a JDBC database"),
    })
}
pub fn prepare(
    root: PathBuf,
    manifest: String,
    local: Option<PathBuf>,
    source: Datasource,
    action: Action,
    statements: Vec<String>,
) -> Result<PreparedExecution> {
    let kind = source.connection.database_type;
    let mut request = Request {
        protocol_version: VERSION,
        action,
        connection: source.connection,
        statements,
        driver_jars: vec![],
        driver_class: String::new(),
    };
    let mut args = Vec::new();
    let native = native_worker(kind);
    let binary = if let Some(dir) = local {
        if let Some(name) = native {
            dir.join(format!(
                "sqlx-driver-{name}{}",
                std::env::consts::EXE_SUFFIX
            ))
        } else {
            let driver = jdbc_driver(kind)?;
            args.extend([
                "-jar".to_owned(),
                dir.join("sqlx-jdbc.jar").to_string_lossy().into_owned(),
            ]);
            for jar in driver.jars {
                let path = dir
                    .join(jar)
                    .canonicalize()
                    .with_context(|| format!("development JDBC driver {jar} is missing"))?;
                request
                    .driver_jars
                    .push(path.to_string_lossy().into_owned());
            }
            PathBuf::from(std::env::var_os("SQLX_JAVA_BIN").unwrap_or_else(|| "java".into()))
        }
    } else {
        let manager = Components::new(root, manifest);
        let m = manager.manifest(false)?;
        let platform = platform()?;
        if let Some(name) = native {
            manager.ensure(name, &platform, manager.asset(&m, name, &platform)?)?
        } else {
            let driver = jdbc_driver(kind)?;
            let java = manager.ensure("java", &platform, manager.asset(&m, "java", &platform)?)?;
            let runner = manager.ensure("jdbc", "any", manager.asset(&m, "jdbc", "any")?)?;
            let entry = manager.ensure(
                driver.component,
                "any",
                manager.asset(&m, driver.component, "any")?,
            )?;
            args.extend(["-jar".into(), runner.to_string_lossy().into_owned()]);
            // A JDBC component can ship more than the driver itself, such as a logging API.
            let directory = entry.parent().context("JDBC component has no directory")?;
            let mut jars: Vec<PathBuf> = fs::read_dir(directory)?
                .filter_map(|item| item.ok().map(|item| item.path()))
                .filter(|path| path.extension().is_some_and(|extension| extension == "jar"))
                .collect();
            jars.sort();
            if jars.is_empty() {
                bail!("JDBC component {} contains no jar", driver.component);
            }
            for jar in jars {
                request
                    .driver_jars
                    .push(jar.canonicalize()?.to_string_lossy().into_owned());
            }
            java
        }
    };
    request.driver_class = match kind {
        Database::Oracle => "oracle.jdbc.OracleDriver",
        Database::Sqlserver => "com.microsoft.sqlserver.jdbc.SQLServerDriver",
        Database::Clickhouse => "com.clickhouse.jdbc.ClickHouseDriver",
        Database::Trino => "io.trino.jdbc.TrinoDriver",
        Database::Tdengine => "com.taosdata.jdbc.ws.WebSocketDriver",
        Database::Opengauss => "org.opengauss.Driver",
        Database::Dameng => "dm.jdbc.driver.DmDriver",
        Database::Kingbase => "com.kingbase8.Driver",
        Database::H2 => "org.h2.Driver",
        _ => "",
    }
    .into();
    Ok(PreparedExecution {
        binary,
        args,
        request,
    })
}

/// Some drivers only report a closed connection when the server does not serve TLS.
/// Point at the documented fix when nothing was executed and TLS verification is on.
fn connection_failure_hint(
    connection: &Connection,
    index: Option<usize>,
    outcome: &str,
) -> Option<&'static str> {
    (index.is_none() && outcome == "not_started" && connection.tls == "verify-full")
        .then_some("the connection failed before any statement; if this database does not serve TLS, retry with --tls disable")
}
/// Resolve the settings one execution uses, so every command stores and previews the same way.
pub fn output_settings(
    root: &Path,
    mode: output::Mode,
    preview: Option<u64>,
    result_mode: Option<settings::ResultMode>,
) -> Result<(Output, settings::Effective)> {
    let settings = settings::Settings::load(root)?;
    let effective = settings::resolve(root, &settings, preview, result_mode)?;
    let out = Output {
        mode,
        full: effective.result_mode == settings::ResultMode::Full,
        preview_rows: effective.preview_rows,
        results_dir: effective.results_dir.clone(),
        results_owned: matches!(effective.results_dir_source, settings::Source::Default),
    };
    Ok((out, effective))
}
/// Remove expired stored results; called once per execution.
pub fn prune_results(effective: &settings::Effective) -> Result<()> {
    crate::results::prune(
        &effective.results_dir,
        crate::results::CLI_ORIGIN,
        effective.retention,
        crate::results::MAX_STORED_BYTES,
    )?;
    Ok(())
}
/// How one execution prints its result.
pub struct Output {
    pub mode: output::Mode,
    /// Print every row instead of a preview, and store nothing.
    pub full: bool,
    pub preview_rows: u64,
    pub results_dir: PathBuf,
    /// Whether the result directory is the private default rather than a configured one.
    pub results_owned: bool,
}
pub fn run(
    root: PathBuf,
    manifest: String,
    local: Option<PathBuf>,
    source: Datasource,
    action: Action,
    statements: Vec<String>,
    out: Output,
) -> Result<bool> {
    run_to(
        io::stdout().lock(),
        root,
        manifest,
        local,
        source,
        action,
        statements,
        out,
    )
}
/// Run an execution and write its result to `output`.
///
/// The CLI passes stdout; the MCP server passes a buffer so protocol output stays clean. Both
/// modes always leave exactly one valid JSON object behind, including when the worker cannot be
/// started or stops in the middle of a result.
#[allow(clippy::too_many_arguments)]
pub fn run_to<W: Write>(
    output: W,
    root: PathBuf,
    manifest: String,
    local: Option<PathBuf>,
    source: Datasource,
    action: Action,
    statements: Vec<String>,
    out: Output,
) -> Result<bool> {
    let connection = source.connection.clone();
    let writer_options = output::writer_options(
        out.mode,
        out.full,
        out.preview_rows,
        &source.id,
        &source.name,
        statements.clone(),
        &out.results_dir,
        out.results_owned,
        &root,
    );
    let prepared = prepare(root, manifest, local, source, action, statements)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let cancel = CancellationToken::new();
        let signal = cancel.clone();
        let signal_task = tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            signal.cancel();
        });
        let mut writer = output::Writer::new(output, writer_options);
        let result = prepared.execute(|event| writer.event(event), cancel).await;
        signal_task.abort();
        let success = match result {
            Ok(success) => {
                writer.finish(success)?;
                success
            }
            Err(error) => {
                // The worker never started or the connection to it broke: report it inside the
                // printed object instead of leaving a half-written one behind.
                writer.failure(
                    "sqlx.error",
                    &sqlx_protocol::redact(&format!("{error:#}"), &connection),
                )?;
                false
            }
        };
        Ok(success)
    })
}

impl PreparedExecution {
    pub async fn execute<F>(self, mut emit: F, cancel: CancellationToken) -> Result<bool>
    where
        F: FnMut(Event) -> Result<()>,
    {
        let Self {
            binary,
            args,
            request,
        } = self;
        let mut child = tokio::process::Command::new(&binary)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("cannot start database worker")?;
        let mut input = child.stdin.take().unwrap();
        let encoded = serde_json::to_vec(&request)?;
        let input_task = tokio::spawn(async move {
            input.write_all(&encoded).await?;
            input.shutdown().await
        });
        let mut errors = child.stderr.take().unwrap();
        let error_task = tokio::spawn(async move {
            let mut tail = Vec::new();
            let mut buffer = [0; 1024];
            while let Ok(n) = errors.read(&mut buffer).await {
                if n == 0 {
                    break;
                }
                tail.extend_from_slice(&buffer[..n]);
                if tail.len() > 8192 {
                    tail.drain(..tail.len() - 8192);
                }
            }
            tail
        });
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut ready = false;
        let mut connected = false;
        let mut complete = None;
        let mut failed = false;
        let mut next = 0_usize;
        let mut active = None;
        let mut active_result: Option<(usize, usize, u64)> = None;
        let mut next_result = 0;
        let mut stream_error = None;
        loop {
            let line = tokio::select! {
                result=lines.next_line()=>result,
                _=cancel.cancelled()=> {stream_error=Some("execution interrupted; an in-flight write may have completed".to_string());break;}
            };
            let line = match line {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(e) => {
                    stream_error = Some(e.to_string());
                    break;
                }
            };
            let mut event: Event = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => {
                    stream_error = Some("worker emitted an invalid protocol event".into());
                    break;
                }
            };
            let validation: Result<()> = (|| {
                if complete.is_some() {
                    bail!("worker emitted data after completion");
                }
                if !ready && !matches!(event, Event::Ready { .. }) {
                    bail!("worker handshake missing");
                }
                match &mut event {
                    Event::Ready { protocol_version } => {
                        if ready || *protocol_version != VERSION {
                            bail!("incompatible worker handshake");
                        }
                        ready = true;
                    }
                    Event::Connected => {
                        if connected {
                            bail!("duplicate connection event");
                        }
                        connected = true;
                    }
                    Event::StatementStart { index } => {
                        if !connected
                            || active.is_some()
                            || *index != next
                            || *index >= request.statements.len()
                            || failed
                        {
                            bail!("invalid statement sequence");
                        }
                        active = Some(*index);
                        next_result = 0;
                    }
                    Event::Columns {
                        index,
                        result,
                        columns,
                    } => {
                        if active != Some(*index)
                            || active_result.is_some()
                            || *result != next_result
                        {
                            bail!("result does not belong to active statement");
                        }
                        active_result = Some((*result, columns.len(), 0));
                    }
                    Event::Row {
                        index,
                        result,
                        values,
                    } => {
                        let (result_id, width, rows) = active_result
                            .as_mut()
                            .context("row arrived without column metadata")?;
                        if active != Some(*index) || *result_id != *result || *width != values.len()
                        {
                            bail!("invalid result row");
                        }
                        *rows += 1;
                    }
                    Event::ResultEnd {
                        index,
                        result,
                        rows,
                        ..
                    } => {
                        let (result_id, _, received) =
                            active_result.context("result ended without metadata")?;
                        if active != Some(*index)
                            || result_id != *result
                            || rows.parse::<u64>()? != received
                        {
                            bail!("incomplete result stream");
                        }
                        active_result = None;
                        next_result += 1;
                    }
                    Event::StatementEnd { index } => {
                        if active != Some(*index) || active_result.is_some() {
                            bail!("invalid statement completion");
                        }
                        active = None;
                        next += 1;
                    }
                    Event::Skipped { index } => {
                        if !failed || *index != next {
                            bail!("invalid skipped statement sequence");
                        }
                        next += 1;
                    }
                    Event::Error {
                        index,
                        message,
                        outcome,
                        ..
                    } => {
                        failed = true;
                        if let Some(i) = index {
                            if active == Some(*i) {
                                active = None;
                                active_result = None;
                                next = *i + 1;
                            } else if *i >= request.statements.len() {
                                bail!("invalid error statement index");
                            }
                        }
                        *message = sqlx_protocol::redact(message, &request.connection);
                        if let Some(hint) =
                            connection_failure_hint(&request.connection, *index, outcome)
                        {
                            message.push_str(&format!(" ({hint})"));
                        }
                    }
                    Event::Complete { success } => {
                        if *success
                            && (!connected
                                || failed
                                || active.is_some()
                                || next != request.statements.len())
                        {
                            bail!("worker claimed success before all statements completed");
                        }
                        complete = Some(*success);
                    }
                }
                Ok(())
            })();
            if let Err(e) = validation {
                stream_error = Some(e.to_string());
                break;
            }
            emit(event)?;
        }
        if stream_error.is_some() {
            let _ = child.kill().await;
        }
        let status = child.wait().await?;
        let stdin_result = input_task.await?;
        let stderr = error_task.await?;
        let success = stream_error.is_none()
            && complete == Some(true)
            && status.success()
            && stdin_result.is_ok();
        if !success && (stream_error.is_some() || complete.is_none() || complete == Some(true)) {
            let message = stream_error
                .unwrap_or_else(|| "worker exited without a valid successful completion".into());
            emit(Event::Error {
                index: active,
                code: "worker.incomplete".into(),
                message,
                outcome: if active.is_some() {
                    "unknown"
                } else {
                    "incomplete"
                }
                .into(),
            })?;
        }
        if !stderr.is_empty() {
            eprintln!(
                "{}",
                sqlx_protocol::redact(&String::from_utf8_lossy(&stderr), &request.connection)
            );
        }
        Ok(success)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn connection(tls: &str) -> Connection {
        Connection {
            database_type: Database::Oracle,
            host: "127.0.0.1".into(),
            port: 1521,
            database: String::new(),
            service: "FREEPDB1".into(),
            username: String::new(),
            password: String::new(),
            tls: tls.into(),
            properties: std::collections::BTreeMap::new(),
        }
    }
    #[test]
    fn hints_at_tls_disable_only_before_any_statement() {
        let verified = connection("verify-full");
        assert!(connection_failure_hint(&verified, None, "not_started").is_some());
        // A statement that already ran must not suggest changing the transport.
        assert!(connection_failure_hint(&verified, Some(0), "failed").is_none());
        assert!(connection_failure_hint(&verified, None, "unknown").is_none());
        assert!(connection_failure_hint(&connection("disable"), None, "not_started").is_none());
    }
}
