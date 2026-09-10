use crate::results::{now, Metadata, ResultStore};
use anyhow::Result;
use axum::{
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx_core::{
    execution,
    storage::{Datasource, Store},
    ui::{SetupRequest, UiState, ViewRequest},
};
use sqlx_protocol::{Action, Connection, Event};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

const SETUP_TTL: u64 = 30 * 60;
const SESSION_TTL: u64 = 12 * 60 * 60;
type SharedResult = Arc<Mutex<ResultStore>>;
type ApiResult = std::result::Result<Json<Value>, ApiError>;
type Local = Arc<App>;

pub struct App {
    pub state: UiState,
    pub root: PathBuf,
    manifest: String,
    workers: Option<PathBuf>,
    setups: Mutex<HashMap<String, Arc<Mutex<Setup>>>>,
    results: Mutex<HashMap<String, SharedResult>>,
    cancellations: Mutex<HashMap<String, CancellationToken>>,
    tickets: Mutex<HashMap<String, u64>>,
    sessions: Mutex<HashMap<String, u64>>,
    last_access: AtomicU64,
    pub active: AtomicUsize,
    pub shutdown: CancellationToken,
}
struct Setup {
    id: String,
    request: SetupRequest,
    baseline: Option<String>,
    created: u64,
    status: String,
    error: Option<String>,
    datasource_id: Option<String>,
}
impl Setup {
    fn expire(&mut self) {
        if self.status == "waiting_for_user" && now().saturating_sub(self.created) > SETUP_TTL {
            self.status = "expired".into();
        }
    }
    fn status(&self) -> Value {
        json!({"request_id":self.id,"status":self.status,"error":self.error,"datasource_id":self.datasource_id})
    }
    fn form(&self) -> Value {
        let mut value = self.status();
        let mut connection = serde_json::to_value(&self.request.connection).unwrap();
        connection.as_object_mut().unwrap().remove("password");
        connection.as_object_mut().unwrap().remove("properties");
        value["name"] = self.request.name.clone().into();
        value["connection"] = connection;
        value["editing"] = self.request.source_id.is_some().into();
        value
    }
}
pub fn token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
fn fingerprint(source: &Datasource) -> String {
    hex::encode(Sha256::digest(serde_json::to_vec(source).unwrap()))
}
fn validate_id(id: &str) -> std::result::Result<(), ApiError> {
    uuid::Uuid::parse_str(id).map_err(|_| bad("Invalid operation ID"))?;
    Ok(())
}
fn validate_name(name: &str) -> std::result::Result<(), ApiError> {
    if name.trim().is_empty() || uuid::Uuid::parse_str(name).is_ok() {
        return Err(bad("Choose a nonempty datasource name that is not a UUID"));
    }
    Ok(())
}
fn bad(message: &str) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, message.into())
}
fn missing() -> ApiError {
    ApiError(
        StatusCode::NOT_FOUND,
        "This page has expired or the operation does not exist".into(),
    )
}
fn conflict(message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, message.into())
}
fn internal(_: impl std::fmt::Display) -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "The local operation failed; existing data was preserved".into(),
    )
}
pub struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":{"message":self.1}}))).into_response()
    }
}

