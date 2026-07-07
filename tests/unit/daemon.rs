//! Unit tests for daemon.
//!
//! Tests exercise the real public API surface: DaemonState (which implements
//! the StateMgr trait) and AgentManager (which takes an Arc<dyn StateMgr>).

use raft_daemon::daemon::agent::AgentManager;
use raft_daemon::daemon::state::DaemonState;
use raft_daemon::daemon::state::StateMgr;
use raft_daemon::models::agent::{Agent, AgentStatus, ResetMode};
use raft_daemon::models::runtime::RuntimeConfig;

use std::path::PathBuf;
use std::sync::Arc;

/// Build a minimal DaemonState suitable for tests.
fn make_state() -> DaemonState {
    DaemonState::new(
        "server-1".to_string(),
        None,
        RuntimeConfig {
            model: "test".to_string(),
            tools: vec![],
            parameters: serde_json::json!({}),
        },
        "test".to_string(),
        PathBuf::from("/tmp"),
    )
}

/// Build a minimal agent.
fn make_agent(id: &str, status: AgentStatus) -> Agent {
    let mut agent = Agent::with_defaults(
        id.to_string(),
        format!("Agent {id}"),
        "A test agent".to_string(),
        "member".to_string(),
        "rusty".to_string(),
        "computer-1".to_string(),
        "server-1".to_string(),
        vec![],
        ResetMode::Restart,
        "/workspace".to_string(),
    );
    agent.status = status;
    agent
}

#[test]
fn test_agent_status_enum() {
    // The real enum is AgentStatus (not AgentState), with these variants.
    assert_ne!(AgentStatus::Online, AgentStatus::Busy);
    assert_ne!(AgentStatus::Busy, AgentStatus::Error);
    assert_ne!(AgentStatus::Error, AgentStatus::Offline);
    assert_ne!(AgentStatus::Offline, AgentStatus::Online);

    assert_eq!(format!("{}", AgentStatus::Online), "online");
    assert_eq!(format!("{}", AgentStatus::Busy), "busy");
    assert_eq!(format!("{}", AgentStatus::Error), "error");
    assert_eq!(format!("{}", AgentStatus::Offline), "offline");
}

#[test]
fn test_daemon_state_implements_statemgr() {
    let state = make_state();
    // DaemonState implements StateMgr; wrap it as a trait object.
    let _sm: Arc<dyn StateMgr> = Arc::new(state);
}

#[test]
fn test_agent_manager_construction() {
    let state = make_state();
    let sm: Arc<dyn StateMgr> = Arc::new(state);
    let _manager = AgentManager::new(sm);
}

#[test]
fn test_agent_manager_add_and_get_agent() {
    let state = make_state();
    let sm: Arc<dyn StateMgr> = Arc::new(state);
    let manager = AgentManager::new(sm);

    let agent = make_agent("agent-1", AgentStatus::Offline);
    let id = manager.add_agent(agent);
    assert_eq!(id, "agent-1");

    let found = manager.get_agent("agent-1");
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, "agent-1");
}

#[test]
fn test_agent_manager_get_missing_agent() {
    let state = make_state();
    let sm: Arc<dyn StateMgr> = Arc::new(state);
    let manager = AgentManager::new(sm);

    assert!(manager.get_agent("does-not-exist").is_none());
}

#[test]
fn test_agent_manager_remove_agent() {
    let state = make_state();
    let sm: Arc<dyn StateMgr> = Arc::new(state);
    let manager = AgentManager::new(sm);

    manager.add_agent(make_agent("agent-1", AgentStatus::Offline));
    assert!(manager.remove_agent("agent-1"));
    assert!(manager.get_agent("agent-1").is_none());
    // Removing again should report false.
    assert!(!manager.remove_agent("agent-1"));
}

#[test]
fn test_agent_manager_get_all_agents() {
    let state = make_state();
    let sm: Arc<dyn StateMgr> = Arc::new(state);
    let manager = AgentManager::new(sm);

    manager.add_agent(make_agent("agent-1", AgentStatus::Offline));
    manager.add_agent(make_agent("agent-2", AgentStatus::Offline));

    let mut ids: Vec<String> = manager.get_all_agents().into_iter().map(|a| a.id).collect();
    ids.sort();
    assert_eq!(ids, vec!["agent-1".to_string(), "agent-2".to_string()]);
}

#[test]
fn test_agent_manager_status_filters() {
    let state = make_state();
    let sm: Arc<dyn StateMgr> = Arc::new(state);
    let manager = AgentManager::new(sm);

    manager.add_agent(make_agent("online-1", AgentStatus::Online));
    manager.add_agent(make_agent("busy-1", AgentStatus::Busy));
    manager.add_agent(make_agent("error-1", AgentStatus::Error));
    manager.add_agent(make_agent("offline-1", AgentStatus::Offline));

    assert_eq!(manager.get_online_agents().len(), 1);
    assert_eq!(manager.get_busy_agents().len(), 1);
    assert_eq!(manager.get_error_agents().len(), 1);
    assert_eq!(manager.get_offline_agents().len(), 1);
}

#[test]
fn test_agent_manager_filter_by_server() {
    let state = make_state();
    let sm: Arc<dyn StateMgr> = Arc::new(state);
    let manager = AgentManager::new(sm);

    // make_agent() defaults to server-1; mutate one to another server.
    let mut other = make_agent("agent-other", AgentStatus::Offline);
    other.server_id = "server-2".to_string();

    manager.add_agent(make_agent("agent-1", AgentStatus::Offline));
    manager.add_agent(other);

    let server1 = manager.get_agents_by_server("server-1");
    let server2 = manager.get_agents_by_server("server-2");
    assert_eq!(server1.len(), 1);
    assert_eq!(server2.len(), 1);
}

#[test]
fn test_agent_manager_filter_by_computer() {
    let state = make_state();
    let sm: Arc<dyn StateMgr> = Arc::new(state);
    let manager = AgentManager::new(sm);

    let mut other = make_agent("agent-other", AgentStatus::Offline);
    other.computer_id = "computer-2".to_string();

    manager.add_agent(make_agent("agent-1", AgentStatus::Offline));
    manager.add_agent(other);

    assert_eq!(manager.get_agents_by_computer("computer-1").len(), 1);
    assert_eq!(manager.get_agents_by_computer("computer-2").len(), 1);
}

#[test]
fn test_agent_model_with_defaults() {
    let agent = make_agent("agent-x", AgentStatus::Offline);
    assert_eq!(agent.id, "agent-x");
    assert_eq!(agent.name, "Agent agent-x");
    assert_eq!(agent.role, "member");
    assert_eq!(agent.runtime, "rusty");
    assert_eq!(agent.computer_id, "computer-1");
    assert_eq!(agent.server_id, "server-1");
    assert_eq!(agent.reset_mode, ResetMode::Restart);
    assert!(agent.is_offline());
}

#[test]
fn test_agent_model_status_helpers() {
    let online = make_agent("a", AgentStatus::Online);
    let busy = make_agent("b", AgentStatus::Busy);
    let error = make_agent("c", AgentStatus::Error);
    let offline = make_agent("d", AgentStatus::Offline);

    assert!(online.is_online() && !online.is_offline());
    assert!(busy.is_busy() && !busy.is_online());
    assert!(error.is_error() && !error.is_online());
    assert!(offline.is_offline() && !offline.is_online());
}
