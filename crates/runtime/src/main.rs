//! `runtime` binary: serve (central) and runner subcommands; CLI verbs
//! submit/status/events/evidence/resolve/retry talk to the HTTP API.

use clap::{Parser, Subcommand};
use serde_json::json;

#[derive(Parser)]
#[command(name = "runtime", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start the central service.
    Serve {
        #[arg(long)]
        config: String,
    },
    /// Start a node runner (control or worker).
    Runner {
        #[arg(long)]
        config: String,
    },
    /// Submit a goal (operator).
    Submit {
        #[arg(long)]
        repo: String,
        #[arg(long)]
        baseline: String,
        /// Required capability, repeatable.
        #[arg(long = "require")]
        require: Vec<String>,
        /// Required route, repeatable.
        #[arg(long = "require-route")]
        require_route: Vec<String>,
        #[arg(trailing_var_arg = true)]
        goal: Vec<String>,
    },
    /// Query request status (operator).
    Status {
        #[arg(long)]
        request: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Read ordered events for a request.
    Events {
        #[arg(long)]
        request: String,
        #[arg(long, default_value_t = 0)]
        after: i64,
    },
    /// Export evidence for a run.
    Evidence {
        #[arg(long)]
        run: String,
        #[arg(long, default_value = "./evidence")]
        out: String,
    },
    /// Resolve an uncertain run (operator).
    Resolve {
        #[arg(long)]
        run: String,
        /// close | continue
        #[arg(long)]
        decision: String,
        #[arg(long)]
        reason: String,
    },
    /// Retry a task after its old run was closed or finished.
    Retry {
        #[arg(long)]
        task: String,
    },
}

fn runtime_url() -> String {
    std::env::var("RUNTIME_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into())
}

fn operator_token() -> anyhow::Result<String> {
    std::env::var("RUNTIME_OPERATOR_TOKEN")
        .map_err(|_| anyhow::anyhow!("RUNTIME_OPERATOR_TOKEN not set"))
}

async fn api_post(path: &str, body: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}{}", runtime_url(), path))
        .bearer_auth(operator_token()?)
        .json(body)
        .send()
        .await?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await.unwrap_or(json!({}));
    anyhow::ensure!(status.is_success(), "HTTP {status}: {v}");
    Ok(v)
}

async fn api_get(path: &str) -> anyhow::Result<serde_json::Value> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}{}", runtime_url(), path))
        .bearer_auth(operator_token()?)
        .send()
        .await?;
    let status = resp.status();
    let v: serde_json::Value = resp.json().await.unwrap_or(json!({}));
    anyhow::ensure!(status.is_success(), "HTTP {status}: {v}");
    Ok(v)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Serve { config } => {
            let cfg = store::CentralConfig::load(&config)
                .map_err(|e| anyhow::anyhow!("config invalid: {e}"))?;
            // Ensure database directory and evidence root exist.
            if let Some(dir) = std::path::Path::new(&cfg.database).parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::create_dir_all(&cfg.evidence_root)?;
            let store = store::Store::open(&cfg.database)
                .map_err(|e| anyhow::anyhow!("db open failed: {e}"))?;
            // Register configured repos for plan validation.
            for (name, repo) in &cfg.repositories {
                store.register_repo(name, &repo.bare_path, &repo.acceptance_profiles)?;
            }
            let tokens = service::AppState::token_map(&cfg);
            let state = std::sync::Arc::new(service::AppState {
                store,
                config: cfg.clone(),
                tokens,
            });
            let app = service::build_router(state);
            let listener = tokio::net::TcpListener::bind(&cfg.listen).await?;
            eprintln!("runtime serve listening on {}", cfg.listen);
            axum::serve(listener, app).await?;
        }
        Cmd::Runner { config } => {
            // Runner implementation lands with TASK-005 (M1b/M2); here we
            // validate the runner config shape early.
            let raw = std::fs::read_to_string(&config)?;
            let v: toml::Value = toml::from_str(&raw)?;
            let node_id = v
                .get("node_id")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow::anyhow!("runner config: node_id required"))?
                .to_string();
            let runtime_url = v
                .get("runtime_url")
                .and_then(|x| x.as_str())
                .ok_or_else(|| anyhow::anyhow!("runner config: runtime_url required"))?
                .to_string();
            eprintln!(
                "runner config ok: node={node_id} runtime={runtime_url}; runner loop not started (TASK-005)"
            );
        }
        Cmd::Submit {
            repo,
            baseline,
            require,
            require_route,
            goal,
        } => {
            let intent = goal.join(" ");
            anyhow::ensure!(!intent.trim().is_empty(), "goal text required");
            let body = json!({
                "intent": intent,
                "repo_ref": repo,
                "baseline_sha": baseline,
                "required_capabilities": require,
                "required_routes": require_route,
            });
            let resp = api_post("/v1/requests", &body).await?;
            println!("{}", resp["request_id"].as_str().unwrap_or("?"));
        }
        Cmd::Status { request, json } => {
            let v = api_get(&format!("/v1/requests/{request}")).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                println!("{}", v["request"]["state"].as_str().unwrap_or("?"));
            }
        }
        Cmd::Events { request, after } => {
            let v = api_get(&format!("/v1/requests/{request}/events?after={after}")).await?;
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Cmd::Evidence { run, out } => {
            // Downloads every registered evidence file for the run.
            let v = api_get(&format!("/v1/runs/{run}")).await.unwrap_or(json!({}));
            let client = reqwest::Client::new();
            std::fs::create_dir_all(&out)?;
            if let Some(files) = v["evidence"].as_array() {
                for f in files {
                    let name = f["rel_path"].as_str().unwrap_or_default();
                    let bytes = client
                        .get(format!("{}/v1/runs/{run}/evidence/{name}", runtime_url()))
                        .bearer_auth(operator_token()?)
                        .send()
                        .await?
                        .bytes()
                        .await?;
                    let path = std::path::Path::new(&out).join(name);
                    if let Some(p) = path.parent() {
                        std::fs::create_dir_all(p)?;
                    }
                    std::fs::write(path, &bytes)?;
                }
            }
            println!("evidence exported to {out}");
        }
        Cmd::Resolve { run, decision, reason } => {
            let close = matches!(decision.as_str(), "close" | "关闭");
            let body = json!({ "close": close, "reason": reason });
            api_post(&format!("/v1/runs/{run}/resolve"), &body).await?;
            println!("resolved");
        }
        Cmd::Retry { task } => {
            api_post(&format!("/v1/tasks/{task}/retry"), &json!({})).await?;
            println!("retried");
        }
    }
    Ok(())
}
