//! Reducer-backed human-facing read models and status helpers for planning,
//! queue/control status, and operator snapshot surfaces.

mod authority;

use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use wave_config::ExecutionMode;
use wave_config::ProjectConfig;
use wave_dark_factory::LintFinding;
use wave_dark_factory::SkillCatalogIssue;
use wave_events::SchedulerEvent;
use wave_gates::CompatibilityRunInput;
use wave_gates::REQUIRED_CLOSURE_AGENT_IDS;
use wave_gates::compatibility_run_inputs_by_wave;
use wave_reducer::PlanningReducerState;
use wave_reducer::reduce_planning_state_with_scheduler;
use wave_spec::WaveDocument;
use wave_trace::WaveRunRecord;
use wave_trace::WaveRunStatus;

pub use authority::load_canonical_compatibility_runs;
pub use authority::load_scheduler_events;

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

pub type SchedulerOwnerState = wave_reducer::SchedulerOwnerState;
pub type WaveClaimStateView = wave_reducer::WaveClaimStateView;
pub type TaskLeaseStateView = wave_reducer::TaskLeaseStateView;
pub type SchedulerBudgetState = wave_reducer::SchedulerBudgetState;
pub type WaveOwnershipState = wave_reducer::WaveOwnershipState;

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
pub struct WaveQueueEntry {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub depends_on: Vec<u32>,
    pub blocked_by: Vec<String>,
    pub blocker_state: Vec<WaveBlockerState>,
    pub lint_errors: usize,
    pub ready: bool,
    pub ownership: WaveOwnershipState,
    pub agent_count: usize,
    pub implementation_agent_count: usize,
    pub closure_agent_count: usize,
    pub closure_complete: bool,
    pub required_closure_agents: Vec<String>,
    pub present_closure_agents: Vec<String>,
    pub missing_closure_agents: Vec<String>,
    pub readiness: WaveReadinessState,
    pub rerun_requested: bool,
    pub completed: bool,
    pub last_run_status: Option<WaveRunStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningStatusSummary {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningStatus {
    pub project_name: String,
    pub default_mode: ExecutionMode,
    pub summary: PlanningStatusSummary,
    pub skill_catalog: SkillCatalogHealth,
    pub queue: QueueReadinessProjection,
    pub next_ready_wave_ids: Vec<u32>,
    pub waves: Vec<WaveQueueEntry>,
    pub has_errors: bool,
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
pub struct WaveClosureStatus {
    pub complete: bool,
    pub required_agents: Vec<String>,
    pub present_agents: Vec<String>,
    pub missing_agents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WavePlanningProjection {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub depends_on: Vec<u32>,
    pub blocked_by: Vec<String>,
    pub blocker_state: Vec<WaveBlockerState>,
    pub readiness: WaveReadinessState,
    pub lint_errors: usize,
    pub ready: bool,
    pub ownership: WaveOwnershipState,
    pub rerun_requested: bool,
    pub completed: bool,
    pub last_run_status: Option<WaveRunStatus>,
    pub agents: WaveAgentCounts,
    pub closure: WaveClosureStatus,
    pub blockers: WaveBlockerFlags,
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
pub struct SkillCatalogProjection {
    pub ok: bool,
    pub issue_count: usize,
    pub issue_paths: Vec<String>,
    pub issues: Vec<SkillCatalogIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningStatusProjection {
    pub agent_counts: AgentCountProjection,
    pub closure_coverage: ClosureCoverageProjection,
    pub queue: QueueProjection,
    pub skill_catalog: SkillCatalogProjection,
    pub run: RunProjection,
    pub agents: AgentsProjection,
    pub control: ControlProjection,
    pub waves: Vec<WavePlanningProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunProjection {
    pub active_wave_ids: Vec<u32>,
    pub active_run_ids: Vec<String>,
    pub active_run_count: usize,
    pub completed_run_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentsProjection {
    pub total_agents: usize,
    pub implementation_agents: usize,
    pub closure_agents: usize,
    pub required_closure_agents: Vec<String>,
    pub present_closure_agents: Vec<String>,
    pub missing_closure_agents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlProjection {
    pub rerun_supported: bool,
    pub clear_rerun_supported: bool,
    pub launch_supported: bool,
    pub autonomous_supported: bool,
    pub launcher_required: bool,
    pub launcher_ready: bool,
    pub unavailable_actions: Vec<String>,
    pub unavailable_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningProjectionBundle {
    pub status: PlanningStatus,
    pub projection: PlanningStatusProjection,
}

pub type PlanningStatusReadModel = PlanningStatus;
pub type PlanningProjectionReadModel = PlanningStatusProjection;
pub type WaveStatusReadModel = WaveQueueEntry;
pub type QueueReadinessReadModel = QueueReadinessProjection;
pub type QueueReadinessStateReadModel = QueueReadinessState;
pub type WaveReadinessReadModel = WaveReadinessState;
pub type QueueBlockerReadModel = WaveBlockerState;
pub type QueueBlockerKindReadModel = WaveBlockerKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DashboardRunReadModel {
    pub wave_id: u32,
    pub run_id: String,
    pub status: String,
    pub agent_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DashboardReadModel {
    pub project_name: String,
    pub next_ready_wave_ids: Vec<u32>,
    pub active_runs: Vec<DashboardRunReadModel>,
    pub total_waves: usize,
    pub completed_waves: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueuePanelWaveReadModel {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub queue_state: String,
    pub blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueuePanelReadModel {
    pub ready_wave_count: usize,
    pub claimed_wave_count: usize,
    pub blocked_wave_count: usize,
    pub active_wave_count: usize,
    pub completed_wave_count: usize,
    pub ready_wave_ids: Vec<u32>,
    pub claimed_wave_ids: Vec<u32>,
    pub blocked_wave_ids: Vec<u32>,
    pub active_wave_ids: Vec<u32>,
    pub blocker_summary: QueueBlockerSummary,
    pub next_ready_wave_ids: Vec<u32>,
    pub claimable_wave_ids: Vec<u32>,
    pub queue_ready: bool,
    pub queue_ready_reason: String,
    pub waves: Vec<QueuePanelWaveReadModel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlActionReadModel {
    pub key: String,
    pub label: String,
    pub description: String,
    pub implemented: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlPanelReadModel {
    pub rerun_supported: bool,
    pub clear_rerun_supported: bool,
    pub launch_supported: bool,
    pub autonomous_supported: bool,
    pub launcher_required: bool,
    pub launcher_ready: bool,
    pub actions: Vec<ControlActionReadModel>,
    pub implemented_actions: Vec<ControlActionReadModel>,
    pub unavailable_actions: Vec<ControlActionReadModel>,
    pub unavailable_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorSnapshotInputs {
    pub dashboard: DashboardReadModel,
    pub run: RunProjection,
    pub agents: AgentsProjection,
    pub queue: QueuePanelReadModel,
    pub control: ControlPanelReadModel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectionSpine {
    pub planning: PlanningProjectionBundle,
    pub operator: OperatorSnapshotInputs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueDecisionReadModel {
    pub next_claimable_wave_id: Option<u32>,
    pub claimable_wave_ids: Vec<u32>,
    pub claimed_wave_ids: Vec<u32>,
    pub queue_ready_reason: String,
    pub blocker_summary: QueueBlockerSummary,
    pub closure_blocked: Vec<WaveRef>,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlStatusReadModel {
    pub queue_decision: QueueDecisionReadModel,
    pub closure_attention_lines: Vec<String>,
    pub skill_issue_paths: Vec<String>,
    pub skill_issue_lines: Vec<String>,
}

pub type PlanningStatusView = PlanningStatus;
pub type PlanningProjection = PlanningStatusProjection;
pub type WaveStatusEntry = WaveQueueEntry;

pub fn build_planning_projection_bundle(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> PlanningProjectionBundle {
    build_planning_projection_bundle_with_state(
        config,
        waves,
        findings,
        &[],
        latest_runs,
        &HashSet::new(),
    )
}

pub fn build_planning_projection_bundle_with_skill_catalog(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> PlanningProjectionBundle {
    build_planning_projection_bundle_with_state(
        config,
        waves,
        findings,
        skill_catalog_issues,
        latest_runs,
        &HashSet::new(),
    )
}

pub fn build_planning_projection_bundle_with_state(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, WaveRunRecord>,
    rerun_wave_ids: &HashSet<u32>,
) -> PlanningProjectionBundle {
    let compatibility_runs = compatibility_run_inputs_by_wave(latest_runs);
    build_planning_projection_bundle_with_compatibility_state(
        config,
        waves,
        findings,
        skill_catalog_issues,
        &compatibility_runs,
        rerun_wave_ids,
    )
}

pub fn build_planning_projection_bundle_with_compatibility_state(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, CompatibilityRunInput>,
    rerun_wave_ids: &HashSet<u32>,
) -> PlanningProjectionBundle {
    let reduced = reduce_planning_state_with_scheduler(
        waves,
        findings,
        skill_catalog_issues,
        latest_runs,
        rerun_wave_ids,
        &[],
    );
    let status = build_planning_status_from_reducer(config, &reduced);
    let projection = build_planning_status_projection(&status);
    PlanningProjectionBundle { status, projection }
}

pub fn build_planning_projection_bundle_with_scheduler_state(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, CompatibilityRunInput>,
    rerun_wave_ids: &HashSet<u32>,
    scheduler_events: &[SchedulerEvent],
) -> PlanningProjectionBundle {
    let reduced = reduce_planning_state_with_scheduler(
        waves,
        findings,
        skill_catalog_issues,
        latest_runs,
        rerun_wave_ids,
        scheduler_events,
    );
    let status = build_planning_status_from_reducer(config, &reduced);
    let projection = build_planning_status_projection(&status);
    PlanningProjectionBundle { status, projection }
}

pub fn build_projection_spine_with_state(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, WaveRunRecord>,
    rerun_wave_ids: &HashSet<u32>,
    launcher_ready: bool,
) -> ProjectionSpine {
    let compatibility_runs = compatibility_run_inputs_by_wave(latest_runs);
    build_projection_spine_with_compatibility_state(
        config,
        waves,
        findings,
        skill_catalog_issues,
        &compatibility_runs,
        rerun_wave_ids,
        launcher_ready,
    )
}

pub fn build_projection_spine_with_compatibility_state(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, CompatibilityRunInput>,
    rerun_wave_ids: &HashSet<u32>,
    launcher_ready: bool,
) -> ProjectionSpine {
    let planning = build_planning_projection_bundle_with_scheduler_state(
        config,
        waves,
        findings,
        skill_catalog_issues,
        latest_runs,
        rerun_wave_ids,
        &[],
    );
    let operator = build_operator_snapshot_inputs_from_compatibility_runs(
        &planning,
        latest_runs,
        launcher_ready,
    );
    ProjectionSpine { planning, operator }
}

pub fn build_projection_spine_with_scheduler_state(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, CompatibilityRunInput>,
    rerun_wave_ids: &HashSet<u32>,
    scheduler_events: &[SchedulerEvent],
    launcher_ready: bool,
) -> ProjectionSpine {
    let planning = build_planning_projection_bundle_with_scheduler_state(
        config,
        waves,
        findings,
        skill_catalog_issues,
        latest_runs,
        rerun_wave_ids,
        scheduler_events,
    );
    let operator = build_operator_snapshot_inputs_from_compatibility_runs(
        &planning,
        latest_runs,
        launcher_ready,
    );
    ProjectionSpine { planning, operator }
}

pub fn build_projection_spine_from_authority(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    fallback_runs: &HashMap<u32, WaveRunRecord>,
    rerun_wave_ids: &HashSet<u32>,
    launcher_ready: bool,
) -> Result<ProjectionSpine> {
    let mut compatibility_runs = load_canonical_compatibility_runs(root, config, waves)?;
    for (wave_id, run) in compatibility_run_inputs_by_wave(fallback_runs) {
        compatibility_runs.entry(wave_id).or_insert(run);
    }
    let scheduler_events = load_scheduler_events(root, config)?;

    Ok(build_projection_spine_with_scheduler_state(
        config,
        waves,
        findings,
        skill_catalog_issues,
        &compatibility_runs,
        rerun_wave_ids,
        &scheduler_events,
        launcher_ready,
    ))
}

pub fn build_planning_status(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> PlanningStatus {
    build_planning_projection_bundle(config, waves, findings, latest_runs).status
}

pub fn build_planning_status_with_skill_catalog(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> PlanningStatus {
    build_planning_projection_bundle_with_skill_catalog(
        config,
        waves,
        findings,
        skill_catalog_issues,
        latest_runs,
    )
    .status
}

pub fn build_planning_status_with_state(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, WaveRunRecord>,
    rerun_wave_ids: &HashSet<u32>,
) -> PlanningStatus {
    build_planning_projection_bundle_with_state(
        config,
        waves,
        findings,
        skill_catalog_issues,
        latest_runs,
        rerun_wave_ids,
    )
    .status
}

pub fn build_planning_status_from_authority(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    fallback_runs: &HashMap<u32, WaveRunRecord>,
    rerun_wave_ids: &HashSet<u32>,
) -> Result<PlanningStatus> {
    Ok(build_projection_spine_from_authority(
        root,
        config,
        waves,
        findings,
        skill_catalog_issues,
        fallback_runs,
        rerun_wave_ids,
        true,
    )?
    .planning
    .status)
}

pub fn build_planning_status_projection(status: &PlanningStatus) -> PlanningStatusProjection {
    let mut ready = Vec::new();
    let mut claimed = Vec::new();
    let mut active = Vec::new();
    let mut completed = Vec::new();
    let mut blocked = Vec::new();
    let mut blocker_summary = QueueBlockerSummary::default();
    let mut blocker_waves = QueueBlockerWaves::default();
    let mut active_wave_ids = Vec::new();
    let mut complete_wave_ids = Vec::new();
    let mut missing_wave_ids = Vec::new();
    let mut closure_gaps = Vec::new();
    let mut required_agents = 0;
    let mut present_agents = 0;
    let mut waves = Vec::new();

    for wave in &status.waves {
        let wave_ref = WaveRef {
            id: wave.id,
            slug: wave.slug.clone(),
            title: wave.title.clone(),
        };
        let blocker_state = if wave.blocker_state.is_empty() {
            classify_blockers(&wave.blocked_by)
        } else {
            wave.blocker_state.clone()
        };
        let blocker_flags = classify_blocker_flags(&blocker_state);

        if wave.ready {
            ready.push(wave_ref.clone());
        }

        if matches!(wave.readiness.state, QueueReadinessState::Claimed) {
            claimed.push(wave_ref.clone());
        }

        if matches!(wave.readiness.state, QueueReadinessState::Active) {
            active.push(wave_ref.clone());
            active_wave_ids.push(wave.id);
        }

        if wave.completed {
            completed.push(wave_ref.clone());
        }

        required_agents += wave.required_closure_agents.len();
        present_agents += wave.present_closure_agents.len();

        if wave.closure_complete {
            complete_wave_ids.push(wave.id);
        } else {
            missing_wave_ids.push(wave.id);
            closure_gaps.push(WaveClosureGap {
                id: wave.id,
                slug: wave.slug.clone(),
                title: wave.title.clone(),
                present_closure_agents: wave.present_closure_agents.clone(),
                missing_closure_agents: wave.missing_closure_agents.clone(),
                implementation_agent_count: wave.implementation_agent_count,
                closure_agent_count: wave.closure_agent_count,
                blocked_by: wave.blocked_by.clone(),
            });
        }

        if !wave.ready {
            accumulate_blockers(&mut blocker_summary, &wave.blocked_by);
            accumulate_blocker_waves(&mut blocker_waves, &wave_ref, &blocker_flags);
        }

        if matches!(wave.readiness.state, QueueReadinessState::Blocked) {
            blocked.push(BlockedWaveProjection {
                id: wave.id,
                slug: wave.slug.clone(),
                title: wave.title.clone(),
                depends_on: wave.depends_on.clone(),
                blocked_by: wave.blocked_by.clone(),
                blocker_state: blocker_state.clone(),
                lint_errors: wave.lint_errors,
                missing_closure_agents: wave.missing_closure_agents.clone(),
                rerun_requested: wave.rerun_requested,
                last_run_status: wave.last_run_status,
            });
        }

        waves.push(WavePlanningProjection {
            id: wave.id,
            slug: wave.slug.clone(),
            title: wave.title.clone(),
            depends_on: wave.depends_on.clone(),
            blocked_by: wave.blocked_by.clone(),
            blocker_state: blocker_state.clone(),
            readiness: wave.readiness.clone(),
            lint_errors: wave.lint_errors,
            ready: wave.ready,
            ownership: wave.ownership.clone(),
            rerun_requested: wave.rerun_requested,
            completed: wave.completed,
            last_run_status: wave.last_run_status,
            agents: WaveAgentCounts {
                total: wave.agent_count,
                implementation: wave.implementation_agent_count,
                closure: wave.closure_agent_count,
            },
            closure: WaveClosureStatus {
                complete: wave.closure_complete,
                required_agents: wave.required_closure_agents.clone(),
                present_agents: wave.present_closure_agents.clone(),
                missing_agents: wave.missing_closure_agents.clone(),
            },
            blockers: blocker_flags,
        });
    }

    let active_run_count = active.len();
    let completed_run_count = completed.len();

    PlanningStatusProjection {
        agent_counts: AgentCountProjection {
            total: status.summary.total_agents,
            implementation: status.summary.implementation_agents,
            closure: status.summary.closure_agents,
        },
        closure_coverage: ClosureCoverageProjection {
            complete_wave_ids,
            missing_wave_ids,
            required_agents,
            present_agents,
            missing_required_agents: status.summary.total_missing_closure_agents,
            waves: closure_gaps,
        },
        queue: QueueProjection {
            ready,
            claimed,
            active,
            completed,
            blocked,
            blocker_summary,
            blocker_waves,
            readiness: status.queue.clone(),
        },
        skill_catalog: SkillCatalogProjection {
            ok: status.skill_catalog.ok,
            issue_count: status.skill_catalog.issue_count,
            issue_paths: status
                .skill_catalog
                .issues
                .iter()
                .map(|issue| issue.path.clone())
                .collect(),
            issues: status.skill_catalog.issues.clone(),
        },
        run: RunProjection {
            active_wave_ids: active_wave_ids.clone(),
            active_run_ids: active_wave_ids
                .iter()
                .map(|wave_id| wave_id.to_string())
                .collect(),
            active_run_count,
            completed_run_count,
        },
        agents: AgentsProjection {
            total_agents: status.summary.total_agents,
            implementation_agents: status.summary.implementation_agents,
            closure_agents: status.summary.closure_agents,
            required_closure_agents: REQUIRED_CLOSURE_AGENT_IDS
                .iter()
                .map(|agent_id| (*agent_id).to_string())
                .collect(),
            present_closure_agents: status
                .waves
                .iter()
                .flat_map(|wave| wave.present_closure_agents.clone())
                .collect(),
            missing_closure_agents: status
                .waves
                .iter()
                .flat_map(|wave| wave.missing_closure_agents.clone())
                .collect(),
        },
        control: ControlProjection {
            rerun_supported: true,
            clear_rerun_supported: true,
            launch_supported: true,
            autonomous_supported: true,
            launcher_required: true,
            launcher_ready: true,
            unavailable_actions: Vec::new(),
            unavailable_reasons: Vec::new(),
        },
        waves,
    }
}

pub fn build_dashboard_read_model(
    status: &PlanningStatus,
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> DashboardReadModel {
    build_dashboard_read_model_from_compatibility_runs(
        status,
        &compatibility_run_inputs_by_wave(latest_runs),
    )
}

pub fn build_dashboard_read_model_from_compatibility_runs(
    status: &PlanningStatus,
    latest_runs: &HashMap<u32, CompatibilityRunInput>,
) -> DashboardReadModel {
    let mut active_runs = latest_runs
        .values()
        .filter(|run| matches!(run.status, WaveRunStatus::Planned | WaveRunStatus::Running))
        .map(|run| DashboardRunReadModel {
            wave_id: run.wave_id,
            run_id: run.run_id.clone(),
            status: run.status.to_string(),
            agent_count: run.agents.len(),
        })
        .collect::<Vec<_>>();
    active_runs.sort_by_key(|run| run.wave_id);

    DashboardReadModel {
        project_name: status.project_name.clone(),
        next_ready_wave_ids: status.next_ready_wave_ids.clone(),
        active_runs,
        total_waves: status.waves.len(),
        completed_waves: status.waves.iter().filter(|wave| wave.completed).count(),
    }
}

pub fn build_operator_snapshot_inputs(
    planning: &PlanningProjectionBundle,
    latest_runs: &HashMap<u32, WaveRunRecord>,
    launcher_ready: bool,
) -> OperatorSnapshotInputs {
    build_operator_snapshot_inputs_from_compatibility_runs(
        planning,
        &compatibility_run_inputs_by_wave(latest_runs),
        launcher_ready,
    )
}

pub fn build_operator_snapshot_inputs_from_compatibility_runs(
    planning: &PlanningProjectionBundle,
    latest_runs: &HashMap<u32, CompatibilityRunInput>,
    launcher_ready: bool,
) -> OperatorSnapshotInputs {
    OperatorSnapshotInputs {
        dashboard: build_dashboard_read_model_from_compatibility_runs(
            &planning.status,
            latest_runs,
        ),
        run: planning.projection.run.clone(),
        agents: planning.projection.agents.clone(),
        queue: build_queue_panel_read_model(&planning.projection),
        control: build_control_panel_read_model(&planning.projection.control, launcher_ready),
    }
}

pub fn build_queue_panel_read_model(projection: &PlanningStatusProjection) -> QueuePanelReadModel {
    QueuePanelReadModel {
        ready_wave_count: projection.queue.ready.len(),
        claimed_wave_count: projection.queue.claimed.len(),
        blocked_wave_count: projection.queue.blocked.len(),
        active_wave_count: projection.queue.active.len(),
        completed_wave_count: projection.queue.completed.len(),
        ready_wave_ids: projection.queue.ready.iter().map(|wave| wave.id).collect(),
        claimed_wave_ids: projection
            .queue
            .claimed
            .iter()
            .map(|wave| wave.id)
            .collect(),
        blocked_wave_ids: projection
            .queue
            .blocked
            .iter()
            .map(|wave| wave.id)
            .collect(),
        active_wave_ids: projection.queue.active.iter().map(|wave| wave.id).collect(),
        blocker_summary: projection.queue.blocker_summary.clone(),
        next_ready_wave_ids: projection.queue.readiness.next_ready_wave_ids.clone(),
        claimable_wave_ids: projection.queue.readiness.claimable_wave_ids.clone(),
        queue_ready: projection.queue.readiness.queue_ready,
        queue_ready_reason: projection.queue.readiness.queue_ready_reason.clone(),
        waves: projection
            .waves
            .iter()
            .map(|wave| QueuePanelWaveReadModel {
                id: wave.id,
                slug: wave.slug.clone(),
                title: wave.title.clone(),
                queue_state: queue_state_label(wave.readiness.state, &wave.blocked_by),
                blocked: matches!(wave.readiness.state, QueueReadinessState::Blocked),
            })
            .collect(),
    }
}

pub fn build_control_panel_read_model(
    control: &ControlProjection,
    launcher_ready: bool,
) -> ControlPanelReadModel {
    let actions = build_control_action_read_models(control, launcher_ready);
    let mut unavailable_reasons = control.unavailable_reasons.clone();
    if control.launcher_required && !launcher_ready {
        unavailable_reasons.push("codex binary is missing".to_string());
    }

    ControlPanelReadModel {
        rerun_supported: control.rerun_supported,
        clear_rerun_supported: control.clear_rerun_supported,
        launch_supported: control.launch_supported,
        autonomous_supported: control.autonomous_supported,
        launcher_required: control.launcher_required,
        launcher_ready,
        implemented_actions: actions
            .iter()
            .filter(|action| action.implemented)
            .cloned()
            .collect(),
        unavailable_actions: actions
            .iter()
            .filter(|action| !action.implemented)
            .cloned()
            .collect(),
        actions,
        unavailable_reasons,
    }
}

pub fn build_control_action_read_models(
    control: &ControlProjection,
    launcher_ready: bool,
) -> Vec<ControlActionReadModel> {
    vec![
        ControlActionReadModel {
            key: "Tab".to_string(),
            label: "Next panel".to_string(),
            description: "Cycle the right-side panel tabs.".to_string(),
            implemented: true,
        },
        ControlActionReadModel {
            key: "j/k".to_string(),
            label: "Select wave".to_string(),
            description: "Move the queue selection in the operator shell.".to_string(),
            implemented: true,
        },
        ControlActionReadModel {
            key: "r".to_string(),
            label: "Request rerun".to_string(),
            description: if control.rerun_supported {
                "Write a rerun intent for the selected wave.".to_string()
            } else {
                "Rerun requests are not supported by the control plane yet.".to_string()
            },
            implemented: control.rerun_supported,
        },
        ControlActionReadModel {
            key: "c".to_string(),
            label: "Clear rerun".to_string(),
            description: if control.clear_rerun_supported {
                "Clear the selected wave's rerun intent.".to_string()
            } else {
                "Clearing rerun intents is not supported by the control plane yet.".to_string()
            },
            implemented: control.clear_rerun_supported,
        },
        ControlActionReadModel {
            key: "launch".to_string(),
            label: "Launch wave".to_string(),
            description: if control.launch_supported {
                if launcher_ready {
                    "Start the selected ready wave through the Codex launcher.".to_string()
                } else {
                    "Launch is unavailable because the Codex binary is missing.".to_string()
                }
            } else {
                "Launch is not supported by the control plane yet.".to_string()
            },
            implemented: control.launch_supported && launcher_ready,
        },
        ControlActionReadModel {
            key: "autonomous".to_string(),
            label: "Launch queue".to_string(),
            description: if control.autonomous_supported {
                if launcher_ready {
                    "Run the ready queue through the Codex launcher.".to_string()
                } else {
                    "Autonomous launch is unavailable because the Codex binary is missing."
                        .to_string()
                }
            } else {
                "Autonomous launch is not supported by the control plane yet.".to_string()
            },
            implemented: control.autonomous_supported && launcher_ready,
        },
        ControlActionReadModel {
            key: "q".to_string(),
            label: "Quit".to_string(),
            description: "Leave the operator shell.".to_string(),
            implemented: true,
        },
    ]
}

pub fn build_queue_decision_read_model(
    status: &PlanningStatus,
    projection: &PlanningStatusProjection,
) -> QueueDecisionReadModel {
    let next_claimable_wave_id = status.queue.next_ready_wave_id;
    let closure_blocked = projection.queue.blocker_waves.closure.clone();

    QueueDecisionReadModel {
        next_claimable_wave_id,
        claimable_wave_ids: status.queue.claimable_wave_ids.clone(),
        claimed_wave_ids: status.queue.claimed_wave_ids.clone(),
        queue_ready_reason: status.queue.queue_ready_reason.clone(),
        blocker_summary: projection.queue.blocker_summary.clone(),
        closure_blocked: closure_blocked.clone(),
        lines: queue_decision_story_lines(
            next_claimable_wave_id,
            &status.queue.queue_ready_reason,
            &status.queue.claimable_wave_ids,
            &status.queue.claimed_wave_ids,
            &projection.queue.blocker_summary,
            &closure_blocked,
        ),
    }
}

pub fn build_queue_decision_read_model_from_status(
    status: &PlanningStatus,
) -> QueueDecisionReadModel {
    let projection = build_planning_status_projection(status);
    build_queue_decision_read_model(status, &projection)
}

pub fn build_control_status_read_model(
    status: &PlanningStatus,
    projection: &PlanningStatusProjection,
) -> ControlStatusReadModel {
    ControlStatusReadModel {
        queue_decision: build_queue_decision_read_model(status, projection),
        closure_attention_lines: build_closure_attention_lines(projection),
        skill_issue_paths: projection.skill_catalog.issue_paths.clone(),
        skill_issue_lines: build_skill_issue_lines(&projection.skill_catalog),
    }
}

pub fn build_control_status_read_model_from_spine(
    spine: &ProjectionSpine,
) -> ControlStatusReadModel {
    build_control_status_read_model(&spine.planning.status, &spine.planning.projection)
}

pub fn build_closure_attention_lines(projection: &PlanningStatusProjection) -> Vec<String> {
    projection
        .waves
        .iter()
        .filter(|wave| !wave.closure.complete)
        .map(|wave| {
            format!(
                "closure gap: wave {} {} missing {} | agents={} (impl={} closure={}) | blockers={}",
                wave.id,
                wave.slug,
                format_string_list(&wave.closure.missing_agents),
                wave.agents.total,
                wave.agents.implementation,
                wave.agents.closure,
                format_blockers(&wave.blocked_by)
            )
        })
        .collect()
}

pub fn build_skill_issue_lines(skill_catalog: &SkillCatalogProjection) -> Vec<String> {
    skill_catalog
        .issues
        .iter()
        .map(|issue| format!("skill issue: {} ({})", issue.path, issue.message))
        .collect()
}

fn queue_decision_story_lines(
    next_claimable_wave_id: Option<u32>,
    queue_ready_reason: &str,
    claimable_wave_ids: &[u32],
    claimed_wave_ids: &[u32],
    blocker_summary: &QueueBlockerSummary,
    closure_blocked: &[WaveRef],
) -> Vec<String> {
    vec![
        format!(
            "queue decision: next claimable wave={}",
            next_claimable_wave_id
                .map(|wave_id| wave_id.to_string())
                .unwrap_or_else(|| "none".to_string())
        ),
        format!(
            "queue decision: claimable waves={}",
            format_u32_list(claimable_wave_ids)
        ),
        format!(
            "queue decision: claimed waves={}",
            format_u32_list(claimed_wave_ids)
        ),
        format!("queue decision: queue ready reason={queue_ready_reason}"),
        format!(
            "queue decision: blocker story dependency={} lint={} closure={} ownership={} lease_expired={} budget={} active_run={}",
            blocker_summary.dependency,
            blocker_summary.lint,
            blocker_summary.closure,
            blocker_summary.ownership,
            blocker_summary.lease_expired,
            blocker_summary.budget,
            blocker_summary.active_run
        ),
        format!(
            "queue decision: closure-blocked={}",
            format_wave_refs(closure_blocked)
        ),
    ]
}

fn format_u32_list(values: &[u32]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn format_string_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn format_wave_refs(values: &[WaveRef]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .map(|wave| format!("{}:{}", wave.id, wave.slug))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn format_blockers(blocked_by: &[String]) -> String {
    if blocked_by.is_empty() {
        "none".to_string()
    } else {
        blocked_by.join(", ")
    }
}

fn queue_state_label(state: QueueReadinessState, blocked_by: &[String]) -> String {
    match state {
        QueueReadinessState::Completed => "completed".to_string(),
        QueueReadinessState::Ready => "ready".to_string(),
        QueueReadinessState::Claimed => "claimed".to_string(),
        QueueReadinessState::Active => "active".to_string(),
        QueueReadinessState::Blocked if blocked_by.is_empty() => "blocked".to_string(),
        QueueReadinessState::Blocked => format!("blocked: {}", blocked_by.join(", ")),
    }
}

fn build_planning_status_from_reducer(
    config: &ProjectConfig,
    reduced: &PlanningReducerState,
) -> PlanningStatus {
    PlanningStatus {
        project_name: config.project_name.clone(),
        default_mode: config.default_mode,
        summary: PlanningStatusSummary {
            total_waves: reduced.summary.total_waves,
            ready_waves: reduced.summary.ready_waves,
            blocked_waves: reduced.summary.blocked_waves,
            active_waves: reduced.summary.active_waves,
            completed_waves: reduced.summary.completed_waves,
            total_agents: reduced.summary.total_agents,
            implementation_agents: reduced.summary.implementation_agents,
            closure_agents: reduced.summary.closure_agents,
            waves_with_complete_closure: reduced.summary.waves_with_complete_closure,
            waves_missing_closure: reduced.summary.waves_missing_closure,
            total_missing_closure_agents: reduced.summary.total_missing_closure_agents,
            lint_error_waves: reduced.summary.lint_error_waves,
            skill_catalog_issue_count: reduced.summary.skill_catalog_issue_count,
        },
        skill_catalog: SkillCatalogHealth {
            ok: reduced.skill_catalog.ok,
            issue_count: reduced.skill_catalog.issue_count,
            issues: reduced.skill_catalog.issues.clone(),
        },
        queue: convert_queue_readiness(&reduced.queue.readiness),
        next_ready_wave_ids: reduced.queue.readiness.next_ready_wave_ids.clone(),
        waves: reduced
            .waves
            .iter()
            .map(|wave| WaveQueueEntry {
                id: wave.id,
                slug: wave.slug.clone(),
                title: wave.title.clone(),
                depends_on: wave.depends_on.clone(),
                blocked_by: wave.blocked_by.clone(),
                blocker_state: wave
                    .blocker_state
                    .iter()
                    .map(convert_blocker_state)
                    .collect(),
                lint_errors: wave.lint_errors,
                ready: wave.ready,
                ownership: wave.ownership.clone(),
                agent_count: wave.agents.total,
                implementation_agent_count: wave.agents.implementation,
                closure_agent_count: wave.agents.closure,
                closure_complete: wave.closure.complete,
                required_closure_agents: wave.closure.required_agent_ids.clone(),
                present_closure_agents: wave.closure.present_agent_ids.clone(),
                missing_closure_agents: wave.closure.missing_agent_ids.clone(),
                readiness: convert_wave_readiness(&wave.readiness),
                rerun_requested: wave.lifecycle.rerun_requested,
                completed: wave.lifecycle.completed,
                last_run_status: wave.lifecycle.last_run_status,
            })
            .collect(),
        has_errors: reduced.has_errors,
    }
}

fn convert_queue_readiness(
    projection: &wave_reducer::QueueReadinessProjection,
) -> QueueReadinessProjection {
    QueueReadinessProjection {
        next_ready_wave_ids: projection.next_ready_wave_ids.clone(),
        next_ready_wave_id: projection.next_ready_wave_id,
        claimable_wave_ids: projection.claimable_wave_ids.clone(),
        claimed_wave_ids: projection.claimed_wave_ids.clone(),
        ready_wave_count: projection.ready_wave_count,
        claimed_wave_count: projection.claimed_wave_count,
        blocked_wave_count: projection.blocked_wave_count,
        active_wave_count: projection.active_wave_count,
        completed_wave_count: projection.completed_wave_count,
        queue_ready: projection.queue_ready,
        queue_ready_reason: projection.queue_ready_reason.clone(),
    }
}

fn convert_wave_readiness(readiness: &wave_reducer::WaveReadinessState) -> WaveReadinessState {
    WaveReadinessState {
        state: convert_queue_readiness_state(readiness.state),
        planning_ready: readiness.planning_ready,
        claimable: readiness.claimable,
        reasons: readiness
            .reasons
            .iter()
            .map(convert_blocker_state)
            .collect(),
        primary_reason: readiness.primary_reason.as_ref().map(convert_blocker_state),
    }
}

fn convert_blocker_state(state: &wave_reducer::WaveBlockerState) -> WaveBlockerState {
    WaveBlockerState {
        kind: convert_blocker_kind(state.kind),
        raw: state.raw.clone(),
        detail: state.detail.clone(),
    }
}

fn convert_blocker_kind(kind: wave_reducer::WaveBlockerKind) -> WaveBlockerKind {
    match kind {
        wave_reducer::WaveBlockerKind::Dependency => WaveBlockerKind::Dependency,
        wave_reducer::WaveBlockerKind::Lint => WaveBlockerKind::Lint,
        wave_reducer::WaveBlockerKind::Closure => WaveBlockerKind::Closure,
        wave_reducer::WaveBlockerKind::Ownership => WaveBlockerKind::Ownership,
        wave_reducer::WaveBlockerKind::LeaseExpired => WaveBlockerKind::LeaseExpired,
        wave_reducer::WaveBlockerKind::Budget => WaveBlockerKind::Budget,
        wave_reducer::WaveBlockerKind::ActiveRun => WaveBlockerKind::ActiveRun,
        wave_reducer::WaveBlockerKind::AlreadyCompleted => WaveBlockerKind::AlreadyCompleted,
        wave_reducer::WaveBlockerKind::Other => WaveBlockerKind::Other,
    }
}

fn convert_queue_readiness_state(state: wave_reducer::QueueReadinessState) -> QueueReadinessState {
    match state {
        wave_reducer::QueueReadinessState::Ready => QueueReadinessState::Ready,
        wave_reducer::QueueReadinessState::Claimed => QueueReadinessState::Claimed,
        wave_reducer::QueueReadinessState::Blocked => QueueReadinessState::Blocked,
        wave_reducer::QueueReadinessState::Active => QueueReadinessState::Active,
        wave_reducer::QueueReadinessState::Completed => QueueReadinessState::Completed,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use wave_config::AuthorityConfig;
    use wave_config::DarkFactoryPolicy;
    use wave_config::LaneConfig;
    use wave_dark_factory::FindingSeverity;
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
    use wave_events::SchedulerEvent;
    use wave_events::SchedulerEventKind;
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

    #[test]
    fn dependent_wave_becomes_ready_after_successful_dependency() {
        let config = test_config();
        let waves = vec![test_wave(0, Vec::new()), test_wave(1, vec![0])];
        let findings = vec![LintFinding {
            wave_id: 0,
            severity: FindingSeverity::Warning,
            rule: "note",
            message: "noop".to_string(),
        }];

        let status_before = build_planning_status_with_skill_catalog(
            &config,
            &waves,
            &findings,
            &[],
            &HashMap::new(),
        );
        assert!(status_before.waves[0].ready);
        assert_eq!(
            status_before.waves[0].readiness.state,
            QueueReadinessState::Ready
        );
        assert_eq!(status_before.queue.next_ready_wave_ids, vec![0]);
        assert_eq!(status_before.queue.next_ready_wave_id, Some(0));
        assert_eq!(
            status_before.waves[1].readiness.state,
            QueueReadinessState::Blocked
        );

        let latest_runs = HashMap::from([(
            0,
            WaveRunRecord {
                run_id: "wave-0-1".to_string(),
                wave_id: 0,
                slug: "bootstrap".to_string(),
                title: "Bootstrap".to_string(),
                status: WaveRunStatus::Succeeded,
                dry_run: false,
                bundle_dir: PathBuf::from(".wave/state/build/specs/wave-0"),
                trace_path: PathBuf::from(".wave/traces/wave-0.json"),
                codex_home: PathBuf::from(".wave/codex"),
                created_at_ms: 1,
                started_at_ms: Some(1),
                launcher_pid: None,
                launcher_started_at_ms: None,
                completed_at_ms: Some(2),
                agents: Vec::new(),
                error: None,
            },
        )]);

        let status_after =
            build_planning_status_with_skill_catalog(&config, &waves, &[], &[], &latest_runs);
        assert!(status_after.waves[0].completed);
        assert!(status_after.waves[1].ready);
        assert_eq!(
            status_after.waves[1].readiness.state,
            QueueReadinessState::Ready
        );
        assert_eq!(status_after.summary.completed_waves, 1);
        assert_eq!(status_after.summary.ready_waves, 1);
    }

    #[test]
    fn planning_projection_surfaces_queue_and_closure_details() {
        let config = test_config();
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
            (
                0,
                WaveRunRecord {
                    run_id: "wave-0-running".to_string(),
                    wave_id: 0,
                    slug: "wave-0".to_string(),
                    title: "Wave 0".to_string(),
                    status: WaveRunStatus::Running,
                    dry_run: false,
                    bundle_dir: PathBuf::from(".wave/state/build/specs/wave-0"),
                    trace_path: PathBuf::from(".wave/traces/wave-0.json"),
                    codex_home: PathBuf::from(".wave/codex"),
                    created_at_ms: 1,
                    started_at_ms: Some(1),
                    launcher_pid: None,
                    launcher_started_at_ms: None,
                    completed_at_ms: None,
                    agents: Vec::new(),
                    error: None,
                },
            ),
            (
                2,
                WaveRunRecord {
                    run_id: "wave-2-succeeded".to_string(),
                    wave_id: 2,
                    slug: "wave-2".to_string(),
                    title: "Wave 2".to_string(),
                    status: WaveRunStatus::Succeeded,
                    dry_run: false,
                    bundle_dir: PathBuf::from(".wave/state/build/specs/wave-2"),
                    trace_path: PathBuf::from(".wave/traces/wave-2.json"),
                    codex_home: PathBuf::from(".wave/codex"),
                    created_at_ms: 1,
                    started_at_ms: Some(1),
                    launcher_pid: None,
                    launcher_started_at_ms: None,
                    completed_at_ms: Some(2),
                    agents: Vec::new(),
                    error: None,
                },
            ),
        ]);

        let bundle = build_planning_projection_bundle_with_skill_catalog(
            &config,
            &[running_wave, blocked_wave, completed_wave],
            &findings,
            &[],
            &latest_runs,
        );

        assert_eq!(bundle.projection.queue.ready.len(), 0);
        assert_eq!(bundle.projection.queue.active[0].id, 0);
        assert_eq!(bundle.projection.queue.completed[0].id, 2);
        assert_eq!(bundle.projection.queue.blocked[0].id, 1);
        assert_eq!(bundle.projection.queue.blocker_summary.dependency, 1);
        assert_eq!(bundle.projection.queue.blocker_summary.lint, 1);
        assert_eq!(bundle.projection.queue.blocker_summary.closure, 1);
        assert_eq!(bundle.projection.queue.blocker_summary.active_run, 1);
        assert_eq!(bundle.projection.queue.blocker_summary.already_completed, 1);
        assert_eq!(
            bundle.projection.closure_coverage.complete_wave_ids,
            vec![0, 2]
        );
        assert_eq!(bundle.projection.closure_coverage.missing_wave_ids, vec![1]);
        assert_eq!(bundle.projection.closure_coverage.waves[0].id, 1);
        assert_eq!(
            bundle.projection.closure_coverage.waves[0].missing_closure_agents,
            vec!["A9".to_string()]
        );
        assert_eq!(bundle.projection.waves.len(), 3);
        assert!(bundle.projection.waves[0].blockers.active_run);
        assert!(bundle.projection.waves[1].blockers.dependency);
        assert!(bundle.projection.waves[1].blockers.lint);
        assert!(bundle.projection.waves[1].blockers.closure);
        assert!(bundle.projection.waves[2].blockers.already_completed);
    }

    #[test]
    fn projection_spine_surfaces_operator_snapshot_inputs() {
        let config = test_config();
        let waves = vec![test_wave(0, Vec::new())];
        let latest_runs = HashMap::from([(
            0,
            WaveRunRecord {
                run_id: "wave-0-running".to_string(),
                wave_id: 0,
                slug: "wave-0".to_string(),
                title: "Wave 0".to_string(),
                status: WaveRunStatus::Running,
                dry_run: false,
                bundle_dir: PathBuf::from(".wave/state/build/specs/wave-0"),
                trace_path: PathBuf::from(".wave/traces/wave-0.json"),
                codex_home: PathBuf::from(".wave/codex"),
                created_at_ms: 1,
                started_at_ms: Some(1),
                launcher_pid: None,
                launcher_started_at_ms: None,
                completed_at_ms: None,
                agents: Vec::new(),
                error: None,
            },
        )]);

        let spine = build_projection_spine_with_state(
            &config,
            &waves,
            &[],
            &[],
            &latest_runs,
            &HashSet::new(),
            false,
        );

        assert_eq!(spine.operator.dashboard.active_runs[0].wave_id, 0);
        assert_eq!(spine.operator.run.active_wave_ids, vec![0]);
        assert_eq!(spine.operator.queue.active_wave_ids, vec![0]);
        assert_eq!(spine.operator.queue.waves[0].queue_state, "active");
        assert!(!spine.operator.queue.waves[0].blocked);
        assert!(!spine.operator.control.launcher_ready);
        assert_eq!(
            spine.operator.control.unavailable_reasons,
            vec!["codex binary is missing"]
        );
        assert_eq!(spine.operator.control.unavailable_actions[0].key, "launch");
        assert_eq!(spine.operator.control.actions.len(), 7);
    }

    #[test]
    fn queue_panel_read_model_preserves_explicit_readiness_state_labels() {
        let config = test_config();
        let active_wave = test_wave(0, Vec::new());
        let blocked_wave = test_wave(1, vec![0]);
        let completed_wave = test_wave(2, Vec::new());
        let latest_runs = HashMap::from([
            (
                0,
                WaveRunRecord {
                    run_id: "wave-0-running".to_string(),
                    wave_id: 0,
                    slug: "wave-0".to_string(),
                    title: "Wave 0".to_string(),
                    status: WaveRunStatus::Running,
                    dry_run: false,
                    bundle_dir: PathBuf::from(".wave/state/build/specs/wave-0"),
                    trace_path: PathBuf::from(".wave/traces/wave-0.json"),
                    codex_home: PathBuf::from(".wave/codex"),
                    created_at_ms: 1,
                    started_at_ms: Some(1),
                    launcher_pid: None,
                    launcher_started_at_ms: None,
                    completed_at_ms: None,
                    agents: Vec::new(),
                    error: None,
                },
            ),
            (
                2,
                WaveRunRecord {
                    run_id: "wave-2-succeeded".to_string(),
                    wave_id: 2,
                    slug: "wave-2".to_string(),
                    title: "Wave 2".to_string(),
                    status: WaveRunStatus::Succeeded,
                    dry_run: false,
                    bundle_dir: PathBuf::from(".wave/state/build/specs/wave-2"),
                    trace_path: PathBuf::from(".wave/traces/wave-2.json"),
                    codex_home: PathBuf::from(".wave/codex"),
                    created_at_ms: 1,
                    started_at_ms: Some(1),
                    launcher_pid: None,
                    launcher_started_at_ms: None,
                    completed_at_ms: Some(2),
                    agents: Vec::new(),
                    error: None,
                },
            ),
        ]);

        let bundle = build_planning_projection_bundle_with_skill_catalog(
            &config,
            &[active_wave, blocked_wave, completed_wave],
            &[],
            &[],
            &latest_runs,
        );
        let queue = build_queue_panel_read_model(&bundle.projection);

        assert_eq!(queue.waves[0].queue_state, "active");
        assert!(!queue.waves[0].blocked);
        assert!(queue.waves[1].queue_state.starts_with("blocked: wave:0"));
        assert!(queue.waves[1].blocked);
        assert_eq!(queue.waves[2].queue_state, "completed");
        assert!(!queue.waves[2].blocked);
    }

    #[test]
    fn dashboard_read_model_ignores_dry_run_records() {
        let config = test_config();
        let waves = vec![test_wave(0, Vec::new())];
        let status =
            build_planning_status_with_skill_catalog(&config, &waves, &[], &[], &HashMap::new());
        let latest_runs = HashMap::from([(
            0,
            WaveRunRecord {
                run_id: "wave-0-dry-run".to_string(),
                wave_id: 0,
                slug: "wave-0".to_string(),
                title: "Wave 0".to_string(),
                status: WaveRunStatus::DryRun,
                dry_run: true,
                bundle_dir: PathBuf::from(".wave/state/build/specs/wave-0"),
                trace_path: PathBuf::from(".wave/traces/wave-0.json"),
                codex_home: PathBuf::from(".wave/codex"),
                created_at_ms: 1,
                started_at_ms: None,
                launcher_pid: None,
                launcher_started_at_ms: None,
                completed_at_ms: Some(1),
                agents: Vec::new(),
                error: None,
            },
        )]);

        let dashboard = build_dashboard_read_model(&status, &latest_runs);

        assert!(dashboard.active_runs.is_empty());
    }

    #[test]
    fn control_status_can_be_read_directly_from_projection_spine() {
        let config = test_config();
        let mut blocked_wave = test_wave(1, Vec::new());
        blocked_wave.agents.retain(|agent| agent.id != "A9");

        let spine = build_projection_spine_with_state(
            &config,
            &[blocked_wave],
            &[],
            &[SkillCatalogIssue {
                path: "skills/missing.md".to_string(),
                message: "missing prompt".to_string(),
            }],
            &HashMap::new(),
            &HashSet::new(),
            true,
        );

        let status_surface = build_control_status_read_model_from_spine(&spine);

        assert_eq!(
            status_surface.queue_decision.lines[5],
            "queue decision: closure-blocked=1:wave-1"
        );
        assert_eq!(
            status_surface.skill_issue_lines,
            vec!["skill issue: skills/missing.md (missing prompt)".to_string()]
        );
    }

    #[test]
    fn control_status_read_model_preserves_queue_story_and_attention_lines() {
        let config = test_config();
        let ready_wave = test_wave(0, Vec::new());
        let mut blocked_wave = test_wave(1, vec![0]);
        blocked_wave.agents.retain(|agent| agent.id != "A9");
        let findings = vec![LintFinding {
            wave_id: 1,
            severity: FindingSeverity::Error,
            rule: "lint",
            message: "broken prompt".to_string(),
        }];
        let latest_runs = HashMap::from([(
            0,
            WaveRunRecord {
                run_id: "wave-0-succeeded".to_string(),
                wave_id: 0,
                slug: "wave-0".to_string(),
                title: "Wave 0".to_string(),
                status: WaveRunStatus::Succeeded,
                dry_run: false,
                bundle_dir: PathBuf::from(".wave/state/build/specs/wave-0"),
                trace_path: PathBuf::from(".wave/traces/wave-0.json"),
                codex_home: PathBuf::from(".wave/codex"),
                created_at_ms: 1,
                started_at_ms: Some(1),
                launcher_pid: None,
                launcher_started_at_ms: None,
                completed_at_ms: Some(2),
                agents: Vec::new(),
                error: None,
            },
        )]);

        let bundle = build_planning_projection_bundle_with_skill_catalog(
            &config,
            &[ready_wave, blocked_wave],
            &findings,
            &[SkillCatalogIssue {
                path: "skills/missing.md".to_string(),
                message: "missing prompt".to_string(),
            }],
            &latest_runs,
        );
        let status_surface = build_control_status_read_model(&bundle.status, &bundle.projection);

        assert_eq!(
            status_surface.queue_decision.lines[0],
            "queue decision: next claimable wave=none"
        );
        assert_eq!(
            status_surface.queue_decision.lines[5],
            "queue decision: closure-blocked=1:wave-1"
        );
        assert_eq!(status_surface.closure_attention_lines.len(), 1);
        assert!(status_surface.closure_attention_lines[0].contains("missing A9"));
        assert_eq!(
            status_surface.skill_issue_lines,
            vec!["skill issue: skills/missing.md (missing prompt)".to_string()]
        );
    }

    #[test]
    fn queue_decision_read_model_from_status_rebuilds_closure_blocked_story() {
        let config = test_config();
        let mut blocked_wave = test_wave(1, Vec::new());
        blocked_wave.agents.retain(|agent| agent.id != "A9");
        let status = build_planning_status_with_skill_catalog(
            &config,
            &[blocked_wave],
            &[],
            &[],
            &HashMap::new(),
        );

        let queue_story = build_queue_decision_read_model_from_status(&status);

        assert_eq!(
            queue_story.lines[5],
            "queue decision: closure-blocked=1:wave-1"
        );
    }

    #[test]
    fn scheduler_ownership_state_is_projected_for_claimed_and_stale_waves() {
        let config = test_config();
        let waves = vec![test_wave(0, Vec::new()), test_wave(1, Vec::new())];
        let scheduler_events = vec![
            budget_event(1, 1),
            claim_event(0, "claim-wave-0", "wave-0-run", 10),
            claim_event(1, "claim-wave-1", "wave-1-run", 20),
            lease_event(
                1,
                "claim-wave-1",
                "wave-1-run",
                "A1",
                TaskLeaseState::Expired,
                21,
                Some(22),
            ),
        ];

        let bundle = build_planning_projection_bundle_with_scheduler_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
            &scheduler_events,
        );
        let queue = build_queue_panel_read_model(&bundle.projection);
        let control_status = build_control_status_read_model(&bundle.status, &bundle.projection);

        assert_eq!(bundle.projection.queue.claimed.len(), 2);
        assert_eq!(
            bundle.projection.queue.readiness.claimed_wave_ids,
            vec![0, 1]
        );
        assert_eq!(
            bundle.status.waves[0].readiness.state,
            QueueReadinessState::Claimed
        );
        assert_eq!(
            bundle.status.waves[0]
                .ownership
                .claim
                .as_ref()
                .unwrap()
                .owner
                .scheduler_path,
            "wave-runtime/codex"
        );
        assert_eq!(bundle.status.waves[1].ownership.stale_leases.len(), 1);
        assert!(bundle.projection.waves[1].blockers.lease_expired);
        assert_eq!(queue.claimed_wave_count, 2);
        assert_eq!(queue.waves[0].queue_state, "claimed");
        assert_eq!(control_status.queue_decision.claimed_wave_ids, vec![0, 1]);
    }

    fn test_config() -> ProjectConfig {
        ProjectConfig {
            version: 1,
            project_name: "Test".to_string(),
            default_lane: "main".to_string(),
            default_mode: ExecutionMode::DarkFactory,
            waves_dir: PathBuf::from("waves"),
            authority: AuthorityConfig {
                project_codex_home: PathBuf::from(".wave/codex"),
                state_dir: PathBuf::from(".wave/state"),
                state_runs_dir: PathBuf::from(".wave/state/runs"),
                state_control_dir: PathBuf::from(".wave/state/control"),
                trace_dir: PathBuf::from(".wave/traces"),
                trace_runs_dir: PathBuf::from(".wave/traces/runs"),
                ..AuthorityConfig::default()
            },
            codex_vendor_dir: PathBuf::from("third_party/codex-rs"),
            reference_wave_repo_dir: PathBuf::from("third_party/agent-wave-orchestrator"),
            dark_factory: DarkFactoryPolicy {
                require_validation: true,
                require_rollback: true,
                require_proof: true,
                require_closure: true,
            },
            lanes: BTreeMap::<String, LaneConfig>::new(),
            ..ProjectConfig::default()
        }
    }

    fn scheduler_owner(session_id: &str) -> SchedulerOwner {
        SchedulerOwner {
            scheduler_id: "wave-runtime".to_string(),
            scheduler_path: "wave-runtime/codex".to_string(),
            runtime: Some("codex".to_string()),
            executor: Some("codex".to_string()),
            session_id: Some(session_id.to_string()),
        }
    }

    fn claim_event(
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
        let task_id = wave_domain::task_id_for_agent(wave_id, agent_id);
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
            detail: Some("lease update".to_string()),
        };
        let kind = match state {
            TaskLeaseState::Granted => SchedulerEventKind::TaskLeaseGranted,
            TaskLeaseState::Released => SchedulerEventKind::TaskLeaseReleased,
            TaskLeaseState::Expired => SchedulerEventKind::TaskLeaseExpired,
            TaskLeaseState::Revoked => SchedulerEventKind::TaskLeaseRevoked,
        };
        SchedulerEvent::new(format!("sched-lease-{wave_id}-{agent_id}"), kind)
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
                    deliverables: vec!["crates/wave-projections/src/lib.rs".to_string()],
                    file_ownership: vec!["crates/wave-projections/src/lib.rs".to_string()],
                    final_markers: vec![
                        "[wave-proof]".to_string(),
                        "[wave-doc-delta]".to_string(),
                        "[wave-component]".to_string(),
                    ],
                    prompt: [
                        "Primary goal:",
                        "- Land reducer-backed projections.",
                        "",
                        "Required context before coding:",
                        "- Read README.md.",
                        "",
                        "File ownership (only touch these paths):",
                        "- crates/wave-projections/src/lib.rs",
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
}
