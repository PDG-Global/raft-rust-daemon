//! Local agent-api HTTP proxy.
//!
//! Bundled `raft`/`slock` CLI tools call this localhost proxy using a short-
//! lived token. The proxy looks up the token in the running agent registry,
//! swaps it for the agent's real `sk_agent_…` key, and forwards the request to
//! the raft server under `/internal/agent-api/*`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use http::header::AUTHORIZATION;
use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1::Builder;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::daemon::agent::{AgentProcessRegistry, raft_http_client};

type ProxyBody = Full<Bytes>;

/// Local proxy that forwards `/internal/agent-api/*` requests to the raft
/// server after swapping a bearer token for the agent's real credential key.
pub struct AgentApiProxy {
    /// Base URL of the local proxy, e.g. `http://127.0.0.1:12345`.
    pub url: String,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    handle: tokio::task::JoinHandle<()>,
}

impl AgentApiProxy {
    /// Bind to `localhost:0` and start the proxy server.
    ///
    /// # Errors
    ///
    /// Returns an error if the listener cannot be bound or the HTTP client
    /// cannot be built.
    pub async fn start(server_url: String, registry: Arc<AgentProcessRegistry>) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("binding agent-api proxy listener")?;
        let local_addr = listener
            .local_addr()
            .context("getting agent-api proxy local address")?;
        let url = format!("http://{local_addr}");
        let client = raft_http_client().context("building agent-api proxy client")?;
        let state = Arc::new(ProxyState {
            server_url,
            registry,
            client,
        });
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(run_accept_loop(listener, state, shutdown_rx));
        info!(url = %url, "agent-api proxy listening");
        Ok(Self {
            url,
            shutdown_tx: Some(shutdown_tx),
            handle,
        })
    }

    /// Gracefully shut down the proxy and wait for the accept loop to finish.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let _ = self.handle.await;
    }
}

struct ProxyState {
    server_url: String,
    registry: Arc<AgentProcessRegistry>,
    client: reqwest::Client,
}

async fn run_accept_loop(
    listener: TcpListener,
    state: Arc<ProxyState>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                break;
            }
            result = listener.accept() => {
                let Ok((stream, _)) = result else {
                    continue;
                };
                let state = Arc::clone(&state);
                tokio::spawn(handle_connection(stream, state));
            }
        }
    }
}

async fn handle_connection(stream: tokio::net::TcpStream, state: Arc<ProxyState>) {
    let io = TokioIo::new(stream);
    let service = service_fn(move |req: Request<Incoming>| {
        let state = Arc::clone(&state);
        async move { Ok::<_, std::convert::Infallible>(proxy_service(req, state).await) }
    });
    let builder = Builder::new();
    if let Err(err) = builder.serve_connection(io, service).await {
        warn!(error = %err, "agent-api proxy connection error");
    }
}

async fn proxy_service(req: Request<Incoming>, state: Arc<ProxyState>) -> Response<ProxyBody> {
    let path = req.uri().path();
    if !path.starts_with("/internal/agent-api/") {
        warn!(path = %path, "proxy request outside /internal/agent-api/");
        return error_response(StatusCode::NOT_FOUND, "not found");
    }
    let inner_path = &path["/internal/agent-api/".len()..];

    let Some(token) = extract_bearer(&req) else {
        warn!("agent-api proxy request missing bearer token");
        return error_response(StatusCode::UNAUTHORIZED, "unauthorized");
    };

    let Some(process) = state.registry.find_by_proxy_token(&token) else {
        warn!("agent-api proxy request with unknown token");
        return error_response(StatusCode::UNAUTHORIZED, "unauthorized");
    };
    let Some(agent_key) = process.agent_credential_key.as_ref() else {
        warn!(
            agent_id = %process.agent_id,
            "agent-api proxy request for agent without credential key"
        );
        return error_response(StatusCode::UNAUTHORIZED, "unauthorized");
    };

    let query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();
    let upstream_url = format!(
        "{}/internal/agent-api/{}{}",
        state.server_url.trim_end_matches('/'),
        inner_path,
        query
    );

    let mut upstream = state
        .client
        .request(req.method().clone(), upstream_url)
        .timeout(Duration::from_secs(30));

    for (name, value) in req.headers() {
        let name_lc = name.as_str().to_ascii_lowercase();
        if matches!(
            name_lc.as_str(),
            "host"
                | "connection"
                | "authorization"
                | "content-length"
                | "keep-alive"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "te"
                | "trailer"
                | "transfer-encoding"
        ) {
            continue;
        }
        upstream = upstream.header(name, value);
    }
    upstream = upstream
        .header(AUTHORIZATION, format!("Bearer {agent_key}"))
        .header("X-Agent-Id", &process.agent_id)
        .header("X-Slock-Client", "cli");

    let body = match req.into_body().collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => {
            warn!(error = %err, "failed to read agent-api proxy request body");
            return error_response(StatusCode::BAD_GATEWAY, "bad gateway");
        }
    };
    upstream = upstream.body(body);

    let resp = match upstream.send().await {
        Ok(r) => r,
        Err(err) => {
            warn!(error = %err, "agent-api proxy upstream request failed");
            return error_response(StatusCode::BAD_GATEWAY, "bad gateway");
        }
    };

    let status = resp.status();
    let mut builder = Response::builder().status(status);
    for (key, value) in resp.headers() {
        builder = builder.header(key, value);
    }
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(err) => {
            warn!(error = %err, "failed to read agent-api proxy response body");
            return error_response(StatusCode::BAD_GATEWAY, "bad gateway");
        }
    };
    builder
        .body(Full::new(bytes))
        .unwrap_or_else(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal error"))
}

