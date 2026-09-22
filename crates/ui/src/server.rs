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
use sqlx_core::results::{now, Metadata, Page, ResultStore};
use sqlx_core::{
    execution,
    plugins::{self, Plugin},
    storage::{Datasource, Store},
    ui::{SetupRequest, UiState, ViewRequest},
};
use sqlx_protocol::{Action, Connection, Event};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
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
    plugin_cache: Mutex<HashMap<String, Arc<Plugin>>>,
    pub root: PathBuf,
    manifest: String,
    workers: Option<PathBuf>,
    setups: Mutex<HashMap<String, Arc<Mutex<Setup>>>>,
    results: Mutex<HashMap<String, SharedResult>>,
    cancellations: Mutex<HashMap<String, CancellationToken>>,
    sessions: Mutex<HashMap<String, u64>>,
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
                } else if store.metadata.origin != sqlx_core::results::CLI_ORIGIN {
                    // Command line results are read back with `sqlx results`; the page shows its
                    // own results, which always hold every statement.
                    results.insert(
                        store.metadata.result_id.clone(),
                        Arc::new(Mutex::new(store)),
                    );
                }
            }
        }
        let plugin = Arc::new(plugins::active(&root)?);
        let mut plugin_cache = HashMap::new();
        plugin_cache.insert(
            format!("{}/{}", plugin.manifest.id, plugin.manifest.version),
            plugin.clone(),
        );
        Ok(Self {
            plugin_cache: Mutex::new(plugin_cache),
            state,
            root,
            manifest,
            workers,
            results: Mutex::new(results),
            setups: Mutex::new(HashMap::new()),
            cancellations: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
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
    let mut renewal = None;
    let headers = request.headers();
    let path = request.uri().path();
    let browser_page = request.method() == axum::http::Method::GET
        && (path == "/"
            || path.starts_with("/setup/")
            || path.starts_with("/result/")
            || path.starts_with("/datasource/"));
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
            renewal = cookie(headers, &app.cookie_name()).filter(|c| {
                app.sessions
                    .lock()
                    .unwrap()
                    .get(c)
                    .is_some_and(|expires| *expires > now())
            });
            if renewal.is_none() {
                return forbidden();
            }
            if matches!(path, "/api/health" | "/api/stop")
                || (mutation && matches!(path, "/api/setups" | "/api/results"))
            {
                return forbidden();
            }
        }
    } else if browser_page {
        // Opening the local page creates or renews its browser session.
        let session = cookie(headers, &app.cookie_name()).filter(|c| {
            app.sessions
                .lock()
                .unwrap()
                .get(c)
                .is_some_and(|expires| *expires > now())
        });
        renewal = Some(session.unwrap_or_else(token));
    }
    if let Some(session) = &renewal {
        app.sessions
            .lock()
            .unwrap()
            .insert(session.clone(), now() + SESSION_TTL);
    }
    let mut response = next.run(request).await;
    let h = response.headers_mut();
    if let Some(session) = renewal {
        h.insert(
            header::SET_COOKIE,
            session_cookie(&app, &session).parse().unwrap(),
        );
    }
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    h.insert(header::CONTENT_SECURITY_POLICY,"default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'self'; frame-ancestors 'none'; form-action 'self'".parse().unwrap());
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
        .route("/datasource/{id}", get(index))
        .route("/_ui/{plugin}/{version}/{*asset}", get(plugin_asset))
        .route("/api/plugin", get(active_plugin))
        .route("/api/health", get(health))
        .route("/api/home", get(home))
        .route("/api/datasources/{id}", get(datasource_details))
        .route("/api/datasources/{id}/edit", post(edit_datasource))
        .route("/api/datasources/{id}/test", post(test_datasource))
        .route("/api/setups", post(create_setup))
        .route("/api/setups/{id}", get(setup_form).post(save_setup))
        .route("/api/setups/{id}/status", get(setup_status))
        .route("/api/setups/{id}/cancel", post(cancel_setup))
        .route("/api/results", post(create_result))
        .route("/api/results/{id}", get(result_metadata))
        .route("/api/results/{id}/rows", get(result_page))
        .route("/api/results/{id}/cancel", post(cancel_result))
        .route("/api/results/{id}/refresh", post(refresh_result))
        .route("/api/stop", post(stop))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), protect))
        .with_state(app)
}
async fn index(State(app): State<Local>) -> std::result::Result<Html<String>, ApiError> {
    Ok(Html(current_plugin(&app)?.html().map_err(internal)?))
}
async fn plugin_asset(
    State(app): State<Local>,
    Path((id, version, asset)): Path<(String, String, String)>,
) -> std::result::Result<Response, ApiError> {
    let plugin = cached_plugin(&app, &id, &version)?;
    let bytes = plugin.read(&asset).map_err(|_| missing())?;
    let mime = match std::path::Path::new(&asset)
        .extension()
        .and_then(|s| s.to_str())
    {
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        _ => "text/plain; charset=utf-8",
    };
    Ok(([(header::CONTENT_TYPE, mime)], bytes).into_response())
}
fn cached_plugin(app: &App, id: &str, version: &str) -> std::result::Result<Arc<Plugin>, ApiError> {
    let key = format!("{id}/{version}");
    let mut cache = app.plugin_cache.lock().unwrap();
    if let Some(plugin) = cache.get(&key) {
        return Ok(plugin.clone());
    }
    let plugin = Arc::new(plugins::load(&app.root, id, version).map_err(|_| missing())?);
    cache.insert(key, plugin.clone());
    Ok(plugin)
}
fn current_plugin(app: &App) -> std::result::Result<Arc<Plugin>, ApiError> {
    let selection = plugins::selected(&app.root)
        .map_err(internal)?
        .ok_or_else(missing)?;
    cached_plugin(app, &selection.id, &selection.version)
}
async fn active_plugin(State(app): State<Local>) -> std::result::Result<Json<Value>, ApiError> {
    Ok(Json(
        serde_json::to_value(&current_plugin(&app)?.manifest).unwrap(),
    ))
}
async fn health(State(app): State<Local>) -> Json<Value> {
    Json(json!({"protocol":app.state.protocol,"instance":app.state.instance}))
}

