//! Unit tests for the pure-logic daemon managers: TaskManager, ReminderManager, MessageHandler.
//!
//! These managers do not take a StateMgr dependency; they own their own in-memory maps,
//! so they can be exercised in isolation.

use raft_daemon::daemon::message::MessageHandler;
use raft_daemon::daemon::reminder::ReminderManager;
use raft_daemon::daemon::task::TaskManager;
use raft_daemon::models::Message;
use raft_daemon::models::message::MessageType;
use raft_daemon::models::reminder::{Reminder, ReminderInterval, ReminderStatus};
use raft_daemon::models::task::{Task, TaskStatus};

// ---------- helpers ----------

fn make_task(id: &str, status: TaskStatus, channel: &str, assigned_to: &str) -> Task {
    let mut t = Task::with_defaults(
        id.to_string(),
        format!("Task {id}"),
        format!("Description for {id}"),
        channel.to_string(),
        None,
        assigned_to.to_string(),
    );
    t.status = status;
    t
}

fn make_reminder(id: &str, status: ReminderStatus, interval: ReminderInterval) -> Reminder {
    let mut r = Reminder::with_defaults(
        id.to_string(),
        format!("Reminder {id}"),
        300,
        interval,
        300,
        "msg-anchor".to_string(),
        "author-1".to_string(),
    );
    r.status = status;
    r
}

fn make_message(id: &str, mtype: MessageType, channel: &str, sender: &str) -> Message {
    Message::with_defaults(
        id.to_string(),
        mtype,
        format!("content {id}"),
        channel.to_string(),
        None,
        sender.to_string(),
        0,
    )
}

// ---------- TaskManager ----------

#[test]
fn test_task_manager_add_and_get() {
    let tm = TaskManager::new();
    let id = tm.add_task(make_task("task-1", TaskStatus::Pending, "chan-1", ""));
    assert_eq!(id, "task-1");
    assert!(tm.get_task("task-1").is_some());
    assert!(tm.get_task("missing").is_none());
    assert_eq!(tm.get_all_tasks().len(), 1);
}

#[test]
fn test_task_manager_remove() {
    let tm = TaskManager::new();
    tm.add_task(make_task("task-1", TaskStatus::Pending, "chan-1", ""));
    assert!(tm.remove_task("task-1"));
    assert!(!tm.remove_task("task-1"));
    assert_eq!(tm.get_all_tasks().len(), 0);
}

#[test]
fn test_task_manager_status_filters() {
    let tm = TaskManager::new();
    tm.add_task(make_task("p1", TaskStatus::Pending, "c", ""));
    tm.add_task(make_task("p2", TaskStatus::Pending, "c", ""));
    tm.add_task(make_task("c1", TaskStatus::Claimed, "c", "a1"));
    tm.add_task(make_task("i1", TaskStatus::InProgress, "c", "a1"));
    tm.add_task(make_task("done1", TaskStatus::Completed, "c", "a1"));
    tm.add_task(make_task("fail1", TaskStatus::Failed, "c", "a1"));

    assert_eq!(tm.get_pending_tasks().len(), 2);
    assert_eq!(tm.get_claimed_tasks().len(), 1);
    assert_eq!(tm.get_in_progress_tasks().len(), 1);
    assert_eq!(tm.get_completed_tasks().len(), 1);
    // done = completed + failed
    assert_eq!(tm.get_done_tasks().len(), 2);
}

#[test]
fn test_task_manager_by_channel_and_agent() {
    let tm = TaskManager::new();
    tm.add_task(make_task("t1", TaskStatus::Pending, "chan-a", "a1"));
    tm.add_task(make_task("t2", TaskStatus::Pending, "chan-a", "a2"));
    tm.add_task(make_task("t3", TaskStatus::Pending, "chan-b", "a1"));

    assert_eq!(tm.get_tasks_by_channel("chan-a").len(), 2);
    assert_eq!(tm.get_tasks_by_channel("chan-b").len(), 1);
    assert_eq!(tm.get_tasks_by_agent("a1").len(), 2);
    assert_eq!(tm.get_tasks_by_agent("missing").len(), 0);
}

// ---------- ReminderManager ----------

