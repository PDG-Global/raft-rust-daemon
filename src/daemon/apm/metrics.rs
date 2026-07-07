//! APM metrics module.

/// Agent performance metrics.
#[derive(Debug, Clone)]
pub struct AgentMetrics {
    /// Total requests handled.
    pub total_requests: u64,
    /// Total tokens used.
    pub total_tokens: u64,
    /// Average response time in milliseconds.
    pub avg_response_time_ms: f64,
    /// Total errors.
    pub total_errors: u64,
}

impl Default for AgentMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            total_tokens: 0,
            avg_response_time_ms: 0.0,
            total_errors: 0,
        }
    }
}
