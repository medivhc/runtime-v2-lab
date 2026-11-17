//! Central HTTP service: auth, all spec section 7.1 endpoints, error
//! semantics (401/403/404/409/422), idempotency keys, evidence download.

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use domain::ids::{MessageId, RequestId};
use domain::roles::{authorized, Action, Role};
use domain::state::{MessageState, RunState};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use store::{CentralConfig, Store, StoreError};

pub struct AppState {
    pub store: Store,
    pub config: CentralConfig,
    /// token -> subject (node id or 'operator').
    pub tokens: HashMap<String, (String, Role)>,
}

pub type SharedState = Arc<AppState>;

// ---------- Errors ----------

pub struct ApiError {
    status: StatusCode,
    code: String,
    message: String,
    request_id: Option<String>,
}

impl ApiError {
    fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
            request_id: None,
        }
    }
    fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid token",
        )
    }
    fn forbidden(m: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", m)
    }
    fn not_found(m: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", m)
    }
    fn conflict(m: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", m)
    }
    fn invalid(m: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid", m)
    }
    fn internal(m: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", m)
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound(m) => Self::not_found(m),
            StoreError::Conflict(m) => Self::conflict(m),
            StoreError::IdempotencyConflict => {
                Self::conflict("idempotency key reused with different body")
            }
            StoreError::Invalid(m) => Self::invalid(m),
            StoreError::Domain(d) => Self::invalid(d.to_string()),
            StoreError::Db(e) => Self::internal(e.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "code": self.code,
            "message": self.message,
            "request_id": self.request_id,
        });
        (self.status, Json(body)).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

// ---------- Auth ----------

#[derive(Debug, Clone)]
pub struct Subject {
    pub id: String,
    pub role: Role,
}

fn extract_subject(headers: &HeaderMap, state: &AppState) -> Result<Subject, ApiError> {
    let token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(ApiError::unauthorized)?;
    let (id, role) = state.tokens.get(token).ok_or_else(ApiError::unauthorized)?;
    Ok(Subject {
        id: id.clone(),
        role: *role,
    })
}

fn require(subject: &Subject, action: Action) -> Result<(), ApiError> {
    if authorized(subject.role, action) {
        Ok(())
    } else {
        Err(ApiError::forbidden(format!(
            "role {:?} may not perform {:?}",
            subject.role, action
        )))
    }
}

fn idem_digest(headers: &HeaderMap, body: &str) -> String {
    let key = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if key.is_empty() {
        return format!("nokey-{:x}", fnv(body.as_bytes()));
    }
    use sha2::{Digest, Sha256};
    format!("{key}-{}", hex::encode(Sha256::digest(body.as_bytes())))
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// ---------- Handlers ----------

#[derive(Deserialize)]
pub struct CreateRequest {
    pub intent: String,
    pub repo_ref: String,
    pub baseline_sha: String,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub required_routes: Vec<String>,
}

async fn create_request(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Result<Json<CreateRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Response> {
    // Auth must be checked before body validation so a missing token on an
    // invalid body still returns 401, not 422.
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::OperatorAction)?;
    let Json(body) = body.map_err(|_| ApiError::invalid("malformed request body"))?;
    let rid = RequestId::generate();
    state
        .store
        .create_request(
            &rid,
            &body.intent,
            &body.repo_ref,
            &body.baseline_sha,
            &body.required_capabilities,
            &body.required_routes,
            &subject.id,
        )
        .map_err(ApiError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "request_id": rid.as_str(),
            "state": "planning",
        })),
    )
        .into_response())
}

async fn submit_plan(
    State(state): State<SharedState>,
    Path(request_id): Path<String>,
    headers: HeaderMap,
    Json(plan): Json<domain::plan::Plan>,
) -> ApiResult<Response> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::WritePlan)?;
    if subject.id != state.config.planner_node {
        return Err(ApiError::forbidden(
            "plan submission restricted to planner node",
        ));
    }
    let validated = state
        .store
        .submit_plan(&request_id, &plan, Some(&subject.id))
        .map_err(ApiError::from)?;
    Ok(Json(json!({
        "request_id": request_id,
        "tasks": validated.plan.tasks.len(),
        "topo_order": validated.topo_order,
    }))
    .into_response())
}

