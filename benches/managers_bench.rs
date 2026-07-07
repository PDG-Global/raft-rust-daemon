//! Criterion benchmarks for the daemon's manager layer.
//!
//! Covers the four in-memory managers that back hot paths in the daemon:
//! [`DaemonState`] insertion/lookup, [`AgentManager`] registration and
//! filtering, [`TaskManager`] add/claim/filter, [`MessageHandler`] add/query,
//! and [`ReminderManager`] add/filter. Each routine is parameterised over a
//! small and large collection size so regressions in the underlying `DashMap`
//! usage show up at scale.
//!
//! Run with: `cargo bench`.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::path::PathBuf;
use std::sync::Arc;

use raft_daemon::daemon::agent::AgentManager;
use raft_daemon::daemon::message::MessageHandler;
use raft_daemon::daemon::reminder::ReminderManager;
use raft_daemon::daemon::state::DaemonState;
use raft_daemon::daemon::task::TaskManager;
use raft_daemon::models::{
    Agent, Message, MessageType, Reminder, ReminderInterval, ResetMode, RuntimeConfig, Task,
};

/// Collection sizes the benchmarks are run at.
const SIZES: &[usize] = &[100, 1_000];

/// Build a fresh [`DaemonState`] rooted in a throwaway workspace path.
fn make_state() -> DaemonState {
    DaemonState::new(
        "srv_bench".into(),
        None,
        RuntimeConfig {
            model: "gpt-4o".into(),
            tools: Vec::new(),
            parameters: serde_json::json!({}),
        },
        "bench".into(),
        PathBuf::from("/tmp/raft-bench-workspace"),
    )
}

fn make_agent(i: usize) -> Agent {
    Agent::with_defaults(
        format!("agent_{i}"),
        format!("Agent {i}"),
        "bench agent".into(),
        "worker".into(),
        "rusty".into(),
        "comp_1".into(),
        "srv_bench".into(),
        vec!["chan_1".into()],
        ResetMode::Restart,
        format!("/tmp/raft-bench/agent_{i}"),
    )
}

fn make_task(i: usize) -> Task {
    let assigned_to = if i % 3 == 0 {
        format!("agent_{}", i % 10)
    } else {
        String::new()
    };
    Task::with_defaults(
        format!("task_{i}"),
        format!("Task {i}"),
        "bench task".into(),
        format!("chan_{}", i % 4),
        None,
        assigned_to,
    )
}

fn make_message(i: usize) -> Message {
    Message::with_defaults(
        format!("msg_{i}"),
        if i % 2 == 0 {
            MessageType::Task
        } else {
            MessageType::Message
        },
        format!("content {i}"),
        format!("chan_{}", i % 4),
        None,
        format!("sender_{}", i % 5),
        i64::try_from(i).unwrap_or(i64::MAX),
    )
}

fn make_reminder(i: usize) -> Reminder {
    Reminder::with_defaults(
        format!("rem_{i}"),
        format!("Reminder {i}"),
        60_000,
        if i % 2 == 0 {
            ReminderInterval::Recurring
        } else {
            ReminderInterval::Once
        },
        0,
        format!("msg_{}", i % 8),
        format!("author_{}", i % 3),
    )
}

/// Benchmark state manager insertion and point lookups.
fn bench_state(c: &mut Criterion) {
    let mut group = c.benchmark_group("state");
    for &size in SIZES {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            b.iter_batched(
                make_state,
                |state| {
                    for i in 0..size {
                        state.agents().insert(format!("agent_{i}"), make_agent(i));
                    }
                    for i in 0..size {
                        black_box(state.get_agent(&format!("agent_{i}")));
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// Benchmark agent manager registration plus filtered reads.
fn bench_agent_manager(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_manager");
    for &size in SIZES {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            b.iter_batched(
                || AgentManager::new(Arc::new(make_state())),
                |manager| {
                    for i in 0..size {
                        manager.add_agent(make_agent(i));
                    }
                    black_box(manager.get_all_agents());
                    black_box(manager.get_agents_by_server("srv_bench"));
                    black_box(manager.get_agents_by_computer("comp_1"));
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// Benchmark task manager add, claim-filter, and channel-filter reads.
fn bench_task_manager(c: &mut Criterion) {
    let mut group = c.benchmark_group("task_manager");
    for &size in SIZES {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            b.iter_batched(
                TaskManager::new,
                |manager| {
                    for i in 0..size {
                        manager.add_task(make_task(i));
                    }
                    black_box(manager.get_pending_tasks());
                    black_box(manager.get_claimed_tasks());
                    black_box(manager.get_tasks_by_channel("chan_0"));
                    black_box(manager.get_tasks_by_agent("agent_0"));
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// Benchmark message handler add and filtered reads.
fn bench_message_handler(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_handler");
    for &size in SIZES {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            b.iter_batched(
                MessageHandler::new,
                |mut handler| {
                    for i in 0..size {
                        handler.add_message(make_message(i));
                    }
                    black_box(handler.get_unread_messages());
                    black_box(handler.get_messages_by_channel("chan_0"));
                    black_box(handler.get_messages_by_sender("sender_0"));
                    black_box(handler.get_messages_by_type(MessageType::Task));
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// Benchmark reminder manager add and filtered reads.
fn bench_reminder_manager(c: &mut Criterion) {
    let mut group = c.benchmark_group("reminder_manager");
    for &size in SIZES {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            b.iter_batched(
                ReminderManager::new,
                |manager| {
                    for i in 0..size {
                        manager.add_reminder(make_reminder(i));
                    }
                    black_box(manager.get_active_reminders());
                    black_box(manager.get_reminders_by_author("author_0"));
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_state,
    bench_agent_manager,
    bench_task_manager,
    bench_message_handler,
    bench_reminder_manager,
);
criterion_main!(benches);
