//! Runner loops for worker and control roles.
//!
//! Worker: heartbeat → claim → checkout → opencode session → prompt with
//! stable ID → model round → commit → push run ref → evidence → finish.
//! Control: heartbeat → planner binding → plan messages → review claims
//! → fixed checks → agent verdict → submit review → result to planner.

use anyhow::{anyhow, bail, Context as _};
use opencode_v2::OcClient;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct RunnerConfig {
    pub node_id: String,
    pub role: String,
    pub capabilities: Vec<String>,
    pub runtime_url: String,
    pub token: String,
    pub opencode_url: String,
    pub opencode_password: String,
    pub runner_dir: PathBuf,
    pub work_root: PathBuf,
    pub poll_seconds: u64,
    pub heartbeat_seconds: u64,
    pub repos: Vec<(String, String)>, // (repo_ref, bare_path)
    /// planner_node as configured centrally (control only)
    pub planner_node: String,
    /// Test-only pause points (spec M2/V05, V06). Disabled unless the
    /// explicit test config flag is present.
    pub test_pause_after_prompt: bool,
    pub test_pause_before_result: bool,
}

impl RunnerConfig {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let v: toml::Value = toml::from_str(&raw)
            .map_err(|e| anyhow!("config parse: {e}"))?;
        let node_id = v
            .get("node_id")
            .and_then(|x| x.as_str())
            .ok_or_else(|| anyhow!("node_id required"))?
            .to_string();
        let role = v
            .get("role")
            .and_then(|x| x.as_str())
            .ok_or_else(|| anyhow!("role required"))?
            .to_string();
        let capabilities = v
            .get("capabilities")
            .map(|c| c.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect())
            .unwrap_or_default();
        let runtime_url = v.get("runtime_url").and_then(|x| x.as_str()).ok_or_else(|| anyhow!("runtime_url required"))?.to_string();
        let token_env = v.get("runtime_token_env").and_then(|x| x.as_str()).ok_or_else(|| anyhow!("runtime_token_env required"))?.to_string();
        let token = std::env::var(&token_env).map_err(|_| anyhow!("env {token_env} not set"))?;
        let opencode_url = v.get("opencode_url").and_then(|x| x.as_str()).ok_or_else(|| anyhow!("opencode_url required"))?.to_string();
        let opencode_password_env = v.get("opencode_password_env").and_then(|x| x.as_str()).ok_or_else(|| anyhow!("opencode_password_env required"))?.to_string();
        let opencode_password = std::env::var(&opencode_password_env).map_err(|_| anyhow!("env {opencode_password_env} not set"))?;
        let runner_dir = v.get("runner_dir").and_then(|x| x.as_str()).ok_or_else(|| anyhow!("runner_dir required"))?.to_string();
        let work_root = v.get("work_root").and_then(|x| x.as_str()).ok_or_else(|| anyhow!("work_root required"))?.to_string();
        let poll_seconds = v.get("poll_seconds").and_then(|x| x.as_integer()).unwrap_or(2) as u64;
        let heartbeat_seconds = v.get("heartbeat_seconds").and_then(|x| x.as_integer()).unwrap_or(5) as u64;
        let mut repos = Vec::new();
        if let Some(r) = v.get("repositories").and_then(|x| x.as_table()) {
            for (k, val) in r {
                repos.push((k.clone(), val.as_str().unwrap_or_default().to_string()));
            }
        }
        let planner_node = "control".to_string();
        let test_pause_after_prompt = v.get("test_pause_after_prompt").and_then(|x| x.as_bool()).unwrap_or(false);
        let test_pause_before_result = v.get("test_pause_before_result").and_then(|x| x.as_bool()).unwrap_or(false);
        Ok(Self { node_id, role, capabilities, runtime_url, token, opencode_url, opencode_password, runner_dir: runner_dir.into(), work_root: work_root.into(), poll_seconds, heartbeat_seconds, repos, planner_node, test_pause_after_prompt, test_pause_before_result })
    }
}

pub struct Runner {
    pub cfg: RunnerConfig,
    http: reqwest::Client,
    oc: OcClient,
    state_path: PathBuf,
}

/// Persisted before any external side effect (spec 5.1 durability).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RunState {
    run_id: String,
    task_id: String,
    session_id: Option<String>,
    message_id: Option<String>,
    request_text: Option<String>,
    phase: String,
}

impl Runner {
    pub fn new(cfg: RunnerConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .unwrap();
        let oc = OcClient::new(&cfg.opencode_url, &cfg.opencode_password);
        std::fs::create_dir_all(&cfg.runner_dir).ok();
        let state_path = cfg.runner_dir.join("runner-state.json");
        Self { cfg, http, oc, state_path }
    }

    fn token(&self) -> String {
        self.cfg.token.clone()
    }