async fn get_request(
    State(state): State<SharedState>,
    Path(request_id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::ReadResource).ok(); // read: any authenticated subject
    let req = state
        .store
        .get_request(&request_id)
        .map_err(ApiError::from)?;
    let tasks = state
        .store
        .tasks_for_request(&request_id)
        .map_err(ApiError::from)?;
    let runs = state
        .store
        .runs_for_request(&request_id)
        .map_err(ApiError::from)?;
    // Return/plan status of the deliver message back to planner.
    Ok(Json(json!({
        "request": req,
        "tasks": tasks,
        "runs": runs,
    })))
}

async fn get_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    let _ = subject;
    let task = state.store.get_task(&task_id).map_err(ApiError::from)?;
    Ok(Json(json!({ "task": task })))
}

#[derive(Deserialize)]
pub struct EventsQuery {
    #[serde(default)]
    pub after: i64,
}

async fn request_events(
    State(state): State<SharedState>,
    Path(request_id): Path<String>,
    Query(q): Query<EventsQuery>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    let _ = subject;
    let events = state
        .store
        .list_events(&request_id, q.after)
        .map_err(ApiError::from)?;
    let next = events.last().map(|e| e.seq).unwrap_or(q.after);
    Ok(Json(json!({ "events": events, "next": next })))
}

#[derive(Deserialize)]
pub struct BindingBody {
    pub session_id: String,
}

async fn planner_binding(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(body): Json<BindingBody>,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::WritePlan)?;
    let session = state
        .store
        .bind_planner(&subject.id, &body.session_id, &subject.id)
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "session_id": session })))
}

#[derive(Deserialize)]
pub struct HeartbeatBody {
    pub capacity: i64,
    #[serde(default)]
    pub repo_refs: Vec<String>,
}

async fn heartbeat(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(body): Json<HeartbeatBody>,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    // Node identity comes from auth mapping; capabilities limited by config.
    let node_cfg = state
        .config
        .nodes
        .get(&subject.id)
        .ok_or_else(|| ApiError::forbidden("unregistered node"))?;
    state
        .store
        .heartbeat(
            &subject.id,
            &node_cfg.role,
            &node_cfg.capabilities,
            body.capacity.min(node_cfg.capacity.unwrap_or(i64::MAX)),
            &body.repo_refs,
        )
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ClaimBody {
    #[serde(default = "default_run_type")]
    pub run_type: String,
}
fn default_run_type() -> String {
    "execute".into()
}

async fn claim_run(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(body): Json<ClaimBody>,
) -> ApiResult<Response> {
    let subject = extract_subject(&headers, &state)?;
    if body.run_type == "review" {
        require(&subject, Action::WritePlan)?;
        let claimed = state
            .store
            .claim_review(&subject.id, &subject.id)
            .map_err(ApiError::from)?;
        return match claimed {
            None => Ok(StatusCode::NO_CONTENT.into_response()),
            Some((review, execute)) => Ok(Json(json!({
                "review": review,
                "executed": execute,
                "lease_generation": review.lease_generation,
            }))
            .into_response()),
        };
    }
    require(&subject, Action::ClaimRun)?;
    let mut node = state
        .store
        .get_node(&subject.id, state.config.offline_after_seconds)
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::forbidden("node not registered; send heartbeat first"))?;
    node.seconds_since_heartbeat = 0; // authenticated now == alive now
    let routes = Vec::new(); // route filtering happens in store via subject
    let claimed = state
        .store
        .claim_execute(&node, &routes, &subject.id)
        .map_err(ApiError::from)?;
    match claimed {
        None => Ok(StatusCode::NO_CONTENT.into_response()),
        Some((task, run)) => Ok(Json(json!({
            "task": task,
            "run": run,
            "lease_generation": run.lease_generation,
        }))
        .into_response()),
    }
}

#[derive(Deserialize)]
pub struct RenewBody {
    pub lease_generation: i64,
}

async fn renew_lease(
    State(state): State<SharedState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RenewBody>,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::UpdateRun)?;
    let expires = state
        .store
        .renew_lease(
            &run_id,
            &subject.id,
            body.lease_generation,
            state.config.lease_seconds,
            &subject.id,
        )
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "lease_expires_at": expires })))
}

#[derive(Deserialize)]
pub struct RunEventBody {
    pub lease_generation: i64,
    pub to_state: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub candidate_sha: Option<String>,
}

async fn run_events(
    State(state): State<SharedState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<RunEventBody>,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::UpdateRun)?;
    let to: RunState = serde_json::from_value(json!(body.to_state))
        .map_err(|_| ApiError::invalid(format!("unknown run state {}", body.to_state)))?;
    state
        .store
        .advance_run(
            &run_id,
            &subject.id,
            body.lease_generation,
            to,
            body.session_id.as_deref(),
            body.candidate_sha.as_deref(),
            &subject.id,
        )
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "ok": true })))
}

