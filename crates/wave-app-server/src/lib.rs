//! Bootstrap operator snapshot assembly for the Wave workspace.
//!
//! This crate stays focused on reading authored wave state, live run records,
//! and rerun intents into a single operator snapshot. It is a landing zone for
//! later control-plane and UI refinements, not a separate source of truth.

use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use wave_config::ProjectConfig;
use wave_control_plane::ControlProjection;
use wave_control_plane::PlanningStatus;
use wave_control_plane::PlanningStatusProjection;
use wave_control_plane::QueueBlockerSummary;
use wave_control_plane::build_planning_status_projection;
use wave_control_plane::build_planning_status_with_state;
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
    pub planning: PlanningStatus,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlPanelSnapshot {
    pub rerun_supported: bool,
    pub clear_rerun_supported: bool,
    pub launch_supported: bool,
    pub autonomous_supported: bool,
    pub launcher_required: bool,
    pub launcher_ready: bool,
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
    let planning = build_planning_status_with_state(
        config,
        &waves,
        &findings,
        &skill_catalog_issues,
        &latest_runs,
        &rerun_wave_ids,
    );
    let planning_projection = build_planning_status_projection(&planning);
    let rerun_intents = list_rerun_intents(root, config)?
        .into_values()
        .collect::<Vec<_>>();
    let mut active_run_details = latest_runs
        .values()
        .filter(|run| matches!(run.status, WaveRunStatus::Planned | WaveRunStatus::Running))
        .filter_map(|run| build_active_run_detail(root, &waves, run))
        .collect::<Vec<_>>();
    active_run_details.sort_by_key(|detail| detail.wave_id);
    let control_actions =
        build_control_actions(&planning_projection.control, codex_binary_available());
    Ok(build_operator_snapshot(
        &planning,
        &planning_projection,
        &latest_runs,
        rerun_intents,
        active_run_details,
        control_actions,
    )?)
}

pub fn build_operator_snapshot(
    planning: &PlanningStatus,
    projection: &PlanningStatusProjection,
    latest_runs: &HashMap<u32, WaveRunRecord>,
    mut rerun_intents: Vec<RerunIntentRecord>,
    active_run_details: Vec<ActiveRunDetail>,
    control_actions: Vec<ControlAction>,
) -> Result<OperatorSnapshot> {
    rerun_intents.sort_by_key(|intent| intent.wave_id);
    let launcher_ready = codex_binary_available();
    let launcher = LauncherStatus {
        codex_binary_available: launcher_ready,
        ready: launcher_ready,
    };
    let panels = build_operator_panels_snapshot(
        projection,
        launcher_ready,
        active_run_details.clone(),
        control_actions.clone(),
    );

    Ok(OperatorSnapshot {
        generated_at_ms: now_epoch_ms()?,
        dashboard: build_dashboard_snapshot(planning, latest_runs),
        planning: planning.clone(),
        panels,
        launcher,
        active_run_details,
        rerun_intents,
        control_actions,
    })
}

