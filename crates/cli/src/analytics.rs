//! Durable datasets, immutable result snapshots, chart specifications and dashboards.
use crate::{
    results::ResultStore,
    storage::{atomic_write, open_private, restrict},
};
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sqlparser::{
    ast::{Query, SetExpr, Statement, Visit, Visitor},
    dialect::GenericDialect,
    parser::Parser,
};
use std::{
    fs,
    ops::ControlFlow,
    path::{Path, PathBuf},
};

/// Shape marker for `catalog.json`. Nothing has shipped yet, so this stays at the first value:
/// the structs below are the format. A stale local file is rejected by their `deny_unknown_fields`
/// rather than silently loaded with missing pieces.
pub const SCHEMA_VERSION: u32 = 1;
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Catalog {
    pub schema_version: u32,
    pub charts: Vec<Chart>,
    pub dashboards: Vec<Dashboard>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum ChartType {
    Column,
    Bar,
    Line,
    AreaLine,
    Scatter,
    Pie,
    RingPie,
    RosePie,
    Funnel,
    WordCloud,
    Statistics,
    Combo,
    Table,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum SeriesType {
    Column,
    Line,
    AreaLine,
    Scatter,
}
#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    #[default]
    Left,
    Right,
}
#[derive(Clone, Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub enum LineStyle {
    #[default]
    Straight,
    Smooth,
    Step,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    pub field: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub axis: Axis,
    #[serde(default)]
    pub kind: Option<SeriesType>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartSpec {
    pub chart_type: ChartType,
    pub dimension: Option<String>,
    #[serde(default)]
    pub group_by: Vec<String>,
    pub metrics: Vec<Metric>,
    #[serde(default)]
    pub stack: bool,
    #[serde(default)]
    pub line_style: LineStyle,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chart {
    pub id: String,
    pub name: String,
    /// The datasource this chart's SQL runs against.
    pub datasource_id: String,
    /// The read-only SQL behind the chart; running it publishes the snapshot below.
    pub statements: Vec<String>,
    pub statement: usize,
    pub result: usize,
    /// The snapshot the chart renders. Running the SQL replaces it and pushes the old one to history.
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub history: Vec<String>,
    pub spec: ChartSpec,
    pub revision: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub chart_id: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dashboard {
    pub id: String,
    pub name: String,
    pub description: String,
    pub charts: Vec<Placement>,
    pub revision: u64,
}

/// Conservative structural gate for repeatable queries; use a database read-only account as well.
pub fn validate_queries(statements: &[String]) -> Result<()> {
    fn read_body(body: &SetExpr) -> bool {
        match body {
            SetExpr::Select(select) => select.into.is_none(),
            SetExpr::SetOperation { left, right, .. } => read_body(left) && read_body(right),
            SetExpr::Query(_) | SetExpr::Values(_) | SetExpr::Table(_) => true,
            _ => false,
        }
    }
    struct ReadQuery;
    impl Visitor for ReadQuery {
        type Break = ();
        fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<()> {
            if !matches!(statement, Statement::Query(_)) {
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        }
        fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<()> {
            if !query.locks.is_empty() || !read_body(&query.body) {
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        }
    }
    if statements.is_empty() {
        bail!("A dataset needs at least one SELECT query");
    }
    for sql in statements {
        let parsed = Parser::parse_sql(&GenericDialect {}, sql)?;
        if parsed.len() != 1 || parsed[0].visit(&mut ReadQuery).is_break() {
            bail!("Datasets accept one read query per statement, without writes, INTO or locking");
        }
    }
    Ok(())
}
pub fn id(value: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)?;
    Ok(())
}
fn name(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 200 {
        bail!("Choose a name of 1–200 characters");
    }
    Ok(())
}
fn revision(current: Option<u64>, supplied: u64) -> Result<u64> {
    if current.unwrap_or(0) != supplied {
        bail!("This object changed. Reload before saving");
    }
    Ok(supplied + 1)
}

pub struct Analytics {
    root: PathBuf,
}
impl Analytics {
    pub fn new(data: &Path) -> Result<Self> {
        let root = data.join("analytics");
        for path in [&root, &root.join("snapshots"), &root.join("pending")] {
            fs::create_dir_all(path)?;
            restrict(path, true)?;
        }
        Ok(Self { root })
    }
    pub fn catalog(&self) -> Result<Catalog> {
        match fs::read(self.root.join("catalog.json")) {
            Ok(bytes) => {
                let value: Catalog = serde_json::from_slice(&bytes)?;
                if value.schema_version != SCHEMA_VERSION {
                    bail!("Unsupported analytics schema version");
                }
                Ok(value)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Catalog {
                schema_version: SCHEMA_VERSION,
                ..Catalog::default()
            }),
            Err(e) => Err(e.into()),
        }
    }
    fn edit<T>(&self, change: impl FnOnce(&mut Catalog) -> Result<T>) -> Result<T> {
        let lock = open_private(&self.root.join("catalog.lock"))?;
        lock.lock_exclusive()?;
        let mut catalog = self.catalog()?;
        let result = change(&mut catalog)?;
        atomic_write(
            &self.root.join("catalog.json"),
            &serde_json::to_vec_pretty(&catalog)?,
        )?;
        Ok(result)
    }
    pub fn snapshot(&self, snapshot: &str) -> Result<ResultStore> {
        id(snapshot)?;
        ResultStore::recover(&self.root.join("snapshots").join(snapshot))
    }
    /// Seal all files before publishing the catalog pointer. A failure leaves the old head usable.
    pub fn publish(
        &self,
        chart_id: &str,
        chart_revision: u64,
        result: &ResultStore,
    ) -> Result<Chart> {
        let snapshot = uuid::Uuid::new_v4().to_string();
        let pending = self.root.join("pending");
        if let Err(e) = result.copy_snapshot(&pending, &snapshot) {
            let _ = fs::remove_dir_all(pending.join(&snapshot));
            return Err(e);
        }
        atomic_write(
            &pending.join(&snapshot).join("provenance.json"),
            &serde_json::to_vec(
                &serde_json::json!({"chart_id":chart_id,"chart_revision":chart_revision,"parameters":{}}),
            )?,
        )?;
        let target = self.root.join("snapshots").join(&snapshot);
        fs::rename(pending.join(&snapshot), &target)?;
        let published = self.edit(|catalog| {
            // The chart must not have been edited while its SQL ran.
            let chart = catalog
                .charts
                .iter_mut()
                .find(|c| c.id == chart_id)
                .context("Chart not found")?;
            if chart.revision != chart_revision
                || chart.statements != result.metadata.statements
                || chart.datasource_id != result.metadata.datasource_id
            {
                bail!("The chart SQL changed during execution; the old snapshot is preserved");
            }
            validate_chart(chart, result)?;
            if let Some(old) = chart.snapshot_id.replace(snapshot.clone()) {
                chart.history.insert(0, old);
            }
            chart.history.truncate(3);
            Ok(chart.clone())
        });
        if published.is_err() {
            let _ = fs::remove_dir_all(target);
        }
        published
    }
    /// A chart carries its own SQL: saving validates the SQL, keeps the executed snapshot head and
    /// checks the bindings against that snapshot. A chart with no snapshot yet is a valid draft.
    pub fn save_chart(&self, mut chart: Chart) -> Result<Chart> {
        id(&chart.id)?;
        name(&chart.name)?;
        id(&chart.datasource_id)?;
        validate_queries(&chart.statements)?;
        self.edit(|catalog| {
            let previous = catalog.charts.iter().find(|c| c.id == chart.id);
            chart.revision = revision(previous.map(|c| c.revision), chart.revision)?;
            // Snapshot heads belong to execution, never to an editor's stale copy: a save keeps the
            // previous head unless it names one this chart already owns (head or history).
            let owned = previous.is_some_and(|c| {
                c.snapshot_id.as_ref() == chart.snapshot_id.as_ref()
                    || chart
                        .snapshot_id
                        .as_ref()
                        .is_some_and(|snapshot| c.history.contains(snapshot))
            });
            chart.snapshot_id = match previous {
                Some(c) if !owned => c.snapshot_id.clone(),
                _ => chart.snapshot_id.clone(),
            };
            chart.history = previous.map(|c| c.history.clone()).unwrap_or_default();
            if let Some(snapshot) = &chart.snapshot_id {
                validate_chart(&chart, &self.snapshot(snapshot)?)?;
            }
            catalog.charts.retain(|c| c.id != chart.id);
            catalog.charts.push(chart.clone());
            Ok(chart)
        })
    }
    pub fn save_dashboard(&self, mut dashboard: Dashboard) -> Result<Dashboard> {
        id(&dashboard.id)?;
        name(&dashboard.name)?;
        self.edit(|catalog| {
            let mut seen = std::collections::HashSet::new();
            for cell in &dashboard.charts {
                if !catalog.charts.iter().any(|c| c.id == cell.chart_id)
                    || !seen.insert(&cell.chart_id)
                {
                    bail!("Dashboard contains an unknown or duplicate chart");
                }
                if cell.w == 0
                    || cell.x.checked_add(cell.w).is_none_or(|end| end > 12)
                    || !(2..=16).contains(&cell.h)
                    || cell.y > 10000
                {
                    bail!("Invalid dashboard layout");
                }
            }
            dashboard.revision = revision(
                catalog
                    .dashboards
                    .iter()
                    .find(|d| d.id == dashboard.id)
                    .map(|d| d.revision),
                dashboard.revision,
            )?;
            catalog.dashboards.retain(|d| d.id != dashboard.id);
            catalog.dashboards.push(dashboard.clone());
            Ok(dashboard)
        })
    }
    pub fn delete(&self, kind: &str, value: &str, supplied: u64) -> Result<()> {
        id(value)?;
        self.edit(|catalog| {
            match kind {
                "dashboard" => {
                    revision(
                        catalog
                            .dashboards
                            .iter()
                            .find(|d| d.id == value)
                            .map(|d| d.revision),
                        supplied,
                    )?;
                    catalog.dashboards.retain(|d| d.id != value);
                }
                "chart" => {
                    revision(
                        catalog
                            .charts
                            .iter()
                            .find(|d| d.id == value)
                            .map(|d| d.revision),
                        supplied,
                    )?;
                    if catalog
                        .dashboards
                        .iter()
                        .any(|d| d.charts.iter().any(|c| c.chart_id == value))
                    {
                        bail!("Remove the chart from its dashboards first");
                    }
                    catalog.charts.retain(|d| d.id != value);
                }
                _ => bail!("Unknown analytics object"),
            }
            Ok(())
        })
    }
    /// Unreferenced snapshots only; every chart's current snapshot and recent history stay durable.
    pub fn collect(&self) -> Result<()> {
        let lock = open_private(&self.root.join("catalog.lock"))?;
        lock.lock_exclusive()?;
        let catalog = self.catalog()?;
        let referenced: std::collections::HashSet<&str> = catalog
            .charts
            .iter()
            .flat_map(|c| c.snapshot_id.iter().chain(c.history.iter()))
            .map(String::as_str)
            .collect();
        for entry in fs::read_dir(self.root.join("snapshots"))? {
            let entry = entry?;
            let key = entry.file_name();
            if entry.file_type()?.is_dir()
                && !referenced.contains(key.to_str().unwrap_or(""))
                && entry
                    .metadata()?
                    .modified()?
                    .elapsed()
                    .unwrap_or_default()
                    .as_secs()
                    > 3600
            {
                fs::remove_dir_all(entry.path())?;
            }
        }
        Ok(())
    }
}

pub fn validate_chart(chart: &Chart, store: &ResultStore) -> Result<()> {
    let spec = &chart.spec;
    let table = store
        .metadata
        .tables
        .iter()
        .find(|t| t.statement == chart.statement && t.result == chart.result)
        .context("Result set not found")?;
    if table.rows > 100_000 {
        bail!(
            "Charts support up to 100,000 complete rows. Aggregate in SQL; no rows were truncated"
        );
    }
    let mut fields = std::collections::HashSet::new();
    if table.columns.iter().any(|c| !fields.insert(&c.name)) {
        bail!("Duplicate column names need distinct SQL aliases");
    }
    if spec.group_by.len() > 3 || spec.metrics.len() > 8 {
        bail!("At most 3 group fields and 8 metrics are supported");
    }
    let mut bindings = std::collections::HashSet::new();
    for field in spec
        .dimension
        .iter()
        .chain(spec.group_by.iter())
        .chain(spec.metrics.iter().map(|m| &m.field))
    {
        if !fields.contains(field) || !bindings.insert(field) {
            bail!("Chart fields must exist and have distinct bindings");
        }
    }
    if spec.chart_type == ChartType::Table {
        return Ok(());
    }
    if spec.metrics.is_empty() {
        bail!("Choose at least one numeric metric");
    }
    if spec.chart_type == ChartType::Statistics {
        if table.rows != 1 || spec.metrics.len() != 1 || !spec.group_by.is_empty() {
            bail!("Statistics requires one SQL summary row and one metric");
        }
    } else if spec.dimension.is_none() {
        bail!("Choose the primary dimension");
    }
    let radial = matches!(
        spec.chart_type,
        ChartType::Pie
            | ChartType::RingPie
            | ChartType::RosePie
            | ChartType::Funnel
            | ChartType::WordCloud
    );
    if radial && (spec.metrics.len() != 1 || !spec.group_by.is_empty()) {
        bail!("This chart requires one metric and no series grouping");
    }
    for axis in [Axis::Left, Axis::Right] {
        let units: std::collections::HashSet<_> = spec
            .metrics
            .iter()
            .filter(|m| m.axis == axis)
            .map(|m| &m.unit)
            .collect();
        if units.len() > 1 {
            bail!("Metrics on the same axis must use the same unit");
        }
    }
    if spec.chart_type != ChartType::Combo
        && spec
            .metrics
            .iter()
            .any(|m| m.axis == Axis::Right || m.kind.is_some())
    {
        bail!("Mixed series types and dual axes require Combo");
    }
    if spec.chart_type == ChartType::Combo && spec.metrics.iter().any(|m| m.kind.is_none()) {
        bail!("Choose a series type for each Combo metric");
    }
    let index = |field: &String| table.columns.iter().position(|c| c.name == *field).unwrap();
    let group_indices: Vec<_> = spec.group_by.iter().map(index).collect();
    let mut groups = std::collections::HashSet::new();
    let mut grains = std::collections::HashSet::new();
    let mut offset = 0;
    while offset < table.rows {
        let page = store.page(chart.statement, chart.result, offset, 200)?;
        for row in &page.rows {
            if spec.chart_type == ChartType::Scatter {
                numeric(&row[index(spec.dimension.as_ref().unwrap())], true)?;
            }
            let tuple: Vec<_> = group_indices.iter().map(|i| &row[*i]).collect();
            groups.insert(serde_json::to_string(&tuple)?);
            if groups.len() * spec.metrics.len() > 32 {
                bail!("The chart would produce more than 32 series. Narrow the SQL grouping");
            }
            if !matches!(
                spec.chart_type,
                ChartType::Scatter | ChartType::Statistics | ChartType::Table
            ) {
                let mut grain = tuple;
                grain.push(&row[index(spec.dimension.as_ref().unwrap())]);
                if !grains.insert(serde_json::to_string(&grain)?) {
                    bail!("Duplicate dimension + group grain. Aggregate in SQL first");
                }
            }
            for metric in &spec.metrics {
                let value = &row[index(&metric.field)];
                if value.is_null() {
                    continue;
                }
                let number = numeric(value, spec.chart_type != ChartType::Statistics)?.unwrap();
                if radial && number < 0.0 {
                    bail!("This chart cannot use negative values");
                }
            }
        }
        offset = page.next_offset;
    }
    Ok(())
}
fn numeric(value: &serde_json::Value, safe: bool) -> Result<Option<f64>> {
    if value.is_null() {
        return Ok(None);
    }
    let number = value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.parse::<f64>().ok()))
        .filter(|v| v.is_finite())
        .context("A chart coordinate or metric contains nonnumeric values")?;
    if safe && number.abs() > 9_007_199_254_740_991.0 {
        bail!("Metric exceeds safe chart precision; scale it in SQL. Exact values remain in the table");
    }
    Ok(Some(number))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refresh_queries_exclude_writes_even_in_ctes() {
        assert!(validate_queries(&["WITH t AS (SELECT 1) SELECT * FROM t".into()]).is_ok());
        for sql in [
            "DELETE FROM t",
            "SELECT 1; DELETE FROM t",
            "SELECT * INTO t FROM s",
            "SELECT * FROM t FOR UPDATE",
            "WITH t AS (DELETE FROM s RETURNING *) SELECT * FROM t",
        ] {
            assert!(validate_queries(&[sql.into()]).is_err(), "{sql}");
        }
    }
    #[test]
    fn object_revisions_are_independent_and_survive_reopen() {
        let root = tempfile::tempdir().unwrap();
        let store = Analytics::new(root.path()).unwrap();
        let mut board = Dashboard {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Demo".into(),
            description: String::new(),
            charts: vec![],
            revision: 0,
        };
        board = store.save_dashboard(board).unwrap();
        let stale = board.clone();
        board.name = "Renamed".into();
        assert_eq!(store.save_dashboard(board).unwrap().revision, 2);
        assert!(store.save_dashboard(stale).is_err());
        assert_eq!(
            Analytics::new(root.path())
                .unwrap()
                .catalog()
                .unwrap()
                .dashboards[0]
                .name,
            "Renamed"
        );
    }
    fn chart(store: &Analytics) -> Chart {
        store
            .save_chart(Chart {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Sales".into(),
                datasource_id: uuid::Uuid::new_v4().to_string(),
                statements: vec!["SELECT 1 AS value".into()],
                statement: 0,
                result: 0,
                snapshot_id: None,
                history: vec![],
                spec: ChartSpec {
                    chart_type: ChartType::Column,
                    dimension: Some("label".into()),
                    group_by: vec![],
                    metrics: vec![Metric {
                        field: "value".into(),
                        label: String::new(),
                        unit: String::new(),
                        axis: Axis::Left,
                        kind: None,
                    }],
                    stack: false,
                    line_style: LineStyle::Straight,
                },
                revision: 0,
            })
            .unwrap()
    }
    fn result(root: &Path, chart: &Chart, value: &str) -> ResultStore {
        use sqlx_protocol::{Column, Event};
        let mut result = ResultStore::create(
            &root.join("results"),
            &uuid::Uuid::new_v4().to_string(),
            chart.datasource_id.clone(),
            "mysql".into(),
            chart.statements.clone(),
            "ui",
        )
        .unwrap();
        result
            .record(Event::Columns {
                index: 0,
                result: 0,
                columns: vec![
                    Column {
                        name: "label".into(),
                        database_type: "varchar".into(),
                        encoding: "string".into(),
                    },
                    Column {
                        name: "value".into(),
                        database_type: "decimal".into(),
                        encoding: "string".into(),
                    },
                ],
            })
            .unwrap();
        result
            .record(Event::Row {
                index: 0,
                result: 0,
                values: vec![
                    serde_json::Value::String("a".into()),
                    serde_json::Value::String(value.into()),
                ],
            })
            .unwrap();
        result
            .record(Event::ResultEnd {
                index: 0,
                result: 0,
                rows: "1".into(),
                affected_rows: None,
            })
            .unwrap();
        result.finish(true, false, 1, None).unwrap();
        result
    }
    #[test]
    fn immutable_snapshots_survive_cache_removal_and_failed_publication() {
        let root = tempfile::tempdir().unwrap();
        let store = Analytics::new(root.path()).unwrap();
        let chart = chart(&store);
        let old = result(root.path(), &chart, "123.4567890123456789");
        let saved = store.publish(&chart.id, chart.revision, &old).unwrap();
        old.remove().unwrap();
        assert_eq!(
            store
                .snapshot(saved.snapshot_id.as_ref().unwrap())
                .unwrap()
                .page(0, 0, 0, 100)
                .unwrap()
                .rows[0][1],
            "123.4567890123456789"
        );
        // A run that failed, or a chart edited while its SQL ran, cannot move the snapshot head.
        let mut next = result(root.path(), &chart, "2");
        next.finish(false, false, 1, None).unwrap();
        assert!(store.publish(&chart.id, chart.revision, &next).is_err());
        assert_eq!(
            store.catalog().unwrap().charts[0].snapshot_id,
            saved.snapshot_id
        );
        next.finish(true, false, 1, None).unwrap();
        assert!(store.publish(&chart.id, chart.revision + 1, &next).is_err());
        assert_eq!(
            store.catalog().unwrap().charts[0].snapshot_id,
            saved.snapshot_id
        );
        // Running it again with the revision it owns publishes a new head and keeps the old one.
        let published = store.publish(&chart.id, chart.revision, &next).unwrap();
        assert!(published
            .history
            .contains(saved.snapshot_id.as_ref().unwrap()));
    }
}
