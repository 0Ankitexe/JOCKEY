//! Typed conveniences pinned to the JSON Schema contracts in `shared/schemas`.
//!
//! The JSON Schema files remain the cross-language source of truth. Contract
//! tests guard this Rust representation against drift.

#![forbid(unsafe_code)]

pub mod compiler;

use serde::{Deserialize, Serialize};

/// Lifecycle states accepted by the `TaskStatusEvent` contract.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Assigned,
    Delivered,
    Running,
    Completed,
    Failed,
    Cancelled,
    Expired,
}

impl TaskState {
    /// Every task state, in lifecycle order.
    pub const ALL: [Self; 8] = [
        Self::Queued,
        Self::Assigned,
        Self::Delivered,
        Self::Running,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
        Self::Expired,
    ];

    /// Return the JSON representation fixed by the shared contract.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Assigned => "assigned",
            Self::Delivered => "delivered",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TaskState;

    #[test]
    fn task_states_serialize_to_the_contract_values() {
        let values = TaskState::ALL.map(TaskState::as_str);
        assert_eq!(
            values,
            [
                "queued",
                "assigned",
                "delivered",
                "running",
                "completed",
                "failed",
                "cancelled",
                "expired",
            ]
        );
    }

    #[test]
    fn unknown_task_state_is_rejected() {
        let result = serde_json::from_str::<TaskState>(r#""unknown""#);
        assert!(result.is_err());
    }
}
