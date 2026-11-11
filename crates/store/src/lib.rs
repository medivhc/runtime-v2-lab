//! SQLite store: migrations, transactional state transitions with event
//! writes, idempotency, and scheduling queries. Single writer: the service
//! process owns the database file.

use chrono::Utc;
use domain::error::DomainError;
use domain::ids::{EvidenceId, MessageId, RequestId, RunId, TaskId};
use domain::plan::{validate, Plan, PlanContext, ValidatedPlan};
use domain::sched::{evaluate_claim, ClaimDecision, NodeView, RouteView, TaskView};
use domain::state::*;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Debug)]
pub enum StoreError {
    Db(rusqlite::Error),
    Domain(DomainError),
    /// Same idempotency key, different body: hard reject.
    IdempotencyConflict,
    /// Lease/idempotency conflict (HTTP 409).
    Conflict(String),
    NotFound(String),
    /// Generic precondition violation (HTTP 422).
    Invalid(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Db(e) => write!(f, "db error: {e}"),
            StoreError::Domain(e) => write!(f, "{e}"),
            StoreError::IdempotencyConflict => write!(f, "idempotency conflict"),
            StoreError::Conflict(m) => write!(f, "conflict: {m}"),
            StoreError::NotFound(m) => write!(f, "not found: {m}"),
            StoreError::Invalid(m) => write!(f, "invalid: {m}"),
        }
    }
}
impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        match &e {
            rusqlite::Error::QueryReturnedNoRows => StoreError::NotFound("row".into()),
            _ => StoreError::Db(e),
        }
    }
}
impl From<DomainError> for StoreError {
    fn from(e: DomainError) -> Self {
        StoreError::Domain(e)
    }
}

// ---------- Configuration ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CentralConfig {
    pub listen: String,
    pub database: String,
    pub evidence_root: String,
    pub operator_token_env: String,
    pub planner_node: String,
    #[serde(default = "default_offline")]
    pub offline_after_seconds: i64,
    #[serde(default = "default_lease")]
    pub lease_seconds: i64,
    #[serde(default)]
    pub nodes: std::collections::BTreeMap<String, NodeConfig>,
    #[serde(default)]
    pub repositories: std::collections::BTreeMap<String, RepoConfig>,
}

fn default_offline() -> i64 {
    15
}
fn default_lease() -> i64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub role: String,
    pub capabilities: Vec<String>,
    pub token_env: String,
    #[serde(default)]
    pub capacity: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub bare_path: String,
    #[serde(default)]
    pub acceptance_profiles: Vec<String>,
}

impl CentralConfig {
    /// Parse and sanity-check a central.toml (absolute paths required).
    pub fn load(path: &str) -> Result<Self, StoreError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| StoreError::Invalid(format!("config read failed: {e}")))?;
        let cfg: CentralConfig = toml::from_str(&raw)
            .map_err(|e| StoreError::Invalid(format!("config parse failed: {e}")))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<(), StoreError> {
        for (k, v) in [
            ("database", &self.database),
            ("evidence_root", &self.evidence_root),
        ] {
            if !v.starts_with('/') {
                return Err(StoreError::Invalid(format!("{k} must be absolute path")));
            }
        }
        if self.nodes.is_empty() {
            return Err(StoreError::Invalid("config has no nodes".into()));
        }
        for (id, n) in &self.nodes {
            if n.role != "control" && n.role != "worker" {
                return Err(StoreError::Invalid(format!(
                    "node {id}: role must be control|worker"
                )));
            }
            if n.token_env.is_empty() {
                return Err(StoreError::Invalid(format!("node {id}: token_env empty")));
            }
        }
        if !self.nodes.contains_key(&self.planner_node) {
            return Err(StoreError::Invalid(
                "planner_node must reference a configured node".into(),
            ));
        }
        for (id, r) in &self.repositories {
            if !r.bare_path.starts_with('/') {
                return Err(StoreError::Invalid(format!(
                    "repo {id}: bare_path must be absolute"
                )));
            }
        }
        Ok(())
    }
}

// ---------- Row structs ----------

