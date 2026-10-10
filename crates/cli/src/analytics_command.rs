//! `sqlx analytics` — create and run the charts and dashboards the local UI renders.
//!
//! A chart owns its SQL, the datasource it runs against and the snapshot it shows. The browser
//! reaches the same code over HTTP because a page cannot call a binary, but an agent must never
//! need the UI service to publish a chart.
use crate::analytics::{
    Analytics, Axis, Catalog, Chart, ChartSpec, ChartType, Dashboard, Metric, Placement,
};
use crate::{execution, results::ResultStore, storage::Store};
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

#[derive(Subcommand)]
pub enum AnalyticsCommand {
    /// List saved charts and dashboards.
    List,
    /// Create a chart: it carries its own read-only SQL.
    Chart {
        #[command(subcommand)]
        command: ChartCommand,
    },
    /// Create a dashboard.
    Dashboard {
        #[command(subcommand)]
        command: DashboardCommand,
    },
    /// Delete a chart or a dashboard by name or id.
    Remove { kind: String, value: String },
}
#[derive(Subcommand)]
pub enum ChartCommand {
    /// Save a chart; --run executes its SQL first, so the chart has data to draw.
    Add {
        #[arg(long)]
        name: String,
        /// Datasource name or id.
        #[arg(long)]
        datasource: String,
        /// One complete SELECT; repeat for several statements.
        #[arg(
            long = "sql",
            alias = "command",
            required = true,
            allow_hyphen_values = true
        )]
        statements: Vec<String>,
        /// column, bar, line, arealine, scatter, pie, ringpie, rosepie, funnel, wordcloud, statistics or table.
        #[arg(long = "type", default_value = "column")]
        kind: String,
        /// Column plotted along the X axis; omit for statistic cards and tables.
        #[arg(long)]
        x: Option<String>,
        /// Numeric column plotted on the Y axis; repeat for several series.
        #[arg(long = "y")]
        metrics: Vec<String>,
        /// Dashboard name or id; the new chart is placed on it.
        #[arg(long)]
        dashboard: Option<String>,
        /// Run the SQL now and publish the snapshot the chart renders.
        #[arg(long)]
        run: bool,
    },
    /// Run a saved chart's SQL and publish a fresh snapshot.
    Run {
        /// Chart name or id.
        chart: String,
    },
}
#[derive(Subcommand)]
pub enum DashboardCommand {
    /// Create an empty dashboard.
    Add {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "")]
        description: String,
    },
}

