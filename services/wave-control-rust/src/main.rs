use anyhow::Context;
use axum::Json;
use axum::Router;
use axum::extract::Path as AxumPath;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use sqlx::PgPool;
use sqlx::Row;
use sqlx::postgres::PgPoolOptions;
use sqlx::types::Json as SqlJson;
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use wave_app_server::load_operator_snapshot;
use wave_config::ProjectConfig;
use wave_coordination::CoordinationLog;
use wave_coordination::CoordinationRecord;
use wave_domain::ResultEnvelope;
use wave_events::ControlEvent;
use wave_events::ControlEventLog;
use wave_results::ResultEnvelopeStore;

#[derive(Debug, Clone)]
struct ServiceConfig {
    bind_addr: SocketAddr,
    auth_token: Option<String>,
    data_dir: PathBuf,
    repo_root: Option<PathBuf>,
    config_path: Option<PathBuf>,
    default_project_key: String,
    orchestrator_id: String,
    runtime_version: String,
}

#[derive(Clone)]
struct AppState {
    config: Arc<ServiceConfig>,
    store: MirrorStore,
    write_lock: Arc<Mutex<()>>,
}

#[derive(Clone)]
enum MirrorStore {
    File,
    Postgres(PgPool),
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    service: &'static str,
    status: &'static str,
    auth_required: bool,
    live_repo_mode: bool,
    repo_root: Option<String>,
    storage_mode: &'static str,
    default_project_key: String,
    orchestrator_id: String,
    runtime_version: String,
    data_dir: String,
    snapshot_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MirrorSnapshotEnvelope {
    received_at_ms: u128,
    snapshot: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ControlEventBatch {
    events: Vec<ControlEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoordinationRecordBatch {
    records: Vec<CoordinationRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResultEnvelopeBatch {
    envelopes: Vec<ResultEnvelope>,
}

#[derive(Debug, Serialize)]
struct MirrorWriteResponse {
    stored_at_ms: u128,
    path: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: "missing or invalid bearer token".to_string(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Arc::new(ServiceConfig::from_env()?);
    let store = MirrorStore::from_config(&config).await?;
    let state = AppState {
        config,
        store,
        write_lock: Arc::new(Mutex::new(())),
    };

    let app = Router::new()
        .route("/healthz", get(health))
        .route("/v1/operator/snapshot", get(get_operator_snapshot).post(post_operator_snapshot))
        .route(
            "/v1/control-events/:wave_id",
            get(get_control_events).post(post_control_events),
        )
        .route(
            "/v1/coordination/:wave_id",
            get(get_coordination_records).post(post_coordination_records),
        )
        .route(
            "/v1/result-envelopes/:wave_id",
            get(get_result_envelopes).post(post_result_envelopes),
        )
        .with_state(state.clone());

    let listener = TcpListener::bind(state.config.bind_addr)
        .await
        .with_context(|| format!("failed to bind {}", state.config.bind_addr))?;
    println!("wave-control-rust listening on {}", state.config.bind_addr);
    axum::serve(listener, app)
        .await
        .context("wave-control-rust server failed")?;
    Ok(())
}

impl ServiceConfig {
    fn from_env() -> anyhow::Result<Self> {
        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(3000);
        let bind_addr = format!("{host}:{port}")
            .parse::<SocketAddr>()
            .with_context(|| format!("invalid bind address {host}:{port}"))?;
        let data_dir = env::var("WAVE_CONTROL_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data"));
        let repo_root = env::var("WAVE_REPO_ROOT").ok().map(PathBuf::from);
        let config_path = match env::var("WAVE_CONFIG_PATH") {
            Ok(path) => Some(PathBuf::from(path)),
            Err(_) => repo_root.as_ref().map(|root| root.join("wave.toml")),
        };

        Ok(Self {
            bind_addr,
            auth_token: env::var("WAVE_CONTROL_API_TOKEN").ok(),
            data_dir,
            repo_root,
            config_path,
            default_project_key: env::var("WAVE_CONTROL_PROJECT_KEY")
                .unwrap_or_else(|_| "default".to_string()),
            orchestrator_id: env::var("WAVE_CONTROL_ORCHESTRATOR_ID")
                .unwrap_or_else(|_| "rust-wave".to_string()),
            runtime_version: env::var("WAVE_CONTROL_RUNTIME_VERSION")
                .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string()),
        })
    }
}

impl MirrorStore {
    async fn from_config(_config: &ServiceConfig) -> anyhow::Result<Self> {
        let Some(database_url) = env::var("DATABASE_URL").ok() else {
            return Ok(Self::File);
        };

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .context("failed to connect to DATABASE_URL")?;
        ensure_schema(&pool).await?;
        Ok(Self::Postgres(pool))
    }

    fn mode(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Postgres(_) => "postgres",
        }
    }
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let project_key = &state.config.default_project_key;
    Json(HealthResponse {
        service: "wave-control-rust",
        status: "ok",
        auth_required: state.config.auth_token.is_some(),
        live_repo_mode: state.config.repo_root.is_some(),
        repo_root: state
            .config
            .repo_root
            .as_ref()
            .map(|path| path.display().to_string()),
        storage_mode: state.store.mode(),
        default_project_key: project_key.clone(),
        orchestrator_id: state.config.orchestrator_id.clone(),
        runtime_version: state.config.runtime_version.clone(),
        data_dir: state.config.data_dir.display().to_string(),
        snapshot_path: snapshot_path(&state.config, project_key).display().to_string(),
    })
}

async fn get_operator_snapshot(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    authorize(&headers, &state.config)?;
    let project_key = project_key(&headers, &state.config);
    if let Some(snapshot) = load_live_snapshot(&state.config)? {
        return Ok(Json(snapshot));
    }
    if let Some(snapshot) = load_mirrored_snapshot(&state, project_key.as_str()).await? {
        return Ok(Json(snapshot));
    }
    Err(ApiError::not_found(
        "no live or mirrored operator snapshot is available",
    ))
}

async fn post_operator_snapshot(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(snapshot): Json<Value>,
) -> Result<Json<MirrorWriteResponse>, ApiError> {
    authorize(&headers, &state.config)?;
    let project_key = project_key(&headers, &state.config);
    let stored_at_ms = now_epoch_ms().map_err(ApiError::internal)?;
    let path = store_snapshot(&state, project_key.as_str(), stored_at_ms, snapshot).await?;
    Ok(Json(MirrorWriteResponse { stored_at_ms, path }))
}

async fn get_control_events(
    headers: HeaderMap,
    State(state): State<AppState>,
    AxumPath(wave_id): AxumPath<u32>,
) -> Result<Json<ControlEventBatch>, ApiError> {
    authorize(&headers, &state.config)?;
    let project_key = project_key(&headers, &state.config);
    if let Some(events) = load_live_control_events(&state.config, wave_id)? {
        return Ok(Json(ControlEventBatch { events }));
    }
    if let Some(events) = load_mirrored_control_events(&state, project_key.as_str(), wave_id).await?
    {
        return Ok(Json(ControlEventBatch { events }));
    }
    Err(ApiError::not_found(format!(
        "no control events found for wave {wave_id}"
    )))
}

async fn post_control_events(
    headers: HeaderMap,
    State(state): State<AppState>,
    AxumPath(wave_id): AxumPath<u32>,
    Json(batch): Json<ControlEventBatch>,
) -> Result<Json<MirrorWriteResponse>, ApiError> {
    authorize(&headers, &state.config)?;
    if batch.events.iter().any(|event| event.wave_id != wave_id) {
        return Err(ApiError::bad_request(
            "all mirrored control events must match the requested wave_id",
        ));
    }
    let stored_at_ms = now_epoch_ms().map_err(ApiError::internal)?;
    let project_key = project_key(&headers, &state.config);
    let path = store_control_events(&state, project_key.as_str(), wave_id, batch.events).await?;
    Ok(Json(MirrorWriteResponse { stored_at_ms, path }))
}

async fn get_coordination_records(
    headers: HeaderMap,
    State(state): State<AppState>,
    AxumPath(wave_id): AxumPath<u32>,
) -> Result<Json<CoordinationRecordBatch>, ApiError> {
    authorize(&headers, &state.config)?;
    let project_key = project_key(&headers, &state.config);
    if let Some(records) = load_live_coordination_records(&state.config, wave_id)? {
        return Ok(Json(CoordinationRecordBatch { records }));
    }
    if let Some(records) =
        load_mirrored_coordination_records(&state, project_key.as_str(), wave_id).await?
    {
        return Ok(Json(CoordinationRecordBatch { records }));
    }
    Err(ApiError::not_found(format!(
        "no coordination records found for wave {wave_id}"
    )))
}

async fn post_coordination_records(
    headers: HeaderMap,
    State(state): State<AppState>,
    AxumPath(wave_id): AxumPath<u32>,
    Json(batch): Json<CoordinationRecordBatch>,
) -> Result<Json<MirrorWriteResponse>, ApiError> {
    authorize(&headers, &state.config)?;
    if batch.records.iter().any(|record| record.wave_id != wave_id) {
        return Err(ApiError::bad_request(
            "all mirrored coordination records must match the requested wave_id",
        ));
    }
    let stored_at_ms = now_epoch_ms().map_err(ApiError::internal)?;
    let project_key = project_key(&headers, &state.config);
    let path =
        store_coordination_records(&state, project_key.as_str(), wave_id, batch.records).await?;
    Ok(Json(MirrorWriteResponse { stored_at_ms, path }))
}

async fn get_result_envelopes(
    headers: HeaderMap,
    State(state): State<AppState>,
    AxumPath(wave_id): AxumPath<u32>,
) -> Result<Json<ResultEnvelopeBatch>, ApiError> {
    authorize(&headers, &state.config)?;
    let project_key = project_key(&headers, &state.config);
    if let Some(envelopes) = load_live_result_envelopes(&state.config, wave_id)? {
        return Ok(Json(ResultEnvelopeBatch { envelopes }));
    }
    if let Some(envelopes) =
        load_mirrored_result_envelopes(&state, project_key.as_str(), wave_id).await?
    {
        return Ok(Json(ResultEnvelopeBatch { envelopes }));
    }
    Err(ApiError::not_found(format!(
        "no result envelopes found for wave {wave_id}"
    )))
}

async fn post_result_envelopes(
    headers: HeaderMap,
    State(state): State<AppState>,
    AxumPath(wave_id): AxumPath<u32>,
    Json(batch): Json<ResultEnvelopeBatch>,
) -> Result<Json<MirrorWriteResponse>, ApiError> {
    authorize(&headers, &state.config)?;
    if batch.envelopes.iter().any(|envelope| envelope.wave_id != wave_id) {
        return Err(ApiError::bad_request(
            "all mirrored result envelopes must match the requested wave_id",
        ));
    }
    let stored_at_ms = now_epoch_ms().map_err(ApiError::internal)?;
    let project_key = project_key(&headers, &state.config);
    let path = store_result_envelopes(&state, project_key.as_str(), wave_id, batch.envelopes).await?;
    Ok(Json(MirrorWriteResponse { stored_at_ms, path }))
}

fn authorize(headers: &HeaderMap, config: &ServiceConfig) -> Result<(), ApiError> {
    let Some(expected) = config.auth_token.as_deref() else {
        return Ok(());
    };
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if provided == Some(expected) {
        Ok(())
    } else {
        Err(ApiError::unauthorized())
    }
}

fn project_key(headers: &HeaderMap, config: &ServiceConfig) -> String {
    headers
        .get("x-wave-project")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(config.default_project_key.as_str())
        .to_string()
}

fn load_live_snapshot(config: &ServiceConfig) -> Result<Option<Value>, ApiError> {
    let (Some(repo_root), Some(config_path)) = (&config.repo_root, &config.config_path) else {
        return Ok(None);
    };
    let project_config = ProjectConfig::load(config_path).map_err(ApiError::internal)?;
    let snapshot = load_operator_snapshot(repo_root, &project_config).map_err(ApiError::internal)?;
    serde_json::to_value(snapshot)
        .map(Some)
        .map_err(ApiError::internal)
}

fn load_live_control_events(
    config: &ServiceConfig,
    wave_id: u32,
) -> Result<Option<Vec<ControlEvent>>, ApiError> {
    let (Some(repo_root), Some(config_path)) = (&config.repo_root, &config.config_path) else {
        return Ok(None);
    };
    let project_config = ProjectConfig::load(config_path).map_err(ApiError::internal)?;
    let control_dir = project_config
        .resolved_paths(repo_root)
        .authority
        .state_events_control_dir;
    let log = ControlEventLog::new(control_dir);
    let events = log.load_wave(wave_id).map_err(ApiError::internal)?;
    if events.is_empty() {
        Ok(None)
    } else {
        Ok(Some(events))
    }
}

fn load_live_coordination_records(
    config: &ServiceConfig,
    wave_id: u32,
) -> Result<Option<Vec<CoordinationRecord>>, ApiError> {
    let (Some(repo_root), Some(config_path)) = (&config.repo_root, &config.config_path) else {
        return Ok(None);
    };
    let project_config = ProjectConfig::load(config_path).map_err(ApiError::internal)?;
    let coordination_dir = project_config
        .resolved_paths(repo_root)
        .authority
        .state_events_coordination_dir;
    let log = CoordinationLog::new(coordination_dir);
    let records = log.load_wave(wave_id).map_err(ApiError::internal)?;
    if records.is_empty() {
        Ok(None)
    } else {
        Ok(Some(records))
    }
}

fn load_live_result_envelopes(
    config: &ServiceConfig,
    wave_id: u32,
) -> Result<Option<Vec<ResultEnvelope>>, ApiError> {
    let (Some(repo_root), Some(config_path)) = (&config.repo_root, &config.config_path) else {
        return Ok(None);
    };
    let project_config = ProjectConfig::load(config_path).map_err(ApiError::internal)?;
    let results_dir = project_config.resolved_paths(repo_root).authority.state_results_dir;
    let store = ResultEnvelopeStore::new(results_dir);
    let envelopes = store.load_wave_envelopes(wave_id).map_err(ApiError::internal)?;
    if envelopes.is_empty() {
        Ok(None)
    } else {
        Ok(Some(envelopes))
    }
}

async fn load_mirrored_snapshot(
    state: &AppState,
    project_key: &str,
) -> Result<Option<Value>, ApiError> {
    match &state.store {
        MirrorStore::File => {
            let path = snapshot_path(&state.config, project_key);
            if !path.exists() {
                return Ok(None);
            }
            let envelope =
                read_json_file::<MirrorSnapshotEnvelope>(&path).map_err(ApiError::internal)?;
            Ok(Some(envelope.snapshot))
        }
        MirrorStore::Postgres(pool) => {
            let row =
                sqlx::query("SELECT snapshot FROM rust_wave_operator_snapshots WHERE project_key = $1")
                    .bind(project_key)
                    .fetch_optional(pool)
                    .await
                    .map_err(ApiError::internal)?;
            row.map(|row| row.try_get::<SqlJson<Value>, _>("snapshot").map(|value| value.0))
                .transpose()
                .map_err(ApiError::internal)
        }
    }
}

async fn load_mirrored_control_events(
    state: &AppState,
    project_key: &str,
    wave_id: u32,
) -> Result<Option<Vec<ControlEvent>>, ApiError> {
    match &state.store {
        MirrorStore::File => {
            let path = control_events_path(&state.config, project_key, wave_id);
            if !path.exists() {
                return Ok(None);
            }
            let batch = read_json_file::<ControlEventBatch>(&path).map_err(ApiError::internal)?;
            Ok(Some(batch.events))
        }
        MirrorStore::Postgres(pool) => {
            let rows = sqlx::query(
                "SELECT event FROM rust_wave_control_events
                 WHERE project_key = $1 AND wave_id = $2
                 ORDER BY created_at_ms ASC, event_id ASC",
            )
            .bind(project_key)
            .bind(i64::from(wave_id))
            .fetch_all(pool)
            .await
            .map_err(ApiError::internal)?;
            if rows.is_empty() {
                return Ok(None);
            }
            let mut events = Vec::with_capacity(rows.len());
            for row in rows {
                events.push(
                    row.try_get::<SqlJson<ControlEvent>, _>("event")
                        .map_err(ApiError::internal)?
                        .0,
                );
            }
            Ok(Some(events))
        }
    }
}

async fn load_mirrored_coordination_records(
    state: &AppState,
    project_key: &str,
    wave_id: u32,
) -> Result<Option<Vec<CoordinationRecord>>, ApiError> {
    match &state.store {
        MirrorStore::File => {
            let path = coordination_records_path(&state.config, project_key, wave_id);
            if !path.exists() {
                return Ok(None);
            }
            let batch =
                read_json_file::<CoordinationRecordBatch>(&path).map_err(ApiError::internal)?;
            Ok(Some(batch.records))
        }
        MirrorStore::Postgres(pool) => {
            let rows = sqlx::query(
                "SELECT record FROM rust_wave_coordination_records
                 WHERE project_key = $1 AND wave_id = $2
                 ORDER BY created_at_ms ASC, record_id ASC",
            )
            .bind(project_key)
            .bind(i64::from(wave_id))
            .fetch_all(pool)
            .await
            .map_err(ApiError::internal)?;
            if rows.is_empty() {
                return Ok(None);
            }
            let mut records = Vec::with_capacity(rows.len());
            for row in rows {
                records.push(
                    row.try_get::<SqlJson<CoordinationRecord>, _>("record")
                        .map_err(ApiError::internal)?
                        .0,
                );
            }
            Ok(Some(records))
        }
    }
}

async fn load_mirrored_result_envelopes(
    state: &AppState,
    project_key: &str,
    wave_id: u32,
) -> Result<Option<Vec<ResultEnvelope>>, ApiError> {
    match &state.store {
        MirrorStore::File => {
            let path = result_envelopes_path(&state.config, project_key, wave_id);
            if !path.exists() {
                return Ok(None);
            }
            let batch = read_json_file::<ResultEnvelopeBatch>(&path).map_err(ApiError::internal)?;
            Ok(Some(batch.envelopes))
        }
        MirrorStore::Postgres(pool) => {
            let rows = sqlx::query(
                "SELECT envelope FROM rust_wave_result_envelopes
                 WHERE project_key = $1 AND wave_id = $2
                 ORDER BY created_at_ms ASC, result_envelope_id ASC",
            )
            .bind(project_key)
            .bind(i64::from(wave_id))
            .fetch_all(pool)
            .await
            .map_err(ApiError::internal)?;
            if rows.is_empty() {
                return Ok(None);
            }
            let mut envelopes = Vec::with_capacity(rows.len());
            for row in rows {
                envelopes.push(
                    row.try_get::<SqlJson<ResultEnvelope>, _>("envelope")
                        .map_err(ApiError::internal)?
                        .0,
                );
            }
            Ok(Some(envelopes))
        }
    }
}

async fn store_snapshot(
    state: &AppState,
    project_key: &str,
    stored_at_ms: u128,
    snapshot: Value,
) -> Result<String, ApiError> {
    match &state.store {
        MirrorStore::File => {
            let path = snapshot_path(&state.config, project_key);
            let envelope = MirrorSnapshotEnvelope {
                received_at_ms: stored_at_ms,
                snapshot,
            };
            write_json_file(state, &path, &envelope).await?;
            Ok(path.display().to_string())
        }
        MirrorStore::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO rust_wave_operator_snapshots
                 (project_key, received_at_ms, orchestrator_id, runtime_version, snapshot)
                 VALUES ($1, $2, $3, $4, $5)
                 ON CONFLICT (project_key)
                 DO UPDATE SET
                    received_at_ms = EXCLUDED.received_at_ms,
                    orchestrator_id = EXCLUDED.orchestrator_id,
                    runtime_version = EXCLUDED.runtime_version,
                    snapshot = EXCLUDED.snapshot",
            )
            .bind(project_key)
            .bind(to_i64_epoch_ms(stored_at_ms)?)
            .bind(state.config.orchestrator_id.as_str())
            .bind(state.config.runtime_version.as_str())
            .bind(SqlJson(snapshot))
            .execute(pool)
            .await
            .map_err(ApiError::internal)?;
            Ok(format!("db://rust-wave-control/{project_key}/operator-snapshot"))
        }
    }
}

