//! opencode_v2 — OpenCode V2 native protocol adapter.
//! Basic auth, session create/reuse, stable-ID prompt delivery,
//! round wait with poll fallback, history read, assistant text
//! extraction strictly from content[].text (reasoning must not leak in).

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct OcError(pub String);

impl std::fmt::Display for OcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "opencode_v2: {}", self.0)
    }
}
impl std::error::Error for OcError {}

pub type OcResult<T> = Result<T, OcError>;

pub struct OcClient {
    base_url: String,
    password: String,
    http: reqwest::Client,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionRef {
    pub id: String,
    pub directory: String,
}

#[derive(Debug, Deserialize)]
struct SessionData {
    id: String,
    location: SessionLocation,
}

#[derive(Debug, Deserialize)]
struct SessionLocation {
    directory: String,
}

#[derive(Debug, Deserialize)]
struct MessageList {
    data: Vec<MessageRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageRow {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub content: serde_json::Value,
}

impl MessageRow {
    /// Assistant-visible text, extracted only from content[].text parts.
    /// Never aggregates other strings (reasoning content would inflate it).
    pub fn assistant_text(&self) -> String {
        if self.kind != "assistant" {
            return String::new();
        }
        let mut out = String::new();
        if let Some(parts) = self.content.as_array() {
            for p in parts {
                if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                        out.push_str(t);
                    }
                }
            }
        } else if let Some(t) = self.content.get("text").and_then(|t| t.as_str()) {
            // Some schema versions use a single content object with text.
            out.push_str(t);
        }
        out
    }

    pub fn contains_text(&self, needle: &str) -> bool {
        self.assistant_text().contains(needle)
    }
}

impl OcClient {
    pub fn new(base_url: &str, password: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            password: password.to_string(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .expect("http client"),
        }
    }

    async fn get(&self, path: &str) -> OcResult<reqwest::Response> {
        self.http
            .get(format!("{}{}", self.base_url, path))
            .basic_auth("opencode", Some(&self.password))
            .send()
            .await
            .map_err(|e| OcError(format!("GET {path}: {e}")))
    }

    async fn post(&self, path: &str, body: &serde_json::Value) -> OcResult<reqwest::Response> {
        self.http
            .post(format!("{}{}", self.base_url, path))
            .basic_auth("opencode", Some(&self.password))
            .json(body)
            .send()
            .await
            .map_err(|e| OcError(format!("POST {path}: {e}")))
    }