pub fn command(
    root: &Path,
    manifest: String,
    worker_dir: Option<PathBuf>,
    command: AnalyticsCommand,
) -> Result<Value> {
    let analytics = Analytics::new(root)?;
    match command {
        AnalyticsCommand::List => {
            let catalog = analytics.catalog()?;
            Ok(json!({
                "charts": catalog.charts.iter().map(chart_json).collect::<Vec<_>>(),
                "dashboards": catalog.dashboards.iter().map(dashboard_json).collect::<Vec<_>>(),
            }))
        }
        AnalyticsCommand::Chart { command } => match command {
            ChartCommand::Add {
                name,
                datasource,
                statements,
                kind,
                x,
                metrics,
                dashboard,
                run,
            } => {
                if statements
                    .iter()
                    .any(|statement| statement.trim().is_empty())
                {
                    bail!("SQL statements must not be empty");
                }
                let source = Store::open(root.to_path_buf())?.find(&datasource)?;
                let mut chart = analytics.save_chart(Chart {
                    id: uuid::Uuid::new_v4().to_string(),
                    name,
                    datasource_id: source.id,
                    statements,
                    statement: 0,
                    result: 0,
                    snapshot_id: None,
                    history: vec![],
                    spec: ChartSpec {
                        chart_type: chart_type(&kind)?,
                        dimension: x,
                        group_by: vec![],
                        metrics: metrics
                            .into_iter()
                            .map(|field| Metric {
                                field,
                                label: String::new(),
                                unit: String::new(),
                                axis: Axis::Left,
                                kind: None,
                            })
                            .collect(),
                        stack: false,
                        line_style: Default::default(),
                    },
                    revision: 0,
                })?;
                if run {
                    chart = run_chart(root, &manifest, worker_dir.as_deref(), &analytics, &chart)?;
                }
                let mut value = chart_json(&chart);
                if let Some(board) = dashboard {
                    let board = find_dashboard(&analytics.catalog()?, &board)?;
                    let placed = place(&analytics, &board, &chart)?;
                    value["dashboard"] = json!({"id": placed.id, "name": placed.name});
                }
                Ok(value)
            }
            ChartCommand::Run { chart } => {
                let found = find_chart(&analytics.catalog()?, &chart)?;
                Ok(chart_json(&run_chart(
                    root,
                    &manifest,
                    worker_dir.as_deref(),
                    &analytics,
                    &found,
                )?))
            }
        },
        AnalyticsCommand::Dashboard { command } => match command {
            DashboardCommand::Add { name, description } => {
                Ok(dashboard_json(&analytics.save_dashboard(Dashboard {
                    id: uuid::Uuid::new_v4().to_string(),
                    name,
                    description,
                    charts: vec![],
                    revision: 0,
                })?))
            }
        },
        AnalyticsCommand::Remove { kind, value } => {
            let token = match kind.trim().to_ascii_lowercase().as_str() {
                "chart" => "chart",
                "dashboard" | "board" => "dashboard",
                other => bail!("kind must be chart or dashboard, not {other}"),
            };
            let catalog = analytics.catalog()?;
            let (id, name, revision) = match token {
                "chart" => {
                    let found = find_chart(&catalog, &value)?;
                    // A chart lives on a dashboard, so take it off every board first.
                    for board in catalog
                        .dashboards
                        .iter()
                        .filter(|board| board.charts.iter().any(|p| p.chart_id == found.id))
                    {
                        let mut next = board.clone();
                        next.charts
                            .retain(|placement| placement.chart_id != found.id);
                        analytics.save_dashboard(next)?;
                    }
                    (found.id, found.name, found.revision)
                }
                _ => {
                    let found = find_dashboard(&catalog, &value)?;
                    // Deleting a dashboard takes its charts with it, like the browser does.
                    for placement in &found.charts {
                        if let Ok(chart) = find_chart(&catalog, &placement.chart_id) {
                            let _ = analytics.delete("chart", &chart.id, chart.revision);
                        }
                    }
                    let board = find_dashboard(&analytics.catalog()?, &value)?;
                    (board.id, board.name, board.revision)
                }
            };
            analytics.delete(token, &id, revision)?;
            Ok(json!({"kind": token, "removed": name, "id": id}))
        }
    }
}

