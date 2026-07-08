//! Agent module.

pub mod manager;
pub mod process;
pub mod raft_client;

pub use manager::*;
pub use process::{
    AgentProcess, AgentProcessRegistry, CredentialSource, ProviderConfig, ResolvedLlmCredentials,
    SharedAgentProcessRegistry, ensure_agents_dir, pick_rustycli_preset,
    resolve_llm_credentials, resolve_rustycli_path, run_one_turn, strip_provider_prefix,
    workspace_for,
};
pub use raft_client::{
    RunnerCredential, SendBody, SendResponse, derive_target, mint_runner_credential,
    send_agent_message,
};