fn session_cookie(app: &App, session: &str) -> String {
    format!(
        "{}={session}; HttpOnly; SameSite=Strict; Path=/; Max-Age={SESSION_TTL}",
        app.cookie_name()
    )
}
async fn home(State(app): State<Local>) -> ApiResult {
    let root = app.root.clone();
    let datasources = tokio::task::spawn_blocking(move || {
        Store::open(root)?
            .load()
            .map(|sources| sources.iter().map(Datasource::public).collect::<Vec<_>>())
    })
    .await
    .map_err(internal)?
    .map_err(internal)?;
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
    Ok(Json(
        json!({"datasources":datasources,"setups":setups,"results":results}),
    ))
}

async fn load_datasource(app: &App, id: &str) -> std::result::Result<Datasource, ApiError> {
    validate_id(id)?;
    let root = app.root.clone();
    let id = id.to_owned();
    tokio::task::spawn_blocking(move || {
        Store::open(root).map_err(internal)?.find(&id).map_err(|_| {
            ApiError(
                StatusCode::NOT_FOUND,
                "This datasource no longer exists".into(),
            )
        })
    })
    .await
    .map_err(internal)?
}

async fn datasource_details(State(app): State<Local>, Path(id): Path<String>) -> ApiResult {
    Ok(Json(load_datasource(&app, &id).await?.public()))
}

async fn edit_datasource(State(app): State<Local>, Path(id): Path<String>) -> ApiResult {
    let mut source = load_datasource(&app, &id).await?;
    // Capture fields and revision together, so a concurrent CLI edit cannot be overwritten.
    let baseline = fingerprint(&source);
    source.connection.password.clear();
    Ok(register_setup(
        &app,
        SetupRequest {
            name: source.name,
            source_id: Some(source.id),
            connection: source.connection,
        },
        Some(baseline),
    ))
}

async fn test_datasource(State(app): State<Local>, Path(id): Path<String>) -> ApiResult {
    let source = load_datasource(&app, &id).await?;
    app.active.fetch_add(1, Ordering::Relaxed);
    let _active = Active(app.clone());
    let started = Instant::now();
    test_connection(&app, source).await?;
    Ok(Json(
        json!({"connected":true,"duration_ms":started.elapsed().as_millis()}),
    ))
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
    Ok(register_setup(&app, request, baseline))
}