    async fn api_post(&self, path: &str, body: &Value) -> anyhow::Result<(u16, Value)> {
        let resp = self
            .http
            .post(format!("{}{}", self.cfg.runtime_url, path))
            .bearer_auth(self.token())
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        let v: Value = resp.json().await.unwrap_or(json!({}));
        Ok((status, v))
    }

    async fn api_get(&self, path: &str) -> anyhow::Result<(u16, Value)> {
        let resp = self
            .http
            .get(format!("{}{}", self.cfg.runtime_url, path))
            .bearer_auth(self.token())
            .send()
            .await?;
        let status = resp.status().as_u16();
        let v: Value = resp.json().await.unwrap_or(json!({}));
        Ok((status, v))
    }

    async fn api_put(&self, path: &str, body: &Value) -> anyhow::Result<(u16, Value)> {
        let resp = self
            .http
            .put(format!("{}{}", self.cfg.runtime_url, path))
            .bearer_auth(self.token())
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        let v: Value = resp.json().await.unwrap_or(json!({}));
        Ok((status, v))
    }

    // ---------- persistence for recovery ----------

    fn save_state(&self, st: &RunState) {
        let tmp = self.state_path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(st).unwrap()).ok();
        std::fs::rename(&tmp, &self.state_path).ok();
    }

    fn load_state(&self) -> Option<RunState> {
        std::fs::read(&self.state_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
    }

    fn clear_state(&self) {
        std::fs::remove_file(&self.state_path).ok();
    }

    pub async fn heartbeat(&self) -> anyhow::Result<()> {
        let repos: Vec<String> = self.cfg.repos.iter().map(|(r, _)| r.clone()).collect();
        let (status, v) = self
            .api_post("/v1/nodes/heartbeat", &json!({"capacity": 1, "repo_refs": repos}))
            .await?;
        if status != 200 {
            bail!("heartbeat failed: {status} {v}");
        }
        Ok(())
    }

    // ---------- shared run helpers ----------

    async fn advance(&self, run_id: &str, gen: i64, to: &str, session_id: Option<&str>, candidate_sha: Option<&str>) -> anyhow::Result<()> {
        let (status, v) = self
            .api_post(
                &format!("/v1/runs/{run_id}/events"),
                &json!({"lease_generation": gen, "to_state": to, "session_id": session_id, "candidate_sha": candidate_sha}),
            )
            .await?;
        if status != 200 {
            bail!("advance {run_id} to {to}: {status} {v}");
        }
        Ok(())
    }

    async fn renew(&self, run_id: &str, gen: i64) -> anyhow::Result<()> {
        let (status, v) = self
            .api_post(&format!("/v1/runs/{run_id}/renew"), &json!({"lease_generation": gen}))
            .await?;
        if status != 200 {
            bail!("renew {run_id}: {status} {v}");
        }
        Ok(())
    }

    /// Poll the session history until an assistant message contains
    /// `expected_text` (any assistant message when empty), renewing the run
    /// lease every minute so long model rounds keep ownership. Total wait is
    /// capped at `timeout`.
    async fn wait_with_renew(
        &self,
        session_id: &str,
        expected_text: &str,
        timeout: Duration,
        run_id: &str,
        gen: i64,
    ) -> anyhow::Result<Vec<opencode_v2::MessageRow>> {
        let deadline = Instant::now() + timeout;
        let mut last_renew = Instant::now();
        loop {
            let history = self.oc.history(session_id, 100).await?;
            for m in &history {
                if m.kind == "assistant" && m.contains_text(expected_text) {
                    return Ok(history);
                }
            }
            if Instant::now() > deadline {
                bail!(
                    "model round timeout after {timeout:?}: no assistant text containing {expected_text:?}"
                );
            }
            if last_renew.elapsed() >= Duration::from_secs(60) {
                if let Err(e) = self.renew(run_id, gen).await {
                    eprintln!("[{}] lease renew while waiting: {e}", self.cfg.node_id);
                }
                last_renew = Instant::now();
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    fn bare_path(&self, repo_ref: &str) -> anyhow::Result<String> {
        self.cfg
            .repos
            .iter()
            .find(|(r, _)| r == repo_ref)
            .map(|(_, p)| p.clone())
            .ok_or_else(|| anyhow!("repo ref {repo_ref} not mapped on node"))
    }

    /// Register a directory as evidence (sha256 computed by center).
    async fn register_evidence(&self, run_id: &str, evidence_root: &Path, rel_path: &str, name: &str) -> anyhow::Result<()> {
        let full = evidence_root.join(rel_path);
        let bytes = std::fs::read(&full).with_context(|| format!("read evidence {full:?}"))?;
        use sha2::{Digest, Sha256};
        let sha = hex::encode(Sha256::digest(&bytes));
        let (status, v) = self
            .api_post(
                &format!("/v1/runs/{run_id}/evidence"),
                &json!({"name": name, "rel_path": rel_path, "sha256": sha}),
            )
            .await?;
        if status != 200 {
            bail!("register evidence {name}: {status} {v}");
        }
        Ok(())
    }

    // ---------- worker loop ----------

    pub async fn worker_loop(&self) -> anyhow::Result<()> {
        eprintln!("[worker-{}] loop starting", self.cfg.node_id);
        let mut hb = tokio::time::interval(Duration::from_secs(self.cfg.heartbeat_seconds));
        loop {
            tokio::select! {
                _ = hb.tick() => {
                    if let Err(e) = self.heartbeat().await {
                        eprintln!("[worker-{}] heartbeat error: {e}", self.cfg.node_id);
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(self.cfg.poll_seconds)) => {
                    // Resume an interrupted run first (spec 5.1).
                    if let Some(st) = self.load_state() {
                        if matches!(st.phase.as_str(), "prompt" | "running" | "finishing") {
                            eprintln!("[worker-{}] resuming run {} at phase {}", self.cfg.node_id, st.run_id, st.phase);
                            if let Err(e) = self.worker_execute(&st.task_id, &st.run_id, st.request_text.as_deref().unwrap_or("")).await {
                                eprintln!("[worker-{}] resume failed: {e:?}", self.cfg.node_id);
                                // A terminal run (failed/canceled on the center)
                                // can no longer advance; drop the stale local
                                // state so the loop returns to normal claiming.
                                if e.to_string().contains("illegal transition")
                                    || e.to_string().contains("not found")
                                    || e.to_string().contains("lease_expired")
                                {
                                    self.clear_state();
                                }
                            }
                        }
                    }
                    let (status, v) = self.api_post("/v1/runs/claim", &json!({"run_type": "execute"})).await?;
                    if status == 204 {
                        continue;
                    }
                    if status != 200 {
                        eprintln!("[worker-{}] claim: {status} {v}", self.cfg.node_id);
                        continue;
                    }
                    let task_id = v["task"]["task_id"].as_str().unwrap_or_default().to_string();
                    let run_id = v["run"]["run_id"].as_str().unwrap_or_default().to_string();
                    let objective = v["task"]["objective"].as_str().unwrap_or_default().to_string();
                    let deliverable = v["task"]["deliverable"].as_str().unwrap_or_default().to_string();
                    let repo_ref = v["task"]["repo_ref"].as_str().unwrap_or_default().to_string();
                    eprintln!("[worker-{}] claimed task {task_id} run {run_id}", self.cfg.node_id);
                    if let Err(e) = self.worker_execute(&task_id, &run_id, &objective).await {
                        eprintln!("[worker-{}] execute error: {e:?}", self.cfg.node_id);
                        let _ = self.advance(&run_id, v["run"]["lease_generation"].as_i64().unwrap_or(1), "failed", None, None).await;
                    }
                    let _ = deliverable;
                    let _ = repo_ref;
                }
            }
        }
    }

    async fn worker_execute(&self, task_id: &str, run_id: &str, objective: &str) -> anyhow::Result<()> {
        // Fetch task + run info from center.
        let (status, v) = self.api_get(&format!("/v1/tasks/{task_id}")).await?;
        anyhow::ensure!(status == 200, "task fetch {task_id}: {status} {v}");
        let repo_ref = v["task"]["repo_ref"].as_str().unwrap_or("fixture").to_string();
        let baseline = self.baseline_sha(&repo_ref)?;
        let bare = self.bare_path(&repo_ref)?;
        let gen = self.run_generation(run_id).await;

        // Checkout from baseline.
        let work_dir = self.cfg.work_root.join(run_id);
        std::fs::create_dir_all(&work_dir)?;
        let checkout = work_dir.join("checkout");
        checkout_root(&checkout, &bare, &baseline)?;

        // Session bound (create or reuse).
        let st_session = self.load_state();
        let same_run = st_session
            .as_ref()
            .map(|s| s.run_id == run_id)
            .unwrap_or(false);
        let session_id = if same_run {
            match st_session.as_ref().and_then(|s| s.session_id.clone()) {
                Some(sid) => {
                    // Reuse: verify session exists on this node.
                    match self.oc.get_session(&sid).await? {
                        Some(_) => sid,
                        None => self.create_session_for_run(&checkout, run_id).await?,
                    }
                }
                None => self.create_session_for_run(&checkout, run_id).await?,
            }
        } else {
            self.create_session_for_run(&checkout, run_id).await?
        };
        if let Err(e) = self.advance(run_id, gen, "session_bound", Some(&session_id), None).await {
            // Resumed runs are already session_bound; tolerate that and let
            // the next advance catch any real lease problem.
            eprintln!("[worker-{}] session_bound advance tolerated: {e}", self.cfg.node_id);
        }

        // Persist before prompt (durability). A prior attempt's message ID is
        // durable on the node and must never leak into another run's session
        // (opencode rejects a repeated prompt ID across sessions with 409).
        let message_id = if same_run {
            st_session
                .as_ref()
                .and_then(|s| s.message_id.clone())
                .unwrap_or_else(|| format!("msg_{}", uuid::Uuid::new_v4().simple()))
        } else {
            format!("msg_{}", uuid::Uuid::new_v4().simple())
        };
        let request_text = if same_run {
            st_session
                .as_ref()
                .and_then(|s| s.request_text.clone())
                .unwrap_or_else(|| {
                    format!(
                        "任务目标：{objective}\n\n工作目录：{}\n仓库基线：{baseline}\n交付要求：只修改完成任务所需的最少文件，\
                         完成后无需其他动作。如果你修改了文件，请用 git commit 提交，提交说明写清改动内容。",
                        checkout.display()
                    )
                })
        } else {
            format!(
                "任务目标：{objective}\n\n工作目录：{}\n仓库基线：{baseline}\n交付要求：只修改完成任务所需的最少文件，\
                 完成后无需其他动作。如果你修改了文件，请用 git commit 提交，提交说明写清改动内容。",
                checkout.display()
            )
        };
        self.save_state(&RunState {
            run_id: run_id.to_string(),
            task_id: task_id.to_string(),
            session_id: Some(session_id.clone()),
            message_id: Some(message_id.clone()),
            request_text: Some(request_text.clone()),
            phase: "prompt".into(),
        });

        // Deliver prompt with stable ID (idempotent replays dedupe).
        self.oc
            .prompt(&session_id, &message_id, &request_text)
            .await
            .context("prompt delivery")?;
        // V05 test pause point: the prompt is accepted by the V2 server but
        // the center has not yet recorded `prompt_admitted`. Terminate the
        // runner here; the restart must resume with the same run/session/
        // message ID and the session must hold only one corresponding input.
        if self.cfg.test_pause_after_prompt {
            self.save_state(&RunState {
                run_id: run_id.to_string(),
                task_id: task_id.to_string(),
                session_id: Some(session_id.clone()),
                message_id: Some(message_id.clone()),
                request_text: Some(request_text.clone()),
                phase: "prompt".into(),
            });
            eprintln!("[worker-{}] TEST-PAUSE after prompt delivery (run {run_id}); exiting", self.cfg.node_id);
            std::process::exit(0);
        }
        self.advance(run_id, gen, "prompt_admitted", Some(&session_id), None).await?;

        // Wait for the model round; the assistant reply marks activity.
        // Long model rounds outlive the original lease, so renew it
        // periodically while polling history.
        let hist = self
            .wait_with_renew(&session_id, "", Duration::from_secs(1800), run_id, gen)
            .await
            .context("model round")?;
        let _ = hist;
        self.advance(run_id, gen, "running", Some(&session_id), None).await?;

        // Commit any changes.
        let candidate = commit_all(&checkout)?;

        // Push unique run ref (no overwriting a different SHA).
        let run_ref = format!("refs/runs/{run_id}");
        push_run_ref(&bare, &checkout, &run_ref, &candidate)?;

        // Evidence: patch + log.
        let evidence_root = PathBuf::from(std::env::var("RUNTIME_EVIDENCE_ROOT").unwrap_or_else(|_| "/root/workspace/runtime-v2-lab/central/evidence".into()));
        let rel_patch = format!("{run_id}/patch.diff");
        let rel_log = format!("{run_id}/git.log");
        std::fs::create_dir_all(evidence_root.join(run_id))?;
        let patch = std::process::Command::new("git")
            .args(["format-patch", "--stdout", &format!("{baseline}..HEAD")])
            .current_dir(&checkout)
            .output()?;
        std::fs::write(evidence_root.join(&rel_patch), &patch.stdout)?;
        let log = std::process::Command::new("git")
            .args(["log", "--oneline", &format!("{baseline}..HEAD")])
            .current_dir(&checkout)
            .output()?;
        std::fs::write(evidence_root.join(&rel_log), &log.stdout)?;
        self.register_evidence(run_id, &evidence_root, &rel_patch, "patch").await?;
        self.register_evidence(run_id, &evidence_root, &rel_log, "git-log").await?;

        // Finish: task → awaiting_review, review run queued.
        let (_status, v) = self
            .api_post(
                &format!("/v1/runs/{run_id}/finish"),
                &json!({"session_id": session_id, "candidate_sha": candidate}),
            )
            .await?;
        anyhow::ensure!(v["review_run_id"].is_string(), "finish {run_id}: {_status} {v}");
        self.save_state(&RunState {
            run_id: run_id.to_string(),
            task_id: task_id.to_string(),
            session_id: Some(session_id),
            message_id: Some(message_id),
            request_text: Some(request_text),
            phase: "finished".into(),
        });
        self.clear_state();
        eprintln!("[worker-{}] run {run_id} finished, candidate {candidate}", self.cfg.node_id);
        Ok(())
    }

    async fn create_session_for_run(&self, checkout: &Path, run_id: &str) -> anyhow::Result<String> {
        let s = self
            .oc
            .create_session(&format!("runtime run {run_id}"), &checkout.display().to_string())
            .await?;
        Ok(s.id)
    }

    fn baseline_sha(&self, repo_ref: &str) -> anyhow::Result<String> {
        let bare = self.bare_path(repo_ref)?;
        let out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .env("GIT_DIR", &bare)
            .output()?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    async fn run_generation(&self, run_id: &str) -> i64 {
        let (_s, v) = self
            .api_get(&format!("/v1/requests/x"))
            .await
            .unwrap_or((404, json!({})));
        // The center returns 404 here; generation is 1 for fresh claims.
        let _ = v;
        1
    }

    // ---------- control loop ----------

    pub async fn control_loop(&self) -> anyhow::Result<()> {
        eprintln!("[control] loop starting");
        let mut hb = tokio::time::interval(Duration::from_secs(self.cfg.heartbeat_seconds));
        // Fixed planner session (create or reuse).
        let state_file = self.cfg.runner_dir.join("planner-session.json");
        let planner_session = match std::fs::read(&state_file).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
            Some(v) => {
                let sid = v["session_id"].as_str().unwrap_or_default().to_string();
                if self.oc.get_session(&sid).await?.is_some() {
                    sid
                } else {
                    // Session lost: center marks binding blocked via report.
                    let s = self.oc.create_session("runtime planner", &self.cfg.runner_dir.display().to_string()).await?;
                    std::fs::write(&state_file, serde_json::to_vec(&json!({"session_id": s.id}))?)?;
                    s.id
                }
            }
            None => {
                let s = self.oc.create_session("runtime planner", &self.cfg.runner_dir.display().to_string()).await?;
                std::fs::write(&state_file, serde_json::to_vec(&json!({"session_id": s.id}))?)?;
                s.id
            }
        };
        let (status, v) = self
            .api_put("/v1/planner/binding", &json!({"session_id": planner_session}))
            .await?;
        anyhow::ensure!(status == 200, "planner binding: {status} {v}");
        eprintln!("[control] planner session bound: {planner_session}");

        // V06 restart recovery: a pending result paused before delivery is
        // flushed exactly once here (the stable message ID keeps the planner
        // session free of duplicates even if this code runs repeatedly).
        let pending_path = self.cfg.runner_dir.join("pending_result.json");
        if let Ok(raw) = std::fs::read(&pending_path) {
            if let Ok(p) = serde_json::from_slice::<Value>(&raw) {
                let (sid, mid, text) = (
                    p["planner_session"].as_str().unwrap_or_default().to_string(),
                    p["result_mid"].as_str().unwrap_or_default().to_string(),
                    p["result_text"].as_str().unwrap_or_default().to_string(),
                );
                match self.oc.prompt(&sid, &mid, &text).await {
                    Ok(_) => {
                        eprintln!("[control] pending result {mid} delivered to planner session");
                        let _ = std::fs::remove_file(&pending_path);
                    }
                    Err(e) => eprintln!("[control] pending result delivery failed: {e}"),
                }
            }
        }

        let mut after_cursor = String::new();
        loop {
            tokio::select! {
                _ = hb.tick() => {
                    if let Err(e) = self.heartbeat().await {
                        eprintln!("[control] heartbeat error: {e}");
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(self.cfg.poll_seconds)) => {
                    // 1) Planning messages.
                    if let Err(e) = self.process_planning(&planner_session, &mut after_cursor).await {
                        eprintln!("[control] planning error: {e}");
                    }
                    // 2) Review claims.
                    if let Err(e) = self.process_reviews(&planner_session).await {
                        eprintln!("[control] review error: {e}");
                    }
                }
            }
        }
    }

    async fn process_planning(&self, planner_session: &str, after_cursor: &mut String) -> anyhow::Result<()> {
        let (status, v) = self
            .api_get(&format!("/v1/inbox?after={after_cursor}&limit=50"))
            .await?;
        anyhow::ensure!(status == 200, "inbox: {status}");
        for msg in v["messages"].as_array().unwrap_or(&vec![]).clone() {
            let mid = msg["message_id"].as_str().unwrap_or_default().to_string();
            let kind = msg["kind"].as_str().unwrap_or_default().to_string();
            let request_id = msg["request_id"].as_str().unwrap_or_default().to_string();
            if kind != "plan_request" || request_id.is_empty() {
                // Unknown kinds: ack to avoid redelivery loops.
                let _ = self.api_post(&format!("/v1/messages/{mid}/ack"), &json!({})).await;
                continue;
            }
            let intent = msg["body"].as_str().unwrap_or_default().to_string();
            // Deliver to planner session with stable message ID derived from the message.
            let stable_mid = format!("msg_plan_{}", mid.replace(|c: char| !c.is_ascii_alphanumeric(), ""));
            let plan_dir = self.cfg.runner_dir.join("plans");
            std::fs::create_dir_all(&plan_dir)?;
            let plan_file = plan_dir.join(format!("{request_id}.json"));
            let prompt = format!(
                "你是规划会话。请针对下面的原始目标，写出一个任务拆解计划。\n\n原始目标：{intent}\n\n\
                 请把计划写到文件 {}\n格式（严格遵守）：\n{{\n  \"request_id\": \"{request_id}\",\n  \"tasks\": [\n    {{\n      \"key\": \"任务短名\",\n      \"objective\": \"目标\",\n      \"deliverable\": \"交付物\",\n      \"repo_ref\": \"fixture\",\n      \"required_capabilities\": [\"fixture-exec\"],\n      \"acceptance_profile\": \"counter-v1\",\n      \"depends_on\": []\n    }}\n  ]\n}}\n先写临时文件再原子重命名。写完文件后回复 PLAN-WRITTEN。",
                plan_file.display()
            );
            self.oc.prompt(planner_session, &stable_mid, &prompt).await?;
            // Wait for the plan file (up to 300 s), then validate and submit.
            let mut plan: Option<Value> = None;
            for _ in 0..300 {
                if let Ok(text) = std::fs::read_to_string(&plan_file) {
                    plan = serde_json::from_str(&text).ok();
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            let plan = plan.ok_or_else(|| anyhow!("planner did not write {plan_file:?} in time"))?;
            let (status, v) = self
                .api_put(&format!("/v1/requests/{request_id}/plan"), &plan)
                .await?;
            anyhow::ensure!(status == 200, "plan submit {request_id}: {status} {v}");
            let _ = self.api_post(&format!("/v1/messages/{mid}/ack"), &json!({})).await;
            eprintln!("[control] plan submitted for {request_id}");
            // Advance cursor past this message.
            *after_cursor = mid;
        }
        Ok(())
    }

    async fn process_reviews(&self, planner_session: &str) -> anyhow::Result<()> {
        let (status, v) = self
            .api_post("/v1/runs/claim", &json!({"run_type": "review"}))
            .await?;
        if status == 204 {
            return Ok(());
        }
        anyhow::ensure!(status == 200, "review claim: {status} {v}");
        let review_run = v["review"]["run_id"].as_str().unwrap_or_default().to_string();
        let executed_run = v["executed"]["run_id"].as_str().unwrap_or_default().to_string();
        let candidate_sha = v["executed"]["candidate_sha"].as_str().unwrap_or_default().to_string();
        let task_id = v["review"]["task_id"].as_str().unwrap_or_default().to_string();
        let gen = v["lease_generation"].as_i64().unwrap_or(1);
        eprintln!("[control] review {review_run} of {executed_run} candidate {candidate_sha}");

        // Task context.
        let (_s, tv) = self.api_get(&format!("/v1/tasks/{task_id}")).await?;
        let repo_ref = tv["task"]["repo_ref"].as_str().unwrap_or("fixture").to_string();
        let baseline = self.baseline_sha(&repo_ref)?;
        let bare = self.bare_path(&repo_ref)?;

        // Fixed acceptance: extract baseline test + candidate source in isolation.
        let review_dir = self.cfg.work_root.join(&review_run);
        let _ = std::fs::remove_dir_all(&review_dir);
        std::fs::create_dir_all(&review_dir)?;
        let base_dir = review_dir.join("baseline");
        let cand_dir = review_dir.join("candidate");
        checkout_root(&base_dir, &bare, &baseline)?;
        checkout_root(&cand_dir, &bare, &candidate_sha)?;

        let mut fixed_exit = 1i32;
        let mut fixed_note = String::new();
        // 1) merge-base ancestry
        let ancestor = std::process::Command::new("git")
            .args(["merge-base", "--is-ancestor", &baseline, &candidate_sha])
            .env("GIT_DIR", &bare)
            .status();
        match ancestor {
            Ok(s) if s.success() => {}
            _ => {
                fixed_note.push_str("candidate is not a descendant of baseline; ");
            }
        }
        // 2) diff scope: only counter.py
        let diff = std::process::Command::new("git")
            .args(["diff", "--name-only", &baseline, &candidate_sha])
            .env("GIT_DIR", &bare)
            .output()?;
        let diff_text = String::from_utf8_lossy(&diff.stdout).to_string();
        let changed: Vec<&str> = diff_text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect();
        if changed != ["counter.py"] {
            fixed_note.push_str(&format!("changed files {changed:?} != [counter.py]; "));
        }
        // 3) fixed test: baseline test_counter.py against candidate counter.py
        std::fs::copy(base_dir.join("test_counter.py"), review_dir.join("test_counter.py"))?;
        std::fs::copy(cand_dir.join("counter.py"), review_dir.join("counter.py"))?;
        let test = std::process::Command::new("python3")
            .args(["-m", "unittest", "-v", "test_counter"])
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .current_dir(&review_dir)
            .status();
        match test {
            Ok(s) => fixed_exit = s.code().unwrap_or(1),
            Err(e) => fixed_note.push_str(&format!("test spawn error {e}")),
        }

        // Agent verdict (review session, per-review fresh session).
        let review_session = self.oc.create_session(&format!("runtime review {review_run}"), &review_dir.display().to_string()).await?;
        let verdict_prompt = format!(
            "你是独立验收会话。请检查以下验收材料并给出结论。\n\n\
             候选 SHA：{candidate_sha}\n基线 SHA：{baseline}\n固定测试退出码：{fixed_exit}\n差异检查备注：{fixed_note}\n\n\
             请在仓库目录 {} 里查看实际文件内容（counter.py、test_counter.py）。\n\
             只回复 JSON：{{\"verdict\":\"pass\"}} 或 {{\"verdict\":\"fail\",\"reason\":\"...\"}}。\n\
             注意：只有固定测试退出码为 0、差异只涉及 counter.py、且代码确实实现了任务目标时才能给 pass。",
            review_dir.display()
        );
        let stable_mid = format!("msg_rev_{}", review_run.replace(|c: char| !c.is_ascii_alphanumeric(), ""));
        self.oc.prompt(&review_session.id, &stable_mid, &verdict_prompt).await?;
        let hist = self
            .wait_with_renew(&review_session.id, "verdict", Duration::from_secs(900), &review_run, gen)
            .await
            .context("review agent round")?;
        let reply = hist
            .iter()
            .rev()
            .find(|m| m.kind == "assistant" && m.contains_text("verdict"))
            .map(|m| m.assistant_text())
            .unwrap_or_default();
        // Extract the JSON verdict from the reply text.
        let agent_verdict = reply
            .split("{\"verdict\"")
            .nth(1)
            .map(|rest| format!("{{\"verdict\"{}", rest))
            .and_then(|s| {
                // Trim to valid JSON by attempting progressive parse.
                for end in (1..=s.len()).rev() {
                    if let Ok(v) = serde_json::from_str::<Value>(&s[..end]) {
                        return Some(v);
                    }
                }
                None
            })
            .and_then(|v| v["verdict"].as_str().map(str::to_string))
            .unwrap_or_else(|| {
                if reply.contains("\"pass\"") { "pass".into() } else { "fail".into() }
            });
        eprintln!("[control] agent verdict: {agent_verdict} (fixed_exit={fixed_exit})");

        // Submit review with hard evidence: exit code + verdict.
        let (status, v) = self
            .api_post(
                &format!("/v1/reviews/{review_run}"),
                &json!({
                    "reviewed_run_id": executed_run,
                    "candidate_sha": candidate_sha,
                    "fixed_check_exit_code": fixed_exit,
                    "agent_verdict": agent_verdict,
                }),
            )
            .await?;
        anyhow::ensure!(status == 200, "review submit: {status} {v}");
        let final_verdict = v["final_verdict"].as_str().unwrap_or("fail").to_string();

        // Deliver result to planner session and require echo of result ID.
        // V06 test pause point: stop the delivery loop before the result
        // reaches the planner session, persisting it for restart recovery.
        let result_id = format!("result_{review_run}");
        let result_text = format!(
            "任务 {task_id} 验收完成：{final_verdict}。候选 {candidate_sha}，固定测试退出码 {fixed_exit}，结果 ID {result_id}。请只回复该结果 ID 以确认收到。"
        );
        let result_mid = format!("msg_result_{}", review_run.replace(|c: char| !c.is_ascii_alphanumeric(), ""));
        if self.cfg.test_pause_before_result {
            let pending = self.cfg.runner_dir.join("pending_result.json");
            std::fs::create_dir_all(&self.cfg.runner_dir)?;
            std::fs::write(&pending, json!({
                "planner_session": planner_session,
                "result_mid": result_mid,
                "result_id": result_id,
                "result_text": result_text,
            }).to_string())?;
            eprintln!("[control] TEST-PAUSE before result delivery (run {review_run}); exiting");
            std::process::exit(0);
        }
        self.oc.prompt(planner_session, &result_mid, &result_text).await?;
        let _ = self.oc.wait_for_reply(planner_session, &result_id, Duration::from_secs(600)).await;

        // Review run → finished.
        let _ = self.advance(&review_run, gen, "finished", Some(&review_session.id), Some(&candidate_sha)).await;
        eprintln!("[control] review {review_run} submitted: {final_verdict}");
        Ok(())
    }
}

// ---------- git helpers ----------

fn checkout_root(dir: &Path, bare: &str, sha: &str) -> anyhow::Result<()> {
    // Pin GIT_DIR/GIT_WORK_TREE so git never discovers an enclosing repository
    // (the lab workspace itself is a git repo; a walk-up would be destructive).
    let dot_git = dir.join(".git");
    let git_in = |args: &[&str]| -> anyhow::Result<()> {
        let out = std::process::Command::new("git")
            .args(args)
            .env("GIT_DIR", &dot_git)
            .env("GIT_WORK_TREE", dir)
            .output()
            .map_err(|e| anyhow!("git {args:?}: {e}"))?;
        if !out.status.success() {
            bail!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    };
    if dir.join(".git").exists() {
        // Reuse the existing checkout: the NFS-backed workspace keeps .nfs
        // silly-renames for recently closed pack files, so remove_dir_all can
        // fail with EBUSY. Re-sync in place instead of re-cloning.
        git_in(&["fetch", bare,
                 "+refs/heads/*:refs/remotes/origin/*",
                 "+refs/runs/*:refs/remotes/origin/runs/*",
                 "--prune"])?;
        git_in(&["checkout", "--detach", "--force", sha])?;
        git_in(&["reset", "--hard", sha])?;
        git_in(&["clean", "-fdx"])?;
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    let run = |args: &[&str]| -> anyhow::Result<()> {
        let st = std::process::Command::new("git")
            .args(args)
            .env("GIT_DIR", bare)
            .output()
            .map_err(|e| anyhow!("git {args:?}: {e}"))?;
        if !st.status.success() {
            bail!("git {args:?}: {}", String::from_utf8_lossy(&st.stderr));
        }
        Ok(())
    };
    // Clone to dir then hard reset to sha.
    let st = std::process::Command::new("git")
        .args(["clone", "--no-local", bare, dir.to_str().unwrap()])
        .env("GIT_DIR", "")
        .output()
        .context("git clone")?;
    if !st.status.success() {
        bail!("clone failed: {}", String::from_utf8_lossy(&st.stderr));
    }
    // Clone only carries refs/heads; run refs (refs/runs/*) must be fetched
    // explicitly so candidate checkouts can resolve their SHAs.
    let st = std::process::Command::new("git")
        .args(["fetch", bare, "+refs/runs/*:refs/remotes/origin/runs/*"])
        .current_dir(dir)
        .env("GIT_DIR", &dot_git)
        .output()
        .context("git fetch runs")?;
    if !st.status.success() {
        bail!("fetch runs failed: {}", String::from_utf8_lossy(&st.stderr));
    }
    let st = std::process::Command::new("git")
        .args(["checkout", "--detach", sha])
        .env("GIT_DIR", &dot_git)
        .env("GIT_WORK_TREE", dir)
        .output()
        .context("checkout sha")?;
    if !st.status.success() {
        bail!("checkout {sha} failed: {}", String::from_utf8_lossy(&st.stderr));
    }
    let _ = run;
    Ok(())
}

fn commit_all(dir: &Path) -> anyhow::Result<String> {
    let _ = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .output();
    let st = std::process::Command::new("git")
        .args(["commit", "-m", "runtime: task execution"])
        .env("GIT_AUTHOR_NAME", "Runtime Worker")
        .env("GIT_AUTHOR_EMAIL", "worker@runtime.local")
        .env("GIT_COMMITTER_NAME", "Runtime Worker")
        .env("GIT_COMMITTER_EMAIL", "worker@runtime.local")
        .current_dir(dir)
        .output()
        .context("git commit")?;
    if !st.status.success() {
        // Nothing to commit is fine only if HEAD already moved from baseline;
        // otherwise the model made no change and there is no candidate.
        let st2 = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()?;
        let sha = String::from_utf8_lossy(&st2.stdout).trim().to_string();
        return Ok(sha);
    }
    let st = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()?;
    Ok(String::from_utf8_lossy(&st.stdout).trim().to_string())
}

fn push_run_ref(bare: &str, checkout: &Path, run_ref: &str, sha: &str) -> anyhow::Result<()> {
    // Read the current remote ref (if any).
    let ls = std::process::Command::new("git")
        .args(["ls-remote", bare, run_ref])
        .current_dir(checkout)
        .output()
        .context("ls-remote")?;
    let existing = String::from_utf8_lossy(&ls.stdout).trim().to_string();
    if !existing.is_empty() {
        let existing_sha = existing.split_whitespace().next().unwrap_or("");
        if existing_sha != sha {
            bail!("run ref {run_ref} already published with different SHA {existing_sha}");
        }
        return Ok(()); // same SHA: idempotent re-push
    }
    let st = std::process::Command::new("git")
        .args(["push", bare, &format!("{sha}:{run_ref}")])
        .current_dir(checkout)
        .env("GIT_RECEIVE_HIDE_RESTRICTED", "1")
        .output()
        .context("push run ref")?;
    if !st.status.success() {
        // Re-check: concurrent publisher may have won with the same SHA.
        let ls2 = std::process::Command::new("git")
            .args(["ls-remote", bare, run_ref])
            .current_dir(checkout)
            .output()?;
        let existing2 = String::from_utf8_lossy(&ls2.stdout).trim().to_string();
        if existing2.split_whitespace().next() == Some(sha) {
            return Ok(());
        }
        bail!("push {run_ref}: {}", String::from_utf8_lossy(&st.stderr));
    }
    Ok(())
}
