//! Roles and the central authorization matrix (spec section 6).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Operator,
    Control,
    Worker,
}

/// Actions that subjects can perform against the center.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Create goals, view evidence, resolve uncertain runs, retry tasks.
    OperatorAction,
    /// Submit plans, submit review reports, register planner binding, ack
    /// messages addressed to control.
    WritePlan,
    SubmitReview,
    AckMessage,
    /// Claim work (execute or review), update own runs.
    ClaimRun,
    UpdateRun,
    /// Read/observe resources; update own route observations.
    ReadResource,
    UpdateObservation,
}

/// Central permission matrix. Returns true if `role` may perform `action`.
/// Enforced by the service layer; this is the single source of truth.
pub fn authorized(role: Role, action: Action) -> bool {
    use Action::*;
    use Role::*;
    match (role, action) {
        // Operator can create goals, query evidence, resolve uncertain runs,
        // retry tasks after closing old attempts.
        (Operator, OperatorAction)
        | (Operator, ReadResource) => true,
        // Control writes plans, reviews, bindings; acks own messages.
        (Control, WritePlan)
        | (Control, SubmitReview)
        | (Control, AckMessage)
        | (Control, ReadResource) => true,
        // Workers claim execution work, update their own runs, read
        // permitted resources, update their own observations.
        (Worker, ClaimRun)
        | (Worker, UpdateRun)
        | (Worker, ReadResource)
        | (Worker, UpdateObservation) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_can_create_and_resolve_but_not_claim() {
        assert!(authorized(Role::Operator, Action::OperatorAction));
        assert!(authorized(Role::Operator, Action::ReadResource));
        assert!(!authorized(Role::Operator, Action::ClaimRun));
        assert!(!authorized(Role::Operator, Action::WritePlan));
        assert!(!authorized(Role::Operator, Action::UpdateRun));
    }

    #[test]
    fn control_can_plan_and_review_but_not_claim_execute() {
        assert!(authorized(Role::Control, Action::WritePlan));
        assert!(authorized(Role::Control, Action::SubmitReview));
        assert!(authorized(Role::Control, Action::AckMessage));
        assert!(!authorized(Role::Control, Action::ClaimRun));
        assert!(!authorized(Role::Control, Action::UpdateRun));
    }

    #[test]
    fn worker_can_claim_and_update_own_runs_but_not_plan() {
        assert!(authorized(Role::Worker, Action::ClaimRun));
        assert!(authorized(Role::Worker, Action::UpdateRun));
        assert!(authorized(Role::Worker, Action::UpdateObservation));
        assert!(!authorized(Role::Worker, Action::WritePlan));
        assert!(!authorized(Role::Worker, Action::SubmitReview));
        assert!(!authorized(Role::Worker, Action::OperatorAction));
        assert!(!authorized(Role::Worker, Action::AckMessage));
    }
}
