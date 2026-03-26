//! Bootstrap operator snapshot assembly for the Wave workspace.
//!
//! This crate stays focused on mapping the reducer-backed projection spine plus
//! compatibility active-run details plus envelope-first proof state into a
//! transport snapshot for the operator surfaces, including the projection-owned
//! control-status payload that queue and control consumers share. It is a
//! landing zone for later control-plane and UI refinements, not a separate
//! source of truth.

use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;
use wave_config::ProjectConfig;
use wave_control_plane::ControlActionReadModel;
use wave_control_plane::ControlStatusReadModel;
use wave_control_plane::DashboardReadModel;
use wave_control_plane::OperatorSnapshotInputs;
use wave_control_plane::PlanningStatusReadModel;
use wave_control_plane::ProjectionSpine;
use wave_control_plane::QueueBlockerSummary;
use wave_control_plane::build_control_status_read_model_from_spine;
use wave_control_plane::build_projection_spine_from_authority;
use wave_dark_factory::lint_project;
use wave_dark_factory::validate_skill_catalog;
use wave_runtime::RerunIntentRecord;
use wave_runtime::active_closure_override_wave_ids;
use wave_runtime::list_rerun_intents;
use wave_runtime::list_closure_overrides;
use wave_runtime::load_latest_runs;
use wave_runtime::load_relevant_runs;
use wave_runtime::pending_rerun_wave_ids;
use wave_runtime::runtime_boundary_status;
use wave_domain::WaveClosureOverrideRecord;
use wave_spec::WaveAgent;
use wave_spec::WaveDocument;
use wave_spec::load_wave_documents;
use wave_trace::ReplayReport;
use wave_trace::WaveRunRecord;
use wave_trace::WaveRunStatus;
use wave_trace::now_epoch_ms;
use wave_trace::validate_replay;