#[test]
fn test_reminder_manager_add_and_get() {
    let rm = ReminderManager::new();
    let id = rm.add_reminder(make_reminder(
        "rem-1",
        ReminderStatus::Active,
        ReminderInterval::Once,
    ));
    assert_eq!(id, "rem-1");
    assert!(rm.get_reminder("rem-1").is_some());
    assert!(rm.get_reminder("missing").is_none());
    assert_eq!(rm.get_all_reminders().len(), 1);
}

#[test]
fn test_reminder_manager_remove() {
    let rm = ReminderManager::new();
    rm.add_reminder(make_reminder(
        "rem-1",
        ReminderStatus::Active,
        ReminderInterval::Once,
    ));
    assert!(rm.remove_reminder("rem-1"));
    assert!(!rm.remove_reminder("rem-1"));
}

#[test]
fn test_reminder_manager_status_filters() {
    let rm = ReminderManager::new();
    rm.add_reminder(make_reminder(
        "active-1",
        ReminderStatus::Active,
        ReminderInterval::Recurring,
    ));
    rm.add_reminder(make_reminder(
        "active-2",
        ReminderStatus::Active,
        ReminderInterval::Once,
    ));
    rm.add_reminder(make_reminder(
        "snoozed-1",
        ReminderStatus::Snoozed,
        ReminderInterval::Once,
    ));

    assert_eq!(rm.get_active_reminders().len(), 2);
    assert_eq!(rm.get_snoozed_reminders().len(), 1);
}

#[test]
fn test_reminder_manager_by_author() {
    let rm = ReminderManager::new();
    let mut other = make_reminder("rem-2", ReminderStatus::Active, ReminderInterval::Once);
    other.author_id = "author-2".to_string();
    rm.add_reminder(make_reminder(
        "rem-1",
        ReminderStatus::Active,
        ReminderInterval::Once,
    ));
    rm.add_reminder(other);

    assert_eq!(rm.get_reminders_by_author("author-1").len(), 1);
    assert_eq!(rm.get_reminders_by_author("author-2").len(), 1);
    assert_eq!(rm.get_reminders_by_author("missing").len(), 0);
}

// ---------- MessageHandler ----------

#[test]
fn test_message_handler_add_and_get() {
    let mut mh = MessageHandler::new();
    let id = mh.add_message(make_message(
        "msg-1",
        MessageType::Message,
        "chan-1",
        "user-1",
    ));
    assert_eq!(id, "msg-1");
    assert!(mh.get_message("msg-1").is_some());
    assert!(mh.get_message("missing").is_none());
    assert_eq!(mh.get_all_messages().len(), 1);
}

#[test]
fn test_message_handler_remove() {
    let mut mh = MessageHandler::new();
    mh.add_message(make_message(
        "msg-1",
        MessageType::Message,
        "chan-1",
        "user-1",
    ));
    assert!(mh.remove_message("msg-1"));
    assert!(!mh.remove_message("msg-1"));
    assert_eq!(mh.get_all_messages().len(), 0);
}

#[test]
fn test_message_handler_unread() {
    let mut mh = MessageHandler::new();
    let mut read = make_message("read-1", MessageType::Message, "chan-1", "user-1");
    read.mark_read();
    mh.add_message(read);
    mh.add_message(make_message(
        "unread-1",
        MessageType::Message,
        "chan-1",
        "user-1",
    ));

    // fresh messages default to unread
    assert_eq!(mh.get_unread_messages().len(), 1);
}

#[test]
fn test_message_handler_by_channel_sender_and_type() {
    let mut mh = MessageHandler::new();
    mh.add_message(make_message("m1", MessageType::Message, "chan-a", "user-1"));
    mh.add_message(make_message("m2", MessageType::Task, "chan-a", "user-2"));
    mh.add_message(make_message("m3", MessageType::Message, "chan-b", "user-1"));

    assert_eq!(mh.get_messages_by_channel("chan-a").len(), 2);
    assert_eq!(mh.get_messages_by_sender("user-1").len(), 2);
    assert_eq!(mh.get_messages_by_type(MessageType::Task).len(), 1);
    assert_eq!(mh.get_messages_by_type(MessageType::Ping).len(), 0);
}
