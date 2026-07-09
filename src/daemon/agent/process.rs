//! Per-agent runtime state and RustyCLI spawn logic.
//!
//! The npm daemon keeps a long-lived child process per agent and pipes
//! messages to its stdin. RustyCLI doesn't expose that interface — it's a
//! one-shot / interactive CLI (`rusty --prompt "…" --headless` runs once and
//! exits, `--resume <session>` provides conversation continuity). So this
//! module implements the simpler *spawn-per-delivery* model: one process
//! invocation per inbound message, keyed on a persistent `session_id`.
//!
//! State held per agent:
//!
//! - `session_id` — passed to `rusty --resume` so each delivery continues
//!   the same conversation. Empty until the first delivery, because we let
//!   RustyCLI allocate the session on first run and capture it from
//!   `--list-sessions` after the fact. (A follow-up can pass `--session-id`
//!   if RustyCLI ever grows that flag.)
//! - `model` — chosen by the server in the `agent:start` config.
//! - `workspace` — per-agent working directory under
//!   `<home>/agents/<agent_id>/`, created on start.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use dashmap::DashMap;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{info, warn};

use crate::daemon::paths;
use crate::daemon::runner::starts_with_no_reply_marker;

/// Maximum time a single RustyCLI turn is allowed to run before the daemon
/// kills it and returns an error. Prevents a hung tool/LLM call from blocking
/// the agent forever.
const RUSTY_TURN_TIMEOUT: Duration = Duration::from_secs(120);

/// Active capabilities advertised to RustyCLI via `SLOCK_AGENT_ACTIVE_CAPABILITIES`.
const DEFAULT_ACTIVE_CAPABILITIES: &str =
    "send,read,mentions,tasks,reactions,server,channels,knowledge";

