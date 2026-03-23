//! Bootstrap operator snapshot assembly for the Wave workspace.
//!
//! This crate stays focused on mapping the reducer-backed projection spine plus
//! compatibility active-run details into a transport snapshot for the operator
//! surfaces, including the projection-owned control-status payload that queue
//! and control consumers share. It is a landing zone for later control-plane
//! and UI refinements, not a separate source of truth.

use anyhow::Result;
use serde::Serialize;
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
use wave_control_plane::build_projection_spine_with_state;
use wave_dark_factory::lint_project;
use wave_dark_factory::validate_skill_catalog;
use wave_runtime::RerunIntentRecord;
use wave_runtime::codex_binary_available;
use wave_runtime::list_rerun_intents;
use wave_runtime::load_latest_runs;
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
    pub completed_agents: usize,
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
    let spine = build_projection_spine_with_state(
        config,
        &waves,
        &findings,
        &skill_catalog_issues,
        &latest_runs,
        &rerun_wave_ids,
        launcher_ready,
    );
    let rerun_intents = list_rerun_intents(root, config)?
        .into_values()
        .collect::<Vec<_>>();
    let mut active_run_details = latest_runs
        .values()
        .filter(|run| matches!(run.status, WaveRunStatus::Planned | WaveRunStatus::Running))
        .filter_map(|run| build_active_run_detail(root, &waves, run))
        .collect::<Vec<_>>();
    active_run_details.sort_by_key(|detail| detail.wave_id);
    Ok(build_operator_snapshot(
        &spine,
        rerun_intents,
        active_run_details,
    )?)
}

pub fn build_operator_snapshot(
    spine: &ProjectionSpine,
    mut rerun_intents: Vec<RerunIntentRecord>,
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

fn build_active_run_detail(
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
                build_agent_panel_item(agent, declared)
            })
            .collect(),
    })
}

fn build_agent_panel_item(
    agent: &wave_trace::AgentRunRecord,
    declared: Option<&WaveAgent>,
) -> AgentPanelItem {
    let missing_markers = agent
        .expected_markers
        .iter()
        .filter(|marker| !agent.observed_markers.iter().any(|seen| seen == *marker))
        .cloned()
        .collect::<Vec<_>>();

    AgentPanelItem {
        id: agent.id.clone(),
        title: agent.title.clone(),
        status: agent.status,
        current_task: declared
            .map(|declared| declared.title.clone())
            .unwrap_or_else(|| agent.title.clone()),
        proof_complete: missing_markers.is_empty() && agent.status == WaveRunStatus::Succeeded,
        expected_markers: agent.expected_markers.clone(),
        observed_markers: agent.observed_markers.clone(),
        missing_markers,
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
    let completed_agents = run
        .agents
        .iter()
        .filter(|agent| agent.status == WaveRunStatus::Succeeded)
        .count();
    let agent_proof_complete = run.agents.iter().all(|agent| {
        let missing = agent
            .expected_markers
            .iter()
            .filter(|marker| !agent.observed_markers.iter().any(|seen| seen == *marker))
            .count();
        missing == 0 && agent.status == WaveRunStatus::Succeeded
    });

    ProofSnapshot {
        complete: declared_artifacts.iter().all(|artifact| artifact.exists) && agent_proof_complete,
        declared_artifacts,
        completed_agents,
        total_agents: run.agents.len(),
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
        let snapshot = build_operator_snapshot(&spine, Vec::new(), Vec::new()).unwrap();

        assert!(snapshot.panels.queue.queue_ready);
        assert_eq!(
            snapshot.panels.queue.queue_ready_reason,
            "ready waves are available to claim"
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
    }
}
