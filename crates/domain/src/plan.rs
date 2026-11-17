//! Plan validation (spec section 7.1 PUT /v1/requests/{id}/plan):
//! unique task keys, acyclic dependencies, valid repo/acceptance refs,
//! request-level required_capabilities and required_routes preserved.

use crate::error::DomainError;
use crate::ids::RequestId;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanTask {
    pub key: String,
    pub objective: String,
    #[serde(default)]
    pub deliverable: Option<String>,
    pub repo_ref: String,
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    #[serde(default)]
    pub required_routes: Vec<String>,
    pub acceptance_profile: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Optional fixed node assignment.
    #[serde(default)]
    pub fixed_node: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    pub request_id: RequestId,
    pub tasks: Vec<PlanTask>,
}

/// Validated plan, produced only by `validate`.
#[derive(Debug, Clone)]
pub struct ValidatedPlan {
    pub plan: Plan,
    /// Tasks in topological order (dependencies first).
    pub topo_order: Vec<String>,
}

/// Context needed to validate a plan against the originating request.
#[derive(Debug, Clone)]
pub struct PlanContext {
    /// Repositories registered at the center (logical id allowed).
    pub repos: HashSet<String>,
    /// Acceptance profiles allowed per repo: repo -> profiles.
    pub acceptance_profiles: HashMap<String, Vec<String>>,
    /// The submitting node id (must match request's planner binding).
    pub submitted_by_node: Option<String>,
    /// Bound planner node for the request, if any.
    pub planner_binding: Option<String>,
}

pub fn validate(
    plan: &Plan,
    request_required_capabilities: &[String],
    request_required_routes: &[String],
    ctx: &PlanContext,
) -> Result<ValidatedPlan, DomainError> {
    // 1. Non-empty task list.
    if plan.tasks.is_empty() {
        return Err(DomainError::PlanInvalid("plan has no tasks".into()));
    }

    // 2. Unique task keys.
    let mut seen = HashSet::new();
    for t in &plan.tasks {
        if t.key.trim().is_empty() {
            return Err(DomainError::PlanInvalid("task key empty".into()));
        }
        if !seen.insert(t.key.clone()) {
            return Err(DomainError::PlanInvalid(format!(
                "duplicate task key: {}",
                t.key
            )));
        }
    }

    // 3. Dependencies reference existing keys; no cycles (Kahn's algorithm).
    let keys: HashSet<&String> = plan.tasks.iter().map(|t| &t.key).collect();
    for t in &plan.tasks {
        for dep in &t.depends_on {
            if !keys.contains(dep) {
                return Err(DomainError::PlanInvalid(format!(
                    "task {} depends on unknown task {}",
                    t.key, dep
                )));
            }
        }
        if t.depends_on.contains(&t.key) {
            return Err(DomainError::PlanInvalid(format!(
                "task {} depends on itself",
                t.key
            )));
        }
    }
    let topo_order = topo_sort(&plan.tasks)?;

    // 4. Repo must be registered; acceptance profile allowed for that repo;
    //    request-level required capabilities/routes must be preserved on at
    //    least one task (they may not be silently dropped).
    for t in &plan.tasks {
        if !ctx.repos.contains(&t.repo_ref) {
            return Err(DomainError::PlanInvalid(format!(
                "unknown repo_ref: {}",
                t.repo_ref
            )));
        }
        let allowed = ctx
            .acceptance_profiles
            .get(&t.repo_ref)
            .map(|v| v.iter().any(|p| p == &t.acceptance_profile))
            .unwrap_or(false);
        if !allowed {
            return Err(DomainError::PlanInvalid(format!(
                "acceptance profile {} not allowed for repo {}",
                t.acceptance_profile, t.repo_ref
            )));
        }
    }
    for cap in request_required_capabilities {
        let preserved = plan
            .tasks
            .iter()
            .any(|t| t.required_capabilities.contains(cap));
        if !preserved {
            return Err(DomainError::PlanInvalid(format!(
                "request-level required capability dropped: {}",
                cap
            )));
        }
    }
    for route in request_required_routes {
        let preserved = plan.tasks.iter().any(|t| t.required_routes.contains(route));
        if !preserved {
            return Err(DomainError::PlanInvalid(format!(
                "request-level required route dropped: {}",
                route
            )));
        }
    }

    // 5. Planner binding: if a binding exists, the submitting node must match.
    if let (Some(bound), Some(submitted)) = (&ctx.planner_binding, &ctx.submitted_by_node) {
        if bound != submitted {
            return Err(DomainError::Forbidden(
                "plan submitted by node that does not hold planner binding".into(),
            ));
        }
    }

    Ok(ValidatedPlan {
        plan: plan.clone(),
        topo_order,
    })
}