/// A person types a name; the catalog stores ids. Both resolve here, and an ambiguous name is an
/// error rather than a guess.
fn unique<'a, T>(
    items: impl Iterator<Item = &'a T>,
    needle: &str,
    id: impl Fn(&T) -> &str,
    name: impl Fn(&T) -> &str,
    what: &str,
) -> Result<T>
where
    T: Clone + 'a,
{
    let matches: Vec<&T> = items
        .filter(|item| id(item) == needle || name(item) == needle)
        .collect();
    match matches.as_slice() {
        [one] => Ok((*one).clone()),
        [] => bail!("no saved {what} matches {needle}"),
        _ => bail!("{needle} matches more than one {what}; use its id"),
    }
}
fn find_chart(catalog: &Catalog, needle: &str) -> Result<Chart> {
    unique(
        catalog.charts.iter(),
        needle,
        |c| &c.id,
        |c| &c.name,
        "chart",
    )
}
fn find_dashboard(catalog: &Catalog, needle: &str) -> Result<Dashboard> {
    unique(
        catalog.dashboards.iter(),
        needle,
        |d| &d.id,
        |d| &d.name,
        "dashboard",
    )
}
fn chart_type(value: &str) -> Result<ChartType> {
    let normalized = value.trim().to_ascii_lowercase().replace(['-', '_'], "");
    Ok(match normalized.as_str() {
        "column" => ChartType::Column,
        "bar" => ChartType::Bar,
        "line" => ChartType::Line,
        "arealine" | "area" => ChartType::AreaLine,
        "scatter" => ChartType::Scatter,
        "pie" => ChartType::Pie,
        "ringpie" | "ring" | "donut" => ChartType::RingPie,
        "rosepie" | "rose" => ChartType::RosePie,
        "funnel" => ChartType::Funnel,
        "wordcloud" => ChartType::WordCloud,
        "statistics" | "statistic" | "card" => ChartType::Statistics,
        "combo" => ChartType::Combo,
        "table" => ChartType::Table,
        other => bail!("unknown chart type {other}"),
    })
}
/// Append the chart below the last card, six columns wide, like the editor's default placement.
fn place(analytics: &Analytics, board: &Dashboard, chart: &Chart) -> Result<Dashboard> {
    let mut next = board.clone();
    let y = next
        .charts
        .iter()
        .map(|placement| placement.y + placement.h)
        .max()
        .unwrap_or(0);
    next.charts.push(Placement {
        chart_id: chart.id.clone(),
        x: 0,
        y,
        w: 6,
        h: 4,
    });
    analytics.save_dashboard(next)
}
/// Execute a chart's SQL and publish its snapshot: the same step the browser performs on 运行查询,
/// writing the same store the service writes, so both doors produce identical snapshots.
fn run_chart(
    root: &Path,
    manifest: &str,
    worker_dir: Option<&Path>,
    analytics: &Analytics,
    chart: &Chart,
) -> Result<Chart> {
    let source = Store::open(root.to_path_buf())?.find(&chart.datasource_id)?;
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut store = ResultStore::create(
        &root.join("analytics/pending"),
        &request_id,
        source.id.clone(),
        source.name.clone(),
        chart.statements.clone(),
        "analytics",
    )?;
    let prepared = execution::prepare(
        root.to_path_buf(),
        manifest.to_owned(),
        worker_dir.map(Path::to_path_buf),
        source,
        sqlx_protocol::Action::Execute,
        chart.statements.clone(),
    )?;
    let started = std::time::Instant::now();
    let runtime = tokio::runtime::Runtime::new().context("cannot start the execution runtime")?;
    let success = runtime
        .block_on(prepared.execute(|event| store.record(event), CancellationToken::new()))?;
    store.finish(success, false, started.elapsed().as_millis() as u64, None)?;
    if !success {
        let message = store
            .metadata
            .events
            .iter()
            .find_map(|event| {
                (event["event"] == "error")
                    .then(|| event["message"].as_str())
                    .flatten()
            })
            .unwrap_or("the query failed; the previous snapshot is kept");
        let _ = store.remove();
        bail!("{message}");
    }
    let published = analytics.publish(&chart.id, chart.revision, &store)?;
    let _ = store.remove();
    analytics.collect()?;
    Ok(published)
}
fn chart_json(chart: &Chart) -> Value {
    json!({
        "id": chart.id,
        "name": chart.name,
        "datasource_id": chart.datasource_id,
        "statements": chart.statements,
        "type": chart.spec.chart_type,
        "dimension": chart.spec.dimension,
        "metrics": chart.spec.metrics.iter().map(|metric| metric.field.clone()).collect::<Vec<_>>(),
        "snapshot_id": chart.snapshot_id,
        "revision": chart.revision,
    })
}
fn dashboard_json(board: &Dashboard) -> Value {
    json!({
        "id": board.id,
        "name": board.name,
        "description": board.description,
        "charts": board.charts.len(),
        "revision": board.revision,
    })
}
