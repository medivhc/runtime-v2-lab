//! State machines for Request/Task/Run/Message (spec section 5).
//! Every legal transition is explicit; everything else is rejected.

use crate::error::DomainError;
use serde::{Deserialize, Serialize};
use std::fmt;

// ---------- Request ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestState {
    Planning,
    Active,
    Blocked,
    Accepted,
    Rejected,
}

impl fmt::Display for RequestState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            RequestState::Planning => "planning",
            RequestState::Active => "active",
            RequestState::Blocked => "blocked",
            RequestState::Accepted => "accepted",
            RequestState::Rejected => "rejected",
        };
        f.write_str(s)
    }
}

/// Request transitions:
/// planning -> active (plan accepted) | blocked (plan failed, reason kept)
/// active -> accepted (all required tasks passed) | rejected
/// blocked is terminal until operator intervention; accepted/rejected terminal.
pub fn request_transition(from: RequestState, to: RequestState) -> Result<(), DomainError> {
    let ok = matches!(
        (from, to),
        (RequestState::Planning, RequestState::Active)
            | (RequestState::Planning, RequestState::Blocked)
            | (RequestState::Active, RequestState::Accepted)
            | (RequestState::Active, RequestState::Rejected)
    );
    if ok {
        Ok(())
    } else {
        Err(DomainError::IllegalTransition {
            entity: "request",
            from: from.to_string(),
            to: to.to_string(),
        })
    }
}

// ---------- Task ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    WaitingDependency,
    Queued,
    Running,
    AwaitingReview,
    Accepted,
    Rejected,
    Failed,
    Canceled,
    Uncertain,
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TaskState::WaitingDependency => "waiting_dependency",
            TaskState::Queued => "queued",
            TaskState::Running => "running",
            TaskState::AwaitingReview => "awaiting_review",
            TaskState::Accepted => "accepted",
            TaskState::Rejected => "rejected",
            TaskState::Failed => "failed",
            TaskState::Canceled => "canceled",
            TaskState::Uncertain => "uncertain",
        };
        f.write_str(s)
    }
}

/// Task transitions. Failure/cancel/uncertain keep an explicit reason at the
/// store layer; failed dependencies keep downstream blocked.
pub fn task_transition(from: TaskState, to: TaskState) -> Result<(), DomainError> {
    let ok = matches!(
        (from, to),
        (TaskState::WaitingDependency, TaskState::Queued)
            | (TaskState::WaitingDependency, TaskState::Canceled)
            | (TaskState::Queued, TaskState::Running)
            | (TaskState::Queued, TaskState::Canceled)
            | (TaskState::Queued, TaskState::Uncertain)
            | (TaskState::Running, TaskState::AwaitingReview)
            | (TaskState::Running, TaskState::Failed)
            | (TaskState::Running, TaskState::Uncertain)
            | (TaskState::Running, TaskState::Canceled)
            | (TaskState::AwaitingReview, TaskState::Accepted)
            | (TaskState::AwaitingReview, TaskState::Rejected)
            | (TaskState::AwaitingReview, TaskState::Failed)
            | (TaskState::AwaitingReview, TaskState::Uncertain)
            // Operator retry re-opens a task after the old run finished or
            // was closed (store layer verifies the old run is terminal).
            | (TaskState::Failed, TaskState::Queued)
            | (TaskState::Rejected, TaskState::Queued)
            | (TaskState::Uncertain, TaskState::Queued)
            | (TaskState::Canceled, TaskState::Queued)
    );
    if ok {
        Ok(())
    } else {
        Err(DomainError::IllegalTransition {
            entity: "task",
            from: from.to_string(),
            to: to.to_string(),
        })
    }
}

/// True if the task state is final (no further transitions allowed by
/// scheduling; only operator retry may reopen via explicit transition).
pub fn task_is_final(s: TaskState) -> bool {
    matches!(
        s,
        TaskState::Accepted | TaskState::Rejected | TaskState::Failed | TaskState::Canceled
    )
}

/// A dependency in this state blocks downstream tasks (spec: failed
/// dependencies keep downstream waiting; uncertain/queued also block).
pub fn dependency_blocks_downstream(s: TaskState) -> bool {
    !matches!(s, TaskState::Accepted)
}

// ---------- Run ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Leased,
    SessionBound,
    PromptAdmitted,
    Running,
    Finished,
    Failed,
    Uncertain,
    Canceled,
}

impl fmt::Display for RunState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            RunState::Leased => "leased",
            RunState::SessionBound => "session_bound",
            RunState::PromptAdmitted => "prompt_admitted",
            RunState::Running => "running",
            RunState::Finished => "finished",
            RunState::Failed => "failed",
            RunState::Uncertain => "uncertain",
            RunState::Canceled => "canceled",
        };
        f.write_str(s)
    }
}