impl App {
    pub fn new(
        state: UiState,
        root: PathBuf,
        manifest: String,
        workers: Option<PathBuf>,
    ) -> Result<Self> {
        let result_root = root.join("results");
        fs::create_dir_all(&result_root)?;
        sqlx_core::storage::restrict(&result_root, true)?;
        let mut results = HashMap::new();
        for entry in fs::read_dir(&result_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir()
                || uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err()
            {
                continue;
            }
            if let Ok(store) = ResultStore::recover(&entry.path()) {
                if store.expired() {
                    store.remove()?;
                } else {
                    results.insert(
                        store.metadata.result_id.clone(),
                        Arc::new(Mutex::new(store)),
                    );
                }
            }
        }
        Ok(Self {
            state,
            root,
            manifest,
            workers,
            results: Mutex::new(results),
            setups: Mutex::new(HashMap::new()),
            cancellations: Mutex::new(HashMap::new()),
            tickets: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            last_access: AtomicU64::new(now()),
            active: AtomicUsize::new(0),
            shutdown: CancellationToken::new(),
        })
    }
    fn setup(&self, id: &str) -> std::result::Result<Arc<Mutex<Setup>>, ApiError> {
        validate_id(id)?;
        self.setups
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(missing)
    }
    fn result(&self, id: &str) -> std::result::Result<SharedResult, ApiError> {
        validate_id(id)?;
        let result = self
            .results
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(missing)?;
        if result.lock().unwrap().expired() {
            return Err(missing());
        }
        Ok(result)
    }
    pub fn cancel_tasks(&self) {
        for cancel in self.cancellations.lock().unwrap().values() {
            cancel.cancel();
        }
    }
    fn cookie_name(&self) -> String {
        format!("sqlx_ui_{}", self.state.instance.replace('-', ""))
    }
}
struct Active(Local);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::Relaxed);
    }
}
fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .filter_map(|s| s.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.into())
}
fn is_admin(app: &App, headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|v| {
            v.strip_prefix("Bearer ")
                .is_some_and(|s| equal(s, &app.state.token))
        })
}
fn equal(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

async fn protect(State(app): State<Local>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let path = request.uri().path();
    let forbidden = || {
        ApiError(
            StatusCode::FORBIDDEN,
            "This local page is not authorized. Open a new page from SQLX.".into(),
        )
        .into_response()
    };
    if headers.get(header::HOST).and_then(|h| h.to_str().ok())
        != app.state.origin.strip_prefix("http://")
    {
        return forbidden();
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        if origin.to_str().ok() != Some(app.state.origin.as_str()) {
            return forbidden();
        }
    }
    if path.starts_with("/api/") {
        let admin = is_admin(&app, headers);
        let mutation = request.method() != axum::http::Method::GET;
        if !admin {
            if headers.get("x-sqlx-ui").and_then(|h| h.to_str().ok()) != Some("1") {
                return forbidden();
            }
            if mutation
                && headers.get(header::ORIGIN).and_then(|h| h.to_str().ok())
                    != Some(app.state.origin.as_str())
            {
                return forbidden();
            }
            if headers
                .get("sec-fetch-site")
                .is_some_and(|h| h != "same-origin")
            {
                return forbidden();
            }
            if path != "/api/session" {
                let valid = cookie(headers, &app.cookie_name())
                    .and_then(|c| app.sessions.lock().unwrap().get(&c).copied())
                    .is_some_and(|expires| expires > now());
                if !valid {
                    return forbidden();
                }
            }
            if matches!(path, "/api/health" | "/api/tickets" | "/api/stop")
                || (mutation && matches!(path, "/api/setups" | "/api/results"))
            {
                return forbidden();
            }
        }
    }
    app.last_access.store(now(), Ordering::Relaxed);
    let mut response = next.run(request).await;
    let h = response.headers_mut();
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    h.insert(header::CONTENT_SECURITY_POLICY,"default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; frame-ancestors 'none'; form-action 'self'".parse().unwrap());
    h.insert("referrer-policy", "no-referrer".parse().unwrap());
    h.insert("x-content-type-options", "nosniff".parse().unwrap());
    response
}
pub fn router(app: Local) -> Router {
    Router::new()
        .route("/favicon.ico", get(|| async { StatusCode::NO_CONTENT }))
        .route("/", get(index))
        .route("/setup/{id}", get(index))
        .route("/result/{id}", get(index))
        .route("/app.js", get(script))
        .route("/app.css", get(styles))
        .route("/api/health", get(health))
        .route("/api/tickets", post(ticket))
        .route("/api/session", post(session))
        .route("/api/home", get(home))
        .route("/api/setups", post(create_setup))
        .route("/api/setups/{id}", get(setup_form).post(save_setup))
        .route("/api/setups/{id}/status", get(setup_status))
        .route("/api/setups/{id}/cancel", post(cancel_setup))
        .route("/api/results", post(create_result))
        .route("/api/results/{id}", get(result_metadata))
        .route("/api/results/{id}/rows", get(result_page))
        .route("/api/results/{id}/cancel", post(cancel_result))
        .route("/api/stop", post(stop))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), protect))
        .with_state(app)
}
async fn index() -> Html<&'static str> {
    Html(include_str!("../../../ui/index.html"))
}
async fn script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../../../ui/dist/app.js"),
    )
}
async fn styles() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../../../ui/style.css"),
    )
}
async fn health(State(app): State<Local>) -> Json<Value> {
    Json(json!({"protocol":app.state.protocol,"instance":app.state.instance}))
}