/// Per-agent runtime state.
///
/// One of these lives in the [`AgentProcessRegistry`] for as long as the
/// server considers the agent started on this daemon.
#[derive(Debug, Clone)]
pub struct AgentProcess {
    /// Raft agent ID (`ag_…`).
    pub agent_id: String,
    /// Display name from the server's `agent:start` config.
    pub name: String,
    /// Role/description from the server's `agent:start` config.
    pub description: String,
    /// Runtime ID we advertised (always `builtin` for this port — backed by
    /// RustyCLI).
    pub runtime: String,
    /// Model ID from the server's `agent:start` config.
    pub model: String,
    /// Per-agent workspace directory.
    pub workspace: PathBuf,
    /// Daemon home directory (used for agent-wide paths like the CLI wrappers).
    pub home: PathBuf,
    /// RustyCLI session ID for `--resume`. `None` until the agent has been
    /// invoked at least once and we've observed a session.
    pub session_id: Option<String>,
    /// Server-assigned launch ID, echoed back in `agent:status` /
    /// `agent:session` frames.
    pub launch_id: Option<String>,
    /// LLM provider API key extracted from `config.provider.apiKey` in the
    /// `agent:start` message. Fed to RustyCLI via `--api-key` so it can
    /// actually talk to the LLM. None if the server didn't supply one.
    pub llm_api_key: Option<String>,
    /// LLM provider base URL from `config.provider.baseUrl` (gateways /
    /// custom providers). Fed to RustyCLI via `--api-base` when present.
    pub llm_base_url: Option<String>,
    /// LLM provider ID from `config.provider.providerId` (e.g. `"openai"`,
    /// `"kimi"`, `"deepseek"`). Mapped to a RustyCLI `--preset` so the
    /// right endpoint shape and auth scheme are used. Without this RustyCLI
    /// defaults to OpenAI and any non-OpenAI model name bounces with
    /// "model not found".
    pub provider_id: Option<String>,
    /// Raft `sk_agent_…` key minted at start, used to POST chat replies
    /// back to raft as this agent. None if the mint failed (in which case
    /// the agent can think out loud but can't reply in chat).
    pub agent_credential_key: Option<String>,
    /// Credential ID returned alongside `agent_credential_key`, for
    /// revocation later.
    pub agent_credential_id: Option<String>,
    /// Per-agent monotonic counter for `agent:activity` frames. Mirrors
    /// the npm daemon's `nextActivityClientSeq` so the server can correlate
    /// lifecycle events and detect stale activity.
    pub activity_client_seq: Arc<AtomicU64>,
    /// Short-lived proxy token used by the bundled `raft`/`slock` CLI tools
    /// to authenticate against this daemon's local agent-api proxy.
    pub agent_proxy_token: Option<String>,
    /// Path to the file holding `agent_proxy_token`, written with `0o600`
    /// permissions so the CLI can read it.
    pub agent_proxy_token_file: Option<PathBuf>,
    /// Local agent-api proxy URL passed to RustyCLI via `SLOCK_AGENT_PROXY_URL`.
    pub proxy_url: Option<String>,
    /// Raft server URL passed to RustyCLI via `SLOCK_SERVER_URL`.
    pub server_url: Option<String>,
    /// Lock that serializes RustyCLI turns for this agent. RustyCLI keeps a
    /// per-agent task registry (SQLite) in the workspace; running two
    /// invocations concurrently causes "database is locked" errors. The lock
    /// is shared across clones so deliveries for the same agent queue up and
    /// run one at a time.
    pub turn_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AgentProcess {
    /// Build the initial state from a server `agent:start` config block.
    ///
    /// Does **not** mint a runner credential — that's an async HTTP call
    /// the caller does before installing the process. The returned struct
    /// has `agent_credential_*` set to `None`; the runner fills them in
    /// after the mint succeeds.
    ///
    /// LLM credentials (`llm_api_key`, `llm_base_url`) are resolved here
    /// via [`resolve_llm_credentials`] — config first, then env, then
    /// per-provider defaults.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace directory cannot be created.
    pub fn from_start(
        agent_id: &str,
        config: &serde_json::Value,
        launch_id: Option<&serde_json::Value>,
        home: &Path,
    ) -> Result<Self> {
        let runtime = config
            .get("runtime")
            .and_then(|v| v.as_str())
            .unwrap_or("builtin")
            .to_string();
        let model = config
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let name = config
            .get("displayName")
            .or_else(|| config.get("name"))
            .or_else(|| config.get("agentName"))
            .or_else(|| config.get("title"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(agent_id)
            .to_string();
        let description = config
            .get("description")
            .or_else(|| config.get("role"))
            .or_else(|| config.get("bio"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("No role defined yet.")
            .to_string();
        let workspace = workspace_for(agent_id, home);

        info!(
            agent_id = agent_id,
            name = %name,
            description = %description,
            "agent identity resolved from start config"
        );

        // LLM provider block: `config.provider.{providerId, apiKey, baseUrl}`.
        // We parse defensively because the server may send the block under a
        // handful of shapes/casings (and may omit it entirely when it expects
        // the daemon to pull credentials from elsewhere).
        let provider_config = ProviderConfig::from_config(config);
        let provider_id = provider_config.provider_id.as_deref();
        let preset = pick_rustycli_preset(provider_id, Some(&model));
        let creds = resolve_llm_credentials(Some(&provider_config), provider_id, preset);

        info!(
            agent_id = agent_id,
            api_key_source = %creds.api_key_source.unwrap_or(CredentialSource::Missing),
            base_url = ?creds.base_url.as_deref(),
            base_url_source = %creds.base_url_source.unwrap_or(CredentialSource::Missing),
            preset = ?preset,
            env_vars_checked = ?env_var_candidates(provider_id, preset),
            "resolved LLM credentials",
        );

        // If we ended up with no key, surface an actionable hint so the operator
        // knows exactly which env vars we looked for — these are the same names
        // rusty itself checks, in priority order.
        if creds.api_key.is_none() {
            let candidates = env_var_candidates(provider_id, preset);
            let env_list = candidates.join(", ");
            warn!(
                agent_id = agent_id,
                env_vars = %env_list,
                "no LLM API key found for agent; spawn will likely fail at auth. \
                 Set one of these env vars (or run `rusty --setup` to populate \
                 ~/.rusty/settings.json).",
            );
        }

        // Best-effort: create the workspace now so the first spawn doesn't
        // fail on a missing cwd.
        std::fs::create_dir_all(&workspace)
            .with_context(|| format!("creating workspace {}", workspace.display()))?;

        // Set up the agent's memory index and notes directory. If the user has
        // an existing slock/raft agent home at ~/.slock/agents/<id>, migrate
        // MEMORY.md and notes so the agent keeps its identity across the Rust
        // port. Otherwise seed a minimal initial MEMORY.md from the agent name
        // and description sent by the server.
        setup_agent_memory(agent_id, &name, &description, &workspace)?;

        Ok(Self {
            agent_id: agent_id.to_string(),
            name,
            description,
            runtime,
            model,
            workspace,
            home: home.to_path_buf(),
            session_id: None,
            launch_id: launch_id.and_then(|v| v.as_str()).map(str::to_string),
            llm_api_key: creds.api_key,
            llm_base_url: creds.base_url,
            provider_id: provider_config.provider_id,
            agent_credential_key: None,
            agent_credential_id: None,
            agent_proxy_token: None,
            agent_proxy_token_file: None,
            proxy_url: None,
            server_url: None,
            activity_client_seq: Arc::new(AtomicU64::new(0)),
            turn_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    /// Return the next per-agent activity client sequence number.
    ///
    /// Used to correlate `agent:activity` frames with the server's lifecycle
    /// tracking. The counter is shared across clones of this process.
    pub fn next_activity_client_seq(&self) -> u64 {
        self.activity_client_seq.fetch_add(1, Ordering::SeqCst)
    }
}

/// Extracted provider configuration from the server `agent:start` config.
///
/// The exact field names and nesting vary between server versions and the
/// runtime being used, so this is populated defensively from several possible
/// locations and casing conventions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderConfig {
    /// Provider kind: "preset", "managed", "gateway", "custom", etc.
    pub kind: Option<String>,
    /// Provider identifier, e.g. "kimi-coding", "openai", "deepseek".
    pub provider_id: Option<String>,
    /// LLM API key.
    pub api_key: Option<String>,
    /// Custom base URL for gateway / custom providers.
    pub base_url: Option<String>,
}

impl ProviderConfig {
    /// Extract provider config from the `agent:start` config block.
    ///
    /// Tries, in order:
    /// 1. `config.provider` (camelCase and snake_case)
    /// 2. `config.runtimeConfig.provider` (camelCase and snake_case)
    /// 3. `config.model` prefix (`provider/model`) as a fallback provider_id
    pub fn from_config(config: &serde_json::Value) -> Self {
        // 1. config.provider
        if let Some(obj) = config.get("provider").and_then(|v| v.as_object()) {
            let extracted = Self::from_object(obj);
            if extracted.provider_id.is_some() || extracted.api_key.is_some() {
                return extracted;
            }
        }

        // 2. config.runtimeConfig.provider
        if let Some(obj) = config
            .pointer("/runtimeConfig/provider")
            .and_then(|v| v.as_object())
        {
            let extracted = Self::from_object(obj);
            if extracted.provider_id.is_some() || extracted.api_key.is_some() {
                return extracted;
            }
        }

        // 3. Infer provider_id from model prefix
        let mut out = Self::default();
        if let Some(model) = config.get("model").and_then(|v| v.as_str()) {
            if let Some((prefix, _)) = model.split_once('/') {
                if !prefix.is_empty() {
                    out.provider_id = Some(prefix.to_string());
                }
            }
        }
        out
    }

    fn from_object(obj: &serde_json::Map<String, serde_json::Value>) -> Self {
        Self {
            kind: read_str(obj, &["kind"]),
            provider_id: read_str(obj, &["providerId", "provider_id", "id", "name"]),
            api_key: read_str(obj, &["apiKey", "api_key", "key", "token"]),
            base_url: read_str(obj, &["baseUrl", "base_url", "baseURL", "url"]),
        }
    }
}

/// Read the first non-empty string value from a JSON object for a list of
/// candidate keys.
fn read_str(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = obj
            .get(*key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            return Some(s.to_string());
        }
    }
    None
}

/// Pick the matching RustyCLI `--preset` for a given raft provider ID and
/// model name.
///
/// RustyCLI ships built-in presets for `xiaomi, kimi, openai, ollama,
/// deepseek`. Raft `providerId` values are usually human-readable provider
/// slugs (`"openai"`, `"anthropic"`, `"moonshot"`, `"kimi"`, …). Without
/// this mapping RustyCLI defaults to OpenAI and any non-OpenAI model name
/// bounces with a localised "model not found" error.
///
/// Returns `None` for providers without a rusty preset (e.g. Anthropic);
/// in that case the caller can still fall back to `--api-base`.
pub fn pick_rustycli_preset(
    provider_id: Option<&str>,
    model: Option<&str>,
) -> Option<&'static str> {
    let pid = provider_id.unwrap_or("").to_ascii_lowercase();
    let mdl = model.unwrap_or("").to_ascii_lowercase();

    if pid.contains("kimi") || pid.contains("moonshot") || mdl.starts_with("kimi") {
        return Some("kimi");
    }
    if pid.contains("deepseek") || mdl.starts_with("deepseek") {
        return Some("deepseek");
    }
    if pid.contains("xiaomi") {
        return Some("xiaomi");
    }
    if pid.contains("ollama") {
        return Some("ollama");
    }
    if pid.contains("openai") || mdl.starts_with("gpt-") {
        return Some("openai");
    }
    None
}

/// Strip a raft-style provider prefix from a model name.
///
/// Raft model IDs are often `provider/model` (e.g.
/// `kimi-coding/kimi-for-coding`). RustyCLI doesn't always understand the
/// raft prefix and may reject the request with "model not found". This
/// keeps the part after the first `/` when one is present.
pub fn strip_provider_prefix(model: &str) -> &str {
    match model.split_once('/') {
        Some((_, rest)) if !rest.is_empty() => rest,
        _ => model,
    }
}

/// Where a resolved credential came from. Used for log visibility so the
/// operator can see whether raft supplied the key or we fell back to env.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    /// `provider_config.{api_key, base_url}` from the `agent:start` frame.
    Config,
    /// Provider-specific env var (e.g. `KIMI_API_KEY`).
    Env,
    /// Last-resort `RAFT_LLM_API_KEY` / `RAFT_LLM_BASE_URL` override.
    OverrideEnv,
    /// Hardcoded default per-provider base URL.
    Default,
    /// Nothing found.
    Missing,
}

impl std::fmt::Display for CredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Config => "config.provider",
            Self::Env => "provider env var",
            Self::OverrideEnv => "RAFT_LLM_* override",
            Self::Default => "built-in default",
            Self::Missing => "missing",
        };
        f.write_str(s)
    }
}

/// Resolved LLM credentials after walking the fallback chain.
#[derive(Debug, Clone, Default)]
pub struct ResolvedLlmCredentials {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub api_key_source: Option<CredentialSource>,
    pub base_url_source: Option<CredentialSource>,
}

/// Env var names checked for each known provider (in priority order).
///
/// `RUSTY_API_KEY` is always first because that's rusty's own highest-
/// priority env var (per the rusty README "API Key Resolution" section).
/// Otherwise we follow the per-provider conventions of the major LLM CLIs.
/// The first present var wins.
fn env_var_candidates(provider_id: Option<&str>, preset: Option<&str>) -> Vec<&'static str> {
    let mut out = vec!["RUSTY_API_KEY"];
    let pid = provider_id.unwrap_or("").to_ascii_lowercase();
    match preset {
        Some("kimi") => out.extend(["KIMI_API_KEY", "MOONSHOT_API_KEY", "OPENAI_API_KEY"]),
        Some("deepseek") => out.extend(["DEEPSEEK_API_KEY", "OPENAI_API_KEY"]),
        Some("xiaomi") => out.extend(["XIAOMI_API_KEY", "MIMO_API_KEY", "OPENAI_API_KEY"]),
        Some("ollama") => out.push("OLLAMA_API_KEY"),
        Some("openai") => out.push("OPENAI_API_KEY"),
        _ => {}
    }
    // Heuristic on the provider id when no preset matched.
    if preset.is_none() {
        if pid.contains("kimi") || pid.contains("moonshot") {
            out.extend(["KIMI_API_KEY", "MOONSHOT_API_KEY"]);
        } else if pid.contains("deepseek") {
            out.push("DEEPSEEK_API_KEY");
        } else if pid.contains("anthropic") || pid.contains("claude") {
            out.push("ANTHROPIC_API_KEY");
        } else if pid.contains("zhipu") || pid.contains("glm") {
            out.extend(["ZHIPU_API_KEY", "GLM_API_KEY"]);
        } else if pid.contains("openai") {
            out.push("OPENAI_API_KEY");
        }
    }
    // NOTE: RAFT_LLM_API_KEY is intentionally NOT included here; it's the
    // last-resort override checked separately below so its source is tagged
    // `OverrideEnv`, not `Env`.
    out
}