async fn store_control_events(
    state: &AppState,
    project_key: &str,
    wave_id: u32,
    events: Vec<ControlEvent>,
) -> Result<String, ApiError> {
    match &state.store {
        MirrorStore::File => {
            let mut merged = load_mirrored_control_events(state, project_key, wave_id)
                .await?
                .unwrap_or_default();
            merge_control_events(&mut merged, events);
            let path = control_events_path(&state.config, project_key, wave_id);
            write_json_file(state, &path, &ControlEventBatch { events: merged }).await?;
            Ok(path.display().to_string())
        }
        MirrorStore::Postgres(pool) => {
            let mut tx = pool.begin().await.map_err(ApiError::internal)?;
            for event in events {
                let event_id = event.event_id.clone();
                let correlation_id = event.correlation_id.clone();
                sqlx::query(
                    "INSERT INTO rust_wave_control_events
                     (project_key, wave_id, event_id, created_at_ms, correlation_id, orchestrator_id, runtime_version, event)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                     ON CONFLICT (project_key, event_id)
                     DO UPDATE SET
                        created_at_ms = EXCLUDED.created_at_ms,
                        correlation_id = EXCLUDED.correlation_id,
                        orchestrator_id = EXCLUDED.orchestrator_id,
                        runtime_version = EXCLUDED.runtime_version,
                        event = EXCLUDED.event",
                )
                .bind(project_key)
                .bind(i64::from(wave_id))
                .bind(event_id)
                .bind(to_i64_epoch_ms(event.created_at_ms)?)
                .bind(correlation_id)
                .bind(state.config.orchestrator_id.as_str())
                .bind(state.config.runtime_version.as_str())
                .bind(SqlJson(event))
                .execute(&mut *tx)
                .await
                .map_err(ApiError::internal)?;
            }
            tx.commit().await.map_err(ApiError::internal)?;
            Ok(format!("db://rust-wave-control/{project_key}/control-events/{wave_id}"))
        }
    }
}

