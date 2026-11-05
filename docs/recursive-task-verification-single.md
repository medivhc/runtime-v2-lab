---
name: recursive-task-verification
description: >-
  Execute complex, ambiguous, or high-risk goals using adaptive recursive task decomposition,
  delegated subagents, independent evidence-based verification, adversarial gap discovery,
  and failure-driven replanning. Use when a user asks to implement software, fix an issue,
  analyze a website, research a topic, or complete a multi-step task where apparent success
  may conceal errors or missing requirements.
---

# Recursive Task Verification (RTV)

## Mission

Deliver the user's actual goal, not merely an apparently complete response. Prefer real evidence over agent self-assessment. Distinguish **implemented**, **verified against stated criteria**, and **fully established**; never claim the last without adequate evidence. The original user goal remains authoritative throughout decomposition.

## Operating contract

1. Preserve the user's original goal, explicit constraints, authorized scope, and success criteria in a durable task record. Never silently narrow the goal or change acceptance criteria to match the implementation.
2. Discover relevant context before acting: repositories, issue trackers, dependencies, environments, credentials *availability* (never ask to paste secrets into chat if a secure mechanism exists), permissions, tools, and upstream/downstream systems. Explicitly search for unknown unknowns through boundary and dependency review.
3. Choose the smallest *sufficient* plan, not the smallest possible tasks. Delegate bounded, testable work to subagents whenever the runtime permits; avoid unnecessary delegation for trivial tasks.
4. Separate implementation from verification. A worker's assertion, self-written tests, or confidence score is not independent proof.
5. After verification failure, classify the cause before acting: implementation defect, missing context/resource, flawed plan, incomplete requirements, inadequate test, integration mismatch, or insufficient model capability. Select repair, discovery, replan, alternative worker, or recursive decomposition accordingly.
6. All children passing does **not** imply the parent passed. Run parent-level integration and acceptance checks.
7. Report uncertainty and blockers accurately. Do not fabricate tool calls, subagents, evidence, citations, model identities, or successful checks.

## Phase 0 — Discover runtime capabilities