fn extract_bearer(req: &Request<Incoming>) -> Option<String> {
    req.headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(str::to_string)
}

fn error_response(status: StatusCode, message: &str) -> Response<ProxyBody> {
    Response::builder()
        .status(status)
        .body(Full::new(Bytes::copy_from_slice(message.as_bytes())))
        .unwrap_or_else(|_| {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::new()))
                .expect("valid response")
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::daemon::agent::AgentProcess;

    fn sample_process(agent_id: &str, token: &str, key: &str) -> AgentProcess {
        AgentProcess {
            agent_id: agent_id.into(),
            name: "Test".into(),
            description: "desc".into(),
            runtime: "builtin".into(),
            model: "gpt-4o".into(),
            workspace: PathBuf::from("/tmp"),
            home: PathBuf::from("/tmp"),
            session_id: None,
            launch_id: None,
            llm_api_key: None,
            llm_base_url: None,
            provider_id: None,
            agent_credential_key: Some(key.into()),
            agent_credential_id: None,
            agent_proxy_token: Some(token.into()),
            agent_proxy_token_file: None,
            proxy_url: None,
            server_url: None,
            activity_client_seq: Arc::new(AtomicU64::new(0)),
            turn_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    #[tokio::test]
    async fn proxy_rejects_missing_bearer() {
        let registry = Arc::new(AgentProcessRegistry::new());
        let proxy = AgentApiProxy::start("https://example.com".into(), registry)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let response = send_raw(
            proxy.url.trim_start_matches("http://"),
            "GET /internal/agent-api/send HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;
        proxy.shutdown().await;
        assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
    }

    #[tokio::test]
    async fn proxy_rejects_unknown_token() {
        let registry = Arc::new(AgentProcessRegistry::new());
        let proxy = AgentApiProxy::start("https://example.com".into(), registry)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let response = send_raw(
            proxy.url.trim_start_matches("http://"),
            "GET /internal/agent-api/send HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer sap_unknown\r\nConnection: close\r\n\r\n",
        ).await;
        proxy.shutdown().await;
        assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
    }

    #[tokio::test]
    async fn proxy_looks_up_token() {
        let registry = Arc::new(AgentProcessRegistry::new());
        registry.install(sample_process("ag_1", "sap_token_1", "sk_agent_xxx"));
        let proxy = AgentApiProxy::start("http://127.0.0.1:1".into(), registry)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let response = send_raw(
            proxy.url.trim_start_matches("http://"),
            "GET /internal/agent-api/send HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer sap_token_1\r\nConnection: close\r\n\r\n",
        ).await;
        proxy.shutdown().await;
        // Upstream is not reachable, so we expect a 502.
        assert!(response.starts_with("HTTP/1.1 502"), "got: {response}");
    }

    async fn send_raw(addr: &str, req: &str) -> String {
        let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (mut read, mut write) = stream.into_split();
        write.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        let _ = read.read_to_end(&mut buf).await;
        String::from_utf8_lossy(&buf).to_string()
    }
}