    /// Liveness + version check.
    pub async fn info(&self) -> OcResult<String> {
        let resp = self.get("/api/info").await?;
        let status = resp.status();
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| OcError(format!("info json: {e}")))?;
        if !status.is_success() {
            return Err(OcError(format!("info HTTP {status}")));
        }
        Ok(v["version"].as_str().unwrap_or("unknown").to_string())
    }

    /// Create a session bound to a working directory.
    pub async fn create_session(&self, title: &str, directory: &str) -> OcResult<SessionRef> {
        let body = json!({ "title": title, "location": { "directory": directory } });
        let resp = self.post("/api/session", &body).await?;
        let status = resp.status();
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| OcError(format!("session json: {e}")))?;
        if !status.is_success() {
            return Err(OcError(format!("session HTTP {status}: {v}")));
        }
        let data: SessionData = serde_json::from_value(v["data"].clone())
            .map_err(|e| OcError(format!("session shape: {e}")))?;
        if data.location.directory != directory {
            return Err(OcError(format!(
                "session directory mismatch: want {directory}, got {}",
                data.location.directory
            )));
        }
        Ok(SessionRef {
            id: data.id,
            directory: directory.to_string(),
        })
    }

    /// Retrieve an existing session by id (used for restart recovery).
    /// Cross-node access yields 404; here mapped to Ok(None) for the
    /// "not found on this node" case only.
    pub async fn get_session(&self, session_id: &str) -> OcResult<Option<SessionRef>> {
        let resp = self.get(&format!("/api/session/{session_id}")).await?;
        match resp.status() {
            s if s.is_success() => {
                let v: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| OcError(format!("session json: {e}")))?;
                let data: SessionData = serde_json::from_value(v["data"].clone())
                    .map_err(|e| OcError(format!("session shape: {e}")))?;
                Ok(Some(SessionRef {
                    id: data.id,
                    directory: data.location.directory,
                }))
            }
            s if s.as_u16() == 404 => Ok(None),
            s => Err(OcError(format!("get_session HTTP {s}"))),
        }
    }

    /// Deliver a prompt with a stable message ID. Same ID replays are
    /// deduplicated by the server (verified in M0); we admit then return.
    pub async fn prompt(&self, session_id: &str, message_id: &str, text: &str) -> OcResult<()> {
        let body = json!({
            "id": message_id,
            "text": text,
            "delivery": "queue",
            "resume": true
        });
        let resp = self
            .post(&format!("/api/session/{session_id}/prompt"), &body)
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let v: serde_json::Value = resp.json().await.unwrap_or(json!({}));
            return Err(OcError(format!("prompt HTTP {status}: {v}")));
        }
        Ok(())
    }

    /// Wait for the current round to finish. The experimental wait can
    /// return before the scheduler starts (seen in M0), so callers use
    /// wait_with_poll or we poll the history here.
    pub async fn wait(&self, session_id: &str) -> OcResult<()> {
        let resp = self
            .post(
                &format!("/api/experimental/session/{session_id}/wait"),
                &json!({}),
            )
            .await?;
        if !resp.status().is_success() {
            return Err(OcError(format!("wait HTTP {}", resp.status())));
        }
        Ok(())
    }

    pub async fn history(&self, session_id: &str, limit: i64) -> OcResult<Vec<MessageRow>> {
        let resp = self
            .get(&format!("/api/session/{session_id}/message?limit={limit}"))
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(OcError(format!("history HTTP {status}")));
        }
        let v: MessageList = resp
            .json()
            .await
            .map_err(|e| OcError(format!("history json: {e}")))?;
        Ok(v.data)
    }

    /// Wait until `expected_text` appears among assistant messages, polling
    /// as a safety net when the experimental wait returns early. Timeout is
    /// bounded; on timeout returns Err with diagnostics.
    pub async fn wait_for_reply(
        &self,
        session_id: &str,
        expected_text: &str,
        timeout: Duration,
    ) -> OcResult<Vec<MessageRow>> {
        // Best-effort initial wait (fire and forget semantics on failure).
        let _ = self.wait(session_id).await;
        let start = Instant::now();
        loop {
            let history = self.history(session_id, 100).await?;
            for m in &history {
                if m.kind == "assistant" && m.contains_text(expected_text) {
                    return Ok(history);
                }
            }
            if start.elapsed() > timeout {
                return Err(OcError(format!(
                    "wait_for_reply timeout after {:?}: no assistant text containing {expected_text:?}",
                    timeout
                )));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    pub async fn inbox(&self, session_id: &str) -> OcResult<serde_json::Value> {
        let resp = self
            .get(&format!("/api/session/{session_id}/inbox"))
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(OcError(format!("inbox HTTP {status}")));
        }
        resp.json()
            .await
            .map_err(|e| OcError(format!("inbox json: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// Minimal HTTP mock: returns configured responses per path prefix.
    struct MockServer {
        addr: String,
        handle: std::thread::JoinHandle<()>,
    }

    impl MockServer {
        fn start(
            require_auth: bool,
            routes: std::collections::HashMap<String, (u16, serde_json::Value)>,
        ) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap().to_string();
            let handle = std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let mut stream = stream.unwrap();
                    let mut buf = [0u8; 65536];
                    let mut req = Vec::new();
                    loop {
                        let n = stream.read(&mut buf).unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        req.extend_from_slice(&buf[..n]);
                        // crude end-of-headers check
                        if req.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    let req_str = String::from_utf8_lossy(&req);
                    let first_line = req_str.lines().next().unwrap_or("");
                    let path = first_line
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("/")
                        .to_string();
                    let path = path.split('?').next().unwrap_or("/").to_string();

                    // Auth check (header name case-insensitive)
                    if require_auth {
                        let lower = req_str.to_ascii_lowercase();
                        // Client is accepted only when an Authorization: Basic
                        // header exists and its value is not the WRONG password
                        // credential (opencode:WRONG base64: b3BlbmNvZGU6V1JPTkc=).
                        let wrong_cred = req_str.lines().any(|l| {
                            l.to_ascii_lowercase().starts_with("authorization: basic")
                                && l.contains("b3BlbmNvZGU6V1JPTkc=")
                        });
                        let authed = lower.contains("authorization: basic") && !wrong_cred;
                        if !authed {
                            stream
                                .write_all(
                                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n",
                                )
                                .unwrap();
                            continue;
                        }
                    }

                    // Handle POST /api/experimental/session/X/wait — hang then 200
                    if path.ends_with("/wait") {
                        std::thread::sleep(Duration::from_millis(50));
                        stream
                            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                            .unwrap();
                        continue;
                    }

                    let route_key = format!("{} {}", method_of(&req_str), path);
                    let (code, body) = routes
                        .get(&route_key)
                        .cloned()
                        .unwrap_or((404, json!({"error": "no route"})));
                    let body_str = body.to_string();
                    let resp = format!(
                        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                        code,
                        if code < 400 { "OK" } else { "ERR" },
                        body_str.len(),
                        body_str
                    );
                    stream.write_all(resp.as_bytes()).unwrap();
                }
            });
            MockServer { addr, handle }
        }
    }

    fn method_of(req: &str) -> String {
        req.lines()
            .next()
            .unwrap_or("")
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string()
    }

    fn client(addr: &str) -> OcClient {
        OcClient::new(&format!("http://{addr}"), "secret")
    }

    #[tokio::test]
    async fn test_auth_failure_is_error() {
        let routes = std::collections::HashMap::new();
        let server = MockServer::start(true, routes);
        // Wrong password: our mock rejects anything that isn't opencode:*
        // other than the exact right pair; simulate mismatch via server
        // requiring auth but our client sending wrong password.
        let wrong = OcClient::new(&format!("http://{}", server.addr), "WRONG");
        let err = wrong.info().await;
        assert!(err.is_err(), "expected auth failure");
    }

    #[tokio::test]
    async fn test_session_create_and_shape() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "POST /api/session".to_string(),
            (
                200,
                json!({"data": {"id": "ses_1", "location": {"directory": "/tmp/x"}}}),
            ),
        );
        let server = MockServer::start(true, routes);
        let c = client(&server.addr);
        let s = c.create_session("t", "/tmp/x").await.unwrap();
        assert_eq!(s.id, "ses_1");
        assert_eq!(s.directory, "/tmp/x");
    }

    #[tokio::test]
    async fn test_session_directory_mismatch_rejected() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "POST /api/session".to_string(),
            (
                200,
                json!({"data": {"id": "ses_1", "location": {"directory": "/other"}}}),
            ),
        );
        let server = MockServer::start(true, routes);
        let c = client(&server.addr);
        let err = c.create_session("t", "/tmp/x").await;
        assert!(err.is_err(), "directory mismatch must fail");
    }

    #[tokio::test]
    async fn test_prompt_ok_and_error_path() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "POST /api/session/ses_1/prompt".to_string(),
            (200, json!({"data": {"id": "msg_1"}})),
        );
        let server = MockServer::start(true, routes);
        let c = client(&server.addr);
        c.prompt("ses_1", "msg_1", "hello").await.unwrap();
        let err = c.prompt("ses_missing", "msg_1", "hello").await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_history_assistant_text_extraction() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "GET /api/session/ses_1/message".to_string(),
            (
                200,
                json!({"data": [
                    {"id": "m1", "type": "assistant", "content": [
                        {"type": "reasoning", "text": "secret reasoning"},
                        {"type": "text", "text": "SECOND-abc"}
                    ]},
                    {"id": "m2", "type": "user", "content": [{"type": "text", "text": "user text"}]}
                ]}),
            ),
        );
        let server = MockServer::start(true, routes);
        let c = client(&server.addr);
        let hist = c.history("ses_1", 100).await.unwrap();
        assert_eq!(hist.len(), 2);
        // Assistant text contains ONLY the text part, not reasoning.
        let a = hist.iter().find(|m| m.id == "m1").unwrap();
        let text = a.assistant_text();
        assert_eq!(text, "SECOND-abc");
        assert!(!text.contains("secret"));
        // User messages produce no assistant text.
        let u = hist.iter().find(|m| m.id == "m2").unwrap();
        assert_eq!(u.assistant_text(), "");
        assert!(a.contains_text("SECOND-abc"));
    }

    #[tokio::test]
    async fn test_wait_for_reply_found() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "GET /api/session/ses_1/message".to_string(),
            (
                200,
                json!({"data": [
                    {"id": "m1", "type": "assistant", "content": [{"type": "text", "text": "DONE-42"}]}
                ]}),
            ),
        );
        let server = MockServer::start(true, routes);
        let c = client(&server.addr);
        let hist = c
            .wait_for_reply("ses_1", "DONE-42", Duration::from_secs(3))
            .await
            .unwrap();
        assert!(hist.iter().any(|m| m.contains_text("DONE-42")));
    }

    #[tokio::test]
    async fn test_wait_for_reply_timeout() {
        let mut routes = std::collections::HashMap::new();
        routes.insert(
            "GET /api/session/ses_1/message".to_string(),
            (200, json!({"data": []})),
        );
        let server = MockServer::start(true, routes);
        let c = client(&server.addr);
        let err = c
            .wait_for_reply("ses_1", "NEVER", Duration::from_secs(1))
            .await;
        assert!(err.is_err(), "expected timeout");
    }

    #[tokio::test]
    async fn test_get_session_404_is_none() {
        let routes = std::collections::HashMap::new();
        let server = MockServer::start(true, routes);
        let c = client(&server.addr);
        let r = c.get_session("ses_missing").await.unwrap();
        assert!(r.is_none(), "404 should map to None");
    }
}