fn register_setup(app: &App, request: SetupRequest, baseline: Option<String>) -> Json<Value> {
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
    Json(status)
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
    test_connection(&app, source.clone()).await?;
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

async fn test_connection(app: &App, source: Datasource) -> std::result::Result<(), ApiError> {
    let root = app.root.clone();
    let manifest = app.manifest.clone();
    let local = app.workers.clone();
    let prepared = tokio::task::spawn_blocking(move || {
        execution::prepare(root, manifest, local, source, Action::Test, vec![])
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
    Ok(())
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
                &app.root.join("results"),
                &id,
                source.id.clone(),
                source.name.clone(),
                request.statements.clone(),
                sqlx_core::results::PAGE_ORIGIN,
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
        let outcome = execute_query(&work, source, request.statements, cancel.clone(), |event| {
            result.lock().unwrap().record(event)
        })
        .await;
        let success = outcome.as_ref().is_ok_and(|s| *s);
        let error = outcome.err().map(|_| {
            "Execution could not be completed. No automatic retry was attempted.".to_string()
        });
        // Remove this run's cancellation handle before publishing its terminal status.
        work.cancellations.lock().unwrap().remove(&id);
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
    });
    Ok(Json(
        json!({"result_id":request.request_id,"status":"queued"}),
    ))
}
async fn execute_query(
    app: &App,
    source: Datasource,
    statements: Vec<String>,
    cancel: CancellationToken,
    record: impl FnMut(Event) -> Result<()> + Send,
) -> Result<bool> {
    let root = app.root.clone();
    let manifest = app.manifest.clone();
    let local = app.workers.clone();
    let prepared = tokio::task::spawn_blocking(move || {
        execution::prepare(root, manifest, local, source, Action::Execute, statements)
    })
    .await??;
    if cancel.is_cancelled() {
        anyhow::bail!("Query was cancelled before execution");
    }
    prepared.execute(record, cancel).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RefreshRequest {
    request_id: String,
}
async fn refresh_result(
    State(app): State<Local>,
    Path(id): Path<String>,
    Json(request): Json<RefreshRequest>,
) -> ApiResult {
    validate_id(&request.request_id)?;
    let result = app.result(&id)?;
    let source_id = result.lock().unwrap().metadata.datasource_id.clone();
    let source = load_datasource(&app, &source_id).await?;
    let mut staged = {
        let mut previous = result.lock().unwrap();
        if let Some(refresh) = previous
            .refresh_record(&request.request_id)
            .map_err(internal)?
        {
            return Ok(Json(serde_json::to_value(refresh).unwrap()));
        }
        if let Some(refresh) = &previous.metadata.refresh {
            if refresh.status == "running" {
                return Err(conflict("A refresh is already running"));
            }
        }
        if matches!(previous.metadata.status.as_str(), "queued" | "running") {
            return Err(conflict("Wait for the current query to finish"));
        }
        previous
            .begin_refresh(&request.request_id, source.name.clone())
            .map_err(internal)?
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
        let started = Instant::now();
        let statements = staged.metadata.statements.clone();
        let outcome = execute_query(&work, source, statements, cancel.clone(), |event| {
            staged.record(event)
        })
        .await;
        let success = outcome.is_ok_and(|ok| ok) && !cancel.is_cancelled();
        let finished = staged.finish(
            success,
            cancel.is_cancelled(),
            started.elapsed().as_millis() as u64,
            None,
        );
        work.cancellations.lock().unwrap().remove(&id);
        let mut previous = result.lock().unwrap();
        if success && finished.is_ok() {
            if previous.commit_refresh(staged).is_err() {
                let _ = previous.fail_refresh(
                    "failed",
                    "Could not save the refreshed result. The previous result is preserved.".into(),
                );
            }
        } else {
            let error = staged
                .metadata
                .events
                .iter()
                .find_map(|event| {
                    (event["event"] == "error")
                        .then(|| event["message"].as_str())
                        .flatten()
                })
                .unwrap_or("Refresh did not complete. The previous result is preserved.")
                .to_owned();
            let state = if cancel.is_cancelled() {
                "cancelled"
            } else {
                "failed"
            };
            let _ = previous.fail_refresh(state, error);
            staged.discard_refresh();
        }
    });
    Ok(Json(
        json!({"request_id":request.request_id,"status":"running","error":null}),
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
    snapshot: Option<String>,
}
fn page_limit() -> usize {
    100
}
async fn result_page(
    State(app): State<Local>,
    Path(id): Path<String>,
    Query(query): Query<PageQuery>,
) -> std::result::Result<Json<Page>, ApiError> {
    let result = app.result(&id)?;
    let page = tokio::task::spawn_blocking(move || {
        let result = result.lock().unwrap();
        if query
            .snapshot
            .as_deref()
            .is_some_and(|s| s != result.metadata.snapshot.as_deref().unwrap_or("initial"))
        {
            anyhow::bail!("The result changed. Reload the result page.");
        }
        result.page(query.statement, query.result, query.offset, query.limit)
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
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/dist");
        let plugin = plugins::install(dir.path(), &source).unwrap();
        plugins::activate(dir.path(), &plugin.id, Some(&plugin.version)).unwrap();
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
    async fn result_routes_reject_edits_and_filter_clauses() {
        let (_dir, app) = fixture();
        let session = token();
        app.sessions
            .lock()
            .unwrap()
            .insert(session.clone(), now() + SESSION_TTL);
        let cookie = format!("{}={session}", app.cookie_name());
        for (path, body, expected) in [
            ("/api/results/test/edit", json!({}), StatusCode::NOT_FOUND),
            ("/api/results/test/edits", json!({}), StatusCode::NOT_FOUND),
            (
                "/api/results/test/edits/test",
                json!({}),
                StatusCode::NOT_FOUND,
            ),
            (
                "/api/results/test/refresh",
                json!({"request_id":uuid::Uuid::new_v4().to_string(),"filter":{"where_clause":"1=0","order_by":"id DESC"}}),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
        ] {
            let response = router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri(path)
                        .header("Host", "127.0.0.1:32145")
                        .header("Origin", &app.state.origin)
                        .header("X-SQLX-UI", "1")
                        .header("Cookie", &cookie)
                        .header("Content-Type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{path}");
        }
    }

    #[tokio::test]
    async fn local_page_bootstraps_a_session_without_a_launch_token() {
        for path in ["/", "/result/test"] {
            let (_dir, app) = fixture();
            let response = router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri(path)
                        .header("Host", "127.0.0.1:32145")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let cookie = response.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned();
            assert!(cookie.starts_with(&format!("{}=", app.cookie_name())));

            let home = router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/api/home")
                        .header("Host", "127.0.0.1:32145")
                        .header("X-SQLX-UI", "1")
                        .header("Cookie", cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(home.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn local_page_rejects_a_foreign_host_without_bootstrapping() {
        let (_dir, app) = fixture();
        let response = router(app)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .header("Host", "attacker.example:32145")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(!response.headers().contains_key(header::SET_COOKIE));
    }
    #[tokio::test]
    async fn authenticated_activity_renews_the_browser_session_but_expiry_does_not() {
        let (_dir, app) = fixture();
        let session = token();
        app.sessions
            .lock()
            .unwrap()
            .insert(session.clone(), now() + 1);
        let request = || {
            axum::http::Request::builder()
                .uri("/api/home")
                .header("Host", "127.0.0.1:32145")
                .header("X-SQLX-UI", "1")
                .header("Cookie", format!("{}={session}", app.cookie_name()))
                .body(Body::empty())
                .unwrap()
        };
        let requested_at = now();
        let response = router(app.clone()).oneshot(request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let renewed = response.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(renewed.contains("Max-Age=43200"));
        assert!(renewed.contains("HttpOnly; SameSite=Strict"));
        assert!(app.sessions.lock().unwrap()[&session] >= requested_at + SESSION_TTL);
        app.sessions
            .lock()
            .unwrap()
            .insert(session.clone(), now() - 1);
        let response = router(app.clone()).oneshot(request()).await.unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(!response.headers().contains_key(header::SET_COOKIE));
    }
    #[tokio::test]
    async fn browser_session_cannot_control_admin_endpoints_or_exchange_launch_tokens() {
        let (_dir, app) = fixture();
        let response = router(app.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .header("Host", "127.0.0.1:32145")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
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
        for path in ["/api/session", "/api/tickets"] {
            let response = router(app.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri(path)
                        .header("Host", "127.0.0.1:32145")
                        .header("Origin", &app.state.origin)
                        .header("X-SQLX-UI", "1")
                        .header("Cookie", &cookie)
                        .header("Content-Type", "application/json")
                        .body(Body::from(json!({"token":"old-launch-token"}).to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }
}