fn build_operator_panels_snapshot(
    projection: &PlanningStatusProjection,
    launcher_ready: bool,
    active_run_details: Vec<ActiveRunDetail>,
    control_actions: Vec<ControlAction>,
) -> OperatorPanelsSnapshot {
    let active_wave_ids = projection.run.active_wave_ids.clone();
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
            active_run_count: projection.run.active_run_count,
            completed_run_count: projection.run.completed_run_count,
            active_runs: active_run_details,
            proof_complete_run_count,
        },
        agents: AgentsPanelSnapshot {
            total_agents: projection.agents.total_agents,
            implementation_agents: projection.agents.implementation_agents,
            closure_agents: projection.agents.closure_agents,
            required_closure_agents: projection.agents.required_closure_agents.clone(),
            present_closure_agents: projection.agents.present_closure_agents.clone(),
            missing_closure_agents: projection.agents.missing_closure_agents.clone(),
            agent_details,
        },
        queue: QueuePanelSnapshot {
            ready_wave_count: projection.queue.ready.len(),
            blocked_wave_count: projection.queue.blocked.len(),
            active_wave_count: projection.queue.active.len(),
            completed_wave_count: projection.queue.completed.len(),
            ready_wave_ids: projection.queue.ready.iter().map(|wave| wave.id).collect(),
            blocked_wave_ids: projection
                .queue
                .blocked
                .iter()
                .map(|wave| wave.id)
                .collect(),
            active_wave_ids: projection.queue.active.iter().map(|wave| wave.id).collect(),
            blocker_summary: projection.queue.blocker_summary.clone(),
            next_ready_wave_ids: projection.queue.ready.iter().map(|wave| wave.id).collect(),
            claimable_wave_ids: projection.queue.ready.iter().map(|wave| wave.id).collect(),
            queue_ready: !projection.queue.ready.is_empty() || !projection.queue.active.is_empty(),
            queue_ready_reason: if !projection.queue.ready.is_empty() {
                "ready waves are available to claim".to_string()
            } else if !projection.queue.active.is_empty() {
                "active waves are still in flight".to_string()
            } else if !projection.queue.blocked.is_empty() {
                let blocked_count = projection.queue.blocked.len();
                let dependency = projection.queue.blocker_summary.dependency;
                let lint = projection.queue.blocker_summary.lint;
                let closure = projection.queue.blocker_summary.closure;
                let active_run = projection.queue.blocker_summary.active_run;
                let already_completed = projection.queue.blocker_summary.already_completed;
                let other = projection.queue.blocker_summary.other;
                format!(
                    "{blocked_count} wave(s) are blocked: dependency={dependency}, lint={lint}, closure={closure}, active_run={active_run}, already_completed={already_completed}, other={other}"
                )
            } else {
                "no ready or active waves are currently available".to_string()
            },
        },
        control: ControlPanelSnapshot {
            rerun_supported: projection.control.rerun_supported,
            clear_rerun_supported: projection.control.clear_rerun_supported,
            launch_supported: projection.control.launch_supported,
            autonomous_supported: projection.control.autonomous_supported,
            launcher_required: projection.control.launcher_required,
            launcher_ready,
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
            unavailable_reasons: projection.control.unavailable_reasons.clone(),
        },
    }
}

