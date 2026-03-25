//! Pure planning reducer over authored waves, lint findings, rerun intents, and
//! compatibility-backed run inputs.
//!
//! Compatibility run records remain explicit adapter inputs in this wave. The
//! reducer consumes typed gate verdicts and closure facts rather than
//! re-deriving queue/blocker semantics from raw run records inline.

use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use wave_dark_factory::FindingSeverity;
use wave_dark_factory::LintFinding;
use wave_dark_factory::SkillCatalogIssue;
use wave_dark_factory::has_errors;
use wave_domain::SchedulerBudget;
use wave_domain::SchedulerBudgetRecord;
use wave_domain::SchedulerEventPayload;
use wave_domain::SchedulerOwner;
use wave_domain::TaskLeaseRecord;
use wave_domain::TaskLeaseState;
use wave_domain::WaveClaimRecord;
use wave_domain::WaveExecutionPhase;
use wave_domain::WavePromotionRecord;
use wave_domain::WavePromotionState;
use wave_domain::WaveSchedulingRecord;
use wave_domain::WaveSchedulingState;
use wave_domain::WaveWorktreeRecord;
use wave_events::SchedulerEvent;
use wave_gates::CompatibilityRunFacts;
use wave_gates::CompatibilityRunInput;
use wave_gates::DependencyGateVerdict;
use wave_gates::PlanningGateVerdict;
use wave_gates::WaveClosureFacts;
use wave_gates::compatibility_run_facts;
use wave_gates::dependency_gate_verdict_for_wave;
use wave_gates::planning_gate_verdict;
use wave_gates::wave_closure_facts_with_run;
use wave_spec::WaveDocument;
use wave_trace::WaveRunStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveBlockerKind {
    Dependency,
    Lint,
    Closure,
    Ownership,
    LeaseExpired,
    Budget,
    ActiveRun,
    AlreadyCompleted,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueReadinessState {
    Ready,
    Claimed,
    Blocked,
    Active,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveBlockerState {
    pub kind: WaveBlockerKind,
    pub raw: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveReadinessState {
    pub state: QueueReadinessState,
    pub planning_ready: bool,
    pub claimable: bool,
    pub reasons: Vec<WaveBlockerState>,
    pub primary_reason: Option<WaveBlockerState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchedulerOwnerState {
    pub scheduler_id: String,
    pub scheduler_path: String,
    pub runtime: Option<String>,
    pub executor: Option<String>,
    pub session_id: Option<String>,
    pub process_id: Option<u32>,
    pub process_started_at_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveClaimStateView {
    pub claim_id: String,
    pub owner: SchedulerOwnerState,
    pub claimed_at_ms: u128,
    pub released_at_ms: Option<u128>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskLeaseStateView {
    pub lease_id: String,
    pub task_id: String,
    pub claim_id: Option<String>,
    pub owner: SchedulerOwnerState,
    pub state: TaskLeaseState,
    pub granted_at_ms: u128,
    pub heartbeat_at_ms: Option<u128>,
    pub expires_at_ms: Option<u128>,
    pub finished_at_ms: Option<u128>,
    pub stale: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchedulerBudgetState {
    pub max_active_wave_claims: Option<u32>,
    pub max_active_task_leases: Option<u32>,
    pub reserved_closure_task_leases: Option<u32>,
    pub active_wave_claims: usize,
    pub active_task_leases: usize,
    pub active_implementation_task_leases: usize,
    pub active_closure_task_leases: usize,
    pub closure_capacity_reserved: bool,
    pub preemption_enabled: bool,
    pub budget_blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveOwnershipState {
    pub claim: Option<WaveClaimStateView>,
    pub active_leases: Vec<TaskLeaseStateView>,
    pub stale_leases: Vec<TaskLeaseStateView>,
    pub contention_reasons: Vec<String>,
    pub blocked_by_owner: Option<SchedulerOwnerState>,
    pub budget: SchedulerBudgetState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveExecutionState {
    pub worktree: Option<WaveWorktreeRecord>,
    pub promotion: Option<WavePromotionRecord>,
    pub scheduling: Option<WaveSchedulingRecord>,
    pub merge_blocked: bool,
    pub closure_blocked_by_promotion: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueReadinessProjection {
    pub next_ready_wave_ids: Vec<u32>,
    pub next_ready_wave_id: Option<u32>,
    pub claimable_wave_ids: Vec<u32>,
    pub claimed_wave_ids: Vec<u32>,
    pub ready_wave_count: usize,
    pub claimed_wave_count: usize,
    pub blocked_wave_count: usize,
    pub active_wave_count: usize,
    pub completed_wave_count: usize,
    pub queue_ready: bool,
    pub queue_ready_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveRef {
    pub id: u32,
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct WaveBlockerFlags {
    pub dependency: bool,
    pub lint: bool,
    pub closure: bool,
    pub ownership: bool,
    pub lease_expired: bool,
    pub budget: bool,
    pub active_run: bool,
    pub already_completed: bool,
    pub other: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveAgentCounts {
    pub total: usize,
    pub implementation: usize,
    pub closure: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveLifecycleSummary {
    pub last_run_status: Option<WaveRunStatus>,
    pub actively_running: bool,
    pub completed: bool,
    pub rerun_requested: bool,
    pub latest_run: Option<WaveRunSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveRunSummary {
    pub run_id: String,
    pub status: WaveRunStatus,
    pub created_at_ms: u128,
    pub started_at_ms: Option<u128>,
    pub completed_at_ms: Option<u128>,
    pub completed_successfully: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WavePlanningState {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub depends_on: Vec<u32>,
    pub blocked_by: Vec<String>,
    pub blocker_state: Vec<WaveBlockerState>,
    pub blockers: WaveBlockerFlags,
    pub lint_errors: usize,
    pub ready: bool,
    pub ownership: WaveOwnershipState,
    pub execution: WaveExecutionState,
    pub agents: WaveAgentCounts,
    pub closure: WaveClosureFacts,
    pub lifecycle: WaveLifecycleSummary,
    pub dependency_gates: Vec<DependencyGateVerdict>,
    pub run_gate: CompatibilityRunFacts,
    pub planning_gate: PlanningGateVerdict,
    pub readiness: WaveReadinessState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningSummary {
    pub total_waves: usize,
    pub ready_waves: usize,
    pub blocked_waves: usize,
    pub active_waves: usize,
    pub completed_waves: usize,
    pub total_agents: usize,
    pub implementation_agents: usize,
    pub closure_agents: usize,
    pub waves_with_complete_closure: usize,
    pub waves_missing_closure: usize,
    pub total_missing_closure_agents: usize,
    pub lint_error_waves: usize,
    pub skill_catalog_issue_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillCatalogHealth {
    pub ok: bool,
    pub issue_count: usize,
    pub issues: Vec<SkillCatalogIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct QueueBlockerSummary {
    pub dependency: usize,
    pub lint: usize,
    pub closure: usize,
    pub ownership: usize,
    pub lease_expired: usize,
    pub budget: usize,
    pub active_run: usize,
    pub already_completed: usize,
    pub other: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct QueueBlockerWaves {
    pub dependency: Vec<WaveRef>,
    pub lint: Vec<WaveRef>,
    pub closure: Vec<WaveRef>,
    pub ownership: Vec<WaveRef>,
    pub lease_expired: Vec<WaveRef>,
    pub budget: Vec<WaveRef>,
    pub active_run: Vec<WaveRef>,
    pub already_completed: Vec<WaveRef>,
    pub other: Vec<WaveRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockedWaveProjection {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub depends_on: Vec<u32>,
    pub blocked_by: Vec<String>,
    pub blocker_state: Vec<WaveBlockerState>,
    pub lint_errors: usize,
    pub missing_closure_agents: Vec<String>,
    pub rerun_requested: bool,
    pub last_run_status: Option<WaveRunStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueProjection {
    pub ready: Vec<WaveRef>,
    pub claimed: Vec<WaveRef>,
    pub active: Vec<WaveRef>,
    pub completed: Vec<WaveRef>,
    pub blocked: Vec<BlockedWaveProjection>,
    pub blocker_summary: QueueBlockerSummary,
    pub blocker_waves: QueueBlockerWaves,
    pub readiness: QueueReadinessProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentCountProjection {
    pub total: usize,
    pub implementation: usize,
    pub closure: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveClosureGap {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub present_closure_agents: Vec<String>,
    pub missing_closure_agents: Vec<String>,
    pub implementation_agent_count: usize,
    pub closure_agent_count: usize,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClosureCoverageProjection {
    pub complete_wave_ids: Vec<u32>,
    pub missing_wave_ids: Vec<u32>,
    pub required_agents: usize,
    pub present_agents: usize,
    pub missing_required_agents: usize,
    pub waves: Vec<WaveClosureGap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningReducerState {
    pub summary: PlanningSummary,
    pub agent_counts: AgentCountProjection,
    pub closure_coverage: ClosureCoverageProjection,
    pub queue: QueueProjection,
    pub skill_catalog: SkillCatalogHealth,
    pub waves: Vec<WavePlanningState>,
    pub has_errors: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SchedulerAuthorityState {
    pub waves: HashMap<u32, SchedulerWaveState>,
    pub budget: Option<SchedulerBudgetRecord>,
    pub reference_time_ms: u128,
    pub active_wave_claims: usize,
    pub active_task_leases: usize,
    pub active_implementation_task_leases: usize,
    pub active_closure_task_leases: usize,
    pub waiting_closure_waves: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SchedulerWaveState {
    pub claim: Option<WaveClaimRecord>,
    pub active_leases: Vec<TaskLeaseRecord>,
    pub stale_leases: Vec<TaskLeaseRecord>,
    pub worktree: Option<WaveWorktreeRecord>,
    pub promotion: Option<WavePromotionRecord>,
    pub scheduling: Option<WaveSchedulingRecord>,
    pub contention_reasons: Vec<String>,
}

pub fn reduce_planning_state(
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, CompatibilityRunInput>,
    rerun_wave_ids: &HashSet<u32>,
) -> PlanningReducerState {
    reduce_planning_state_with_scheduler(
        waves,
        findings,
        skill_catalog_issues,
        latest_runs,
        rerun_wave_ids,
        &[],
    )
}

pub fn reduce_planning_state_with_scheduler(
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, CompatibilityRunInput>,
    rerun_wave_ids: &HashSet<u32>,
    scheduler_events: &[SchedulerEvent],
) -> PlanningReducerState {
    let mut findings_by_wave: HashMap<u32, usize> = HashMap::new();
    for finding in findings {
        if matches!(finding.severity, FindingSeverity::Error) {
            *findings_by_wave.entry(finding.wave_id).or_default() += 1;
        }
    }

    let scheduler_state = reduce_scheduler_authority(scheduler_events);
    let mut waves_state = Vec::new();
    for wave in waves {
        let latest_run = latest_runs.get(&wave.metadata.id);
        let lint_errors = findings_by_wave
            .get(&wave.metadata.id)
            .copied()
            .unwrap_or_default();
        let rerun_requested = rerun_wave_ids.contains(&wave.metadata.id);
        let run_gate = compatibility_run_facts(wave.metadata.id, latest_run, rerun_requested);
        let closure = wave_closure_facts_with_run(wave, latest_run);
        let dependency_gates = wave
            .metadata
            .depends_on
            .iter()
            .map(|dependency| {
                dependency_gate_verdict_for_wave(
                    wave.metadata.id,
                    *dependency,
                    latest_runs.get(dependency),
                )
            })
            .collect::<Vec<_>>();
        let planning_gate = planning_gate_verdict(
            wave.metadata.id,
            lint_errors,
            &dependency_gates,
            &closure,
            &run_gate,
        );
        let planning_ready = planning_gate.blocking_reasons.is_empty();
        let ownership =
            build_wave_ownership_state(&scheduler_state, wave.metadata.id, planning_ready);
        let execution = build_wave_execution_state(&scheduler_state, wave.metadata.id);
        let blocked_by = combined_blockers(
            &planning_gate.blocking_reasons,
            &ownership,
            &execution,
            planning_ready,
        );
        let blocker_state = classify_blockers(&blocked_by);
        let blockers = classify_blocker_flags(&blocker_state);
        let readiness = classify_wave_readiness(
            run_gate.completed,
            run_gate.actively_running,
            planning_ready,
            &ownership,
            &blocker_state,
        );

        waves_state.push(WavePlanningState {
            id: wave.metadata.id,
            slug: wave.metadata.slug.clone(),
            title: wave.metadata.title.clone(),
            depends_on: wave.metadata.depends_on.clone(),
            blocked_by,
            blocker_state,
            blockers,
            lint_errors,
            ready: readiness.claimable,
            ownership,
            execution,
            agents: WaveAgentCounts {
                total: wave.agents.len(),
                implementation: wave.implementation_agents().count(),
                closure: wave.closure_agents().count(),
            },
            closure,
            lifecycle: WaveLifecycleSummary {
                last_run_status: run_gate.latest_run.as_ref().map(|run| run.status),
                actively_running: run_gate.actively_running,
                completed: run_gate.completed,
                rerun_requested,
                latest_run: run_gate.latest_run.as_ref().map(run_summary),
            },
            dependency_gates,
            run_gate,
            planning_gate,
            readiness,
        });
    }

    let next_ready_wave_ids = waves_state
        .iter()
        .filter(|wave| matches!(wave.readiness.state, QueueReadinessState::Ready))
        .map(|wave| wave.id)
        .collect::<Vec<_>>();
    let next_ready_wave_id = next_ready_wave_ids.first().copied();
    let claimable_wave_ids = next_ready_wave_ids.clone();
    let claimed_wave_ids = waves_state
        .iter()
        .filter(|wave| matches!(wave.readiness.state, QueueReadinessState::Claimed))
        .map(|wave| wave.id)
        .collect::<Vec<_>>();

    let ready_waves = waves_state
        .iter()
        .filter(|wave| matches!(wave.readiness.state, QueueReadinessState::Ready))
        .count();
    let claimed_waves = waves_state
        .iter()
        .filter(|wave| matches!(wave.readiness.state, QueueReadinessState::Claimed))
        .count();
    let active_waves = waves_state
        .iter()
        .filter(|wave| matches!(wave.readiness.state, QueueReadinessState::Active))
        .count();
    let completed_waves = waves_state
        .iter()
        .filter(|wave| matches!(wave.readiness.state, QueueReadinessState::Completed))
        .count();
    let blocked_waves = waves_state
        .iter()
        .filter(|wave| matches!(wave.readiness.state, QueueReadinessState::Blocked))
        .count();
    let waves_missing_closure = waves_state
        .iter()
        .filter(|wave| !wave.closure.complete)
        .count();
    let total_missing_closure_agents = waves_state
        .iter()
        .map(|wave| wave.closure.missing_agent_ids.len())
        .sum();
    let ownership_blocked_waves = waves_state
        .iter()
        .filter(|wave| wave.blockers.ownership)
        .count();
    let budget_blocked_waves = waves_state
        .iter()
        .filter(|wave| wave.blockers.budget)
        .count();
    let stale_lease_waves = waves_state
        .iter()
        .filter(|wave| wave.blockers.lease_expired)
        .count();
    let queue_ready_reason = if !next_ready_wave_ids.is_empty() {
        "ready waves are available to claim".to_string()
    } else if budget_blocked_waves > 0 {
        "capacity is exhausted by scheduler budget".to_string()
    } else if claimed_waves > 0 {
        "waves are already claimed by scheduler authority".to_string()
    } else if active_waves > 0 {
        "active waves are still running".to_string()
    } else if stale_lease_waves > 0 {
        "stale scheduler leases require attention".to_string()
    } else if ownership_blocked_waves > 0 {
        "scheduler ownership prevents new claims".to_string()
    } else if blocked_waves > 0 {
        "all remaining waves are blocked".to_string()
    } else {
        "queue is empty".to_string()
    };

    let summary = PlanningSummary {
        total_waves: waves_state.len(),
        ready_waves,
        blocked_waves,
        active_waves,
        completed_waves,
        total_agents: waves_state.iter().map(|wave| wave.agents.total).sum(),
        implementation_agents: waves_state
            .iter()
            .map(|wave| wave.agents.implementation)
            .sum(),
        closure_agents: waves_state.iter().map(|wave| wave.agents.closure).sum(),
        waves_with_complete_closure: waves_state.len().saturating_sub(waves_missing_closure),
        waves_missing_closure,
        total_missing_closure_agents,
        lint_error_waves: waves_state
            .iter()
            .filter(|wave| wave.lint_errors > 0)
            .count(),
        skill_catalog_issue_count: skill_catalog_issues.len(),
    };

    let mut queue_ready = Vec::new();
    let mut queue_claimed = Vec::new();
    let mut queue_active = Vec::new();
    let mut queue_completed = Vec::new();
    let mut queue_blocked = Vec::new();
    let mut blocker_summary = QueueBlockerSummary::default();
    let mut blocker_waves = QueueBlockerWaves::default();
    let mut complete_wave_ids = Vec::new();
    let mut missing_wave_ids = Vec::new();
    let mut closure_gaps = Vec::new();
    let mut required_agents = 0;
    let mut present_agents = 0;

    for wave in &waves_state {
        let wave_ref = WaveRef {
            id: wave.id,
            slug: wave.slug.clone(),
            title: wave.title.clone(),
        };

        if wave.ready {
            queue_ready.push(wave_ref.clone());
        }
        if matches!(wave.readiness.state, QueueReadinessState::Claimed) {
            queue_claimed.push(wave_ref.clone());
        }
        if matches!(wave.readiness.state, QueueReadinessState::Active) {
            queue_active.push(wave_ref.clone());
        }
        if matches!(wave.readiness.state, QueueReadinessState::Completed) {
            queue_completed.push(wave_ref.clone());
        }

        required_agents += wave.closure.required_agent_ids.len();
        present_agents += wave.closure.present_agent_ids.len();
        if wave.closure.complete {
            complete_wave_ids.push(wave.id);
        } else {
            missing_wave_ids.push(wave.id);
            closure_gaps.push(WaveClosureGap {
                id: wave.id,
                slug: wave.slug.clone(),
                title: wave.title.clone(),
                present_closure_agents: wave.closure.present_agent_ids.clone(),
                missing_closure_agents: wave.closure.missing_agent_ids.clone(),
                implementation_agent_count: wave.agents.implementation,
                closure_agent_count: wave.agents.closure,
                blocked_by: wave.blocked_by.clone(),
            });
        }

        if !wave.ready {
            accumulate_blockers(&mut blocker_summary, &wave.blocked_by);
            accumulate_blocker_waves(&mut blocker_waves, &wave_ref, &wave.blockers);
        }

        if matches!(wave.readiness.state, QueueReadinessState::Blocked) {
            queue_blocked.push(BlockedWaveProjection {
                id: wave.id,
                slug: wave.slug.clone(),
                title: wave.title.clone(),
                depends_on: wave.depends_on.clone(),
                blocked_by: wave.blocked_by.clone(),
                blocker_state: wave.blocker_state.clone(),
                lint_errors: wave.lint_errors,
                missing_closure_agents: wave.closure.missing_agent_ids.clone(),
                rerun_requested: wave.lifecycle.rerun_requested,
                last_run_status: wave.lifecycle.last_run_status,
            });
        }
    }

    PlanningReducerState {
        summary: summary.clone(),
        agent_counts: AgentCountProjection {
            total: summary.total_agents,
            implementation: summary.implementation_agents,
            closure: summary.closure_agents,
        },
        closure_coverage: ClosureCoverageProjection {
            complete_wave_ids,
            missing_wave_ids,
            required_agents,
            present_agents,
            missing_required_agents: summary.total_missing_closure_agents,
            waves: closure_gaps,
        },
        queue: QueueProjection {
            ready: queue_ready,
            claimed: queue_claimed,
            active: queue_active,
            completed: queue_completed,
            blocked: queue_blocked,
            blocker_summary,
            blocker_waves,
            readiness: QueueReadinessProjection {
                next_ready_wave_ids,
                next_ready_wave_id,
                claimable_wave_ids,
                claimed_wave_ids,
                ready_wave_count: ready_waves,
                claimed_wave_count: claimed_waves,
                blocked_wave_count: blocked_waves,
                active_wave_count: active_waves,
                completed_wave_count: completed_waves,
                queue_ready: next_ready_wave_id.is_some() || claimed_waves > 0 || active_waves > 0,
                queue_ready_reason,
            },
        },
        skill_catalog: SkillCatalogHealth {
            ok: skill_catalog_issues.is_empty(),
            issue_count: skill_catalog_issues.len(),
            issues: skill_catalog_issues.to_vec(),
        },
        waves: waves_state,
        has_errors: has_errors(findings) || !skill_catalog_issues.is_empty(),
    }
}

fn classify_blockers(blocked_by: &[String]) -> Vec<WaveBlockerState> {
    blocked_by
        .iter()
        .map(|blocker| {
            if let Some(detail) = blocker.strip_prefix("wave:") {
                WaveBlockerState {
                    kind: WaveBlockerKind::Dependency,
                    raw: blocker.clone(),
                    detail: Some(detail.to_string()),
                }
            } else if blocker == "lint:error" {
                WaveBlockerState {
                    kind: WaveBlockerKind::Lint,
                    raw: blocker.clone(),
                    detail: None,
                }
            } else if let Some(detail) = blocker.strip_prefix("closure:") {
                WaveBlockerState {
                    kind: WaveBlockerKind::Closure,
                    raw: blocker.clone(),
                    detail: Some(detail.to_string()),
                }
            } else if let Some(detail) = blocker.strip_prefix("ownership:") {
                WaveBlockerState {
                    kind: WaveBlockerKind::Ownership,
                    raw: blocker.clone(),
                    detail: Some(detail.to_string()),
                }
            } else if let Some(detail) = blocker.strip_prefix("lease-expired:") {
                WaveBlockerState {
                    kind: WaveBlockerKind::LeaseExpired,
                    raw: blocker.clone(),
                    detail: Some(detail.to_string()),
                }
            } else if let Some(detail) = blocker.strip_prefix("budget:") {
                WaveBlockerState {
                    kind: WaveBlockerKind::Budget,
                    raw: blocker.clone(),
                    detail: Some(detail.to_string()),
                }
            } else if let Some(detail) = blocker.strip_prefix("active-run:") {
                WaveBlockerState {
                    kind: WaveBlockerKind::ActiveRun,
                    raw: blocker.clone(),
                    detail: Some(detail.to_string()),
                }
            } else if blocker == "already-completed" {
                WaveBlockerState {
                    kind: WaveBlockerKind::AlreadyCompleted,
                    raw: blocker.clone(),
                    detail: None,
                }
            } else {
                WaveBlockerState {
                    kind: WaveBlockerKind::Other,
                    raw: blocker.clone(),
                    detail: Some(blocker.clone()),
                }
            }
        })
        .collect()
}

fn classify_blocker_flags(blocker_state: &[WaveBlockerState]) -> WaveBlockerFlags {
    let mut flags = WaveBlockerFlags::default();
    for blocker in blocker_state {
        match blocker.kind {
            WaveBlockerKind::Dependency => flags.dependency = true,
            WaveBlockerKind::Lint => flags.lint = true,
            WaveBlockerKind::Closure => flags.closure = true,
            WaveBlockerKind::Ownership => flags.ownership = true,
            WaveBlockerKind::LeaseExpired => flags.lease_expired = true,
            WaveBlockerKind::Budget => flags.budget = true,
            WaveBlockerKind::ActiveRun => flags.active_run = true,
            WaveBlockerKind::AlreadyCompleted => flags.already_completed = true,
            WaveBlockerKind::Other => flags.other = true,
        }
    }
    flags
}

fn classify_wave_readiness(
    completed: bool,
    actively_running: bool,
    planning_ready: bool,
    ownership: &WaveOwnershipState,
    blocker_state: &[WaveBlockerState],
) -> WaveReadinessState {
    let claimed = ownership.claim.is_some();
    let active_by_lease = !ownership.active_leases.is_empty();
    let claimable = planning_ready
        && !completed
        && !actively_running
        && !active_by_lease
        && !claimed
        && ownership.stale_leases.is_empty()
        && ownership.contention_reasons.is_empty()
        && !ownership.budget.budget_blocked
        && blocker_state.is_empty();
    let state = if completed {
        QueueReadinessState::Completed
    } else if actively_running || active_by_lease {
        QueueReadinessState::Active
    } else if claimed {
        QueueReadinessState::Claimed
    } else if claimable {
        QueueReadinessState::Ready
    } else {
        QueueReadinessState::Blocked
    };
    let reasons = blocker_state.to_vec();
    let primary_reason = reasons.first().cloned();

    WaveReadinessState {
        state,
        planning_ready,
        claimable,
        reasons,
        primary_reason,
    }
}

fn run_summary(run: &CompatibilityRunInput) -> WaveRunSummary {
    WaveRunSummary {
        run_id: run.run_id.clone(),
        status: run.status,
        created_at_ms: run.created_at_ms,
        started_at_ms: run.started_at_ms,
        completed_at_ms: run.completed_at_ms,
        completed_successfully: run.completed_successfully,
    }
}

fn accumulate_blockers(summary: &mut QueueBlockerSummary, blocked_by: &[String]) {
    for blocker in blocked_by {
        if blocker.starts_with("wave:") {
            summary.dependency += 1;
        } else if blocker == "lint:error" {
            summary.lint += 1;
        } else if blocker.starts_with("closure:") {
            summary.closure += 1;
        } else if blocker.starts_with("ownership:") {
            summary.ownership += 1;
        } else if blocker.starts_with("lease-expired:") {
            summary.lease_expired += 1;
        } else if blocker.starts_with("budget:") {
            summary.budget += 1;
        } else if blocker.starts_with("active-run:") {
            summary.active_run += 1;
        } else if blocker == "already-completed" {
            summary.already_completed += 1;
        } else {
            summary.other += 1;
        }
    }
}

fn accumulate_blocker_waves(
    summary: &mut QueueBlockerWaves,
    wave: &WaveRef,
    flags: &WaveBlockerFlags,
) {
    if flags.dependency {
        summary.dependency.push(wave.clone());
    }
    if flags.lint {
        summary.lint.push(wave.clone());
    }
    if flags.closure {
        summary.closure.push(wave.clone());
    }
    if flags.ownership {
        summary.ownership.push(wave.clone());
    }
    if flags.lease_expired {
        summary.lease_expired.push(wave.clone());
    }
    if flags.budget {
        summary.budget.push(wave.clone());
    }
    if flags.active_run {
        summary.active_run.push(wave.clone());
    }
    if flags.already_completed {
        summary.already_completed.push(wave.clone());
    }
    if flags.other {
        summary.other.push(wave.clone());
    }
}

pub fn reduce_scheduler_authority(events: &[SchedulerEvent]) -> SchedulerAuthorityState {
    let mut claims_by_id = HashMap::new();
    let mut leases_by_id = HashMap::new();
    let mut waves: HashMap<u32, SchedulerWaveState> = HashMap::new();
    let mut budget = None;
    let mut reference_time_ms = 0;
    let mut sorted_events = events.to_vec();
    sorted_events.sort_by_key(|event| (event.created_at_ms, event.event_id.clone()));

    for event in sorted_events {
        reference_time_ms = reference_time_ms.max(event.created_at_ms);
        match event.payload {
            SchedulerEventPayload::WaveClaimUpdated { claim } => {
                waves.entry(claim.wave_id).or_default();
                claims_by_id.insert(claim.claim_id.clone(), claim);
            }
            SchedulerEventPayload::WaveWorktreeUpdated { worktree } => {
                let wave_id = worktree.wave_id;
                waves.entry(wave_id).or_default().worktree = Some(worktree);
            }
            SchedulerEventPayload::WavePromotionUpdated { promotion } => {
                let wave_id = promotion.wave_id;
                waves.entry(wave_id).or_default().promotion = Some(promotion);
            }
            SchedulerEventPayload::WaveSchedulingUpdated { scheduling } => {
                let wave_id = scheduling.wave_id;
                waves.entry(wave_id).or_default().scheduling = Some(scheduling);
            }
            SchedulerEventPayload::TaskLeaseUpdated { lease } => {
                waves.entry(lease.wave_id).or_default();
                leases_by_id.insert(lease.lease_id.clone(), lease);
            }
            SchedulerEventPayload::SchedulerBudgetUpdated { budget: record } => {
                budget = select_latest_budget(budget, record);
            }
            SchedulerEventPayload::None => {}
        }
    }

    for claim in claims_by_id.into_values() {
        let state = waves.entry(claim.wave_id).or_default();
        if claim.state.is_held() {
            if let Some(existing) = state.claim.as_ref() {
                if existing.claim_id != claim.claim_id {
                    state.contention_reasons.push(format!(
                        "multiple held claims detected: {} and {}",
                        existing.claim_id, claim.claim_id
                    ));
                    if is_newer_claim(&claim, existing) {
                        state.claim = Some(claim);
                    }
                }
            } else {
                state.claim = Some(claim);
            }
        }
    }

    for lease in leases_by_id.into_values() {
        let state = waves.entry(lease.wave_id).or_default();
        let stale = lease_is_stale(&lease, reference_time_ms);
        if lease.state.is_active() && !stale {
            state.active_leases.push(lease);
        } else if stale
            || matches!(
                lease.state,
                TaskLeaseState::Expired | TaskLeaseState::Revoked
            )
        {
            state.stale_leases.push(lease);
        }
    }

    for state in waves.values_mut() {
        state
            .active_leases
            .sort_by_key(|lease| (lease.granted_at_ms, lease.lease_id.as_str().to_string()));
        state.stale_leases.sort_by_key(|lease| {
            (
                lease
                    .finished_at_ms
                    .unwrap_or(lease.expires_at_ms.unwrap_or(lease.granted_at_ms)),
                lease.lease_id.as_str().to_string(),
            )
        });
        state.contention_reasons.sort();
        state.contention_reasons.dedup();
    }

    let active_wave_claims = waves.values().filter(|state| state.claim.is_some()).count();
    let active_implementation_task_leases = waves
        .values()
        .flat_map(|state| state.active_leases.iter())
        .filter(|lease| !task_id_is_closure(&lease.task_id))
        .count();
    let active_closure_task_leases = waves
        .values()
        .flat_map(|state| state.active_leases.iter())
        .filter(|lease| task_id_is_closure(&lease.task_id))
        .count();
    let active_task_leases = active_implementation_task_leases + active_closure_task_leases;
    let waiting_closure_waves = waves
        .values()
        .filter(|state| {
            state
                .scheduling
                .as_ref()
                .map(|record| {
                    matches!(record.phase, WaveExecutionPhase::Closure)
                        && matches!(
                            record.state,
                            WaveSchedulingState::Waiting
                                | WaveSchedulingState::Protected
                                | WaveSchedulingState::Preempted
                        )
                })
                .unwrap_or(false)
        })
        .count();

    SchedulerAuthorityState {
        waves,
        budget,
        reference_time_ms,
        active_wave_claims,
        active_task_leases,
        active_implementation_task_leases,
        active_closure_task_leases,
        waiting_closure_waves,
    }
}

fn build_wave_ownership_state(
    scheduler_state: &SchedulerAuthorityState,
    wave_id: u32,
    planning_ready: bool,
) -> WaveOwnershipState {
    let wave_state = scheduler_state.waves.get(&wave_id);
    let claim = wave_state.and_then(|state| state.claim.as_ref().map(convert_claim_view));
    let active_leases = wave_state
        .map(|state| {
            state
                .active_leases
                .iter()
                .map(|lease| convert_lease_view(lease, scheduler_state.reference_time_ms))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let stale_leases = wave_state
        .map(|state| {
            state
                .stale_leases
                .iter()
                .map(|lease| convert_lease_view(lease, scheduler_state.reference_time_ms))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let contention_reasons = wave_state
        .map(|state| state.contention_reasons.clone())
        .unwrap_or_default();
    let blocked_by_owner = claim.as_ref().map(|claim| claim.owner.clone());
    let budget = build_budget_state(scheduler_state, planning_ready, claim.is_some());

    WaveOwnershipState {
        claim,
        active_leases,
        stale_leases,
        contention_reasons,
        blocked_by_owner,
        budget,
    }
}

fn build_wave_execution_state(
    scheduler_state: &SchedulerAuthorityState,
    wave_id: u32,
) -> WaveExecutionState {
    let wave_state = scheduler_state.waves.get(&wave_id);
    let promotion = wave_state.and_then(|state| state.promotion.clone());
    let closure_blocked_by_promotion = promotion
        .as_ref()
        .map(|promotion| promotion.state.blocks_closure())
        .unwrap_or(false);
    let merge_blocked = promotion
        .as_ref()
        .map(|promotion| {
            matches!(
                promotion.state,
                WavePromotionState::Conflicted | WavePromotionState::Failed
            )
        })
        .unwrap_or(false);

    WaveExecutionState {
        worktree: wave_state.and_then(|state| state.worktree.clone()),
        promotion,
        scheduling: wave_state.and_then(|state| state.scheduling.clone()),
        merge_blocked,
        closure_blocked_by_promotion,
    }
}

fn build_budget_state(
    scheduler_state: &SchedulerAuthorityState,
    planning_ready: bool,
    already_claimed: bool,
) -> SchedulerBudgetState {
    let limits = scheduler_state
        .budget
        .as_ref()
        .map(|record| record.budget.clone())
        .unwrap_or_else(SchedulerBudget::default);
    let wave_claim_limit_hit = limits
        .max_active_wave_claims
        .map(|limit| scheduler_state.active_wave_claims >= limit as usize)
        .unwrap_or(false);
    let task_lease_limit_hit = limits
        .max_active_task_leases
        .map(|limit| scheduler_state.active_task_leases >= limit as usize)
        .unwrap_or(false);

    SchedulerBudgetState {
        max_active_wave_claims: limits.max_active_wave_claims,
        max_active_task_leases: limits.max_active_task_leases,
        reserved_closure_task_leases: limits.reserved_closure_task_leases,
        active_wave_claims: scheduler_state.active_wave_claims,
        active_task_leases: scheduler_state.active_task_leases,
        active_implementation_task_leases: scheduler_state.active_implementation_task_leases,
        active_closure_task_leases: scheduler_state.active_closure_task_leases,
        closure_capacity_reserved: limits
            .reserved_closure_task_leases
            .map(|reserved| {
                reserved > 0
                    && scheduler_state.waiting_closure_waves > 0
                    && scheduler_state.active_closure_task_leases < reserved as usize
            })
            .unwrap_or(false),
        preemption_enabled: limits.preemption_enabled,
        budget_blocked: planning_ready
            && !already_claimed
            && (wave_claim_limit_hit || task_lease_limit_hit),
    }
}

fn combined_blockers(
    planning_blockers: &[String],
    ownership: &WaveOwnershipState,
    execution: &WaveExecutionState,
    planning_ready: bool,
) -> Vec<String> {
    let mut blocked_by = planning_blockers.to_vec();

    if let Some(owner) = ownership.blocked_by_owner.as_ref() {
        blocked_by.push(format!("ownership:claimed-by:{}", owner.scheduler_path));
    }
    for reason in &ownership.contention_reasons {
        blocked_by.push(format!("ownership:contention:{reason}"));
    }
    for lease in &ownership.stale_leases {
        blocked_by.push(format!(
            "lease-expired:{}:{}",
            lease.task_id,
            lease.state_label()
        ));
    }
    if planning_ready && ownership.budget.budget_blocked {
        blocked_by.push(format!(
            "budget:wave-claims:{}/{}",
            ownership.budget.active_wave_claims,
            ownership
                .budget
                .max_active_wave_claims
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unbounded".to_string())
        ));
    }
    if execution.merge_blocked {
        blocked_by.push(format!(
            "closure:promotion-blocked:{}",
            execution
                .promotion
                .as_ref()
                .map(|promotion| promotion_state_label(promotion.state))
                .unwrap_or("unknown")
        ));
    }

    blocked_by
}

fn convert_claim_view(claim: &WaveClaimRecord) -> WaveClaimStateView {
    WaveClaimStateView {
        claim_id: claim.claim_id.as_str().to_string(),
        owner: convert_owner_state(&claim.owner),
        claimed_at_ms: claim.claimed_at_ms,
        released_at_ms: claim.released_at_ms,
        detail: claim.detail.clone(),
    }
}

fn convert_lease_view(lease: &TaskLeaseRecord, reference_time_ms: u128) -> TaskLeaseStateView {
    TaskLeaseStateView {
        lease_id: lease.lease_id.as_str().to_string(),
        task_id: lease.task_id.as_str().to_string(),
        claim_id: lease
            .claim_id
            .as_ref()
            .map(|claim_id| claim_id.as_str().to_string()),
        owner: convert_owner_state(&lease.owner),
        state: lease.state,
        granted_at_ms: lease.granted_at_ms,
        heartbeat_at_ms: lease.heartbeat_at_ms,
        expires_at_ms: lease.expires_at_ms,
        finished_at_ms: lease.finished_at_ms,
        stale: lease_is_stale(lease, reference_time_ms),
        detail: lease.detail.clone(),
    }
}

fn convert_owner_state(owner: &SchedulerOwner) -> SchedulerOwnerState {
    SchedulerOwnerState {
        scheduler_id: owner.scheduler_id.clone(),
        scheduler_path: owner.scheduler_path.clone(),
        runtime: owner.runtime.clone(),
        executor: owner.executor.clone(),
        session_id: owner.session_id.clone(),
        process_id: owner.process_id,
        process_started_at_ms: owner.process_started_at_ms,
    }
}

fn lease_is_stale(lease: &TaskLeaseRecord, reference_time_ms: u128) -> bool {
    matches!(
        lease.state,
        TaskLeaseState::Expired | TaskLeaseState::Revoked
    ) || lease
        .expires_at_ms
        .map(|expires_at_ms| expires_at_ms <= reference_time_ms)
        .unwrap_or(false)
}

fn task_id_is_closure(task_id: &wave_domain::TaskId) -> bool {
    task_id
        .as_str()
        .rsplit_once("agent-")
        .map(|(_, agent_id)| matches!(agent_id, "a0" | "a8" | "a9"))
        .unwrap_or(false)
}

fn promotion_state_label(state: WavePromotionState) -> &'static str {
    match state {
        WavePromotionState::NotStarted => "not_started",
        WavePromotionState::Pending => "pending",
        WavePromotionState::Ready => "ready",
        WavePromotionState::Conflicted => "conflicted",
        WavePromotionState::Failed => "failed",
    }
}

fn is_newer_claim(candidate: &WaveClaimRecord, current: &WaveClaimRecord) -> bool {
    (candidate.claimed_at_ms, candidate.claim_id.as_str())
        > (current.claimed_at_ms, current.claim_id.as_str())
}

fn select_latest_budget(
    current: Option<SchedulerBudgetRecord>,
    candidate: SchedulerBudgetRecord,
) -> Option<SchedulerBudgetRecord> {
    match current {
        Some(current)
            if (current.updated_at_ms, current.budget_id.as_str())
                >= (candidate.updated_at_ms, candidate.budget_id.as_str()) =>
        {
            Some(current)
        }
        _ => Some(candidate),
    }
}

trait LeaseStateLabel {
    fn state_label(&self) -> &'static str;
}

impl LeaseStateLabel for TaskLeaseStateView {
    fn state_label(&self) -> &'static str {
        match self.state {
            TaskLeaseState::Granted => "granted",
            TaskLeaseState::Released => "released",
            TaskLeaseState::Expired => "expired",
            TaskLeaseState::Revoked => "revoked",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use wave_config::ExecutionMode;
    use wave_dark_factory::FindingSeverity;
    use wave_domain::ClosureDisposition;
    use wave_domain::GateDisposition;
    use wave_domain::SchedulerBudget;
    use wave_domain::SchedulerBudgetId;
    use wave_domain::SchedulerBudgetRecord;
    use wave_domain::SchedulerEventPayload;
    use wave_domain::SchedulerOwner;
    use wave_domain::TaskLeaseId;
    use wave_domain::TaskLeaseRecord;
    use wave_domain::TaskLeaseState;
    use wave_domain::WaveClaimId;
    use wave_domain::WaveClaimRecord;
    use wave_domain::WaveClaimState;
    use wave_domain::WaveExecutionPhase;
    use wave_domain::WavePromotionState;
    use wave_domain::WaveSchedulerPriority;
    use wave_domain::WaveSchedulingState;
    use wave_domain::task_id_for_agent;
    use wave_events::SchedulerEvent;
    use wave_events::SchedulerEventKind;
    use wave_gates::REQUIRED_CLOSURE_AGENT_IDS;
    use wave_gates::compatibility_run_inputs_by_wave;
    use wave_spec::CompletionLevel;
    use wave_spec::ComponentPromotion;
    use wave_spec::Context7Defaults;
    use wave_spec::DeployEnvironment;
    use wave_spec::DocImpact;
    use wave_spec::DurabilityLevel;
    use wave_spec::ExitContract;
    use wave_spec::ProofLevel;
    use wave_spec::WaveAgent;
    use wave_spec::WaveMetadata;
    use wave_trace::WaveRunRecord;

    #[test]
    fn dependent_wave_becomes_ready_after_successful_dependency() {
        let waves = vec![test_wave(0, Vec::new()), test_wave(1, vec![0])];
        let findings = vec![LintFinding {
            wave_id: 0,
            severity: FindingSeverity::Warning,
            rule: "note",
            message: "noop".to_string(),
        }];

        let status_before = reduce(&waves, &findings, &[], HashMap::new(), HashSet::new());
        assert!(status_before.waves[0].ready);
        assert_eq!(
            status_before.waves[0].readiness.state,
            QueueReadinessState::Ready
        );
        assert!(status_before.waves[0].readiness.claimable);
        assert_eq!(status_before.queue.readiness.next_ready_wave_ids, vec![0]);
        assert_eq!(status_before.queue.readiness.next_ready_wave_id, Some(0));
        assert!(!status_before.waves[1].ready);
        assert_eq!(
            status_before.waves[1].readiness.state,
            QueueReadinessState::Blocked
        );
        assert!(!status_before.waves[1].readiness.claimable);
        assert_eq!(status_before.summary.ready_waves, 1);
        assert_eq!(status_before.summary.blocked_waves, 1);
        assert_eq!(status_before.queue.readiness.ready_wave_count, 1);
        assert_eq!(status_before.queue.readiness.blocked_wave_count, 1);

        let latest_runs = HashMap::from([(0, run_record(0, WaveRunStatus::Succeeded))]);
        let status_after = reduce(&waves, &[], &[], latest_runs, HashSet::new());

        assert!(status_after.waves[0].lifecycle.completed);
        assert!(status_after.waves[1].ready);
        assert_eq!(
            status_after.waves[1].readiness.state,
            QueueReadinessState::Ready
        );
        assert_eq!(status_after.summary.completed_waves, 1);
        assert_eq!(status_after.summary.ready_waves, 1);
    }

    #[test]
    fn running_wave_is_active_and_not_ready() {
        let waves = vec![test_wave(0, Vec::new())];
        let latest_runs = HashMap::from([(0, run_record(0, WaveRunStatus::Running))]);

        let status = reduce(&waves, &[], &[], latest_runs, HashSet::new());

        assert!(!status.waves[0].ready);
        assert_eq!(status.waves[0].readiness.state, QueueReadinessState::Active);
        assert!(!status.waves[0].readiness.claimable);
        assert_eq!(
            status.waves[0].blocked_by,
            vec!["active-run:running".to_string()]
        );
        assert_eq!(
            status.waves[0].planning_gate.blocking_reasons,
            status.waves[0].blocked_by
        );
        assert_eq!(status.summary.active_waves, 1);
        assert_eq!(status.summary.blocked_waves, 0);
        assert_eq!(status.queue.readiness.active_wave_count, 1);
        assert!(status.queue.readiness.queue_ready);
        assert_eq!(
            status.queue.readiness.queue_ready_reason,
            "active waves are still running"
        );
        assert_eq!(status.queue.readiness.next_ready_wave_id, None);
    }

    #[test]
    fn closure_gaps_and_skill_catalog_issues_are_surfaced() {
        let mut wave = test_wave(0, Vec::new());
        wave.agents.retain(|agent| agent.id != "A9");
        let skill_catalog_issues = vec![SkillCatalogIssue {
            path: "skills/missing/skill.json".to_string(),
            message: "missing manifest".to_string(),
        }];

        let status = reduce(
            &[wave],
            &[],
            &skill_catalog_issues,
            HashMap::new(),
            HashSet::new(),
        );

        assert!(!status.waves[0].ready);
        assert_eq!(
            status.waves[0].readiness.state,
            QueueReadinessState::Blocked
        );
        assert_eq!(
            status.waves[0].closure.present_agent_ids,
            vec!["A0".to_string(), "A8".to_string()]
        );
        assert_eq!(
            status.waves[0].closure.missing_agent_ids,
            vec!["A9".to_string()]
        );
        assert_eq!(
            status.waves[0].closure.completion_gate.disposition,
            GateDisposition::Blocked
        );
        assert_eq!(
            status.waves[0].blocked_by,
            vec!["closure:A9:missing".to_string()]
        );
        assert_eq!(
            status.waves[0].planning_gate.blocking_reasons,
            status.waves[0].blocked_by
        );
        assert_eq!(status.summary.waves_missing_closure, 1);
        assert_eq!(status.summary.total_missing_closure_agents, 1);
        assert_eq!(status.summary.skill_catalog_issue_count, 1);
        assert_eq!(status.skill_catalog.issue_count, 1);
        assert!(status.has_errors);
        assert_eq!(status.queue.readiness.next_ready_wave_id, None);
    }

    #[test]
    fn planning_reduction_surfaces_queue_and_closure_details() {
        let running_wave = test_wave(0, Vec::new());
        let mut blocked_wave = test_wave(1, vec![0]);
        blocked_wave.agents.retain(|agent| agent.id != "A9");
        let completed_wave = test_wave(2, Vec::new());
        let findings = vec![LintFinding {
            wave_id: 1,
            severity: FindingSeverity::Error,
            rule: "lint",
            message: "broken prompt".to_string(),
        }];
        let latest_runs = HashMap::from([
            (0, run_record(0, WaveRunStatus::Running)),
            (2, run_record(2, WaveRunStatus::Succeeded)),
        ]);

        let state = reduce(
            &[running_wave, blocked_wave, completed_wave],
            &findings,
            &[],
            latest_runs,
            HashSet::new(),
        );

        assert_eq!(state.queue.ready.len(), 0);
        assert_eq!(state.queue.active.len(), 1);
        assert_eq!(state.queue.active[0].id, 0);
        assert_eq!(state.queue.completed.len(), 1);
        assert_eq!(state.queue.completed[0].id, 2);
        assert_eq!(state.queue.blocked.len(), 1);
        assert_eq!(state.queue.blocked[0].id, 1);
        assert_eq!(state.queue.blocker_summary.dependency, 1);
        assert_eq!(state.queue.blocker_summary.lint, 1);
        assert_eq!(state.queue.blocker_summary.closure, 1);
        assert_eq!(state.queue.blocker_summary.active_run, 1);
        assert_eq!(state.queue.blocker_summary.already_completed, 1);
        assert_eq!(state.queue.blocked[0].blocker_state.len(), 3);
        assert_eq!(state.queue.blocker_waves.dependency.len(), 1);
        assert_eq!(state.queue.blocker_waves.dependency[0].id, 1);
        assert_eq!(state.queue.blocker_waves.lint.len(), 1);
        assert_eq!(state.queue.blocker_waves.lint[0].id, 1);
        assert_eq!(state.queue.blocker_waves.closure.len(), 1);
        assert_eq!(state.queue.blocker_waves.closure[0].id, 1);
        assert_eq!(state.queue.blocker_waves.active_run.len(), 1);
        assert_eq!(state.queue.blocker_waves.active_run[0].id, 0);
        assert_eq!(state.queue.blocker_waves.already_completed.len(), 1);
        assert_eq!(state.queue.blocker_waves.already_completed[0].id, 2);
        assert_eq!(state.queue.readiness.ready_wave_count, 0);
        assert_eq!(state.queue.readiness.next_ready_wave_id, None);
        assert_eq!(state.queue.readiness.blocked_wave_count, 1);
        assert_eq!(state.queue.readiness.active_wave_count, 1);
        assert_eq!(state.queue.readiness.completed_wave_count, 1);
        assert_eq!(state.closure_coverage.complete_wave_ids, vec![0, 2]);
        assert_eq!(state.closure_coverage.missing_wave_ids, vec![1]);
        assert_eq!(state.closure_coverage.required_agents, 9);
        assert_eq!(state.closure_coverage.present_agents, 8);
        assert_eq!(state.closure_coverage.missing_required_agents, 1);
        assert_eq!(state.closure_coverage.waves.len(), 1);
        assert_eq!(state.closure_coverage.waves[0].id, 1);
        assert_eq!(
            state.closure_coverage.waves[0].missing_closure_agents,
            vec!["A9".to_string()]
        );
        assert_eq!(state.waves.len(), 3);
        assert_eq!(state.waves[0].id, 0);
        assert_eq!(state.waves[0].agents.total, 4);
        assert!(state.waves[0].blockers.active_run);
        assert_eq!(state.waves[1].id, 1);
        assert_eq!(state.waves[1].agents.implementation, 1);
        assert_eq!(state.waves[1].agents.closure, 2);
        assert!(!state.waves[1].closure.complete);
        assert_eq!(state.waves[1].blocker_state.len(), 3);
        assert_eq!(
            state.waves[1].closure.missing_agent_ids,
            vec!["A9".to_string()]
        );
        assert_eq!(
            state.waves[1].planning_gate.blocking_reasons,
            state.waves[1].blocked_by
        );
        assert!(state.waves[1].blockers.dependency);
        assert!(state.waves[1].blockers.lint);
        assert!(state.waves[1].blockers.closure);
        assert_eq!(state.waves[2].id, 2);
        assert!(state.waves[2].blockers.already_completed);
    }

    #[test]
    fn rerun_requested_reopens_successful_wave_without_reblocking_dependents() {
        let waves = vec![test_wave(0, Vec::new()), test_wave(1, vec![0])];
        let latest_runs = HashMap::from([(0, run_record(0, WaveRunStatus::Succeeded))]);
        let rerun_wave_ids = HashSet::from([0]);

        let state = reduce(&waves, &[], &[], latest_runs, rerun_wave_ids);

        assert!(state.waves[0].ready);
        assert!(!state.waves[0].lifecycle.completed);
        assert_eq!(
            state.waves[0].run_gate.gate.disposition,
            GateDisposition::Blocked
        );
        assert_eq!(
            state.waves[0].run_gate.gate.blocking_reasons,
            vec!["rerun:requested".to_string()]
        );
        assert_eq!(
            state.waves[0].planning_gate.disposition,
            GateDisposition::Pass
        );
        assert!(state.waves[1].ready);
        assert!(state.waves[1].dependency_gates[0].satisfied);
        assert_eq!(state.queue.readiness.next_ready_wave_ids, vec![0, 1]);
    }

    #[test]
    fn dependency_gates_are_scoped_to_the_wave_being_reduced() {
        let waves = vec![test_wave(10, Vec::new()), test_wave(11, vec![10])];
        let latest_runs = HashMap::from([(10, run_record(10, WaveRunStatus::Running))]);

        let state = reduce(&waves, &[], &[], latest_runs, HashSet::new());

        assert_eq!(state.waves[1].dependency_gates.len(), 1);
        assert_eq!(state.waves[1].dependency_gates[0].dependency_wave_id, 10);
        assert_eq!(state.waves[1].dependency_gates[0].gate.wave_id, 11);
        assert_eq!(
            state.waves[1].dependency_gates[0].gate.gate_id.as_str(),
            "wave-11:dependency-on-10"
        );
    }

    #[test]
    fn dry_run_adapter_unblocks_dependents_without_marking_completion() {
        let waves = vec![test_wave(0, Vec::new()), test_wave(1, vec![0])];
        let latest_runs = HashMap::from([(0, run_record(0, WaveRunStatus::DryRun))]);

        let state = reduce(&waves, &[], &[], latest_runs, HashSet::new());

        assert!(state.waves[0].ready);
        assert_eq!(state.waves[0].readiness.state, QueueReadinessState::Ready);
        assert_eq!(
            state.waves[0].run_gate.gate.disposition,
            GateDisposition::Pass
        );
        assert!(state.waves[0].run_gate.gate.blocking_reasons.is_empty());
        assert!(!state.waves[0].lifecycle.completed);
        assert_eq!(
            state.waves[0].planning_gate.disposition,
            GateDisposition::Pass
        );
        assert!(state.waves[0].planning_gate.blocking_reasons.is_empty());
        assert!(state.waves[1].dependency_gates[0].satisfied);
        assert!(state.waves[1].ready);
        assert_eq!(state.queue.readiness.next_ready_wave_ids, vec![0, 1]);
    }

    #[test]
    fn failed_run_remains_claimable_but_exposes_failed_gate() {
        let waves = vec![test_wave(0, Vec::new())];
        let latest_runs = HashMap::from([(0, run_record(0, WaveRunStatus::Failed))]);

        let state = reduce(&waves, &[], &[], latest_runs, HashSet::new());

        assert!(state.waves[0].ready);
        assert_eq!(state.waves[0].readiness.state, QueueReadinessState::Ready);
        assert!(!state.waves[0].lifecycle.completed);
        assert_eq!(
            state.waves[0].run_gate.gate.disposition,
            GateDisposition::Failed
        );
        assert_eq!(
            state.waves[0].run_gate.gate.blocking_reasons,
            vec!["run:failed".to_string()]
        );
        assert_eq!(
            state.waves[0].planning_gate.disposition,
            GateDisposition::Pass
        );
        assert!(state.waves[0].planning_gate.blocking_reasons.is_empty());
    }

    #[test]
    fn reducer_carries_closure_progress_and_run_summary_from_compatibility_inputs() {
        let waves = vec![test_wave(0, Vec::new())];
        let latest_runs = HashMap::from([(
            0,
            run_record_with_agents(
                0,
                WaveRunStatus::Succeeded,
                vec![
                    closure_agent_run_record("A0", "[wave-gate]"),
                    closure_agent_run_record("A8", "[wave-integration]"),
                    closure_agent_run_record("A9", "[wave-doc-closure]"),
                ],
            ),
        )]);

        let state = reduce(&waves, &[], &[], latest_runs, HashSet::new());
        let wave = &state.waves[0];

        assert!(wave.closure.complete);
        assert!(wave.closure.closed);
        assert_eq!(wave.closure.closure.disposition, ClosureDisposition::Closed);
        assert_eq!(
            wave.closure.completion_gate.disposition,
            GateDisposition::Pass
        );
        assert!(wave.closure.missing_final_markers.is_empty());
        assert_eq!(
            wave.lifecycle
                .latest_run
                .as_ref()
                .map(|run| run.run_id.as_str()),
            Some("wave-0-succeeded")
        );
        assert_eq!(wave.planning_gate.disposition, GateDisposition::Blocked);
        assert_eq!(
            wave.planning_gate.blocking_reasons,
            vec!["already-completed".to_string()]
        );
    }

    #[test]
    fn scheduler_claim_classifies_wave_as_claimed_without_losing_planning_readiness() {
        let waves = vec![test_wave(0, Vec::new())];
        let scheduler_events = vec![claim_acquired_event(
            0,
            "claim-wave-0-a",
            "wave-0-run-a",
            10,
        )];

        let state = reduce_with_scheduler(
            &waves,
            &[],
            &[],
            HashMap::new(),
            HashSet::new(),
            scheduler_events,
        );

        let wave = &state.waves[0];
        assert!(wave.readiness.planning_ready);
        assert!(!wave.readiness.claimable);
        assert_eq!(wave.readiness.state, QueueReadinessState::Claimed);
        assert!(wave.ownership.claim.is_some());
        assert_eq!(
            wave.ownership.claim.as_ref().unwrap().owner.scheduler_path,
            "wave-runtime/codex"
        );
        assert_eq!(state.queue.claimed.len(), 1);
        assert_eq!(state.queue.claimed[0].id, 0);
        assert_eq!(state.queue.readiness.claimed_wave_ids, vec![0]);
        assert_eq!(state.queue.readiness.claimed_wave_count, 1);
    }

    #[test]
    fn scheduler_contention_and_budget_block_ready_waves() {
        let waves = vec![test_wave(0, Vec::new()), test_wave(1, Vec::new())];
        let scheduler_events = vec![
            budget_event(1, 1),
            claim_acquired_event(0, "claim-wave-0-a", "wave-0-run-a", 10),
            claim_acquired_event(0, "claim-wave-0-b", "wave-0-run-b", 11),
        ];

        let state = reduce_with_scheduler(
            &waves,
            &[],
            &[],
            HashMap::new(),
            HashSet::new(),
            scheduler_events,
        );

        let claimed_wave = &state.waves[0];
        assert_eq!(claimed_wave.readiness.state, QueueReadinessState::Claimed);
        assert!(claimed_wave.blockers.ownership);
        assert_eq!(claimed_wave.ownership.contention_reasons.len(), 1);

        let blocked_wave = &state.waves[1];
        assert_eq!(blocked_wave.readiness.state, QueueReadinessState::Blocked);
        assert!(blocked_wave.readiness.planning_ready);
        assert!(blocked_wave.blockers.budget);
        assert!(
            blocked_wave
                .blocked_by
                .iter()
                .any(|reason| reason.starts_with("budget:"))
        );
        assert_eq!(
            state.queue.readiness.queue_ready_reason,
            "capacity is exhausted by scheduler budget"
        );
    }

    #[test]
    fn active_and_stale_leases_are_visible_in_reducer_state() {
        let waves = vec![test_wave(0, Vec::new()), test_wave(1, Vec::new())];
        let scheduler_events = vec![
            claim_acquired_event(0, "claim-wave-0", "wave-0-run", 10),
            lease_event(
                0,
                "claim-wave-0",
                "wave-0-run",
                "A1",
                TaskLeaseState::Granted,
                11,
                None,
            ),
            claim_acquired_event(1, "claim-wave-1", "wave-1-run", 20),
            lease_event(
                1,
                "claim-wave-1",
                "wave-1-run",
                "A1",
                TaskLeaseState::Expired,
                21,
                Some(22),
            ),
            lease_event(
                1,
                "claim-wave-1",
                "wave-1-run",
                "A8",
                TaskLeaseState::Revoked,
                23,
                Some(24),
            ),
        ];

        let state = reduce_with_scheduler(
            &waves,
            &[],
            &[],
            HashMap::new(),
            HashSet::new(),
            scheduler_events,
        );

        let active_wave = &state.waves[0];
        assert_eq!(active_wave.readiness.state, QueueReadinessState::Active);
        assert_eq!(active_wave.ownership.active_leases.len(), 1);

        let stale_wave = &state.waves[1];
        assert_eq!(stale_wave.readiness.state, QueueReadinessState::Claimed);
        assert_eq!(stale_wave.ownership.stale_leases.len(), 2);
        assert!(stale_wave.blockers.lease_expired);
        assert!(
            stale_wave
                .blocked_by
                .iter()
                .any(|reason| reason.starts_with("lease-expired:wave-01:agent-a1"))
        );
    }

    #[test]
    fn promotion_conflict_and_reserved_closure_capacity_are_visible_in_reducer_state() {
        let waves = vec![test_wave(0, Vec::new()), test_wave(1, Vec::new())];
        let scheduler_events = vec![
            SchedulerEvent::new(
                "sched-budget-parallel",
                SchedulerEventKind::SchedulerBudgetUpdated,
            )
            .with_created_at_ms(1)
            .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated {
                budget: SchedulerBudgetRecord {
                    budget_id: SchedulerBudgetId::new("budget-parallel"),
                    budget: SchedulerBudget {
                        max_active_wave_claims: Some(2),
                        max_active_task_leases: Some(2),
                        reserved_closure_task_leases: Some(1),
                        preemption_enabled: true,
                    },
                    owner: scheduler_owner("budget-bootstrap"),
                    updated_at_ms: 1,
                    detail: Some("parallel budget".to_string()),
                },
            }),
            claim_acquired_event(0, "claim-wave-0", "wave-0-run", 10),
            lease_event(
                0,
                "claim-wave-0",
                "wave-0-run",
                "A1",
                TaskLeaseState::Granted,
                11,
                None,
            ),
            worktree_event(1, ".wave/state/worktrees/wave-01-run", 20),
            promotion_event(
                1,
                WavePromotionState::Conflicted,
                vec!["README.md".to_string()],
                21,
            ),
            scheduling_event(
                1,
                WaveExecutionPhase::Closure,
                WaveSchedulerPriority::Closure,
                WaveSchedulingState::Protected,
                true,
                false,
                22,
            ),
        ];

        let state = reduce_with_scheduler(
            &waves,
            &[],
            &[],
            HashMap::new(),
            HashSet::new(),
            scheduler_events,
        );

        let conflicted = state
            .waves
            .iter()
            .find(|wave| wave.id == 1)
            .expect("wave 1");
        assert!(conflicted.execution.merge_blocked);
        assert!(conflicted.execution.closure_blocked_by_promotion);
        assert_eq!(
            conflicted
                .execution
                .promotion
                .as_ref()
                .map(|promotion| promotion.state),
            Some(WavePromotionState::Conflicted)
        );
        assert!(
            conflicted
                .blocked_by
                .iter()
                .any(|reason| reason == "closure:promotion-blocked:conflicted")
        );
        assert!(
            conflicted
                .execution
                .scheduling
                .as_ref()
                .map(|record| record.protected_closure_capacity)
                .unwrap_or(false)
        );
        assert_eq!(
            conflicted.ownership.budget.reserved_closure_task_leases,
            Some(1)
        );
        assert!(conflicted.ownership.budget.preemption_enabled);
        assert!(conflicted.ownership.budget.closure_capacity_reserved);
    }

    fn reduce(
        waves: &[WaveDocument],
        findings: &[LintFinding],
        skill_catalog_issues: &[SkillCatalogIssue],
        latest_runs: HashMap<u32, WaveRunRecord>,
        rerun_wave_ids: HashSet<u32>,
    ) -> PlanningReducerState {
        let latest_runs = compatibility_run_inputs_by_wave(&latest_runs);
        reduce_planning_state(
            waves,
            findings,
            skill_catalog_issues,
            &latest_runs,
            &rerun_wave_ids,
        )
    }

    fn reduce_with_scheduler(
        waves: &[WaveDocument],
        findings: &[LintFinding],
        skill_catalog_issues: &[SkillCatalogIssue],
        latest_runs: HashMap<u32, WaveRunRecord>,
        rerun_wave_ids: HashSet<u32>,
        scheduler_events: Vec<SchedulerEvent>,
    ) -> PlanningReducerState {
        let latest_runs = compatibility_run_inputs_by_wave(&latest_runs);
        reduce_planning_state_with_scheduler(
            waves,
            findings,
            skill_catalog_issues,
            &latest_runs,
            &rerun_wave_ids,
            &scheduler_events,
        )
    }

    fn run_record(wave_id: u32, status: WaveRunStatus) -> WaveRunRecord {
        run_record_with_agents(wave_id, status, Vec::new())
    }

    fn run_record_with_agents(
        wave_id: u32,
        status: WaveRunStatus,
        agents: Vec<wave_trace::AgentRunRecord>,
    ) -> WaveRunRecord {
        WaveRunRecord {
            run_id: format!("wave-{wave_id}-{status}"),
            wave_id,
            slug: format!("wave-{wave_id}"),
            title: format!("Wave {wave_id}"),
            status,
            dry_run: false,
            bundle_dir: PathBuf::from(format!(".wave/state/build/specs/wave-{wave_id}")),
            trace_path: PathBuf::from(format!(".wave/traces/wave-{wave_id}.json")),
            codex_home: PathBuf::from(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(1),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: match status {
                WaveRunStatus::Running | WaveRunStatus::Planned => None,
                _ => Some(2),
            },
            agents,
            error: None,
        }
    }

    fn closure_agent_run_record(id: &str, marker: &str) -> wave_trace::AgentRunRecord {
        wave_trace::AgentRunRecord {
            id: id.to_string(),
            title: "Closure".to_string(),
            status: WaveRunStatus::Succeeded,
            prompt_path: PathBuf::from(".wave/state/build/specs/prompt.md"),
            last_message_path: PathBuf::from(".wave/state/runs/last-message.txt"),
            events_path: PathBuf::from(".wave/state/runs/events.jsonl"),
            stderr_path: PathBuf::from(".wave/state/runs/stderr.txt"),
            result_envelope_path: None,
            expected_markers: vec![marker.to_string()],
            observed_markers: vec![marker.to_string()],
            exit_code: Some(0),
            error: None,
        }
    }

    fn test_wave(id: u32, depends_on: Vec<u32>) -> WaveDocument {
        WaveDocument {
            path: PathBuf::from(format!("waves/{id:02}.md")),
            metadata: WaveMetadata {
                id,
                slug: format!("wave-{id}"),
                title: format!("Wave {id}"),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on,
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
            },
            heading_title: Some(format!("Wave {id}")),
            commit_message: Some("Feat: test".to_string()),
            component_promotions: vec![ComponentPromotion {
                component: "test".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some("Reducer state".to_string()),
            }),
            agents: vec![
                closure_agent("A0", "[wave-gate]"),
                closure_agent("A8", "[wave-integration]"),
                closure_agent("A9", "[wave-doc-closure]"),
                WaveAgent {
                    id: "A1".to_string(),
                    title: "Implementation".to_string(),
                    role_prompts: Vec::new(),
                    executor: BTreeMap::from([(
                        "profile".to_string(),
                        "implement-codex".to_string(),
                    )]),
                    context7: Some(Context7Defaults {
                        bundle: "rust-control-plane".to_string(),
                        query: Some("Reducer state and queue projections".to_string()),
                    }),
                    skills: vec!["wave-core".to_string()],
                    components: vec!["bootstrap".to_string()],
                    capabilities: vec!["queue".to_string()],
                    exit_contract: Some(ExitContract {
                        completion: CompletionLevel::Integrated,
                        durability: DurabilityLevel::Durable,
                        proof: ProofLevel::Integration,
                        doc_impact: DocImpact::Owned,
                    }),
                    deliverables: vec!["crates/wave-reducer/src/lib.rs".to_string()],
                    file_ownership: vec!["crates/wave-reducer/src/lib.rs".to_string()],
                    final_markers: vec![
                        "[wave-proof]".to_string(),
                        "[wave-doc-delta]".to_string(),
                        "[wave-component]".to_string(),
                    ],
                    prompt: [
                        "Primary goal:",
                        "- Land the reducer.",
                        "",
                        "Required context before coding:",
                        "- Read README.md.",
                        "",
                        "File ownership (only touch these paths):",
                        "- crates/wave-reducer/src/lib.rs",
                    ]
                    .join("\n"),
                },
            ],
        }
    }

    fn closure_agent(id: &str, marker: &str) -> WaveAgent {
        WaveAgent {
            id: id.to_string(),
            title: "Closure".to_string(),
            role_prompts: vec![
                match id {
                    "A0" => "docs/agents/wave-cont-qa-role.md",
                    "A8" => "docs/agents/wave-integration-role.md",
                    "A9" => "docs/agents/wave-documentation-role.md",
                    _ => "docs/agents/wave-cont-eval-role.md",
                }
                .to_string(),
            ],
            executor: BTreeMap::from([("profile".to_string(), "review-codex".to_string())]),
            context7: Some(Context7Defaults {
                bundle: "none".to_string(),
                query: Some("Repository docs remain canonical".to_string()),
            }),
            skills: Vec::new(),
            components: Vec::new(),
            capabilities: Vec::new(),
            exit_contract: None,
            deliverables: Vec::new(),
            file_ownership: vec![format!(".wave/reviews/{id}.md")],
            final_markers: vec![marker.to_string()],
            prompt: [
                "Primary goal:",
                "- Close the wave honestly.",
                "",
                "Required context before coding:",
                "- Read README.md.",
                "",
                "File ownership (only touch these paths):",
                &format!("- .wave/reviews/{id}.md"),
            ]
            .join("\n"),
        }
    }

    #[test]
    fn required_closure_agent_ids_stay_stable() {
        assert_eq!(REQUIRED_CLOSURE_AGENT_IDS, ["A0", "A8", "A9"]);
    }

    fn scheduler_owner(session_id: &str) -> SchedulerOwner {
        SchedulerOwner {
            scheduler_id: "wave-runtime".to_string(),
            scheduler_path: "wave-runtime/codex".to_string(),
            runtime: Some("codex".to_string()),
            executor: Some("codex".to_string()),
            session_id: Some(session_id.to_string()),
            process_id: None,
            process_started_at_ms: None,
        }
    }

    fn claim_acquired_event(
        wave_id: u32,
        claim_id: &str,
        session_id: &str,
        created_at_ms: u128,
    ) -> SchedulerEvent {
        let claim = WaveClaimRecord {
            claim_id: WaveClaimId::new(claim_id),
            wave_id,
            state: WaveClaimState::Held,
            owner: scheduler_owner(session_id),
            claimed_at_ms: created_at_ms,
            released_at_ms: None,
            detail: Some("claim acquired".to_string()),
        };
        SchedulerEvent::new(
            format!("sched-claim-{wave_id}-{claim_id}"),
            SchedulerEventKind::WaveClaimAcquired,
        )
        .with_wave_id(wave_id)
        .with_claim_id(claim.claim_id.clone())
        .with_created_at_ms(created_at_ms)
        .with_correlation_id(session_id)
        .with_payload(SchedulerEventPayload::WaveClaimUpdated { claim })
    }

    fn lease_event(
        wave_id: u32,
        claim_id: &str,
        session_id: &str,
        agent_id: &str,
        state: TaskLeaseState,
        granted_at_ms: u128,
        finished_at_ms: Option<u128>,
    ) -> SchedulerEvent {
        let task_id = task_id_for_agent(wave_id, agent_id);
        let lease = TaskLeaseRecord {
            lease_id: TaskLeaseId::new(format!("lease-wave-{wave_id}-{agent_id}")),
            wave_id,
            task_id: task_id.clone(),
            claim_id: Some(WaveClaimId::new(claim_id)),
            state,
            owner: scheduler_owner(session_id),
            granted_at_ms,
            heartbeat_at_ms: Some(granted_at_ms),
            expires_at_ms: finished_at_ms,
            finished_at_ms,
            detail: Some(format!("lease {}", lease_state_name(state))),
        };
        let kind = match state {
            TaskLeaseState::Granted => SchedulerEventKind::TaskLeaseGranted,
            TaskLeaseState::Released => SchedulerEventKind::TaskLeaseReleased,
            TaskLeaseState::Expired => SchedulerEventKind::TaskLeaseExpired,
            TaskLeaseState::Revoked => SchedulerEventKind::TaskLeaseRevoked,
        };
        SchedulerEvent::new(
            format!(
                "sched-lease-{wave_id}-{agent_id}-{}",
                lease_state_name(state)
            ),
            kind,
        )
        .with_wave_id(wave_id)
        .with_task_id(task_id)
        .with_claim_id(WaveClaimId::new(claim_id))
        .with_lease_id(lease.lease_id.clone())
        .with_created_at_ms(finished_at_ms.unwrap_or(granted_at_ms))
        .with_correlation_id(session_id)
        .with_payload(SchedulerEventPayload::TaskLeaseUpdated { lease })
    }

    fn budget_event(max_active_wave_claims: u32, updated_at_ms: u128) -> SchedulerEvent {
        let budget = SchedulerBudgetRecord {
            budget_id: SchedulerBudgetId::new("budget-default"),
            budget: SchedulerBudget {
                max_active_wave_claims: Some(max_active_wave_claims),
                max_active_task_leases: Some(1),
                reserved_closure_task_leases: Some(1),
                preemption_enabled: true,
            },
            owner: scheduler_owner("budget-bootstrap"),
            updated_at_ms,
            detail: Some("serial budget".to_string()),
        };
        SchedulerEvent::new(
            format!("sched-budget-{updated_at_ms}"),
            SchedulerEventKind::SchedulerBudgetUpdated,
        )
        .with_created_at_ms(updated_at_ms)
        .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated { budget })
    }

    fn lease_state_name(state: TaskLeaseState) -> &'static str {
        match state {
            TaskLeaseState::Granted => "granted",
            TaskLeaseState::Released => "released",
            TaskLeaseState::Expired => "expired",
            TaskLeaseState::Revoked => "revoked",
        }
    }

    fn worktree_event(wave_id: u32, path: &str, created_at_ms: u128) -> SchedulerEvent {
        SchedulerEvent::new(
            format!("sched-worktree-{wave_id}-{created_at_ms}"),
            SchedulerEventKind::WaveWorktreeUpdated,
        )
        .with_wave_id(wave_id)
        .with_created_at_ms(created_at_ms)
        .with_payload(SchedulerEventPayload::WaveWorktreeUpdated {
            worktree: wave_domain::WaveWorktreeRecord {
                worktree_id: wave_domain::WaveWorktreeId::new(format!(
                    "worktree-wave-{wave_id:02}"
                )),
                wave_id,
                state: wave_domain::WaveWorktreeState::Allocated,
                path: path.to_string(),
                base_ref: "main".to_string(),
                snapshot_ref: "snapshot".to_string(),
                branch_ref: None,
                shared_scope: wave_domain::WaveWorktreeScope::Wave,
                allocated_at_ms: created_at_ms,
                released_at_ms: None,
                detail: Some("shared wave worktree".to_string()),
            },
        })
    }

    fn promotion_event(
        wave_id: u32,
        state: WavePromotionState,
        conflict_paths: Vec<String>,
        created_at_ms: u128,
    ) -> SchedulerEvent {
        SchedulerEvent::new(
            format!("sched-promotion-{wave_id}-{created_at_ms}"),
            SchedulerEventKind::WavePromotionUpdated,
        )
        .with_wave_id(wave_id)
        .with_created_at_ms(created_at_ms)
        .with_payload(SchedulerEventPayload::WavePromotionUpdated {
            promotion: wave_domain::WavePromotionRecord {
                promotion_id: wave_domain::WavePromotionId::new(format!(
                    "promotion-wave-{wave_id:02}"
                )),
                wave_id,
                worktree_id: Some(wave_domain::WaveWorktreeId::new(format!(
                    "worktree-wave-{wave_id:02}"
                ))),
                state,
                target_ref: "main".to_string(),
                snapshot_ref: "snapshot".to_string(),
                candidate_ref: Some("candidate".to_string()),
                candidate_tree: Some("tree".to_string()),
                conflict_paths,
                checked_at_ms: created_at_ms,
                completed_at_ms: Some(created_at_ms),
                detail: Some("promotion visibility".to_string()),
            },
        })
    }

    fn scheduling_event(
        wave_id: u32,
        phase: WaveExecutionPhase,
        priority: WaveSchedulerPriority,
        state: WaveSchedulingState,
        protected_closure_capacity: bool,
        preemptible: bool,
        created_at_ms: u128,
    ) -> SchedulerEvent {
        SchedulerEvent::new(
            format!("sched-scheduling-{wave_id}-{created_at_ms}"),
            SchedulerEventKind::WaveSchedulingUpdated,
        )
        .with_wave_id(wave_id)
        .with_created_at_ms(created_at_ms)
        .with_payload(SchedulerEventPayload::WaveSchedulingUpdated {
            scheduling: wave_domain::WaveSchedulingRecord {
                wave_id,
                phase,
                priority,
                state,
                fairness_rank: 0,
                waiting_since_ms: Some(created_at_ms),
                protected_closure_capacity,
                preemptible,
                last_decision: Some("scheduler visibility".to_string()),
                updated_at_ms: created_at_ms,
            },
        })
    }
}