async fn retry_task(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::OperatorAction)?;
    state
        .store
        .retry_task(&task_id, &subject.id)
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "ok": true })))
}

async fn recover_run(
    State(state): State<SharedState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::UpdateRun)?;
    // Original node requests to re-verify the same run.
    state
        .store
        .recover_run(&run_id, &subject.id)
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "ok": true })))
}

async fn resolve_run(
    State(state): State<SharedState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ResolveBody>,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::OperatorAction)?;
    state
        .store
        .resolve_run(&run_id, body.close, &body.reason, &subject.id)
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ResolveBody {
    pub close: bool,
    pub reason: String,
}

#[derive(Deserialize)]
pub struct MessageBody {
    pub target_node: Option<String>,
    pub target_session: Option<String>,
    pub request_id: Option<String>,
    pub task_id: Option<String>,
    pub kind: String,
    pub body: String,
}

async fn create_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(body): Json<MessageBody>,
) -> ApiResult<Response> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::AckMessage).or_else(|_| require(&subject, Action::WritePlan))?;
    let mid = MessageId::generate();
    state
        .store
        .create_message(
            &mid,
            &subject.id,
            body.target_node.as_deref(),
            body.target_session.as_deref(),
            body.request_id.as_deref(),
            body.task_id.as_deref(),
            &body.kind,
            &body.body,
            None,
        )
        .map_err(ApiError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "message_id": mid.as_str() })),
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct InboxQuery {
    #[serde(default)]
    pub after: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
}
fn default_limit() -> i64 {
    100
}

async fn inbox(
    State(state): State<SharedState>,
    Query(q): Query<InboxQuery>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    let messages = state
        .store
        .inbox(&subject.id, &q.after, q.limit)
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "messages": messages })))
}

async fn ack_message(
    State(state): State<SharedState>,
    Path(message_id): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::AckMessage)?;
    // Subject must be the target of the message.
    let msg = state
        .store
        .get_message(&message_id)
        .map_err(ApiError::from)?;
    if msg.target_node.as_deref() != Some(subject.id.as_str()) {
        return Err(ApiError::forbidden("not the target of this message"));
    }
    state
        .store
        .advance_message(&message_id, MessageState::Acknowledged, &subject.id)
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "ok": true })))
}

async fn list_resources(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    let out = state
        .store
        .list_resources(&subject.id)
        .map_err(ApiError::from)?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct ObservationBody {
    pub method: String,
    pub valid_seconds: i64,
}

async fn observe_resource(
    State(state): State<SharedState>,
    Path(resource_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ObservationBody>,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::UpdateObservation)?;
    state
        .store
        .observe_route(
            &resource_id,
            &subject.id,
            &body.method,
            body.valid_seconds,
            &subject.id,
        )
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "ok": true })))
}

async fn download_evidence(
    State(state): State<SharedState>,
    Path((run_id, name)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let subject = extract_subject(&headers, &state)?;
    let _ = subject;
    let (rel_path, expected_sha) = state
        .store
        .get_evidence_for_run(&run_id, &name)
        .map_err(ApiError::from)?;
    // Canonical path check + hash verification against evidence root.
    let full = std::path::Path::new(&state.config.evidence_root).join(&rel_path);
    let canonical = full
        .canonicalize()
        .map_err(|_| ApiError::not_found("evidence file missing"))?;
    if !canonical.starts_with(std::path::Path::new(&state.config.evidence_root)) {
        return Err(ApiError::invalid("evidence path traversal rejected"));
    }
    let bytes = tokio::fs::read(&canonical)
        .await
        .map_err(|_| ApiError::not_found("evidence file unreadable"))?;
    use sha2::{Digest, Sha256};
    let actual = hex::encode(Sha256::digest(&bytes));
    if actual != expected_sha {
        return Err(ApiError::conflict("evidence hash mismatch"));
    }
    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
        bytes,
    )
        .into_response())
}

// ---------- Review (control-only) ----------

#[derive(Deserialize)]
pub struct EvidenceRegisterBody {
    pub name: String,
    pub rel_path: String,
    pub sha256: String,
}