#[derive(Deserialize)]
struct TicketInput {
    path: String,
}
async fn ticket(State(app): State<Local>, Json(input): Json<TicketInput>) -> ApiResult {
    if input.path != "/" {
        let (kind, id) = input
            .path
            .trim_start_matches('/')
            .split_once('/')
            .ok_or_else(|| bad("Invalid page path"))?;
        if kind == "setup" {
            app.setup(id)?;
        } else if kind == "result" {
            app.result(id)?;
        } else {
            return Err(bad("Invalid page path"));
        }
    }
    let token = token();
    app.tickets
        .lock()
        .unwrap()
        .insert(token.clone(), now() + 300);
    Ok(Json(
        json!({"url":format!("{}{}#token={token}",app.state.origin,input.path)}),
    ))
}
#[derive(Deserialize)]
struct SessionInput {
    token: String,
}
async fn session(
    State(app): State<Local>,
    Json(input): Json<SessionInput>,
) -> std::result::Result<Response, ApiError> {
    if app
        .tickets
        .lock()
        .unwrap()
        .remove(&input.token)
        .is_none_or(|expires| expires <= now())
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "This launch link has expired or was already used. Open a new page from SQLX.".into(),
        ));
    }
    let session = token();
    app.sessions
        .lock()
        .unwrap()
        .insert(session.clone(), now() + SESSION_TTL);
    Ok((
        [(
            header::SET_COOKIE,
            format!(
                "{}={session}; HttpOnly; SameSite=Strict; Path=/; Max-Age={SESSION_TTL}",
                app.cookie_name()
            ),
        )],
        Json(json!({"authenticated":true})),
    )
        .into_response())
}
async fn home(State(app): State<Local>) -> ApiResult {
    let setups = app
        .setups
        .lock()
        .unwrap()
        .values()
        .map(|s| {
            let mut s = s.lock().unwrap();
            s.expire();
            json!({"id":s.id,"name":s.request.name,"status":s.status})
        })
        .collect::<Vec<_>>();
    let results=app.results.lock().unwrap().values().filter_map(|r|{let r=r.lock().unwrap();(!r.expired()).then(||json!({"id":r.metadata.result_id,"name":r.metadata.datasource_name,"status":r.metadata.status,"created_at":r.metadata.created_at}))}).collect::<Vec<_>>();
    Ok(Json(json!({"setups":setups,"results":results})))
}
async fn create_setup(
    State(app): State<Local>,
    Json(mut request): Json<SetupRequest>,
) -> ApiResult {
    validate_name(&request.name)?;
    if !request.connection.password.is_empty() {
        return Err(bad("Enter the password in the browser form"));
    }
    request
        .connection
        .validate()
        .map_err(|e| bad(&e.to_string()))?;
    let root = app.root.clone();
    let source_id = request.source_id.clone();
    let name = request.name.clone();
    let baseline =
        tokio::task::spawn_blocking(move || -> std::result::Result<Option<String>, ApiError> {
            let store = Store::open(root).map_err(internal)?;
            let sources = store.load().map_err(internal)?;
            if sources
                .iter()
                .any(|s| s.name == name && Some(&s.id) != source_id.as_ref())
            {
                return Err(conflict("A datasource with this name already exists"));
            }
            source_id
                .map(|id| {
                    store
                        .find(&id)
                        .map(|s| fingerprint(&s))
                        .map_err(|_| missing())
                })
                .transpose()
        })
        .await
        .map_err(internal)??;
    request.connection.password.clear();
    let id = uuid::Uuid::new_v4().to_string();
    let setup = Setup {
        id: id.clone(),
        request,
        baseline,
        created: now(),
        status: "waiting_for_user".into(),
        error: None,
        datasource_id: None,
    };
    let status = setup.status();
    app.setups
        .lock()
        .unwrap()
        .insert(id, Arc::new(Mutex::new(setup)));
    Ok(Json(status))
}
async fn setup_status(State(app): State<Local>, Path(id): Path<String>) -> ApiResult {
    let setup = app.setup(&id)?;
    let mut s = setup.lock().unwrap();
    s.expire();
    Ok(Json(s.status()))
}
async fn setup_form(State(app): State<Local>, Path(id): Path<String>) -> ApiResult {
    let setup = app.setup(&id)?;
    let mut s = setup.lock().unwrap();
    s.expire();
    Ok(Json(s.form()))
}
async fn cancel_setup(State(app): State<Local>, Path(id): Path<String>) -> ApiResult {
    let setup = app.setup(&id)?;
    let mut s = setup.lock().unwrap();
    s.expire();
    if s.status != "waiting_for_user" {
        return Err(conflict(
            "This setup request is no longer waiting for input",
        ));
    }
    s.status = "cancelled".into();
    Ok(Json(s.status()))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum PasswordAction {
    Keep,
    Replace,
    Clear,
}
#[derive(Deserialize)]
struct SetupInput {
    name: String,
    connection: Connection,
    password_action: PasswordAction,
}
async fn save_setup(
    State(app): State<Local>,
    Path(id): Path<String>,
    Json(input): Json<SetupInput>,
) -> ApiResult {
    validate_name(&input.name)?;
    input
        .connection
        .validate()
        .map_err(|e| bad(&e.to_string()))?;
    let setup = app.setup(&id)?;
    {
        let mut s = setup.lock().unwrap();
        s.expire();
        if s.status != "waiting_for_user" {
            return Err(conflict("This setup request cannot be submitted again"));
        }
        s.status = "saving".into();
        s.error = None;
    }
    app.active.fetch_add(1, Ordering::Relaxed);
    let work = app.clone();
    let task_setup = setup.clone();
    tokio::spawn(async move {
        let _active = Active(work.clone());
        let outcome = complete_setup(work.clone(), task_setup.clone(), input).await;
        let mut s = task_setup.lock().unwrap();
        match outcome {
            Ok(id) => {
                s.status = "completed".into();
                s.datasource_id = Some(id);
            }
            Err(error) => {
                s.status = "waiting_for_user".into();
                s.error = Some(error.1);
            }
        }
    });
    let status = setup.lock().unwrap().status();
    Ok(Json(status))
}
async fn complete_setup(
    app: Local,
    setup: Arc<Mutex<Setup>>,
    mut input: SetupInput,
) -> std::result::Result<String, ApiError> {
    let (source_id, baseline, properties) = {
        let s = setup.lock().unwrap();
        (
            s.request.source_id.clone(),
            s.baseline.clone(),
            s.request.connection.properties.clone(),
        )
    };
    let root = app.root.clone();
    let original_id = source_id.clone();
    let existing = tokio::task::spawn_blocking(
        move || -> std::result::Result<Option<Datasource>, ApiError> {
            let store = Store::open(root).map_err(internal)?;
            original_id
                .map(|id| store.find(&id).map_err(|_| missing()))
                .transpose()
        },
    )
    .await
    .map_err(internal)??;
    if existing.as_ref().map(fingerprint) != baseline {
        return Err(conflict(
            "The datasource changed while this page was open. Open a new setup page.",
        ));
    }
    input.connection.properties = properties;
    match input.password_action {
        PasswordAction::Keep => {
            input.connection.password = existing
                .as_ref()
                .ok_or_else(|| bad("A new connection needs a password choice"))?
                .connection
                .password
                .clone();
        }
        PasswordAction::Clear => input.connection.password.clear(),
        PasswordAction::Replace => {}
    }
    let source = Datasource {
        id: source_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: input.name,
        connection: input.connection,
    };
    let root = app.root.clone();
    let manifest = app.manifest.clone();
    let local = app.workers.clone();
    let test_source = source.clone();
    let prepared = tokio::task::spawn_blocking(move || {
        execution::prepare(root, manifest, local, test_source, Action::Test, vec![])
    })
    .await
    .map_err(internal)?
    .map_err(|_| {
        bad("Could not prepare the database driver. Check the CLI release and network connection.")
    })?;
    let mut failure = None;
    let success = prepared
        .execute(
            |event| {
                if let Event::Error { message, .. } = event {
                    failure = Some(message);
                }
                Ok(())
            },
            app.shutdown.child_token(),
        )
        .await
        .map_err(|_| bad("The database connection could not be confirmed"))?;
    if !success {
        return Err(bad(failure.as_deref().unwrap_or("Connection failed")));
    }
    let root = app.root.clone();
    tokio::task::spawn_blocking(move || -> std::result::Result<String, ApiError> {
        let store = Store::open(root).map_err(internal)?;
        let mut values = store.load().map_err(internal)?;
        let index = values.iter().position(|s| s.id == source.id);
        if index.map(|i| fingerprint(&values[i])) != baseline {
            return Err(conflict(
                "The datasource changed during connection testing. Open a new setup page.",
            ));
        }
        if values
            .iter()
            .any(|s| s.name == source.name && s.id != source.id)
        {
            return Err(conflict("A datasource with this name already exists"));
        }
        let id = source.id.clone();
        if let Some(i) = index {
            values[i] = source;
        } else {
            values.push(source);
        }
        store.save(&values).map_err(internal)?;
        Ok(id)
    })
    .await
    .map_err(internal)?
}

async fn create_result(State(app): State<Local>, Json(request): Json<ViewRequest>) -> ApiResult {
    validate_id(&request.request_id)?;
    if request.statements.is_empty() || request.statements.iter().any(|s| s.trim().is_empty()) {
        return Err(bad("At least one nonempty SQL statement is required"));
    }
    let root = app.root.clone();
    let name = request.datasource.clone();
    let source = tokio::task::spawn_blocking(move || Store::open(root)?.find(&name))
        .await
        .map_err(internal)?
        .map_err(|_| bad("Datasource not found or unavailable"))?;
    let id = request.request_id.clone();
    let result = {
        let mut results = app.results.lock().unwrap();
        if let Some(previous) = results.get(&id) {
            let previous = previous.lock().unwrap();
            if previous.metadata.datasource_id != source.id
                || previous.metadata.statements != request.statements
            {
                return Err(conflict("This request ID already belongs to another query"));
            }
            return Ok(Json(
                json!({"result_id":id,"status":previous.metadata.status}),
            ));
        }
        let result = Arc::new(Mutex::new(
            ResultStore::create(
                &app.root,
                &id,
                source.id.clone(),
                source.name.clone(),
                request.statements.clone(),
            )
            .map_err(internal)?,
        ));
        results.insert(id.clone(), result.clone());
        result
    };
    let cancel = app.shutdown.child_token();
    app.cancellations
        .lock()
        .unwrap()
        .insert(id.clone(), cancel.clone());
    app.active.fetch_add(1, Ordering::Relaxed);
    let work = app.clone();
    tokio::spawn(async move {
        let _active = Active(work.clone());
        let start = Instant::now();
        let root = work.root.clone();
        let manifest = work.manifest.clone();
        let local = work.workers.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            execution::prepare(
                root,
                manifest,
                local,
                source,
                Action::Execute,
                request.statements,
            )
        })
        .await;
        let outcome=match prepared {Ok(Ok(prepared)) if !cancel.is_cancelled()=>prepared.execute(|event|result.lock().unwrap().record(event),cancel.clone()).await,_=>Err(anyhow::anyhow!("Query could not start. Check the driver download and datasource, then submit a new request."))};
        let success = outcome.as_ref().is_ok_and(|s| *s);
        let error = outcome.err().map(|_| {
            "Execution could not be completed. No automatic retry was attempted.".to_string()
        });
        if result
            .lock()
            .unwrap()
            .finish(
                success,
                cancel.is_cancelled(),
                start.elapsed().as_millis() as u64,
                error,
            )
            .is_err()
        {
            eprintln!("Could not persist query completion");
        }
        work.cancellations.lock().unwrap().remove(&id);
    });
    Ok(Json(
        json!({"result_id":request.request_id,"status":"queued"}),
    ))
}
async fn result_metadata(
    State(app): State<Local>,
    Path(id): Path<String>,
) -> std::result::Result<Json<Metadata>, ApiError> {
    let result = app.result(&id)?;
    let meta = result.lock().unwrap().metadata.clone();
    Ok(Json(meta))
}
#[derive(Deserialize)]
struct PageQuery {
    statement: usize,
    result: usize,
    #[serde(default)]
    offset: u64,
    #[serde(default = "page_limit")]
    limit: usize,
}
fn page_limit() -> usize {
    100
}
async fn result_page(
    State(app): State<Local>,
    Path(id): Path<String>,
    Query(query): Query<PageQuery>,
) -> std::result::Result<Json<crate::results::Page>, ApiError> {
    let result = app.result(&id)?;
    let page = tokio::task::spawn_blocking(move || {
        result
            .lock()
            .unwrap()
            .page(query.statement, query.result, query.offset, query.limit)
    })
    .await
    .map_err(internal)?
    .map_err(|e| bad(&e.to_string()))?;
    Ok(Json(page))
}
async fn cancel_result(State(app): State<Local>, Path(id): Path<String>) -> ApiResult {
    app.result(&id)?;
    if let Some(cancel) = app.cancellations.lock().unwrap().get(&id) {
        cancel.cancel();
    }
    Ok(Json(json!({"cancel_requested":true})))
}
async fn stop(State(app): State<Local>) -> ApiResult {
    app.cancel_tasks();
    app.shutdown.cancel();
    Ok(Json(json!({"status":"stopping"})))
}