/// Resolve LLM credentials for an agent.
///
/// Walks the fallback chain. Both `api_key` and `base_url` start as `None`;
/// we only set them when we find an explicit value somewhere. When neither
/// is found, we pass neither `--api-key` nor `--api-base` to RustyCLI and
/// let it fall back to its own resolution chain (`RUSTY_API_KEY` →
/// `OPENAI_API_KEY` → OS keyring → `~/.rusty/settings.json`, plus the
/// `--preset`'s built-in base URL). This is intentional: rusty knows its
/// own provider defaults better than we do.
///
/// Sources checked, in order:
///
/// 1. **`provider_config.{api_key, base_url}`** — what raft's `agent:start`
///    frame supplied (extracted by [`ProviderConfig::from_config`]). Often
///    empty for `runtime="builtin"` when the server omits credentials and
///    expects the daemon to pull them from another source.
/// 2. **Env vars** — `RUSTY_API_KEY` first (rusty's preferred env name),
///    then provider-specific (`KIMI_API_KEY`, `MOONSHOT_API_KEY`,
///    `DEEPSEEK_API_KEY`, `OPENAI_API_KEY`, …).
/// 3. **`RAFT_LLM_API_KEY` / `RAFT_LLM_BASE_URL`** — last-resort override
///    env vars that work for any provider.
///
/// No hardcoded URL defaults — that's rusty's job via `--preset`.
pub fn resolve_llm_credentials(
    provider_config: Option<&ProviderConfig>,
    provider_id: Option<&str>,
    preset: Option<&str>,
) -> ResolvedLlmCredentials {
    let mut out = ResolvedLlmCredentials::default();

    // 1. provider_config.api_key
    if let Some(key) = provider_config
        .and_then(|p| p.api_key.as_deref())
        .filter(|s| !s.is_empty())
    {
        out.api_key = Some(key.to_string());
        out.api_key_source = Some(CredentialSource::Config);
    }

    // 1. provider_config.base_url
    if let Some(base) = provider_config
        .and_then(|p| p.base_url.as_deref())
        .filter(|s| !s.is_empty())
    {
        out.base_url = Some(base.to_string());
        out.base_url_source = Some(CredentialSource::Config);
    }

    // 2. env vars (RUSTY_API_KEY first, then provider-specific)
    if out.api_key.is_none() {
        for var in env_var_candidates(provider_id, preset) {
            if let Some(val) = std::env::var(var).ok().filter(|s| !s.is_empty()) {
                out.api_key = Some(val);
                out.api_key_source = Some(CredentialSource::Env);
                break;
            }
        }
    }

    // 3. RAFT_LLM_API_KEY / RAFT_LLM_BASE_URL overrides
    if out.api_key.is_none() {
        if let Some(val) = std::env::var("RAFT_LLM_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())
        {
            out.api_key = Some(val);
            out.api_key_source = Some(CredentialSource::OverrideEnv);
        }
    }
    if out.base_url.is_none() {
        if let Some(val) = std::env::var("RAFT_LLM_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
        {
            out.base_url = Some(val);
            out.base_url_source = Some(CredentialSource::OverrideEnv);
        }
    }

    if out.api_key.is_none() {
        out.api_key_source = Some(CredentialSource::Missing);
    }
    // base_url_source stays None when not set — that signals "let rusty's
    // --preset pick its own default".

    out
}

/// Set up the agent's memory workspace: ensure `MEMORY.md` exists and a
/// `notes/` directory is present. If the user previously ran the npm daemon,
/// migrate `MEMORY.md` and notes from `~/.slock/agents/<agent_id>/` so the
/// agent keeps its memory across the Rust port. Otherwise seed a minimal
/// initial `MEMORY.md` from the agent name and description sent by the
/// server.
///
/// # Errors
///
/// Returns an error if the workspace, notes directory, or memory file cannot
/// be created, or if copying from the legacy slock location fails.
pub fn setup_agent_memory(
    agent_id: &str,
    name: &str,
    description: &str,
    workspace: &Path,
) -> Result<()> {
    let memory_path = workspace.join("MEMORY.md");
    let notes_dir = workspace.join("notes");

    std::fs::create_dir_all(&notes_dir)
        .with_context(|| format!("creating notes dir {}", notes_dir.display()))?;

    if memory_path.exists() {
        info!(
            agent_id = %agent_id,
            path = %memory_path.display(),
            "MEMORY.md already exists"
        );
        return Ok(());
    }

    // Try to migrate from the npm daemon's legacy location.
    if let Some(user_home) = user_home_dir_from_env() {
        let legacy_dir = user_home.join(".slock").join("agents").join(agent_id);
        let legacy_memory = legacy_dir.join("MEMORY.md");
        if legacy_memory.exists() {
            info!(
                agent_id = %agent_id,
                legacy = %legacy_memory.display(),
                dest = %memory_path.display(),
                "migrating MEMORY.md from legacy slock location"
            );
            std::fs::copy(&legacy_memory, &memory_path).with_context(|| {
                format!(
                    "copying {} to {}",
                    legacy_memory.display(),
                    memory_path.display()
                )
            })?;

            let legacy_notes = legacy_dir.join("notes");
            if legacy_notes.is_dir() {
                migrate_notes_dir(&legacy_notes, &notes_dir)?;
            }
        }
    }

    if memory_path.exists() {
        info!(
            agent_id = %agent_id,
            path = %memory_path.display(),
            "MEMORY.md ready after migration"
        );
    } else {
        let initial = build_initial_memory_md(name, description);
        std::fs::write(&memory_path, initial)
            .with_context(|| format!("writing initial MEMORY.md {}", memory_path.display()))?;
        info!(
            agent_id = %agent_id,
            path = %memory_path.display(),
            "created initial MEMORY.md"
        );
    }

    Ok(())
}

/// Best-effort migration of the legacy `notes/` directory into the new agent
/// workspace. Files are copied only if the destination doesn't already exist.
fn migrate_notes_dir(src: &Path, dst: &Path) -> Result<()> {
    for entry in std::fs::read_dir(src)
        .with_context(|| format!("reading legacy notes dir {}", src.display()))?
    {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_file() && !dst_path.exists() {
            std::fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "copying note {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }
    Ok(())
}

/// Build a minimal initial `MEMORY.md` matching the npm daemon's format.
fn build_initial_memory_md(name: &str, description: &str) -> String {
    format!(
        "# {name}\n\n## Role\n{description}\n\n## Key Knowledge\n- No notes yet.\n\n## Active Context\n- First startup.\n"
    )
}

/// Resolve the user's home directory from `$HOME` (Unix) or `$USERPROFILE`
/// (Windows). This intentionally mirrors the logic in `crate::daemon::paths`
/// without exposing the private helper.
fn user_home_dir_from_env() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
    }
    #[cfg(not(unix))]
    {
        std::env::var_os("USERPROFILE")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
    }
}

/// Resolve the per-agent workspace directory.
///
/// Layout: `<daemon_home>/agents/<agent_id>/`. The agent_id is already
/// filesystem-safe (server-assigned `ag_…` slug).
pub fn workspace_for(agent_id: &str, home: &Path) -> PathBuf {
    home.join("agents").join(agent_id)
}

/// Concurrency-safe registry of running agent processes.
///
/// One instance lives for the lifetime of a daemon connection; the runner
/// holds it behind `Arc` and shares it between the message dispatcher and
/// (future) background tasks.
#[derive(Default)]
pub struct AgentProcessRegistry {
    processes: DashMap<String, AgentProcess>,
}

impl AgentProcessRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            processes: DashMap::new(),
        }
    }

    /// Insert / replace an agent's state.
    pub fn install(&self, process: AgentProcess) {
        let agent_id = process.agent_id.clone();
        info!(agent_id = %agent_id, model = %process.model, "installed agent process");
        self.processes.insert(agent_id, process);
    }

    /// Remove an agent's state. Returns the removed entry, if any.
    pub fn remove(&self, agent_id: &str) -> Option<AgentProcess> {
        self.processes.remove(agent_id).map(|(_, v)| v)
    }

    /// Read-only access to an agent's state via a closure (avoids leaking
    /// the dashmap guard).
    pub fn with<F, R>(&self, agent_id: &str, f: F) -> Option<R>
    where
        F: FnOnce(&AgentProcess) -> R,
    {
        self.processes.get(agent_id).map(|r| f(&r))
    }

    /// Update an agent's state via a closure.
    pub fn update<F>(&self, agent_id: &str, f: F)
    where
        F: FnOnce(&mut AgentProcess),
    {
        if let Some(mut entry) = self.processes.get_mut(agent_id) {
            f(&mut entry);
        }
    }

    /// Whether the agent is currently tracked as started.
    pub fn contains(&self, agent_id: &str) -> bool {
        self.processes.contains_key(agent_id)
    }

    /// Snapshot of all currently-tracked agent IDs.
    pub fn agent_ids(&self) -> Vec<String> {
        self.processes.iter().map(|kv| kv.key().clone()).collect()
    }

    /// Look up an agent by its local proxy bearer token.
    pub fn find_by_proxy_token(&self, token: &str) -> Option<AgentProcess> {
        self.processes.iter().find_map(|kv| {
            let process = kv.value();
            if process.agent_proxy_token.as_deref() == Some(token) {
                Some(process.clone())
            } else {
                None
            }
        })
    }
}

