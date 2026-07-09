//! HTTP client for the raft server's agent-facing endpoints.
//!
//! Two operations are needed to make a RustyCLI-backed agent actually chat
//! in raft:
//!
//! 1. **Mint a runner credential.** The daemon POSTs to
//!    `/internal/computer/runners/<agent_id>/credentials` with its own
//!    `sk_…` machine key and receives back an `sk_agent_…` key with raft
//!    scopes (send, read, mentions, …). Mirrors `requestRunnerCredentialOnce`
//!    at `chunk-URPIDKXK.js:21852`.
//!
//! 2. **Post a chat reply as that agent.** The daemon POSTs the agent's
//!    response text to `/internal/agent-api/send` using the `sk_agent_…`
//!    key. Mirrors the CLI's `raft message send` command
//!    (`cli/index.js:43442`, base path `AGENT_API_BASE_PATH =
//!    "/internal/agent-api"` at `cli/index.js:42768`).
//!
//! The LLM provider key that RustyCLI actually needs to think is **not**
//! minted here — it comes embedded in the `agent:start` config block from
//! the server (under `config.provider.apiKey`), populated from the
//! computer settings the operator entered on raft.build.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Scopes requested when minting a runner credential. Matches the npm
/// daemon's `RUNNER_CREDENTIAL_SCOPES` constant at `chunk-URPIDKXK.js:21307`.
pub const RUNNER_CREDENTIAL_SCOPES: &[&str] = &[
    "send",
    "read",
    "mentions",
    "tasks",
    "reactions",
    "server",
    "channels",
    "knowledge",
];

/// A minted raft runner credential.
#[derive(Debug, Clone)]
pub struct RunnerCredential {
    /// The `sk_agent_…` API key the agent uses to authenticate raft API
    /// calls (posting messages, reading channels, etc.).
    pub api_key: String,
    /// Server-assigned credential ID; can be used to revoke later.
    pub credential_id: Option<String>,
}

/// Body shape for the credential-mint request.
#[derive(Debug, Serialize)]
struct MintBody<'a> {
    scopes: &'a [&'a str],
    name: &'a str,
}

/// Response shape from the credential-mint endpoint.
#[derive(Debug, Deserialize)]
struct MintResponse {
    #[serde(rename = "apiKey")]
    api_key: String,
    #[serde(rename = "credentialId", default)]
    credential_id: Option<String>,
}

/// Body for the agent `send` endpoint.
#[derive(Debug, Serialize)]
pub struct SendBody {
    /// Target descriptor: `#channel`, `dm:@peer`, `#channel:threadId`, …
    /// Optional in the schema but always set by us.
    pub target: String,
    /// The agent's reply text.
    pub content: String,
    /// Highest inbound seq we've observed for this target before sending.
    /// Lets the server coalesce replies and detect stale drafts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen_up_to_seq: Option<i64>,
}

/// Response from the agent `send` endpoint.
#[derive(Debug, Deserialize)]
pub struct SendResponse {
    /// ID of the message the server accepted, if any.
    #[serde(rename = "messageId", default)]
    pub message_id: Option<String>,
    /// `"sent"` or `"held"`. Held means the server is asking for a
    /// freshness re-check; the daemon doesn't currently handle that case
    /// (it's primarily for human-driven CLI flows).
    #[serde(default, rename = "state")]
    pub state: Option<String>,
}

/// Shared `reqwest::Client` factory.
///
/// Disables system proxy auto-detection (`HTTP_PROXY` / `HTTPS_PROXY` env
/// vars). Reason: those env vars commonly point at corporate intercepting
/// proxies that can't reach `127.0.0.1` (used by the raft API when the
/// server runs on the same host) and may also mangle `Authorization`
/// headers. Operators who need proxy egress for `api.raft.build` can opt
/// back in once we add explicit per-host proxy configuration; for now,
/// direct connect is the safer default.
fn raft_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(format!("raft-daemon/{}", env!("CARGO_PKG_VERSION")))
        .no_proxy()
        .build()
        .context("building reqwest client")
}

