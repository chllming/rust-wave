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
use std::collections::HashMap;
use std::fs;
use std::path::Path;
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
use wave_runtime::codex_binary_available;
use wave_runtime::list_rerun_intents;
use wave_runtime::load_latest_runs;
use wave_runtime::load_relevant_runs;
use wave_runtime::pending_rerun_wave_ids;
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
    pub blocked_wave_count: usize,
    pub active_wave_count: usize,
    pub completed_wave_count: usize,
    pub ready_wave_ids: Vec<u32>,
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
    pub codex_binary_available: bool,
    pub ready: bool,
}

pub fn load_operator_snapshot(root: &Path, config: &ProjectConfig) -> Result<OperatorSnapshot> {
    let waves = load_wave_documents(config, root)?;
    let findings = lint_project(root, &waves);
    let skill_catalog_issues = validate_skill_catalog(root);
    let latest_runs = load_latest_runs(root, config)?;
    let rerun_wave_ids = pending_rerun_wave_ids(root, config)?;
    let launcher_ready = codex_binary_available();
    let spine = build_projection_spine_from_authority(
        root,
        config,
        &waves,
        &findings,
        &skill_catalog_issues,
        &latest_runs,
        &rerun_wave_ids,
        launcher_ready,
    )?;
    let rerun_intents = list_rerun_intents(root, config)?
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
        rerun_intents,
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
    mut rerun_intents: Vec<RerunIntentRecord>,
    latest_run_details: Vec<ActiveRunDetail>,
    active_run_details: Vec<ActiveRunDetail>,
) -> Result<OperatorSnapshot> {
    rerun_intents.sort_by_key(|intent| intent.wave_id);
    let control_status = build_control_status_read_model_from_spine(spine);
    let control_actions = build_control_actions(&spine.operator.control.actions);
    let launcher = LauncherStatus {
        codex_binary_available: spine.operator.control.launcher_ready,
        ready: spine.operator.control.launcher_ready,
    };
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
            blocked_wave_count: operator.queue.blocked_wave_count,
            active_wave_count: operator.queue.active_wave_count,
            completed_wave_count: operator.queue.completed_wave_count,
            ready_wave_ids: operator.queue.ready_wave_ids.clone(),
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
    let activity_excerpt = current_agent
        .and_then(|agent| read_tail(&agent.last_message_path, 16))
        .unwrap_or_else(|| "No live agent output yet.".to_string());
    let proof = build_proof_snapshot(root, wave, run);
    let replay = validate_replay(run);

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
        activity_excerpt,
        proof,
        replay,
        agents: run
            .agents
            .iter()
            .map(|agent| {
                let declared = wave
                    .agents
                    .iter()
                    .find(|candidate| candidate.id == agent.id);
                build_agent_panel_item(root, run, agent, declared)
            })
            .collect(),
    })
}

#[derive(Debug, Clone)]
struct ResolvedEnvelopeProof {
    attempt_state: wave_domain::AttemptState,
    disposition: wave_domain::ResultDisposition,
    source: &'static str,
    required_final_markers: Vec<String>,
    observed_final_markers: Vec<String>,
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
    }
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
                ready_wave_count: 1,
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
                        claimable: true,
                        reasons: Vec::new(),
                        primary_reason: None,
                    },
                    rerun_requested: false,
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
                ready_wave_count: 1,
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
                agent_count: 3,
                implementation_agent_count: 1,
                closure_agent_count: 2,
                closure_complete: true,
                required_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                present_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                missing_closure_agents: Vec::new(),
                readiness: WaveReadinessReadModel {
                    state: QueueReadinessStateReadModel::Ready,
                    claimable: true,
                    reasons: Vec::new(),
                    primary_reason: None,
                },
                rerun_requested: false,
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
        let snapshot = build_operator_snapshot(&spine, Vec::new(), Vec::new(), Vec::new()).unwrap();

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
            vec!["codex binary is missing"]
        );
        assert_eq!(snapshot.panels.control.implemented_actions.len(), 5);
        assert_eq!(snapshot.panels.control.unavailable_actions.len(), 2);
        assert_eq!(snapshot.panels.control.unavailable_actions[0].key, "launch");
        assert_eq!(snapshot.panels.control.actions.len(), 7);
        assert_eq!(snapshot.panels.queue.waves[0].queue_state, "ready");
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
                ready_wave_count: 0,
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
        let snapshot = build_operator_snapshot(&spine, Vec::new(), Vec::new(), Vec::new()).unwrap();

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
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: Some(0),
                error: None,
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
                expected_markers: vec!["[wave-integration]".to_string()],
                observed_markers: Vec::new(),
                exit_code: Some(0),
                error: None,
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
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: Some(1),
                error: Some("structured failure".to_string()),
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
}