/// Stable label for the snapshot assembly landing zone.
pub const SNAPSHOT_LANDING_ZONE: &str = "operator-snapshot-bootstrap";
const STALL_WARNING_AGE_MS: u128 = 5 * 60 * 1_000;
const STALL_THRESHOLD_AGE_MS: u128 = 15 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DashboardSnapshot {
    pub project_name: String,
    pub next_ready_wave_ids: Vec<u32>,
    pub active_runs: Vec<ActiveRunSnapshot>,
    pub total_waves: usize,
    pub completed_waves: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActiveRunSnapshot {
    pub wave_id: u32,
    pub run_id: String,
    pub status: String,
    pub agent_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorSnapshot {
    pub generated_at_ms: u128,
    pub dashboard: DashboardSnapshot,
    pub planning: PlanningStatusReadModel,
    pub control_status: ControlStatusReadModel,
    pub panels: OperatorPanelsSnapshot,
    pub launcher: LauncherStatus,
    pub latest_run_details: Vec<ActiveRunDetail>,
    pub active_run_details: Vec<ActiveRunDetail>,
    pub rerun_intents: Vec<RerunIntentRecord>,
    pub closure_overrides: Vec<WaveClosureOverrideRecord>,
    pub control_actions: Vec<ControlAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorPanelsSnapshot {
    pub run: RunPanelSnapshot,
    pub agents: AgentsPanelSnapshot,
    pub queue: QueuePanelSnapshot,
    pub control: ControlPanelSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunPanelSnapshot {
    pub active_wave_ids: Vec<u32>,
    pub active_run_ids: Vec<String>,
    pub active_run_count: usize,
    pub completed_run_count: usize,
    pub active_runs: Vec<ActiveRunDetail>,
    pub proof_complete_run_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentsPanelSnapshot {
    pub total_agents: usize,
    pub implementation_agents: usize,
    pub closure_agents: usize,
    pub required_closure_agents: Vec<String>,
    pub present_closure_agents: Vec<String>,
    pub missing_closure_agents: Vec<String>,
    pub agent_details: Vec<AgentPanelItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueuePanelSnapshot {
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
    pub waves: Vec<QueuePanelWaveSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueuePanelWaveSnapshot {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub queue_state: String,
    pub blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlPanelSnapshot {
    pub rerun_supported: bool,
    pub clear_rerun_supported: bool,
    pub launch_supported: bool,
    pub autonomous_supported: bool,
    pub launcher_required: bool,
    pub launcher_ready: bool,
    pub actions: Vec<ControlAction>,
    pub implemented_actions: Vec<ControlAction>,
    pub unavailable_actions: Vec<ControlAction>,
    pub unavailable_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActiveRunDetail {
    pub wave_id: u32,
    pub wave_slug: String,
    pub wave_title: String,
    pub run_id: String,
    pub status: WaveRunStatus,
    pub created_at_ms: u128,
    pub started_at_ms: Option<u128>,
    pub elapsed_ms: Option<u128>,
    pub current_agent_id: Option<String>,
    pub current_agent_title: Option<String>,
    pub activity_excerpt: String,
    pub last_activity_at_ms: Option<u128>,
    pub activity_source: Option<String>,
    pub stalled: bool,
    pub stall_reason: Option<String>,
    pub execution: wave_control_plane::WaveExecutionState,
    pub runtime_summary: RuntimeSummary,
    pub proof: ProofSnapshot,
    pub replay: ReplayReport,
    pub agents: Vec<AgentPanelItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentPanelItem {
    pub id: String,
    pub title: String,
    pub status: WaveRunStatus,
    pub current_task: String,
    pub proof_complete: bool,
    pub proof_source: String,
    pub expected_markers: Vec<String>,
    pub observed_markers: Vec<String>,
    pub missing_markers: Vec<String>,
    pub deliverables: Vec<String>,
    pub error: Option<String>,
    pub runtime: Option<RuntimeDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeSummary {
    pub selected_runtimes: Vec<String>,
    pub requested_runtimes: Vec<String>,
    pub selection_sources: Vec<String>,
    pub fallback_targets: Vec<String>,
    pub fallback_count: usize,
    pub agents_with_runtime: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeDetail {
    pub selected_runtime: String,
    pub selection_reason: String,
    pub policy: RuntimePolicyDetail,
    pub fallback: Option<RuntimeFallbackDetail>,
    pub execution_identity: RuntimeExecutionIdentityDetail,
    pub skill_projection: RuntimeSkillProjectionDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimePolicyDetail {
    pub requested_runtime: Option<String>,
    pub allowed_runtimes: Vec<String>,
    pub fallback_runtimes: Vec<String>,
    pub selection_source: Option<String>,
    pub uses_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeFallbackDetail {
    pub requested_runtime: String,
    pub selected_runtime: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeExecutionIdentityDetail {
    pub adapter: String,
    pub binary: String,
    pub provider: String,
    pub artifact_paths: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeSkillProjectionDetail {
    pub declared_skills: Vec<String>,
    pub projected_skills: Vec<String>,
    pub dropped_skills: Vec<String>,
    pub auto_attached_skills: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProofSnapshot {
    pub declared_artifacts: Vec<ProofArtifactStatus>,
    pub complete: bool,
    pub proof_source: String,
    pub completed_agents: usize,
    pub envelope_backed_agents: usize,
    pub compatibility_backed_agents: usize,
    pub total_agents: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProofArtifactStatus {
    pub path: String,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlAction {
    pub key: String,
    pub label: String,
    pub description: String,
    pub implemented: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LauncherStatus {
    pub executor_boundary: String,
    pub selection_policy: String,
    pub fallback_policy: String,
    pub available_runtimes: Vec<String>,
    pub unavailable_runtimes: Vec<String>,
    pub runtimes: Vec<wave_runtime::RuntimeAvailability>,
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RuntimeBoundaryPolicy {
    executor_boundary: String,
    selection_policy: String,
    fallback_policy: String,
    default_runtime: String,
    supported_runtimes: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RuntimePolicyContract {
    requested_runtime: Option<String>,
    allowed_runtimes: Vec<String>,
    fallback_runtimes: Vec<String>,
    selection_source: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RuntimePolicyContractResolution {
    label: String,
    proof_backing: String,
    contract: RuntimePolicyContract,
    selected_runtime: String,
    selection_reason: String,
    uses_fallback: bool,
    fallback: Option<RuntimeFallbackDetail>,
    execution_identity: RuntimeExecutionIdentityDetail,
    projected_skills: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct RuntimePolicyProofSurface {
    surface: String,
    backing: String,
    artifact_path: Option<String>,
    detail: String,
}

pub fn load_operator_snapshot(root: &Path, config: &ProjectConfig) -> Result<OperatorSnapshot> {
    let waves = load_wave_documents(config, root)?;
    let findings = lint_project(root, &waves);
    let skill_catalog_issues = validate_skill_catalog(root);
    let latest_runs = load_latest_runs(root, config)?;
    let rerun_wave_ids = pending_rerun_wave_ids(root, config)?;
    let closure_override_wave_ids = active_closure_override_wave_ids(root, config)?;
    let runtime_boundary = runtime_boundary_status();
    let launcher_ready = runtime_boundary
        .runtimes
        .iter()
        .any(|runtime| runtime.available);
    let spine = build_projection_spine_from_authority(
        root,
        config,
        &waves,
        &findings,
        &skill_catalog_issues,
        &latest_runs,
        &rerun_wave_ids,
        &closure_override_wave_ids,
        launcher_ready,
    )?;
    let rerun_intents = list_rerun_intents(root, config)?
        .into_values()
        .collect::<Vec<_>>();
    let closure_overrides = list_closure_overrides(root, config)?
        .into_values()
        .collect::<Vec<_>>();
    let relevant_runs = load_relevant_run_records(root, config)?;
    let latest_run_details = latest_relevant_run_details(root, &waves, &relevant_runs);
    let active_run_details = latest_run_details
        .iter()
        .filter(|run| matches!(run.status, WaveRunStatus::Planned | WaveRunStatus::Running))
        .cloned()
        .collect::<Vec<_>>();
    Ok(build_operator_snapshot(
        &spine,
        runtime_boundary,
        rerun_intents,
        closure_overrides,
        latest_run_details,
        active_run_details,
    )?)
}

pub fn load_relevant_run_records(
    root: &Path,
    config: &ProjectConfig,
) -> Result<HashMap<u32, WaveRunRecord>> {
    load_relevant_runs(root, config)
}

pub fn build_operator_snapshot(
    spine: &ProjectionSpine,
    runtime_boundary: wave_runtime::RuntimeBoundaryStatus,
    mut rerun_intents: Vec<RerunIntentRecord>,
    mut closure_overrides: Vec<WaveClosureOverrideRecord>,
    latest_run_details: Vec<ActiveRunDetail>,
    active_run_details: Vec<ActiveRunDetail>,
) -> Result<OperatorSnapshot> {
    rerun_intents.sort_by_key(|intent| intent.wave_id);
    closure_overrides.sort_by_key(|record| record.wave_id);
    let control_status = build_control_status_read_model_from_spine(spine);
    let control_actions = build_control_actions(&spine.operator.control.actions);
    let available_runtimes = runtime_boundary
        .runtimes
        .iter()
        .filter(|runtime| runtime.available)
        .map(|runtime| runtime.runtime.to_string())
        .collect::<Vec<_>>();
    let unavailable_runtimes = runtime_boundary
        .runtimes
        .iter()
        .filter(|runtime| !runtime.available)
        .map(|runtime| runtime.runtime.to_string())
        .collect::<Vec<_>>();
    let launcher = build_launcher_status(
        runtime_boundary,
        available_runtimes,
        unavailable_runtimes,
        spine.operator.control.launcher_ready,
    );
    let panels = build_operator_panels_snapshot(
        &spine.operator,
        active_run_details.clone(),
        control_actions.clone(),
    );

    Ok(OperatorSnapshot {
        generated_at_ms: now_epoch_ms()?,
        dashboard: build_dashboard_snapshot(&spine.operator.dashboard),
        planning: spine.planning.status.clone(),
        control_status,
        panels,
        launcher,
        latest_run_details,
        active_run_details,
        rerun_intents,
        closure_overrides,
        control_actions,
    })
}

fn build_operator_panels_snapshot(
    operator: &OperatorSnapshotInputs,
    active_run_details: Vec<ActiveRunDetail>,
    control_actions: Vec<ControlAction>,
) -> OperatorPanelsSnapshot {
    let active_wave_ids = operator.run.active_wave_ids.clone();
    let active_run_ids = active_run_details
        .iter()
        .map(|run| run.run_id.clone())
        .collect::<Vec<_>>();
    let agent_details = active_run_details
        .iter()
        .flat_map(|run| run.agents.clone())
        .collect::<Vec<_>>();
    let proof_complete_run_count = active_run_details
        .iter()
        .filter(|run| run.proof.complete)
        .count();
    OperatorPanelsSnapshot {
        run: RunPanelSnapshot {
            active_wave_ids,
            active_run_ids,
            active_run_count: operator.run.active_run_count,
            completed_run_count: operator.run.completed_run_count,
            active_runs: active_run_details,
            proof_complete_run_count,
        },
        agents: AgentsPanelSnapshot {
            total_agents: operator.agents.total_agents,
            implementation_agents: operator.agents.implementation_agents,
            closure_agents: operator.agents.closure_agents,
            required_closure_agents: operator.agents.required_closure_agents.clone(),
            present_closure_agents: operator.agents.present_closure_agents.clone(),
            missing_closure_agents: operator.agents.missing_closure_agents.clone(),
            agent_details,
        },
        queue: QueuePanelSnapshot {
            ready_wave_count: operator.queue.ready_wave_count,
            claimed_wave_count: operator.queue.claimed_wave_count,
            blocked_wave_count: operator.queue.blocked_wave_count,
            active_wave_count: operator.queue.active_wave_count,
            completed_wave_count: operator.queue.completed_wave_count,
            ready_wave_ids: operator.queue.ready_wave_ids.clone(),
            claimed_wave_ids: operator.queue.claimed_wave_ids.clone(),
            blocked_wave_ids: operator.queue.blocked_wave_ids.clone(),
            active_wave_ids: operator.queue.active_wave_ids.clone(),
            blocker_summary: operator.queue.blocker_summary.clone(),
            next_ready_wave_ids: operator.queue.next_ready_wave_ids.clone(),
            claimable_wave_ids: operator.queue.claimable_wave_ids.clone(),
            queue_ready: operator.queue.queue_ready,
            queue_ready_reason: operator.queue.queue_ready_reason.clone(),
            waves: operator
                .queue
                .waves
                .iter()
                .map(|wave| QueuePanelWaveSnapshot {
                    id: wave.id,
                    slug: wave.slug.clone(),
                    title: wave.title.clone(),
                    queue_state: wave.queue_state.clone(),
                    blocked: wave.blocked,
                })
                .collect(),
        },
        control: ControlPanelSnapshot {
            rerun_supported: operator.control.rerun_supported,
            clear_rerun_supported: operator.control.clear_rerun_supported,
            launch_supported: operator.control.launch_supported,
            autonomous_supported: operator.control.autonomous_supported,
            launcher_required: operator.control.launcher_required,
            launcher_ready: operator.control.launcher_ready,
            actions: control_actions.clone(),
            implemented_actions: control_actions
                .iter()
                .filter(|action| action.implemented)
                .cloned()
                .collect(),
            unavailable_actions: control_actions
                .iter()
                .filter(|action| !action.implemented)
                .cloned()
                .collect(),
            unavailable_reasons: operator.control.unavailable_reasons.clone(),
        },
    }
}

fn build_control_actions(actions: &[ControlActionReadModel]) -> Vec<ControlAction> {
    actions
        .iter()
        .map(|action| ControlAction {
            key: action.key.clone(),
            label: action.label.clone(),
            description: action.description.clone(),
            implemented: action.implemented,
        })
        .collect()
}

pub fn build_dashboard_snapshot(dashboard: &DashboardReadModel) -> DashboardSnapshot {
    DashboardSnapshot {
        project_name: dashboard.project_name.clone(),
        next_ready_wave_ids: dashboard.next_ready_wave_ids.clone(),
        active_runs: dashboard
            .active_runs
            .iter()
            .map(|run| ActiveRunSnapshot {
                wave_id: run.wave_id,
                run_id: run.run_id.clone(),
                status: run.status.clone(),
                agent_count: run.agent_count,
            })
            .collect(),
        total_waves: dashboard.total_waves,
        completed_waves: dashboard.completed_waves,
    }
}

pub fn latest_relevant_run_details(
    root: &Path,
    waves: &[WaveDocument],
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> Vec<ActiveRunDetail> {
    let mut details = latest_runs
        .values()
        .filter_map(|run| build_run_detail(root, waves, run))
        .collect::<Vec<_>>();
    details.sort_by_key(|detail| detail.wave_id);
    details
}

pub fn latest_relevant_run_detail(
    root: &Path,
    waves: &[WaveDocument],
    latest_runs: &HashMap<u32, WaveRunRecord>,
    wave_id: u32,
) -> Option<ActiveRunDetail> {
    latest_runs
        .get(&wave_id)
        .and_then(|run| build_run_detail(root, waves, run))
}

pub fn build_run_detail(
    root: &Path,
    waves: &[WaveDocument],
    run: &WaveRunRecord,
) -> Option<ActiveRunDetail> {
    let wave = waves.iter().find(|wave| wave.metadata.id == run.wave_id)?;
    let current_agent = current_agent(run);
    let activity = build_run_activity_status(root, run, current_agent);
    let proof = build_proof_snapshot(root, wave, run);
    let replay = validate_replay(run);
    let agents = run
        .agents
        .iter()
        .map(|agent| {
            let declared = wave
                .agents
                .iter()
                .find(|candidate| candidate.id == agent.id);
            build_agent_panel_item(root, run, agent, declared)
        })
        .collect::<Vec<_>>();

    Some(ActiveRunDetail {
        wave_id: run.wave_id,
        wave_slug: run.slug.clone(),
        wave_title: run.title.clone(),
        run_id: run.run_id.clone(),
        status: run.status,
        created_at_ms: run.created_at_ms,
        started_at_ms: run.started_at_ms,
        elapsed_ms: run.started_at_ms.and_then(|started_at_ms| {
            now_epoch_ms()
                .ok()
                .map(|now| now.saturating_sub(started_at_ms))
        }),
        current_agent_id: current_agent.map(|agent| agent.id.clone()),
        current_agent_title: current_agent.map(|agent| agent.title.clone()),
        activity_excerpt: activity.activity_excerpt,
        last_activity_at_ms: activity.last_activity_at_ms,
        activity_source: activity.activity_source,
        stalled: activity.stalled,
        stall_reason: activity.stall_reason,
        execution: build_run_execution_state(run),
        runtime_summary: build_runtime_summary(&agents),
        proof,
        replay,
        agents,
    })
}

fn build_run_execution_state(run: &WaveRunRecord) -> wave_control_plane::WaveExecutionState {
    wave_reducer::wave_execution_state_from_records(
        run.worktree.clone(),
        run.promotion.clone(),
        run.scheduling.clone(),
    )
}

#[derive(Debug, Clone)]
struct ResolvedEnvelopeProof {
    attempt_state: wave_domain::AttemptState,
    disposition: wave_domain::ResultDisposition,
    source: &'static str,
    required_final_markers: Vec<String>,
    observed_final_markers: Vec<String>,
    runtime: Option<wave_domain::RuntimeExecutionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunActivityStatus {
    activity_excerpt: String,
    last_activity_at_ms: Option<u128>,
    activity_source: Option<String>,
    stalled: bool,
    stall_reason: Option<String>,
}

fn resolve_effective_result_envelope(
    root: &Path,
    run: &WaveRunRecord,
    agent: &wave_trace::AgentRunRecord,
) -> Option<ResolvedEnvelopeProof> {
    wave_results::resolve_effective_result_envelope_view(root, run, agent)
        .ok()
        .map(|result| ResolvedEnvelopeProof {
            attempt_state: result.attempt_state,
            disposition: result.disposition,
            source: match result.source {
                wave_domain::ResultEnvelopeSource::Structured => "structured-envelope",
                wave_domain::ResultEnvelopeSource::LegacyMarkerAdapter => "compatibility-adapter",
            },
            required_final_markers: result.required_final_markers,
            observed_final_markers: result.observed_final_markers,
            runtime: result.runtime,
        })
}

fn build_agent_panel_item(
    root: &Path,
    run: &WaveRunRecord,
    agent: &wave_trace::AgentRunRecord,
    declared: Option<&WaveAgent>,
) -> AgentPanelItem {
    let effective = resolve_effective_result_envelope(root, run, agent);
    let final_markers = effective
        .as_ref()
        .map(|result| {
            wave_trace::FinalMarkerEnvelope::from_contract(
                result.required_final_markers.clone(),
                result.observed_final_markers.clone(),
            )
        })
        .unwrap_or_else(|| {
            wave_trace::FinalMarkerEnvelope::from_contract(
                agent.expected_markers.clone(),
                agent.observed_markers.clone(),
            )
        });
    let attempt_state = effective
        .as_ref()
        .map(|result| trace_attempt_state(result.attempt_state))
        .unwrap_or_else(|| wave_trace::AttemptState::from_run_status(agent.status, run.dry_run));
    let proof_source = effective
        .as_ref()
        .map(|result| result.source.to_string())
        .unwrap_or_else(|| "compatibility-run-record".to_string());
    let runtime = agent
        .runtime
        .clone()
        .or_else(|| effective.as_ref().and_then(|result| result.runtime.clone()))
        .map(runtime_detail_from_record);

    AgentPanelItem {
        id: agent.id.clone(),
        title: agent.title.clone(),
        status: agent.status,
        current_task: declared
            .map(|declared| declared.title.clone())
            .unwrap_or_else(|| agent.title.clone()),
        proof_complete: effective
            .as_ref()
            .map(|result| {
                matches!(
                    result.disposition,
                    wave_domain::ResultDisposition::Completed
                )
            })
            .unwrap_or(
                final_markers.missing.is_empty()
                    && matches!(attempt_state, wave_trace::AttemptState::Succeeded),
            ),
        proof_source,
        expected_markers: final_markers.required.clone(),
        observed_markers: final_markers.observed.clone(),
        missing_markers: final_markers.missing.clone(),
        deliverables: declared
            .map(|declared| declared.deliverables.clone())
            .unwrap_or_default(),
        error: agent.error.clone(),
        runtime,
    }
}

fn build_runtime_summary(agents: &[AgentPanelItem]) -> RuntimeSummary {
    let mut selected_runtimes = agents
        .iter()
        .filter_map(|agent| agent.runtime.as_ref())
        .map(|runtime| runtime.selected_runtime.clone())
        .collect::<Vec<_>>();
    selected_runtimes.sort();
    selected_runtimes.dedup();
    let mut requested_runtimes = agents
        .iter()
        .filter_map(|agent| agent.runtime.as_ref())
        .filter_map(|runtime| runtime.policy.requested_runtime.clone())
        .collect::<Vec<_>>();
    requested_runtimes.sort();
    requested_runtimes.dedup();
    let mut selection_sources = agents
        .iter()
        .filter_map(|agent| agent.runtime.as_ref())
        .filter_map(|runtime| runtime.policy.selection_source.clone())
        .collect::<Vec<_>>();
    selection_sources.sort();
    selection_sources.dedup();
    let mut fallback_targets = agents
        .iter()
        .filter_map(|agent| agent.runtime.as_ref())
        .filter_map(|runtime| runtime.fallback.as_ref())
        .map(|fallback| fallback.selected_runtime.clone())
        .collect::<Vec<_>>();
    fallback_targets.sort();
    fallback_targets.dedup();

    RuntimeSummary {
        selected_runtimes,
        requested_runtimes,
        selection_sources,
        fallback_targets,
        fallback_count: agents
            .iter()
            .filter(|agent| {
                agent
                    .runtime
                    .as_ref()
                    .and_then(|runtime| runtime.fallback.as_ref())
                    .is_some()
            })
            .count(),
        agents_with_runtime: agents
            .iter()
            .filter(|agent| agent.runtime.is_some())
            .count(),
    }
}

fn runtime_detail_from_record(record: wave_domain::RuntimeExecutionRecord) -> RuntimeDetail {
    let record = record.normalized();
    let uses_fallback = record.uses_fallback();
    let allowed_runtimes = policy_allowed_runtimes_request_first(&record.policy);
    RuntimeDetail {
        selected_runtime: record.selected_runtime.to_string(),
        selection_reason: record.selection_reason,
        policy: RuntimePolicyDetail {
            requested_runtime: record
                .policy
                .requested_runtime
                .map(|runtime| runtime.to_string()),
            allowed_runtimes,
            fallback_runtimes: record
                .policy
                .fallback_runtimes
                .iter()
                .map(ToString::to_string)
                .collect(),
            selection_source: record.policy.selection_source,
            uses_fallback,
        },
        fallback: record.fallback.map(|fallback| RuntimeFallbackDetail {
            requested_runtime: fallback.requested_runtime.to_string(),
            selected_runtime: fallback.selected_runtime.to_string(),
            reason: fallback.reason,
        }),
        execution_identity: RuntimeExecutionIdentityDetail {
            adapter: record.execution_identity.adapter,
            binary: record.execution_identity.binary,
            provider: record.execution_identity.provider,
            artifact_paths: record.execution_identity.artifact_paths,
        },
        skill_projection: RuntimeSkillProjectionDetail {
            declared_skills: record.skill_projection.declared_skills,
            projected_skills: record.skill_projection.projected_skills,
            dropped_skills: record.skill_projection.dropped_skills,
            auto_attached_skills: record.skill_projection.auto_attached_skills,
        },
    }
}

fn build_launcher_status(
    runtime_boundary: wave_runtime::RuntimeBoundaryStatus,
    available_runtimes: Vec<String>,
    unavailable_runtimes: Vec<String>,
    ready: bool,
) -> LauncherStatus {
    let policy = runtime_boundary_policy_from_status(&runtime_boundary);

    LauncherStatus {
        executor_boundary: policy.executor_boundary,
        selection_policy: format!(
            "{}; default runtime={}; supported adapters={}",
            policy.selection_policy,
            policy.default_runtime,
            policy.supported_runtimes.join(", ")
        ),
        fallback_policy: format!(
            "{}; fallback remains inside the explicit per-agent allowed runtime contract",
            policy.fallback_policy
        ),
        available_runtimes,
        unavailable_runtimes,
        runtimes: runtime_boundary.runtimes,
        ready,
    }
}

fn runtime_boundary_policy_from_status(
    runtime_boundary: &wave_runtime::RuntimeBoundaryStatus,
) -> RuntimeBoundaryPolicy {
    let mut supported_runtimes = runtime_boundary
        .runtimes
        .iter()
        .map(|runtime| runtime.runtime.to_string())
        .collect::<Vec<_>>();
    supported_runtimes.sort();
    supported_runtimes.dedup();

    RuntimeBoundaryPolicy {
        executor_boundary: runtime_boundary.executor_boundary.to_string(),
        selection_policy: runtime_boundary.selection_policy.to_string(),
        fallback_policy: runtime_boundary.fallback_policy.to_string(),
        default_runtime: wave_domain::RuntimeId::Codex.to_string(),
        supported_runtimes,
    }
}

fn runtime_policy_contract_from_record(
    record: &wave_domain::RuntimeExecutionRecord,
) -> RuntimePolicyContract {
    let record = record.normalized();
    RuntimePolicyContract {
        requested_runtime: record
            .policy
            .requested_runtime
            .map(|runtime| runtime.to_string()),
        allowed_runtimes: policy_allowed_runtimes_request_first(&record.policy),
        fallback_runtimes: record
            .policy
            .fallback_runtimes
            .iter()
            .map(ToString::to_string)
            .collect(),
        selection_source: record.policy.selection_source,
    }
}

fn policy_allowed_runtimes_request_first(
    policy: &wave_domain::RuntimeSelectionPolicy,
) -> Vec<String> {
    let normalized = policy.normalized();
    let mut allowed_runtimes = Vec::new();

    if let Some(requested_runtime) = normalized.requested_runtime {
        allowed_runtimes.push(requested_runtime.to_string());
    }
    for runtime in normalized.allowed_runtimes {
        let runtime = runtime.to_string();
        if !allowed_runtimes.iter().any(|existing| existing == &runtime) {
            allowed_runtimes.push(runtime);
        }
    }

    allowed_runtimes
}

#[allow(dead_code)]
fn runtime_policy_contract_resolution(
    label: impl Into<String>,
    proof_backing: impl Into<String>,
    record: wave_domain::RuntimeExecutionRecord,
) -> RuntimePolicyContractResolution {
    let contract = runtime_policy_contract_from_record(&record);
    let detail = runtime_detail_from_record(record);

    RuntimePolicyContractResolution {
        label: label.into(),
        proof_backing: proof_backing.into(),
        contract,
        selected_runtime: detail.selected_runtime.clone(),
        selection_reason: detail.selection_reason.clone(),
        uses_fallback: detail.policy.uses_fallback,
        fallback: detail.fallback.clone(),
        execution_identity: detail.execution_identity.clone(),
        projected_skills: detail.skill_projection.projected_skills.clone(),
    }
}

#[allow(dead_code)]
fn phase_3_runtime_policy_proof_surfaces() -> Vec<RuntimePolicyProofSurface> {
    vec![
        RuntimePolicyProofSurface {
            surface: "operator.latest_run_details.runtime".to_string(),
            backing: "live-transport".to_string(),
            artifact_path: None,
            detail: "latest and active run detail transport exposes selected runtime, policy, fallback, execution identity, and skill projection per agent".to_string(),
        },
        RuntimePolicyProofSurface {
            surface: "docs.phase-3.operator-runtime-transport".to_string(),
            backing: "fixture-backed".to_string(),
            artifact_path: Some(
                "docs/implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime/operator-runtime-transport.json".to_string(),
            ),
            detail: "the Wave 15 proof bundle shows one shared runtime contract resolved through codex-direct and claude-fallback paths".to_string(),
        },
    ]
}

fn build_proof_snapshot(root: &Path, wave: &WaveDocument, run: &WaveRunRecord) -> ProofSnapshot {
    let declared_artifacts = wave
        .metadata
        .proof
        .iter()
        .map(|path| ProofArtifactStatus {
            path: path.clone(),
            exists: root.join(path).exists(),
        })
        .collect::<Vec<_>>();
    let mut completed_agents = 0;
    let mut envelope_backed_agents = 0;
    let mut compatibility_backed_agents = 0;
    let mut agent_proof_complete = true;

    for agent in &run.agents {
        match resolve_effective_result_envelope(root, run, agent) {
            Some(result) => {
                if result.source == "structured-envelope" {
                    envelope_backed_agents += 1;
                } else {
                    compatibility_backed_agents += 1;
                }
                if matches!(
                    result.disposition,
                    wave_domain::ResultDisposition::Completed
                ) {
                    completed_agents += 1;
                }
                if !matches!(
                    result.disposition,
                    wave_domain::ResultDisposition::Completed
                ) {
                    agent_proof_complete = false;
                }
            }
            None => {
                compatibility_backed_agents += 1;
                if agent.status == WaveRunStatus::Succeeded {
                    completed_agents += 1;
                }
                let missing = agent
                    .expected_markers
                    .iter()
                    .filter(|marker| !agent.observed_markers.iter().any(|seen| seen == *marker))
                    .count();
                if missing > 0 || agent.status != WaveRunStatus::Succeeded {
                    agent_proof_complete = false;
                }
            }
        }
    }

    ProofSnapshot {
        complete: declared_artifacts.iter().all(|artifact| artifact.exists) && agent_proof_complete,
        proof_source: if compatibility_backed_agents == 0 {
            "structured-envelope".to_string()
        } else if envelope_backed_agents == 0 {
            "compatibility-adapter".to_string()
        } else {
            "mixed-envelope-and-compatibility".to_string()
        },
        declared_artifacts,
        completed_agents,
        envelope_backed_agents,
        compatibility_backed_agents,
        total_agents: run.agents.len(),
    }
}

fn trace_attempt_state(state: wave_domain::AttemptState) -> wave_trace::AttemptState {
    match state {
        wave_domain::AttemptState::Planned => wave_trace::AttemptState::Planned,
        wave_domain::AttemptState::Running => wave_trace::AttemptState::Running,
        wave_domain::AttemptState::Succeeded => wave_trace::AttemptState::Succeeded,
        wave_domain::AttemptState::Failed => wave_trace::AttemptState::Failed,
        wave_domain::AttemptState::Aborted => wave_trace::AttemptState::Aborted,
        wave_domain::AttemptState::Refused => wave_trace::AttemptState::Refused,
    }
}

fn current_agent(run: &WaveRunRecord) -> Option<&wave_trace::AgentRunRecord> {
    run.agents
        .iter()
        .find(|agent| agent.status == WaveRunStatus::Running)
        .or_else(|| {
            run.agents
                .iter()
                .find(|agent| agent.status == WaveRunStatus::Failed)
        })
        .or_else(|| {
            run.agents
                .iter()
                .rev()
                .find(|agent| agent.status == WaveRunStatus::Succeeded)
        })
        .or_else(|| {
            run.agents
                .iter()
                .find(|agent| agent.status == WaveRunStatus::Planned)
        })
}

fn build_run_activity_status(
    root: &Path,
    run: &WaveRunRecord,
    current_agent: Option<&wave_trace::AgentRunRecord>,
) -> RunActivityStatus {
    let Some(agent) = current_agent else {
        return RunActivityStatus {
            activity_excerpt: "No live agent output yet.".to_string(),
            last_activity_at_ms: None,
            activity_source: None,
            stalled: false,
            stall_reason: None,
        };
    };

    let candidates = [
        ("last-message", resolve_run_path(root, &agent.last_message_path)),
        ("events", resolve_run_path(root, &agent.events_path)),
        ("stderr", resolve_run_path(root, &agent.stderr_path)),
    ];
    let latest = candidates
        .iter()
        .filter_map(|(label, path)| {
            modified_at_ms(path).map(|modified_at_ms| ((*label).to_string(), path, modified_at_ms))
        })
        .max_by_key(|(_, _, modified_at_ms)| *modified_at_ms);

    let (activity_excerpt, activity_source, last_activity_at_ms) = if let Some((
        label,
        path,
        modified_at_ms,
    )) = latest
    {
        let excerpt = read_tail(path, 16).unwrap_or_else(|| "No live agent output yet.".to_string());
        (excerpt, Some(label), Some(modified_at_ms))
    } else {
        (
            "No live agent output yet.".to_string(),
            None,
            None,
        )
    };

    let active_run = matches!(run.status, WaveRunStatus::Running | WaveRunStatus::Planned);
    let stalled = if active_run {
        run_activity_age_ms(run, last_activity_at_ms)
            .map(|age_ms| age_ms >= STALL_THRESHOLD_AGE_MS)
            .unwrap_or(false)
    } else {
        false
    };
    let stall_reason = if stalled {
        run_activity_age_ms(run, last_activity_at_ms).map(|age_ms| {
            format!(
                "no {} activity for {}s",
                activity_source.as_deref().unwrap_or("agent"),
                age_ms / 1_000
            )
        })
    } else if active_run {
        run_activity_age_ms(run, last_activity_at_ms)
            .filter(|age_ms| *age_ms >= STALL_WARNING_AGE_MS)
            .map(|age_ms| format!("quiet for {}s", age_ms / 1_000))
    } else {
        None
    };

    RunActivityStatus {
        activity_excerpt,
        last_activity_at_ms,
        activity_source,
        stalled,
        stall_reason,
    }
}

fn run_activity_age_ms(run: &WaveRunRecord, last_activity_at_ms: Option<u128>) -> Option<u128> {
    let reference_at_ms = now_epoch_ms().ok()?;
    let anchor = last_activity_at_ms.or(run.started_at_ms).unwrap_or(run.created_at_ms);
    Some(reference_at_ms.saturating_sub(anchor))
}

fn resolve_run_path(root: &Path, path: &Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn modified_at_ms(path: &Path) -> Option<u128> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    modified.duration_since(UNIX_EPOCH).ok().map(|duration| duration.as_millis())
}

fn read_tail(path: &Path, max_lines: usize) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let lines = raw.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);
    Some(lines[start..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use wave_control_plane::PlanningProjectionBundle;
    use wave_control_plane::PlanningStatusReadModel;
    use wave_control_plane::PlanningStatusSummary;
    use wave_control_plane::QueueBlockerKindReadModel;
    use wave_control_plane::QueueBlockerReadModel;
    use wave_control_plane::QueueReadinessReadModel;
    use wave_control_plane::QueueReadinessStateReadModel;
    use wave_control_plane::SkillCatalogHealth;
    use wave_control_plane::WaveReadinessReadModel;
    use wave_control_plane::WaveStatusReadModel;
    use wave_control_plane::build_dashboard_read_model;
    use wave_spec::CompletionLevel;
    use wave_spec::ComponentPromotion;
    use wave_spec::Context7Defaults;
    use wave_spec::DeployEnvironment;
    use wave_spec::DocImpact;
    use wave_spec::DurabilityLevel;
    use wave_spec::ExitContract;
    use wave_spec::ProofLevel;
    use wave_spec::WaveMetadata;

    fn empty_ownership() -> wave_control_plane::WaveOwnershipState {
        wave_control_plane::WaveOwnershipState {
            claim: None,
            active_leases: Vec::new(),
            stale_leases: Vec::new(),
            contention_reasons: Vec::new(),
            blocked_by_owner: None,
            budget: wave_control_plane::SchedulerBudgetState {
                max_active_wave_claims: None,
                max_active_task_leases: None,
                reserved_closure_task_leases: None,
                active_wave_claims: 0,
                active_task_leases: 0,
                active_implementation_task_leases: 0,
                active_closure_task_leases: 0,
                closure_capacity_reserved: false,
                preemption_enabled: false,
                budget_blocked: false,
            },
        }
    }

    fn empty_execution() -> wave_control_plane::WaveExecutionState {
        wave_control_plane::WaveExecutionState {
            worktree: None,
            promotion: None,
            scheduling: None,
            merge_blocked: false,
            closure_blocked_by_promotion: false,
        }
    }

    fn runtime_boundary_fixture() -> wave_runtime::RuntimeBoundaryStatus {
        wave_runtime::RuntimeBoundaryStatus {
            executor_boundary: "runtime-neutral launch spec plus adapter registry in wave-runtime",
            selection_policy: "explicit executor runtime selection with default codex and authored fallback order",
            fallback_policy: "fallback only when the selected runtime is unavailable before meaningful work starts",
            runtimes: vec![
                wave_runtime::RuntimeAvailability {
                    runtime: wave_domain::RuntimeId::Codex,
                    binary: "/tmp/fake-codex".to_string(),
                    available: true,
                    detail: "available".to_string(),
                },
                wave_runtime::RuntimeAvailability {
                    runtime: wave_domain::RuntimeId::Claude,
                    binary: "/tmp/fake-claude".to_string(),
                    available: false,
                    detail: "not authenticated".to_string(),
                },
            ],
        }
    }

    fn runtime_record_fixture(
        selected_runtime: wave_domain::RuntimeId,
        fallback: Option<wave_domain::RuntimeFallbackRecord>,
        bundle_name: &str,
    ) -> wave_domain::RuntimeExecutionRecord {
        let runtime_name = selected_runtime.as_str();
        let provider = match selected_runtime {
            wave_domain::RuntimeId::Codex => "openai-codex-cli",
            wave_domain::RuntimeId::Claude => "anthropic-claude-code",
            wave_domain::RuntimeId::Opencode => "opencode",
            wave_domain::RuntimeId::Local => "local",
        };
        let prompt_key = if selected_runtime == wave_domain::RuntimeId::Claude {
            "system_prompt"
        } else {
            "runtime_prompt"
        };
        let prompt_path = if selected_runtime == wave_domain::RuntimeId::Claude {
            format!(".wave/state/build/specs/{bundle_name}/agents/A1/claude-system-prompt.txt")
        } else {
            format!(".wave/state/build/specs/{bundle_name}/agents/A1/runtime-prompt.md")
        };

        wave_domain::RuntimeExecutionRecord {
            policy: wave_domain::RuntimeSelectionPolicy {
                requested_runtime: Some(wave_domain::RuntimeId::Codex),
                allowed_runtimes: vec![
                    wave_domain::RuntimeId::Codex,
                    wave_domain::RuntimeId::Claude,
                ],
                fallback_runtimes: vec![wave_domain::RuntimeId::Claude],
                selection_source: Some("executor.id".to_string()),
            },
            selected_runtime,
            selection_reason: fallback
                .as_ref()
                .map(|fallback| {
                    format!(
                        "selected {} after fallback because {}",
                        fallback.selected_runtime, fallback.reason
                    )
                })
                .unwrap_or_else(|| format!("selected {runtime_name} from executor.id")),
            fallback,
            execution_identity: wave_domain::RuntimeExecutionIdentity {
                runtime: selected_runtime,
                adapter: format!("wave-runtime/{runtime_name}"),
                binary: format!("/tmp/fake-{runtime_name}"),
                provider: provider.to_string(),
                artifact_paths: BTreeMap::from([
                    (
                        "runtime_detail".to_string(),
                        format!(
                            ".wave/state/build/specs/{bundle_name}/agents/A1/runtime-detail.json"
                        ),
                    ),
                    (prompt_key.to_string(), prompt_path),
                ]),
            },
            skill_projection: wave_domain::RuntimeSkillProjection {
                declared_skills: vec!["wave-core".to_string()],
                projected_skills: vec!["wave-core".to_string(), format!("runtime-{runtime_name}")],
                dropped_skills: Vec::new(),
                auto_attached_skills: vec![format!("runtime-{runtime_name}")],
            },
        }
    }

    #[test]
    fn dashboard_snapshot_counts_completed_waves() {
        let status = PlanningStatusReadModel {
            project_name: "Test".to_string(),
            default_mode: wave_config::ExecutionMode::DarkFactory,
            summary: PlanningStatusSummary {
                total_waves: 2,
                ready_waves: 1,
                blocked_waves: 0,
                active_waves: 1,
                completed_waves: 1,
                total_agents: 12,
                implementation_agents: 6,
                closure_agents: 6,
                waves_with_complete_closure: 2,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
            skill_catalog: SkillCatalogHealth {
                ok: true,
                issue_count: 0,
                issues: Vec::new(),
            },
            queue: QueueReadinessReadModel {
                next_ready_wave_ids: vec![2],
                next_ready_wave_id: Some(2),
                claimable_wave_ids: vec![2],
                claimed_wave_ids: Vec::new(),
                ready_wave_count: 1,
                claimed_wave_count: 0,
                blocked_wave_count: 0,
                active_wave_count: 1,
                completed_wave_count: 1,
                queue_ready: true,
                queue_ready_reason: "ready waves are available to claim".to_string(),
            },
            next_ready_wave_ids: vec![2],
            waves: vec![
                WaveStatusReadModel {
                    id: 0,
                    slug: "zero".to_string(),
                    title: "Zero".to_string(),
                    depends_on: Vec::new(),
                    blocked_by: vec!["already-completed".to_string()],
                    blocker_state: Vec::new(),
                    lint_errors: 0,
                    ready: false,
                    ownership: empty_ownership(),
                    execution: empty_execution(),
                    agent_count: 6,
                    implementation_agent_count: 3,
                    closure_agent_count: 3,
                    closure_complete: true,
                    required_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    present_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    missing_closure_agents: Vec::new(),
                    readiness: WaveReadinessReadModel {
                        state: QueueReadinessStateReadModel::Completed,
                        planning_ready: false,
                        claimable: false,
                        reasons: vec![QueueBlockerReadModel {
                            kind: QueueBlockerKindReadModel::AlreadyCompleted,
                            raw: "already-completed".to_string(),
                            detail: None,
                        }],
                        primary_reason: Some(QueueBlockerReadModel {
                            kind: QueueBlockerKindReadModel::AlreadyCompleted,
                            raw: "already-completed".to_string(),
                            detail: None,
                        }),
                    },
                    rerun_requested: false,
                    closure_override_applied: false,
                    completed: true,
                    last_run_status: Some(WaveRunStatus::Succeeded),
                },
                WaveStatusReadModel {
                    id: 2,
                    slug: "two".to_string(),
                    title: "Two".to_string(),
                    depends_on: vec![0],
                    blocked_by: Vec::new(),
                    blocker_state: Vec::new(),
                    lint_errors: 0,
                    ready: true,
                    ownership: empty_ownership(),
                    execution: empty_execution(),
                    agent_count: 6,
                    implementation_agent_count: 3,
                    closure_agent_count: 3,
                    closure_complete: true,
                    required_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    present_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    missing_closure_agents: Vec::new(),
                    readiness: WaveReadinessReadModel {
                        state: QueueReadinessStateReadModel::Ready,
                        planning_ready: true,
                        claimable: true,
                        reasons: Vec::new(),
                        primary_reason: None,
                    },
                    rerun_requested: false,
                    closure_override_applied: false,
                    completed: false,
                    last_run_status: None,
                },
            ],
            has_errors: false,
        };
        let latest_runs = HashMap::from([(
            2,
            WaveRunRecord {
                run_id: "wave-2-1".to_string(),
                wave_id: 2,
                slug: "two".to_string(),
                title: "Two".to_string(),
                status: WaveRunStatus::Running,
                dry_run: false,
                bundle_dir: PathBuf::from(".wave/state/build/specs/wave-2"),
                trace_path: PathBuf::from(".wave/traces/runs/wave-2.json"),
                codex_home: PathBuf::from(".wave/codex"),
                created_at_ms: 1,
                started_at_ms: Some(1),
                launcher_pid: None,
                launcher_started_at_ms: None,
                worktree: None,
                promotion: None,
                scheduling: None,
                completed_at_ms: None,
                agents: Vec::new(),
                error: None,
            },
        )]);

        let dashboard = build_dashboard_read_model(&status, &latest_runs);
        let snapshot = build_dashboard_snapshot(&dashboard);
        assert_eq!(snapshot.completed_waves, 1);
        assert_eq!(snapshot.active_runs.len(), 1);
    }

    #[test]
    fn operator_snapshot_exposes_control_plane_truth() {
        let status = PlanningStatusReadModel {
            project_name: "Test".to_string(),
            default_mode: wave_config::ExecutionMode::DarkFactory,
            summary: PlanningStatusSummary {
                total_waves: 1,
                ready_waves: 1,
                blocked_waves: 0,
                active_waves: 0,
                completed_waves: 0,
                total_agents: 3,
                implementation_agents: 1,
                closure_agents: 2,
                waves_with_complete_closure: 1,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
            skill_catalog: SkillCatalogHealth {
                ok: true,
                issue_count: 0,
                issues: Vec::new(),
            },
            queue: QueueReadinessReadModel {
                next_ready_wave_ids: vec![7],
                next_ready_wave_id: Some(7),
                claimable_wave_ids: vec![7],
                claimed_wave_ids: Vec::new(),
                ready_wave_count: 1,
                claimed_wave_count: 0,
                blocked_wave_count: 0,
                active_wave_count: 0,
                completed_wave_count: 0,
                queue_ready: true,
                queue_ready_reason: "ready waves are available to claim".to_string(),
            },
            next_ready_wave_ids: vec![7],
            waves: vec![wave_control_plane::WaveStatusReadModel {
                id: 7,
                slug: "seven".to_string(),
                title: "Seven".to_string(),
                depends_on: Vec::new(),
                blocked_by: Vec::new(),
                blocker_state: vec![QueueBlockerReadModel {
                    kind: wave_control_plane::QueueBlockerKindReadModel::Other,
                    raw: "none".to_string(),
                    detail: Some("none".to_string()),
                }],
                lint_errors: 0,
                ready: true,
                ownership: empty_ownership(),
                execution: empty_execution(),
                agent_count: 3,
                implementation_agent_count: 1,
                closure_agent_count: 2,
                closure_complete: true,
                required_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                present_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                missing_closure_agents: Vec::new(),
                readiness: WaveReadinessReadModel {
                    state: QueueReadinessStateReadModel::Ready,
                    planning_ready: true,
                    claimable: true,
                    reasons: Vec::new(),
                    primary_reason: None,
                },
                rerun_requested: false,
                closure_override_applied: false,
                completed: false,
                last_run_status: None,
            }],
            has_errors: false,
        };
        let projection = wave_control_plane::build_planning_status_projection(&status);
        let planning = PlanningProjectionBundle {
            status: status.clone(),
            projection,
        };
        let operator =
            wave_control_plane::build_operator_snapshot_inputs(&planning, &HashMap::new(), false);
        let spine = ProjectionSpine { planning, operator };
        let snapshot = build_operator_snapshot(
            &spine,
            runtime_boundary_fixture(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        assert_eq!(
            snapshot.control_status,
            build_control_status_read_model_from_spine(&spine)
        );
        assert_eq!(
            snapshot.dashboard.next_ready_wave_ids,
            spine.operator.dashboard.next_ready_wave_ids
        );
        assert!(snapshot.panels.queue.queue_ready);
        assert_eq!(
            snapshot.panels.queue.queue_ready_reason,
            "ready waves are available to claim"
        );
        assert_eq!(
            snapshot.panels.queue.claimable_wave_ids,
            snapshot.control_status.queue_decision.claimable_wave_ids
        );
        assert_eq!(
            snapshot.panels.queue.blocker_summary,
            snapshot.control_status.queue_decision.blocker_summary
        );
        assert_eq!(
            snapshot.control_status.queue_decision.lines[0],
            "queue decision: next claimable wave=7"
        );
        assert_eq!(
            snapshot.panels.control.unavailable_reasons,
            vec!["no supported runtime is ready"]
        );
        assert_eq!(snapshot.panels.control.implemented_actions.len(), 5);
        assert_eq!(snapshot.panels.control.unavailable_actions.len(), 2);
        assert_eq!(snapshot.panels.control.unavailable_actions[0].key, "launch");
        assert_eq!(snapshot.panels.control.actions.len(), 7);
        assert_eq!(snapshot.panels.queue.waves[0].queue_state, "ready");
        assert_eq!(
            snapshot.launcher.selection_policy,
            "explicit executor runtime selection with default codex and authored fallback order; default runtime=codex; supported adapters=claude, codex"
        );
        assert_eq!(
            snapshot.launcher.fallback_policy,
            "fallback only when the selected runtime is unavailable before meaningful work starts; fallback remains inside the explicit per-agent allowed runtime contract"
        );
        assert_eq!(
            snapshot.panels.queue.waves[0].id,
            spine.operator.queue.waves[0].id
        );
        assert_eq!(
            snapshot.panels.queue.waves[0].queue_state,
            spine.operator.queue.waves[0].queue_state
        );
    }

    #[test]
    fn operator_snapshot_carries_manual_close_override_records() {
        let status = PlanningStatusReadModel {
            project_name: "Test".to_string(),
            default_mode: wave_config::ExecutionMode::DarkFactory,
            summary: PlanningStatusSummary {
                total_waves: 1,
                ready_waves: 0,
                blocked_waves: 0,
                active_waves: 0,
                completed_waves: 1,
                total_agents: 3,
                implementation_agents: 1,
                closure_agents: 2,
                waves_with_complete_closure: 1,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
            skill_catalog: SkillCatalogHealth {
                ok: true,
                issue_count: 0,
                issues: Vec::new(),
            },
            queue: QueueReadinessReadModel {
                next_ready_wave_ids: Vec::new(),
                next_ready_wave_id: None,
                claimable_wave_ids: Vec::new(),
                claimed_wave_ids: Vec::new(),
                ready_wave_count: 0,
                claimed_wave_count: 0,
                blocked_wave_count: 0,
                active_wave_count: 0,
                completed_wave_count: 1,
                queue_ready: false,
                queue_ready_reason: "no waves are ready to claim".to_string(),
            },
            next_ready_wave_ids: Vec::new(),
            waves: vec![wave_control_plane::WaveStatusReadModel {
                id: 15,
                slug: "wave-15".to_string(),
                title: "Wave 15".to_string(),
                depends_on: Vec::new(),
                blocked_by: vec!["already-completed".to_string()],
                blocker_state: vec![QueueBlockerReadModel {
                    kind: QueueBlockerKindReadModel::AlreadyCompleted,
                    raw: "already-completed".to_string(),
                    detail: None,
                }],
                lint_errors: 0,
                ready: false,
                ownership: empty_ownership(),
                execution: empty_execution(),
                agent_count: 3,
                implementation_agent_count: 1,
                closure_agent_count: 2,
                closure_complete: true,
                required_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                present_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                missing_closure_agents: Vec::new(),
                readiness: WaveReadinessReadModel {
                    state: QueueReadinessStateReadModel::Completed,
                    planning_ready: false,
                    claimable: false,
                    reasons: vec![QueueBlockerReadModel {
                        kind: QueueBlockerKindReadModel::AlreadyCompleted,
                        raw: "already-completed".to_string(),
                        detail: None,
                    }],
                    primary_reason: Some(QueueBlockerReadModel {
                        kind: QueueBlockerKindReadModel::AlreadyCompleted,
                        raw: "already-completed".to_string(),
                        detail: None,
                    }),
                },
                rerun_requested: false,
                closure_override_applied: true,
                completed: true,
                last_run_status: Some(WaveRunStatus::Failed),
            }],
            has_errors: false,
        };
        let projection = wave_control_plane::build_planning_status_projection(&status);
        let planning = PlanningProjectionBundle {
            status: status.clone(),
            projection,
        };
        let operator =
            wave_control_plane::build_operator_snapshot_inputs(&planning, &HashMap::new(), true);
        let spine = ProjectionSpine { planning, operator };
        let override_record = wave_domain::WaveClosureOverrideRecord {
            override_id: "closure-override-wave-15".to_string(),
            wave_id: 15,
            status: wave_domain::WaveClosureOverrideStatus::Applied,
            reason: "manual close accepted".to_string(),
            requested_by: "operator".to_string(),
            source_run_id: "wave-15-failed".to_string(),
            evidence_paths: vec!["docs/implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime/README.md".to_string()],
            detail: Some("promotion conflict reviewed and accepted".to_string()),
            applied_at_ms: 42,
            cleared_at_ms: None,
        };

        let snapshot = build_operator_snapshot(
            &spine,
            runtime_boundary_fixture(),
            Vec::new(),
            vec![override_record.clone()],
            Vec::new(),
            Vec::new(),
        )
        .expect("build operator snapshot");

        assert_eq!(snapshot.closure_overrides, vec![override_record]);
        assert!(snapshot.planning.waves[0].closure_override_applied);
    }

    #[test]
    fn operator_snapshot_preserves_projection_owned_queue_states() {
        let status = PlanningStatusReadModel {
            project_name: "Test".to_string(),
            default_mode: wave_config::ExecutionMode::DarkFactory,
            summary: PlanningStatusSummary {
                total_waves: 3,
                ready_waves: 0,
                blocked_waves: 1,
                active_waves: 1,
                completed_waves: 1,
                total_agents: 9,
                implementation_agents: 3,
                closure_agents: 6,
                waves_with_complete_closure: 3,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
            skill_catalog: SkillCatalogHealth {
                ok: true,
                issue_count: 0,
                issues: Vec::new(),
            },
            queue: QueueReadinessReadModel {
                next_ready_wave_ids: Vec::new(),
                next_ready_wave_id: None,
                claimable_wave_ids: Vec::new(),
                claimed_wave_ids: Vec::new(),
                ready_wave_count: 0,
                claimed_wave_count: 0,
                blocked_wave_count: 1,
                active_wave_count: 1,
                completed_wave_count: 1,
                queue_ready: false,
                queue_ready_reason: "no waves are ready to claim".to_string(),
            },
            next_ready_wave_ids: Vec::new(),
            waves: vec![
                WaveStatusReadModel {
                    id: 5,
                    slug: "active".to_string(),
                    title: "Active".to_string(),
                    depends_on: Vec::new(),
                    blocked_by: vec!["active-run:running".to_string()],
                    blocker_state: vec![QueueBlockerReadModel {
                        kind: QueueBlockerKindReadModel::ActiveRun,
                        raw: "active-run:running".to_string(),
                        detail: Some("wave is already active".to_string()),
                    }],
                    lint_errors: 0,
                    ready: false,
                    ownership: empty_ownership(),
                    execution: empty_execution(),
                    agent_count: 3,
                    implementation_agent_count: 1,
                    closure_agent_count: 2,
                    closure_complete: true,
                    required_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    present_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    missing_closure_agents: Vec::new(),
                    readiness: WaveReadinessReadModel {
                        state: QueueReadinessStateReadModel::Active,
                        planning_ready: false,
                        claimable: false,
                        reasons: vec![QueueBlockerReadModel {
                            kind: QueueBlockerKindReadModel::ActiveRun,
                            raw: "active-run:running".to_string(),
                            detail: Some("wave is already active".to_string()),
                        }],
                        primary_reason: Some(QueueBlockerReadModel {
                            kind: QueueBlockerKindReadModel::ActiveRun,
                            raw: "active-run:running".to_string(),
                            detail: Some("wave is already active".to_string()),
                        }),
                    },
                    rerun_requested: false,
                    closure_override_applied: false,
                    completed: false,
                    last_run_status: Some(WaveRunStatus::Running),
                },
                WaveStatusReadModel {
                    id: 6,
                    slug: "blocked".to_string(),
                    title: "Blocked".to_string(),
                    depends_on: vec![5],
                    blocked_by: vec!["wave:5".to_string()],
                    blocker_state: vec![QueueBlockerReadModel {
                        kind: QueueBlockerKindReadModel::Dependency,
                        raw: "wave:5".to_string(),
                        detail: Some("5".to_string()),
                    }],
                    lint_errors: 0,
                    ready: false,
                    ownership: empty_ownership(),
                    execution: empty_execution(),
                    agent_count: 3,
                    implementation_agent_count: 1,
                    closure_agent_count: 2,
                    closure_complete: true,
                    required_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    present_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    missing_closure_agents: Vec::new(),
                    readiness: WaveReadinessReadModel {
                        state: QueueReadinessStateReadModel::Blocked,
                        planning_ready: false,
                        claimable: false,
                        reasons: vec![QueueBlockerReadModel {
                            kind: QueueBlockerKindReadModel::Dependency,
                            raw: "wave:5".to_string(),
                            detail: Some("5".to_string()),
                        }],
                        primary_reason: Some(QueueBlockerReadModel {
                            kind: QueueBlockerKindReadModel::Dependency,
                            raw: "wave:5".to_string(),
                            detail: Some("5".to_string()),
                        }),
                    },
                    rerun_requested: false,
                    closure_override_applied: false,
                    completed: false,
                    last_run_status: None,
                },
                WaveStatusReadModel {
                    id: 7,
                    slug: "completed".to_string(),
                    title: "Completed".to_string(),
                    depends_on: Vec::new(),
                    blocked_by: vec!["already-completed".to_string()],
                    blocker_state: vec![QueueBlockerReadModel {
                        kind: QueueBlockerKindReadModel::AlreadyCompleted,
                        raw: "already-completed".to_string(),
                        detail: None,
                    }],
                    lint_errors: 0,
                    ready: false,
                    ownership: empty_ownership(),
                    execution: empty_execution(),
                    agent_count: 3,
                    implementation_agent_count: 1,
                    closure_agent_count: 2,
                    closure_complete: true,
                    required_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    present_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    missing_closure_agents: Vec::new(),
                    readiness: WaveReadinessReadModel {
                        state: QueueReadinessStateReadModel::Completed,
                        planning_ready: false,
                        claimable: false,
                        reasons: vec![QueueBlockerReadModel {
                            kind: QueueBlockerKindReadModel::AlreadyCompleted,
                            raw: "already-completed".to_string(),
                            detail: None,
                        }],
                        primary_reason: Some(QueueBlockerReadModel {
                            kind: QueueBlockerKindReadModel::AlreadyCompleted,
                            raw: "already-completed".to_string(),
                            detail: None,
                        }),
                    },
                    rerun_requested: false,
                    closure_override_applied: false,
                    completed: true,
                    last_run_status: Some(WaveRunStatus::Succeeded),
                },
            ],
            has_errors: false,
        };
        let projection = wave_control_plane::build_planning_status_projection(&status);
        let planning = PlanningProjectionBundle { status, projection };
        let operator =
            wave_control_plane::build_operator_snapshot_inputs(&planning, &HashMap::new(), true);
        let spine = ProjectionSpine { planning, operator };
        let snapshot = build_operator_snapshot(
            &spine,
            runtime_boundary_fixture(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        let queue_states = snapshot
            .panels
            .queue
            .waves
            .iter()
            .map(|wave| wave.queue_state.clone())
            .collect::<Vec<_>>();
        let spine_queue_states = spine
            .operator
            .queue
            .waves
            .iter()
            .map(|wave| wave.queue_state.clone())
            .collect::<Vec<_>>();

        assert_eq!(queue_states, vec!["active", "blocked: wave:5", "completed"]);
        assert_eq!(queue_states, spine_queue_states);
        assert_eq!(
            snapshot.control_status.queue_decision.blocker_summary,
            spine.operator.queue.blocker_summary
        );
    }

    #[test]
    fn build_run_detail_prefers_structured_result_envelope_for_proof() {
        let root = std::env::temp_dir().join(format!(
            "wave-app-server-proof-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        let bundle_dir = root.join(".wave/state/build/specs/wave-12-1");
        let agent_dir = bundle_dir.join("agents/A1");
        let trace_path = root.join(".wave/traces/runs/wave-12-1.json");
        let envelope_path =
            root.join(".wave/state/results/wave-12/attempt-a1/agent_result_envelope.json");
        std::fs::create_dir_all(&agent_dir).expect("agent dir");
        std::fs::create_dir_all(trace_path.parent().expect("trace parent")).expect("trace dir");
        std::fs::create_dir_all(envelope_path.parent().expect("envelope parent"))
            .expect("envelope dir");
        std::fs::create_dir_all(root.join(".wave/codex")).expect("codex dir");
        std::fs::write(root.join("README.md"), "proof\n").expect("write proof");
        std::fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        std::fs::write(agent_dir.join("last-message.txt"), "[wave-proof]\n")
            .expect("write message");
        std::fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        std::fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");

        wave_trace::write_result_envelope(
            &envelope_path,
            &wave_trace::ResultEnvelopeRecord {
                result_envelope_id: "result:wave-12-1:a1".to_string(),
                wave_id: 12,
                task_id: "wave-12:agent-a1".to_string(),
                attempt_id: "attempt-a1".to_string(),
                agent_id: "A1".to_string(),
                task_role: "implementation".to_string(),
                closure_role: None,
                source: wave_trace::ResultEnvelopeSource::Structured,
                attempt_state: wave_trace::AttemptState::Succeeded,
                disposition: wave_trace::ResultDisposition::Completed,
                summary: Some("structured".to_string()),
                output_text: Some("[wave-proof]".to_string()),
                final_markers: wave_trace::FinalMarkerEnvelope::from_contract(
                    vec!["[wave-proof]".to_string()],
                    vec!["[wave-proof]".to_string()],
                ),
                proof_bundle_ids: Vec::new(),
                fact_ids: Vec::new(),
                contradiction_ids: Vec::new(),
                artifacts: Vec::new(),
                doc_delta: wave_trace::DocDeltaEnvelope::default(),
                marker_evidence: Vec::new(),
                closure: wave_trace::ClosureState {
                    disposition: wave_trace::ClosureDisposition::Ready,
                    required_final_markers: vec!["[wave-proof]".to_string()],
                    observed_final_markers: vec!["[wave-proof]".to_string()],
                    blocking_reasons: Vec::new(),
                    satisfied_fact_ids: Vec::new(),
                    contradiction_ids: Vec::new(),
                    verdict: wave_trace::ClosureVerdictPayload::None,
                },
                runtime: None,
                created_at_ms: 3,
            },
        )
        .expect("write envelope");

        let wave = WaveDocument {
            path: PathBuf::from("waves/12.md"),
            metadata: WaveMetadata {
                id: 12,
                slug: "result-envelope".to_string(),
                title: "Result Envelope".to_string(),
                mode: wave_config::ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["README.md".to_string()],
            },
            heading_title: Some("Wave 12".to_string()),
            commit_message: Some("Feat: result envelope".to_string()),
            component_promotions: vec![ComponentPromotion {
                component: "result-envelope".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some("Structured results".to_string()),
            }),
            agents: vec![WaveAgent {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                role_prompts: Vec::new(),
                executor: std::collections::BTreeMap::new(),
                context7: Some(Context7Defaults {
                    bundle: "none".to_string(),
                    query: Some("noop".to_string()),
                }),
                skills: Vec::new(),
                components: Vec::new(),
                capabilities: Vec::new(),
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Contract,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Unit,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec!["README.md".to_string()],
                file_ownership: vec!["README.md".to_string()],
                final_markers: vec!["[wave-proof]".to_string()],
                prompt: "Primary goal:\n- noop".to_string(),
            }],
        };
        let run = WaveRunRecord {
            run_id: "wave-12-1".to_string(),
            wave_id: 12,
            slug: "result-envelope".to_string(),
            title: "Result Envelope".to_string(),
            status: WaveRunStatus::Succeeded,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: trace_path.clone(),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(3),
            agents: vec![wave_trace::AgentRunRecord {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                status: WaveRunStatus::Succeeded,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: agent_dir.join("last-message.txt"),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: Some(envelope_path),
                runtime_detail_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: Some(0),
                error: None,
                runtime: None,
            }],
            error: None,
        };
        wave_trace::write_trace_bundle(&trace_path, &run).expect("write trace");

        let detail = build_run_detail(&root, &[wave], &run).expect("run detail");

        assert!(detail.proof.complete);
        assert_eq!(detail.proof.proof_source, "structured-envelope");
        assert_eq!(detail.proof.envelope_backed_agents, 1);
        assert_eq!(detail.proof.compatibility_backed_agents, 0);
        assert_eq!(detail.agents[0].proof_source, "structured-envelope");
        assert!(detail.agents[0].proof_complete);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn build_run_detail_uses_compatibility_adapter_for_owned_closure_artifacts() {
        let root = std::env::temp_dir().join(format!(
            "wave-app-server-legacy-proof-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        let bundle_dir = root.join(".wave/state/build/specs/wave-12-1");
        let agent_dir = bundle_dir.join("agents/A8");
        let trace_path = root.join(".wave/traces/runs/wave-12-1.json");
        std::fs::create_dir_all(&agent_dir).expect("agent dir");
        std::fs::create_dir_all(root.join(".wave/integration")).expect("integration dir");
        std::fs::create_dir_all(trace_path.parent().expect("trace parent")).expect("trace dir");
        std::fs::create_dir_all(root.join(".wave/codex")).expect("codex dir");
        std::fs::write(root.join("README.md"), "proof\n").expect("write proof");
        std::fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        std::fs::write(agent_dir.join("last-message.txt"), "summary only\n")
            .expect("write last message");
        std::fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        std::fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");
        std::fs::write(
            root.join(".wave/integration/wave-12.md"),
            "# Wave 12 Integration Summary\n\n[wave-integration] state=ready-for-doc-closure claims=4 conflicts=0 blockers=0 detail=owned integration summary closes the seam\n",
        )
        .expect("write integration summary");

        let wave = WaveDocument {
            path: PathBuf::from("waves/12.md"),
            metadata: WaveMetadata {
                id: 12,
                slug: "result-envelope".to_string(),
                title: "Result Envelope".to_string(),
                mode: wave_config::ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["README.md".to_string()],
            },
            heading_title: Some("Wave 12".to_string()),
            commit_message: Some("Feat: result envelope".to_string()),
            component_promotions: vec![ComponentPromotion {
                component: "result-envelope".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some("Structured results".to_string()),
            }),
            agents: vec![WaveAgent {
                id: "A8".to_string(),
                title: "Integration".to_string(),
                role_prompts: Vec::new(),
                executor: std::collections::BTreeMap::new(),
                context7: Some(Context7Defaults {
                    bundle: "none".to_string(),
                    query: Some("noop".to_string()),
                }),
                skills: Vec::new(),
                components: Vec::new(),
                capabilities: Vec::new(),
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Contract,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Unit,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec![".wave/integration/wave-12.md".to_string()],
                file_ownership: vec![".wave/integration/wave-12.md".to_string()],
                final_markers: vec!["[wave-integration]".to_string()],
                prompt: "Primary goal:\n- noop".to_string(),
            }],
        };
        let run = WaveRunRecord {
            run_id: "wave-12-1".to_string(),
            wave_id: 12,
            slug: "result-envelope".to_string(),
            title: "Result Envelope".to_string(),
            status: WaveRunStatus::Succeeded,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: trace_path.clone(),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(3),
            agents: vec![wave_trace::AgentRunRecord {
                id: "A8".to_string(),
                title: "Integration".to_string(),
                status: WaveRunStatus::Succeeded,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: agent_dir.join("last-message.txt"),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: None,
                expected_markers: vec!["[wave-integration]".to_string()],
                observed_markers: Vec::new(),
                exit_code: Some(0),
                error: None,
                runtime: None,
            }],
            error: None,
        };
        wave_trace::write_trace_bundle(&trace_path, &run).expect("write trace");

        let detail = build_run_detail(&root, &[wave], &run).expect("run detail");

        assert!(detail.proof.complete);
        assert_eq!(detail.proof.proof_source, "compatibility-adapter");
        assert_eq!(detail.proof.envelope_backed_agents, 0);
        assert_eq!(detail.proof.compatibility_backed_agents, 1);
        assert_eq!(detail.agents[0].proof_source, "compatibility-adapter");
        assert!(detail.agents[0].proof_complete);
        assert_eq!(
            detail.agents[0].observed_markers,
            vec!["[wave-integration]".to_string()]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn latest_relevant_run_detail_surfaces_failed_structured_proof() {
        let root = std::env::temp_dir().join(format!(
            "wave-app-server-failed-proof-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        let bundle_dir = root.join(".wave/state/build/specs/wave-12-2");
        let agent_dir = bundle_dir.join("agents/A1");
        let trace_path = root.join(".wave/traces/runs/wave-12-2.json");
        let envelope_path =
            root.join(".wave/state/results/wave-12/attempt-a1-failed/agent_result_envelope.json");
        std::fs::create_dir_all(&agent_dir).expect("agent dir");
        std::fs::create_dir_all(trace_path.parent().expect("trace parent")).expect("trace dir");
        std::fs::create_dir_all(envelope_path.parent().expect("envelope dir"))
            .expect("envelope dir");
        std::fs::create_dir_all(root.join(".wave/codex")).expect("codex dir");
        std::fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        std::fs::write(
            agent_dir.join("last-message.txt"),
            "attempt failed\n[wave-proof]\n",
        )
        .expect("write last message");
        std::fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        std::fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");

        wave_trace::write_result_envelope(
            &envelope_path,
            &wave_trace::ResultEnvelopeRecord {
                result_envelope_id: "result:wave-12-2:a1".to_string(),
                wave_id: 12,
                task_id: "wave-12:agent-a1".to_string(),
                attempt_id: "attempt-a1-failed".to_string(),
                agent_id: "A1".to_string(),
                task_role: "implementation".to_string(),
                closure_role: None,
                source: wave_trace::ResultEnvelopeSource::Structured,
                attempt_state: wave_trace::AttemptState::Failed,
                disposition: wave_trace::ResultDisposition::Failed,
                summary: Some("structured failure".to_string()),
                output_text: Some("attempt failed\n[wave-proof]".to_string()),
                final_markers: wave_trace::FinalMarkerEnvelope::from_contract(
                    vec!["[wave-proof]".to_string()],
                    vec!["[wave-proof]".to_string()],
                ),
                proof_bundle_ids: Vec::new(),
                fact_ids: Vec::new(),
                contradiction_ids: Vec::new(),
                artifacts: Vec::new(),
                doc_delta: wave_trace::DocDeltaEnvelope::default(),
                marker_evidence: Vec::new(),
                closure: wave_trace::ClosureState {
                    disposition: wave_trace::ClosureDisposition::Blocked,
                    required_final_markers: vec!["[wave-proof]".to_string()],
                    observed_final_markers: vec!["[wave-proof]".to_string()],
                    blocking_reasons: vec!["structured failure".to_string()],
                    satisfied_fact_ids: Vec::new(),
                    contradiction_ids: Vec::new(),
                    verdict: wave_trace::ClosureVerdictPayload::None,
                },
                runtime: None,
                created_at_ms: 4,
            },
        )
        .expect("write envelope");

        let wave = WaveDocument {
            path: PathBuf::from("waves/12.md"),
            metadata: WaveMetadata {
                id: 12,
                slug: "result-envelope".to_string(),
                title: "Result Envelope".to_string(),
                mode: wave_config::ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["README.md".to_string()],
            },
            heading_title: Some("Wave 12".to_string()),
            commit_message: Some("Feat: result envelope".to_string()),
            component_promotions: vec![ComponentPromotion {
                component: "result-envelope".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some("Structured results".to_string()),
            }),
            agents: vec![WaveAgent {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                role_prompts: Vec::new(),
                executor: std::collections::BTreeMap::new(),
                context7: Some(Context7Defaults {
                    bundle: "none".to_string(),
                    query: Some("noop".to_string()),
                }),
                skills: Vec::new(),
                components: Vec::new(),
                capabilities: Vec::new(),
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Contract,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Unit,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec!["README.md".to_string()],
                file_ownership: vec!["README.md".to_string()],
                final_markers: vec!["[wave-proof]".to_string()],
                prompt: "Primary goal:\n- noop".to_string(),
            }],
        };
        let run = WaveRunRecord {
            run_id: "wave-12-2".to_string(),
            wave_id: 12,
            slug: "result-envelope".to_string(),
            title: "Result Envelope".to_string(),
            status: WaveRunStatus::Failed,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: trace_path.clone(),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 2,
            started_at_ms: Some(3),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(4),
            agents: vec![wave_trace::AgentRunRecord {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                status: WaveRunStatus::Failed,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: agent_dir.join("last-message.txt"),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: Some(envelope_path),
                runtime_detail_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: Some(1),
                error: Some("structured failure".to_string()),
                runtime: None,
            }],
            error: Some("structured failure".to_string()),
        };
        wave_trace::write_trace_bundle(&trace_path, &run).expect("write trace");

        let detail = latest_relevant_run_detail(&root, &[wave], &HashMap::from([(12, run)]), 12)
            .expect("failed run detail");

        assert_eq!(detail.status, WaveRunStatus::Failed);
        assert_eq!(detail.proof.proof_source, "structured-envelope");
        assert!(!detail.proof.complete);
        assert_eq!(detail.agents[0].proof_source, "structured-envelope");
        assert!(!detail.agents[0].proof_complete);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn build_run_detail_carries_execution_state_for_transport() {
        let root = std::env::temp_dir().join(format!(
            "wave-app-server-execution-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        let bundle_dir = root.join(".wave/state/build/specs/wave-14-1");
        let agent_dir = bundle_dir.join("agents/A1");
        let trace_path = root.join(".wave/traces/runs/wave-14-1.json");
        std::fs::create_dir_all(&agent_dir).expect("agent dir");
        std::fs::create_dir_all(trace_path.parent().expect("trace parent")).expect("trace dir");
        std::fs::create_dir_all(root.join(".wave/codex")).expect("codex dir");
        std::fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        std::fs::write(agent_dir.join("last-message.txt"), "[wave-proof]\n")
            .expect("write message");
        std::fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        std::fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");

        let wave = WaveDocument {
            path: PathBuf::from("waves/14.md"),
            metadata: WaveMetadata {
                id: 14,
                slug: "parallel-wave".to_string(),
                title: "Parallel Wave".to_string(),
                mode: wave_config::ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: Vec::new(),
            },
            heading_title: Some("Wave 14".to_string()),
            commit_message: Some("Feat: parallel wave".to_string()),
            component_promotions: vec![ComponentPromotion {
                component: "parallel-wave".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some("Parallel wave execution".to_string()),
            }),
            agents: vec![WaveAgent {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                role_prompts: Vec::new(),
                executor: std::collections::BTreeMap::new(),
                context7: Some(Context7Defaults {
                    bundle: "none".to_string(),
                    query: Some("noop".to_string()),
                }),
                skills: Vec::new(),
                components: Vec::new(),
                capabilities: Vec::new(),
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Contract,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Unit,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec!["src/lib.rs".to_string()],
                file_ownership: vec!["src/lib.rs".to_string()],
                final_markers: vec!["[wave-proof]".to_string()],
                prompt: "Primary goal:\n- noop".to_string(),
            }],
        };
        let worktree = wave_domain::WaveWorktreeRecord {
            worktree_id: wave_domain::WaveWorktreeId::new("worktree-wave-14".to_string()),
            wave_id: 14,
            path: ".wave/state/worktrees/wave-14".to_string(),
            base_ref: "HEAD".to_string(),
            snapshot_ref: "refs/wave/snapshot/14".to_string(),
            branch_ref: Some("wave/14/test".to_string()),
            shared_scope: wave_domain::WaveWorktreeScope::Wave,
            state: wave_domain::WaveWorktreeState::Allocated,
            allocated_at_ms: 10,
            released_at_ms: None,
            detail: Some("shared wave worktree".to_string()),
        };
        let promotion = wave_domain::WavePromotionRecord {
            promotion_id: wave_domain::WavePromotionId::new("promotion-wave-14".to_string()),
            wave_id: 14,
            worktree_id: Some(worktree.worktree_id.clone()),
            state: wave_domain::WavePromotionState::Conflicted,
            target_ref: "HEAD".to_string(),
            snapshot_ref: "refs/wave/snapshot/14".to_string(),
            candidate_ref: Some("refs/wave/candidate/14".to_string()),
            candidate_tree: Some("deadbeef".to_string()),
            conflict_paths: vec!["src/lib.rs".to_string()],
            detail: Some("merge validation found a conflict".to_string()),
            checked_at_ms: 11,
            completed_at_ms: Some(12),
        };
        let scheduling = wave_domain::WaveSchedulingRecord {
            wave_id: 14,
            phase: wave_domain::WaveExecutionPhase::Promotion,
            priority: wave_domain::WaveSchedulerPriority::Closure,
            state: wave_domain::WaveSchedulingState::Protected,
            fairness_rank: 2,
            waiting_since_ms: Some(9),
            protected_closure_capacity: true,
            preemptible: false,
            last_decision: Some("promotion is merge-blocked".to_string()),
            updated_at_ms: 12,
        };
        let run = WaveRunRecord {
            run_id: "wave-14-1".to_string(),
            wave_id: 14,
            slug: "parallel-wave".to_string(),
            title: "Parallel Wave".to_string(),
            status: WaveRunStatus::Failed,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: trace_path.clone(),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: Some(worktree.clone()),
            promotion: Some(promotion.clone()),
            scheduling: Some(scheduling.clone()),
            completed_at_ms: Some(12),
            agents: vec![wave_trace::AgentRunRecord {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                status: WaveRunStatus::Failed,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: agent_dir.join("last-message.txt"),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: vec!["[wave-proof]".to_string()],
                exit_code: Some(1),
                error: Some("merge validation found a conflict".to_string()),
                runtime: None,
            }],
            error: Some("merge validation found a conflict".to_string()),
        };
        wave_trace::write_trace_bundle(&trace_path, &run).expect("write trace");

        let detail = build_run_detail(&root, &[wave], &run).expect("run detail");
        let expected_execution = wave_reducer::wave_execution_state_from_records(
            Some(worktree),
            Some(promotion),
            Some(scheduling),
        );
        assert_eq!(detail.execution, expected_execution);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn build_run_detail_uses_effective_runtime_for_summary_and_agent_transport() {
        let root = std::env::temp_dir().join(format!(
            "wave-app-server-runtime-transport-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        let bundle_dir = root.join(".wave/state/build/specs/wave-15-1");
        let agent_dir = bundle_dir.join("agents/A1");
        let trace_path = root.join(".wave/traces/runs/wave-15-1.json");
        let envelope_path =
            root.join(".wave/state/results/wave-15/attempt-a1/agent_result_envelope.json");
        std::fs::create_dir_all(&agent_dir).expect("agent dir");
        std::fs::create_dir_all(trace_path.parent().expect("trace parent")).expect("trace dir");
        std::fs::create_dir_all(envelope_path.parent().expect("envelope parent"))
            .expect("envelope dir");
        std::fs::create_dir_all(root.join(".wave/codex")).expect("codex dir");
        std::fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        std::fs::write(agent_dir.join("last-message.txt"), "[wave-proof]\n")
            .expect("write message");
        std::fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        std::fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");

        let runtime = runtime_record_fixture(
            wave_domain::RuntimeId::Claude,
            Some(wave_domain::RuntimeFallbackRecord {
                requested_runtime: wave_domain::RuntimeId::Codex,
                selected_runtime: wave_domain::RuntimeId::Claude,
                reason: "codex login status reported unavailable".to_string(),
            }),
            "wave-15-1",
        );

        wave_trace::write_result_envelope(
            &envelope_path,
            &wave_trace::ResultEnvelopeRecord {
                result_envelope_id: "result:wave-15-1:a1".to_string(),
                wave_id: 15,
                task_id: "wave-15:agent-a1".to_string(),
                attempt_id: "attempt-a1".to_string(),
                agent_id: "A1".to_string(),
                task_role: "implementation".to_string(),
                closure_role: None,
                source: wave_trace::ResultEnvelopeSource::Structured,
                attempt_state: wave_trace::AttemptState::Succeeded,
                disposition: wave_trace::ResultDisposition::Completed,
                summary: Some("runtime detail persisted".to_string()),
                output_text: Some("[wave-proof]".to_string()),
                final_markers: wave_trace::FinalMarkerEnvelope::from_contract(
                    vec!["[wave-proof]".to_string()],
                    vec!["[wave-proof]".to_string()],
                ),
                proof_bundle_ids: Vec::new(),
                fact_ids: Vec::new(),
                contradiction_ids: Vec::new(),
                artifacts: Vec::new(),
                doc_delta: wave_trace::DocDeltaEnvelope::default(),
                marker_evidence: Vec::new(),
                closure: wave_trace::ClosureState {
                    disposition: wave_trace::ClosureDisposition::Ready,
                    required_final_markers: vec!["[wave-proof]".to_string()],
                    observed_final_markers: vec!["[wave-proof]".to_string()],
                    blocking_reasons: Vec::new(),
                    satisfied_fact_ids: Vec::new(),
                    contradiction_ids: Vec::new(),
                    verdict: wave_trace::ClosureVerdictPayload::None,
                },
                runtime: Some(runtime.clone()),
                created_at_ms: 3,
            },
        )
        .expect("write envelope");

        let wave = WaveDocument {
            path: PathBuf::from("waves/15.md"),
            metadata: WaveMetadata {
                id: 15,
                slug: "runtime-policy".to_string(),
                title: "Runtime Policy".to_string(),
                mode: wave_config::ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: Vec::new(),
            },
            heading_title: Some("Wave 15".to_string()),
            commit_message: Some("Feat: runtime policy".to_string()),
            component_promotions: vec![ComponentPromotion {
                component: "runtime-policy".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some("Runtime detail transport".to_string()),
            }),
            agents: vec![WaveAgent {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                role_prompts: Vec::new(),
                executor: std::collections::BTreeMap::new(),
                context7: Some(Context7Defaults {
                    bundle: "none".to_string(),
                    query: Some("noop".to_string()),
                }),
                skills: vec!["wave-core".to_string()],
                components: Vec::new(),
                capabilities: Vec::new(),
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Contract,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Unit,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec!["README.md".to_string()],
                file_ownership: vec!["README.md".to_string()],
                final_markers: vec!["[wave-proof]".to_string()],
                prompt: "Primary goal:\n- noop".to_string(),
            }],
        };
        let run = WaveRunRecord {
            run_id: "wave-15-1".to_string(),
            wave_id: 15,
            slug: "runtime-policy".to_string(),
            title: "Runtime Policy".to_string(),
            status: WaveRunStatus::Succeeded,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: trace_path.clone(),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(3),
            agents: vec![wave_trace::AgentRunRecord {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                status: WaveRunStatus::Succeeded,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: agent_dir.join("last-message.txt"),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: Some(envelope_path),
                runtime_detail_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: Some(0),
                error: None,
                runtime: None,
            }],
            error: None,
        };
        wave_trace::write_trace_bundle(&trace_path, &run).expect("write trace");

        let detail = build_run_detail(&root, &[wave], &run).expect("run detail");

        assert_eq!(
            detail.runtime_summary.selected_runtimes,
            vec!["claude".to_string()]
        );
        assert_eq!(
            detail.runtime_summary.requested_runtimes,
            vec!["codex".to_string()]
        );
        assert_eq!(
            detail.runtime_summary.selection_sources,
            vec!["executor.id".to_string()]
        );
        assert_eq!(
            detail.runtime_summary.fallback_targets,
            vec!["claude".to_string()]
        );
        assert_eq!(detail.runtime_summary.fallback_count, 1);
        assert_eq!(detail.runtime_summary.agents_with_runtime, 1);
        let runtime_detail = detail.agents[0].runtime.as_ref().expect("agent runtime");
        assert_eq!(runtime_detail.selected_runtime, "claude");
        assert_eq!(
            runtime_detail.policy.requested_runtime.as_deref(),
            Some("codex")
        );
        assert_eq!(
            runtime_detail.policy.allowed_runtimes,
            vec!["codex".to_string(), "claude".to_string()]
        );
        assert_eq!(
            runtime_detail.policy.fallback_runtimes,
            vec!["claude".to_string()]
        );
        assert_eq!(
            runtime_detail.policy.selection_source.as_deref(),
            Some("executor.id")
        );
        assert!(runtime_detail.policy.uses_fallback);
        assert_eq!(
            runtime_detail.execution_identity.adapter,
            "wave-runtime/claude"
        );
        assert_eq!(
            runtime_detail.skill_projection.projected_skills,
            vec!["wave-core".to_string(), "runtime-claude".to_string()]
        );
        assert_eq!(
            runtime_detail
                .fallback
                .as_ref()
                .expect("fallback detail")
                .requested_runtime,
            "codex"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn build_run_detail_marks_stalled_active_runs() {
        let root = std::env::temp_dir().join(format!(
            "wave-app-server-stalled-run-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        let bundle_dir = root.join(".wave/state/build/specs/wave-15-stalled");
        let agent_dir = bundle_dir.join("agents/A1");
        std::fs::create_dir_all(&agent_dir).expect("agent dir");
        std::fs::create_dir_all(root.join(".wave/codex")).expect("codex dir");
        std::fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");

        let wave = WaveDocument {
            path: PathBuf::from("waves/15.md"),
            metadata: WaveMetadata {
                id: 15,
                slug: "runtime-policy".to_string(),
                title: "Runtime Policy".to_string(),
                mode: wave_config::ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: Vec::new(),
                rollback: Vec::new(),
                proof: Vec::new(),
            },
            heading_title: Some("Wave 15".to_string()),
            commit_message: Some("Feat: runtime policy".to_string()),
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![WaveAgent {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                role_prompts: Vec::new(),
                executor: std::collections::BTreeMap::new(),
                context7: None,
                skills: Vec::new(),
                components: Vec::new(),
                capabilities: Vec::new(),
                exit_contract: None,
                deliverables: Vec::new(),
                file_ownership: vec!["README.md".to_string()],
                final_markers: vec!["[wave-proof]".to_string()],
                prompt: "Primary goal:\n- noop".to_string(),
            }],
        };
        let started_at_ms =
            wave_trace::now_epoch_ms().expect("timestamp") - (16_u128 * 60 * 1_000);
        let run = WaveRunRecord {
            run_id: "wave-15-stalled".to_string(),
            wave_id: 15,
            slug: "runtime-policy".to_string(),
            title: "Runtime Policy".to_string(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir,
            trace_path: root.join(".wave/traces/runs/wave-15-stalled.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: started_at_ms.saturating_sub(1_000),
            started_at_ms: Some(started_at_ms),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: vec![wave_trace::AgentRunRecord {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                status: WaveRunStatus::Running,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: agent_dir.join("last-message.txt"),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: None,
                error: None,
                runtime: None,
            }],
            error: None,
        };

        let detail = build_run_detail(&root, &[wave], &run).expect("run detail");

        assert!(detail.stalled);
        assert_eq!(detail.activity_source, None);
        assert!(detail.last_activity_at_ms.is_none());
        assert!(
            detail
                .stall_reason
                .as_deref()
                .expect("stall reason")
                .contains("no agent activity")
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn runtime_policy_contract_resolution_keeps_one_contract_across_codex_and_claude_paths() {
        let codex = runtime_policy_contract_resolution(
            "codex-direct",
            "fixture-backed",
            runtime_record_fixture(wave_domain::RuntimeId::Codex, None, "wave-15-proof-codex"),
        );
        let claude = runtime_policy_contract_resolution(
            "claude-fallback",
            "fixture-backed",
            runtime_record_fixture(
                wave_domain::RuntimeId::Claude,
                Some(wave_domain::RuntimeFallbackRecord {
                    requested_runtime: wave_domain::RuntimeId::Codex,
                    selected_runtime: wave_domain::RuntimeId::Claude,
                    reason: "codex login status reported unavailable".to_string(),
                }),
                "wave-15-proof-claude",
            ),
        );

        assert_eq!(codex.contract.requested_runtime.as_deref(), Some("codex"));
        assert_eq!(codex.contract, claude.contract);
        assert_eq!(
            codex.contract.allowed_runtimes,
            vec!["codex".to_string(), "claude".to_string()]
        );
        assert_eq!(codex.contract.fallback_runtimes, vec!["claude".to_string()]);
        assert_eq!(codex.selected_runtime, "codex");
        assert!(!codex.uses_fallback);
        assert_eq!(claude.selected_runtime, "claude");
        assert!(claude.uses_fallback);
        assert_eq!(
            claude
                .fallback
                .as_ref()
                .expect("fallback detail")
                .requested_runtime,
            "codex"
        );
        assert_eq!(claude.proof_backing, "fixture-backed");
    }

    #[test]
    fn runtime_detail_from_record_normalizes_policy_contract_and_skill_projection() {
        let mut record = runtime_record_fixture(wave_domain::RuntimeId::Codex, None, "wave-15-1");
        record.policy.allowed_runtimes = vec![
            wave_domain::RuntimeId::Claude,
            wave_domain::RuntimeId::Codex,
            wave_domain::RuntimeId::Claude,
        ];
        record.policy.fallback_runtimes = vec![
            wave_domain::RuntimeId::Claude,
            wave_domain::RuntimeId::Claude,
        ];
        record.skill_projection.projected_skills = vec![
            "wave-core".to_string(),
            "runtime-codex".to_string(),
            "runtime-codex".to_string(),
        ];
        record.skill_projection.auto_attached_skills =
            vec!["runtime-codex".to_string(), "runtime-codex".to_string()];

        let detail = runtime_detail_from_record(record);

        assert_eq!(
            detail.policy.allowed_runtimes,
            vec!["codex".to_string(), "claude".to_string()]
        );
        assert_eq!(detail.policy.fallback_runtimes, vec!["claude".to_string()]);
        assert_eq!(
            detail.skill_projection.projected_skills,
            vec!["wave-core".to_string(), "runtime-codex".to_string()]
        );
        assert_eq!(
            detail.skill_projection.auto_attached_skills,
            vec!["runtime-codex".to_string()]
        );
    }

    #[test]
    #[ignore = "writes the Wave 14 active-run transport proof artifact"]
    fn export_phase_2_parallel_wave_active_run_transport_snapshot() {
        #[derive(Debug, Serialize)]
        struct ActiveRunTransportBundle {
            generated_at_ms: u128,
            latest_run_details: Vec<ActiveRunDetail>,
            active_run_details: Vec<ActiveRunDetail>,
        }

        let root = std::env::temp_dir().join(format!(
            "wave-app-server-transport-proof-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        let bundle_dir = root.join(".wave/state/build/specs/wave-14-transport");
        let agent_dir = bundle_dir.join("agents/A1");
        let trace_path = root.join(".wave/traces/runs/wave-14-transport.json");
        std::fs::create_dir_all(&agent_dir).expect("agent dir");
        std::fs::create_dir_all(trace_path.parent().expect("trace parent")).expect("trace dir");
        std::fs::create_dir_all(root.join(".wave/codex")).expect("codex dir");
        std::fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        std::fs::write(agent_dir.join("last-message.txt"), "[wave-proof]\n")
            .expect("write message");
        std::fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        std::fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");

        let wave = WaveDocument {
            path: PathBuf::from("waves/14.md"),
            metadata: WaveMetadata {
                id: 14,
                slug: "parallel-wave".to_string(),
                title: "Parallel Wave".to_string(),
                mode: wave_config::ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: Vec::new(),
            },
            heading_title: Some("Wave 14".to_string()),
            commit_message: Some("Feat: parallel wave".to_string()),
            component_promotions: vec![ComponentPromotion {
                component: "parallel-wave".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some("Parallel wave execution".to_string()),
            }),
            agents: vec![WaveAgent {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                role_prompts: Vec::new(),
                executor: std::collections::BTreeMap::new(),
                context7: Some(Context7Defaults {
                    bundle: "none".to_string(),
                    query: Some("noop".to_string()),
                }),
                skills: Vec::new(),
                components: Vec::new(),
                capabilities: Vec::new(),
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Contract,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Unit,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec!["src/lib.rs".to_string()],
                file_ownership: vec!["src/lib.rs".to_string()],
                final_markers: vec!["[wave-proof]".to_string()],
                prompt: "Primary goal:\n- noop".to_string(),
            }],
        };
        let worktree = wave_domain::WaveWorktreeRecord {
            worktree_id: wave_domain::WaveWorktreeId::new("worktree-wave-14".to_string()),
            wave_id: 14,
            path: ".wave/state/worktrees/wave-14".to_string(),
            base_ref: "HEAD".to_string(),
            snapshot_ref: "refs/wave/snapshot/14".to_string(),
            branch_ref: Some("wave/14/test".to_string()),
            shared_scope: wave_domain::WaveWorktreeScope::Wave,
            state: wave_domain::WaveWorktreeState::Allocated,
            allocated_at_ms: 10,
            released_at_ms: None,
            detail: Some("shared wave worktree".to_string()),
        };
        let promotion = wave_domain::WavePromotionRecord {
            promotion_id: wave_domain::WavePromotionId::new("promotion-wave-14".to_string()),
            wave_id: 14,
            worktree_id: Some(worktree.worktree_id.clone()),
            state: wave_domain::WavePromotionState::Ready,
            target_ref: "HEAD".to_string(),
            snapshot_ref: "refs/wave/snapshot/14".to_string(),
            candidate_ref: Some("refs/wave/candidate/14".to_string()),
            candidate_tree: Some("deadbeef".to_string()),
            conflict_paths: Vec::new(),
            detail: Some("candidate passed merge validation".to_string()),
            checked_at_ms: 11,
            completed_at_ms: Some(12),
        };
        let scheduling = wave_domain::WaveSchedulingRecord {
            wave_id: 14,
            phase: wave_domain::WaveExecutionPhase::Closure,
            priority: wave_domain::WaveSchedulerPriority::Closure,
            state: wave_domain::WaveSchedulingState::Running,
            fairness_rank: 1,
            waiting_since_ms: Some(9),
            protected_closure_capacity: true,
            preemptible: false,
            last_decision: Some("running A8 in shared wave worktree".to_string()),
            updated_at_ms: 12,
        };
        let run = WaveRunRecord {
            run_id: "wave-14-transport".to_string(),
            wave_id: 14,
            slug: "parallel-wave".to_string(),
            title: "Parallel Wave".to_string(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: trace_path.clone(),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: Some(worktree),
            promotion: Some(promotion),
            scheduling: Some(scheduling),
            completed_at_ms: None,
            agents: vec![wave_trace::AgentRunRecord {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                status: WaveRunStatus::Running,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: agent_dir.join("last-message.txt"),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: None,
                error: None,
                runtime: None,
            }],
            error: None,
        };
        wave_trace::write_trace_bundle(&trace_path, &run).expect("write trace");

        let latest_run_details = latest_relevant_run_details(
            &root,
            std::slice::from_ref(&wave),
            &HashMap::from([(14, run)]),
        );
        let active_run_details = latest_run_details
            .iter()
            .filter(|detail| {
                matches!(
                    detail.status,
                    WaveRunStatus::Planned | WaveRunStatus::Running
                )
            })
            .cloned()
            .collect::<Vec<_>>();

        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let proof_dir =
            workspace_root.join("docs/implementation/live-proofs/phase-2-parallel-wave-execution");
        std::fs::create_dir_all(&proof_dir).expect("create proof dir");
        std::fs::write(
            proof_dir.join("active-run-detail-transport.json"),
            serde_json::to_string_pretty(&ActiveRunTransportBundle {
                generated_at_ms: wave_trace::now_epoch_ms().expect("transport proof timestamp"),
                latest_run_details,
                active_run_details,
            })
            .expect("serialize transport proof"),
        )
        .expect("write transport proof");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "writes the Wave 15 operator runtime transport proof artifact"]
    fn export_phase_3_runtime_policy_operator_transport_snapshot() {
        #[derive(Debug, Serialize)]
        struct RuntimeTransportBundle {
            generated_at_ms: u128,
            latest_run_details: Vec<ActiveRunDetail>,
            active_run_details: Vec<ActiveRunDetail>,
            runtime_boundary_policy: RuntimeBoundaryPolicy,
            proof_surfaces: Vec<RuntimePolicyProofSurface>,
            policy_contract_resolutions: Vec<RuntimePolicyContractResolution>,
        }

        let root = std::env::temp_dir().join(format!(
            "wave-app-server-runtime-proof-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        let bundle_dir = root.join(".wave/state/build/specs/wave-15-proof");
        let agent_dir = bundle_dir.join("agents/A1");
        let trace_path = root.join(".wave/traces/runs/wave-15-proof.json");
        let envelope_path =
            root.join(".wave/state/results/wave-15/attempt-a1-proof/agent_result_envelope.json");
        std::fs::create_dir_all(&agent_dir).expect("agent dir");
        std::fs::create_dir_all(trace_path.parent().expect("trace parent")).expect("trace dir");
        std::fs::create_dir_all(envelope_path.parent().expect("envelope parent"))
            .expect("envelope dir");
        std::fs::create_dir_all(root.join(".wave/codex")).expect("codex dir");
        std::fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        std::fs::write(agent_dir.join("last-message.txt"), "[wave-proof]\n")
            .expect("write message");
        std::fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        std::fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");

        let runtime = runtime_record_fixture(
            wave_domain::RuntimeId::Claude,
            Some(wave_domain::RuntimeFallbackRecord {
                requested_runtime: wave_domain::RuntimeId::Codex,
                selected_runtime: wave_domain::RuntimeId::Claude,
                reason: "codex login status reported unavailable".to_string(),
            }),
            "wave-15-proof",
        );

        wave_trace::write_result_envelope(
            &envelope_path,
            &wave_trace::ResultEnvelopeRecord {
                result_envelope_id: "result:wave-15-proof:a1".to_string(),
                wave_id: 15,
                task_id: "wave-15:agent-a1".to_string(),
                attempt_id: "attempt-a1-proof".to_string(),
                agent_id: "A1".to_string(),
                task_role: "implementation".to_string(),
                closure_role: None,
                source: wave_trace::ResultEnvelopeSource::Structured,
                attempt_state: wave_trace::AttemptState::Succeeded,
                disposition: wave_trace::ResultDisposition::Completed,
                summary: Some("runtime detail proof".to_string()),
                output_text: Some("[wave-proof]".to_string()),
                final_markers: wave_trace::FinalMarkerEnvelope::from_contract(
                    vec!["[wave-proof]".to_string()],
                    vec!["[wave-proof]".to_string()],
                ),
                proof_bundle_ids: Vec::new(),
                fact_ids: Vec::new(),
                contradiction_ids: Vec::new(),
                artifacts: Vec::new(),
                doc_delta: wave_trace::DocDeltaEnvelope::default(),
                marker_evidence: Vec::new(),
                closure: wave_trace::ClosureState {
                    disposition: wave_trace::ClosureDisposition::Ready,
                    required_final_markers: vec!["[wave-proof]".to_string()],
                    observed_final_markers: vec!["[wave-proof]".to_string()],
                    blocking_reasons: Vec::new(),
                    satisfied_fact_ids: Vec::new(),
                    contradiction_ids: Vec::new(),
                    verdict: wave_trace::ClosureVerdictPayload::None,
                },
                runtime: Some(runtime),
                created_at_ms: 3,
            },
        )
        .expect("write proof envelope");

        let wave = WaveDocument {
            path: PathBuf::from("waves/15.md"),
            metadata: WaveMetadata {
                id: 15,
                slug: "runtime-policy".to_string(),
                title: "Runtime Policy".to_string(),
                mode: wave_config::ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: Vec::new(),
            },
            heading_title: Some("Wave 15".to_string()),
            commit_message: Some("Feat: runtime policy".to_string()),
            component_promotions: vec![ComponentPromotion {
                component: "runtime-policy".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some("Runtime detail transport".to_string()),
            }),
            agents: vec![WaveAgent {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                role_prompts: Vec::new(),
                executor: std::collections::BTreeMap::new(),
                context7: Some(Context7Defaults {
                    bundle: "none".to_string(),
                    query: Some("noop".to_string()),
                }),
                skills: vec!["wave-core".to_string()],
                components: Vec::new(),
                capabilities: Vec::new(),
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Contract,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Unit,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec!["README.md".to_string()],
                file_ownership: vec!["README.md".to_string()],
                final_markers: vec!["[wave-proof]".to_string()],
                prompt: "Primary goal:\n- noop".to_string(),
            }],
        };
        let run = WaveRunRecord {
            run_id: "wave-15-proof".to_string(),
            wave_id: 15,
            slug: "runtime-policy".to_string(),
            title: "Runtime Policy".to_string(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: trace_path.clone(),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: vec![wave_trace::AgentRunRecord {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                status: WaveRunStatus::Running,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: agent_dir.join("last-message.txt"),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: Some(envelope_path),
                runtime_detail_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: None,
                error: None,
                runtime: None,
            }],
            error: None,
        };
        wave_trace::write_trace_bundle(&trace_path, &run).expect("write trace");

        let latest_run_details = latest_relevant_run_details(
            &root,
            std::slice::from_ref(&wave),
            &HashMap::from([(15, run)]),
        );
        let active_run_details = latest_run_details
            .iter()
            .filter(|detail| {
                matches!(
                    detail.status,
                    WaveRunStatus::Planned | WaveRunStatus::Running
                )
            })
            .cloned()
            .collect::<Vec<_>>();

        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let proof_dir = workspace_root
            .join("docs/implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime");
        std::fs::create_dir_all(&proof_dir).expect("create proof dir");
        std::fs::write(
            proof_dir.join("operator-runtime-transport.json"),
            serde_json::to_string_pretty(&RuntimeTransportBundle {
                generated_at_ms: wave_trace::now_epoch_ms().expect("transport proof timestamp"),
                latest_run_details,
                active_run_details,
                runtime_boundary_policy: runtime_boundary_policy_from_status(
                    &runtime_boundary_fixture(),
                ),
                proof_surfaces: phase_3_runtime_policy_proof_surfaces(),
                policy_contract_resolutions: vec![
                    runtime_policy_contract_resolution(
                        "codex-direct",
                        "fixture-backed",
                        runtime_record_fixture(
                            wave_domain::RuntimeId::Codex,
                            None,
                            "wave-15-proof-codex",
                        ),
                    ),
                    runtime_policy_contract_resolution(
                        "claude-fallback",
                        "fixture-backed",
                        runtime_record_fixture(
                            wave_domain::RuntimeId::Claude,
                            Some(wave_domain::RuntimeFallbackRecord {
                                requested_runtime: wave_domain::RuntimeId::Codex,
                                selected_runtime: wave_domain::RuntimeId::Claude,
                                reason: "codex login status reported unavailable".to_string(),
                            }),
                            "wave-15-proof-claude",
                        ),
                    ),
                ],
            })
            .expect("serialize runtime transport proof"),
        )
        .expect("write runtime transport proof");

        let _ = std::fs::remove_dir_all(&root);
    }
}