async fn store_coordination_records(
    state: &AppState,
    project_key: &str,
    wave_id: u32,
    records: Vec<CoordinationRecord>,
) -> Result<String, ApiError> {
    match &state.store {
        MirrorStore::File => {
            let mut merged = load_mirrored_coordination_records(state, project_key, wave_id)
                .await?
                .unwrap_or_default();
            merge_coordination_records(&mut merged, records);
            let path = coordination_records_path(&state.config, project_key, wave_id);
            write_json_file(state, &path, &CoordinationRecordBatch { records: merged }).await?;
            Ok(path.display().to_string())
        }
        MirrorStore::Postgres(pool) => {
            let mut tx = pool.begin().await.map_err(ApiError::internal)?;
            for record in records {
                let record_id = record.record_id.clone();
                sqlx::query(
                    "INSERT INTO rust_wave_coordination_records
                     (project_key, wave_id, record_id, created_at_ms, orchestrator_id, runtime_version, record)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)
                     ON CONFLICT (project_key, record_id)
                     DO UPDATE SET
                        created_at_ms = EXCLUDED.created_at_ms,
                        orchestrator_id = EXCLUDED.orchestrator_id,
                        runtime_version = EXCLUDED.runtime_version,
                        record = EXCLUDED.record",
                )
                .bind(project_key)
                .bind(i64::from(wave_id))
                .bind(record_id)
                .bind(to_i64_epoch_ms(record.created_at_ms)?)
                .bind(state.config.orchestrator_id.as_str())
                .bind(state.config.runtime_version.as_str())
                .bind(SqlJson(record))
                .execute(&mut *tx)
                .await
                .map_err(ApiError::internal)?;
            }
            tx.commit().await.map_err(ApiError::internal)?;
            Ok(format!("db://rust-wave-control/{project_key}/coordination/{wave_id}"))
        }
    }
}