/// Resolve the RustyCLI binary path.
///
/// Same resolution rules as the ready-frame detection: `$RAFT_RUSTY_BINARY`
/// overrides everything; otherwise `which rusty` / `rustycli` / `rusty-cli`.
pub fn resolve_rustycli_path() -> Option<PathBuf> {
    crate::daemon::runner::resolve_rustycli_path()
}

/// Spawn RustyCLI for a single delivery and return its stdout.
///
/// Picks a `--preset` based on the agent's `providerId` (so non-OpenAI
/// providers like Kimi, DeepSeek, etc. use the right endpoint shape),
/// passes the LLM key via `--api-key` (NOT `OPENAI_API_KEY` env — that
/// would force OpenAI semantics), uses `--api-base` when a custom
/// `baseUrl` was supplied, and strips any raft `<provider>/` prefix from
/// the model name so RustyCLI recognises it.
///
/// # Errors
///
/// Returns an error if RustyCLI isn't installed, the spawn fails, or the
/// child exits non-zero.
pub async fn run_one_turn(
    process: &AgentProcess,
    prompt: &str,
    message_id: Option<&str>,
) -> Result<String> {
    let binary = resolve_rustycli_path().context(
        "RustyCLI binary not found; install it or set RAFT_RUSTY_BINARY. \
         Until then this daemon cannot run agents.",
    )?;

    // Strip the raft `<provider>/` prefix so e.g. `kimi-coding/kimi-for-coding`
    // becomes `kimi-for-coding`, which rusty recognises once `--preset kimi`
    // is also set.
    let model_for_rusty = strip_provider_prefix(&process.model);

    let preset = pick_rustycli_preset(process.provider_id.as_deref(), Some(&process.model));

    let mut cmd = Command::new(&binary);
    cmd.arg("--headless")
        .arg("--permissions")
        .arg("bypass")
        .arg("--prompt")
        .arg(prompt)
        .arg("--cwd")
        .arg(&process.workspace)
        .env("SLOCK_AGENT_ID", &process.agent_id)
        .env(
            "SLOCK_AGENT_ACTIVE_CAPABILITIES",
            DEFAULT_ACTIVE_CAPABILITIES,
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if !model_for_rusty.is_empty() {
        cmd.arg("--model").arg(model_for_rusty);
    }
    if let Some(session) = &process.session_id {
        cmd.arg("--resume").arg(session);
    }
    if let Some(base_url) = &process.llm_base_url {
        cmd.arg("--api-base").arg(base_url);
    }
    if let Some(preset) = preset {
        cmd.arg("--preset").arg(preset);
    }
    if let Some(key) = &process.llm_api_key {
        // Pass via the explicit flag, NOT the OPENAI_API_KEY env var —
        // the env var is OpenAI-specific and would force OpenAI semantics
        // even when --preset points at a different provider.
        cmd.arg("--api-key").arg(key);
    }
    if let Some(server_url) = &process.server_url {
        cmd.env("SLOCK_SERVER_URL", server_url);
    }
    if let Some(proxy_url) = &process.proxy_url {
        cmd.env("SLOCK_AGENT_PROXY_URL", proxy_url);
    }
    if let Some(token_file) = &process.agent_proxy_token_file {
        cmd.env("SLOCK_AGENT_PROXY_TOKEN_FILE", token_file);
    }
    if let Some(id) = message_id {
        cmd.env("SLOCK_MESSAGE_ID", id);
    }

    let bin_dir = process.home.join("bin");
    let existing_path = std::env::var("PATH").unwrap_or_default();
    let mut path_parts = std::env::split_paths(&existing_path).collect::<Vec<_>>();
    path_parts.insert(0, bin_dir.clone());
    let new_path = std::env::join_paths(path_parts)
        .unwrap_or_else(|_| OsString::from(&existing_path));
    cmd.env("SLOCK_CLI", bin_dir.join("raft"))
        .env("PATH", new_path);

    info!(
        agent_id = %process.agent_id,
        binary = %binary.display(),
        model = model_for_rusty,
        preset = preset,
        base_url = ?process.llm_base_url.as_deref(),
        api_key_present = process.llm_api_key.is_some(),
        prompt_len = prompt.len(),
        "spawning RustyCLI",
    );
    tracing::debug!(
        agent_id = %process.agent_id,
        prompt = %prompt,
        "RustyCLI prompt",
    );

    let mut child = cmd
        .spawn()
        .with_context(|| format!("spawning RustyCLI at {}", binary.display()))?;

    // Read stdout/stderr in the background so we can still collect partial
    // output if the turn times out and we have to kill the child.
    let mut stdout = child.stdout.take().context("capturing RustyCLI stdout")?;
    let mut stderr = child.stderr.take().context("capturing RustyCLI stderr")?;
    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf).await;
        buf
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf).await;
        buf
    });

    let status = timeout(RUSTY_TURN_TIMEOUT, child.wait())
        .await
        .map_err(|_| {
            warn!(
                agent_id = %process.agent_id,
                timeout_secs = RUSTY_TURN_TIMEOUT.as_secs(),
                "RustyCLI turn timed out; killing process"
            );
        });

    let output = match status {
        Ok(Ok(status)) => {
            let stdout = stdout_task.await.unwrap_or_default();
            let stderr = stderr_task.await.unwrap_or_default();
            (status, stdout, stderr)
        }
        Ok(Err(err)) => {
            // Process wait failed; still try to collect output for diagnostics.
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(err.into());
        }
        Err(()) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            let stdout = stdout_task.await.unwrap_or_default();
            let stderr = stderr_task.await.unwrap_or_default();
            let stdout_str = String::from_utf8_lossy(&stdout);
            let trimmed = stdout_str.trim();
            if trimmed.is_empty() {
                anyhow::bail!(
                    "RustyCLI turn timed out after {} seconds",
                    RUSTY_TURN_TIMEOUT.as_secs()
                );
            }
            // Partial output from a timed-out process may begin with the
            // NO_REPLY marker if the model was still emitting its internal
            // reasoning. Treat that as a timeout error so the agent posts a
            // fallback message instead of silently suppressing the partial
            // output as a normal "no reply".
            if starts_with_no_reply_marker(trimmed) {
                anyhow::bail!(
                    "RustyCLI turn timed out after {} seconds; partial response began with NO_REPLY marker",
                    RUSTY_TURN_TIMEOUT.as_secs()
                );
            }
            warn!(
                agent_id = %process.agent_id,
                stdout_len = stdout_str.len(),
                stderr = %String::from_utf8_lossy(&stderr),
                "using partial stdout after timeout"
            );
            return Ok(stdout_str.trim().to_string());
        }
    };

    let status = output.0;
    let stdout = String::from_utf8_lossy(&output.1);
    let stderr = String::from_utf8_lossy(&output.2);

    if !status.success() {
        if stdout.trim().is_empty() {
            anyhow::bail!("rusty exited status={status} — stderr: {stderr} — stdout: {stdout}");
        }
        // RustyCLI may emit a model response on stdout even when it exits
        // non-zero due to a follow-up tool/API error. In that case the useful
        // answer is still in stdout, so return it and log the stderr as a warning.
        warn!(
            agent_id = %process.agent_id,
            status = %status,
            stderr = %stderr,
            "rusty exited non-zero but produced stdout; using stdout as response"
        );
        return Ok(stdout.trim().to_string());
    }

    Ok(stdout.trim().to_string())
}