Inspect available subagent/delegation interfaces, tool access, available model selectors, persistence, and test execution. Do not assume a specific CLI, hooks API, model registry, or filesystem. Use the capability matrix in [内置 Agent routing](#agent-routing).

**Cross-model preference:** Prefer different model families/providers for Worker, Verifier, and Auditor when actually supported and permitted. Choose models based on suitability, not arbitrary difference. If unavailable, use separate isolated subagents, independent prompts, different evidence paths, and record the limitation. Never claim cross-model independence without confirming the actual selected model.

If subagents are unavailable, run the roles sequentially with explicit context separation where possible, and disclose reduced independence. A Skill alone cannot enforce hard process guarantees; external orchestration is required for strict gating.

## Phase 1 — Establish goal and verification plan

Create or update a task record using [内置 Task schema](#task-schema). Include:
- Original goal and constraints, exact source of authority, assumptions and unknowns.
- Scope boundaries and dependency discovery plan.
- Observable acceptance criteria and evidence methods; for open-ended tasks, claim-level evidence and coverage review.
- Explicit risks, irreversibility, authorization boundaries, and stop conditions.

Ask the user only for blocking decisions or permissions that cannot safely be inferred or discovered. Do not treat missing context as a failed implementation. Do not autonomously perform irreversible actions, expose credentials, or expand permissions beyond authorization.

## Phase 2 — Execute with adaptive decomposition

Start with a direct bounded attempt when appropriate. Decompose *before* execution if a task lacks a manageable execution boundary, has multiple independently testable components, or involves major integration risks. Otherwise defer decomposition until evidence shows it is useful.

For each task node:
1. Specify its objective, inputs, dependencies, deliverables, acceptance checks, and parent linkage.
2. Delegate to a Worker subagent where available; provide only necessary context and tools.
3. Collect artifacts and tool outputs; independently verify with a Verifier subagent or external checks.
4. For consequential, ambiguous, or high-complexity nodes, run an Auditor tasked with finding missing requirements, boundary conditions, counterexamples, and evidence gaps. The Auditor must receive the original goal and relevant source material, not only the Worker's summary.
5. If checks fail, diagnose and choose a targeted next action. Do not blindly repeat the same prompt or recursively split indefinitely.
6. Re-verify the parent after children are integrated.

Prefer objective checks: compile, test, reproduce, integration, protocol conformance, fresh environment, source citations, and direct observation. Keep validation artifacts outside Worker control when feasible; do not let a Worker silently rewrite acceptance criteria or delete failing tests.

## Phase 3 — Independent verification and adversarial audit

**Verifier:** Check each acceptance claim against primary evidence, run available tests, reproduce claims, identify untested paths, and record pass/fail/unknown. Explicitly distinguish tests that the Worker authored from independently specified tests.

**Auditor:** Independently reinterpret the original user goal, identify potentially omitted scope, search for counterexamples, challenge assumptions and test adequacy, and propose concrete checks. Avoid simply voting on the Worker's answer. Different model family/provider is preferred but not sufficient for independence.

For research/web analysis, build a claim-to-source table: statement, primary source, retrieval context/date when available, corroboration, contradictory evidence, confidence/limitations. Separate what a website claims from externally established facts. If browsing or primary sources are unavailable, report that limitation.

Verification is not absolute proof. Treat model agreement as weak evidence if agents share sources or assumptions. Never repeatedly generate until a fallible verifier happens to say PASS; require new evidence or a materially different test after failed attempts.

## Phase 4 — Control loop and stopping

Follow [内置 Control loop](#control-loop) for state transitions. Maintain separate states for execution and verification. The terminal states are:
- `VERIFIED`: acceptance evidence is adequate for the stated scope, with residual risks disclosed.
- `PARTIAL`: useful work exists, but acceptance is not established.
- `BLOCKED`: requires inaccessible context, authorization, resources, or user decision.
- `UNVERIFIED`: completed-looking output lacks sufficient independent evidence.

Use a finite attempt/replan/depth budget appropriate to the environment; if unspecified, default to at most 2 materially distinct repair attempts per failure class and at most 3 decomposition levels before escalation. These are operational safety defaults, not success guarantees. Repeated failure without new evidence must trigger escalation or termination. Preserve verified progress and reproducible failure records across iterations when persistence exists.

## Required final report

Report: (1) original goal and actual delivered artifacts; (2) what was verified, by what independent evidence; (3) what remains unverified or outside scope; (4) discovered dependencies, decisions, and blockers; (5) next recommended action. Explicitly identify if cross-model review was unavailable. Never mark success solely because a subagent or the supervisor said so.

## 内置规范索引

- [内置 Task schema](#task-schema): task-node record and acceptance evidence schema.
- [内置 Agent routing](#agent-routing): multi-model role selection and graceful degradation.
- [内置 Control loop](#control-loop): state machine, failure taxonomy, and recursive execution pseudocode.
- [内置 Examples](#examples): software repair and website research examples.


---

<a id="task-schema"></a>

# Task record schema

Use a durable file or equivalent state store when available. This is a **logical schema**, not a requirement to emit verbose JSON on every step.

```yaml
id: TASK-001
parent_id: null
original_goal: "..."
current_objective: "..."
constraints: []
assumptions: []
unknowns: []
dependencies: []
acceptance:
  - id: A1
    criterion: "Observable requirement"
    method: "Independent verification method"
    result: UNKNOWN # PASS | FAIL | UNKNOWN
    evidence: []
children: []
worker:
  agent: null
  model_confirmed: null
verifier:
  agent: null
  model_confirmed: null
auditor:
  agent: null
  model_confirmed: null
artifacts: []
execution_status: PENDING # PENDING | RUNNING | DELIVERED | FAILED
verification_status: UNVERIFIED # UNVERIFIED | PASS | FAIL | INCONCLUSIVE
terminal_status: null # VERIFIED | PARTIAL | BLOCKED | UNVERIFIED
attempts: 0
failure_class: null
next_action: null
```

Evidence entries should identify the exact command/source, environment, timestamp when relevant, output location, and what the evidence does **not** prove. Do not put passwords, access tokens, private keys, or raw secrets into task records.


---

<a id="agent-routing"></a>

# Agent routing and independence

## Capability discovery

At runtime, check: (a) whether subagents can be spawned, (b) whether a model or provider can be selected per subagent, (c) whether isolated context is supported, (d) what tools each role can use, and (e) whether evidence can be stored durably. Never infer model identity from an agent name.

## Routing priorities

1. Worker: best available model/tool combination for implementation, with repository or browser access as needed.
2. Verifier: **prefer a different model family/provider**, independent task framing, read-only access to original criteria, and ability to execute independent tests.
3. Auditor: **prefer a third model family/provider**, independent reading of original goal and primary sources, adversarial coverage review.
4. Planner/Controller: strong reasoning and access to durable state; may share a model with other roles but must not substitute its own opinion for evidence.

Use a distinct provider when practical, but do not sacrifice basic task competence merely for diversity. If only two models are available, diversify Worker and Verifier first; Auditor can reuse a model with separate context and evidence. If only one model is available, use isolated role prompts and distinct evidence methods. If no subagents exist, perform serial role passes and disclose the weaker isolation.

## Delegation envelope

Each subagent request must include: role, task node ID, precise goal, permitted tools/scope, original acceptance criteria, expected artifacts, evidence requirements, and explicit prohibition on self-certifying overall success. Verifier should not see the Worker's internal rationale unless needed to reproduce a failure; Auditor should first inspect the original goal independently before seeing the implementation.

## Limitations

Agent Skills describe behavior; they do not install providers, create missing tools, enforce model selection, or guarantee that a host executes a subagent. A true hard gate requires a host-level orchestrator or policy mechanism outside this Skill. Never claim to have run a distinct model unless the runtime confirms it.


---

<a id="control-loop"></a>

# Control loop

```text
run(node):
  preserve_original_goal_and_acceptance(node)
  discover_missing_context_and_dependencies(node)
  if authorization_or_critical_context_missing: return BLOCKED
  if task_not_safely_bounded: decompose(node)
  else: delegate_worker(node)
  gather_artifacts_and_external_evidence(node)
  verifier_result = independently_verify(node)
  if verifier_result is PASS and task_is_consequential:
      audit_result = independently_audit_goal_coverage(node)
  else:
      audit_result = NOT_RUN
  if verifier_result is PASS and audit_result has no blocking gaps:
      if node has children: verify_parent_integration(node)
      if parent_acceptance_passes: return VERIFIED
  classify_failure_or_unknown(node)
  if no_new_evidence_or_budget_exhausted: return PARTIAL/UNVERIFIED/BLOCKED
  choose repair / context discovery / alternative approach / new worker / decomposition
  persist decision and continue
```

## Failure taxonomy

- IMPLEMENTATION_DEFECT: repair and reproduce.
- MISSING_CONTEXT: discover repository, issue links, external dependencies, or ask for blocking input.
- MISSING_PERMISSION: request scoped authorization through secure host mechanisms.
- WRONG_PLAN: revisit assumptions and choose a different approach.
- REQUIREMENT_GAP: add missing acceptance coverage, without silently redefining original goal.
- INADEQUATE_VERIFICATION: obtain different independent evidence; do not just repeat the same judge.
- INTEGRATION_FAILURE: test parent-level behavior and interfaces.
- MODEL_CAPABILITY_LIMIT: change model, reduce scope of a child, or recursively decompose.
- ENVIRONMENT_BLOCKER: report reproducible limitations; do not pretend a tool ran.

## Safety and stopping

Do not run infinite loops. Default to 2 distinct repair attempts per failure class and 3 decomposition levels unless user/runtime policy says otherwise. Avoid retries without a meaningful change in evidence or approach. Record partial artifacts and exact blockers. Destructive or external side-effect operations require appropriate authorization. A passing check is scoped evidence, not universal correctness.


---

<a id="examples"></a>

# Examples

## Jira issue → merge request

1. Locate Jira issue and linked repositories; inspect related components, deployment topology, and cross-repository dependencies. Unknown links are investigation items, not assumptions.
2. Define acceptance based on bug reproduction, expected behavior, regression tests, and MR requirements.
3. Delegate bounded implementation to Worker; verify reproduction-before/fix-after, unit/integration tests, diff scope, and clean environment when possible.
4. Auditor independently checks missed upstream/downstream effects and hidden assumptions.
5. Replan on failures. Do not say an MR was submitted unless a real MR was created and its URL/identifier is available.

## Analyze a website

1. Define what is being analyzed (product positioning, pricing, claims, technical architecture, competitive evidence) and the desired depth.
2. Worker gathers primary pages and writes claims linked to sources.
3. Verifier independently checks a sample or all material claims against source text; marks unsupported claims.
4. Auditor independently reviews original question, missing pages, alternate explanations, counterexamples, and claims that cannot be established from the website alone.
5. Revisit only identified gaps, and distinguish source claims from externally verified facts. Report unavailable browsing or inaccessible pages.
