use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum DomainError {
    /// Illegal state transition (current state cannot move to target).
    #[error("illegal transition: {entity} {from} -> {to}")]
    IllegalTransition {
        entity: &'static str,
        from: String,
        to: String,
    },
    /// Referenced object does not exist.
    #[error("unknown entity: {0}")]
    UnknownEntity(String),
    /// Plan validation failure with a machine-readable reason.
    #[error("plan invalid: {0}")]
    PlanInvalid(String),
    /// Scheduling precondition failed.
    #[error("schedule rejected: {0}")]
    ScheduleRejected(String),
    /// Authorization denied for this subject on this action.
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// Generic malformed input.
    #[error("invalid input: {0}")]
    InvalidInput(String),
}