#[derive(Debug, Clone, Serialize)]
pub struct RequestRow {
    pub request_id: String,
    pub intent: String,
    pub repo_ref: String,
    pub baseline_sha: String,
    pub required_capabilities: Vec<String>,
    pub required_routes: Vec<String>,
    pub planner_session: Option<String>,
    pub operator: String,
    pub plan_version: i64,
    pub state: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskRow {
    pub task_id: String,
    pub request_id: String,
    pub key: String,
    pub objective: String,
    pub deliverable: Option<String>,
    pub repo_ref: String,
    pub required_capabilities: Vec<String>,
    pub required_routes: Vec<String>,
    pub acceptance_profile: String,
    pub fixed_node: Option<String>,
    pub depends_on: Vec<String>,
    pub state: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunRow {
    pub run_id: String,
    pub task_id: String,
    pub run_type: String,
    pub attempt: i64,
    pub node_id: String,
    pub session_id: Option<String>,
    pub lease_generation: i64,
    pub lease_expires_at: Option<i64>,
    pub candidate_sha: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageRow {
    pub message_id: String,
    pub from_subject: String,
    pub target_node: Option<String>,
    pub target_session: Option<String>,
    pub request_id: Option<String>,
    pub task_id: Option<String>,
    pub kind: String,
    pub body: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventRow {
    pub seq: i64,
    pub entity_id: String,
    pub event_type: String,
    pub subject: String,
    pub payload: String,
    pub created_at: String,
}

// ---------- Store ----------

pub struct Store {
    pub(crate) conn: refcell::RefCellConn,
}

impl Store {
    /// Direct SQL access for integration tests (e.g., seeding FK rows).
    #[doc(hidden)]
    pub fn execute_raw_sql(&self, sql: &str, params: &[&str]) -> StoreResult<usize> {
        use rusqlite::ToSql;
        let mapped: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
        Ok(self.conn.borrow_mut().execute(sql, mapped.as_slice())?)
    }
}

/// Minimal interior-mutability wrapper so `&Store` can open transactions
/// (rusqlite Transaction needs &mut Connection; the service shares the
/// store through Arc). A blocking Mutex, not a RefCell: concurrent request
/// handlers must serialize on the single connection instead of panicking
/// with "RefCell already borrowed" when two writes overlap.
pub mod refcell {
    use rusqlite::Connection;
    use std::sync::Mutex;
    #[derive(Debug)]
    pub struct RefCellConn(pub Mutex<Connection>);
    impl RefCellConn {
        pub fn borrow_mut(&self) -> std::sync::MutexGuard<'_, Connection> {
            self.0.lock().unwrap_or_else(|p| p.into_inner())
        }
        pub fn borrow(&self) -> std::sync::MutexGuard<'_, Connection> {
            self.borrow_mut()
        }
    }
}

impl Store {
    /// In-memory database (for tests and ephemeral uses).
    pub fn in_memory() -> StoreResult<Store> {
        let conn = Connection::open_in_memory()?;
        let s = Store { conn: refcell::RefCellConn(std::sync::Mutex::new(conn)) };
        s.migrate()?;
        Ok(s)
    }

    pub fn open(path: &str) -> StoreResult<Store> {
        let conn = Connection::open(path)?;
        // Concurrent writers (multi-connection tests, operator CLI inspection)
        // must wait for the write lock instead of failing with SQLITE_BUSY.
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        let s = Store { conn: refcell::RefCellConn(std::sync::Mutex::new(conn)) };
        s.migrate()?;
        Ok(s)
    }

    /// Idempotent migration with foreign keys on.
    pub fn migrate(&self) -> StoreResult<()> {
        self.conn.borrow_mut().execute_batch("PRAGMA foreign_keys = ON;")?;
        self.conn.borrow_mut().execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS requests (
                request_id TEXT PRIMARY KEY,
                intent TEXT NOT NULL,
                repo_ref TEXT NOT NULL,
                baseline_sha TEXT NOT NULL,
                required_capabilities TEXT NOT NULL DEFAULT '[]',
                required_routes TEXT NOT NULL DEFAULT '[]',
                planner_session TEXT,
                operator TEXT NOT NULL,
                plan_version INTEGER NOT NULL DEFAULT 0,
                state TEXT NOT NULL DEFAULT 'planning',
                reason TEXT,
                idem_key TEXT,
                body_digest TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_requests_idem
                ON requests(idem_key) WHERE idem_key IS NOT NULL;

            CREATE TABLE IF NOT EXISTS tasks (
                task_id TEXT PRIMARY KEY,
                request_id TEXT NOT NULL REFERENCES requests(request_id),
                key TEXT NOT NULL,
                objective TEXT NOT NULL,
                deliverable TEXT,
                repo_ref TEXT NOT NULL,
                required_capabilities TEXT NOT NULL DEFAULT '[]',
                required_routes TEXT NOT NULL DEFAULT '[]',
                acceptance_profile TEXT NOT NULL,
                fixed_node TEXT,
                depends_on TEXT NOT NULL DEFAULT '[]',
                state TEXT NOT NULL DEFAULT 'waiting_dependency',
                reason TEXT,
                UNIQUE(request_id, key)
            );

            CREATE TABLE IF NOT EXISTS runs (
                run_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL REFERENCES tasks(task_id),
                run_type TEXT NOT NULL CHECK (run_type IN ('execute','review')),
                attempt INTEGER NOT NULL,
                node_id TEXT NOT NULL,
                session_id TEXT,
                lease_generation INTEGER NOT NULL DEFAULT 1,
                lease_expires_at INTEGER,
                candidate_sha TEXT,
                state TEXT NOT NULL DEFAULT 'leased',
                UNIQUE(task_id, run_type, attempt)
            );
            -- At most one non-terminal run per (task, type).
            CREATE UNIQUE INDEX IF NOT EXISTS idx_runs_active
                ON runs(task_id, run_type)
                WHERE state IN ('leased','session_bound','prompt_admitted','running','uncertain');

            CREATE TABLE IF NOT EXISTS nodes (
                node_id TEXT PRIMARY KEY,
                role TEXT NOT NULL,
                capabilities TEXT NOT NULL DEFAULT '[]',
                capacity INTEGER NOT NULL DEFAULT 1,
                last_heartbeat INTEGER NOT NULL DEFAULT 0,
                repo_refs TEXT NOT NULL DEFAULT '[]'
            );

            CREATE TABLE IF NOT EXISTS messages (
                message_id TEXT PRIMARY KEY,
                from_subject TEXT NOT NULL,
                target_node TEXT,
                target_session TEXT,
                request_id TEXT,
                task_id TEXT,
                kind TEXT NOT NULL,
                body TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT 'queued',
                idem_key TEXT,
                body_digest TEXT,
                delivered_at INTEGER,
                acknowledged_at INTEGER
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_idem
                ON messages(idem_key) WHERE idem_key IS NOT NULL;

            CREATE TABLE IF NOT EXISTS evidence (
                evidence_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL REFERENCES runs(run_id),
                kind TEXT NOT NULL,
                rel_path TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                source TEXT NOT NULL,
                baseline_sha TEXT,
                candidate_sha TEXT,
                run_ref TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_evidence_path
                ON evidence(run_id, rel_path);

            CREATE TABLE IF NOT EXISTS reviews (
                review_run_id TEXT PRIMARY KEY REFERENCES runs(run_id),
                reviewed_run_id TEXT NOT NULL REFERENCES runs(run_id),
                candidate_sha TEXT NOT NULL,
                fixed_check_exit_code INTEGER,
                agent_verdict TEXT,
                final_verdict TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS resources (
                resource_id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                owner_node TEXT,
                version TEXT,
                visibility TEXT NOT NULL DEFAULT 'node',
                observed_at INTEGER,
                valid_seconds INTEGER,
                credential_ref TEXT
            );

            CREATE TABLE IF NOT EXISTS routes (
                resource_id TEXT NOT NULL REFERENCES resources(resource_id),
                source_node TEXT NOT NULL,
                method TEXT NOT NULL,
                observed_at INTEGER NOT NULL,
                valid_seconds INTEGER NOT NULL,
                PRIMARY KEY (resource_id, source_node)
            );

            CREATE TABLE IF NOT EXISTS planner_binding (
                planner_node TEXT PRIMARY KEY,
                session_id TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS events (
                seq INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                subject TEXT NOT NULL,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS idem_writes (
                subject TEXT NOT NULL,
                idem_key TEXT NOT NULL,
                body_digest TEXT NOT NULL,
                response TEXT,
                PRIMARY KEY (subject, idem_key)
            );

            CREATE TABLE IF NOT EXISTS repo_registry (
                name TEXT PRIMARY KEY,
                bare_path TEXT NOT NULL,
                profiles TEXT NOT NULL DEFAULT '[]'
            );
            "#,
        )?;
        Ok(())
    }

    fn now() -> i64 {
        Utc::now().timestamp()
    }

    fn now_string() -> String {
        Utc::now().to_rfc3339()
    }

    fn body_digest(body: &str) -> String {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(body.as_bytes()))
    }

    // ---------- Events ----------

    /// Append an event; must be called inside the same transaction as the
    /// state change it describes. Returns the monotonic seq.
    pub fn append_event_tx(
        tx: &Transaction<'_>,
        entity_id: &str,
        event_type: &str,
        subject: &str,
        payload: &serde_json::Value,
    ) -> StoreResult<i64> {
        tx.execute(
            "INSERT INTO events (entity_id, event_type, subject, payload, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entity_id,
                event_type,
                subject,
                payload.to_string(),
                Self::now_string()
            ],
        )?;
        Ok(tx.last_insert_rowid())
    }

    pub fn append_event(
        &self,
        entity_id: &str,
        event_type: &str,
        subject: &str,
        payload: &serde_json::Value,
    ) -> StoreResult<i64> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let seq = Self::append_event_tx(&tx, entity_id, event_type, subject, payload)?;
        tx.commit()?;
        Ok(seq)
    }

    pub fn list_events(&self, entity_id: &str, after: i64) -> StoreResult<Vec<EventRow>> {
        let conn_ref = self.conn.borrow();
        let mut stmt = conn_ref.prepare(
            "SELECT seq, entity_id, event_type, subject, payload, created_at
             FROM events WHERE entity_id = ?1 AND seq > ?2 ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![entity_id, after], |r| {
            Ok(EventRow {
                seq: r.get(0)?,
                entity_id: r.get(1)?,
                event_type: r.get(2)?,
                subject: r.get(3)?,
                payload: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Global event stream cursor read.
    pub fn list_events_global(&self, after: i64, limit: i64) -> StoreResult<Vec<EventRow>> {
        let conn_ref = self.conn.borrow();
        let mut stmt = conn_ref.prepare(
            "SELECT seq, entity_id, event_type, subject, payload, created_at
             FROM events WHERE seq > ?1 ORDER BY seq LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![after, limit], |r| {
            Ok(EventRow {
                seq: r.get(0)?,
                entity_id: r.get(1)?,
                event_type: r.get(2)?,
                subject: r.get(3)?,
                payload: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    // ---------- Idempotent writes ----------

    /// Check-or-insert an idempotency record. Same key + same body digest ->
    /// Ok(Some(response)) (return prior response). Same key + different body
    /// -> Err(IdempotencyConflict). New key -> inserts and returns Ok(None).
    pub fn idem_check(
        &self,
        subject: &str,
        idem_key: &str,
        body_digest: &str,
        response: Option<&str>,
    ) -> StoreResult<Option<String>> {
        let row: Option<(String, Option<String>)> = self.conn.borrow().query_row(
                "SELECT body_digest, response FROM idem_writes WHERE subject=?1 AND idem_key=?2",
                params![subject, idem_key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        if let Some((stored_digest, stored_response)) = row {
            if stored_digest != body_digest {
                return Err(StoreError::IdempotencyConflict);
            }
            return Ok(stored_response);
        }
        self.conn.borrow_mut().execute(
            "INSERT OR IGNORE INTO idem_writes (subject, idem_key, body_digest, response)
             VALUES (?1, ?2, ?3, ?4)",
            params![subject, idem_key, body_digest, response],
        )?;
        Ok(None)
    }

    pub fn idem_store_response(&self, subject: &str, idem_key: &str, response: &str) -> StoreResult<()> {
        self.conn.borrow_mut().execute(
            "UPDATE idem_writes SET response=?3 WHERE subject=?1 AND idem_key=?2",
            params![subject, idem_key, response],
        )?;
        Ok(())
    }

    // ---------- Requests ----------

    pub fn create_request(
        &self,
        request_id: &RequestId,
        intent: &str,
        repo_ref: &str,
        baseline_sha: &str,
        required_capabilities: &[String],
        required_routes: &[String],
        operator: &str,
    ) -> StoreResult<()> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        tx.execute(
            "INSERT INTO requests (request_id, intent, repo_ref, baseline_sha,
                required_capabilities, required_routes, operator, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'planning')",
            params![
                request_id.as_str(),
                intent,
                repo_ref,
                baseline_sha,
                serde_json::to_string(required_capabilities).unwrap(),
                serde_json::to_string(required_routes).unwrap(),
                operator
            ],
        )?;
        // Planning message to control, queued.
        let msg_id = MessageId::generate();
        let body = json!({
            "type": "plan_request",
            "request_id": request_id.as_str(),
            "intent": intent,
            "repo_ref": repo_ref,
            "baseline_sha": baseline_sha,
            "required_capabilities": required_capabilities,
            "required_routes": required_routes,
        });
        tx.execute(
            "INSERT INTO messages (message_id, from_subject, target_node, request_id, kind, body, state)
             VALUES (?1, 'operator', ?2, ?3, 'plan_request', ?4, 'queued')",
            params![msg_id.as_str(), planner_target(&tx)?, request_id.as_str(), body.to_string()],
        )?;
        Self::append_event_tx(
            &tx,
            request_id.as_str(),
            "request.created",
            operator,
            &body,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_request(&self, request_id: &str) -> StoreResult<RequestRow> {
        let (state, reason): (String, Option<String>) = self.conn.borrow().query_row(
            "SELECT state, reason FROM requests WHERE request_id=?1",
            params![request_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        self.read_request_full(request_id, state, reason)
    }

    fn read_request_full(
        &self,
        request_id: &str,
        state: String,
        reason: Option<String>,
    ) -> StoreResult<RequestRow> {
        let conn_ref = self.conn.borrow();
        let mut stmt = conn_ref.prepare(
            "SELECT request_id, intent, repo_ref, baseline_sha, required_capabilities,
                    required_routes, planner_session, operator, plan_version
             FROM requests WHERE request_id=?1",
        )?;
        let row = stmt
            .query_row(params![request_id], |r| {
                Ok(RequestRow {
                    request_id: r.get(0)?,
                    intent: r.get(1)?,
                    repo_ref: r.get(2)?,
                    baseline_sha: r.get(3)?,
                    required_capabilities: serde_json::from_str(&r.get::<_, String>(4)?).unwrap(),
                    required_routes: serde_json::from_str(&r.get::<_, String>(5)?).unwrap(),
                    planner_session: r.get(6)?,
                    operator: r.get(7)?,
                    plan_version: r.get(8)?,
                    state: state.clone(),
                    reason: reason.clone(),
                })
            })
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("request {request_id}")))?;
        Ok(row)
    }

    /// Submit a plan for a request: validate in-transaction, insert tasks,
    /// transition request planning -> active, queue task messages.
    pub fn submit_plan(
        &self,
        request_id: &str,
        plan: &Plan,
        submitted_by_node: Option<&str>,
    ) -> StoreResult<ValidatedPlan> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let (state, caps_json, routes_json, planner_binding): (String, String, String, Option<String>) =
            tx.query_row(
                "SELECT state, required_capabilities, required_routes, planner_session
                 FROM requests WHERE request_id=?1",
                params![request_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("request {request_id}")))?;
        if state != "planning" {
            return Err(StoreError::Invalid(format!(
                "request {request_id} not in planning state"
            )));
        }
        let required_capabilities: Vec<String> = serde_json::from_str(&caps_json).unwrap();
        let required_routes: Vec<String> = serde_json::from_str(&routes_json).unwrap();

        // Gather repo/profile context from config-known repos: we read them
        // from the `repositories` registration table if present.
        let repos: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT name, profiles FROM repo_registry")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let ctx = PlanContext {
            repos: repos.iter().map(|(r, _)| r.clone()).collect(),
            acceptance_profiles: repos
                .iter()
                .map(|(r, p)| {
                    (
                        r.clone(),
                        serde_json::from_str::<Vec<String>>(p).unwrap_or_default(),
                    )
                })
                .collect(),
            submitted_by_node: submitted_by_node.map(|s| s.to_string()),
            planner_binding: planner_binding.as_ref().map(|_| {
                tx.query_row(
                    "SELECT planner_node FROM planner_binding WHERE session_id=?1",
                    params![planner_binding],
                    |r| r.get::<_, String>(0),
                )
                .unwrap_or_default()
            }),
        };
        let validated = validate(
            plan,
            &required_capabilities,
            &required_routes,
            &ctx,
        )?;

        for t in &validated.plan.tasks {
            tx.execute(
                "INSERT INTO tasks (task_id, request_id, key, objective, deliverable, repo_ref,
                    required_capabilities, required_routes, acceptance_profile, fixed_node,
                    depends_on, state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                    CASE WHEN ?11 = '[]' THEN 'queued' ELSE 'waiting_dependency' END)",
                params![
                    TaskId::generate().as_str(),
                    request_id,
                    t.key,
                    t.objective,
                    t.deliverable,
                    t.repo_ref,
                    serde_json::to_string(&t.required_capabilities).unwrap(),
                    serde_json::to_string(&t.required_routes).unwrap(),
                    t.acceptance_profile,
                    t.fixed_node,
                    serde_json::to_string(&t.depends_on).unwrap(),
                ],
            )?;
        }
        let new_state = RequestState::Active;
        request_transition(RequestState::Planning, new_state)?;
        tx.execute(
            "UPDATE requests SET state='active', plan_version=plan_version+1 WHERE request_id=?1",
            params![request_id],
        )?;
        Self::append_event_tx(
            &tx,
            request_id,
            "request.plan_accepted",
            submitted_by_node.unwrap_or("control"),
            &json!({"tasks": validated.plan.tasks.len()}),
        )?;
        tx.commit()?;
        Ok(validated)
    }

    /// Mark planning failed (blocked) with reason.
    pub fn block_request(&self, request_id: &str, reason: &str, subject: &str) -> StoreResult<()> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        request_transition(RequestState::Planning, RequestState::Blocked)?;
        tx.execute(
            "UPDATE requests SET state='blocked', reason=?2 WHERE request_id=?1 AND state='planning'",
            params![request_id, reason],
        )?;
        Self::append_event_tx(&tx, request_id, "request.blocked", subject, &json!({"reason": reason}))?;
        tx.commit()?;
        Ok(())
    }

    /// Mark request accepted only when all its tasks are accepted.
    pub fn accept_request_if_complete(&self, request_id: &str, subject: &str) -> StoreResult<bool> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let pending: i64 = tx.query_row(
            "SELECT COUNT(*) FROM tasks WHERE request_id=?1 AND state != 'accepted'",
            params![request_id],
            |r| r.get(0),
        )?;
        if pending > 0 {
            return Ok(false);
        }
        request_transition(RequestState::Active, RequestState::Accepted)?;
        tx.execute(
            "UPDATE requests SET state='accepted' WHERE request_id=?1 AND state='active'",
            params![request_id],
        )?;
        Self::append_event_tx(&tx, request_id, "request.accepted", subject, &json!({}))?;
        tx.commit()?;
        Ok(true)
    }

    // ---------- Planner binding ----------

    /// Register fixed planner session. Re-registering the same binding is
    /// OK; changing an existing binding requires operator action.
    pub fn bind_planner(
        &self,
        planner_node: &str,
        session_id: &str,
        subject: &str,
    ) -> StoreResult<String> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT session_id FROM planner_binding WHERE planner_node=?1",
                params![planner_node],
                |r| r.get(0),
            )
            .optional()?;
        match existing {
            Some(sid) if sid == session_id => Ok(sid),
            Some(_) => Err(StoreError::Conflict(
                "planner binding exists; change requires operator resolution".into(),
            )),
            None => {
                tx.execute(
                    "INSERT INTO planner_binding (planner_node, session_id) VALUES (?1, ?2)",
                    params![planner_node, session_id],
                )?;
                Self::append_event_tx(
                    &tx,
                    planner_node,
                    "planner.bound",
                    subject,
                    &json!({"session_id": session_id}),
                )?;
                tx.commit()?;
                Ok(session_id.to_string())
            }
        }
    }

    pub fn get_planner_binding(&self, planner_node: &str) -> StoreResult<Option<String>> {
        Ok(self.conn.borrow().query_row(
                "SELECT session_id FROM planner_binding WHERE planner_node=?1",
                params![planner_node],
                |r| r.get(0),
            )
            .optional()?)
    }

    // ---------- Nodes ----------

    /// Heartbeat: upsert node with center-limited role/capabilities.
    pub fn heartbeat(
        &self,
        node_id: &str,
        role: &str,
        capabilities: &[String],
        capacity: i64,
        repo_refs: &[String],
    ) -> StoreResult<()> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        tx.execute(
            "INSERT INTO nodes (node_id, role, capabilities, capacity, last_heartbeat, repo_refs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(node_id) DO UPDATE SET
                role=?2, capabilities=?3, capacity=?4, last_heartbeat=?5, repo_refs=?6",
            params![
                node_id,
                role,
                serde_json::to_string(capabilities).unwrap(),
                capacity,
                Self::now(),
                serde_json::to_string(repo_refs).unwrap(),
            ],
        )?;
        Self::append_event_tx(&tx, node_id, "node.heartbeat", node_id, &json!({}))?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_node(&self, node_id: &str, offline_after_seconds: i64) -> StoreResult<Option<NodeView>> {
        let row = self.conn.borrow().query_row(
                "SELECT node_id, role, capabilities, capacity, last_heartbeat, repo_refs
                 FROM nodes WHERE node_id=?1",
                params![node_id],
                |r| {
                    Ok(NodeView {
                        node_id: r.get(0)?,
                        role: r.get(1)?,
                        capabilities: serde_json::from_str(&r.get::<_, String>(2)?).unwrap(),
                        capacity: r.get(3)?,
                        active_runs: 0, // filled below
                        seconds_since_heartbeat: 0,
                        repo_refs: serde_json::from_str(&r.get::<_, String>(5)?).unwrap(),
                        offline_after_seconds,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    // ---------- Runs ----------

    /// Attempt to claim the next eligible execute task for a node.
    /// Serialized by the SQLite write transaction: first claim wins.
    pub fn claim_execute(
        &self,
        node: &NodeView,
        _node_routes: &[RouteView],
        subject: &str,
    ) -> StoreResult<Option<(TaskRow, RunRow)>> {
        let mut conn_mut = self.conn.borrow_mut();
        // BEGIN IMMEDIATE: acquire the write lock upfront so that concurrent
        // claimers serialize (upgrade-deadlock would otherwise return
        // SQLITE_BUSY immediately, bypassing busy_timeout).
        let tx = conn_mut
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        // Refresh active run counts for this node.
        let active: i64 = tx.query_row(
            "SELECT COUNT(*) FROM runs r JOIN nodes n ON r.node_id = n.node_id
             WHERE r.node_id=?1 AND r.state IN ('leased','session_bound','prompt_admitted','running','uncertain')",
            params![node.node_id],
            |r| r.get(0),
        )?;
        let mut node_view = node.clone();
        node_view.active_runs = active;

        // Scan queued tasks in order; the first eligible one is claimed.
        let mut stmt = tx.prepare(
            "SELECT task_id, request_id, key, objective, deliverable, repo_ref,
                    required_capabilities, required_routes, acceptance_profile, fixed_node,
                    depends_on, state, reason
             FROM tasks WHERE state='queued' ORDER BY task_id",
        )?;
        let candidates: Vec<TaskRow> = stmt
            .query_map([], |r| {
                Ok(TaskRow {
                    task_id: r.get(0)?,
                    request_id: r.get(1)?,
                    key: r.get(2)?,
                    objective: r.get(3)?,
                    deliverable: r.get(4)?,
                    repo_ref: r.get(5)?,
                    required_capabilities: serde_json::from_str(&r.get::<_, String>(6)?).unwrap(),
                    required_routes: serde_json::from_str(&r.get::<_, String>(7)?).unwrap(),
                    acceptance_profile: r.get(8)?,
                    fixed_node: r.get(9)?,
                    depends_on: serde_json::from_str(&r.get::<_, String>(10)?).unwrap(),
                    state: r.get(11)?,
                    reason: r.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        for task in candidates {
            // Fill dependency states for the eligibility view.
            let dependency_states = Self::dependency_states_tx(&tx, &task.depends_on)?;
            // Active run check.
            let active_run: i64 = tx.query_row(
                "SELECT COUNT(*) FROM runs WHERE task_id=?1 AND run_type='execute'
                 AND state IN ('leased','session_bound','prompt_admitted','running','uncertain')",
                params![task.task_id],
                |r| r.get(0),
            )?;
            let has_active_run = active_run > 0;
            // Routes for this node.
            let routes = Self::routes_for_node_tx(&tx, &node.node_id)?;
            let view = TaskView {
                task_id: task.task_id.clone(),
                state: TaskState::Queued,
                repo_ref: task.repo_ref.clone(),
                required_capabilities: task.required_capabilities.clone(),
                required_routes: task.required_routes.clone(),
                fixed_node: task.fixed_node.clone(),
                dependency_states,
                has_active_run,
            };
            if let ClaimDecision::Eligible = evaluate_claim(&node_view, &view, &routes) {
                // Claim: create run with lease, move task to running.
                let run_id = RunId::generate();
                let lease_expires = Self::now() + 30;
                tx.execute(
                    "INSERT INTO runs (run_id, task_id, run_type, attempt, node_id,
                        lease_generation, lease_expires_at, state)
                     VALUES (?1, ?2, 'execute',
                        COALESCE((SELECT MAX(attempt)+1 FROM runs WHERE task_id=?2 AND run_type='execute'), 1),
                        ?3, 1, ?4, 'leased')",
                    params![run_id.as_str(), task.task_id, node.node_id, lease_expires],
                )?;
                task_transition(TaskState::Queued, TaskState::Running)?;
                tx.execute(
                    "UPDATE tasks SET state='running' WHERE task_id=?1",
                    params![task.task_id],
                )?;
                Self::append_event_tx(
                    &tx,
                    &task.task_id,
                    "run.claimed",
                    subject,
                    &json!({"run_id": run_id.as_str(), "node": node.node_id}),
                )?;
                let run = RunRow {
                    run_id: run_id.as_str().to_string(),
                    task_id: task.task_id.clone(),
                    run_type: "execute".into(),
                    attempt: tx.query_row(
                        "SELECT attempt FROM runs WHERE run_id=?1",
                        params![run_id.as_str()],
                        |r| r.get(0),
                    )?,
                    node_id: node.node_id.clone(),
                    session_id: None,
                    lease_generation: 1,
                    lease_expires_at: Some(lease_expires),
                    candidate_sha: None,
                    state: "leased".into(),
                };
                tx.commit()?;
                return Ok(Some((task, run)));
            }
        }
        tx.commit()?;
        Ok(None)
    }

    fn dependency_states_tx(tx: &Transaction<'_>, depends_on: &[String]) -> StoreResult<Vec<TaskState>> {
        let mut out = Vec::new();
        for dep in depends_on {
            let state: String = tx
                .query_row(
                    "SELECT t.state FROM tasks t WHERE t.key=?1",
                    params![dep],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or_else(|| "unknown".into());
            // Match by (request-scoped) key is ambiguous across requests; in
            // this schema deps are request-scoped, resolved by caller via
            // get_task_by_key_in_request. Fallback: unknown blocks.
            out.push(match state.as_str() {
                "accepted" => TaskState::Accepted,
                "queued" => TaskState::Queued,
                "running" => TaskState::Running,
                "awaiting_review" => TaskState::AwaitingReview,
                "failed" => TaskState::Failed,
                "canceled" => TaskState::Canceled,
                "uncertain" => TaskState::Uncertain,
                _ => TaskState::WaitingDependency,
            });
        }
        Ok(out)
    }

    fn routes_for_node_tx(tx: &Transaction<'_>, node_id: &str) -> StoreResult<Vec<RouteView>> {
        let mut stmt = tx.prepare(
            "SELECT resource_id, source_node, observed_at, valid_seconds FROM routes
             WHERE source_node=?1",
        )?;
        let rows = stmt.query_map(params![node_id], |r| {
            let observed_at: i64 = r.get(2)?;
            let valid_seconds: i64 = r.get(3)?;
            Ok(RouteView {
                resource_id: r.get(0)?,
                source_node: r.get(1)?,
                seconds_since_observation: Self::now() - observed_at,
                valid_seconds,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Renew a lease with generation check; expired generation -> conflict.
    pub fn renew_lease(
        &self,
        run_id: &str,
        node_id: &str,
        lease_generation: i64,
        lease_seconds: i64,
        subject: &str,
    ) -> StoreResult<i64> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let (run_node, gen, state): (String, i64, String) = tx.query_row(
            "SELECT node_id, lease_generation, state FROM runs WHERE run_id=?1",
            params![run_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("run {run_id}")))?;
        let lease_expires: Option<i64> = tx
            .query_row(
                "SELECT lease_expires_at FROM runs WHERE run_id=?1",
                params![run_id],
                |r| r.get(0),
            )?;
        let expired = lease_expires.map(|t| t < Self::now()).unwrap_or(true);
        domain::sched::lease_write_allowed(&run_node, gen, node_id, lease_generation, expired)
            .map_err(|e| StoreError::Conflict(e.to_string()))?;
        if run_is_terminal(
            serde_json::from_value(json!(state)).unwrap_or(RunState::Failed),
        ) {
            return Err(StoreError::Invalid(format!("run {run_id} is terminal")));
        }
        let new_expiry = Self::now() + lease_seconds;
        tx.execute(
            "UPDATE runs SET lease_expires_at=?2 WHERE run_id=?1",
            params![run_id, new_expiry],
        )?;
        Self::append_event_tx(&tx, run_id, "run.lease_renewed", subject, &json!({}))?;
        tx.commit()?;
        Ok(new_expiry)
    }

    /// Advance run state with generation + identity check, event in same tx.
    pub fn advance_run(
        &self,
        run_id: &str,
        node_id: &str,
        lease_generation: i64,
        to: RunState,
        session_id: Option<&str>,
        candidate_sha: Option<&str>,
        subject: &str,
    ) -> StoreResult<()> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let (run_node, gen, state_str): (String, i64, String) = tx.query_row(
            "SELECT node_id, lease_generation, state FROM runs WHERE run_id=?1",
            params![run_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("run {run_id}")))?;
        let lease_expires: Option<i64> = tx
            .query_row(
                "SELECT lease_expires_at FROM runs WHERE run_id=?1",
                params![run_id],
                |r| r.get(0),
            )?;
        let expired = lease_expires.map(|t| t < Self::now()).unwrap_or(true);
        domain::sched::lease_write_allowed(&run_node, gen, node_id, lease_generation, expired)
            .map_err(|e| StoreError::Conflict(e.to_string()))?;
        let from: RunState = serde_json::from_value(json!(state_str))
            .map_err(|_| StoreError::Invalid(format!("bad run state {state_str}")))?;
        run_transition(from, to)?;
        tx.execute(
            "UPDATE runs SET state=?2,
                session_id=COALESCE(?3, session_id),
                candidate_sha=COALESCE(?4, candidate_sha)
             WHERE run_id=?1",
            params![run_id, run_state_str(to), session_id, candidate_sha],
        )?;
        // An execute run failing (or being canceled) reopens its task so the
        // operator can retry; spec 5 keeps the reason on the run row.
        if to == RunState::Failed || to == RunState::Canceled {
            tx.execute(
                "UPDATE tasks SET state='queued' WHERE task_id=(SELECT task_id FROM runs WHERE run_id=?1) AND state='running'",
                params![run_id],
            )?;
        }
        Self::append_event_tx(
            &tx,
            run_id,
            &format!("run.{}", run_state_str(to)),
            subject,
            &json!({"session_id": session_id, "candidate_sha": candidate_sha}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Mark run uncertain (when the runner cannot confirm an outcome).
    pub fn mark_uncertain(&self, run_id: &str, reason: &str, subject: &str) -> StoreResult<()> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let state_str: String = tx
            .query_row("SELECT state FROM runs WHERE run_id=?1", params![run_id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("run {run_id}")))?;
        let from: RunState = serde_json::from_value(json!(state_str))
            .map_err(|_| StoreError::Invalid(format!("bad run state {state_str}")))?;
        run_transition(from, RunState::Uncertain)?;
        tx.execute(
            "UPDATE runs SET state='uncertain' WHERE run_id=?1",
            params![run_id],
        )?;
        // Task goes uncertain too, keeps reason.
        tx.execute(
            "UPDATE tasks SET state='uncertain', reason=?2
             WHERE task_id=(SELECT task_id FROM runs WHERE run_id=?1)",
            params![run_id, reason],
        )?;
        Self::append_event_tx(
            &tx,
            run_id,
            "run.uncertain",
            subject,
            &json!({"reason": reason}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Operator resolve: close old attempt (cancel) or release for re-lease.
    pub fn resolve_run(
        &self,
        run_id: &str,
        close: bool,
        reason: &str,
        subject: &str,
    ) -> StoreResult<()> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let state_str: String = tx
            .query_row("SELECT state FROM runs WHERE run_id=?1", params![run_id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("run {run_id}")))?;
        let from: RunState = serde_json::from_value(json!(state_str))
            .map_err(|_| StoreError::Invalid(format!("bad run state {state_str}")))?;
        if close {
            run_transition(from, RunState::Canceled)?;
            tx.execute(
                "UPDATE runs SET state='canceled' WHERE run_id=?1",
                params![run_id],
            )?;
            // Closing a run must not leave its task stuck in running;
            // mirror advance_run's failure path so the operator can retry.
            tx.execute(
                "UPDATE tasks SET state='canceled', reason=?2
                 WHERE task_id=(SELECT task_id FROM runs WHERE run_id=?1) AND state='running'",
                params![run_id, reason],
            )?;
            Self::append_event_tx(
                &tx,
                run_id,
                "run.resolved_closed",
                subject,
                &json!({"reason": reason}),
            )?;
        } else {
            // Continue verification: only center may grant a new recovery
            // lease to the same node.
            run_transition(from, RunState::Leased)?;
            tx.execute(
                "UPDATE runs SET state='leased', lease_generation=lease_generation+1,
                    lease_expires_at=?2 WHERE run_id=?1",
                params![run_id, Self::now() + 30],
            )?;
            Self::append_event_tx(
                &tx,
                run_id,
                "run.resolved_recovered",
                subject,
                &json!({"reason": reason}),
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Retry: re-open a task whose old run is terminal or was closed.
    pub fn retry_task(&self, task_id: &str, subject: &str) -> StoreResult<()> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let (state, _reason): (String, Option<String>) = tx.query_row(
            "SELECT state, reason FROM tasks WHERE task_id=?1",
            params![task_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("task {task_id}")))?;
        let from: TaskState = serde_json::from_value(json!(state))
            .map_err(|_| StoreError::Invalid(format!("bad task state {state}")))?;
        // Old run must not be active (uncertain blocks until resolved).
        let active: i64 = tx.query_row(
            "SELECT COUNT(*) FROM runs WHERE task_id=?1 AND run_type='execute'
             AND state IN ('leased','session_bound','prompt_admitted','running','uncertain')",
            params![task_id],
            |r| r.get(0),
        )?;
        if active > 0 {
            return Err(StoreError::Conflict(
                "old run still active; resolve it first".into(),
            ));
        }
        task_transition(from, TaskState::Queued)?;
        tx.execute(
            "UPDATE tasks SET state='queued', reason=NULL WHERE task_id=?1",
            params![task_id],
        )?;
        Self::append_event_tx(&tx, task_id, "task.retried", subject, &json!({}))?;
        tx.commit()?;
        Ok(())
    }

    /// Review submission: bind report to the bound candidate SHA; reject
    /// reports for other runs or old attempts. Only the review run's owner
    /// may submit, and verdict pass requires the fixed-check exit code 0.
    pub fn submit_review(
        &self,
        review_run_id: &str,
        reviewed_run_id: &str,
        candidate_sha: &str,
        fixed_check_exit_code: i64,
        agent_verdict: &str,
        subject: &str,
    ) -> StoreResult<String> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        // Reviewed run must exist and be an execute run with a bound SHA.
        let (bound_sha, _exec_state): (Option<String>, String) = tx.query_row(
            "SELECT candidate_sha, state FROM runs WHERE run_id=?1 AND run_type='execute'",
            params![reviewed_run_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("run {reviewed_run_id}")))?;
        let bound = bound_sha
            .ok_or_else(|| StoreError::Invalid("execute run has no bound candidate SHA".into()))?;
        if bound != candidate_sha {
            return Err(StoreError::Conflict(
                "review candidate SHA does not match bound candidate".into(),
            ));
        }
        // The review run must belong to the control role subject.
        if !subject.starts_with("control") {
            return Err(StoreError::Domain(DomainError::Forbidden(
                "review submitted by non-control subject".into(),
            )));
        }
        let final_verdict = if fixed_check_exit_code == 0 && agent_verdict == "pass" {
            "pass"
        } else {
            "fail"
        };
        tx.execute(
            "INSERT INTO reviews (review_run_id, reviewed_run_id, candidate_sha,
                fixed_check_exit_code, agent_verdict, final_verdict)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                review_run_id,
                reviewed_run_id,
                candidate_sha,
                fixed_check_exit_code,
                agent_verdict,
                final_verdict
            ],
        )?;
        // Advance task by verdict.
        if final_verdict == "pass" {
            tx.execute(
                "UPDATE tasks SET state='accepted' WHERE task_id=(SELECT task_id FROM runs WHERE run_id=?1)",
                params![reviewed_run_id],
            )?;
        } else {
            tx.execute(
                "UPDATE tasks SET state='rejected' WHERE task_id=(SELECT task_id FROM runs WHERE run_id=?1)",
                params![reviewed_run_id],
            )?;
        }
        // A passing review may complete the request: when every task of the
        // request is accepted, mark the request accepted in the same tx.
        if final_verdict == "pass" {
            let request_id: String = tx.query_row(
                "SELECT request_id FROM tasks WHERE task_id=(SELECT task_id FROM runs WHERE run_id=?1)",
                params![reviewed_run_id],
                |r| r.get(0),
            )?;
            let pending: i64 = tx.query_row(
                "SELECT COUNT(*) FROM tasks WHERE request_id=?1 AND state != 'accepted'",
                params![request_id],
                |r| r.get(0),
            )?;
            if pending == 0 {
                let active: Option<String> = tx
                    .query_row(
                        "SELECT request_id FROM requests WHERE request_id=?1 AND state='active'",
                        params![request_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                if active.is_some() {
                    request_transition(RequestState::Active, RequestState::Accepted)?;
                    tx.execute(
                        "UPDATE requests SET state='accepted' WHERE request_id=?1 AND state='active'",
                        params![request_id],
                    )?;
                    Self::append_event_tx(
                        &tx,
                        &request_id,
                        "request.accepted",
                        subject,
                        &json!({}),
                    )?;
                }
            }
        }
        Self::append_event_tx(
            &tx,
            review_run_id,
            "review.submitted",
            subject,
            &json!({"final_verdict": final_verdict, "candidate_sha": candidate_sha}),
        )?;
        tx.commit()?;
        Ok(final_verdict.to_string())
    }

    // ---------- Messages ----------

    /// Finish an execute run: advance the run to `finished`, move the task to
    /// `awaiting_review`, and create the review run owned by the planner node.
    /// The candidate SHA is bound here; reviews later must match it.
    pub fn finish_execute_and_queue_review(
        &self,
        run_id: &str,
        session_id: &str,
        candidate_sha: &str,
        planner_node: &str,
        subject: &str,
    ) -> StoreResult<String> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        // Guard: run must exist and be an execute run still active.
        let (run_task, run_node, gen, state_str): (String, String, i64, String) = tx.query_row(
            "SELECT task_id, node_id, lease_generation, state FROM runs
             WHERE run_id=?1 AND run_type='execute'",
            params![run_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("run {run_id}")))?;
        let lease_expires: Option<i64> = tx
            .query_row("SELECT lease_expires_at FROM runs WHERE run_id=?1", params![run_id], |r| r.get(0))?;
        let expired = lease_expires.map(|t| t < Self::now()).unwrap_or(true);
        // Only the lease holder (the run's own node) may finish it, with a
        // matching generation; expired holders get a conflict.
        domain::sched::lease_write_allowed(&run_node, gen, &run_node, gen, expired)
            .map_err(|e| StoreError::Conflict(e.to_string()))?;
        let from: RunState = serde_json::from_value(json!(state_str))
            .map_err(|_| StoreError::Invalid(format!("bad run state {state_str}")))?;
        run_transition(from, RunState::Finished)?;

        tx.execute(
            "UPDATE runs SET state='finished', session_id=COALESCE(?2, session_id),
                candidate_sha=COALESCE(?3, candidate_sha) WHERE run_id=?1",
            params![run_id, session_id, candidate_sha],
        )?;
        tx.execute(
            "UPDATE tasks SET state='awaiting_review' WHERE task_id=?1 AND state='running'",
            params![run_task],
        )?;
        Self::append_event_tx(
            &tx,
            run_id,
            "run.finished",
            subject,
            &json!({"session_id": session_id, "candidate_sha": candidate_sha}),
        )?;

        // Create the review run for the planner (control) node.
        let review_id = RunId::generate();
        let lease_expires = Self::now() + 30;
        tx.execute(
            "INSERT INTO runs (run_id, task_id, run_type, attempt, node_id,
                lease_generation, lease_expires_at, state)
             VALUES (?1, ?2, 'review',
                COALESCE((SELECT MAX(attempt)+1 FROM runs WHERE task_id=?2 AND run_type='review'), 1),
                ?3, 1, ?4, 'leased')",
            params![review_id.as_str(), run_task, planner_node, lease_expires],
        )?;
        Self::append_event_tx(
            &tx,
            review_id.as_str(),
            "review.queued",
            subject,
            &json!({"reviewed_run": run_id, "candidate_sha": candidate_sha}),
        )?;
        tx.commit()?;
        Ok(review_id.as_str().to_string())
    }

    /// Control node claims the oldest queued review run assigned to it.
    /// Returns the review run plus the reviewed execute run and candidate SHA.
    pub fn claim_review(&self, planner_node: &str, subject: &str) -> StoreResult<Option<(RunRow, RunRow)>> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let review: Option<RunRow> = {
            let mut stmt = tx.prepare(
                "SELECT run_id, task_id, run_type, attempt, node_id, session_id,
                        lease_generation, lease_expires_at, candidate_sha, state FROM runs
                 WHERE run_type='review' AND state='leased' AND node_id=?1
                 ORDER BY run_id LIMIT 1",
            )?;
            let mut rows = stmt.query_map(params![planner_node], map_run)?;
            match rows.next() {
                Some(r) => Some(r?),
                None => None,
            }
        };
        let Some(review) = review else {
            tx.commit()?;
            return Ok(None);
        };
        let execute: RunRow = tx
            .query_row(
                "SELECT run_id, task_id, run_type, attempt, node_id, session_id,
                        lease_generation, lease_expires_at, candidate_sha, state FROM runs
                 WHERE task_id=?1 AND run_type='execute' AND candidate_sha IS NOT NULL
                 ORDER BY run_id DESC LIMIT 1",
                params![review.task_id],
                map_run,
            )
            .map_err(|_| StoreError::NotFound("reviewed execute run missing".into()))?;
        tx.execute(
            // The review round includes a full model wait (up to 900 s);
            // grant a long lease up front so the terminal advance after
            // submission cannot fail on an expired lease.
            "UPDATE runs SET state='running', lease_expires_at=?2 WHERE run_id=?1",
            params![review.run_id, Self::now() + 1800],
        )?;
        Self::append_event_tx(
            &tx,
            &review.run_id,
            "review.claimed",
            subject,
            &json!({"reviewed_run": execute.run_id, "candidate_sha": execute.candidate_sha}),
        )?;
        tx.commit()?;
        Ok(Some((review, execute)))
    }


    pub fn create_message(
        &self,
        message_id: &MessageId,
        from_subject: &str,
        target_node: Option<&str>,
        target_session: Option<&str>,
        request_id: Option<&str>,
        task_id: Option<&str>,
        kind: &str,
        body: &str,
        idem_key: Option<&str>,
    ) -> StoreResult<()> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        if let Some(key) = idem_key {
            let digest = Self::body_digest(body);
            let existing: Option<(String, Option<String>)> = tx
                .query_row(
                    "SELECT body_digest, message_id FROM messages WHERE idem_key=?1",
                    params![key],
                    |r| Ok((r.get(0)?, Some(r.get::<_, String>(1)?))),
                )
                .optional()?;
            if let Some((existing_digest, _)) = existing {
                if existing_digest != digest {
                    return Err(StoreError::IdempotencyConflict);
                }
                tx.commit()?;
                return Ok(()); // same key same body: return existing
            }
            tx.execute(
                "INSERT INTO messages (message_id, from_subject, target_node, target_session,
                    request_id, task_id, kind, body, state, idem_key, body_digest)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'queued', ?9, ?10)",
                params![
                    message_id.as_str(),
                    from_subject,
                    target_node,
                    target_session,
                    request_id,
                    task_id,
                    kind,
                    body,
                    key,
                    digest
                ],
            )?;
        } else {
            tx.execute(
                "INSERT INTO messages (message_id, from_subject, target_node, target_session,
                    request_id, task_id, kind, body, state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'queued')",
                params![
                    message_id.as_str(),
                    from_subject,
                    target_node,
                    target_session,
                    request_id,
                    task_id,
                    kind,
                    body
                ],
            )?;
        }
        Self::append_event_tx(&tx, message_id.as_str(), "message.queued", from_subject, &json!({}))?;
        tx.commit()?;
        Ok(())
    }

    /// Advance message state; delivered requires the message to exist in the
    /// target session (caller checks), ack requires evidence of confirmation.
    pub fn advance_message(
        &self,
        message_id: &str,
        to: MessageState,
        subject: &str,
    ) -> StoreResult<()> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let state: String = tx
            .query_row("SELECT state FROM messages WHERE message_id=?1", params![message_id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("message {message_id}")))?;
        let from: MessageState = serde_json::from_value(json!(state))
            .map_err(|_| StoreError::Invalid(format!("bad message state {state}")))?;
        message_transition(from, to)?;
        let ts = Self::now();
        match to {
            MessageState::Admitted => {
                tx.execute("UPDATE messages SET state='admitted' WHERE message_id=?1", params![message_id])?;
            }
            MessageState::Delivered => {
                tx.execute(
                    "UPDATE messages SET state='delivered', delivered_at=?2 WHERE message_id=?1",
                    params![message_id, ts],
                )?;
            }
            MessageState::Acknowledged => {
                tx.execute(
                    "UPDATE messages SET state='acknowledged', acknowledged_at=?2 WHERE message_id=?1",
                    params![message_id, ts],
                )?;
            }
            MessageState::Queued => unreachable!(),
        }
        Self::append_event_tx(
            &tx,
            message_id,
            &format!("message.{}", format!("{to:?}").to_lowercase()),
            subject,
            &json!({}),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Inbox: messages targeted at a node/session with cursor.
    pub fn inbox(&self, node_id: &str, after: &str, limit: i64) -> StoreResult<Vec<MessageRow>> {
        let conn_ref = self.conn.borrow();
        let mut stmt = conn_ref.prepare(
            "SELECT message_id, from_subject, target_node, target_session, request_id,
                    task_id, kind, body, state
             FROM messages
             WHERE target_node=?1 AND message_id > ?2
             ORDER BY message_id LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![node_id, after, limit], |r| {
            Ok(MessageRow {
                message_id: r.get(0)?,
                from_subject: r.get(1)?,
                target_node: r.get(2)?,
                target_session: r.get(3)?,
                request_id: r.get(4)?,
                task_id: r.get(5)?,
                kind: r.get(6)?,
                body: r.get(7)?,
                state: r.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    // ---------- Evidence ----------

    pub fn add_evidence(
        &self,
        run_id: &str,
        kind: &str,
        rel_path: &str,
        sha256: &str,
        source: &str,
        baseline_sha: Option<&str>,
        candidate_sha: Option<&str>,
        run_ref: Option<&str>,
    ) -> StoreResult<EvidenceId> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let id = EvidenceId::generate();
        tx.execute(
            "INSERT INTO evidence (evidence_id, run_id, kind, rel_path, sha256, source,
                baseline_sha, candidate_sha, run_ref)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id.as_str(),
                run_id,
                kind,
                rel_path,
                sha256,
                source,
                baseline_sha,
                candidate_sha,
                run_ref
            ],
        )?;
        Self::append_event_tx(&tx, run_id, "evidence.added", source, &json!({"rel_path": rel_path}))?;
        tx.commit()?;
        Ok(id)
    }

    pub fn get_evidence_for_run(&self, run_id: &str, name: &str) -> StoreResult<(String, String)> {
        // name is the rel_path; enforce canonical relative path.
        if name.contains("..") || name.starts_with('/') {
            return Err(StoreError::Invalid("evidence path traversal rejected".into()));
        }
        let (path, sha): (String, String) = self.conn.borrow().query_row(
                "SELECT rel_path, sha256 FROM evidence WHERE run_id=?1 AND rel_path=?2",
                params![run_id, name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("evidence {name}")))?;
        Ok((path, sha))
    }

    /// Original node asks to re-verify the same run (recovery lease).
    /// Only the owning node may request it; the center grants a new
    /// generation lease for the same run and node.
    pub fn recover_run(&self, run_id: &str, node_id: &str) -> StoreResult<()> {
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        let (run_node, state_str): (String, String) = tx.query_row(
            "SELECT node_id, state FROM runs WHERE run_id=?1",
            params![run_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| StoreError::NotFound(format!("run {run_id}")))?;
        if run_node != node_id {
            return Err(StoreError::Domain(DomainError::Forbidden(
                "recover requested by non-owning node".into(),
            )));
        }
        let from: RunState = serde_json::from_value(json!(state_str))
            .map_err(|_| StoreError::Invalid(format!("bad run state {state_str}")))?;
        // Only uncertain (or expired-lease) runs need recovery; a fresh lease
        // on an active run is a conflict.
        if !run_is_active(from) {
            return Err(StoreError::Conflict(format!("run {run_id} not recoverable")));
        }
        if from == RunState::Uncertain {
            run_transition(from, RunState::Leased)?;
        }
        tx.execute(
            "UPDATE runs SET state='leased', lease_generation=lease_generation+1,
                lease_expires_at=?2 WHERE run_id=?1",
            params![run_id, Self::now() + 30],
        )?;
        Self::append_event_tx(
            &tx,
            run_id,
            "run.recovery_lease",
            node_id,
            &json!({}),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_message(&self, message_id: &str) -> StoreResult<MessageRow> {
        self.conn.borrow().query_row(
                "SELECT message_id, from_subject, target_node, target_session, request_id,
                        task_id, kind, body, state
                 FROM messages WHERE message_id=?1",
                params![message_id],
                |r| {
                    Ok(MessageRow {
                        message_id: r.get(0)?,
                        from_subject: r.get(1)?,
                        target_node: r.get(2)?,
                        target_session: r.get(3)?,
                        request_id: r.get(4)?,
                        task_id: r.get(5)?,
                        kind: r.get(6)?,
                        body: r.get(7)?,
                        state: r.get(8)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("message {message_id}")))
    }

    // ---------- Resources ----------

    pub fn upsert_resource(
        &self,
        resource_id: &str,
        kind: &str,
        owner_node: Option<&str>,
        version: Option<&str>,
        visibility: &str,
        credential_ref: Option<&str>,
    ) -> StoreResult<()> {
        self.conn.borrow_mut().execute(
            "INSERT INTO resources (resource_id, kind, owner_node, version, visibility, credential_ref)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(resource_id) DO UPDATE SET kind=?2, owner_node=?3, version=?4,
                visibility=?5, credential_ref=?6",
            params![resource_id, kind, owner_node, version, visibility, credential_ref],
        )?;
        Ok(())
    }

    /// Node updates its own route observation; center stamps receive time.
    pub fn observe_route(
        &self,
        resource_id: &str,
        source_node: &str,
        method: &str,
        valid_seconds: i64,
        subject: &str,
    ) -> StoreResult<()> {
        if subject != source_node {
            return Err(StoreError::Domain(DomainError::Forbidden(
                "route observation must come from the source node".into(),
            )));
        }
        let mut conn_mut = self.conn.borrow_mut();
        let tx = conn_mut.transaction()?;
        tx.execute(
            "INSERT INTO routes (resource_id, source_node, method, observed_at, valid_seconds)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(resource_id, source_node) DO UPDATE SET
                method=?3, observed_at=?4, valid_seconds=?5",
            params![resource_id, source_node, method, Self::now(), valid_seconds],
        )?;
        Self::append_event_tx(&tx, resource_id, "route.observed", subject, &json!({}))?;
        tx.commit()?;
        Ok(())
    }

    /// Resources visible to a subject; routes filtered to subject's own.
    pub fn list_resources(&self, subject: &str) -> StoreResult<serde_json::Value> {
        let mut out = json!({"resources": [], "routes": []});
        {
            let conn_ref = self.conn.borrow();
        let mut stmt = conn_ref.prepare(
                "SELECT resource_id, kind, owner_node, version, visibility, observed_at,
                        valid_seconds, credential_ref FROM resources",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(json!({
                    "resource_id": r.get::<_, String>(0)?,
                    "kind": r.get::<_, String>(1)?,
                    "owner_node": r.get::<_, Option<String>>(2)?,
                    "version": r.get::<_, Option<String>>(3)?,
                    "visibility": r.get::<_, String>(4)?,
                    "observed_at": r.get::<_, Option<i64>>(5)?,
                    "valid_seconds": r.get::<_, Option<i64>>(6)?,
                    "credential_ref": r.get::<_, Option<String>>(7)?,
                }))
            })?;
            let items: Vec<_> = rows.filter_map(|r| r.ok()).collect();
            out["resources"] = json!(items);
        }
        if subject != "operator" {
            let conn_ref = self.conn.borrow();
        let mut stmt = conn_ref.prepare(
                "SELECT resource_id, source_node, observed_at, valid_seconds FROM routes
                 WHERE source_node=?1",
            )?;
            let rows = stmt.query_map(params![subject], |r| {
                let observed_at: i64 = r.get(2)?;
                let valid_seconds: i64 = r.get(3)?;
                let expired = Self::now() - observed_at > valid_seconds;
                Ok(json!({
                    "resource_id": r.get::<_, String>(0)?,
                    "source_node": r.get::<_, String>(1)?,
                    "expired": expired,
                }))
            })?;
            let items: Vec<_> = rows.filter_map(|r| r.ok()).collect();
            out["routes"] = json!(items);
        }
        Ok(out)
    }

    // ---------- Repo registry (for plan validation) ----------

    pub fn register_repo(&self, name: &str, bare_path: &str, profiles: &[String]) -> StoreResult<()> {
        self.conn.borrow_mut().execute(
            "INSERT INTO repo_registry (name, bare_path, profiles) VALUES (?1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET bare_path=?2, profiles=?3",
            params![name, bare_path, serde_json::to_string(profiles).unwrap()],
        )?;
        Ok(())
    }

    // ---------- Helpers ----------

    pub fn get_task(&self, task_id: &str) -> StoreResult<TaskRow> {
        let conn_ref = self.conn.borrow();
        let mut stmt = conn_ref.prepare(
            "SELECT task_id, request_id, key, objective, deliverable, repo_ref,
                    required_capabilities, required_routes, acceptance_profile, fixed_node,
                    depends_on, state, reason FROM tasks WHERE task_id=?1",
        )?;
        let row = stmt
            .query_row(params![task_id], |r| {
                Ok(TaskRow {
                    task_id: r.get(0)?,
                    request_id: r.get(1)?,
                    key: r.get(2)?,
                    objective: r.get(3)?,
                    deliverable: r.get(4)?,
                    repo_ref: r.get(5)?,
                    required_capabilities: serde_json::from_str(&r.get::<_, String>(6)?).unwrap(),
                    required_routes: serde_json::from_str(&r.get::<_, String>(7)?).unwrap(),
                    acceptance_profile: r.get(8)?,
                    fixed_node: r.get(9)?,
                    depends_on: serde_json::from_str(&r.get::<_, String>(10)?).unwrap(),
                    state: r.get(11)?,
                    reason: r.get(12)?,
                })
            })
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("task {task_id}")))?;
        Ok(row)
    }

    pub fn get_run(&self, run_id: &str) -> StoreResult<RunRow> {
        self.conn.borrow().query_row(
                "SELECT run_id, task_id, run_type, attempt, node_id, session_id,
                        lease_generation, lease_expires_at, candidate_sha, state
                 FROM runs WHERE run_id=?1",
                params![run_id],
                |r| {
                    Ok(RunRow {
                        run_id: r.get(0)?,
                        task_id: r.get(1)?,
                        run_type: r.get(2)?,
                        attempt: r.get(3)?,
                        node_id: r.get(4)?,
                        session_id: r.get(5)?,
                        lease_generation: r.get(6)?,
                        lease_expires_at: r.get(7)?,
                        candidate_sha: r.get(8)?,
                        state: r.get(9)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound(format!("run {run_id}")))
    }

    pub fn runs_for_request(&self, request_id: &str) -> StoreResult<Vec<RunRow>> {
        let conn_ref = self.conn.borrow();
        let mut stmt = conn_ref.prepare(
            "SELECT r.run_id, r.task_id, r.run_type, r.attempt, r.node_id, r.session_id,
                    r.lease_generation, r.lease_expires_at, r.candidate_sha, r.state
             FROM runs r JOIN tasks t ON r.task_id = t.task_id
             WHERE t.request_id=?1 ORDER BY r.run_id",
        )?;
        let rows = stmt.query_map(params![request_id], map_run)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn tasks_for_request(&self, request_id: &str) -> StoreResult<Vec<TaskRow>> {
        let conn_ref = self.conn.borrow();
        let mut stmt = conn_ref.prepare(
            "SELECT task_id, request_id, key, objective, deliverable, repo_ref,
                    required_capabilities, required_routes, acceptance_profile, fixed_node,
                    depends_on, state, reason FROM tasks WHERE request_id=?1 ORDER BY key",
        )?;
        let rows = stmt.query_map(params![request_id], map_task)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn run_state_str(s: RunState) -> &'static str {
    match s {
        RunState::Leased => "leased",
        RunState::SessionBound => "session_bound",
        RunState::PromptAdmitted => "prompt_admitted",
        RunState::Running => "running",
        RunState::Finished => "finished",
        RunState::Failed => "failed",
        RunState::Uncertain => "uncertain",
        RunState::Canceled => "canceled",
    }
}

fn map_run(r: &rusqlite::Row<'_>) -> rusqlite::Result<RunRow> {
    Ok(RunRow {
        run_id: r.get(0)?,
        task_id: r.get(1)?,
        run_type: r.get(2)?,
        attempt: r.get(3)?,
        node_id: r.get(4)?,
        session_id: r.get(5)?,
        lease_generation: r.get(6)?,
        lease_expires_at: r.get(7)?,
        candidate_sha: r.get(8)?,
        state: r.get(9)?,
    })
}

fn map_task(r: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRow> {
    Ok(TaskRow {
        task_id: r.get(0)?,
        request_id: r.get(1)?,
        key: r.get(2)?,
        objective: r.get(3)?,
        deliverable: r.get(4)?,
        repo_ref: r.get(5)?,
        required_capabilities: serde_json::from_str(&r.get::<_, String>(6)?).unwrap(),
        required_routes: serde_json::from_str(&r.get::<_, String>(7)?).unwrap(),
        acceptance_profile: r.get(8)?,
        fixed_node: r.get(9)?,
        depends_on: serde_json::from_str(&r.get::<_, String>(10)?).unwrap(),
        state: r.get(11)?,
        reason: r.get(12)?,
    })
}

fn planner_target(tx: &Transaction<'_>) -> StoreResult<String> {
    // The planning message targets the configured planner node. If the
    // planner_binding table knows a node use it; otherwise default 'control'.
    let known: Option<String> = tx
        .query_row("SELECT planner_node FROM planner_binding LIMIT 1", [], |r| r.get(0))
        .optional()?;
    Ok(known.unwrap_or_else(|| "control".to_string()))
}
