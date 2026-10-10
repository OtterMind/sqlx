use super::*;
use serde::Serialize;
use sqlx_core::analytics::{Analytics, Chart, Dashboard};

#[derive(Clone, Serialize)]
pub(super) struct ChartRun {
    request_id: String,
    chart_id: String,
    status: String,
    error: Option<String>,
}
fn store(app: &App) -> std::result::Result<Analytics, ApiError> {
    Analytics::new(&app.root).map_err(internal)
}
fn invalid(error: impl std::fmt::Display) -> ApiError {
    bad(&error.to_string())
}

pub(super) async fn catalog(State(app): State<Local>) -> ApiResult {
    let mut value = serde_json::to_value(store(&app)?.catalog().map_err(internal)?).unwrap();
    value["runs"] = serde_json::to_value(
        app.analytics_runs
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>(),
    )
    .unwrap();
    Ok(Json(value))
}
pub(super) async fn save_chart(State(app): State<Local>, Json(chart): Json<Chart>) -> ApiResult {
    // The chart names its datasource, so an unknown one must fail before anything is stored.
    load_datasource(&app, &chart.datasource_id).await?;
    let root = app.root.clone();
    let saved = tokio::task::spawn_blocking(move || Analytics::new(&root)?.save_chart(chart))
        .await
        .map_err(internal)?
        .map_err(invalid)?;
    Ok(Json(serde_json::to_value(saved).unwrap()))
}
pub(super) async fn save_dashboard(
    State(app): State<Local>,
    Json(board): Json<Dashboard>,
) -> ApiResult {
    Ok(Json(
        serde_json::to_value(store(&app)?.save_dashboard(board).map_err(invalid)?).unwrap(),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DeleteRequest {
    kind: String,
    id: String,
    revision: u64,
}
pub(super) async fn delete(State(app): State<Local>, Json(body): Json<DeleteRequest>) -> ApiResult {
    store(&app)?
        .delete(&body.kind, &body.id, body.revision)
        .map_err(invalid)?;
    Ok(Json(json!({"deleted":true})))
}
fn snapshot(app: &App, key: &str) -> std::result::Result<ResultStore, ApiError> {
    let store = store(app)?;
    let catalog = store.catalog().map_err(internal)?;
    if !catalog
        .charts
        .iter()
        .any(|c| c.snapshot_id.as_deref() == Some(key) || c.history.iter().any(|s| s == key))
    {
        return Err(missing());
    }
    store.snapshot(key).map_err(|_| missing())
}
pub(super) async fn metadata(State(app): State<Local>, Path(key): Path<String>) -> ApiResult {
    Ok(Json(
        serde_json::to_value(snapshot(&app, &key)?.metadata).unwrap(),
    ))
}
pub(super) async fn rows(
    State(app): State<Local>,
    Path(key): Path<String>,
    Query(query): Query<PageQuery>,
) -> std::result::Result<Json<Page>, ApiError> {
    let result = snapshot(&app, &key)?;
    let page = tokio::task::spawn_blocking(move || {
        result.page(query.statement, query.result, query.offset, query.limit)
    })
    .await
    .map_err(internal)?
    .map_err(invalid)?;
    Ok(Json(page))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RunRequest {
    request_id: String,
}
pub(super) async fn run(
    State(app): State<Local>,
    Path(chart_id): Path<String>,
    Json(body): Json<RunRequest>,
) -> ApiResult {
    validate_id(&body.request_id)?;
    let chart = store(&app)?
        .catalog()
        .map_err(internal)?
        .charts
        .into_iter()
        .find(|c| c.id == chart_id)
        .ok_or_else(missing)?;
    sqlx_core::analytics::validate_queries(&chart.statements).map_err(invalid)?;
    let source = load_datasource(&app, &chart.datasource_id).await?;
    let run = ChartRun {
        request_id: body.request_id.clone(),
        chart_id: chart_id.clone(),
        status: "running".into(),
        error: None,
    };
    {
        let mut runs = app.analytics_runs.lock().unwrap();
        if let Some(existing) = runs.get(&body.request_id) {
            if existing.chart_id != chart_id {
                return Err(conflict("Request ID already belongs to another chart"));
            }
            return Ok(Json(serde_json::to_value(existing).unwrap()));
        }
        if runs
            .values()
            .any(|r| r.chart_id == chart_id && r.status == "running")
        {
            return Err(conflict("This chart is already refreshing"));
        }
        runs.insert(body.request_id.clone(), run.clone());
    }
    let work = app.clone();
    app.active.fetch_add(1, Ordering::Relaxed);
    tokio::spawn(async move {
        let _active = Active(work.clone());
        let started = Instant::now();
        let outcome: anyhow::Result<()> = async {
            let _permit = work.analytics_slots.acquire().await?;
            let analytics = Analytics::new(&work.root)?;
            let mut staged = ResultStore::create(
                &work.root.join("analytics/pending"),
                &body.request_id,
                source.id.clone(),
                source.name.clone(),
                chart.statements.clone(),
                "analytics",
            )?;
            let cancel = work.shutdown.child_token();
            let executed = execute_query(
                &work,
                source,
                chart.statements.clone(),
                cancel.clone(),
                |event| staged.record(event),
            )
            .await;
            let success = executed.is_ok_and(|ok| ok) && !cancel.is_cancelled();
            staged.finish(
                success,
                cancel.is_cancelled(),
                started.elapsed().as_millis() as u64,
                None,
            )?;
            let publication = if success {
                analytics
                    .publish(&chart.id, chart.revision, &staged)
                    .map(|_| ())
            } else {
                let error = staged
                    .metadata
                    .events
                    .iter()
                    .find_map(|e| {
                        (e["event"] == "error")
                            .then(|| e["message"].as_str())
                            .flatten()
                    })
                    .unwrap_or("Query failed. The previous snapshot is preserved.");
                Err(anyhow::anyhow!("{error}"))
            };
            let _ = staged.remove();
            publication?;
            analytics.collect()?;
            Ok(())
        }
        .await;
        let mut runs = work.analytics_runs.lock().unwrap();
        if let Some(run) = runs.get_mut(&body.request_id) {
            run.status = if outcome.is_ok() {
                "completed"
            } else {
                "failed"
            }
            .into();
            run.error = outcome.err().map(|e| e.to_string());
        }
    });
    Ok(Json(serde_json::to_value(run).unwrap()))
}