/// Mint a runner credential for the given agent.
///
/// Uses the daemon's machine API key (the `sk_…` one) to authenticate.
/// Returns the per-agent `sk_agent_…` key.
///
/// # Errors
///
/// Returns an error if the server is unreachable, returns non-2xx, or the
/// response payload doesn't look like a valid agent credential.
pub async fn mint_runner_credential(
    server_url: &str,
    daemon_api_key: &str,
    agent_id: &str,
    runtime: &str,
) -> Result<RunnerCredential> {
    let client = raft_http_client()?;

    let short_id: String = agent_id.chars().take(8).collect();
    let name = format!("runner:{runtime}:{short_id}");
    let body = MintBody {
        scopes: RUNNER_CREDENTIAL_SCOPES,
        name: &name,
    };

    let url = format!(
        "{}/internal/computer/runners/{}/credentials",
        server_url.trim_end_matches('/'),
        urlencoding(agent_id),
    );

    let resp = client
        .post(&url)
        .bearer_auth(daemon_api_key)
        .header("Content-Type", "application/json")
        .header("X-Slock-Client", "daemon-runner-credential-minter")
        .json(&body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("runner credential mint failed: HTTP {status} — {text}");
    }

    let parsed: MintResponse = resp
        .json()
        .await
        .context("decoding runner credential response")?;

    if !parsed.api_key.starts_with("sk_agent_") {
        anyhow::bail!(
            "runner credential response did not contain a valid sk_agent_ key (got {:?})",
            parsed.api_key
        );
    }

    Ok(RunnerCredential {
        api_key: parsed.api_key,
        credential_id: parsed.credential_id,
    })
}

/// Post a chat reply as the given agent.
///
/// Uses the agent's `sk_agent_…` key (from [`mint_runner_credential`]) to
/// authenticate against `/internal/agent-api/send`.
///
/// # Errors
///
/// Returns an error if the server is unreachable, returns non-2xx, or the
/// response can't be decoded.
pub async fn send_agent_message(
    server_url: &str,
    agent_api_key: &str,
    body: &SendBody,
) -> Result<SendResponse> {
    let client = raft_http_client()?;

    let url = format!(
        "{}/internal/agent-api/send",
        server_url.trim_end_matches('/'),
    );

    let resp = client
        .post(&url)
        .bearer_auth(agent_api_key)
        .header("Content-Type", "application/json")
        .header("X-Slock-Client", "raft-daemon-rust")
        .json(body)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("agent message send failed: HTTP {status} — {text}");
    }

    resp.json().await.context("decoding send response")
}

/// Download an attachment's bytes from the raft server.
///
/// Uses the provided API key (typically the agent's `sk_agent_…` runner
/// credential) to authenticate against `/internal/agent-api/attachments/<attachmentId>`.
///
/// # Errors
///
/// Returns an error if the server is unreachable, returns non-2xx, or the
/// response bytes cannot be read.
pub async fn download_attachment(
    server_url: &str,
    api_key: &str,
    attachment_id: &str,
) -> Result<Vec<u8>> {
    let client = raft_http_client()?;

    let url = format!(
        "{}/internal/agent-api/attachments/{}",
        server_url.trim_end_matches('/'),
        urlencoding(attachment_id),
    );

    let resp = client
        .get(&url)
        .bearer_auth(api_key)
        .header("X-Slock-Client", "raft-daemon-rust")
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("attachment download failed: HTTP {status} — {text}");
    }

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .context("reading attachment bytes")
}