async fn store_result_envelopes(
    state: &AppState,
    project_key: &str,
    wave_id: u32,
    envelopes: Vec<ResultEnvelope>,
) -> Result<String, ApiError> {
    match &state.store {
        MirrorStore::File => {
            let mut merged = load_mirrored_result_envelopes(state, project_key, wave_id)
                .await?
                .unwrap_or_default();
            merge_result_envelopes(&mut merged, envelopes);
            let path = result_envelopes_path(&state.config, project_key, wave_id);
            write_json_file(state, &path, &ResultEnvelopeBatch { envelopes: merged }).await?;
            Ok(path.display().to_string())
        }
        MirrorStore::Postgres(pool) => {
            let mut tx = pool.begin().await.map_err(ApiError::internal)?;
            for envelope in envelopes {
                let result_envelope_id = envelope.result_envelope_id.clone();
                let task_id = envelope.task_id.clone();
                let attempt_id = envelope.attempt_id.clone();
                let agent_id = envelope.agent_id.clone();
                sqlx::query(
                    "INSERT INTO rust_wave_result_envelopes
                     (project_key, wave_id, result_envelope_id, task_id, attempt_id, agent_id, created_at_ms, orchestrator_id, runtime_version, envelope)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                     ON CONFLICT (project_key, result_envelope_id)
                     DO UPDATE SET
                        task_id = EXCLUDED.task_id,
                        attempt_id = EXCLUDED.attempt_id,
                        agent_id = EXCLUDED.agent_id,
                        created_at_ms = EXCLUDED.created_at_ms,
                        orchestrator_id = EXCLUDED.orchestrator_id,
                        runtime_version = EXCLUDED.runtime_version,
                        envelope = EXCLUDED.envelope",
                )
                .bind(project_key)
                .bind(i64::from(wave_id))
                .bind(result_envelope_id.as_str())
                .bind(task_id.as_str())
                .bind(attempt_id.as_str())
                .bind(agent_id.as_str())
                .bind(to_i64_epoch_ms(envelope.created_at_ms)?)
                .bind(state.config.orchestrator_id.as_str())
                .bind(state.config.runtime_version.as_str())
                .bind(SqlJson(envelope))
                .execute(&mut *tx)
                .await
                .map_err(ApiError::internal)?;
            }
            tx.commit().await.map_err(ApiError::internal)?;
            Ok(format!("db://rust-wave-control/{project_key}/result-envelopes/{wave_id}"))
        }
    }
}

