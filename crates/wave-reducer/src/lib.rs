//! Pure planning reducer over authored waves, lint findings, rerun intents, and
//! compatibility-backed run inputs.

use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use wave_dark_factory::FindingSeverity;
use wave_dark_factory::LintFinding;
use wave_dark_factory::SkillCatalogIssue;
use wave_dark_factory::has_errors;
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
    ActiveRun,
    AlreadyCompleted,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueReadinessState {
    Ready,
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
    pub claimable: bool,
    pub reasons: Vec<WaveBlockerState>,
    pub primary_reason: Option<WaveBlockerState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueReadinessProjection {
    pub next_ready_wave_ids: Vec<u32>,
    pub next_ready_wave_id: Option<u32>,
    pub claimable_wave_ids: Vec<u32>,
    pub ready_wave_count: usize,
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
    pub active_run: usize,
    pub already_completed: usize,
    pub other: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct QueueBlockerWaves {
    pub dependency: Vec<WaveRef>,
    pub lint: Vec<WaveRef>,
    pub closure: Vec<WaveRef>,
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

pub fn reduce_planning_state(
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, CompatibilityRunInput>,
    rerun_wave_ids: &HashSet<u32>,
) -> PlanningReducerState {
    let mut findings_by_wave: HashMap<u32, usize> = HashMap::new();
    for finding in findings {
        if matches!(finding.severity, FindingSeverity::Error) {
            *findings_by_wave.entry(finding.wave_id).or_default() += 1;
        }
    }

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
        let blocked_by = planning_gate.blocking_reasons.clone();
        let blocker_state = classify_blockers(&blocked_by);
        let blockers = classify_blocker_flags(&blocker_state);
        let readiness = classify_wave_readiness(
            run_gate.completed,
            run_gate.actively_running,
            &planning_gate,
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

    let ready_waves = waves_state
        .iter()
        .filter(|wave| matches!(wave.readiness.state, QueueReadinessState::Ready))
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
    let queue_ready_reason = if !next_ready_wave_ids.is_empty() {
        "ready waves are available to claim".to_string()
    } else if active_waves > 0 {
        "active waves are still running".to_string()
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

        if !wave.ready
            && !matches!(wave.readiness.state, QueueReadinessState::Completed)
            && !matches!(wave.readiness.state, QueueReadinessState::Active)
        {
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
            active: queue_active,
            completed: queue_completed,
            blocked: queue_blocked,
            blocker_summary,
            blocker_waves,
            readiness: QueueReadinessProjection {
                next_ready_wave_ids,
                next_ready_wave_id,
                claimable_wave_ids,
                ready_wave_count: ready_waves,
                blocked_wave_count: blocked_waves,
                active_wave_count: active_waves,
                completed_wave_count: completed_waves,
                queue_ready: next_ready_wave_id.is_some() || active_waves > 0,
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
    planning_gate: &PlanningGateVerdict,
    blocker_state: &[WaveBlockerState],
) -> WaveReadinessState {
    let claimable = !completed
        && !actively_running
        && blocker_state.is_empty()
        && planning_gate.blocking_reasons.is_empty();
    let state = if completed {
        QueueReadinessState::Completed
    } else if actively_running {
        QueueReadinessState::Active
    } else if claimable {
        QueueReadinessState::Ready
    } else {
        QueueReadinessState::Blocked
    };
    let reasons = blocker_state.to_vec();
    let primary_reason = reasons.first().cloned();

    WaveReadinessState {
        state,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use wave_config::ExecutionMode;
    use wave_dark_factory::FindingSeverity;
    use wave_domain::ClosureDisposition;
    use wave_domain::GateDisposition;
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
}