pub async fn maintenance(app: Local) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        app.tickets
            .lock()
            .unwrap()
            .retain(|_, expires| *expires > now());
        app.sessions
            .lock()
            .unwrap()
            .retain(|_, expires| *expires > now());
        app.setups.lock().unwrap().retain(|_, setup| {
            let mut setup = setup.lock().unwrap();
            setup.expire();
            setup.status == "saving" || now().saturating_sub(setup.created) < 2 * SETUP_TTL
        });
        if app.active.load(Ordering::Relaxed) == 0 {
            app.results.lock().unwrap().retain(|_, result| {
                let result = result.lock().unwrap();
                if result.expired() {
                    let _ = result.remove();
                    false
                } else {
                    true
                }
            });
            if now().saturating_sub(app.last_access.load(Ordering::Relaxed)) > 30 * 60 {
                app.shutdown.cancel();
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use tower::ServiceExt;
    fn fixture() -> (tempfile::TempDir, Local) {
        let dir = tempfile::tempdir().unwrap();
        let state = UiState {
            protocol: 1,
            instance: "test-instance".into(),
            origin: "http://127.0.0.1:32145".into(),
            token: token(),
            pid: 1,
        };
        let app = Arc::new(
            App::new(
                state,
                dir.path().to_path_buf(),
                "https://example.invalid/manifest.json".into(),
                None,
            )
            .unwrap(),
        );
        (dir, app)
    }
    #[tokio::test]
    async fn local_api_requires_auth_and_rejects_foreign_origins_and_hosts() {
        let (_dir, app) = fixture();
        for (host, origin, admin, expected) in [
            ("127.0.0.1:32145", None, false, StatusCode::FORBIDDEN),
            (
                "127.0.0.1:32145",
                Some("https://unrelated.example"),
                true,
                StatusCode::FORBIDDEN,
            ),
            ("attacker.example:32145", None, true, StatusCode::FORBIDDEN),
            ("127.0.0.1:32145", None, true, StatusCode::OK),
        ] {
            let mut request = axum::http::Request::builder()
                .uri("/api/health")
                .header("Host", host);
            if let Some(origin) = origin {
                request = request.header("Origin", origin);
            }
            if admin {
                request = request.header("Authorization", format!("Bearer {}", app.state.token));
            }
            let response = router(app.clone())
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
    }
    #[tokio::test]
    async fn browser_bootstrap_is_one_use_and_cannot_control_admin_endpoints() {
        let (_dir, app) = fixture();
        let launch = token();
        app.tickets
            .lock()
            .unwrap()
            .insert(launch.clone(), now() + 300);
        let request = || {
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/session")
                .header("Host", "127.0.0.1:32145")
                .header("Origin", &app.state.origin)
                .header("X-SQLX-UI", "1")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"token":launch}).to_string()))
                .unwrap()
        };
        let response = router(app.clone()).oneshot(request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        assert_eq!(
            router(app.clone())
                .oneshot(request())
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        for (path, expected) in [
            ("/api/home", StatusCode::OK),
            ("/api/health", StatusCode::FORBIDDEN),
        ] {
            let request = axum::http::Request::builder()
                .uri(path)
                .header("Host", "127.0.0.1:32145")
                .header("X-SQLX-UI", "1")
                .header("Cookie", &cookie)
                .body(Body::empty())
                .unwrap();
            let response = router(app.clone()).oneshot(request).await.unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::OK {
                let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
                assert!(!String::from_utf8_lossy(&body).contains(&app.state.token));
            }
        }
    }
}