/// Convenience: ensure the agents directory exists under the daemon home.
///
/// Called once at startup so per-agent workspace creation doesn't have to
/// defend against a missing parent.
///
/// # Errors
///
/// Returns an error if the daemon home or `agents/` subdir cannot be
/// created.
pub fn ensure_agents_dir() -> Result<PathBuf> {
    let home = paths::home_dir()?;
    let agents = home.join("agents");
    std::fs::create_dir_all(&agents)
        .with_context(|| format!("creating agents dir {}", agents.display()))?;
    Ok(agents)
}

/// Shared registry type used by the runner.
pub type SharedAgentProcessRegistry = Arc<AgentProcessRegistry>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::EnvGuard;

    #[test]
    fn workspace_for_namespaces_under_agents() {
        let home = PathBuf::from("/tmp/raft-test");
        let ws = workspace_for("ag_123", &home);
        assert_eq!(ws, PathBuf::from("/tmp/raft-test/agents/ag_123"));
    }

    #[test]
    fn from_start_picks_up_model_and_runtime() {
        let tmp = tempfile::tempdir().unwrap();
        let config = serde_json::json!({
            "runtime": "builtin",
            "model": "claude-opus-4-8",
        });
        let p = AgentProcess::from_start(
            "ag_1",
            &config,
            Some(&serde_json::json!("launch_42")),
            tmp.path(),
        )
        .unwrap();
        assert_eq!(p.agent_id, "ag_1");
        assert_eq!(p.runtime, "builtin");
        assert_eq!(p.model, "claude-opus-4-8");
        assert_eq!(p.launch_id.as_deref(), Some("launch_42"));
        assert!(p.workspace.starts_with(tmp.path()));
        assert!(p.workspace.is_dir());
        assert!(p.llm_api_key.is_none());
        assert!(p.llm_base_url.is_none());
        assert!(p.agent_credential_key.is_none());
    }

    #[test]
    fn from_start_extracts_provider_block() {
        let tmp = tempfile::tempdir().unwrap();
        let config = serde_json::json!({
            "runtime": "builtin",
            "model": "gpt-4o",
            "provider": {
                "kind": "gateway",
                "providerId": "openai",
                "apiKey": "sk-llm-xyz",
                "baseUrl": "https://api.openai.com/v1",
            },
        });
        let p = AgentProcess::from_start("ag_p", &config, None, tmp.path()).unwrap();
        assert_eq!(p.llm_api_key.as_deref(), Some("sk-llm-xyz"));
        assert_eq!(p.llm_base_url.as_deref(), Some("https://api.openai.com/v1"));
        assert_eq!(p.provider_id.as_deref(), Some("openai"));
    }

    #[test]
    fn from_start_defaults_runtime_to_builtin() {
        let tmp = tempfile::tempdir().unwrap();
        let config = serde_json::json!({}); // no runtime/model
        let p = AgentProcess::from_start("ag_x", &config, None, tmp.path()).unwrap();
        assert_eq!(p.runtime, "builtin");
        assert!(p.model.is_empty());
        assert_eq!(p.name, "ag_x");
        assert_eq!(p.description, "No role defined yet.");
        assert!(p.launch_id.is_none());
    }

    #[test]
    fn from_start_extracts_name_and_description() {
        let tmp = tempfile::tempdir().unwrap();
        let config = serde_json::json!({
            "name": "Alice",
            "displayName": "Alice the Coder",
            "description": "Full-stack assistant.",
        });
        let p = AgentProcess::from_start("ag_alice", &config, None, tmp.path()).unwrap();
        // displayName takes precedence over name.
        assert_eq!(p.name, "Alice the Coder");
        assert_eq!(p.description, "Full-stack assistant.");
    }

    #[test]
    fn from_start_creates_initial_memory_md() {
        let tmp = tempfile::tempdir().unwrap();
        let config = serde_json::json!({
            "name": "Cindy",
            "description": "Onboarding lead.",
        });
        let p = AgentProcess::from_start("ag_cindy", &config, None, tmp.path()).unwrap();
        let memory_path = p.workspace.join("MEMORY.md");
        assert!(memory_path.exists());
        let contents = std::fs::read_to_string(memory_path).unwrap();
        assert!(contents.contains("# Cindy"));
        assert!(contents.contains("Onboarding lead."));
        assert!(p.workspace.join("notes").is_dir());
    }

    #[test]
    fn from_start_migrates_legacy_memory_md() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();

        // Scope HOME to this test so other tests don't see the legacy path.
        let _home_guard = unsafe { EnvGuard::set("HOME", &home) };

        let agent_id = "ag_migrator";
        let legacy_agent = home.join(".slock").join("agents").join(agent_id);
        std::fs::create_dir_all(&legacy_agent).unwrap();
        std::fs::write(legacy_agent.join("MEMORY.md"), "# Legacy memory\n").unwrap();
        std::fs::create_dir_all(legacy_agent.join("notes")).unwrap();
        std::fs::write(legacy_agent.join("notes").join("legacy-note.md"), "note\n").unwrap();

        let daemon_home = tmp.path().join("raft-daemon");
        let p =
            AgentProcess::from_start(agent_id, &serde_json::json!({}), None, &daemon_home).unwrap();

        assert_eq!(
            std::fs::read_to_string(p.workspace.join("MEMORY.md")).unwrap(),
            "# Legacy memory\n"
        );
        assert!(p.workspace.join("notes").join("legacy-note.md").exists());
    }

    #[test]
    fn from_start_infers_provider_id_from_model_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let config = serde_json::json!({
            "runtime": "builtin",
            "model": "kimi-coding/kimi-for-coding",
        });
        let p = AgentProcess::from_start("ag_k", &config, None, tmp.path()).unwrap();
        assert_eq!(p.provider_id.as_deref(), Some("kimi-coding"));
        assert_eq!(p.model, "kimi-coding/kimi-for-coding");
    }

    #[test]
    fn registry_install_remove_contains() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = AgentProcessRegistry::new();
        assert!(!reg.contains("ag_a"));

        let p = AgentProcess {
            agent_id: "ag_a".into(),
            name: "Agent A".into(),
            description: "A test agent".into(),
            runtime: "builtin".into(),
            model: "sonnet".into(),
            workspace: tmp.path().join("ag_a"),
            home: tmp.path().to_path_buf(),
            session_id: None,
            launch_id: None,
            llm_api_key: None,
            llm_base_url: None,
            provider_id: None,
            agent_credential_key: None,
            agent_credential_id: None,
            agent_proxy_token: None,
            agent_proxy_token_file: None,
            proxy_url: None,
            server_url: None,
            activity_client_seq: Arc::new(AtomicU64::new(0)),
            turn_lock: Arc::new(tokio::sync::Mutex::new(())),
        };
        reg.install(p);
        assert!(reg.contains("ag_a"));

        let model = reg.with("ag_a", |p| p.model.clone());
        assert_eq!(model.as_deref(), Some("sonnet"));

        reg.update("ag_a", |p| p.session_id = Some("sess_1".into()));
        // `with` wraps the closure return in another Option, so we end up with
        // Option<Option<String>>. Flatten and unwrap the outer (agent must exist
        // since we just installed it).
        let sid = reg
            .with("ag_a", |p| p.session_id.clone())
            .flatten()
            .unwrap_or_default();
        assert_eq!(sid, "sess_1");

        let removed = reg.remove("ag_a");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().model, "sonnet");
        assert!(!reg.contains("ag_a"));
    }

    #[test]
    fn pick_preset_matches_kimi_provider() {
        assert_eq!(
            pick_rustycli_preset(Some("moonshot-kimi"), Some("kimi-coding/kimi-for-coding")),
            Some("kimi"),
        );
    }

    #[test]
    fn pick_preset_matches_openai_model_prefix() {
        assert_eq!(pick_rustycli_preset(None, Some("gpt-4o")), Some("openai"));
    }

    #[test]
    fn pick_preset_matches_deepseek() {
        assert_eq!(
            pick_rustycli_preset(Some("deepseek"), Some("deepseek-v3")),
            Some("deepseek"),
        );
    }

    #[test]
    fn pick_preset_returns_none_for_unknown() {
        assert_eq!(
            pick_rustycli_preset(Some("anthropic"), Some("claude-3")),
            None
        );
        assert_eq!(pick_rustycli_preset(None, Some("some-custom-model")), None);
    }

    #[test]
    fn strip_provider_prefix_handles_provider_slash_model() {
        assert_eq!(
            strip_provider_prefix("kimi-coding/kimi-for-coding"),
            "kimi-for-coding",
        );
        assert_eq!(strip_provider_prefix("openai/gpt-4o"), "gpt-4o");
    }

    #[test]
    fn strip_provider_prefix_passes_through_when_no_slash() {
        assert_eq!(strip_provider_prefix("sonnet"), "sonnet");
        assert_eq!(strip_provider_prefix(""), "");
    }

    #[test]
    fn resolve_prefers_config_provider_block() {
        let provider = ProviderConfig {
            api_key: Some("sk-config".into()),
            base_url: Some("https://from.config/v1".into()),
            ..Default::default()
        };
        let creds = resolve_llm_credentials(Some(&provider), Some("openai"), Some("openai"));
        assert_eq!(creds.api_key.as_deref(), Some("sk-config"));
        assert_eq!(creds.api_key_source, Some(CredentialSource::Config));
        assert_eq!(creds.base_url.as_deref(), Some("https://from.config/v1"));
        assert_eq!(creds.base_url_source, Some(CredentialSource::Config));
    }

    #[test]
    fn resolve_does_not_default_base_url_for_kimi_preset() {
        // Rusty's --preset kimi knows its own base URL (https://api.kimi.com/coding/v1/).
        // We must NOT inject a different default or we'd override the preset.
        let _g1 = unsafe { EnvGuard::remove("KIMI_API_KEY") };
        let _g2 = unsafe { EnvGuard::remove("MOONSHOT_API_KEY") };
        let _g3 = unsafe { EnvGuard::remove("RAFT_LLM_API_KEY") };
        let _g4 = unsafe { EnvGuard::remove("RAFT_LLM_BASE_URL") };

        let creds = resolve_llm_credentials(None, None, Some("kimi"));
        assert!(creds.base_url.is_none(), "expected no default base URL");
        assert!(
            creds.base_url_source.is_none(),
            "expected no base_url_source, got {:?}",
            creds.base_url_source,
        );
    }

    #[test]
    fn resolve_uses_provider_env_var_when_config_missing() {
        let _g1 = unsafe { EnvGuard::remove("OPENAI_API_KEY") };
        let _g2 = unsafe { EnvGuard::remove("RAFT_LLM_API_KEY") };
        let _g3 = unsafe { EnvGuard::set("KIMI_API_KEY", "sk-from-env") };

        let creds = resolve_llm_credentials(None, None, Some("kimi"));
        assert_eq!(creds.api_key.as_deref(), Some("sk-from-env"));
        assert_eq!(creds.api_key_source, Some(CredentialSource::Env));
    }

    #[test]
    fn resolve_falls_back_to_raft_llm_api_key() {
        let _g1 = unsafe { EnvGuard::remove("OPENAI_API_KEY") };
        let _g2 = unsafe { EnvGuard::remove("KIMI_API_KEY") };
        let _g3 = unsafe { EnvGuard::set("RAFT_LLM_API_KEY", "sk-override") };

        let creds = resolve_llm_credentials(None, None, None);
        assert_eq!(creds.api_key.as_deref(), Some("sk-override"));
        assert_eq!(creds.api_key_source, Some(CredentialSource::OverrideEnv));
    }

    #[test]
    fn resolve_reports_missing_when_nothing_found() {
        let _g1 = unsafe { EnvGuard::remove("OPENAI_API_KEY") };
        let _g2 = unsafe { EnvGuard::remove("KIMI_API_KEY") };
        let _g3 = unsafe { EnvGuard::remove("MOONSHOT_API_KEY") };
        let _g4 = unsafe { EnvGuard::remove("RAFT_LLM_API_KEY") };

        let creds = resolve_llm_credentials(None, None, None);
        assert!(creds.api_key.is_none());
        assert_eq!(creds.api_key_source, Some(CredentialSource::Missing));
    }

    #[test]
    fn provider_config_reads_camel_case_provider_block() {
        let config = serde_json::json!({
            "provider": {
                "kind": "preset",
                "providerId": "kimi-coding",
                "apiKey": "sk-kimi",
                "baseUrl": "https://api.kimi.com/v1",
            }
        });
        let p = ProviderConfig::from_config(&config);
        assert_eq!(p.kind.as_deref(), Some("preset"));
        assert_eq!(p.provider_id.as_deref(), Some("kimi-coding"));
        assert_eq!(p.api_key.as_deref(), Some("sk-kimi"));
        assert_eq!(p.base_url.as_deref(), Some("https://api.kimi.com/v1"));
    }

    #[test]
    fn provider_config_reads_snake_case_provider_block() {
        let config = serde_json::json!({
            "provider": {
                "kind": "gateway",
                "provider_id": "openai-compatible",
                "api_key": "sk-openai",
                "base_url": "https://gateway.example.com/v1",
            }
        });
        let p = ProviderConfig::from_config(&config);
        assert_eq!(p.kind.as_deref(), Some("gateway"));
        assert_eq!(p.provider_id.as_deref(), Some("openai-compatible"));
        assert_eq!(p.api_key.as_deref(), Some("sk-openai"));
        assert_eq!(
            p.base_url.as_deref(),
            Some("https://gateway.example.com/v1")
        );
    }

    #[test]
    fn provider_config_reads_nested_runtime_config_provider() {
        let config = serde_json::json!({
            "runtime": "builtin",
            "model": "kimi-coding/kimi-for-coding",
            "runtimeConfig": {
                "provider": {
                    "kind": "preset",
                    "providerId": "kimi-coding",
                    "apiKey": "sk-nested",
                }
            }
        });
        let p = ProviderConfig::from_config(&config);
        assert_eq!(p.provider_id.as_deref(), Some("kimi-coding"));
        assert_eq!(p.api_key.as_deref(), Some("sk-nested"));
    }

    #[test]
    fn provider_config_infers_provider_id_from_model_prefix() {
        let config = serde_json::json!({
            "runtime": "builtin",
            "model": "kimi-coding/kimi-for-coding",
        });
        let p = ProviderConfig::from_config(&config);
        assert_eq!(p.provider_id.as_deref(), Some("kimi-coding"));
        assert!(p.api_key.is_none());
    }
}