/// Run transitions. `running` is only set when tool activity or a model run
/// is observed; short runs may skip straight to finished with completion
/// evidence. `uncertain` when the runner cannot confirm what happened.
pub fn run_transition(from: RunState, to: RunState) -> Result<(), DomainError> {
    let ok = matches!(
        (from, to),
        (RunState::Leased, RunState::SessionBound)
            | (RunState::Leased, RunState::Finished) // short run w/ evidence
            | (RunState::Leased, RunState::Failed)
            | (RunState::Leased, RunState::Uncertain)
            | (RunState::Leased, RunState::Canceled)
            | (RunState::SessionBound, RunState::PromptAdmitted)
            | (RunState::SessionBound, RunState::Running)
            | (RunState::SessionBound, RunState::Finished)
            | (RunState::SessionBound, RunState::Failed)
            | (RunState::SessionBound, RunState::Uncertain)
            | (RunState::SessionBound, RunState::Canceled)
            | (RunState::PromptAdmitted, RunState::Running)
            | (RunState::PromptAdmitted, RunState::Finished)
            | (RunState::PromptAdmitted, RunState::Failed)
            | (RunState::PromptAdmitted, RunState::Uncertain)
            | (RunState::PromptAdmitted, RunState::Canceled)
            | (RunState::Running, RunState::Finished)
            | (RunState::Running, RunState::Failed)
            | (RunState::Running, RunState::Uncertain)
            | (RunState::Running, RunState::Canceled)
            // Uncertain resolution: operator closes (canceled) or re-opens
            // verification; re-lease after recovery goes through recover path
            // which re-enters leased via the store (only center may re-grant).
            | (RunState::Uncertain, RunState::Canceled)
            | (RunState::Uncertain, RunState::Leased)
            | (RunState::Uncertain, RunState::Finished)
            | (RunState::Uncertain, RunState::Failed)
    );
    if ok {
        Ok(())
    } else {
        Err(DomainError::IllegalTransition {
            entity: "run",
            from: from.to_string(),
            to: to.to_string(),
        })
    }
}

/// Run states that count as active (block alternative run creation).
pub fn run_is_active(s: RunState) -> bool {
    matches!(
        s,
        RunState::Leased
            | RunState::SessionBound
            | RunState::PromptAdmitted
            | RunState::Running
            | RunState::Uncertain
    )
}

/// Terminal run states; retry may create a new attempt only after the
/// previous run is terminal or explicitly closed by the operator.
pub fn run_is_terminal(s: RunState) -> bool {
    matches!(
        s,
        RunState::Finished | RunState::Failed | RunState::Canceled
    )
}

// ---------- Message ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageState {
    Queued,
    Admitted,
    Delivered,
    Acknowledged,
}

impl fmt::Display for MessageState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            MessageState::Queued => "queued",
            MessageState::Admitted => "admitted",
            MessageState::Delivered => "delivered",
            MessageState::Acknowledged => "acknowledged",
        };
        f.write_str(s)
    }
}