fn snapshot_path(config: &ServiceConfig, project_key: &str) -> PathBuf {
    project_dir(config, project_key)
        .join("mirror")
        .join("operator-snapshot.json")
}

fn control_events_path(config: &ServiceConfig, project_key: &str, wave_id: u32) -> PathBuf {
    project_dir(config, project_key)
        .join("mirror")
        .join("control-events")
        .join(format!("wave-{wave_id:02}.json"))
}

fn coordination_records_path(
    config: &ServiceConfig,
    project_key: &str,
    wave_id: u32,
) -> PathBuf {
    project_dir(config, project_key)
        .join("mirror")
        .join("coordination")
        .join(format!("wave-{wave_id:02}.json"))
}

fn result_envelopes_path(config: &ServiceConfig, project_key: &str, wave_id: u32) -> PathBuf {
    project_dir(config, project_key)
        .join("mirror")
        .join("result-envelopes")
        .join(format!("wave-{wave_id:02}.json"))
}

fn project_dir(config: &ServiceConfig, project_key: &str) -> PathBuf {
    config.data_dir.join(sanitize_project_key(project_key))
}

fn sanitize_project_key(project_key: &str) -> String {
    let mut sanitized = String::with_capacity(project_key.len());
    for ch in project_key.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    if sanitized.is_empty() {
        "default".to_string()
    } else {
        sanitized
    }
}