fn build_control_actions(control: &ControlProjection, launcher_ready: bool) -> Vec<ControlAction> {
    let actions = vec![
        ControlAction {
            key: "Tab".to_string(),
            label: "Next panel".to_string(),
            description: "Cycle the right-side panel tabs.".to_string(),
            implemented: true,
        },
        ControlAction {
            key: "j/k".to_string(),
            label: "Select wave".to_string(),
            description: "Move the queue selection in the operator shell.".to_string(),
            implemented: true,
        },
        ControlAction {
            key: "r".to_string(),
            label: "Request rerun".to_string(),
            description: if control.rerun_supported {
                "Write a rerun intent for the selected wave.".to_string()
            } else {
                "Rerun requests are not supported by the control plane yet.".to_string()
            },
            implemented: control.rerun_supported,
        },
        ControlAction {
            key: "c".to_string(),
            label: "Clear rerun".to_string(),
            description: if control.clear_rerun_supported {
                "Clear the selected wave's rerun intent.".to_string()
            } else {
                "Clearing rerun intents is not supported by the control plane yet.".to_string()
            },
            implemented: control.clear_rerun_supported,
        },
        ControlAction {
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
        ControlAction {
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
        ControlAction {
            key: "q".to_string(),
            label: "Quit".to_string(),
            description: "Leave the operator shell.".to_string(),
            implemented: true,
        },
    ];

    actions
        .into_iter()
        .map(|mut action| {
            action.implemented = match action.key.as_str() {
                "r" => control.rerun_supported,
                "c" => control.clear_rerun_supported,
                "launch" => control.launch_supported && launcher_ready,
                "autonomous" => control.autonomous_supported && launcher_ready,
                _ => true,
            };
            action
        })
        .collect()
}

pub fn build_dashboard_snapshot(
    status: &PlanningStatus,
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> DashboardSnapshot {
    DashboardSnapshot {
        project_name: status.project_name.clone(),
        next_ready_wave_ids: status.next_ready_wave_ids.clone(),
        active_runs: latest_runs
            .values()
            .filter(|run| !run.completed_successfully())
            .map(|run| ActiveRunSnapshot {
                wave_id: run.wave_id,
                run_id: run.run_id.clone(),
                status: run.status.to_string(),
                agent_count: run.agents.len(),
            })
            .collect(),
        total_waves: status.waves.len(),
        completed_waves: status.waves.iter().filter(|wave| wave.completed).count(),
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
    use std::path::PathBuf;
    use wave_control_plane::PlanningStatusSummary;
    use wave_control_plane::QueueReadinessProjection;
    use wave_control_plane::QueueReadinessState;
    use wave_control_plane::SkillCatalogHealth;
    use wave_control_plane::WaveBlockerKind;
    use wave_control_plane::WaveBlockerState;
    use wave_control_plane::WaveQueueEntry;
    use wave_control_plane::WaveReadinessState;

    #[test]
    fn dashboard_snapshot_counts_completed_waves() {
        let status = PlanningStatus {
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
            queue: QueueReadinessProjection {
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
                WaveQueueEntry {
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
                    readiness: WaveReadinessState {
                        state: QueueReadinessState::Completed,
                        claimable: false,
                        reasons: vec![WaveBlockerState {
                            kind: WaveBlockerKind::AlreadyCompleted,
                            raw: "already-completed".to_string(),
                            detail: None,
                        }],
                        primary_reason: Some(WaveBlockerState {
                            kind: WaveBlockerKind::AlreadyCompleted,
                            raw: "already-completed".to_string(),
                            detail: None,
                        }),
                    },
                    rerun_requested: false,
                    completed: true,
                    last_run_status: Some(WaveRunStatus::Succeeded),
                },
                WaveQueueEntry {
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
                    readiness: WaveReadinessState {
                        state: QueueReadinessState::Ready,
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

        let snapshot = build_dashboard_snapshot(&status, &latest_runs);
        assert_eq!(snapshot.completed_waves, 1);
        assert_eq!(snapshot.active_runs.len(), 1);
    }

    #[test]
    fn operator_snapshot_exposes_control_plane_truth() {
        let status = PlanningStatus {
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
            queue: QueueReadinessProjection {
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
            waves: vec![wave_control_plane::WaveQueueEntry {
                id: 7,
                slug: "seven".to_string(),
                title: "Seven".to_string(),
                depends_on: Vec::new(),
                blocked_by: Vec::new(),
                blocker_state: vec![WaveBlockerState {
                    kind: wave_control_plane::WaveBlockerKind::Other,
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
                readiness: WaveReadinessState {
                    state: QueueReadinessState::Ready,
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
        let snapshot = build_operator_snapshot(
            &status,
            &projection,
            &HashMap::new(),
            Vec::new(),
            Vec::new(),
            vec![
                ControlAction {
                    key: "r".to_string(),
                    label: "Request rerun".to_string(),
                    description: "Request rerun".to_string(),
                    implemented: true,
                },
                ControlAction {
                    key: "launch".to_string(),
                    label: "Launch wave".to_string(),
                    description: "Launch is unavailable".to_string(),
                    implemented: false,
                },
            ],
        )
        .unwrap();

        assert!(snapshot.panels.queue.queue_ready);
        assert_eq!(
            snapshot.panels.queue.queue_ready_reason,
            "ready waves are available to claim"
        );
        assert!(snapshot.panels.control.unavailable_reasons.is_empty());
        assert_eq!(snapshot.panels.control.implemented_actions.len(), 1);
        assert_eq!(snapshot.panels.control.unavailable_actions.len(), 1);
        assert_eq!(snapshot.panels.control.unavailable_actions[0].key, "launch");
    }
}
