//! Unit tests for models.

use raft_daemon::models::*;

#[test]
fn test_agent_status_display() {
    assert_eq!(format!("{}", AgentStatus::Online), "online");
    assert_eq!(format!("{}", AgentStatus::Busy), "busy");
    assert_eq!(format!("{}", AgentStatus::Error), "error");
    assert_eq!(format!("{}", AgentStatus::Offline), "offline");
}

#[test]
fn test_reset_mode_display() {
    assert_eq!(format!("{}", ResetMode::Restart), "restart");
    assert_eq!(format!("{}", ResetMode::SessionReset), "session_reset");
    assert_eq!(format!("{}", ResetMode::FullReset), "full_reset");
}

#[test]
fn test_agent_methods() {
    let mut agent = Agent::with_defaults(
        "agent-1".to_string(),
        "Test Agent".to_string(),
        "A test agent".to_string(),
        "member".to_string(),
        "rusty".to_string(),
        "computer-1".to_string(),
        "server-1".to_string(),
        vec!["channel-1".to_string()],
        ResetMode::Restart,
        "/workspace".to_string(),
    );

    assert_eq!(agent.name, "Test Agent");
    assert_eq!(agent.role, "member");
    assert_eq!(agent.runtime, "rusty");
    assert_eq!(agent.computer_id, "computer-1");
    assert_eq!(agent.server_id, "server-1");
    assert_eq!(agent.channel_ids.len(), 1);
    assert_eq!(agent.reset_mode, ResetMode::Restart);

    // Test set_status
    agent.set_status(AgentStatus::Online);
    assert!(agent.is_online());
    assert!(!agent.is_offline());

    // Test join_channel
    agent.join_channel("channel-2".to_string());
    assert!(agent.is_member_of_channel("channel-2"));
}

#[test]
fn test_server_methods() {
    let mut server = Server::with_defaults(
        "server-1".to_string(),
        "Test Server".to_string(),
        "test-slug".to_string(),
        "user-1".to_string(),
        vec!["agent-1".to_string()],
        vec!["channel-1".to_string()],
        vec!["user-2".to_string()],
        vec!["computer-1".to_string()],
    );

    assert_eq!(server.name, "Test Server");
    assert_eq!(server.slug, "test-slug");
    assert_eq!(server.owner_id, "user-1");
    assert_eq!(server.agent_ids().len(), 1);
    assert_eq!(server.channel_ids().len(), 1);
    assert_eq!(server.member_ids().len(), 1);
    assert_eq!(server.computer_ids().len(), 1);

    // Test add_agent
    server.add_agent("agent-2".to_string());
    assert_eq!(server.agent_ids().len(), 2);
}

#[test]
fn test_computer_methods() {
    let mut computer = Computer::with_defaults(
        "computer-1".to_string(),
        "Test Computer".to_string(),
        "server-1".to_string(),
        "api-key-1".to_string(),
        "https://app.raft.build".to_string(),
        "setup-command".to_string(),
    );

    assert_eq!(computer.name, "Test Computer");
    assert_eq!(computer.server_id, "server-1");
    assert_eq!(computer.api_key, "api-key-1");
    assert!(computer.is_online());
    assert!(!computer.is_offline());

    // Test set_status
    computer.set_status(ComputerStatus::Offline);
    assert!(!computer.is_online());
    assert!(computer.is_offline());
}

#[test]
fn test_task_methods() {
    let mut task = Task::with_defaults(
        "task-1".to_string(),
        "Test Task".to_string(),
        "A test task".to_string(),
        "channel-1".to_string(),
        None,
        "agent-1".to_string(),
    );

    assert_eq!(task.title, "Test Task");
    assert_eq!(task.channel_id, "channel-1");
    assert_eq!(task.assigned_to, "agent-1");
    assert_eq!(task.status, TaskStatus::Claimed);

    // Test claim
    task.claim("agent-2".to_string());
    assert_eq!(task.assigned_to, "agent-2");
    assert!(task.is_claimed());
}

#[test]
fn test_message_methods() {
    let message = Message::with_defaults(
        "msg-1".to_string(),
        MessageType::Message,
        "Test message".to_string(),
        "channel-1".to_string(),
        None,
        "agent-1".to_string(),
        123_456,
    );

    assert_eq!(message.r#type, MessageType::Message);
    assert_eq!(message.content, "Test message");
    assert_eq!(message.channel_id, "channel-1");
    assert_eq!(message.sender_id, "agent-1");
    assert_eq!(message.timestamp, 123_456);
    assert!(!message.is_read());
}

#[test]
fn test_reminder_methods() {
    let reminder = Reminder::with_defaults(
        "reminder-1".to_string(),
        "Test Reminder".to_string(),
        3600,
        ReminderInterval::Once,
        0,
        "msg-1".to_string(),
        "agent-1".to_string(),
    );

    assert_eq!(reminder.title, "Test Reminder");
    assert_eq!(reminder.duration, 3600);
    assert_eq!(reminder.interval, ReminderInterval::Once);
    assert!(reminder.is_active());
    assert!(!reminder.is_snoozed());
}

#[test]
fn test_runtime_methods() {
    let runtime = Runtime::with_defaults(
        "runtime-1".to_string(),
        "Test Runtime".to_string(),
        "claude".to_string(),
        "claude-3".to_string(),
        "computer-1".to_string(),
        "api-key-1".to_string(),
        "https://app.raft.build".to_string(),
    );

    assert_eq!(runtime.name, "Test Runtime");
    assert_eq!(runtime.r#type, "claude");
    assert_eq!(runtime.model, "claude-3");
    assert!(runtime.is_ready());
    assert!(!runtime.is_offline());
}