/// Message transitions: queued -> admitted (V2 receive response) ->
/// delivered (found in target session message query) -> acknowledged
/// (session confirmed the result id).
pub fn message_transition(from: MessageState, to: MessageState) -> Result<(), DomainError> {
    let ok = matches!(
        (from, to),
        (MessageState::Queued, MessageState::Admitted)
            | (MessageState::Admitted, MessageState::Delivered)
            | (MessageState::Delivered, MessageState::Acknowledged)
            // Redelivery retries may re-mark delivered on an admitted msg.
            | (MessageState::Queued, MessageState::Delivered)
    );
    if ok {
        Ok(())
    } else {
        Err(DomainError::IllegalTransition {
            entity: "message",
            from: from.to_string(),
            to: to.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_happy_path() {
        request_transition(RequestState::Planning, RequestState::Active).unwrap();
        request_transition(RequestState::Active, RequestState::Accepted).unwrap();
    }

    #[test]
    fn request_blocked_keeps_reason_path() {
        request_transition(RequestState::Planning, RequestState::Blocked).unwrap();
    }

    #[test]
    fn request_illegal_transitions_rejected() {
        // Every illegal transition path must be rejected.
        let all = [
            RequestState::Planning,
            RequestState::Active,
            RequestState::Blocked,
            RequestState::Accepted,
            RequestState::Rejected,
        ];
        let legal = [
            (RequestState::Planning, RequestState::Active),
            (RequestState::Planning, RequestState::Blocked),
            (RequestState::Active, RequestState::Accepted),
            (RequestState::Active, RequestState::Rejected),
        ];
        for from in all {
            for to in all {
                if !legal.contains(&(from, to)) {
                    assert!(
                        request_transition(from, to).is_err(),
                        "expected {from:?}->{to:?} to be illegal"
                    );
                }
            }
        }
    }

    #[test]
    fn task_happy_path() {
        task_transition(TaskState::WaitingDependency, TaskState::Queued).unwrap();
        task_transition(TaskState::Queued, TaskState::Running).unwrap();
        task_transition(TaskState::Running, TaskState::AwaitingReview).unwrap();
        task_transition(TaskState::AwaitingReview, TaskState::Accepted).unwrap();
    }

    #[test]
    fn task_illegal_transitions_rejected() {
        let all = [
            TaskState::WaitingDependency,
            TaskState::Queued,
            TaskState::Running,
            TaskState::AwaitingReview,
            TaskState::Accepted,
            TaskState::Rejected,
            TaskState::Failed,
            TaskState::Canceled,
            TaskState::Uncertain,
        ];
        let legal = [
            (TaskState::WaitingDependency, TaskState::Queued),
            (TaskState::WaitingDependency, TaskState::Canceled),
            (TaskState::Queued, TaskState::Running),
            (TaskState::Queued, TaskState::Canceled),
            (TaskState::Queued, TaskState::Uncertain),
            (TaskState::Running, TaskState::AwaitingReview),
            (TaskState::Running, TaskState::Failed),
            (TaskState::Running, TaskState::Uncertain),
            (TaskState::Running, TaskState::Canceled),
            (TaskState::AwaitingReview, TaskState::Accepted),
            (TaskState::AwaitingReview, TaskState::Rejected),
            (TaskState::AwaitingReview, TaskState::Failed),
            (TaskState::AwaitingReview, TaskState::Uncertain),
            (TaskState::Failed, TaskState::Queued),
            (TaskState::Rejected, TaskState::Queued),
            (TaskState::Uncertain, TaskState::Queued),
            (TaskState::Canceled, TaskState::Queued),
        ];
        for from in all {
            for to in all {
                if !legal.contains(&(from, to)) {
                    assert!(
                        task_transition(from, to).is_err(),
                        "expected {from:?}->{to:?} to be illegal"
                    );
                }
            }
        }
    }

    #[test]
    fn run_illegal_transitions_rejected() {
        let all = [
            RunState::Leased,
            RunState::SessionBound,
            RunState::PromptAdmitted,
            RunState::Running,
            RunState::Finished,
            RunState::Failed,
            RunState::Uncertain,
            RunState::Canceled,
        ];
        let legal = [
            (RunState::Leased, RunState::SessionBound),
            (RunState::Leased, RunState::Finished),
            (RunState::Leased, RunState::Failed),
            (RunState::Leased, RunState::Uncertain),
            (RunState::Leased, RunState::Canceled),
            (RunState::SessionBound, RunState::PromptAdmitted),
            (RunState::SessionBound, RunState::Running),
            (RunState::SessionBound, RunState::Finished),
            (RunState::SessionBound, RunState::Failed),
            (RunState::SessionBound, RunState::Uncertain),
            (RunState::SessionBound, RunState::Canceled),
            (RunState::PromptAdmitted, RunState::Running),
            (RunState::PromptAdmitted, RunState::Finished),
            (RunState::PromptAdmitted, RunState::Failed),
            (RunState::PromptAdmitted, RunState::Uncertain),
            (RunState::PromptAdmitted, RunState::Canceled),
            (RunState::Running, RunState::Finished),
            (RunState::Running, RunState::Failed),
            (RunState::Running, RunState::Uncertain),
            (RunState::Running, RunState::Canceled),
            (RunState::Uncertain, RunState::Canceled),
            (RunState::Uncertain, RunState::Leased),
            (RunState::Uncertain, RunState::Finished),
            (RunState::Uncertain, RunState::Failed),
        ];
        for from in all {
            for to in all {
                if !legal.contains(&(from, to)) {
                    assert!(
                        run_transition(from, to).is_err(),
                        "expected {from:?}->{to:?} to be illegal"
                    );
                }
            }
        }
    }

    #[test]
    fn message_illegal_transitions_rejected() {
        let all = [
            MessageState::Queued,
            MessageState::Admitted,
            MessageState::Delivered,
            MessageState::Acknowledged,
        ];
        let legal = [
            (MessageState::Queued, MessageState::Admitted),
            (MessageState::Admitted, MessageState::Delivered),
            (MessageState::Delivered, MessageState::Acknowledged),
            (MessageState::Queued, MessageState::Delivered),
        ];
        for from in all {
            for to in all {
                if !legal.contains(&(from, to)) {
                    assert!(
                        message_transition(from, to).is_err(),
                        "expected {from:?}->{to:?} to be illegal"
                    );
                }
            }
        }
    }

    #[test]
    fn failed_dependency_blocks_downstream_but_accepted_does_not() {
        assert!(dependency_blocks_downstream(TaskState::Failed));
        assert!(dependency_blocks_downstream(TaskState::Uncertain));
        assert!(dependency_blocks_downstream(TaskState::Queued));
        assert!(!dependency_blocks_downstream(TaskState::Accepted));
    }

    #[test]
    fn uncertain_run_blocks_alternative_run_creation() {
        assert!(run_is_active(RunState::Uncertain));
        assert!(run_is_active(RunState::Leased));
        assert!(!run_is_active(RunState::Finished));
        assert!(!run_is_active(RunState::Canceled));
    }
}