async fn write_json_file<T: Serialize>(
    state: &AppState,
    path: &Path,
    payload: &T,
) -> Result<(), ApiError> {
    let _guard = state.write_lock.lock().await;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(ApiError::internal)?;
    }
    let raw = serde_json::to_vec_pretty(payload).map_err(ApiError::internal)?;
    fs::write(path, raw).map_err(ApiError::internal)
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> anyhow::Result<T> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn merge_control_events(existing: &mut Vec<ControlEvent>, incoming: Vec<ControlEvent>) {
    for event in incoming {
        if existing.iter().any(|candidate| candidate.event_id == event.event_id) {
            continue;
        }
        existing.push(event);
    }
    existing.sort_by_key(|event| (event.created_at_ms, event.event_id.clone()));
}

fn merge_coordination_records(
    existing: &mut Vec<CoordinationRecord>,
    incoming: Vec<CoordinationRecord>,
) {
    for record in incoming {
        if existing
            .iter()
            .any(|candidate| candidate.record_id == record.record_id)
        {
            continue;
        }
        existing.push(record);
    }
    existing.sort_by_key(|record| (record.created_at_ms, record.record_id.clone()));
}

fn merge_result_envelopes(existing: &mut Vec<ResultEnvelope>, incoming: Vec<ResultEnvelope>) {
    for envelope in incoming {
        if existing
            .iter()
            .any(|candidate| candidate.result_envelope_id == envelope.result_envelope_id)
        {
            continue;
        }
        existing.push(envelope);
    }
    existing.sort_by_key(|envelope| {
        (
            envelope.created_at_ms,
            envelope.result_envelope_id.as_str().to_string(),
        )
    });
}

