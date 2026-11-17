//! Scheduling rules for claim transactions (spec section 5.1):
//! node online, role match, repo mapping, remaining execution slots, all
//! capabilities present, required routes unexpired, dependencies passed.
//! Lease generation matching is also here.

use crate::state::{dependency_blocks_downstream, run_is_active, task_is_final, TaskState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeView {
    pub node_id: String,
    pub role: String,
    pub capabilities: Vec<String>,
    pub capacity: i64,
    /// Number of currently active runs held by this node.
    pub active_runs: i64,
    /// Seconds since last heartbeat, in center clock terms.
    pub seconds_since_heartbeat: i64,
    /// Repos the node can resolve: logical repo id list.
    pub repo_refs: Vec<String>,
    pub offline_after_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskView {
    pub task_id: String,
    pub state: TaskState,
    pub repo_ref: String,
    pub required_capabilities: Vec<String>,
    pub required_routes: Vec<String>,
    pub fixed_node: Option<String>,
    /// States of dependencies (all of them).
    pub dependency_states: Vec<TaskState>,
    /// True if this task already has a non-terminal run.
    pub has_active_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteView {
    /// Resource id the route leads to.
    pub resource_id: String,
    pub source_node: String,
    /// Seconds since the center received the observation.
    pub seconds_since_observation: i64,
    pub valid_seconds: i64,
}

/// Outcome of evaluating whether a node may claim a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimDecision {
    Eligible,
    Rejected(&'static str),
}

/// Pure claim check: is `task` claimable by `node`, given the routes the
/// node can currently use? Returns a machine-readable rejection reason
/// (used for `no_matching_online_node` explanations).
pub fn evaluate_claim(
    node: &NodeView,
    task: &TaskView,
    node_routes: &[RouteView],
) -> ClaimDecision {
    // Node must be online (heartbeat fresh).
    if node.seconds_since_heartbeat > node.offline_after_seconds {
        return ClaimDecision::Rejected("node_offline");
    }
    // Workers only execute; review claims are control-only and handled
    // elsewhere (role must be worker for execute claims).
    if node.role != "worker" {
        return ClaimDecision::Rejected("role_mismatch");
    }
    // Capacity: remaining execution slots.
    if node.active_runs >= node.capacity {
        return ClaimDecision::Rejected("no_free_slot");
    }
    // Task must still be queued.
    if task.state != TaskState::Queued {
        return ClaimDecision::Rejected("task_not_queued");
    }
    // Fixed node constraint.
    if let Some(fixed) = &task.fixed_node {
        if fixed != &node.node_id {
            return ClaimDecision::Rejected("fixed_node_mismatch");
        }
    }
    // Repo mapping.
    if !node.repo_refs.contains(&task.repo_ref) {
        return ClaimDecision::Rejected("repo_not_mapped");
    }
    // All capabilities required by the task must be held by the node.
    for cap in &task.required_capabilities {
        if !node.capabilities.contains(cap) {
            return ClaimDecision::Rejected(format!("missing_capability:{cap}").leak());
        }
    }
    // Required routes must exist, belong to this node, and be unexpired.
    for route in &task.required_routes {
        let found = node_routes.iter().find(|r| r.resource_id == *route);
        match found {
            None => return ClaimDecision::Rejected("route_missing"),
            Some(r) => {
                if r.source_node != node.node_id {
                    return ClaimDecision::Rejected("route_not_owned");
                }
                if r.seconds_since_observation > r.valid_seconds {
                    return ClaimDecision::Rejected("route_expired");
                }
            }
        }
    }
    // Dependencies must all have passed.
    for dep in &task.dependency_states {
        if dependency_blocks_downstream(*dep) {
            return ClaimDecision::Rejected("dependency_not_passed");
        }
    }
    // No alternative run while an old run's status is uncertain/active.
    if task.has_active_run {
        return ClaimDecision::Rejected("active_run_exists");
    }
    ClaimDecision::Eligible
}

/// Lease generation check: run events and renewals must match the current
/// lease generation and the node identity; expired holders get a conflict.
pub fn lease_write_allowed(
    run_node: &str,
    run_lease_generation: i64,
    writer_node: &str,
    writer_generation: i64,
    lease_expired: bool,
) -> Result<(), &'static str> {
    if run_node != writer_node {
        return Err("writer_is_not_lease_holder");
    }
    if run_lease_generation != writer_generation {
        return Err("lease_generation_mismatch");
    }
    if lease_expired {
        return Err("lease_expired");
    }
    Ok(())
}

/// Whether a task may be re-opened for claiming (retry): only when the old
/// run is terminal or explicitly closed, and never while uncertain.
pub fn retry_allowed(old_run_state: Option<crate::state::RunState>) -> bool {
    match old_run_state {
        None => true,
        Some(s) => {
            !run_is_active(s)
                && (task_is_final(TaskState::Failed) || true)
                && crate::state::run_is_terminal(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RunState;

    fn node(caps: &[&str]) -> NodeView {
        NodeView {
            node_id: "worker-a".into(),
            role: "worker".into(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            capacity: 1,
            active_runs: 0,
            seconds_since_heartbeat: 0,
            repo_refs: vec!["fixture".into()],
            offline_after_seconds: 15,
        }
    }

    fn task(caps: &[&str], routes: &[&str]) -> TaskView {
        TaskView {
            task_id: "task-1".into(),
            state: TaskState::Queued,
            repo_ref: "fixture".into(),
            required_capabilities: caps.iter().map(|s| s.to_string()).collect(),
            required_routes: routes.iter().map(|s| s.to_string()).collect(),
            fixed_node: None,
            dependency_states: vec![],
            has_active_run: false,
        }
    }

    fn route(res: &str, node: &str, age: i64) -> RouteView {
        RouteView {
            resource_id: res.into(),
            source_node: node.into(),
            seconds_since_observation: age,
            valid_seconds: 100,
        }
    }

    #[test]
    fn eligible_when_all_conditions_met() {
        let n = node(&["fixture-exec"]);
        let t = task(&["fixture-exec"], &[]);
        assert_eq!(evaluate_claim(&n, &t, &[]), ClaimDecision::Eligible);
    }

    #[test]
    fn offline_node_rejected() {
        let mut n = node(&["fixture-exec"]);
        n.seconds_since_heartbeat = 16;
        assert_eq!(
            evaluate_claim(&n, &task(&["fixture-exec"], &[]), &[]),
            ClaimDecision::Rejected("node_offline")
        );
    }

    #[test]
    fn control_role_cannot_execute_claim() {
        let mut n = node(&["fixture-exec"]);
        n.role = "control".into();
        assert_eq!(
            evaluate_claim(&n, &task(&["fixture-exec"], &[]), &[]),
            ClaimDecision::Rejected("role_mismatch")
        );
    }

    #[test]
    fn missing_capability_rejected() {
        let n = node(&[]);
        assert_eq!(
            evaluate_claim(&n, &task(&["fixture-special"], &[]), &[]),
            ClaimDecision::Rejected("missing_capability:fixture-special")
        );
    }

    #[test]
    fn unmapped_repo_rejected() {
        let n = node(&["fixture-exec"]);
        let mut t = task(&["fixture-exec"], &[]);
        t.repo_ref = "other".into();
        assert_eq!(
            evaluate_claim(&n, &t, &[]),
            ClaimDecision::Rejected("repo_not_mapped")
        );
    }

    #[test]
    fn no_free_slot_rejected() {
        let mut n = node(&["fixture-exec"]);
        n.active_runs = 1;
        assert_eq!(
            evaluate_claim(&n, &task(&["fixture-exec"], &[]), &[]),
            ClaimDecision::Rejected("no_free_slot")
        );
    }

    #[test]
    fn fixed_node_mismatch_rejected() {
        let n = node(&["fixture-exec"]);
        let mut t = task(&["fixture-exec"], &[]);
        t.fixed_node = Some("worker-b".into());
        assert_eq!(
            evaluate_claim(&n, &t, &[]),
            ClaimDecision::Rejected("fixed_node_mismatch")
        );
    }

    #[test]
    fn missing_route_rejected() {
        let n = node(&["fixture-exec"]);
        assert_eq!(
            evaluate_claim(&n, &task(&["fixture-exec"], &["lab-service"]), &[]),
            ClaimDecision::Rejected("route_missing")
        );
    }

    #[test]
    fn expired_route_rejected() {
        let n = node(&["fixture-exec"]);
        let routes = vec![route("lab-service", "worker-a", 200)];
        assert_eq!(
            evaluate_claim(&n, &task(&["fixture-exec"], &["lab-service"]), &routes),
            ClaimDecision::Rejected("route_expired")
        );
    }

    #[test]
    fn foreign_route_rejected() {
        let n = node(&["fixture-exec"]);
        let routes = vec![route("lab-service", "worker-b", 10)];
        assert_eq!(
            evaluate_claim(&n, &task(&["fixture-exec"], &["lab-service"]), &routes),
            ClaimDecision::Rejected("route_not_owned")
        );
    }

    #[test]
    fn failed_dependency_rejected() {
        let n = node(&["fixture-exec"]);
        let mut t = task(&["fixture-exec"], &[]);
        t.dependency_states = vec![TaskState::Failed];
        assert_eq!(
            evaluate_claim(&n, &t, &[]),
            ClaimDecision::Rejected("dependency_not_passed")
        );
    }

    #[test]
    fn active_run_blocks_reclaim() {
        let n = node(&["fixture-exec"]);
        let mut t = task(&["fixture-exec"], &[]);
        t.has_active_run = true;
        assert_eq!(
            evaluate_claim(&n, &t, &[]),
            ClaimDecision::Rejected("active_run_exists")
        );
    }

    #[test]
    fn lease_generation_mismatch_is_conflict() {
        assert!(lease_write_allowed("worker-a", 1, "worker-a", 1, false).is_ok());
        assert_eq!(
            lease_write_allowed("worker-a", 1, "worker-a", 2, false),
            Err("lease_generation_mismatch")
        );
        assert_eq!(
            lease_write_allowed("worker-a", 1, "worker-b", 1, false),
            Err("writer_is_not_lease_holder")
        );
        assert_eq!(
            lease_write_allowed("worker-a", 1, "worker-a", 1, true),
            Err("lease_expired")
        );
    }

    #[test]
    fn retry_blocked_while_old_run_active() {
        assert!(retry_allowed(None));
        assert!(retry_allowed(Some(RunState::Finished)));
        assert!(retry_allowed(Some(RunState::Failed)));
        assert!(retry_allowed(Some(RunState::Canceled)));
        assert!(!retry_allowed(Some(RunState::Uncertain)));
        assert!(!retry_allowed(Some(RunState::Running)));
        assert!(!retry_allowed(Some(RunState::Leased)));
    }
}