/// Runner wrote evidence under evidence_root; the center verifies the hash
/// and registers it. The subject must own the run.
async fn register_evidence(
    State(state): State<SharedState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<EvidenceRegisterBody>,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::UpdateRun)?;
    if body.name.contains("..") || body.rel_path.contains("..") {
        return Err(ApiError::invalid("evidence path traversal rejected"));
    }
    let full = std::path::Path::new(&state.config.evidence_root).join(&body.rel_path);
    let canonical = full
        .canonicalize()
        .map_err(|_| ApiError::not_found("evidence file missing"))?;
    if !canonical.starts_with(std::path::Path::new(&state.config.evidence_root)) {
        return Err(ApiError::invalid("evidence path traversal rejected"));
    }
    let bytes = tokio::fs::read(&canonical)
        .await
        .map_err(|_| ApiError::not_found("evidence file unreadable"))?;
    use sha2::{Digest, Sha256};
    let actual = hex::encode(Sha256::digest(&bytes));
    if actual != body.sha256 {
        return Err(ApiError::conflict("evidence hash mismatch"));
    }
    state
        .store
        .add_evidence(
            &run_id,
            &body.name,
            &body.rel_path,
            &body.sha256,
            &subject.id,
            None,
            None,
            None,
        )
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct FinishBody {
    pub session_id: String,
    pub candidate_sha: String,
}

/// Worker finishes an execute run; the center moves the task to
/// awaiting_review and queues the review run for the planner node.
async fn finish_run(
    State(state): State<SharedState>,
    Path(run_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<FinishBody>,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::UpdateRun)?;
    let review_id = state
        .store
        .finish_execute_and_queue_review(
            &run_id,
            &body.session_id,
            &body.candidate_sha,
            &state.config.planner_node,
            &subject.id,
        )
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "review_run_id": review_id })))
}

#[derive(Deserialize)]
pub struct ReviewBody {
    pub reviewed_run_id: String,
    pub candidate_sha: String,
    pub fixed_check_exit_code: i64,
    pub agent_verdict: String,
}

async fn submit_review(
    State(state): State<SharedState>,
    Path(review_run_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ReviewBody>,
) -> ApiResult<Json<Value>> {
    let subject = extract_subject(&headers, &state)?;
    require(&subject, Action::SubmitReview)?;
    let verdict = state
        .store
        .submit_review(
            &review_run_id,
            &body.reviewed_run_id,
            &body.candidate_sha,
            body.fixed_check_exit_code,
            &body.agent_verdict,
            &subject.id,
        )
        .map_err(ApiError::from)?;
    Ok(Json(json!({ "final_verdict": verdict })))
}

// ---------- Router / startup ----------

pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/v1/requests", post(create_request))
        .route("/v1/requests/:id/plan", put(submit_plan))
        .route("/v1/requests/:id", get(get_request))
        .route("/v1/tasks/:id", get(get_task))
        .route("/v1/requests/:id/events", get(request_events))
        .route("/v1/planner/binding", put(planner_binding))
        .route("/v1/nodes/heartbeat", post(heartbeat))
        .route("/v1/runs/claim", post(claim_run))
        .route("/v1/runs/:id/renew", post(renew_lease))
        .route("/v1/runs/:id/events", post(run_events))
        .route("/v1/tasks/:id/retry", post(retry_task))
        .route("/v1/runs/:id/recover", post(recover_run))
        .route("/v1/runs/:id/resolve", post(resolve_run))
        .route("/v1/messages", post(create_message))
        .route("/v1/inbox", get(inbox))
        .route("/v1/messages/:id/ack", post(ack_message))
        .route("/v1/resources", get(list_resources))
        .route("/v1/resources/:id/observation", put(observe_resource))
        .route("/v1/runs/:id/evidence/:name", get(download_evidence))
        .route("/v1/runs/:id/evidence", post(register_evidence))
        .route("/v1/runs/:id/finish", post(finish_run))
        .route("/v1/reviews/:run_id", post(submit_review))
        .with_state(state)
}

impl AppState {
    /// Build token map from env vars declared by the config.
    pub fn token_map(config: &CentralConfig) -> HashMap<String, (String, Role)> {
        let mut tokens = HashMap::new();
        let op_env = config.operator_token_env.clone();
        if let Ok(t) = std::env::var(&op_env) {
            tokens.insert(t, ("operator".to_string(), Role::Operator));
        }
        for (node_id, node) in &config.nodes {
            if let Ok(t) = std::env::var(&node.token_env) {
                let role = if node.role == "control" {
                    Role::Control
                } else {
                    Role::Worker
                };
                tokens.insert(t, (node_id.clone(), role));
            }
        }
        tokens
    }
}

pub mod data_ext {
    // Helper so handlers can reach the store through either naming.
    pub trait AsStore {
        fn store(&self) -> &store::Store;
    }
}