fn topo_sort(tasks: &[PlanTask]) -> Result<Vec<String>, DomainError> {
    let mut indegree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    for t in tasks {
        indegree.entry(&t.key).or_insert(0);
        for dep in &t.depends_on {
            *indegree.entry(&t.key).or_insert(0) += 1;
            dependents.entry(dep.as_str()).or_default().push(&t.key);
        }
    }
    let mut queue: Vec<&str> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(k, _)| *k)
        .collect();
    queue.sort();
    let mut order = Vec::with_capacity(tasks.len());
    while let Some(k) = queue.pop() {
        order.push(k.to_string());
        for dep in dependents.get(k).cloned().unwrap_or_default() {
            let e = indegree.get_mut(dep).unwrap();
            *e -= 1;
            if *e == 0 {
                queue.push(dep);
            }
        }
    }
    if order.len() != tasks.len() {
        return Err(DomainError::PlanInvalid("dependency cycle detected".into()));
    }
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> PlanContext {
        PlanContext {
            repos: ["fixture".to_string()].into_iter().collect(),
            acceptance_profiles: [("fixture".to_string(), vec!["counter-v1".to_string()])]
                .into_iter()
                .collect(),
            submitted_by_node: Some("control".to_string()),
            planner_binding: Some("control".to_string()),
        }
    }

    fn task(key: &str, deps: &[&str]) -> PlanTask {
        PlanTask {
            key: key.to_string(),
            objective: format!("objective {key}"),
            deliverable: None,
            repo_ref: "fixture".to_string(),
            required_capabilities: vec!["fixture-exec".to_string()],
            required_routes: vec![],
            acceptance_profile: "counter-v1".to_string(),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            fixed_node: None,
        }
    }

    #[test]
    fn valid_single_task_plan_passes() {
        let plan = Plan {
            request_id: RequestId::new("req-1"),
            tasks: vec![task("fix-counter", &[])],
        };
        let v = validate(&plan, &["fixture-exec".to_string()], &[], &ctx()).unwrap();
        assert_eq!(v.topo_order, vec!["fix-counter"]);
    }

    #[test]
    fn duplicate_keys_rejected() {
        let plan = Plan {
            request_id: RequestId::new("req-1"),
            tasks: vec![task("a", &[]), task("a", &[])],
        };
        assert!(matches!(
            validate(&plan, &[], &[], &ctx()),
            Err(DomainError::PlanInvalid(msg)) if msg.contains("duplicate")
        ));
    }

    #[test]
    fn unknown_dependency_rejected() {
        let plan = Plan {
            request_id: RequestId::new("req-1"),
            tasks: vec![task("a", &["ghost"])],
        };
        assert!(matches!(
            validate(&plan, &[], &[], &ctx()),
            Err(DomainError::PlanInvalid(msg)) if msg.contains("unknown task")
        ));
    }

    #[test]
    fn cycle_rejected() {
        let plan = Plan {
            request_id: RequestId::new("req-1"),
            tasks: vec![task("a", &["b"]), task("b", &["a"])],
        };
        assert!(matches!(
            validate(&plan, &[], &[], &ctx()),
            Err(DomainError::PlanInvalid(msg)) if msg.contains("cycle")
        ));
    }

    #[test]
    fn self_dependency_rejected() {
        let plan = Plan {
            request_id: RequestId::new("req-1"),
            tasks: vec![task("a", &["a"])],
        };
        assert!(matches!(
            validate(&plan, &[], &[], &ctx()),
            Err(DomainError::PlanInvalid(msg)) if msg.contains("itself")
        ));
    }

    #[test]
    fn unknown_repo_rejected() {
        let mut t = task("a", &[]);
        t.repo_ref = "nope".into();
        let plan = Plan {
            request_id: RequestId::new("req-1"),
            tasks: vec![t],
        };
        assert!(matches!(
            validate(&plan, &[], &[], &ctx()),
            Err(DomainError::PlanInvalid(msg)) if msg.contains("unknown repo_ref")
        ));
    }

    #[test]
    fn disallowed_acceptance_profile_rejected() {
        let mut t = task("a", &[]);
        t.acceptance_profile = "rogue-profile".into();
        let plan = Plan {
            request_id: RequestId::new("req-1"),
            tasks: vec![t],
        };
        assert!(matches!(
            validate(&plan, &[], &[], &ctx()),
            Err(DomainError::PlanInvalid(msg)) if msg.contains("acceptance profile")
        ));
    }

    #[test]
    fn dropped_request_capability_rejected() {
        let mut t = task("a", &[]);
        t.required_capabilities = vec![]; // silently dropping fixture-exec
        let plan = Plan {
            request_id: RequestId::new("req-1"),
            tasks: vec![t],
        };
        assert!(matches!(
            validate(&plan, &["fixture-exec".to_string()], &[], &ctx()),
            Err(DomainError::PlanInvalid(msg)) if msg.contains("capability dropped")
        ));
    }

    #[test]
    fn dropped_request_route_rejected() {
        let plan = Plan {
            request_id: RequestId::new("req-1"),
            tasks: vec![task("a", &[])],
        };
        assert!(matches!(
            validate(&plan, &[], &["lab-service".to_string()], &ctx()),
            Err(DomainError::PlanInvalid(msg)) if msg.contains("route dropped")
        ));
    }

    #[test]
    fn wrong_planner_node_rejected() {
        let mut c = ctx();
        c.submitted_by_node = Some("worker-a".into());
        let plan = Plan {
            request_id: RequestId::new("req-1"),
            tasks: vec![task("a", &[])],
        };
        assert!(matches!(
            validate(&plan, &[], &[], &c),
            Err(DomainError::Forbidden(_))
        ));
    }

    #[test]
    fn topological_order_respects_dependencies() {
        let plan = Plan {
            request_id: RequestId::new("req-1"),
            tasks: vec![task("c", &["b"]), task("b", &["a"]), task("a", &[])],
        };
        let v = validate(&plan, &["fixture-exec".to_string()], &[], &ctx()).unwrap();
        let pos = |k: &str| v.topo_order.iter().position(|x| x == k).unwrap();
        assert!(pos("a") < pos("b") && pos("b") < pos("c"));
    }
}