fn to_i64_epoch_ms(value: u128) -> Result<i64, ApiError> {
    i64::try_from(value).map_err(|_| ApiError::bad_request("timestamp exceeds i64 range"))
}

async fn ensure_schema(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS rust_wave_operator_snapshots (
            project_key TEXT PRIMARY KEY,
            received_at_ms BIGINT NOT NULL,
            orchestrator_id TEXT NOT NULL,
            runtime_version TEXT NOT NULL,
            snapshot JSONB NOT NULL
        )",
    )
    .execute(pool)
    .await
    .context("failed to create rust_wave_operator_snapshots")?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS rust_wave_control_events (
            project_key TEXT NOT NULL,
            wave_id BIGINT NOT NULL,
            event_id TEXT NOT NULL,
            created_at_ms BIGINT NOT NULL,
            correlation_id TEXT,
            orchestrator_id TEXT NOT NULL,
            runtime_version TEXT NOT NULL,
            event JSONB NOT NULL,
            PRIMARY KEY (project_key, event_id)
        )",
    )
    .execute(pool)
    .await
    .context("failed to create rust_wave_control_events")?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS rust_wave_control_events_wave_idx
         ON rust_wave_control_events (project_key, wave_id, created_at_ms, event_id)",
    )
    .execute(pool)
    .await
    .context("failed to create rust_wave_control_events_wave_idx")?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS rust_wave_coordination_records (
            project_key TEXT NOT NULL,
            wave_id BIGINT NOT NULL,
            record_id TEXT NOT NULL,
            created_at_ms BIGINT NOT NULL,
            orchestrator_id TEXT NOT NULL,
            runtime_version TEXT NOT NULL,
            record JSONB NOT NULL,
            PRIMARY KEY (project_key, record_id)
        )",
    )
    .execute(pool)
    .await
    .context("failed to create rust_wave_coordination_records")?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS rust_wave_coordination_records_wave_idx
         ON rust_wave_coordination_records (project_key, wave_id, created_at_ms, record_id)",
    )
    .execute(pool)
    .await
    .context("failed to create rust_wave_coordination_records_wave_idx")?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS rust_wave_result_envelopes (
            project_key TEXT NOT NULL,
            wave_id BIGINT NOT NULL,
            result_envelope_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            attempt_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            created_at_ms BIGINT NOT NULL,
            orchestrator_id TEXT NOT NULL,
            runtime_version TEXT NOT NULL,
            envelope JSONB NOT NULL,
            PRIMARY KEY (project_key, result_envelope_id)
        )",
    )
    .execute(pool)
    .await
    .context("failed to create rust_wave_result_envelopes")?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS rust_wave_result_envelopes_wave_idx
         ON rust_wave_result_envelopes (project_key, wave_id, created_at_ms, result_envelope_id)",
    )
    .execute(pool)
    .await
    .context("failed to create rust_wave_result_envelopes_wave_idx")?;

    Ok(())
}

fn now_epoch_ms() -> anyhow::Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the unix epoch")?
        .as_millis())
}
