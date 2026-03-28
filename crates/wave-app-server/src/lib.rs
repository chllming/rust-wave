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
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;
use wave_config::ProjectConfig;
use wave_control_plane::ControlActionReadModel;
use wave_control_plane::ControlStatusReadModel;
use wave_control_plane::DashboardReadModel;
use wave_control_plane::DeliveryReadModel;
use wave_control_plane::OperatorSnapshotInputs;
use wave_control_plane::PlanningStatusReadModel;
use wave_control_plane::ProjectionSpine;
use wave_control_plane::QueueBlockerSummary;
use wave_control_plane::build_control_status_read_model_from_spine;
use wave_control_plane::build_projection_spine_from_authority;
use wave_control_plane::load_canonical_compatibility_runs;
use wave_control_plane::load_scheduler_events;
use wave_coordination::CoordinationLog;
use wave_coordination::CoordinationRecord;
use wave_coordination::CoordinationRecordKind;
use wave_dark_factory::lint_project;
use wave_dark_factory::validate_skill_catalog;
use wave_domain::ControlEventPayload;
use wave_domain::DesignCompletenessState;
use wave_domain::HumanInputRequest;
use wave_domain::HumanInputState;
use wave_domain::HumanInputWorkflowKind;
use wave_domain::LineageRecord;
use wave_domain::LineageRecordSubject;
use wave_domain::LineageRef;
use wave_domain::WaveClosureOverrideRecord;
use wave_events::ControlEvent;
use wave_events::ControlEventLog;
use wave_reducer::reduce_planning_state_with_authorities;
use wave_runtime::RerunIntentRecord;
use wave_runtime::active_closure_override_wave_ids;
use wave_runtime::latest_operator_shell_session;
use wave_runtime::latest_orchestrator_session;
use wave_runtime::list_agent_sandboxes;
use wave_runtime::list_closure_overrides;
use wave_runtime::list_control_directives;
use wave_runtime::list_directive_deliveries;
use wave_runtime::list_head_proposals;
use wave_runtime::list_invalidations;
use wave_runtime::list_merge_results;
use wave_runtime::list_operator_shell_turns;
use wave_runtime::list_recovery_actions;
use wave_runtime::list_recovery_plans;
use wave_runtime::list_rerun_intents;
use wave_runtime::load_latest_runs;
use wave_runtime::load_relevant_runs;
use wave_runtime::pending_rerun_wave_ids;
use wave_runtime::runtime_boundary_status;
use wave_spec::BarrierClass;
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
    pub delivery: DeliveryReadModel,
    pub control_status: ControlStatusReadModel,
    pub panels: OperatorPanelsSnapshot,
    pub launcher: LauncherStatus,
    pub latest_run_details: Vec<ActiveRunDetail>,
    pub active_run_details: Vec<ActiveRunDetail>,
    pub design_details: Vec<WaveDesignDetail>,
    pub operator_objects: Vec<OperatorActionableItem>,
    pub acceptance_packages: Vec<AcceptancePackageSnapshot>,
    pub rerun_intents: Vec<RerunIntentRecord>,
    pub closure_overrides: Vec<WaveClosureOverrideRecord>,
    pub control_actions: Vec<ControlAction>,
    pub shell: OperatorShellSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorPanelsSnapshot {
    pub run: RunPanelSnapshot,
    pub agents: AgentsPanelSnapshot,
    pub queue: QueuePanelSnapshot,
    pub control: ControlPanelSnapshot,
    pub orchestrator: OrchestratorPanelSnapshot,
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
    pub apply_closure_override_supported: bool,
    pub clear_closure_override_supported: bool,
    pub approve_operator_action_supported: bool,
    pub reject_operator_action_supported: bool,
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
pub struct OrchestratorPanelSnapshot {
    pub mode: String,
    pub active: bool,
    pub multi_agent_wave_count: usize,
    pub selected_wave_id: Option<u32>,
    pub autonomous_wave_ids: Vec<u32>,
    pub pending_proposal_count: usize,
    pub autonomous_action_count: usize,
    pub failed_head_turn_count: usize,
    pub unresolved_recovery_count: usize,
    pub recent_autonomous_actions: Vec<AutonomousActionSnapshot>,
    pub recent_autonomous_failures: Vec<AutonomousFailureSnapshot>,
    pub waves: Vec<WaveOrchestratorSnapshot>,
    pub directives: Vec<DirectiveSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutonomousActionSnapshot {
    pub proposal_id: String,
    pub wave_id: u32,
    pub agent_id: Option<String>,
    pub summary: String,
    pub resolution: String,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutonomousFailureSnapshot {
    pub turn_id: String,
    pub wave_id: u32,
    pub agent_id: Option<String>,
    pub summary: String,
    pub detail: Option<String>,
    pub created_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct OperatorShellSnapshot {
    pub default_target: OperatorShellTargetSnapshot,
    pub session: Option<OperatorShellSessionSnapshot>,
    pub transcript: Vec<OperatorShellTranscriptItem>,
    pub proposals: Vec<OperatorShellProposalItem>,
    pub command_availability: BTreeMap<String, bool>,
    pub commands: Vec<OperatorShellCommand>,
    pub last_event_at_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorShellSessionSnapshot {
    pub session_id: String,
    pub scope: String,
    pub wave_id: Option<u32>,
    pub agent_id: Option<String>,
    pub tab: String,
    pub follow_mode: String,
    pub mode: String,
    pub active: bool,
    pub started_at_ms: u128,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct OperatorShellTargetSnapshot {
    pub scope: String,
    pub wave_id: Option<u32>,
    pub agent_id: Option<String>,
    pub label: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorShellTranscriptItem {
    pub item_id: String,
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub origin: Option<String>,
    pub wave_id: Option<u32>,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub proposal_id: Option<String>,
    pub created_at_ms: u128,
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorShellProposalItem {
    pub proposal_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub cycle_id: Option<String>,
    pub wave_id: u32,
    pub agent_id: Option<String>,
    pub action_kind: String,
    pub state: String,
    pub resolution: Option<String>,
    pub resolved_by: Option<String>,
    pub resolved_at_ms: Option<u128>,
    pub summary: String,
    pub detail: Option<String>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorShellCommand {
    pub name: String,
    pub usage: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveOrchestratorSnapshot {
    pub wave_id: u32,
    pub title: String,
    pub execution_model: String,
    pub mode: String,
    pub active_run_id: Option<String>,
    pub pending_proposal_count: usize,
    pub autonomous_action_count: usize,
    pub recovery_required: bool,
    pub last_head_turn_at_ms: Option<u128>,
    pub last_head_summary: Option<String>,
    pub last_autonomous_failure: Option<String>,
    pub agents: Vec<MasAgentSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MasAgentSnapshot {
    pub id: String,
    pub title: String,
    pub barrier_class: String,
    pub depends_on_agents: Vec<String>,
    pub writes_artifacts: Vec<String>,
    pub exclusive_resources: Vec<String>,
    pub status: String,
    pub merge_state: Option<String>,
    pub sandbox_id: Option<String>,
    pub heartbeat_age_ms: Option<u128>,
    pub pending_directive_count: usize,
    pub last_head_action: Option<String>,
    pub recovery_state: Option<String>,
    pub barrier_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirectiveSnapshot {
    pub directive_id: String,
    pub wave_id: u32,
    pub agent_id: Option<String>,
    pub kind: String,
    pub origin: String,
    pub message: Option<String>,
    pub requested_by: String,
    pub requested_at_ms: u128,
    pub delivery_state: Option<String>,
    pub delivery_detail: Option<String>,
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
    pub mas: Option<MasRunDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MasRunDetail {
    pub execution_model: String,
    pub running_agent_ids: Vec<String>,
    pub merged_agent_ids: Vec<String>,
    pub conflicted_agent_ids: Vec<String>,
    pub invalidated_agent_ids: Vec<String>,
    pub sandboxes: Vec<MasSandboxSnapshot>,
    pub merges: Vec<MasMergeSnapshot>,
    pub invalidations: Vec<MasInvalidationSnapshot>,
    pub recovery: Option<MasRecoverySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MasSandboxSnapshot {
    pub sandbox_id: String,
    pub agent_id: String,
    pub path: String,
    pub base_integration_ref: Option<String>,
    pub released_at_ms: Option<u128>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MasMergeSnapshot {
    pub agent_id: String,
    pub disposition: String,
    pub conflict_paths: Vec<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MasInvalidationSnapshot {
    pub source_agent_id: String,
    pub invalidated_agent_ids: Vec<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MasRecoverySnapshot {
    pub recovery_plan_id: String,
    pub run_id: String,
    pub status: String,
    pub causes: Vec<String>,
    pub affected_agent_ids: Vec<String>,
    pub preserved_accepted_agent_ids: Vec<String>,
    pub required_actions: Vec<String>,
    pub detail: Option<String>,
    pub recent_actions: Vec<MasRecoveryActionSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MasRecoveryActionSnapshot {
    pub recovery_action_id: String,
    pub agent_id: Option<String>,
    pub action_kind: String,
    pub requested_by: String,
    pub created_at_ms: u128,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingHumanInputDetail {
    pub request_id: String,
    pub task_id: Option<String>,
    pub state: HumanInputState,
    pub workflow_kind: HumanInputWorkflowKind,
    pub route: String,
    pub prompt: String,
    pub requested_by: String,
    pub answer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveDesignDetail {
    pub wave_id: u32,
    pub completeness: DesignCompletenessState,
    pub blocker_reasons: Vec<String>,
    pub active_contradictions: Vec<ContradictionDetail>,
    pub unresolved_question_ids: Vec<String>,
    pub unresolved_assumption_ids: Vec<String>,
    pub pending_human_inputs: Vec<PendingHumanInputDetail>,
    pub dependency_handshake_routes: Vec<String>,
    pub invalidated_fact_ids: Vec<String>,
    pub invalidated_decision_ids: Vec<String>,
    pub invalidation_routes: Vec<String>,
    pub selectively_invalidated_task_ids: Vec<String>,
    pub superseded_decision_ids: Vec<String>,
    pub ambiguous_dependency_wave_ids: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContradictionDetail {
    pub contradiction_id: String,
    pub state: String,
    pub summary: String,
    pub detail: Option<String>,
    pub invalidated_refs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShipReadinessState {
    Ship,
    NoShip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseReadinessState {
    NotStarted,
    BuildingEvidence,
    AwaitingPromotion,
    PromotionBlocked,
    AwaitingSignoff,
    Accepted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptanceSignoffState {
    PendingEvidence,
    AwaitingClosure,
    AwaitingOperator,
    BlockedByOverride,
    SignedOff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptancePackageSnapshot {
    pub package_id: String,
    pub wave_id: u32,
    pub wave_slug: String,
    pub wave_title: String,
    pub run_id: Option<String>,
    pub ship_state: ShipReadinessState,
    pub release_state: ReleaseReadinessState,
    pub summary: String,
    pub blocking_reasons: Vec<String>,
    pub design_intent: AcceptanceDesignIntentSnapshot,
    pub implementation: AcceptanceImplementationSnapshot,
    pub release: AcceptanceReleaseSnapshot,
    pub signoff: AcceptanceSignoffSnapshot,
    pub known_risks: Vec<DeliveryStateItem>,
    pub outstanding_debt: Vec<DeliveryStateItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptanceDesignIntentSnapshot {
    pub completeness: DesignCompletenessState,
    pub blocker_count: usize,
    pub contradiction_count: usize,
    pub unresolved_question_count: usize,
    pub unresolved_assumption_count: usize,
    pub pending_human_input_count: usize,
    pub ambiguous_dependency_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptanceImplementationSnapshot {
    pub proof_complete: bool,
    pub proof_source: Option<String>,
    pub replay_ok: Option<bool>,
    pub completed_agents: usize,
    pub total_agents: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptanceReleaseSnapshot {
    pub promotion_state: Option<wave_domain::WavePromotionState>,
    pub merge_blocked: bool,
    pub closure_blocked: bool,
    pub scheduler_phase: Option<wave_domain::WaveExecutionPhase>,
    pub scheduler_state: Option<wave_domain::WaveSchedulingState>,
    pub last_decision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptanceSignoffSnapshot {
    pub state: AcceptanceSignoffState,
    pub complete: bool,
    pub manual_close_applied: bool,
    pub required_closure_agents: Vec<String>,
    pub completed_closure_agents: Vec<String>,
    pub pending_closure_agents: Vec<String>,
    pub pending_operator_actions: Vec<String>,
    pub closure_agents: Vec<AcceptanceClosureAgentSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptanceClosureAgentSnapshot {
    pub agent_id: String,
    pub title: Option<String>,
    pub status: Option<wave_trace::WaveRunStatus>,
    pub proof_complete: bool,
    pub satisfied: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeliveryStateItem {
    pub code: String,
    pub summary: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorActionableKind {
    Approval,
    Proposal,
    Override,
    Escalation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OperatorActionableItem {
    pub kind: OperatorActionableKind,
    pub wave_id: u32,
    pub record_id: String,
    pub state: String,
    pub summary: String,
    pub detail: Option<String>,
    pub waiting_on: Option<String>,
    pub next_action: Option<String>,
    pub route: Option<String>,
    pub task_id: Option<String>,
    pub source_run_id: Option<String>,
    pub evidence_count: usize,
    pub created_at_ms: Option<u128>,
}

#[derive(Debug, Default)]
struct DesignNarrativeIndex {
    contradictions_by_wave: BTreeMap<u32, Vec<ContradictionDetail>>,
    invalidation_routes_by_wave: BTreeMap<u32, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentPanelItem {
    pub id: String,
    pub title: String,
    pub status: WaveRunStatus,
    pub current_task: String,
    pub reused_from_prior_run: bool,
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
    let shell_session = latest_operator_shell_session(root, config)?;
    let shell_turns = list_operator_shell_turns(root, config, None)?;
    let head_proposals = list_head_proposals(root, config, None)?;
    let design_details = load_wave_design_details(
        root,
        config,
        &waves,
        &findings,
        &skill_catalog_issues,
        &rerun_wave_ids,
        &closure_override_wave_ids,
    )?;
    let operator_objects = load_operator_actionable_items(
        root,
        config,
        &design_details,
        &closure_overrides,
        &head_proposals,
    )?;
    let relevant_runs = load_relevant_run_records(root, config)?;
    let latest_run_details = latest_relevant_run_details(root, config, &waves, &relevant_runs);
    let active_run_details = latest_run_details
        .iter()
        .filter(|run| matches!(run.status, WaveRunStatus::Planned | WaveRunStatus::Running))
        .cloned()
        .collect::<Vec<_>>();
    let mut snapshot = build_operator_snapshot_with_design_details(
        &spine,
        runtime_boundary,
        rerun_intents,
        closure_overrides,
        latest_run_details,
        active_run_details,
        design_details,
        operator_objects,
    )?;
    snapshot.panels.orchestrator = build_orchestrator_panel_snapshot(
        root,
        config,
        &waves,
        &snapshot.planning.waves,
        &snapshot.active_run_details,
        &shell_turns,
        &head_proposals,
    )?;
    snapshot.shell = build_operator_shell_snapshot(
        &snapshot,
        shell_session.as_ref(),
        &shell_turns,
        &head_proposals,
    );
    Ok(snapshot)
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
    rerun_intents: Vec<RerunIntentRecord>,
    closure_overrides: Vec<WaveClosureOverrideRecord>,
    latest_run_details: Vec<ActiveRunDetail>,
    active_run_details: Vec<ActiveRunDetail>,
) -> Result<OperatorSnapshot> {
    build_operator_snapshot_with_design_details(
        spine,
        runtime_boundary,
        rerun_intents,
        closure_overrides,
        latest_run_details,
        active_run_details,
        Vec::new(),
        Vec::new(),
    )
}

pub fn build_operator_snapshot_with_design_details(
    spine: &ProjectionSpine,
    runtime_boundary: wave_runtime::RuntimeBoundaryStatus,
    mut rerun_intents: Vec<RerunIntentRecord>,
    mut closure_overrides: Vec<WaveClosureOverrideRecord>,
    latest_run_details: Vec<ActiveRunDetail>,
    active_run_details: Vec<ActiveRunDetail>,
    mut design_details: Vec<WaveDesignDetail>,
    mut operator_objects: Vec<OperatorActionableItem>,
) -> Result<OperatorSnapshot> {
    rerun_intents.sort_by_key(|intent| intent.wave_id);
    closure_overrides.sort_by_key(|record| record.wave_id);
    design_details.sort_by_key(|detail| detail.wave_id);
    operator_objects.sort_by_key(|item| {
        (
            item.wave_id,
            operator_actionable_kind_priority(item.kind),
            item.created_at_ms.unwrap_or_default(),
            item.record_id.clone(),
        )
    });
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
    let acceptance_packages = build_acceptance_packages(
        &spine.planning.status.waves,
        &latest_run_details,
        &design_details,
        &operator_objects,
        &closure_overrides,
    );
    let panels = build_operator_panels_snapshot(
        &spine.operator,
        active_run_details.clone(),
        control_actions.clone(),
    );

    let mut snapshot = OperatorSnapshot {
        generated_at_ms: now_epoch_ms()?,
        dashboard: build_dashboard_snapshot(&spine.operator.dashboard),
        planning: spine.planning.status.clone(),
        delivery: spine.delivery.clone(),
        control_status,
        panels,
        launcher,
        latest_run_details,
        active_run_details,
        design_details,
        operator_objects,
        acceptance_packages,
        rerun_intents,
        closure_overrides,
        control_actions,
        shell: OperatorShellSnapshot::default(),
    };
    snapshot.shell = build_operator_shell_snapshot(&snapshot, None, &[], &[]);
    Ok(snapshot)
}

fn build_acceptance_packages(
    waves: &[wave_control_plane::WaveStatusReadModel],
    latest_run_details: &[ActiveRunDetail],
    design_details: &[WaveDesignDetail],
    operator_objects: &[OperatorActionableItem],
    closure_overrides: &[WaveClosureOverrideRecord],
) -> Vec<AcceptancePackageSnapshot> {
    let mut packages = waves
        .iter()
        .map(|wave| {
            let run = latest_run_details.iter().find(|run| run.wave_id == wave.id);
            let design = design_details
                .iter()
                .find(|detail| detail.wave_id == wave.id);
            let items = operator_objects
                .iter()
                .filter(|item| item.wave_id == wave.id)
                .cloned()
                .collect::<Vec<_>>();
            let closure_override = closure_overrides
                .iter()
                .find(|record| record.wave_id == wave.id && record.is_active());
            build_acceptance_package(wave, run, design, &items, closure_override)
        })
        .collect::<Vec<_>>();
    packages.sort_by_key(|package| package.wave_id);
    packages
}

fn build_acceptance_package(
    wave: &wave_control_plane::WaveStatusReadModel,
    run: Option<&ActiveRunDetail>,
    design: Option<&WaveDesignDetail>,
    operator_objects: &[OperatorActionableItem],
    closure_override: Option<&WaveClosureOverrideRecord>,
) -> AcceptancePackageSnapshot {
    let design_intent = build_acceptance_design_intent(wave, design);
    let implementation = build_acceptance_implementation(run);
    let release = build_acceptance_release(run);
    let promotion_ready = matches!(
        release.promotion_state,
        Some(wave_domain::WavePromotionState::Ready)
    );
    let signoff = build_acceptance_signoff(
        wave,
        run,
        operator_objects,
        closure_override,
        &design_intent,
        &implementation,
        promotion_ready,
    );
    let release_state = derive_release_state(
        run,
        &design_intent,
        &implementation,
        &release,
        signoff.state,
    );
    let known_risks = build_known_risks(design, run);
    let outstanding_debt = build_outstanding_debt(
        wave,
        design,
        run,
        &release,
        &signoff,
        closure_override,
        operator_objects,
    );
    let mut blocking_reasons = acceptance_blocking_reasons(
        &design_intent,
        &implementation,
        &release,
        &signoff,
        release_state,
        &known_risks,
        &outstanding_debt,
    );
    let ship_state = if release_state == ReleaseReadinessState::Accepted
        && known_risks.is_empty()
        && outstanding_debt.is_empty()
    {
        ShipReadinessState::Ship
    } else {
        if blocking_reasons.is_empty() {
            blocking_reasons.push("delivery package is not yet complete".to_string());
        }
        ShipReadinessState::NoShip
    };
    let summary = match ship_state {
        ShipReadinessState::Ship => {
            "ready to ship: design intent, proof, promotion, and signoff are all clear".to_string()
        }
        ShipReadinessState::NoShip => format!(
            "no ship: {}",
            blocking_reasons
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        ),
    };

    AcceptancePackageSnapshot {
        package_id: format!("acceptance-package-wave-{}", wave.id),
        wave_id: wave.id,
        wave_slug: wave.slug.clone(),
        wave_title: wave.title.clone(),
        run_id: run.map(|run| run.run_id.clone()),
        ship_state,
        release_state,
        summary,
        blocking_reasons,
        design_intent,
        implementation,
        release,
        signoff,
        known_risks,
        outstanding_debt,
    }
}

fn build_acceptance_design_intent(
    wave: &wave_control_plane::WaveStatusReadModel,
    design: Option<&WaveDesignDetail>,
) -> AcceptanceDesignIntentSnapshot {
    AcceptanceDesignIntentSnapshot {
        completeness: design
            .map(|detail| detail.completeness)
            .unwrap_or(wave.design_completeness),
        blocker_count: design
            .map(|detail| detail.blocker_reasons.len())
            .unwrap_or(0),
        contradiction_count: design
            .map(|detail| detail.active_contradictions.len())
            .unwrap_or(0),
        unresolved_question_count: design
            .map(|detail| detail.unresolved_question_ids.len())
            .unwrap_or(0),
        unresolved_assumption_count: design
            .map(|detail| detail.unresolved_assumption_ids.len())
            .unwrap_or(0),
        pending_human_input_count: design
            .map(|detail| detail.pending_human_inputs.len())
            .unwrap_or(0),
        ambiguous_dependency_count: design
            .map(|detail| detail.ambiguous_dependency_wave_ids.len())
            .unwrap_or(0),
    }
}

fn build_acceptance_implementation(
    run: Option<&ActiveRunDetail>,
) -> AcceptanceImplementationSnapshot {
    AcceptanceImplementationSnapshot {
        proof_complete: run.map(|run| run.proof.complete).unwrap_or(false),
        proof_source: run.map(|run| run.proof.proof_source.clone()),
        replay_ok: run.map(|run| run.replay.ok),
        completed_agents: run.map(|run| run.proof.completed_agents).unwrap_or(0),
        total_agents: run.map(|run| run.proof.total_agents).unwrap_or(0),
    }
}

fn build_acceptance_release(run: Option<&ActiveRunDetail>) -> AcceptanceReleaseSnapshot {
    AcceptanceReleaseSnapshot {
        promotion_state: run
            .and_then(|run| run.execution.promotion.as_ref())
            .map(|promotion| promotion.state),
        merge_blocked: run.map(|run| run.execution.merge_blocked).unwrap_or(false),
        closure_blocked: run
            .map(|run| run.execution.closure_blocked_by_promotion)
            .unwrap_or(false),
        scheduler_phase: run
            .and_then(|run| run.execution.scheduling.as_ref())
            .map(|scheduling| scheduling.phase),
        scheduler_state: run
            .and_then(|run| run.execution.scheduling.as_ref())
            .map(|scheduling| scheduling.state),
        last_decision: run
            .and_then(|run| run.execution.scheduling.as_ref())
            .and_then(|scheduling| scheduling.last_decision.clone())
            .or_else(|| {
                run.and_then(|run| {
                    run.execution
                        .promotion
                        .as_ref()
                        .and_then(|promotion| promotion.detail.clone())
                })
            }),
    }
}

fn build_acceptance_signoff(
    wave: &wave_control_plane::WaveStatusReadModel,
    run: Option<&ActiveRunDetail>,
    operator_objects: &[OperatorActionableItem],
    closure_override: Option<&WaveClosureOverrideRecord>,
    design: &AcceptanceDesignIntentSnapshot,
    implementation: &AcceptanceImplementationSnapshot,
    promotion_ready: bool,
) -> AcceptanceSignoffSnapshot {
    let required_closure_agents = wave.required_closure_agents.clone();
    let closure_agents = build_closure_agent_signoff_details(&required_closure_agents, run);
    let completed_closure_agents = closure_agents
        .iter()
        .filter(|agent| agent.satisfied)
        .map(|agent| agent.agent_id.clone())
        .collect::<Vec<_>>();
    let completed = completed_closure_agents
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let pending_closure_agents = closure_agents
        .iter()
        .filter(|agent| !completed.contains(&agent.agent_id))
        .map(|agent| agent.agent_id.clone())
        .collect::<Vec<_>>();
    let pending_operator_actions = operator_objects
        .iter()
        .filter(|item| {
            matches!(
                item.kind,
                OperatorActionableKind::Approval
                    | OperatorActionableKind::Proposal
                    | OperatorActionableKind::Escalation
            )
        })
        .map(|item| item.summary.clone())
        .collect::<Vec<_>>();
    let manual_close_applied = closure_override.is_some() || wave.closure_override_applied;
    let design_ready = design_is_ready(design);
    let evidence_ready = design_ready
        && implementation.proof_complete
        && implementation.replay_ok.unwrap_or(false)
        && promotion_ready;
    let state = if manual_close_applied {
        AcceptanceSignoffState::BlockedByOverride
    } else if !evidence_ready {
        AcceptanceSignoffState::PendingEvidence
    } else if !pending_closure_agents.is_empty() {
        AcceptanceSignoffState::AwaitingClosure
    } else if !pending_operator_actions.is_empty() {
        AcceptanceSignoffState::AwaitingOperator
    } else {
        AcceptanceSignoffState::SignedOff
    };

    AcceptanceSignoffSnapshot {
        state,
        complete: matches!(state, AcceptanceSignoffState::SignedOff),
        manual_close_applied,
        required_closure_agents,
        completed_closure_agents,
        pending_closure_agents,
        pending_operator_actions,
        closure_agents,
    }
}

fn build_closure_agent_signoff_details(
    required_closure_agents: &[String],
    run: Option<&ActiveRunDetail>,
) -> Vec<AcceptanceClosureAgentSnapshot> {
    required_closure_agents
        .iter()
        .map(|agent_id| {
            let detail = run.and_then(|run| run.agents.iter().find(|agent| agent.id == *agent_id));
            let status = detail.map(|agent| agent.status);
            let proof_complete = detail.map(|agent| agent.proof_complete).unwrap_or(false);
            let satisfied =
                matches!(status, Some(wave_trace::WaveRunStatus::Succeeded)) && proof_complete;

            AcceptanceClosureAgentSnapshot {
                agent_id: agent_id.clone(),
                title: detail.map(|agent| agent.title.clone()),
                status,
                proof_complete,
                satisfied,
                error: detail.and_then(|agent| agent.error.clone()),
            }
        })
        .collect()
}

fn derive_release_state(
    run: Option<&ActiveRunDetail>,
    design: &AcceptanceDesignIntentSnapshot,
    implementation: &AcceptanceImplementationSnapshot,
    release: &AcceptanceReleaseSnapshot,
    signoff_state: AcceptanceSignoffState,
) -> ReleaseReadinessState {
    if run.is_none() {
        ReleaseReadinessState::NotStarted
    } else if release.merge_blocked {
        ReleaseReadinessState::PromotionBlocked
    } else if !design_is_ready(design)
        || !implementation.proof_complete
        || implementation.replay_ok == Some(false)
    {
        ReleaseReadinessState::BuildingEvidence
    } else if !matches!(
        release.promotion_state,
        Some(wave_domain::WavePromotionState::Ready)
    ) {
        ReleaseReadinessState::AwaitingPromotion
    } else if !matches!(signoff_state, AcceptanceSignoffState::SignedOff) {
        ReleaseReadinessState::AwaitingSignoff
    } else {
        ReleaseReadinessState::Accepted
    }
}

fn build_known_risks(
    design: Option<&WaveDesignDetail>,
    run: Option<&ActiveRunDetail>,
) -> Vec<DeliveryStateItem> {
    let mut items = Vec::new();

    if let Some(design) = design {
        for blocker in &design.blocker_reasons {
            items.push(delivery_state_item(
                "design-blocker",
                format!("design blocker {}", blocker),
                None,
            ));
        }
        for contradiction in &design.active_contradictions {
            items.push(delivery_state_item(
                "design-contradiction",
                format!(
                    "contradiction {} is {}",
                    contradiction.contradiction_id, contradiction.state
                ),
                contradiction.detail.clone(),
            ));
        }
        for request in &design.pending_human_inputs {
            items.push(delivery_state_item(
                "human-input",
                format!("pending human input {}", request.request_id),
                Some(format!("{} via {}", request.prompt, request.route)),
            ));
        }
        if !design.dependency_handshake_routes.is_empty() {
            items.push(delivery_state_item(
                "dependency-handshake",
                "dependency handshake is still open".to_string(),
                Some(design.dependency_handshake_routes.join(", ")),
            ));
        }
        if !design.invalidation_routes.is_empty() {
            items.push(delivery_state_item(
                "invalidation",
                "design lineage is invalidated".to_string(),
                Some(design.invalidation_routes.join(" | ")),
            ));
        }
        if !design.ambiguous_dependency_wave_ids.is_empty() {
            items.push(delivery_state_item(
                "ambiguous-dependency",
                "dependency ownership is ambiguous".to_string(),
                Some(
                    design
                        .ambiguous_dependency_wave_ids
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            ));
        }
    }

    if let Some(run) = run {
        if run.stalled {
            items.push(delivery_state_item(
                "stalled-run",
                format!("run {} appears stalled", run.run_id),
                run.stall_reason.clone(),
            ));
        }
        if run.execution.merge_blocked {
            items.push(delivery_state_item(
                "promotion-conflict",
                "promotion is merge blocked".to_string(),
                run.execution
                    .promotion
                    .as_ref()
                    .and_then(|promotion| promotion.detail.clone()),
            ));
        }
        for issue in &run.replay.issues {
            items.push(delivery_state_item(
                "replay-issue",
                format!("replay issue {}", issue.kind),
                Some(issue.detail.clone()),
            ));
        }
        for agent in run.agents.iter().filter(|agent| agent.error.is_some()) {
            items.push(delivery_state_item(
                "agent-error",
                format!("agent {} failed", agent.id),
                agent.error.clone(),
            ));
        }
    }

    items
}

fn build_outstanding_debt(
    wave: &wave_control_plane::WaveStatusReadModel,
    design: Option<&WaveDesignDetail>,
    run: Option<&ActiveRunDetail>,
    release: &AcceptanceReleaseSnapshot,
    signoff: &AcceptanceSignoffSnapshot,
    closure_override: Option<&WaveClosureOverrideRecord>,
    operator_objects: &[OperatorActionableItem],
) -> Vec<DeliveryStateItem> {
    let mut items = Vec::new();

    if let Some(design) = design {
        if !design.unresolved_question_ids.is_empty() {
            items.push(delivery_state_item(
                "open-question",
                "open design questions remain".to_string(),
                Some(design.unresolved_question_ids.join(", ")),
            ));
        }
        if !design.unresolved_assumption_ids.is_empty() {
            items.push(delivery_state_item(
                "open-assumption",
                "open design assumptions remain".to_string(),
                Some(design.unresolved_assumption_ids.join(", ")),
            ));
        }
        if !design.invalidated_fact_ids.is_empty() {
            items.push(delivery_state_item(
                "invalidated-fact",
                "invalidated facts still need resolution".to_string(),
                Some(design.invalidated_fact_ids.join(", ")),
            ));
        }
        if !design.invalidated_decision_ids.is_empty() {
            items.push(delivery_state_item(
                "invalidated-decision",
                "invalidated decisions still need resolution".to_string(),
                Some(design.invalidated_decision_ids.join(", ")),
            ));
        }
        if !design.superseded_decision_ids.is_empty() {
            items.push(delivery_state_item(
                "superseded-decision",
                "superseded decisions still affect delivery".to_string(),
                Some(design.superseded_decision_ids.join(", ")),
            ));
        }
        if !design.selectively_invalidated_task_ids.is_empty() {
            items.push(delivery_state_item(
                "selective-rerun",
                "selectively invalidated tasks still need rerun".to_string(),
                Some(design.selectively_invalidated_task_ids.join(", ")),
            ));
        }
    }

    match run {
        Some(run) => {
            if !run.proof.complete {
                items.push(delivery_state_item(
                    "proof-incomplete",
                    format!(
                        "implementation proof is incomplete ({}/{})",
                        run.proof.completed_agents, run.proof.total_agents
                    ),
                    Some(format!("proof source {}", run.proof.proof_source)),
                ));
            }
            let missing_artifacts = run
                .proof
                .declared_artifacts
                .iter()
                .filter(|artifact| !artifact.exists)
                .map(|artifact| artifact.path.clone())
                .collect::<Vec<_>>();
            if !missing_artifacts.is_empty() {
                items.push(delivery_state_item(
                    "proof-artifact-missing",
                    "declared proof artifacts are missing".to_string(),
                    Some(missing_artifacts.join(", ")),
                ));
            }
            if run.proof.proof_source != "structured-envelope" {
                items.push(delivery_state_item(
                    "proof-backing",
                    "proof still depends on compatibility backing".to_string(),
                    Some(run.proof.proof_source.clone()),
                ));
            }
            let incomplete_agents = run
                .agents
                .iter()
                .filter(|agent| !agent.proof_complete)
                .map(|agent| agent.id.clone())
                .collect::<Vec<_>>();
            if !incomplete_agents.is_empty() {
                items.push(delivery_state_item(
                    "agent-proof-pending",
                    "agent proof markers are still incomplete".to_string(),
                    Some(incomplete_agents.join(", ")),
                ));
            }
            if !matches!(
                release.promotion_state,
                Some(wave_domain::WavePromotionState::Ready)
            ) {
                items.push(delivery_state_item(
                    "promotion-pending",
                    "promotion is not yet ready".to_string(),
                    release.last_decision.clone(),
                ));
            }
        }
        None => items.push(delivery_state_item(
            "run-missing",
            "no release candidate run is recorded".to_string(),
            None,
        )),
    }

    if wave.rerun_requested {
        items.push(delivery_state_item(
            "rerun-requested",
            "an active rerun request is still open".to_string(),
            None,
        ));
    }
    if !signoff.pending_closure_agents.is_empty() {
        items.push(delivery_state_item(
            "signoff-pending",
            "required closure signoff is incomplete".to_string(),
            Some(signoff.pending_closure_agents.join(", ")),
        ));
    }
    if !signoff.pending_operator_actions.is_empty() {
        items.push(delivery_state_item(
            "operator-action-pending",
            "operator review is still pending".to_string(),
            Some(signoff.pending_operator_actions.join(", ")),
        ));
    }
    if signoff.manual_close_applied {
        items.push(delivery_state_item(
            "manual-close",
            "manual close override is active".to_string(),
            closure_override.and_then(|record| record.detail.clone()),
        ));
    } else if operator_objects
        .iter()
        .any(|item| matches!(item.kind, OperatorActionableKind::Override))
    {
        items.push(delivery_state_item(
            "manual-close-review",
            "manual close review item is still present".to_string(),
            None,
        ));
    }

    items
}

fn design_is_ready(design: &AcceptanceDesignIntentSnapshot) -> bool {
    matches!(
        design.completeness,
        DesignCompletenessState::StructurallyComplete
            | DesignCompletenessState::ImplementationReady
            | DesignCompletenessState::Verified
    ) && design.blocker_count == 0
        && design.contradiction_count == 0
        && design.pending_human_input_count == 0
        && design.ambiguous_dependency_count == 0
}

fn acceptance_blocking_reasons(
    design: &AcceptanceDesignIntentSnapshot,
    implementation: &AcceptanceImplementationSnapshot,
    release: &AcceptanceReleaseSnapshot,
    signoff: &AcceptanceSignoffSnapshot,
    release_state: ReleaseReadinessState,
    known_risks: &[DeliveryStateItem],
    outstanding_debt: &[DeliveryStateItem],
) -> Vec<String> {
    let mut reasons = Vec::new();

    if !design_is_ready(design) {
        reasons.push(format!(
            "design intent is {} with {} blocker(s), {} contradiction(s), and {} pending human input(s)",
            debug_label(design.completeness),
            design.blocker_count,
            design.contradiction_count,
            design.pending_human_input_count
        ));
    }
    if !implementation.proof_complete {
        reasons.push(format!(
            "implementation proof is only {}/{} complete",
            implementation.completed_agents, implementation.total_agents
        ));
    }
    if implementation.replay_ok == Some(false) {
        reasons.push("replay validation has issues".to_string());
    }
    if release.merge_blocked {
        reasons.push("promotion is merge blocked".to_string());
    } else if matches!(release_state, ReleaseReadinessState::AwaitingPromotion) {
        reasons.push("promotion is not ready".to_string());
    }
    if !signoff.complete {
        match signoff.state {
            AcceptanceSignoffState::PendingEvidence => reasons.push(
                "signoff cannot begin until proof and release evidence are complete".to_string(),
            ),
            AcceptanceSignoffState::AwaitingClosure => reasons.push(format!(
                "required signoff is pending from {}",
                signoff.pending_closure_agents.join(", ")
            )),
            AcceptanceSignoffState::AwaitingOperator => {
                reasons.push("operator review is still pending".to_string())
            }
            AcceptanceSignoffState::BlockedByOverride => {
                reasons.push("manual close override is active".to_string())
            }
            AcceptanceSignoffState::SignedOff => {}
        }
    }
    if reasons.is_empty() && !known_risks.is_empty() {
        reasons.push(format!("{} known risk(s) remain", known_risks.len()));
    }
    if reasons.is_empty() && !outstanding_debt.is_empty() {
        reasons.push(format!(
            "{} outstanding delivery debt item(s) remain",
            outstanding_debt.len()
        ));
    }

    reasons
}

fn delivery_state_item(
    code: impl Into<String>,
    summary: impl Into<String>,
    detail: Option<String>,
) -> DeliveryStateItem {
    DeliveryStateItem {
        code: code.into(),
        summary: summary.into(),
        detail,
    }
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
            apply_closure_override_supported: operator.control.apply_closure_override_supported,
            clear_closure_override_supported: operator.control.clear_closure_override_supported,
            approve_operator_action_supported: operator.control.approve_operator_action_supported,
            reject_operator_action_supported: operator.control.reject_operator_action_supported,
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
        orchestrator: OrchestratorPanelSnapshot {
            mode: "operator".to_string(),
            active: false,
            multi_agent_wave_count: 0,
            selected_wave_id: None,
            autonomous_wave_ids: Vec::new(),
            pending_proposal_count: 0,
            autonomous_action_count: 0,
            failed_head_turn_count: 0,
            unresolved_recovery_count: 0,
            recent_autonomous_actions: Vec::new(),
            recent_autonomous_failures: Vec::new(),
            waves: Vec::new(),
            directives: Vec::new(),
        },
    }
}

fn build_orchestrator_panel_snapshot(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    planning_waves: &[wave_control_plane::WaveStatusReadModel],
    active_run_details: &[ActiveRunDetail],
    turns: &[wave_domain::OperatorShellTurnRecord],
    proposals: &[wave_domain::HeadProposalRecord],
) -> Result<OrchestratorPanelSnapshot> {
    let directives = list_control_directives(root, config, None)?;
    let deliveries = list_directive_deliveries(root, config, None)?;
    let recovery_plans = list_recovery_plans(root, config, None)?;
    let recovery_actions = list_recovery_actions(root, config, None)?;
    let active_wave_ids = active_run_details
        .iter()
        .map(|run| run.wave_id)
        .collect::<BTreeSet<_>>();
    let planning_waves_by_id = planning_waves
        .iter()
        .map(|wave| (wave.id, wave))
        .collect::<BTreeMap<_, _>>();
    let multi_agent_waves = waves
        .iter()
        .filter(|wave| wave.is_multi_agent())
        .collect::<Vec<_>>();
    let selected_wave_id = active_wave_ids
        .iter()
        .next()
        .copied()
        .or_else(|| multi_agent_waves.first().map(|wave| wave.metadata.id));
    let mut modes_by_wave = BTreeMap::new();
    for wave in &multi_agent_waves {
        if let Some(session) = latest_orchestrator_session(root, config, wave.metadata.id)? {
            modes_by_wave.insert(wave.metadata.id, session.mode);
        }
    }
    let autonomous_wave_ids = active_wave_ids
        .iter()
        .copied()
        .filter(|wave_id| {
            modes_by_wave
                .get(wave_id)
                .is_some_and(|mode| matches!(mode, wave_domain::OrchestratorMode::Autonomous))
        })
        .collect::<Vec<_>>();
    let mode = if autonomous_wave_ids.is_empty() {
        modes_by_wave
            .get(&selected_wave_id.unwrap_or_default())
            .copied()
            .unwrap_or(wave_domain::OrchestratorMode::Operator)
    } else if autonomous_wave_ids.len() == active_wave_ids.len() && !active_wave_ids.is_empty() {
        wave_domain::OrchestratorMode::Autonomous
    } else {
        wave_domain::OrchestratorMode::Operator
    };
    let directive_snapshots = directives
        .into_iter()
        .map(|directive| {
            let delivery = deliveries
                .iter()
                .find(|delivery| delivery.directive_id == directive.directive_id);
            DirectiveSnapshot {
                directive_id: directive.directive_id.as_str().to_string(),
                wave_id: directive.wave_id,
                agent_id: directive.agent_id.clone(),
                kind: format!("{:?}", directive.kind).to_ascii_lowercase(),
                origin: format!("{:?}", directive.origin).to_ascii_lowercase(),
                message: directive.message.clone(),
                requested_by: directive.requested_by.clone(),
                requested_at_ms: directive.requested_at_ms,
                delivery_state: delivery
                    .map(|delivery| format!("{:?}", delivery.state).to_ascii_lowercase()),
                delivery_detail: delivery.and_then(|delivery| delivery.detail.clone()),
            }
        })
        .collect::<Vec<_>>();
    let pending_proposals_by_wave = proposals
        .iter()
        .filter(|proposal| matches!(proposal.state, wave_domain::HeadProposalState::Pending))
        .fold(BTreeMap::<u32, usize>::new(), |mut counts, proposal| {
            *counts.entry(proposal.wave_id).or_default() += 1;
            counts
        });
    let autonomous_actions = proposals
        .iter()
        .filter_map(|proposal| {
            proposal.resolution.as_ref().and_then(|resolution| {
                matches!(
                    resolution.kind,
                    wave_domain::HeadProposalResolutionKind::AutonomousApplied
                )
                .then(|| AutonomousActionSnapshot {
                    proposal_id: proposal.proposal_id.as_str().to_string(),
                    wave_id: proposal.wave_id,
                    agent_id: proposal.agent_id.clone(),
                    summary: proposal.summary.clone(),
                    resolution: debug_label(resolution.kind),
                    updated_at_ms: proposal.updated_at_ms,
                })
            })
        })
        .collect::<Vec<_>>();
    let latest_recovery_by_wave = recovery_plans.into_iter().fold(
        BTreeMap::<u32, wave_domain::RecoveryPlanRecord>::new(),
        |mut acc, recovery_plan| {
            let replace = acc
                .get(&recovery_plan.wave_id)
                .map(|current| {
                    (
                        recovery_plan.updated_at_ms.max(recovery_plan.created_at_ms),
                        recovery_plan.recovery_plan_id.as_str(),
                    ) >= (
                        current.updated_at_ms.max(current.created_at_ms),
                        current.recovery_plan_id.as_str(),
                    )
                })
                .unwrap_or(true);
            if replace {
                acc.insert(recovery_plan.wave_id, recovery_plan);
            }
            acc
        },
    );
    let _recovery_actions_by_plan = recovery_actions.into_iter().fold(
        BTreeMap::<String, Vec<wave_domain::RecoveryActionRecord>>::new(),
        |mut acc, action| {
            acc.entry(action.recovery_plan_id.as_str().to_string())
                .or_default()
                .push(action);
            acc
        },
    );
    let autonomous_action_counts =
        autonomous_actions
            .iter()
            .fold(BTreeMap::<u32, usize>::new(), |mut counts, action| {
                *counts.entry(action.wave_id).or_default() += 1;
                counts
            });
    let mut latest_head_turn_by_wave = BTreeMap::new();
    let mut latest_failed_turn_by_wave = BTreeMap::new();
    let mut recent_autonomous_failures = turns
        .iter()
        .filter(|turn| turn.status == wave_domain::OperatorShellTurnStatus::Failed)
        .filter_map(|turn| {
            turn.wave_id.map(|wave_id| AutonomousFailureSnapshot {
                turn_id: turn.turn_id.as_str().to_string(),
                wave_id,
                agent_id: turn.agent_id.clone(),
                summary: first_line(turn.output.as_deref()).unwrap_or_else(|| turn.input.clone()),
                detail: turn.failed_reason.clone().or_else(|| turn.output.clone()),
                created_at_ms: turn.created_at_ms,
            })
        })
        .collect::<Vec<_>>();
    recent_autonomous_failures.sort_by_key(|item| item.created_at_ms);
    if recent_autonomous_failures.len() > 8 {
        recent_autonomous_failures =
            recent_autonomous_failures.split_off(recent_autonomous_failures.len() - 8);
    }
    for turn in turns {
        if let Some(wave_id) = turn.wave_id {
            if turn.origin == wave_domain::OperatorShellTurnOrigin::Head {
                latest_head_turn_by_wave.insert(wave_id, turn);
            }
            if turn.status == wave_domain::OperatorShellTurnStatus::Failed {
                latest_failed_turn_by_wave.insert(wave_id, turn);
            }
        }
    }
    let waves = multi_agent_waves
        .into_iter()
        .map(|wave| {
            let active_run = active_run_details
                .iter()
                .find(|run| run.wave_id == wave.metadata.id);
            let mas_detail = active_run.and_then(|run| run.mas.as_ref());
            let planning_wave = planning_waves_by_id.get(&wave.metadata.id).copied();
            let recovery_plan = latest_recovery_by_wave.get(&wave.metadata.id);
            let statuses = active_run
                .map(|run| {
                    run.agents
                        .iter()
                        .map(|agent| {
                            (
                                agent.id.clone(),
                                format!("{:?}", agent.status).to_ascii_lowercase(),
                            )
                        })
                        .collect::<BTreeMap<_, _>>()
                })
                .unwrap_or_default();
            let mode = modes_by_wave
                .get(&wave.metadata.id)
                .copied()
                .unwrap_or(wave_domain::OrchestratorMode::Operator);
            WaveOrchestratorSnapshot {
                wave_id: wave.metadata.id,
                title: wave.metadata.title.clone(),
                execution_model: "multi-agent".to_string(),
                mode: debug_label(mode),
                active_run_id: active_run.map(|run| run.run_id.clone()),
                pending_proposal_count: pending_proposals_by_wave
                    .get(&wave.metadata.id)
                    .copied()
                    .unwrap_or_default(),
                autonomous_action_count: autonomous_action_counts
                    .get(&wave.metadata.id)
                    .copied()
                    .unwrap_or_default(),
                recovery_required: planning_wave
                    .map(|wave| wave.recovery.required)
                    .unwrap_or_else(|| {
                        recovery_plan
                            .map(|plan| {
                                !matches!(plan.status, wave_domain::RecoveryPlanStatus::Resolved)
                            })
                            .unwrap_or(false)
                    }),
                last_head_turn_at_ms: latest_head_turn_by_wave
                    .get(&wave.metadata.id)
                    .map(|turn| turn.created_at_ms),
                last_head_summary: latest_head_turn_by_wave
                    .get(&wave.metadata.id)
                    .and_then(|turn| first_line(turn.output.as_deref())),
                last_autonomous_failure: latest_failed_turn_by_wave.get(&wave.metadata.id).map(
                    |turn| {
                        first_line(turn.failed_reason.as_deref())
                            .or_else(|| first_line(turn.output.as_deref()))
                            .unwrap_or_else(|| "autonomous head failure".to_string())
                    },
                ),
                agents: wave
                    .agents
                    .iter()
                    .map(|agent| MasAgentSnapshot {
                        id: agent.id.clone(),
                        title: agent.title.clone(),
                        barrier_class: barrier_class_label(agent.barrier_class).to_string(),
                        depends_on_agents: agent.depends_on_agents.clone(),
                        writes_artifacts: agent.writes_artifacts.clone(),
                        exclusive_resources: agent.exclusive_resources.clone(),
                        status: statuses
                            .get(&agent.id)
                            .cloned()
                            .unwrap_or_else(|| "planned".to_string()),
                        merge_state: mas_detail.and_then(|detail| {
                            detail
                                .merges
                                .iter()
                                .find(|merge| merge.agent_id == agent.id)
                                .map(|merge| merge.disposition.clone())
                        }),
                        sandbox_id: mas_detail.and_then(|detail| {
                            detail
                                .sandboxes
                                .iter()
                                .find(|sandbox| sandbox.agent_id == agent.id)
                                .map(|sandbox| sandbox.sandbox_id.clone())
                        }),
                        heartbeat_age_ms: planning_wave.and_then(|wave_state| {
                            wave_state
                                .ownership
                                .active_leases
                                .iter()
                                .chain(wave_state.ownership.stale_leases.iter())
                                .find(|lease| {
                                    task_id_agent_id(&wave_domain::TaskId::new(
                                        lease.task_id.clone(),
                                    ))
                                    .as_deref()
                                        == Some(agent.id.as_str())
                                })
                                .map(|lease| lease.heartbeat_at_ms.unwrap_or(lease.granted_at_ms))
                                .and_then(|heartbeat_at_ms| {
                                    now_epoch_ms()
                                        .ok()
                                        .map(|now| now.saturating_sub(heartbeat_at_ms))
                                })
                        }),
                        pending_directive_count: directive_snapshots
                            .iter()
                            .filter(|directive| {
                                directive.wave_id == wave.metadata.id
                                    && directive.agent_id.as_deref() == Some(agent.id.as_str())
                                    && directive.delivery_state.as_deref() != Some("rejected")
                                    && directive.delivery_state.as_deref() != Some("acked")
                            })
                            .count(),
                        last_head_action: proposals
                            .iter()
                            .filter(|proposal| proposal.wave_id == wave.metadata.id)
                            .filter(|proposal| {
                                proposal.agent_id.as_deref() == Some(agent.id.as_str())
                            })
                            .max_by_key(|proposal| proposal.updated_at_ms)
                            .map(|proposal| proposal.summary.clone()),
                        recovery_state: recovery_plan.and_then(|plan| {
                            plan.agent_plans
                                .iter()
                                .find(|entry| entry.agent_id == agent.id)
                                .map(|entry| format!("{:?}", entry.cause).to_ascii_lowercase())
                        }),
                        barrier_reasons: build_agent_barrier_reasons(
                            wave, active_run, agent, mas_detail,
                        ),
                    })
                    .collect(),
            }
        })
        .collect::<Vec<_>>();
    let mut recent_autonomous_actions = autonomous_actions;
    recent_autonomous_actions.sort_by_key(|item| item.updated_at_ms);
    if recent_autonomous_actions.len() > 8 {
        recent_autonomous_actions =
            recent_autonomous_actions.split_off(recent_autonomous_actions.len() - 8);
    }
    Ok(OrchestratorPanelSnapshot {
        mode: if !autonomous_wave_ids.is_empty()
            && autonomous_wave_ids.len() != active_wave_ids.len()
        {
            "mixed".to_string()
        } else {
            match mode {
                wave_domain::OrchestratorMode::Operator => "operator".to_string(),
                wave_domain::OrchestratorMode::Autonomous => "autonomous".to_string(),
            }
        },
        active: !autonomous_wave_ids.is_empty(),
        multi_agent_wave_count: waves.len(),
        selected_wave_id,
        autonomous_wave_ids,
        pending_proposal_count: pending_proposals_by_wave.values().sum(),
        autonomous_action_count: autonomous_action_counts.values().sum(),
        failed_head_turn_count: recent_autonomous_failures.len(),
        unresolved_recovery_count: latest_recovery_by_wave
            .values()
            .filter(|plan| !matches!(plan.status, wave_domain::RecoveryPlanStatus::Resolved))
            .count(),
        recent_autonomous_actions,
        recent_autonomous_failures,
        waves,
        directives: directive_snapshots,
    })
}

fn first_line(value: Option<&str>) -> Option<String> {
    value.and_then(|text| {
        text.lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn build_operator_shell_snapshot(
    snapshot: &OperatorSnapshot,
    session: Option<&wave_domain::OperatorShellSessionRecord>,
    turns: &[wave_domain::OperatorShellTurnRecord],
    proposals: &[wave_domain::HeadProposalRecord],
) -> OperatorShellSnapshot {
    let mut transcript = build_operator_shell_transcript(snapshot, turns);
    transcript.sort_by_key(|item| (item.created_at_ms, item.item_id.clone()));
    if transcript.len() > 40 {
        transcript = transcript.split_off(transcript.len().saturating_sub(40));
    }
    let last_event_at_ms = transcript.last().map(|item| item.created_at_ms);

    OperatorShellSnapshot {
        default_target: build_default_shell_target(snapshot, session),
        session: session.map(build_operator_shell_session_snapshot),
        transcript,
        proposals: build_operator_shell_proposals(proposals),
        command_availability: default_shell_command_availability(),
        commands: default_shell_commands(),
        last_event_at_ms,
    }
}

fn build_default_shell_target(
    snapshot: &OperatorSnapshot,
    session: Option<&wave_domain::OperatorShellSessionRecord>,
) -> OperatorShellTargetSnapshot {
    if let Some(session) = session {
        let scope_label = match session.scope {
            wave_domain::OperatorShellScope::Head => "Head",
            wave_domain::OperatorShellScope::Wave => "Wave",
            wave_domain::OperatorShellScope::Agent => "Agent",
        };
        return OperatorShellTargetSnapshot {
            scope: debug_label(session.scope),
            wave_id: session.wave_id,
            agent_id: session.agent_id.clone(),
            label: match session.agent_id.as_deref() {
                Some(agent_id) => format!(
                    "{scope_label} / Wave {} / {}",
                    session.wave_id.unwrap_or_default(),
                    agent_id
                ),
                None if session.wave_id.is_some() => {
                    format!(
                        "{scope_label} / Wave {}",
                        session.wave_id.unwrap_or_default()
                    )
                }
                None => scope_label.to_string(),
            },
            summary: format!(
                "resumed operator shell session {}",
                session.session_id.as_str()
            ),
        };
    }
    if let Some(run) = snapshot.active_run_details.first() {
        return OperatorShellTargetSnapshot {
            scope: "head".to_string(),
            wave_id: None,
            agent_id: None,
            label: "Head".to_string(),
            summary: format!(
                "default operator shell target for active waves; currently includes wave {} {}",
                run.wave_id, run.wave_title
            ),
        };
    }

    if let Some(wave_id) = snapshot.dashboard.next_ready_wave_ids.first().copied() {
        let title = snapshot
            .planning
            .waves
            .iter()
            .find(|wave| wave.id == wave_id)
            .map(|wave| wave.title.clone())
            .unwrap_or_else(|| format!("wave {wave_id}"));
        return OperatorShellTargetSnapshot {
            scope: "head".to_string(),
            wave_id: None,
            agent_id: None,
            label: "Head".to_string(),
            summary: format!("default operator shell target for ready wave {wave_id} {title}"),
        };
    }

    if let Some(wave) = snapshot.planning.waves.first() {
        return OperatorShellTargetSnapshot {
            scope: "head".to_string(),
            wave_id: None,
            agent_id: None,
            label: "Head".to_string(),
            summary: format!(
                "default operator shell target for wave {} {}",
                wave.id, wave.title
            ),
        };
    }

    OperatorShellTargetSnapshot {
        scope: "head".to_string(),
        wave_id: None,
        agent_id: None,
        label: "Head".to_string(),
        summary: "operator shell has no wave target yet".to_string(),
    }
}

fn build_operator_shell_transcript(
    snapshot: &OperatorSnapshot,
    turns: &[wave_domain::OperatorShellTurnRecord],
) -> Vec<OperatorShellTranscriptItem> {
    let mut items = Vec::new();

    for turn in turns {
        let kind = match turn.origin {
            wave_domain::OperatorShellTurnOrigin::Operator => "operator-turn",
            wave_domain::OperatorShellTurnOrigin::Head => "head-turn",
            wave_domain::OperatorShellTurnOrigin::System => "system-turn",
        };
        let title = match turn.origin {
            wave_domain::OperatorShellTurnOrigin::Operator => "Operator input",
            wave_domain::OperatorShellTurnOrigin::Head => "Head response",
            wave_domain::OperatorShellTurnOrigin::System => "System update",
        };
        let detail = turn.output.clone().unwrap_or_else(|| turn.input.clone());
        items.push(OperatorShellTranscriptItem {
            item_id: turn.turn_id.as_str().to_string(),
            kind: kind.to_string(),
            title: title.to_string(),
            detail,
            origin: Some(debug_label(turn.origin)),
            wave_id: turn.wave_id,
            agent_id: turn.agent_id.clone(),
            session_id: Some(turn.session_id.as_str().to_string()),
            turn_id: Some(turn.turn_id.as_str().to_string()),
            proposal_id: None,
            created_at_ms: turn.created_at_ms,
            status: Some(debug_label(turn.status)),
        });
    }

    for run in snapshot.latest_run_details.iter().take(8) {
        let mut detail_parts = Vec::new();
        if let Some(agent_id) = run.current_agent_id.as_deref() {
            let title = run
                .current_agent_title
                .as_deref()
                .unwrap_or("current agent");
            detail_parts.push(format!("agent {agent_id} {title}"));
        }
        detail_parts.push(format!(
            "proof {}/{} complete={}",
            run.proof.completed_agents, run.proof.total_agents, run.proof.complete
        ));
        if let Some(source) = run.activity_source.as_deref() {
            detail_parts.push(format!("activity via {source}"));
        }
        let excerpt = run
            .activity_excerpt
            .lines()
            .next()
            .unwrap_or("no live activity");
        detail_parts.push(excerpt.to_string());

        items.push(OperatorShellTranscriptItem {
            item_id: format!("run-{}", run.run_id),
            kind: "run".to_string(),
            title: format!(
                "Wave {} {}",
                run.wave_id,
                debug_label(run.status).replace('_', " ")
            ),
            detail: detail_parts.join(" | "),
            origin: Some("system".to_string()),
            wave_id: Some(run.wave_id),
            agent_id: run.current_agent_id.clone(),
            session_id: None,
            turn_id: None,
            proposal_id: None,
            created_at_ms: run
                .last_activity_at_ms
                .or(run.started_at_ms)
                .unwrap_or(run.created_at_ms),
            status: Some(debug_label(run.status)),
        });
    }

    for directive in &snapshot.panels.orchestrator.directives {
        let title = if let Some(agent_id) = directive.agent_id.as_deref() {
            format!("Directive for {agent_id}")
        } else {
            format!("Wave {} directive", directive.wave_id)
        };
        let mut detail = Vec::new();
        detail.push(format!(
            "{} via {}",
            directive.kind.replace('_', " "),
            directive.origin.replace('_', " ")
        ));
        if let Some(message) = directive.message.as_deref() {
            detail.push(message.to_string());
        }
        if let Some(state) = directive.delivery_state.as_deref() {
            detail.push(format!("delivery {state}"));
        }
        if let Some(extra) = directive.delivery_detail.as_deref() {
            detail.push(extra.to_string());
        }
        items.push(OperatorShellTranscriptItem {
            item_id: directive.directive_id.clone(),
            kind: "directive".to_string(),
            title,
            detail: detail.join(" | "),
            origin: Some("system".to_string()),
            wave_id: Some(directive.wave_id),
            agent_id: directive.agent_id.clone(),
            session_id: None,
            turn_id: None,
            proposal_id: None,
            created_at_ms: directive.requested_at_ms,
            status: directive.delivery_state.clone(),
        });
    }

    for item in &snapshot.operator_objects {
        let mut detail = Vec::new();
        if let Some(waiting_on) = item.waiting_on.as_deref() {
            detail.push(waiting_on.to_string());
        }
        if let Some(next_action) = item.next_action.as_deref() {
            detail.push(next_action.to_string());
        }
        if let Some(extra) = item.detail.as_deref() {
            detail.push(extra.to_string());
        }
        items.push(OperatorShellTranscriptItem {
            item_id: item.record_id.clone(),
            kind: operator_actionable_kind_label(item.kind).to_string(),
            title: item.summary.clone(),
            detail: if detail.is_empty() {
                format!("state={}", item.state)
            } else {
                detail.join(" | ")
            },
            origin: Some("system".to_string()),
            wave_id: Some(item.wave_id),
            agent_id: None,
            session_id: None,
            turn_id: None,
            proposal_id: matches!(item.kind, OperatorActionableKind::Proposal)
                .then(|| item.record_id.clone()),
            created_at_ms: item.created_at_ms.unwrap_or_default(),
            status: Some(item.state.clone()),
        });
    }

    for intent in &snapshot.rerun_intents {
        items.push(OperatorShellTranscriptItem {
            item_id: intent
                .request_id
                .clone()
                .unwrap_or_else(|| format!("rerun-wave-{}", intent.wave_id)),
            kind: "rerun".to_string(),
            title: format!(
                "Wave {} rerun {}",
                intent.wave_id,
                debug_label(intent.status)
            ),
            detail: format!("scope={} | {}", debug_label(intent.scope), intent.reason),
            origin: Some("system".to_string()),
            wave_id: Some(intent.wave_id),
            agent_id: None,
            session_id: None,
            turn_id: None,
            proposal_id: None,
            created_at_ms: intent.requested_at_ms,
            status: Some(debug_label(intent.status)),
        });
    }

    for record in &snapshot.closure_overrides {
        items.push(OperatorShellTranscriptItem {
            item_id: record.override_id.clone(),
            kind: "manual-close".to_string(),
            title: format!(
                "Wave {} manual close {}",
                record.wave_id,
                if record.is_active() {
                    "applied"
                } else {
                    "cleared"
                }
            ),
            detail: record
                .detail
                .clone()
                .unwrap_or_else(|| record.reason.clone()),
            origin: Some("system".to_string()),
            wave_id: Some(record.wave_id),
            agent_id: None,
            session_id: None,
            turn_id: None,
            proposal_id: None,
            created_at_ms: record.applied_at_ms,
            status: Some(if record.is_active() {
                "applied".to_string()
            } else {
                "cleared".to_string()
            }),
        });
    }

    items
}

fn build_operator_shell_session_snapshot(
    session: &wave_domain::OperatorShellSessionRecord,
) -> OperatorShellSessionSnapshot {
    OperatorShellSessionSnapshot {
        session_id: session.session_id.as_str().to_string(),
        scope: debug_label(session.scope),
        wave_id: session.wave_id,
        agent_id: session.agent_id.clone(),
        tab: session.tab.clone(),
        follow_mode: session.follow_mode.clone(),
        mode: debug_label(session.mode),
        active: session.active,
        started_at_ms: session.started_at_ms,
        updated_at_ms: session.updated_at_ms,
    }
}

fn build_operator_shell_proposals(
    proposals: &[wave_domain::HeadProposalRecord],
) -> Vec<OperatorShellProposalItem> {
    proposals
        .iter()
        .map(|proposal| OperatorShellProposalItem {
            proposal_id: proposal.proposal_id.as_str().to_string(),
            session_id: proposal.session_id.as_str().to_string(),
            turn_id: proposal.turn_id.as_str().to_string(),
            cycle_id: proposal.cycle_id.clone(),
            wave_id: proposal.wave_id,
            agent_id: proposal.agent_id.clone(),
            action_kind: debug_label(proposal.action_kind),
            state: debug_label(proposal.state),
            resolution: proposal
                .resolution
                .as_ref()
                .map(|resolution| debug_label(resolution.kind)),
            resolved_by: proposal
                .resolution
                .as_ref()
                .map(|resolution| resolution.resolved_by.clone()),
            resolved_at_ms: proposal
                .resolution
                .as_ref()
                .map(|resolution| resolution.resolved_at_ms),
            summary: proposal.summary.clone(),
            detail: proposal.detail.clone(),
            created_at_ms: proposal.created_at_ms,
            updated_at_ms: proposal.updated_at_ms,
        })
        .collect()
}

fn default_shell_command_availability() -> BTreeMap<String, bool> {
    default_shell_commands()
        .into_iter()
        .map(|command| (command.name, true))
        .collect()
}

fn default_shell_commands() -> Vec<OperatorShellCommand> {
    vec![
        OperatorShellCommand {
            name: "/wave".to_string(),
            usage: "/wave <id>".to_string(),
            summary: "retarget the shell to a wave".to_string(),
        },
        OperatorShellCommand {
            name: "/agent".to_string(),
            usage: "/agent <id>".to_string(),
            summary: "retarget the shell to an agent in the selected wave".to_string(),
        },
        OperatorShellCommand {
            name: "/scope".to_string(),
            usage: "/scope head|wave|agent".to_string(),
            summary: "switch the current shell target scope".to_string(),
        },
        OperatorShellCommand {
            name: "/mode".to_string(),
            usage: "/mode operator|autonomous".to_string(),
            summary: "change orchestrator mode for the current target or active head workspace"
                .to_string(),
        },
        OperatorShellCommand {
            name: "/launch".to_string(),
            usage: "/launch [wave-id]".to_string(),
            summary: "launch the selected or explicit wave".to_string(),
        },
        OperatorShellCommand {
            name: "/rerun".to_string(),
            usage: "/rerun [full|closure-only|promotion-only]".to_string(),
            summary: "request rerun for the selected wave".to_string(),
        },
        OperatorShellCommand {
            name: "/pause".to_string(),
            usage: "/pause".to_string(),
            summary: "pause the selected MAS agent".to_string(),
        },
        OperatorShellCommand {
            name: "/resume".to_string(),
            usage: "/resume".to_string(),
            summary: "resume the selected MAS agent".to_string(),
        },
        OperatorShellCommand {
            name: "/rerun-agent".to_string(),
            usage: "/rerun-agent".to_string(),
            summary: "request an agent-only rerun for the selected MAS agent".to_string(),
        },
        OperatorShellCommand {
            name: "/rebase".to_string(),
            usage: "/rebase".to_string(),
            summary: "rebase the selected MAS sandbox onto accepted state".to_string(),
        },
        OperatorShellCommand {
            name: "/reconcile".to_string(),
            usage: "/reconcile".to_string(),
            summary: "request reconciliation for the selected MAS agent".to_string(),
        },
        OperatorShellCommand {
            name: "/approve-merge".to_string(),
            usage: "/approve-merge".to_string(),
            summary: "approve the selected MAS merge or recovery item".to_string(),
        },
        OperatorShellCommand {
            name: "/reject-merge".to_string(),
            usage: "/reject-merge".to_string(),
            summary: "reject the selected MAS merge or recovery item".to_string(),
        },
        OperatorShellCommand {
            name: "/clear-rerun".to_string(),
            usage: "/clear-rerun".to_string(),
            summary: "clear rerun intent for the selected wave".to_string(),
        },
        OperatorShellCommand {
            name: "/approve".to_string(),
            usage: "/approve".to_string(),
            summary: "approve the selected operator action".to_string(),
        },
        OperatorShellCommand {
            name: "/reject".to_string(),
            usage: "/reject".to_string(),
            summary: "reject or dismiss the selected operator action".to_string(),
        },
        OperatorShellCommand {
            name: "/close".to_string(),
            usage: "/close".to_string(),
            summary: "prepare manual close for the selected wave".to_string(),
        },
        OperatorShellCommand {
            name: "/open".to_string(),
            usage: "/open overview|agents|queue|proof|control".to_string(),
            summary: "switch the right-side dashboard tab".to_string(),
        },
        OperatorShellCommand {
            name: "/follow".to_string(),
            usage: "/follow run|agent|off".to_string(),
            summary: "control transcript follow behavior".to_string(),
        },
        OperatorShellCommand {
            name: "/search".to_string(),
            usage: "/search <text>".to_string(),
            summary: "filter transcript rows by case-insensitive text".to_string(),
        },
        OperatorShellCommand {
            name: "/clear-search".to_string(),
            usage: "/clear-search".to_string(),
            summary: "clear the current transcript search".to_string(),
        },
        OperatorShellCommand {
            name: "/compare".to_string(),
            usage: "/compare wave <id> | /compare agent <id>".to_string(),
            summary: "open a shell-local compare view for waves or MAS agents".to_string(),
        },
        OperatorShellCommand {
            name: "/clear-compare".to_string(),
            usage: "/clear-compare".to_string(),
            summary: "clear the current compare view".to_string(),
        },
        OperatorShellCommand {
            name: "/help".to_string(),
            usage: "/help".to_string(),
            summary: "show shell commands and keybindings".to_string(),
        },
    ]
}

fn operator_actionable_kind_label(kind: OperatorActionableKind) -> &'static str {
    match kind {
        OperatorActionableKind::Approval => "approval",
        OperatorActionableKind::Proposal => "proposal",
        OperatorActionableKind::Override => "manual-close",
        OperatorActionableKind::Escalation => "escalation",
    }
}

fn barrier_class_label(barrier_class: BarrierClass) -> &'static str {
    match barrier_class {
        BarrierClass::Independent => "independent",
        BarrierClass::MergeAfter => "merge-after",
        BarrierClass::IntegrationBarrier => "integration-barrier",
        BarrierClass::ClosureBarrier => "closure-barrier",
        BarrierClass::ReportOnly => "report-only",
    }
}

fn build_agent_barrier_reasons(
    wave: &WaveDocument,
    active_run: Option<&ActiveRunDetail>,
    agent: &WaveAgent,
    mas: Option<&MasRunDetail>,
) -> Vec<String> {
    let Some(active_run) = active_run else {
        return Vec::new();
    };
    let completed = active_run
        .agents
        .iter()
        .filter(|candidate| candidate.status == WaveRunStatus::Succeeded)
        .map(|candidate| candidate.id.clone())
        .collect::<HashSet<_>>();
    let mut reasons = agent
        .depends_on_agents
        .iter()
        .filter(|dependency| !completed.contains(*dependency))
        .map(|dependency| format!("waiting on {dependency}"))
        .collect::<Vec<_>>();
    if let Some(detail) = mas {
        if detail
            .conflicted_agent_ids
            .iter()
            .any(|candidate| candidate == &agent.id)
        {
            reasons.push("merge conflicted".to_string());
        }
        if detail
            .invalidated_agent_ids
            .iter()
            .any(|candidate| candidate == &agent.id)
        {
            reasons.push("invalidated by newer accepted state".to_string());
        }
    }
    match agent.barrier_class {
        BarrierClass::IntegrationBarrier => {
            let open_implementation = wave
                .agents
                .iter()
                .filter(|candidate| !candidate.is_closure_agent())
                .filter(|candidate| !completed.contains(&candidate.id))
                .map(|candidate| candidate.id.clone())
                .collect::<Vec<_>>();
            if !open_implementation.is_empty() {
                reasons.push(format!(
                    "awaiting implementation frontier {}",
                    open_implementation.join(", ")
                ));
            }
        }
        BarrierClass::ClosureBarrier => {
            let open_non_report = wave
                .agents
                .iter()
                .filter(|candidate| candidate.id != agent.id)
                .filter(|candidate| !matches!(candidate.barrier_class, BarrierClass::ReportOnly))
                .filter(|candidate| !completed.contains(&candidate.id))
                .map(|candidate| candidate.id.clone())
                .collect::<Vec<_>>();
            if !open_non_report.is_empty() {
                reasons.push(format!(
                    "awaiting merged closure inputs {}",
                    open_non_report.join(", ")
                ));
            }
        }
        _ => {}
    }
    reasons
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

fn load_wave_design_details(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[wave_dark_factory::LintFinding],
    skill_catalog_issues: &[wave_dark_factory::SkillCatalogIssue],
    rerun_wave_ids: &HashSet<u32>,
    closure_override_wave_ids: &HashSet<u32>,
) -> Result<Vec<WaveDesignDetail>> {
    let compatibility_runs = load_canonical_compatibility_runs(root, config, waves)?;
    let scheduler_events = load_scheduler_events(root, config)?;
    let control_events = control_event_log(root, config).load_all()?;
    let reduced = reduce_planning_state_with_authorities(
        waves,
        findings,
        skill_catalog_issues,
        &compatibility_runs,
        rerun_wave_ids,
        closure_override_wave_ids,
        &scheduler_events,
        &control_events,
    );
    let human_inputs = latest_human_inputs_by_id(&control_events);
    let design_narratives = build_design_narrative_index(&control_events);

    Ok(reduced
        .waves
        .into_iter()
        .map(|wave| {
            let pending_human_inputs = wave
                .design
                .pending_human_input_request_ids
                .iter()
                .filter_map(|request_id| human_inputs.get(request_id).cloned())
                .collect::<Vec<_>>();
            let dependency_handshake_routes = dependency_handshake_routes(&pending_human_inputs);
            WaveDesignDetail {
                wave_id: wave.id,
                completeness: wave.design.completeness,
                blocker_reasons: wave.design.blocker_reasons,
                active_contradictions: design_narratives
                    .contradictions_by_wave
                    .get(&wave.id)
                    .cloned()
                    .unwrap_or_default(),
                unresolved_question_ids: wave.design.unresolved_question_ids,
                unresolved_assumption_ids: wave.design.unresolved_assumption_ids,
                pending_human_inputs,
                dependency_handshake_routes,
                invalidated_fact_ids: wave.design.invalidated_fact_ids,
                invalidated_decision_ids: wave.design.invalidated_decision_ids,
                invalidation_routes: design_narratives
                    .invalidation_routes_by_wave
                    .get(&wave.id)
                    .cloned()
                    .unwrap_or_default(),
                selectively_invalidated_task_ids: wave.design.selectively_invalidated_task_ids,
                superseded_decision_ids: wave.design.superseded_decision_ids,
                ambiguous_dependency_wave_ids: wave.design.ambiguous_dependency_wave_ids,
            }
        })
        .collect())
}

fn build_design_narrative_index(control_events: &[ControlEvent]) -> DesignNarrativeIndex {
    let mut sorted_events = control_events.to_vec();
    sorted_events.sort_by_key(|event| (event.created_at_ms, event.event_id.clone()));

    let mut facts = BTreeMap::new();
    let mut contradictions = BTreeMap::new();
    let mut human_inputs = BTreeMap::new();
    let mut lineages = BTreeMap::new();

    for event in sorted_events {
        match event.payload {
            ControlEventPayload::FactObserved { fact } => {
                facts.insert(fact.fact_id.as_str().to_string(), fact);
            }
            ControlEventPayload::ContradictionUpdated { contradiction } => {
                contradictions.insert(
                    contradiction.contradiction_id.as_str().to_string(),
                    contradiction,
                );
            }
            ControlEventPayload::HumanInputUpdated { request } => {
                human_inputs.insert(request.request_id.as_str().to_string(), request);
            }
            ControlEventPayload::LineageUpdated { lineage } => {
                lineages.insert(lineage_subject_key(&lineage), lineage);
            }
            _ => {}
        }
    }

    let mut contradictions_by_wave = BTreeMap::<u32, BTreeMap<String, ContradictionDetail>>::new();
    let mut invalidation_routes_by_wave = BTreeMap::<u32, BTreeSet<String>>::new();

    for contradiction in contradictions
        .values()
        .filter(|record| record.state.is_active())
    {
        let contradiction_id = contradiction.contradiction_id.as_str().to_string();
        let detail = ContradictionDetail {
            contradiction_id: contradiction_id.clone(),
            state: debug_label(contradiction.state),
            summary: contradiction.summary.clone(),
            detail: contradiction.detail.clone(),
            invalidated_refs: contradiction
                .invalidated_refs
                .iter()
                .map(format_lineage_ref)
                .collect(),
        };
        let mut affected_waves = BTreeSet::from([contradiction.wave_id]);

        for task_id in &contradiction.task_ids {
            if let Some(wave_id) = wave_id_from_task_id(task_id.as_str()) {
                affected_waves.insert(wave_id);
                invalidation_routes_by_wave
                    .entry(wave_id)
                    .or_default()
                    .insert(format!(
                        "contradiction {} involves task {}",
                        contradiction_id,
                        task_id.as_str()
                    ));
            }
        }

        for fact_id in &contradiction.fact_ids {
            if let Some(fact) = facts.get(fact_id.as_str()) {
                affected_waves.insert(fact.wave_id);
                invalidation_routes_by_wave
                    .entry(fact.wave_id)
                    .or_default()
                    .insert(format!(
                        "contradiction {} cites fact {}",
                        contradiction_id,
                        fact_id.as_str()
                    ));
            }
        }

        for invalidated in &contradiction.invalidated_refs {
            record_invalidation_routes(
                &mut affected_waves,
                &mut invalidation_routes_by_wave,
                &contradiction_id,
                invalidated,
                &facts,
                &human_inputs,
                &lineages,
            );
        }

        for wave_id in affected_waves {
            contradictions_by_wave
                .entry(wave_id)
                .or_default()
                .insert(contradiction_id.clone(), detail.clone());
        }
    }

    DesignNarrativeIndex {
        contradictions_by_wave: contradictions_by_wave
            .into_iter()
            .map(|(wave_id, details)| {
                let mut details = details.into_values().collect::<Vec<_>>();
                details.sort_by_key(|detail| detail.contradiction_id.clone());
                (wave_id, details)
            })
            .collect(),
        invalidation_routes_by_wave: invalidation_routes_by_wave
            .into_iter()
            .map(|(wave_id, routes)| (wave_id, routes.into_iter().collect()))
            .collect(),
    }
}

fn record_invalidation_routes(
    affected_waves: &mut BTreeSet<u32>,
    invalidation_routes_by_wave: &mut BTreeMap<u32, BTreeSet<String>>,
    contradiction_id: &str,
    invalidated: &LineageRef,
    facts: &BTreeMap<String, wave_domain::FactRecord>,
    human_inputs: &BTreeMap<String, HumanInputRequest>,
    lineages: &BTreeMap<String, LineageRecord>,
) {
    match invalidated {
        LineageRef::Fact(fact_id) => {
            if let Some(fact) = facts.get(fact_id.as_str()) {
                affected_waves.insert(fact.wave_id);
                invalidation_routes_by_wave
                    .entry(fact.wave_id)
                    .or_default()
                    .insert(format!(
                        "contradiction {} invalidates fact {}",
                        contradiction_id,
                        fact_id.as_str()
                    ));
            }
            for lineage in lineages
                .values()
                .filter(|lineage| lineage_depends_on_fact(lineage, fact_id.as_str()))
            {
                if let Some(decision_id) = lineage.decision_id() {
                    record_decision_route(
                        affected_waves,
                        invalidation_routes_by_wave,
                        contradiction_id,
                        decision_id.as_str(),
                        lineage,
                        Some(format!("fact {}", fact_id.as_str())),
                    );
                }
            }
        }
        LineageRef::Decision(decision_id) => {
            if let Some(lineage) = lineages.get(&format!("decision:{}", decision_id.as_str())) {
                record_decision_route(
                    affected_waves,
                    invalidation_routes_by_wave,
                    contradiction_id,
                    decision_id.as_str(),
                    lineage,
                    None,
                );
            }
        }
        LineageRef::Question(question_id) => {
            if let Some(lineage) = lineages.get(&format!("question:{}", question_id.as_str())) {
                affected_waves.insert(lineage.wave_id);
                invalidation_routes_by_wave
                    .entry(lineage.wave_id)
                    .or_default()
                    .insert(format!(
                        "contradiction {} reopens question {}",
                        contradiction_id,
                        question_id.as_str()
                    ));
            }
        }
        LineageRef::Assumption(assumption_id) => {
            if let Some(lineage) = lineages.get(&format!("assumption:{}", assumption_id.as_str())) {
                affected_waves.insert(lineage.wave_id);
                invalidation_routes_by_wave
                    .entry(lineage.wave_id)
                    .or_default()
                    .insert(format!(
                        "contradiction {} invalidates assumption {}",
                        contradiction_id,
                        assumption_id.as_str()
                    ));
            }
        }
        LineageRef::HumanInput(request_id) => {
            if let Some(request) = human_inputs.get(request_id.as_str()) {
                affected_waves.insert(request.wave_id);
                invalidation_routes_by_wave
                    .entry(request.wave_id)
                    .or_default()
                    .insert(format!(
                        "contradiction {} blocks human input {}",
                        contradiction_id,
                        request_id.as_str()
                    ));
            }
        }
    }
}

fn record_decision_route(
    affected_waves: &mut BTreeSet<u32>,
    invalidation_routes_by_wave: &mut BTreeMap<u32, BTreeSet<String>>,
    contradiction_id: &str,
    decision_id: &str,
    lineage: &LineageRecord,
    via: Option<String>,
) {
    affected_waves.insert(lineage.wave_id);
    let direct_route = match via {
        Some(via) => format!(
            "contradiction {} invalidates {} -> decision {}",
            contradiction_id, via, decision_id
        ),
        None => format!(
            "contradiction {} invalidates decision {}",
            contradiction_id, decision_id
        ),
    };
    invalidation_routes_by_wave
        .entry(lineage.wave_id)
        .or_default()
        .insert(direct_route);

    for task_id in &lineage.downstream_task_ids {
        if let Some(wave_id) = wave_id_from_task_id(task_id.as_str()) {
            affected_waves.insert(wave_id);
            invalidation_routes_by_wave
                .entry(wave_id)
                .or_default()
                .insert(format!(
                    "decision {} invalidates task {}",
                    decision_id,
                    task_id.as_str()
                ));
        }
    }
    for wave_id in &lineage.downstream_wave_ids {
        affected_waves.insert(*wave_id);
        invalidation_routes_by_wave
            .entry(*wave_id)
            .or_default()
            .insert(format!(
                "decision {} invalidates wave {}",
                decision_id, wave_id
            ));
    }
}

fn lineage_depends_on_fact(lineage: &LineageRecord, fact_id: &str) -> bool {
    lineage
        .supporting_fact_ids
        .iter()
        .any(|candidate| candidate.as_str() == fact_id)
        || lineage
            .upstream_refs
            .iter()
            .any(|reference| match reference {
                LineageRef::Fact(candidate) => candidate.as_str() == fact_id,
                _ => false,
            })
}

fn wave_id_from_task_id(task_id: &str) -> Option<u32> {
    task_id
        .strip_prefix("wave-")?
        .split(':')
        .next()?
        .parse()
        .ok()
}

fn format_lineage_ref(reference: &LineageRef) -> String {
    match reference {
        LineageRef::Fact(fact_id) => format!("fact:{}", fact_id.as_str()),
        LineageRef::Question(question_id) => format!("question:{}", question_id.as_str()),
        LineageRef::Assumption(assumption_id) => {
            format!("assumption:{}", assumption_id.as_str())
        }
        LineageRef::Decision(decision_id) => format!("decision:{}", decision_id.as_str()),
        LineageRef::HumanInput(request_id) => format!("human-input:{}", request_id.as_str()),
    }
}

fn lineage_subject_key(lineage: &LineageRecord) -> String {
    match &lineage.subject {
        LineageRecordSubject::Question { question_id } => {
            format!("question:{}", question_id.as_str())
        }
        LineageRecordSubject::Assumption { assumption_id } => {
            format!("assumption:{}", assumption_id.as_str())
        }
        LineageRecordSubject::Decision { decision_id }
        | LineageRecordSubject::SupersededDecision { decision_id, .. } => {
            format!("decision:{}", decision_id.as_str())
        }
    }
}

fn control_event_log(root: &Path, config: &ProjectConfig) -> ControlEventLog {
    ControlEventLog::new(
        config
            .resolved_paths(root)
            .authority
            .state_events_control_dir,
    )
}

fn latest_human_inputs_by_id(
    control_events: &[ControlEvent],
) -> HashMap<String, PendingHumanInputDetail> {
    let mut sorted_events = control_events.to_vec();
    sorted_events.sort_by_key(|event| (event.created_at_ms, event.event_id.clone()));

    let mut requests = HashMap::new();
    for event in sorted_events {
        let wave_domain::ControlEventPayload::HumanInputUpdated { request } = event.payload else {
            continue;
        };
        requests.insert(
            request.request_id.as_str().to_string(),
            PendingHumanInputDetail {
                request_id: request.request_id.as_str().to_string(),
                task_id: request
                    .task_id
                    .as_ref()
                    .map(|task_id| task_id.as_str().to_string()),
                state: request.state,
                workflow_kind: request.effective_workflow_kind(),
                route: request.route,
                prompt: request.prompt,
                requested_by: request.requested_by,
                answer: request.answer,
            },
        );
    }
    requests
}

fn dependency_handshake_routes(requests: &[PendingHumanInputDetail]) -> Vec<String> {
    let mut routes = requests
        .iter()
        .filter(|request| {
            matches!(
                request.workflow_kind,
                HumanInputWorkflowKind::DependencyHandshake
            )
        })
        .map(|request| request.route.trim())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    routes.sort();
    routes.dedup();
    routes
}

fn approval_waiting_on(request: &PendingHumanInputDetail) -> String {
    if matches!(
        request.workflow_kind,
        HumanInputWorkflowKind::DependencyHandshake
    ) {
        "operator dependency approval".to_string()
    } else {
        "operator approval".to_string()
    }
}

fn operator_actionable_kind_priority(kind: OperatorActionableKind) -> u8 {
    match kind {
        OperatorActionableKind::Approval => 0,
        OperatorActionableKind::Proposal => 1,
        OperatorActionableKind::Escalation => 2,
        OperatorActionableKind::Override => 3,
    }
}

fn open_escalation_records(records: &[CoordinationRecord]) -> Vec<CoordinationRecord> {
    let resolved_escalations = records
        .iter()
        .filter(|record| record.kind != CoordinationRecordKind::Escalation)
        .flat_map(|record| record.related_record_ids.iter().cloned())
        .collect::<HashSet<_>>();
    let mut escalations = records
        .iter()
        .filter(|record| record.kind == CoordinationRecordKind::Escalation)
        .filter(|record| !resolved_escalations.contains(&record.record_id))
        .cloned()
        .collect::<Vec<_>>();
    escalations.sort_by_key(|record| (record.created_at_ms, record.record_id.clone()));
    escalations
}

fn load_operator_actionable_items(
    root: &Path,
    config: &ProjectConfig,
    design_details: &[WaveDesignDetail],
    closure_overrides: &[WaveClosureOverrideRecord],
    head_proposals: &[wave_domain::HeadProposalRecord],
) -> Result<Vec<OperatorActionableItem>> {
    let mut items = Vec::new();
    let mut seen_approvals = HashSet::new();
    let pending_human_inputs = design_details
        .iter()
        .flat_map(|design| {
            design
                .pending_human_inputs
                .iter()
                .cloned()
                .map(|request| (request.request_id.clone(), request))
                .collect::<Vec<_>>()
        })
        .collect::<BTreeMap<_, _>>();
    for design in design_details {
        for request in &design.pending_human_inputs {
            if !seen_approvals.insert(request.request_id.clone()) {
                continue;
            }
            items.push(OperatorActionableItem {
                kind: OperatorActionableKind::Approval,
                wave_id: design.wave_id,
                record_id: request.request_id.clone(),
                state: debug_label(request.state),
                summary: request.prompt.clone(),
                detail: Some(format!(
                    "requested by {} via {}",
                    request.requested_by, request.route
                )),
                waiting_on: Some(approval_waiting_on(request)),
                next_action: Some("press u to approve or x to reject".to_string()),
                route: Some(request.route.clone()),
                task_id: request.task_id.clone(),
                source_run_id: None,
                evidence_count: 0,
                created_at_ms: None,
            });
        }
    }
    for record in closure_overrides.iter().filter(|record| record.is_active()) {
        items.push(OperatorActionableItem {
            kind: OperatorActionableKind::Override,
            wave_id: record.wave_id,
            record_id: record.override_id.clone(),
            state: "applied".to_string(),
            summary: record.reason.clone(),
            detail: record.detail.clone(),
            waiting_on: Some("manual close override is active".to_string()),
            next_action: Some("press M to clear".to_string()),
            route: None,
            task_id: None,
            source_run_id: Some(record.source_run_id.clone()),
            evidence_count: record.evidence_paths.len(),
            created_at_ms: Some(record.applied_at_ms),
        });
    }

    let coordination_root = config
        .resolved_paths(root)
        .authority
        .state_events_coordination_dir;
    let coordination_records = CoordinationLog::new(coordination_root).load_all()?;
    let escalations = open_escalation_records(&coordination_records);
    for record in escalations {
        let route = record
            .human_input_request_id
            .as_ref()
            .and_then(|request_id| pending_human_inputs.get(request_id.as_str()))
            .map(|request| request.route.clone());
        items.push(OperatorActionableItem {
            kind: OperatorActionableKind::Escalation,
            wave_id: record.wave_id,
            record_id: record.record_id,
            state: "open".to_string(),
            summary: record.summary,
            detail: record.detail,
            waiting_on: Some("operator escalation review".to_string()),
            next_action: Some("press u to acknowledge or x to dismiss".to_string()),
            route,
            task_id: record
                .task_id
                .map(|task_id: wave_domain::TaskId| task_id.as_str().to_string()),
            source_run_id: None,
            evidence_count: record.citations.len(),
            created_at_ms: Some(record.created_at_ms),
        });
    }
    for proposal in head_proposals
        .iter()
        .filter(|proposal| matches!(proposal.state, wave_domain::HeadProposalState::Pending))
    {
        items.push(OperatorActionableItem {
            kind: OperatorActionableKind::Proposal,
            wave_id: proposal.wave_id,
            record_id: proposal.proposal_id.as_str().to_string(),
            state: debug_label(proposal.state),
            summary: proposal.summary.clone(),
            detail: proposal.detail.clone(),
            waiting_on: Some("operator proposal review".to_string()),
            next_action: Some("press u to apply or x to dismiss".to_string()),
            route: Some(format!("head:{}", debug_label(proposal.action_kind))),
            task_id: None,
            source_run_id: None,
            evidence_count: 0,
            created_at_ms: Some(proposal.created_at_ms),
        });
    }
    Ok(items)
}

pub fn latest_relevant_run_details(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> Vec<ActiveRunDetail> {
    let mut details = latest_runs
        .values()
        .filter_map(|run| build_run_detail(root, config, waves, run))
        .collect::<Vec<_>>();
    details.sort_by_key(|detail| detail.wave_id);
    details
}

pub fn latest_relevant_run_detail(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    latest_runs: &HashMap<u32, WaveRunRecord>,
    wave_id: u32,
) -> Option<ActiveRunDetail> {
    latest_runs
        .get(&wave_id)
        .and_then(|run| build_run_detail(root, config, waves, run))
}

pub fn build_run_detail(
    root: &Path,
    config: &ProjectConfig,
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
    let mas = build_mas_run_detail(root, config, wave, run).ok().flatten();

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
        mas,
    })
}

fn build_run_execution_state(run: &WaveRunRecord) -> wave_control_plane::WaveExecutionState {
    wave_reducer::wave_execution_state_from_records(
        run.worktree.clone(),
        run.promotion.clone(),
        run.scheduling.clone(),
    )
}

fn build_mas_run_detail(
    root: &Path,
    config: &ProjectConfig,
    wave: &WaveDocument,
    run: &WaveRunRecord,
) -> Result<Option<MasRunDetail>> {
    if !wave.is_multi_agent() {
        return Ok(None);
    }
    let sandboxes = list_agent_sandboxes(root, config, Some(run.wave_id))?;
    let merge_results = list_merge_results(root, config, Some(run.wave_id))?;
    let invalidations = list_invalidations(root, config, Some(run.wave_id))?;
    let recovery_plans = list_recovery_plans(root, config, Some(run.wave_id))?;
    let recovery_actions = list_recovery_actions(root, config, Some(run.wave_id))?;
    let latest_sandboxes = sandboxes.into_iter().fold(
        BTreeMap::<String, wave_domain::AgentSandboxRecord>::new(),
        |mut acc, record| {
            let key = record.agent_id.clone();
            let replace = acc
                .get(&key)
                .map(|current| {
                    (record.allocated_at_ms, record.sandbox_id.as_str())
                        >= (current.allocated_at_ms, current.sandbox_id.as_str())
                })
                .unwrap_or(true);
            if replace {
                acc.insert(key, record);
            }
            acc
        },
    );
    let latest_merges = merge_results.into_iter().fold(
        BTreeMap::<String, wave_domain::MergeResultRecord>::new(),
        |mut acc, record| {
            let key = record.task_id.as_str().to_string();
            let replace = acc
                .get(&key)
                .map(|current| {
                    (record.applied_at_ms, record.merge_result_id.as_str())
                        >= (current.applied_at_ms, current.merge_result_id.as_str())
                })
                .unwrap_or(true);
            if replace {
                acc.insert(key, record);
            }
            acc
        },
    );
    let mut invalidated_agent_ids = BTreeSet::new();
    let invalidation_snapshots = invalidations
        .iter()
        .map(|record| {
            let invalidated = record
                .invalidated_task_ids
                .iter()
                .filter_map(task_id_agent_id)
                .collect::<Vec<_>>();
            invalidated_agent_ids.extend(invalidated.iter().cloned());
            MasInvalidationSnapshot {
                source_agent_id: task_id_agent_id(&record.source_task_id)
                    .unwrap_or_else(|| "unknown".to_string()),
                invalidated_agent_ids: invalidated,
                reasons: record.reasons.clone(),
            }
        })
        .collect::<Vec<_>>();
    let recovery = recovery_plans.into_iter().last().map(|plan| {
        let mut recent_actions = recovery_actions
            .iter()
            .filter(|action| action.recovery_plan_id == plan.recovery_plan_id)
            .map(|action| MasRecoveryActionSnapshot {
                recovery_action_id: action.recovery_action_id.as_str().to_string(),
                agent_id: action.agent_id.clone(),
                action_kind: debug_label(action.action_kind),
                requested_by: action.requested_by.clone(),
                created_at_ms: action.created_at_ms,
                detail: action.detail.clone(),
            })
            .collect::<Vec<_>>();
        recent_actions.sort_by_key(|action| action.created_at_ms);
        MasRecoverySnapshot {
            recovery_plan_id: plan.recovery_plan_id.as_str().to_string(),
            run_id: plan.run_id.clone(),
            status: debug_label(plan.status),
            causes: plan
                .causes
                .iter()
                .map(|cause| debug_label(*cause))
                .collect(),
            affected_agent_ids: plan.affected_agent_ids.clone(),
            preserved_accepted_agent_ids: plan.preserved_accepted_agent_ids.clone(),
            required_actions: plan
                .agent_plans
                .iter()
                .flat_map(|agent| agent.required_actions.iter().copied())
                .map(debug_label)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            detail: plan.detail.clone(),
            recent_actions,
        }
    });
    let sandboxes = latest_sandboxes
        .values()
        .map(|sandbox| MasSandboxSnapshot {
            sandbox_id: sandbox.sandbox_id.as_str().to_string(),
            agent_id: sandbox.agent_id.clone(),
            path: sandbox.path.clone(),
            base_integration_ref: sandbox.base_integration_ref.clone(),
            released_at_ms: sandbox.released_at_ms,
            detail: sandbox.detail.clone(),
        })
        .collect::<Vec<_>>();
    let merges = latest_merges
        .values()
        .map(|merge| MasMergeSnapshot {
            agent_id: task_id_agent_id(&merge.task_id).unwrap_or_else(|| "unknown".to_string()),
            disposition: format!("{:?}", merge.disposition).to_ascii_lowercase(),
            conflict_paths: merge.conflict_paths.clone(),
            detail: merge.detail.clone(),
        })
        .collect::<Vec<_>>();
    let running_agent_ids = run
        .agents
        .iter()
        .filter(|agent| agent.status == WaveRunStatus::Running)
        .map(|agent| agent.id.clone())
        .collect::<Vec<_>>();
    let merged_agent_ids = latest_merges
        .values()
        .filter(|merge| matches!(merge.disposition, wave_domain::MergeDisposition::Accepted))
        .filter_map(|merge| task_id_agent_id(&merge.task_id))
        .collect::<Vec<_>>();
    let conflicted_agent_ids = latest_merges
        .values()
        .filter(|merge| {
            matches!(
                merge.disposition,
                wave_domain::MergeDisposition::Conflicted | wave_domain::MergeDisposition::Rejected
            )
        })
        .filter_map(|merge| task_id_agent_id(&merge.task_id))
        .collect::<Vec<_>>();
    Ok(Some(MasRunDetail {
        execution_model: "multi-agent".to_string(),
        running_agent_ids,
        merged_agent_ids,
        conflicted_agent_ids,
        invalidated_agent_ids: invalidated_agent_ids.into_iter().collect(),
        sandboxes,
        merges,
        invalidations: invalidation_snapshots,
        recovery,
    }))
}

fn task_id_agent_id(task_id: &wave_domain::TaskId) -> Option<String> {
    task_id
        .as_str()
        .split("agent-")
        .nth(1)
        .map(|agent| agent.to_ascii_uppercase())
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
    let reused_from_prior_run = agent_reused_from_prior_run(root, run, agent);
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
        reused_from_prior_run,
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

fn agent_reused_from_prior_run(
    root: &Path,
    run: &WaveRunRecord,
    agent: &wave_trace::AgentRunRecord,
) -> bool {
    let bundle_dir = resolve_run_path(root, &run.bundle_dir);
    let prompt_path = resolve_run_path(root, &agent.prompt_path);
    !prompt_path.starts_with(bundle_dir)
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
        (
            "last-message",
            resolve_run_path(root, &agent.last_message_path),
        ),
        ("events", resolve_run_path(root, &agent.events_path)),
        ("stderr", resolve_run_path(root, &agent.stderr_path)),
    ];
    let latest = candidates
        .iter()
        .filter_map(|(label, path)| {
            modified_at_ms(path).map(|modified_at_ms| ((*label).to_string(), path, modified_at_ms))
        })
        .max_by_key(|(_, _, modified_at_ms)| *modified_at_ms);

    let (activity_excerpt, activity_source, last_activity_at_ms) =
        if let Some((label, path, modified_at_ms)) = latest {
            let excerpt =
                read_tail(path, 16).unwrap_or_else(|| "No live agent output yet.".to_string());
            (excerpt, Some(label), Some(modified_at_ms))
        } else {
            ("No live agent output yet.".to_string(), None, None)
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
    let anchor = last_activity_at_ms
        .or(run.started_at_ms)
        .unwrap_or(run.created_at_ms);
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
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn read_tail(path: &Path, max_lines: usize) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    let lines = raw.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);
    Some(lines[start..].join("\n"))
}

fn debug_label(value: impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    let mut label = String::new();
    for (index, ch) in debug.chars().enumerate() {
        if ch.is_uppercase() && index > 0 {
            label.push('_');
        }
        for lower in ch.to_lowercase() {
            label.push(lower);
        }
    }
    label
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
    use wave_runtime::set_orchestrator_mode;
    use wave_runtime::steer_agent;
    use wave_runtime::steer_wave;
    use wave_spec::CompletionLevel;
    use wave_spec::ComponentPromotion;
    use wave_spec::Context7Defaults;
    use wave_spec::DeployEnvironment;
    use wave_spec::DocImpact;
    use wave_spec::DurabilityLevel;
    use wave_spec::ExitContract;
    use wave_spec::ProofLevel;
    use wave_spec::WaveMetadata;

    fn default_delivery() -> wave_control_plane::DeliveryReadModel {
        wave_control_plane::DeliveryReadModel::default()
    }

    fn default_wave_metadata() -> WaveMetadata {
        WaveMetadata {
            id: 0,
            slug: String::new(),
            title: String::new(),
            mode: wave_config::ExecutionMode::DarkFactory,
            execution_model: wave_spec::WaveExecutionModel::Serial,
            concurrency_budget: wave_spec::WaveConcurrencyBudget::default(),
            owners: Vec::new(),
            depends_on: Vec::new(),
            validation: Vec::new(),
            rollback: Vec::new(),
            proof: Vec::new(),
            wave_class: wave_spec::WaveClass::Implementation,
            intent: None,
            delivery: None,
            design_gate: None,
        }
    }

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

    fn empty_recovery() -> wave_control_plane::WaveRecoveryState {
        wave_control_plane::WaveRecoveryState::default()
    }

    fn unavailable_runtime(
        runtime: wave_domain::RuntimeId,
        binary: &str,
        detail: &str,
    ) -> wave_runtime::RuntimeAvailability {
        wave_runtime::RuntimeAvailability {
            runtime,
            binary: binary.to_string(),
            available: false,
            detail: detail.to_string(),
            directive_capabilities: wave_runtime::RuntimeDirectiveCapabilities {
                live_injection: false,
                checkpoint_overlay: false,
                ack_support: false,
            },
        }
    }

    fn closure_test_wave() -> WaveStatusReadModel {
        WaveStatusReadModel {
            id: 17,
            slug: "portfolio-release-and-acceptance-packages".to_string(),
            title: "Wave 17".to_string(),
            depends_on: vec![16],
            blocked_by: Vec::new(),
            blocker_state: Vec::new(),
            design_completeness: wave_domain::DesignCompletenessState::StructurallyComplete,
            lint_errors: 0,
            ready: true,
            ownership: empty_ownership(),
            execution: empty_execution(),
            recovery: empty_recovery(),
            agent_count: 6,
            implementation_agent_count: 2,
            closure_agent_count: 4,
            closure_complete: true,
            required_closure_agents: vec![
                "A6".to_string(),
                "A8".to_string(),
                "A9".to_string(),
                "A0".to_string(),
            ],
            present_closure_agents: vec![
                "A6".to_string(),
                "A8".to_string(),
                "A9".to_string(),
                "A0".to_string(),
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
            last_run_status: Some(WaveRunStatus::Failed),
            soft_state: wave_domain::SoftState::Clear,
        }
    }

    fn closure_test_run(agents: Vec<AgentPanelItem>) -> ActiveRunDetail {
        ActiveRunDetail {
            wave_id: 17,
            wave_slug: "portfolio-release-and-acceptance-packages".to_string(),
            wave_title: "Wave 17".to_string(),
            run_id: "wave-17-test".to_string(),
            status: WaveRunStatus::Failed,
            created_at_ms: 1,
            started_at_ms: Some(2),
            elapsed_ms: Some(30_000),
            current_agent_id: None,
            current_agent_title: None,
            activity_excerpt: "closure test".to_string(),
            last_activity_at_ms: Some(3),
            activity_source: Some("events".to_string()),
            stalled: false,
            stall_reason: None,
            execution: empty_execution(),
            runtime_summary: RuntimeSummary {
                selected_runtimes: vec!["codex".to_string()],
                requested_runtimes: vec!["codex".to_string()],
                selection_sources: vec!["executor.id".to_string()],
                fallback_targets: Vec::new(),
                fallback_count: 0,
                agents_with_runtime: agents.len(),
            },
            proof: ProofSnapshot {
                declared_artifacts: Vec::new(),
                complete: false,
                proof_source: "mixed-envelope-and-compatibility".to_string(),
                completed_agents: 2,
                envelope_backed_agents: 2,
                compatibility_backed_agents: 4,
                total_agents: 6,
            },
            replay: wave_trace::ReplayReport {
                run_id: "wave-17-test".to_string(),
                wave_id: 17,
                ok: true,
                issues: Vec::new(),
            },
            agents,
            mas: None,
        }
    }

    fn closure_test_agent(
        id: &str,
        status: WaveRunStatus,
        proof_complete: bool,
        error: Option<&str>,
    ) -> AgentPanelItem {
        AgentPanelItem {
            id: id.to_string(),
            title: format!("Agent {id}"),
            status,
            current_task: format!("task {id}"),
            reused_from_prior_run: false,
            proof_complete,
            proof_source: "structured-envelope".to_string(),
            expected_markers: vec![format!("[marker-{id}]")],
            observed_markers: proof_complete
                .then(|| vec![format!("[marker-{id}]")])
                .unwrap_or_default(),
            missing_markers: (!proof_complete)
                .then(|| vec![format!("[marker-{id}]")])
                .unwrap_or_default(),
            deliverables: Vec::new(),
            error: error.map(str::to_string),
            runtime: None,
        }
    }

    #[test]
    fn build_orchestrator_panel_snapshot_surfaces_mode_and_directives() {
        let root = std::env::temp_dir().join(format!(
            "wave-app-server-orchestrator-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".wave")).expect("create wave dir");
        let config = ProjectConfig::default();
        set_orchestrator_mode(
            &root,
            &config,
            18,
            wave_domain::OrchestratorMode::Autonomous,
            "test",
        )
        .expect("set mode");
        steer_wave(
            &root,
            &config,
            18,
            "Coordinate the pilot wave",
            wave_domain::DirectiveOrigin::Operator,
            "test",
        )
        .expect("steer wave");
        steer_agent(
            &root,
            &config,
            18,
            "A1",
            "Focus on merge queue invariants",
            wave_domain::DirectiveOrigin::Operator,
            "test",
        )
        .expect("steer agent");

        let waves = vec![WaveDocument {
            path: PathBuf::from("waves/18.md"),
            metadata: WaveMetadata {
                id: 18,
                slug: "true-mas-parallel-runtime-and-head-control".to_string(),
                title: "Wave 18".to_string(),
                execution_model: wave_spec::WaveExecutionModel::MultiAgent,
                owners: vec!["architecture".to_string()],
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 18".to_string()),
            commit_message: Some("Feat: wave 18".to_string()),
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![wave_spec::WaveAgent {
                id: "A1".to_string(),
                title: "Parallel Runtime".to_string(),
                role_prompts: Vec::new(),
                executor: BTreeMap::new(),
                context7: None,
                skills: vec!["wave-core".to_string()],
                components: Vec::new(),
                capabilities: Vec::new(),
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Integrated,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Integration,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec!["crates/wave-runtime/src/lib.rs".to_string()],
                file_ownership: vec!["crates/wave-runtime/src/lib.rs".to_string()],
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: vec!["merge-queue-state".to_string()],
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::ParallelSafe,
                exclusive_resources: vec!["runtime-core".to_string()],
                parallel_with: Vec::new(),
                final_markers: vec!["[wave-proof]".to_string()],
                prompt: "Primary goal:\n- noop\n\nRequired context before coding:\n- Read crates/wave-runtime/src/lib.rs.\n\nSpecific expectations:\n- noop\n- emit the final [wave-proof] marker as a plain line by itself at the end of the output\n\nFile ownership (only touch these paths):\n- crates/wave-runtime/src/lib.rs".to_string(),
            }],
        }];
        let active_run = ActiveRunDetail {
            wave_id: 18,
            wave_slug: "true-mas-parallel-runtime-and-head-control".to_string(),
            wave_title: "Wave 18".to_string(),
            run_id: "wave-18-test".to_string(),
            status: WaveRunStatus::Running,
            created_at_ms: 1,
            started_at_ms: Some(2),
            elapsed_ms: Some(10),
            current_agent_id: Some("A1".to_string()),
            current_agent_title: Some("Parallel Runtime".to_string()),
            activity_excerpt: "running".to_string(),
            last_activity_at_ms: Some(3),
            activity_source: Some("events".to_string()),
            stalled: false,
            stall_reason: None,
            execution: empty_execution(),
            runtime_summary: RuntimeSummary {
                selected_runtimes: vec!["codex".to_string()],
                requested_runtimes: vec!["codex".to_string()],
                selection_sources: vec!["executor.id".to_string()],
                fallback_targets: Vec::new(),
                fallback_count: 0,
                agents_with_runtime: 1,
            },
            proof: ProofSnapshot {
                declared_artifacts: Vec::new(),
                complete: false,
                proof_source: "none".to_string(),
                completed_agents: 0,
                envelope_backed_agents: 0,
                compatibility_backed_agents: 0,
                total_agents: 1,
            },
            replay: wave_trace::ReplayReport {
                run_id: "wave-18-test".to_string(),
                wave_id: 18,
                ok: true,
                issues: Vec::new(),
            },
            agents: vec![closure_test_agent(
                "A1",
                WaveRunStatus::Running,
                false,
                None,
            )],
            mas: Some(MasRunDetail {
                execution_model: "multi-agent".to_string(),
                running_agent_ids: vec!["A1".to_string()],
                merged_agent_ids: Vec::new(),
                conflicted_agent_ids: Vec::new(),
                invalidated_agent_ids: Vec::new(),
                sandboxes: vec![MasSandboxSnapshot {
                    sandbox_id: "sandbox-wave-18-a1".to_string(),
                    agent_id: "A1".to_string(),
                    path: ".wave/state/worktrees/wave-18-a1".to_string(),
                    base_integration_ref: Some("HEAD".to_string()),
                    released_at_ms: None,
                    detail: Some("isolated MAS agent sandbox".to_string()),
                }],
                merges: vec![MasMergeSnapshot {
                    agent_id: "A1".to_string(),
                    disposition: "pending".to_string(),
                    conflict_paths: Vec::new(),
                    detail: Some("queued for integration merge".to_string()),
                }],
                invalidations: Vec::new(),
                recovery: None,
            }),
        };

        let planning_wave = WaveStatusReadModel {
            id: 18,
            slug: "true-mas-parallel-runtime-and-head-control".to_string(),
            title: "Wave 18".to_string(),
            depends_on: Vec::new(),
            blocked_by: vec!["active-run:running".to_string()],
            blocker_state: Vec::new(),
            design_completeness: wave_domain::DesignCompletenessState::ImplementationReady,
            lint_errors: 0,
            ready: false,
            ownership: empty_ownership(),
            execution: empty_execution(),
            recovery: empty_recovery(),
            agent_count: 1,
            implementation_agent_count: 1,
            closure_agent_count: 0,
            closure_complete: true,
            required_closure_agents: Vec::new(),
            present_closure_agents: Vec::new(),
            missing_closure_agents: Vec::new(),
            readiness: WaveReadinessReadModel {
                state: QueueReadinessStateReadModel::Active,
                planning_ready: false,
                claimable: false,
                reasons: Vec::new(),
                primary_reason: None,
            },
            rerun_requested: false,
            closure_override_applied: false,
            completed: false,
            last_run_status: Some(WaveRunStatus::Running),
            soft_state: wave_domain::SoftState::Clear,
        };

        let snapshot = build_orchestrator_panel_snapshot(
            &root,
            &config,
            &waves,
            &[planning_wave],
            &[active_run],
            &[],
            &[],
        )
        .expect("snapshot");
        assert_eq!(snapshot.mode, "autonomous");
        assert!(snapshot.active);
        assert_eq!(snapshot.selected_wave_id, Some(18));
        assert_eq!(snapshot.waves.len(), 1);
        assert_eq!(snapshot.waves[0].agents[0].status, "running");
        assert_eq!(
            snapshot.waves[0].agents[0].merge_state.as_deref(),
            Some("pending")
        );
        assert_eq!(
            snapshot.waves[0].agents[0].sandbox_id.as_deref(),
            Some("sandbox-wave-18-a1")
        );
        assert_eq!(snapshot.directives.len(), 3);
        assert!(
            snapshot
                .directives
                .iter()
                .any(|directive| directive.agent_id.is_none())
        );
        assert_eq!(
            snapshot
                .directives
                .iter()
                .filter(|directive| directive.delivery_state.as_deref() == Some("pending"))
                .count(),
            2
        );
        assert_eq!(
            snapshot
                .directives
                .iter()
                .filter(|directive| directive.delivery_state.as_deref() == Some("acked"))
                .count(),
            1
        );
    }

    #[test]
    fn acceptance_signoff_uses_run_backed_closure_progress() {
        let wave = closure_test_wave();
        let run = closure_test_run(vec![
            closure_test_agent("A1", WaveRunStatus::Succeeded, true, None),
            closure_test_agent("A2", WaveRunStatus::Succeeded, true, None),
            closure_test_agent(
                "A6",
                WaveRunStatus::Failed,
                false,
                Some("design review blocked"),
            ),
            closure_test_agent("A8", WaveRunStatus::Planned, false, None),
            closure_test_agent("A9", WaveRunStatus::Planned, false, None),
            closure_test_agent("A0", WaveRunStatus::Planned, false, None),
        ]);
        let design = AcceptanceDesignIntentSnapshot {
            completeness: wave_domain::DesignCompletenessState::StructurallyComplete,
            blocker_count: 0,
            contradiction_count: 0,
            unresolved_question_count: 0,
            unresolved_assumption_count: 0,
            pending_human_input_count: 0,
            ambiguous_dependency_count: 0,
        };
        let implementation = AcceptanceImplementationSnapshot {
            proof_complete: false,
            proof_source: Some("mixed-envelope-and-compatibility".to_string()),
            replay_ok: Some(true),
            completed_agents: 2,
            total_agents: 6,
        };

        let signoff = build_acceptance_signoff(
            &wave,
            Some(&run),
            &[],
            None,
            &design,
            &implementation,
            false,
        );

        assert_eq!(signoff.state, AcceptanceSignoffState::PendingEvidence);
        assert!(signoff.completed_closure_agents.is_empty());
        assert_eq!(
            signoff.pending_closure_agents,
            vec![
                "A6".to_string(),
                "A8".to_string(),
                "A9".to_string(),
                "A0".to_string()
            ]
        );
        assert_eq!(signoff.closure_agents[0].agent_id, "A6");
        assert_eq!(
            signoff.closure_agents[0].status,
            Some(WaveRunStatus::Failed)
        );
        assert!(!signoff.closure_agents[0].satisfied);
        assert_eq!(
            signoff.closure_agents[0].error.as_deref(),
            Some("design review blocked")
        );
    }

    #[test]
    fn acceptance_signoff_only_counts_successful_proof_complete_closure_agents() {
        let wave = closure_test_wave();
        let run = closure_test_run(vec![
            closure_test_agent("A6", WaveRunStatus::Succeeded, false, None),
            closure_test_agent("A8", WaveRunStatus::Succeeded, true, None),
            closure_test_agent("A9", WaveRunStatus::Succeeded, true, None),
            closure_test_agent("A0", WaveRunStatus::Succeeded, true, None),
        ]);
        let design = AcceptanceDesignIntentSnapshot {
            completeness: wave_domain::DesignCompletenessState::ImplementationReady,
            blocker_count: 0,
            contradiction_count: 0,
            unresolved_question_count: 0,
            unresolved_assumption_count: 0,
            pending_human_input_count: 0,
            ambiguous_dependency_count: 0,
        };
        let implementation = AcceptanceImplementationSnapshot {
            proof_complete: true,
            proof_source: Some("structured-envelope".to_string()),
            replay_ok: Some(true),
            completed_agents: 4,
            total_agents: 4,
        };

        let signoff =
            build_acceptance_signoff(&wave, Some(&run), &[], None, &design, &implementation, true);

        assert_eq!(signoff.state, AcceptanceSignoffState::AwaitingClosure);
        assert_eq!(
            signoff.completed_closure_agents,
            vec!["A8".to_string(), "A9".to_string(), "A0".to_string()]
        );
        assert_eq!(signoff.pending_closure_agents, vec!["A6".to_string()]);
        assert!(!signoff.closure_agents[0].satisfied);
        assert!(!signoff.complete);
    }

    #[test]
    fn design_narrative_index_surfaces_contradictions_and_invalidation_routes() {
        let fact = wave_domain::FactRecord {
            fact_id: wave_domain::FactId::new("fact-api"),
            wave_id: 4,
            task_id: None,
            attempt_id: None,
            state: wave_domain::FactState::Active,
            summary: "API fact".to_string(),
            detail: None,
            source_artifact: None,
            introduced_by_event_id: None,
            citations: Vec::new(),
            contradiction_ids: vec![wave_domain::ContradictionId::new("contradiction-1")],
            superseded_by_fact_id: None,
        };
        let lineage = wave_domain::LineageRecord {
            record_id: wave_domain::LineageRecordId::new("lineage-1"),
            wave_id: 5,
            task_id: Some(wave_domain::task_id_for_agent(5, "A1")),
            subject: wave_domain::LineageRecordSubject::Decision {
                decision_id: wave_domain::DecisionId::new("decision-api-shape"),
            },
            authority: wave_domain::DesignAuthority::Agent,
            state: wave_domain::LineageState::Decided,
            summary: "Decision".to_string(),
            detail: None,
            citations: Vec::new(),
            upstream_refs: vec![wave_domain::LineageRef::Fact(fact.fact_id.clone())],
            supporting_fact_ids: vec![fact.fact_id.clone()],
            downstream_task_ids: vec![wave_domain::task_id_for_agent(5, "A1")],
            downstream_wave_ids: vec![5],
            required_human_input_request_ids: Vec::new(),
            introduced_by_event_id: None,
        };
        let contradiction = wave_domain::ContradictionRecord {
            contradiction_id: wave_domain::ContradictionId::new("contradiction-1"),
            wave_id: 4,
            task_ids: vec![wave_domain::task_id_for_agent(4, "A1")],
            fact_ids: vec![fact.fact_id.clone()],
            state: wave_domain::ContradictionState::Detected,
            summary: "Contradiction".to_string(),
            detail: Some("API no longer matches".to_string()),
            introduced_by_event_id: None,
            invalidated_refs: vec![wave_domain::LineageRef::Fact(fact.fact_id.clone())],
        };

        let events = vec![
            wave_events::ControlEvent::new(
                "evt-fact",
                wave_events::ControlEventKind::FactObserved,
                4,
            )
            .with_created_at_ms(1)
            .with_payload(wave_domain::ControlEventPayload::FactObserved { fact }),
            wave_events::ControlEvent::new(
                "evt-lineage",
                wave_events::ControlEventKind::LineageUpdated,
                5,
            )
            .with_created_at_ms(2)
            .with_payload(wave_domain::ControlEventPayload::LineageUpdated { lineage }),
            wave_events::ControlEvent::new(
                "evt-contradiction",
                wave_events::ControlEventKind::ContradictionUpdated,
                4,
            )
            .with_created_at_ms(3)
            .with_payload(wave_domain::ControlEventPayload::ContradictionUpdated { contradiction }),
        ];

        let narratives = build_design_narrative_index(&events);

        assert_eq!(
            narratives.contradictions_by_wave.get(&5).unwrap()[0].contradiction_id,
            "contradiction-1"
        );
        assert!(narratives
            .invalidation_routes_by_wave
            .get(&5)
            .unwrap()
            .contains(
                &"contradiction contradiction-1 invalidates fact fact-api -> decision decision-api-shape"
                    .to_string()
            ));
        assert!(
            narratives
                .invalidation_routes_by_wave
                .get(&5)
                .unwrap()
                .contains(
                    &"decision decision-api-shape invalidates task wave-05:agent-a1".to_string()
                )
        );
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
                    directive_capabilities: wave_runtime::RuntimeDirectiveCapabilities {
                        live_injection: true,
                        checkpoint_overlay: true,
                        ack_support: false,
                    },
                },
                unavailable_runtime(
                    wave_domain::RuntimeId::Claude,
                    "/tmp/fake-claude",
                    "not authenticated",
                ),
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
    fn pending_human_inputs_use_typed_workflow_kind_instead_of_route_text() {
        let explicit_dependency = PendingHumanInputDetail {
            request_id: "human-typed-dependency".to_string(),
            task_id: Some("wave-16:agent-a2".to_string()),
            state: HumanInputState::Pending,
            workflow_kind: HumanInputWorkflowKind::DependencyHandshake,
            route: "operator:review".to_string(),
            prompt: "Need dependency confirmation".to_string(),
            requested_by: "A2".to_string(),
            answer: None,
        };
        let explicit_operator = PendingHumanInputDetail {
            request_id: "human-typed-approval".to_string(),
            task_id: Some("wave-16:agent-a6".to_string()),
            state: HumanInputState::Pending,
            workflow_kind: HumanInputWorkflowKind::OperatorApproval,
            route: "dependency:wave-15".to_string(),
            prompt: "Need operator approval".to_string(),
            requested_by: "A6".to_string(),
            answer: None,
        };

        assert_eq!(
            dependency_handshake_routes(&[explicit_dependency.clone(), explicit_operator.clone()]),
            vec!["operator:review".to_string()]
        );
        assert_eq!(
            approval_waiting_on(&explicit_dependency),
            "operator dependency approval"
        );
        assert_eq!(approval_waiting_on(&explicit_operator), "operator approval");
    }

    #[test]
    fn latest_human_inputs_preserve_legacy_route_fallback_for_unspecified_workflow_kind() {
        let events = vec![
            wave_events::ControlEvent::new(
                "evt-human",
                wave_events::ControlEventKind::HumanInputUpdated,
                16,
            )
            .with_created_at_ms(1)
            .with_payload(wave_domain::ControlEventPayload::HumanInputUpdated {
                request: HumanInputRequest {
                    request_id: wave_domain::HumanInputRequestId::new("human-legacy"),
                    wave_id: 16,
                    task_id: Some(wave_domain::task_id_for_agent(16, "A2")),
                    state: HumanInputState::Pending,
                    workflow_kind: HumanInputWorkflowKind::Unspecified,
                    prompt: "Need dependency confirmation".to_string(),
                    route: "dependency:wave-15".to_string(),
                    requested_by: "A2".to_string(),
                    answer: None,
                },
            }),
        ];

        let detail = latest_human_inputs_by_id(&events)
            .remove("human-legacy")
            .expect("legacy request detail");

        assert_eq!(
            detail.workflow_kind,
            HumanInputWorkflowKind::DependencyHandshake
        );
        assert_eq!(approval_waiting_on(&detail), "operator dependency approval");
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
                design_incomplete_waves: 0,
                total_agents: 12,
                implementation_agents: 6,
                closure_agents: 6,
                waves_with_complete_closure: 2,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
            delivery: default_delivery().summary.clone(),
            portfolio: Default::default(),
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
                    design_completeness: wave_domain::DesignCompletenessState::Verified,
                    lint_errors: 0,
                    ready: false,
                    ownership: empty_ownership(),
                    execution: empty_execution(),
                    recovery: empty_recovery(),
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
                    soft_state: wave_domain::SoftState::Clear,
                },
                WaveStatusReadModel {
                    id: 2,
                    slug: "two".to_string(),
                    title: "Two".to_string(),
                    depends_on: vec![0],
                    blocked_by: Vec::new(),
                    blocker_state: Vec::new(),
                    design_completeness: wave_domain::DesignCompletenessState::ImplementationReady,
                    lint_errors: 0,
                    ready: true,
                    ownership: empty_ownership(),
                    execution: empty_execution(),
                    recovery: empty_recovery(),
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
                    soft_state: wave_domain::SoftState::Clear,
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
                design_incomplete_waves: 0,
                total_agents: 3,
                implementation_agents: 1,
                closure_agents: 2,
                waves_with_complete_closure: 1,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
            delivery: default_delivery().summary.clone(),
            portfolio: Default::default(),
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
                design_completeness: wave_domain::DesignCompletenessState::ImplementationReady,
                lint_errors: 0,
                ready: true,
                ownership: empty_ownership(),
                execution: empty_execution(),
                recovery: empty_recovery(),
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
                soft_state: wave_domain::SoftState::Clear,
            }],
            has_errors: false,
        };
        let projection = wave_control_plane::build_planning_status_projection(&status);
        let planning = PlanningProjectionBundle {
            status: status.clone(),
            projection,
        };
        let delivery = default_delivery();
        let operator = wave_control_plane::build_operator_snapshot_inputs(
            &planning,
            &delivery,
            &HashMap::new(),
            false,
        );
        let spine = ProjectionSpine {
            planning,
            operator,
            delivery,
        };
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
        assert!(snapshot.panels.control.apply_closure_override_supported);
        assert!(snapshot.panels.control.clear_closure_override_supported);
        assert!(snapshot.panels.control.approve_operator_action_supported);
        assert!(snapshot.panels.control.reject_operator_action_supported);
        assert_eq!(snapshot.panels.control.implemented_actions.len(), 10);
        assert_eq!(snapshot.panels.control.unavailable_actions.len(), 2);
        assert_eq!(snapshot.panels.control.unavailable_actions[0].key, "launch");
        assert_eq!(snapshot.panels.control.actions.len(), 12);
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
                design_incomplete_waves: 0,
                total_agents: 3,
                implementation_agents: 1,
                closure_agents: 2,
                waves_with_complete_closure: 1,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
            delivery: default_delivery().summary.clone(),
            portfolio: Default::default(),
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
                design_completeness: wave_domain::DesignCompletenessState::Verified,
                lint_errors: 0,
                ready: false,
                ownership: empty_ownership(),
                execution: empty_execution(),
                recovery: empty_recovery(),
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
                soft_state: wave_domain::SoftState::Clear,
            }],
            has_errors: false,
        };
        let projection = wave_control_plane::build_planning_status_projection(&status);
        let planning = PlanningProjectionBundle {
            status: status.clone(),
            projection,
        };
        let delivery = default_delivery();
        let operator = wave_control_plane::build_operator_snapshot_inputs(
            &planning,
            &delivery,
            &HashMap::new(),
            true,
        );
        let spine = ProjectionSpine {
            planning,
            operator,
            delivery,
        };
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
                design_incomplete_waves: 0,
                total_agents: 9,
                implementation_agents: 3,
                closure_agents: 6,
                waves_with_complete_closure: 3,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
            delivery: default_delivery().summary.clone(),
            portfolio: Default::default(),
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
                    design_completeness: wave_domain::DesignCompletenessState::ImplementationReady,
                    lint_errors: 0,
                    ready: false,
                    ownership: empty_ownership(),
                    execution: empty_execution(),
                    recovery: empty_recovery(),
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
                    soft_state: wave_domain::SoftState::Clear,
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
                    design_completeness: wave_domain::DesignCompletenessState::ImplementationReady,
                    lint_errors: 0,
                    ready: false,
                    ownership: empty_ownership(),
                    execution: empty_execution(),
                    recovery: empty_recovery(),
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
                    soft_state: wave_domain::SoftState::Clear,
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
                    design_completeness: wave_domain::DesignCompletenessState::Verified,
                    lint_errors: 0,
                    ready: false,
                    ownership: empty_ownership(),
                    execution: empty_execution(),
                    recovery: empty_recovery(),
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
                    soft_state: wave_domain::SoftState::Clear,
                },
            ],
            has_errors: false,
        };
        let projection = wave_control_plane::build_planning_status_projection(&status);
        let planning = PlanningProjectionBundle { status, projection };
        let delivery = default_delivery();
        let operator = wave_control_plane::build_operator_snapshot_inputs(
            &planning,
            &delivery,
            &HashMap::new(),
            true,
        );
        let spine = ProjectionSpine {
            planning,
            operator,
            delivery,
        };
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
        let config = ProjectConfig::default();
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
                ..default_wave_metadata()
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
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: Vec::new(),
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                exclusive_resources: Vec::new(),
                parallel_with: Vec::new(),
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

        let detail = build_run_detail(&root, &config, &[wave], &run).expect("run detail");

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
        let config = ProjectConfig::default();
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
                ..default_wave_metadata()
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
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: Vec::new(),
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                exclusive_resources: Vec::new(),
                parallel_with: Vec::new(),
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

        let detail = build_run_detail(&root, &config, &[wave], &run).expect("run detail");

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
        let config = ProjectConfig::default();
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
                ..default_wave_metadata()
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
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: Vec::new(),
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                exclusive_resources: Vec::new(),
                parallel_with: Vec::new(),
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

        let detail =
            latest_relevant_run_detail(&root, &config, &[wave], &HashMap::from([(12, run)]), 12)
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
        let config = ProjectConfig::default();
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
                ..default_wave_metadata()
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
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: Vec::new(),
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                exclusive_resources: Vec::new(),
                parallel_with: Vec::new(),
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

        let detail = build_run_detail(&root, &config, &[wave], &run).expect("run detail");
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
        let config = ProjectConfig::default();
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
                ..default_wave_metadata()
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
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: Vec::new(),
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                exclusive_resources: Vec::new(),
                parallel_with: Vec::new(),
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

        let detail = build_run_detail(&root, &config, &[wave], &run).expect("run detail");

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
        let config = ProjectConfig::default();
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
                ..default_wave_metadata()
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
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: Vec::new(),
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                exclusive_resources: Vec::new(),
                parallel_with: Vec::new(),
                prompt: "Primary goal:\n- noop".to_string(),
            }],
        };
        let started_at_ms = wave_trace::now_epoch_ms().expect("timestamp") - (16_u128 * 60 * 1_000);
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

        let detail = build_run_detail(&root, &config, &[wave], &run).expect("run detail");

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
        let config = ProjectConfig::default();
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
                ..default_wave_metadata()
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
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: Vec::new(),
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                exclusive_resources: Vec::new(),
                parallel_with: Vec::new(),
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
            &config,
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
        let config = ProjectConfig::default();
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
                ..default_wave_metadata()
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
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: Vec::new(),
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                exclusive_resources: Vec::new(),
                parallel_with: Vec::new(),
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
            &config,
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