/// Best-effort: derive the raft send-target descriptor from an inbound
/// `agent:deliver` message.
///
/// The server-side `formatChannelTarget` produces strings like `#general`
/// or `dm:@alice`. Our delivery payload includes channel metadata we can
/// use to reconstruct one. Returns `None` if we can't confidently pick a
/// target — the caller should then skip the reply and log.
///
/// Recognised fields (any subset may be present):
///
/// - `message.channel_target` — if the server pre-formatted it for us, use
///   it verbatim.
/// - `message.channel_kind` or `message.channel_type` ∈ {`"channel"`, `"dm"`}
///   plus `message.channel_name` / `message.channel_id` / `message.sender_name`.
pub fn derive_target(delivery: &serde_json::Value) -> Option<String> {
    let msg = delivery.get("message")?;

    // Server pre-formatted target — best case.
    if let Some(t) = msg.get("channel_target").and_then(|v| v.as_str()) {
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }

    let kind = msg
        .get("channel_kind")
        .and_then(|v| v.as_str())
        .or_else(|| msg.get("channel_type").and_then(|v| v.as_str()));
    match kind {
        Some("channel") => msg
            .get("channel_name")
            .and_then(|v| v.as_str())
            .map(|n| format!("#{n}")),
        Some("dm") => msg
            .get("sender_name")
            .and_then(|v| v.as_str())
            .map(|p| format!("dm:@{p}")),
        _ => None,
    }
    // If we have a channel_id we *could* try using it as the target, but the
    // send schema expects a human-readable form. Skipping is safer than
    // guessing wrong.
}

/// Minimal percent-encoding for path segments. The raft server's runner
/// endpoint encodes agent IDs (which are server-assigned `ag_…` slugs,
/// already URL-safe) but we encode defensively in case an agent ID ever
/// contains an `&` or `?`.
fn urlencoding(s: &str) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_target_uses_preformatted_channel_target() {
        let d = serde_json::json!({
            "message": { "channel_target": "#general" }
        });
        assert_eq!(derive_target(&d).as_deref(), Some("#general"));
    }

    #[test]
    fn derive_target_builds_channel_from_name() {
        let d = serde_json::json!({
            "message": { "channel_kind": "channel", "channel_name": "all" }
        });
        assert_eq!(derive_target(&d).as_deref(), Some("#all"));
    }

    #[test]
    fn derive_target_builds_dm_from_sender() {
        let d = serde_json::json!({
            "message": { "channel_kind": "dm", "sender_name": "alice" }
        });
        assert_eq!(derive_target(&d).as_deref(), Some("dm:@alice"));
    }

    #[test]
    fn derive_target_uses_channel_type_dm() {
        let d = serde_json::json!({
            "message": { "channel_type": "dm", "sender_name": "alice" }
        });
        assert_eq!(derive_target(&d).as_deref(), Some("dm:@alice"));
    }

    #[test]
    fn derive_target_uses_channel_type_channel() {
        let d = serde_json::json!({
            "message": { "channel_type": "channel", "channel_name": "general" }
        });
        assert_eq!(derive_target(&d).as_deref(), Some("#general"));
    }

    #[test]
    fn derive_target_channel_kind_takes_precedence_over_channel_type() {
        let d = serde_json::json!({
            "message": {
                "channel_kind": "dm",
                "channel_type": "channel",
                "sender_name": "alice",
                "channel_name": "general"
            }
        });
        assert_eq!(derive_target(&d).as_deref(), Some("dm:@alice"));
    }

    #[test]
    fn derive_target_returns_none_without_enough_info() {
        let d = serde_json::json!({ "message": {} });
        assert_eq!(derive_target(&d).as_deref(), None);
    }

    #[test]
    fn derive_target_returns_none_without_message_block() {
        let d = serde_json::json!({ "agentId": "ag_1" });
        assert!(derive_target(&d).is_none());
    }

    #[test]
    fn urlencoding_passes_safe_chars_through() {
        assert_eq!(urlencoding("ag_1234"), "ag_1234");
        assert_eq!(urlencoding("ag-foo.bar"), "ag-foo.bar");
    }

    #[test]
    fn urlencoding_escapes_unsafe_chars() {
        assert_eq!(urlencoding("ag foo"), "ag%20foo");
        assert_eq!(urlencoding("ag/foo"), "ag%2Ffoo");
        assert_eq!(urlencoding("ag?x"), "ag%3Fx");
    }
}
