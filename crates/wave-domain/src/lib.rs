use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;
use wave_spec::WaveAgent;
use wave_spec::WaveDocument;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

string_id!(TaskId);
string_id!(AttemptId);
string_id!(GateId);
string_id!(FactId);
string_id!(ContradictionId);
string_id!(QuestionId);
string_id!(AssumptionId);
string_id!(DecisionId);
string_id!(LineageRecordId);
string_id!(ProofBundleId);
string_id!(RerunRequestId);
string_id!(HumanInputRequestId);
string_id!(ResultEnvelopeId);
string_id!(WaveClaimId);
string_id!(TaskLeaseId);
string_id!(SchedulerBudgetId);
string_id!(WaveWorktreeId);
string_id!(WavePromotionId);
string_id!(InitiativeId);
string_id!(ReleaseId);
string_id!(AcceptancePackageId);
string_id!(DeliveryRiskId);
string_id!(DeliveryDebtId);
string_id!(MilestoneId);
string_id!(ReleaseTrainId);
string_id!(OutcomeContractId);
string_id!(AgentSandboxId);
string_id!(MergeIntentId);
string_id!(MergeResultId);
string_id!(InvalidationId);
string_id!(RecoveryPlanId);
string_id!(RecoveryActionId);
string_id!(ControlDirectiveId);
string_id!(OrchestratorSessionId);
string_id!(OperatorShellSessionId);
string_id!(OperatorShellTurnId);
string_id!(HeadProposalId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskRole {
    Implementation,
    Design,
    Integration,
    Documentation,
    ContQa,
    ContEval,
    Security,
    Infra,
    Deploy,
    Research,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosureRole {
    ContEval,
    DesignReview,
    SecurityReview,
    Integration,
    Documentation,
    ContQa,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Declared,
    Leased,
    InProgress,
    OwnedSliceProven,
    Blocked,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveState {
    Planned,
    Running,
    ClosurePending,
    WaveClosureReady,
    Completed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    Planned,
    Running,
    Succeeded,
    Failed,
    Aborted,
    Refused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanInputState {
    Pending,
    Assigned,
    Answered,
    Rerouted,
    Escalated,
    Resolved,
    TimedOut,
}

impl HumanInputState {
    pub fn is_resolved(self) -> bool {
        matches!(self, Self::Answered | Self::Resolved)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HumanInputWorkflowKind {
    #[default]
    Unspecified,
    OperatorApproval,
    DependencyHandshake,
}

impl HumanInputWorkflowKind {
    pub fn effective(self, route: &str) -> Self {
        if !matches!(self, Self::Unspecified) {
            return self;
        }
        let normalized = route.to_ascii_lowercase();
        if normalized.contains("dependency") || normalized.contains("handshake") {
            Self::DependencyHandshake
        } else {
            Self::OperatorApproval
        }
    }

    pub fn is_dependency_handshake(self, route: &str) -> bool {
        matches!(self.effective(route), Self::DependencyHandshake)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionState {
    Detected,
    Acknowledged,
    RepairInProgress,
    Resolved,
    Waived,
}

impl ContradictionState {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Detected | Self::Acknowledged | Self::RepairInProgress
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactState {
    Active,
    Superseded,
    Retracted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignAuthority {
    Agent,
    Dependency,
    Human,
    Operator,
    Review,
    Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineageState {
    Open,
    PendingHuman,
    Resolved,
    Accepted,
    Decided,
    Superseded,
    Invalidated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignCompletenessState {
    Underspecified,
    Fragmented,
    StructurallyComplete,
    ImplementationReady,
    Verified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofStatus {
    Proposed,
    Active,
    Superseded,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RerunState {
    Requested,
    Approved,
    Cancelled,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RerunScope {
    Full,
    FromFirstIncomplete,
    ClosureOnly,
    PromotionOnly,
}

impl Default for RerunScope {
    fn default() -> Self {
        Self::Full
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaveClosureOverrideStatus {
    Applied,
    Cleared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveClaimState {
    Held,
    Released,
}

impl WaveClaimState {
    pub fn is_held(self) -> bool {
        matches!(self, Self::Held)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskLeaseState {
    Granted,
    Released,
    Expired,
    Revoked,
}

impl TaskLeaseState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Granted)
    }

    pub fn is_terminal(self) -> bool {
        !self.is_active()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SchedulerOwner {
    pub scheduler_id: String,
    pub scheduler_path: String,
    pub runtime: Option<String>,
    pub executor: Option<String>,
    pub session_id: Option<String>,
    pub process_id: Option<u32>,
    pub process_started_at_ms: Option<u128>,
}

impl SchedulerOwner {
    pub fn display_label(&self) -> &str {
        if self.scheduler_path.is_empty() {
            self.scheduler_id.as_str()
        } else {
            self.scheduler_path.as_str()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveClaimRecord {
    pub claim_id: WaveClaimId,
    pub wave_id: u32,
    pub state: WaveClaimState,
    pub owner: SchedulerOwner,
    pub claimed_at_ms: u128,
    pub released_at_ms: Option<u128>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskLeaseRecord {
    pub lease_id: TaskLeaseId,
    pub wave_id: u32,
    pub task_id: TaskId,
    pub claim_id: Option<WaveClaimId>,
    pub state: TaskLeaseState,
    pub owner: SchedulerOwner,
    pub granted_at_ms: u128,
    pub heartbeat_at_ms: Option<u128>,
    pub expires_at_ms: Option<u128>,
    pub finished_at_ms: Option<u128>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SchedulerBudget {
    pub max_active_wave_claims: Option<u32>,
    pub max_active_task_leases: Option<u32>,
    pub reserved_closure_task_leases: Option<u32>,
    #[serde(default)]
    pub preemption_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerBudgetRecord {
    pub budget_id: SchedulerBudgetId,
    pub budget: SchedulerBudget,
    pub owner: SchedulerOwner,
    pub updated_at_ms: u128,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveWorktreeState {
    Allocated,
    Released,
}

impl WaveWorktreeState {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Allocated)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveWorktreeScope {
    Wave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveWorktreeRecord {
    pub worktree_id: WaveWorktreeId,
    pub wave_id: u32,
    pub state: WaveWorktreeState,
    pub path: String,
    pub base_ref: String,
    pub snapshot_ref: String,
    pub branch_ref: Option<String>,
    pub shared_scope: WaveWorktreeScope,
    pub allocated_at_ms: u128,
    pub released_at_ms: Option<u128>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WavePromotionState {
    NotStarted,
    Pending,
    Ready,
    Conflicted,
    Failed,
}

impl WavePromotionState {
    pub fn blocks_closure(self) -> bool {
        !matches!(self, Self::Ready)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WavePromotionRecord {
    pub promotion_id: WavePromotionId,
    pub wave_id: u32,
    pub worktree_id: Option<WaveWorktreeId>,
    pub state: WavePromotionState,
    pub target_ref: String,
    pub snapshot_ref: String,
    pub candidate_ref: Option<String>,
    pub candidate_tree: Option<String>,
    #[serde(default)]
    pub conflict_paths: Vec<String>,
    pub checked_at_ms: u128,
    pub completed_at_ms: Option<u128>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveExecutionPhase {
    Implementation,
    Promotion,
    Closure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveSchedulerPriority {
    Implementation,
    Closure,
}

impl WaveSchedulerPriority {
    pub fn is_closure(self) -> bool {
        matches!(self, Self::Closure)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveSchedulingState {
    Waiting,
    Admitted,
    Running,
    Protected,
    Preempted,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveSchedulingRecord {
    pub wave_id: u32,
    pub phase: WaveExecutionPhase,
    pub priority: WaveSchedulerPriority,
    pub state: WaveSchedulingState,
    pub fairness_rank: u32,
    pub waiting_since_ms: Option<u128>,
    pub protected_closure_capacity: bool,
    pub preemptible: bool,
    pub last_decision: Option<String>,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SoftState {
    #[default]
    Clear,
    Advisory,
    Degraded,
    Stale,
}

impl SoftState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Advisory => "advisory",
            Self::Degraded => "degraded",
            Self::Stale => "stale",
        }
    }

    pub fn merge(self, other: Self) -> Self {
        self.max(other)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InitiativeState {
    Planned,
    InProgress,
    Blocked,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseState {
    Planned,
    Assembling,
    Candidate,
    Ready,
    Shipped,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AcceptancePackageState {
    Draft,
    CollectingEvidence,
    ReviewReady,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeliverySeverity {
    Advisory,
    Blocking,
}

impl DeliverySeverity {
    pub fn is_blocking(self) -> bool {
        matches!(self, Self::Blocking)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InitiativeRecord {
    pub id: InitiativeId,
    pub title: String,
    pub summary: String,
    pub state: Option<InitiativeState>,
    #[serde(default)]
    pub soft_state: SoftState,
    #[serde(default)]
    pub owners: Vec<String>,
    #[serde(default)]
    pub wave_ids: Vec<u32>,
    #[serde(default)]
    pub release_ids: Vec<ReleaseId>,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReleaseRecord {
    pub id: ReleaseId,
    pub title: String,
    pub summary: String,
    pub initiative_id: Option<InitiativeId>,
    pub state: Option<ReleaseState>,
    #[serde(default)]
    pub soft_state: SoftState,
    #[serde(default)]
    pub owners: Vec<String>,
    #[serde(default)]
    pub wave_ids: Vec<u32>,
    #[serde(default)]
    pub acceptance_package_ids: Vec<AcceptancePackageId>,
    pub milestone_id: Option<String>,
    pub release_train_id: Option<String>,
    #[serde(default)]
    pub blocking_risk_ids: Vec<DeliveryRiskId>,
    #[serde(default)]
    pub blocking_debt_ids: Vec<DeliveryDebtId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AcceptancePackageRecord {
    pub id: AcceptancePackageId,
    pub title: String,
    pub summary: String,
    pub release_id: Option<ReleaseId>,
    pub state: Option<AcceptancePackageState>,
    #[serde(default)]
    pub soft_state: SoftState,
    #[serde(default)]
    pub wave_ids: Vec<u32>,
    #[serde(default)]
    pub proof_artifacts: Vec<String>,
    #[serde(default)]
    pub design_evidence: Vec<String>,
    #[serde(default)]
    pub documentation_evidence: Vec<String>,
    #[serde(default)]
    pub signoffs: Vec<String>,
    #[serde(default)]
    pub blocking_risk_ids: Vec<DeliveryRiskId>,
    #[serde(default)]
    pub blocking_debt_ids: Vec<DeliveryDebtId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeliveryRiskRecord {
    pub id: DeliveryRiskId,
    pub title: String,
    pub summary: String,
    pub severity: Option<DeliverySeverity>,
    #[serde(default)]
    pub soft_state: SoftState,
    pub release_id: Option<ReleaseId>,
    pub acceptance_package_id: Option<AcceptancePackageId>,
    #[serde(default)]
    pub wave_ids: Vec<u32>,
    #[serde(default)]
    pub owners: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeliveryDebtRecord {
    pub id: DeliveryDebtId,
    pub title: String,
    pub summary: String,
    pub severity: Option<DeliverySeverity>,
    #[serde(default)]
    pub soft_state: SoftState,
    pub release_id: Option<ReleaseId>,
    pub acceptance_package_id: Option<AcceptancePackageId>,
    #[serde(default)]
    pub wave_ids: Vec<u32>,
    #[serde(default)]
    pub owners: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryCatalog {
    #[serde(default = "default_delivery_catalog_version")]
    pub version: u32,
    #[serde(default)]
    pub initiatives: Vec<InitiativeRecord>,
    #[serde(default)]
    pub releases: Vec<ReleaseRecord>,
    #[serde(default)]
    pub acceptance_packages: Vec<AcceptancePackageRecord>,
    #[serde(default)]
    pub risks: Vec<DeliveryRiskRecord>,
    #[serde(default)]
    pub debts: Vec<DeliveryDebtRecord>,
}

impl Default for DeliveryCatalog {
    fn default() -> Self {
        Self {
            version: default_delivery_catalog_version(),
            initiatives: Vec::new(),
            releases: Vec::new(),
            acceptance_packages: Vec::new(),
            risks: Vec::new(),
            debts: Vec::new(),
        }
    }
}

fn default_delivery_catalog_version() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PortfolioDeliveryModel {
    #[serde(default)]
    pub initiatives: Vec<PortfolioInitiative>,
    #[serde(default)]
    pub milestones: Vec<PortfolioMilestone>,
    #[serde(default)]
    pub release_trains: Vec<ReleaseTrain>,
    #[serde(default)]
    pub outcome_contracts: Vec<OutcomeContract>,
}

impl PortfolioDeliveryModel {
    pub fn is_empty(&self) -> bool {
        self.initiatives.is_empty()
            && self.milestones.is_empty()
            && self.release_trains.is_empty()
            && self.outcome_contracts.is_empty()
    }

    pub fn referenced_wave_ids(&self) -> Vec<u32> {
        let mut wave_ids = BTreeSet::new();
        for initiative in &self.initiatives {
            wave_ids.extend(initiative.wave_ids.iter().copied());
        }
        for milestone in &self.milestones {
            wave_ids.extend(milestone.wave_ids.iter().copied());
        }
        for release_train in &self.release_trains {
            wave_ids.extend(release_train.wave_ids.iter().copied());
        }
        for outcome_contract in &self.outcome_contracts {
            wave_ids.extend(outcome_contract.wave_ids.iter().copied());
        }
        wave_ids.into_iter().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioInitiative {
    pub initiative_id: InitiativeId,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub wave_ids: Vec<u32>,
    #[serde(default)]
    pub milestone_ids: Vec<MilestoneId>,
    pub release_train_id: Option<ReleaseTrainId>,
    #[serde(default)]
    pub outcome_contract_ids: Vec<OutcomeContractId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortfolioMilestone {
    pub milestone_id: MilestoneId,
    pub initiative_id: InitiativeId,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub wave_ids: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseTrain {
    pub release_train_id: ReleaseTrainId,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub wave_ids: Vec<u32>,
    #[serde(default)]
    pub initiative_ids: Vec<InitiativeId>,
    #[serde(default)]
    pub milestone_ids: Vec<MilestoneId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeContract {
    pub outcome_contract_id: OutcomeContractId,
    pub slug: String,
    pub title: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub wave_ids: Vec<u32>,
    #[serde(default)]
    pub initiative_ids: Vec<InitiativeId>,
    #[serde(default)]
    pub milestone_ids: Vec<MilestoneId>,
    pub release_train_id: Option<ReleaseTrainId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskDependencyKind {
    WaveClosure,
    DesignApproval,
    ImplementationSlice,
    ContEvalVerdict,
    DesignReviewVerdict,
    SecurityReviewVerdict,
    IntegrationClosure,
    DocumentationClosure,
    AgentGraph,
    ArtifactFlow,
    Barrier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WaveExecutionModel {
    #[default]
    Serial,
    MultiAgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BarrierClass {
    #[default]
    Independent,
    MergeAfter,
    IntegrationBarrier,
    ClosureBarrier,
    ReportOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ParallelSafetyClass {
    #[default]
    Derived,
    ParallelSafe,
    Serialized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WaveConcurrencyBudgetPlan {
    pub max_concurrent_implementation_agents: Option<u32>,
    pub max_concurrent_report_only_agents: Option<u32>,
    pub max_merge_operations: Option<u32>,
    pub max_conflict_resolution_agents: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDependency {
    pub artifact: String,
    pub source_agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSandboxRecord {
    pub sandbox_id: AgentSandboxId,
    pub wave_id: u32,
    pub task_id: TaskId,
    pub agent_id: String,
    pub path: String,
    pub base_integration_ref: Option<String>,
    pub allocated_at_ms: u128,
    pub released_at_ms: Option<u128>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeIntentRecord {
    pub merge_intent_id: MergeIntentId,
    pub wave_id: u32,
    pub task_id: TaskId,
    pub sandbox_id: AgentSandboxId,
    pub ownership_paths: Vec<String>,
    pub produced_artifacts: Vec<String>,
    pub invalidation_hints: Vec<String>,
    pub created_at_ms: u128,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeDisposition {
    Pending,
    Accepted,
    Rejected,
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeResultRecord {
    pub merge_result_id: MergeResultId,
    pub merge_intent_id: MergeIntentId,
    pub wave_id: u32,
    pub task_id: TaskId,
    pub disposition: MergeDisposition,
    pub conflict_paths: Vec<String>,
    pub applied_at_ms: u128,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidationRecord {
    pub invalidation_id: InvalidationId,
    pub wave_id: u32,
    pub source_task_id: TaskId,
    #[serde(default)]
    pub invalidated_task_ids: Vec<TaskId>,
    #[serde(default)]
    pub reasons: Vec<String>,
    pub created_at_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryCause {
    MergeConflict,
    MergeRejected,
    Invalidated,
    LeaseExpired,
    AgentFailed,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryActionKind {
    RerunAgent,
    RebaseSandbox,
    RequestReconciliation,
    ApproveMerge,
    RejectMerge,
    ResumeAgent,
    PauseAgent,
    ClearResolvedStep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryPlanStatus {
    #[default]
    Open,
    InProgress,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryAgentPlan {
    pub agent_id: String,
    pub cause: RecoveryCause,
    #[serde(default)]
    pub required_actions: Vec<RecoveryActionKind>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryPlanRecord {
    pub recovery_plan_id: RecoveryPlanId,
    pub wave_id: u32,
    pub run_id: String,
    #[serde(default)]
    pub causes: Vec<RecoveryCause>,
    #[serde(default)]
    pub affected_agent_ids: Vec<String>,
    #[serde(default)]
    pub preserved_accepted_agent_ids: Vec<String>,
    #[serde(default)]
    pub agent_plans: Vec<RecoveryAgentPlan>,
    pub status: RecoveryPlanStatus,
    pub detail: Option<String>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryActionRecord {
    pub recovery_action_id: RecoveryActionId,
    pub recovery_plan_id: RecoveryPlanId,
    pub wave_id: u32,
    pub run_id: String,
    pub agent_id: Option<String>,
    pub action_kind: RecoveryActionKind,
    pub requested_by: String,
    pub detail: Option<String>,
    pub created_at_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectiveOrigin {
    Operator,
    AutonomousHead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlDirectiveKind {
    PauseAgent,
    ResumeAgent,
    RerunAgent,
    RebaseSandbox,
    SteerPrompt,
    ApproveMerge,
    RejectMerge,
    RequestReconciliation,
    SetOrchestratorMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DirectiveDeliveryState {
    #[default]
    Pending,
    Delivered,
    Acked,
    Deferred,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectiveDeliveryMethod {
    LiveInjection,
    CheckpointOverlay,
    Deferred,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrchestratorMode {
    #[default]
    Operator,
    Autonomous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OperatorShellScope {
    #[default]
    Head,
    Wave,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OperatorShellTurnOrigin {
    #[default]
    Operator,
    Head,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OperatorShellTurnStatus {
    Pending,
    #[default]
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HeadProposalState {
    #[default]
    Pending,
    Applied,
    Dismissed,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadProposalResolutionKind {
    OperatorApplied,
    AutonomousApplied,
    Dismissed,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadProposalResolution {
    pub kind: HeadProposalResolutionKind,
    pub resolved_by: String,
    pub resolved_at_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadProposalActionKind {
    SteerWave,
    SteerAgent,
    PauseAgent,
    ResumeAgent,
    RerunAgent,
    RebaseSandbox,
    RequestReconciliation,
    ApproveMerge,
    RejectMerge,
    RequestWaveRerun,
    SetOrchestratorMode,
    LaunchWave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlDirectiveRecord {
    pub directive_id: ControlDirectiveId,
    pub wave_id: u32,
    pub task_id: Option<TaskId>,
    pub agent_id: Option<String>,
    pub sandbox_id: Option<AgentSandboxId>,
    pub kind: ControlDirectiveKind,
    pub origin: DirectiveOrigin,
    pub message: Option<String>,
    pub requested_by: String,
    pub requested_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectiveDeliveryRecord {
    pub directive_id: ControlDirectiveId,
    pub wave_id: u32,
    pub agent_id: Option<String>,
    pub state: DirectiveDeliveryState,
    #[serde(default)]
    pub method: Option<DirectiveDeliveryMethod>,
    #[serde(default)]
    pub runtime: Option<RuntimeId>,
    #[serde(default)]
    pub ack_supported: bool,
    pub detail: Option<String>,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestratorSessionRecord {
    pub session_id: OrchestratorSessionId,
    pub wave_id: u32,
    pub mode: OrchestratorMode,
    pub active: bool,
    pub runtime: Option<RuntimeId>,
    pub detail: Option<String>,
    pub started_at_ms: u128,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorShellSessionRecord {
    pub session_id: OperatorShellSessionId,
    pub scope: OperatorShellScope,
    pub wave_id: Option<u32>,
    pub agent_id: Option<String>,
    pub tab: String,
    pub follow_mode: String,
    pub mode: OrchestratorMode,
    pub active: bool,
    pub started_at_ms: u128,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorShellTurnRecord {
    pub turn_id: OperatorShellTurnId,
    pub session_id: OperatorShellSessionId,
    pub origin: OperatorShellTurnOrigin,
    pub scope: OperatorShellScope,
    #[serde(default)]
    pub cycle_id: Option<String>,
    pub wave_id: Option<u32>,
    pub agent_id: Option<String>,
    pub input: String,
    pub output: Option<String>,
    pub status: OperatorShellTurnStatus,
    pub created_at_ms: u128,
    pub failed_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadProposalRecord {
    pub proposal_id: HeadProposalId,
    pub session_id: OperatorShellSessionId,
    pub turn_id: OperatorShellTurnId,
    #[serde(default)]
    pub cycle_id: Option<String>,
    pub wave_id: u32,
    pub agent_id: Option<String>,
    pub action_kind: HeadProposalActionKind,
    #[serde(default)]
    pub action_payload: BTreeMap<String, String>,
    pub state: HeadProposalState,
    #[serde(default)]
    pub resolution: Option<HeadProposalResolution>,
    pub summary: String,
    pub detail: Option<String>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Patch,
    TestLog,
    DocDelta,
    Trace,
    Review,
    ResultEnvelope,
    Other,
}

impl ArtifactKind {
    pub fn supports_machine_readable_proof(self) -> bool {
        matches!(
            self,
            Self::Patch | Self::TestLog | Self::Review | Self::ResultEnvelope
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResultEnvelopeSource {
    #[default]
    Structured,
    LegacyMarkerAdapter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultDisposition {
    Completed,
    Partial,
    Failed,
    Aborted,
    Refused,
}

impl ResultDisposition {
    pub fn from_attempt_state(state: AttemptState, missing_final_markers: usize) -> Self {
        match state {
            AttemptState::Succeeded if missing_final_markers == 0 => Self::Completed,
            AttemptState::Succeeded | AttemptState::Planned | AttemptState::Running => {
                Self::Partial
            }
            AttemptState::Failed => Self::Failed,
            AttemptState::Aborted => Self::Aborted,
            AttemptState::Refused => Self::Refused,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResultPayloadStatus {
    #[default]
    Missing,
    EvidenceOnly,
    Recorded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FinalMarkerEnvelope {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub observed: Vec<String>,
    #[serde(default)]
    pub missing: Vec<String>,
}

impl FinalMarkerEnvelope {
    pub fn from_contract(
        required: impl IntoIterator<Item = String>,
        observed: impl IntoIterator<Item = String>,
    ) -> Self {
        let required = dedup_strings(required);
        let observed = dedup_strings(observed);
        let missing = required
            .iter()
            .filter(|marker| !observed.iter().any(|seen| seen == *marker))
            .cloned()
            .collect();
        Self {
            required,
            observed,
            missing,
        }
    }

    pub fn is_satisfied(&self) -> bool {
        self.missing.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DocDeltaEnvelope {
    #[serde(default)]
    pub status: ResultPayloadStatus,
    pub summary: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
}

impl DocDeltaEnvelope {
    pub fn has_recorded_payload(&self) -> bool {
        self.summary.is_some() || !self.paths.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkerEvidence {
    pub marker: String,
    pub line: String,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProofEnvelope {
    #[serde(default)]
    pub status: ResultPayloadStatus,
    pub summary: Option<String>,
    #[serde(default)]
    pub proof_bundle_ids: Vec<ProofBundleId>,
    #[serde(default)]
    pub fact_ids: Vec<FactId>,
    #[serde(default)]
    pub contradiction_ids: Vec<ContradictionId>,
    #[serde(default)]
    pub artifacts: Vec<ProofArtifact>,
}

impl ProofEnvelope {
    pub fn has_recorded_payload(&self) -> bool {
        self.summary.is_some()
            || !self.proof_bundle_ids.is_empty()
            || !self.fact_ids.is_empty()
            || !self.contradiction_ids.is_empty()
            || self
                .artifacts
                .iter()
                .any(|artifact| artifact.kind.supports_machine_readable_proof())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ClosureInputEnvelope {
    #[serde(default)]
    pub status: ResultPayloadStatus,
    #[serde(default)]
    pub final_markers: FinalMarkerEnvelope,
    #[serde(default)]
    pub marker_evidence: Vec<MarkerEvidence>,
}

impl ClosureInputEnvelope {
    pub fn has_evidence(&self) -> bool {
        !self.final_markers.required.is_empty()
            || !self.final_markers.observed.is_empty()
            || !self.marker_evidence.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClosureVerdictPayload {
    #[default]
    None,
    ContQa(ContQaClosureVerdict),
    Design(DesignClosureVerdict),
    Security(SecurityClosureVerdict),
    Integration(IntegrationClosureVerdict),
    Documentation(DocumentationClosureVerdict),
}

impl ClosureVerdictPayload {
    pub fn is_present(&self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContQaClosureVerdict {
    pub verdict: Option<String>,
    pub gate_state: Option<String>,
    pub gate_line: Option<String>,
    #[serde(default)]
    pub gate_dimensions: BTreeMap<String, String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IntegrationClosureVerdict {
    pub state: Option<String>,
    pub claims: Option<u32>,
    pub conflicts: Option<u32>,
    pub blockers: Option<u32>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DesignClosureVerdict {
    pub state: Option<String>,
    pub findings: Option<u32>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SecurityClosureVerdict {
    pub state: Option<String>,
    pub findings: Option<u32>,
    pub approvals: Option<u32>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DocumentationClosureVerdict {
    pub state: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeId {
    Codex,
    Claude,
    Opencode,
    Local,
}

impl RuntimeId {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "codex" => Some(Self::Codex),
            "claude" => Some(Self::Claude),
            "opencode" => Some(Self::Opencode),
            "local" => Some(Self::Local),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Opencode => "opencode",
            Self::Local => "local",
        }
    }

    pub fn skill_id(self) -> String {
        format!("runtime-{}", self.as_str())
    }
}

impl fmt::Display for RuntimeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeSelectionPolicy {
    pub requested_runtime: Option<RuntimeId>,
    #[serde(default)]
    pub allowed_runtimes: Vec<RuntimeId>,
    #[serde(default)]
    pub fallback_runtimes: Vec<RuntimeId>,
    pub selection_source: Option<String>,
}

impl RuntimeSelectionPolicy {
    pub fn normalized(&self) -> Self {
        let mut allowed_runtimes = dedup_runtime_ids(self.allowed_runtimes.clone());
        let fallback_runtimes = dedup_runtime_ids(self.fallback_runtimes.clone());

        if let Some(requested_runtime) = self.requested_runtime {
            if !allowed_runtimes
                .iter()
                .any(|runtime| *runtime == requested_runtime)
            {
                allowed_runtimes.insert(0, requested_runtime);
            }
        }
        for fallback in &fallback_runtimes {
            if !allowed_runtimes.iter().any(|runtime| runtime == fallback) {
                allowed_runtimes.push(*fallback);
            }
        }

        Self {
            requested_runtime: self.requested_runtime,
            allowed_runtimes,
            fallback_runtimes,
            selection_source: self.selection_source.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeFallbackRecord {
    pub requested_runtime: RuntimeId,
    pub selected_runtime: RuntimeId,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeSkillProjection {
    #[serde(default)]
    pub declared_skills: Vec<String>,
    #[serde(default)]
    pub projected_skills: Vec<String>,
    #[serde(default)]
    pub dropped_skills: Vec<String>,
    #[serde(default)]
    pub auto_attached_skills: Vec<String>,
}

impl RuntimeSkillProjection {
    pub fn normalized(&self) -> Self {
        let declared_skills = dedup_string_values(self.declared_skills.clone());
        let dropped_skills = dedup_string_values(self.dropped_skills.clone());
        let auto_attached_skills = dedup_string_values(self.auto_attached_skills.clone());
        let mut projected_skills = dedup_string_values(self.projected_skills.clone());
        for skill in &auto_attached_skills {
            if !projected_skills.iter().any(|projected| projected == skill) {
                projected_skills.push(skill.clone());
            }
        }

        Self {
            declared_skills,
            projected_skills,
            dropped_skills,
            auto_attached_skills,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeExecutionIdentity {
    pub runtime: RuntimeId,
    pub adapter: String,
    pub binary: String,
    pub provider: String,
    #[serde(default)]
    pub artifact_paths: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeExecutionRecord {
    pub policy: RuntimeSelectionPolicy,
    pub selected_runtime: RuntimeId,
    pub selection_reason: String,
    #[serde(default)]
    pub fallback: Option<RuntimeFallbackRecord>,
    pub execution_identity: RuntimeExecutionIdentity,
    #[serde(default)]
    pub skill_projection: RuntimeSkillProjection,
}

impl RuntimeExecutionRecord {
    pub fn normalized(&self) -> Self {
        Self {
            policy: self.policy.normalized(),
            selected_runtime: self.selected_runtime,
            selection_reason: self.selection_reason.clone(),
            fallback: self.fallback.clone(),
            execution_identity: self.execution_identity.clone(),
            skill_projection: self.skill_projection.normalized(),
        }
    }

    pub fn uses_fallback(&self) -> bool {
        self.fallback.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskExecutor {
    pub runtime_id: Option<RuntimeId>,
    #[serde(default)]
    pub fallback_runtimes: Vec<RuntimeId>,
    pub profile: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskContext7 {
    pub bundle: String,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskCompletionLevel {
    Contract,
    Integrated,
    Closure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskDurabilityLevel {
    None,
    Ephemeral,
    Durable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskProofLevel {
    Unit,
    Integration,
    Live,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskDocImpact {
    None,
    Owned,
    SharedPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskExitContract {
    pub completion: TaskCompletionLevel,
    pub durability: TaskDurabilityLevel,
    pub proof: TaskProofLevel,
    pub doc_impact: TaskDocImpact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDependency {
    pub task_id: TaskId,
    pub kind: TaskDependencyKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSeed {
    pub task_id: TaskId,
    pub wave_id: u32,
    pub wave_slug: String,
    pub wave_title: String,
    pub agent_id: String,
    pub agent_title: String,
    #[serde(default)]
    pub execution_model: WaveExecutionModel,
    pub role: TaskRole,
    pub closure_role: Option<ClosureRole>,
    pub state: TaskState,
    pub executor: TaskExecutor,
    pub context7: Option<TaskContext7>,
    pub skills: Vec<String>,
    pub components: Vec<String>,
    pub capabilities: Vec<String>,
    pub exit_contract: Option<TaskExitContract>,
    pub wave_dependencies: Vec<u32>,
    pub dependencies: Vec<TaskDependency>,
    #[serde(default)]
    pub depends_on_agent_ids: Vec<String>,
    #[serde(default)]
    pub reads_artifacts_from: Vec<ArtifactDependency>,
    #[serde(default)]
    pub writes_artifacts: Vec<String>,
    #[serde(default)]
    pub barrier_class: BarrierClass,
    #[serde(default)]
    pub parallel_safety: ParallelSafetyClass,
    #[serde(default)]
    pub exclusive_resources: Vec<String>,
    #[serde(default)]
    pub parallel_with: Vec<String>,
    pub required_role_prompts: Vec<String>,
    pub owned_paths: Vec<String>,
    pub deliverables: Vec<String>,
    pub declared_final_markers: Vec<String>,
    pub expected_final_markers: Vec<String>,
}

impl TaskSeed {
    pub fn depends_on_task_ids(&self) -> Vec<TaskId> {
        self.dependencies
            .iter()
            .map(|dependency| dependency.task_id.clone())
            .collect()
    }

    pub fn declared_task_record(&self) -> TaskRecord {
        TaskRecord::from_seed(self.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeclaredWaveDeliveryLink {
    pub initiative_id: Option<String>,
    pub release_id: Option<String>,
    pub acceptance_package_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DesignGatePlan {
    #[serde(default)]
    pub agent_ids: Vec<String>,
    pub ready_marker: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredWavePlan {
    pub wave_id: u32,
    pub slug: String,
    pub title: String,
    pub commit_message: Option<String>,
    pub depends_on: Vec<u32>,
    pub validation: Vec<String>,
    pub rollback: Vec<String>,
    pub proof: Vec<String>,
    pub wave_class: String,
    pub intent: Option<String>,
    pub delivery: Option<DeclaredWaveDeliveryLink>,
    pub design_gate: Option<DesignGatePlan>,
    #[serde(default)]
    pub execution_model: WaveExecutionModel,
    #[serde(default)]
    pub concurrency_budget: WaveConcurrencyBudgetPlan,
    pub task_seeds: Vec<TaskSeed>,
}

impl DeclaredWavePlan {
    pub fn task_seed(&self, task_id: &TaskId) -> Option<&TaskSeed> {
        self.task_seeds.iter().find(|task| &task.task_id == task_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptRecord {
    pub attempt_id: AttemptId,
    pub wave_id: u32,
    pub task_id: TaskId,
    pub attempt_number: u32,
    pub state: AttemptState,
    pub executor: String,
    pub created_at_ms: u128,
    pub started_at_ms: Option<u128>,
    pub finished_at_ms: Option<u128>,
    pub summary: Option<String>,
    pub proof_bundle_ids: Vec<ProofBundleId>,
    #[serde(default)]
    pub result_envelope_id: Option<ResultEnvelopeId>,
    #[serde(default)]
    pub runtime: Option<RuntimeExecutionRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosureDisposition {
    Pending,
    Ready,
    Blocked,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosureState {
    pub disposition: ClosureDisposition,
    pub required_final_markers: Vec<String>,
    pub observed_final_markers: Vec<String>,
    pub blocking_reasons: Vec<String>,
    pub satisfied_fact_ids: Vec<FactId>,
    pub contradiction_ids: Vec<ContradictionId>,
    #[serde(default)]
    pub verdict: ClosureVerdictPayload,
}

impl ClosureState {
    pub fn declared(required_final_markers: Vec<String>) -> Self {
        Self {
            disposition: ClosureDisposition::Pending,
            required_final_markers,
            observed_final_markers: Vec::new(),
            blocking_reasons: Vec::new(),
            satisfied_fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            verdict: ClosureVerdictPayload::None,
        }
    }

    pub fn expected_result_envelope_disposition(
        attempt_state: AttemptState,
        final_markers: &FinalMarkerEnvelope,
        blocking_reasons: &[String],
    ) -> ClosureDisposition {
        match attempt_state {
            AttemptState::Succeeded
                if final_markers.is_satisfied() && blocking_reasons.is_empty() =>
            {
                ClosureDisposition::Ready
            }
            AttemptState::Planned | AttemptState::Running => ClosureDisposition::Pending,
            _ => ClosureDisposition::Blocked,
        }
    }

    pub fn matches_result_envelope_disposition(
        &self,
        attempt_state: AttemptState,
        final_markers: &FinalMarkerEnvelope,
    ) -> bool {
        let expected = Self::expected_result_envelope_disposition(
            attempt_state,
            final_markers,
            &self.blocking_reasons,
        );

        self.disposition == expected
            || matches!(
                (self.disposition, expected),
                (ClosureDisposition::Closed, ClosureDisposition::Ready)
            )
    }

    pub fn has_machine_readable_signal(&self) -> bool {
        self.disposition != ClosureDisposition::Pending
            || !self.blocking_reasons.is_empty()
            || !self.satisfied_fact_ids.is_empty()
            || !self.contradiction_ids.is_empty()
            || self.verdict.is_present()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRecord {
    pub seed: TaskSeed,
    pub state: TaskState,
    pub latest_attempt_id: Option<AttemptId>,
    pub latest_gate: Option<GateVerdict>,
    pub proof_bundle_ids: Vec<ProofBundleId>,
    pub fact_ids: Vec<FactId>,
    pub contradiction_ids: Vec<ContradictionId>,
    pub pending_human_input_request_ids: Vec<HumanInputRequestId>,
    pub closure: ClosureState,
}

impl TaskRecord {
    pub fn from_seed(seed: TaskSeed) -> Self {
        let state = seed.state;
        let closure = ClosureState::declared(seed.expected_final_markers.clone());
        Self {
            seed,
            state,
            latest_attempt_id: None,
            latest_gate: None,
            proof_bundle_ids: Vec::new(),
            fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            pending_human_input_request_ids: Vec::new(),
            closure,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactCitation {
    pub path: String,
    pub line: Option<u32>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum LineageRef {
    Fact(FactId),
    Question(QuestionId),
    Assumption(AssumptionId),
    Decision(DecisionId),
    HumanInput(HumanInputRequestId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum LineageRecordSubject {
    Question {
        question_id: QuestionId,
    },
    Assumption {
        assumption_id: AssumptionId,
    },
    Decision {
        decision_id: DecisionId,
    },
    SupersededDecision {
        decision_id: DecisionId,
        superseded_by_decision_id: Option<DecisionId>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageRecord {
    pub record_id: LineageRecordId,
    pub wave_id: u32,
    pub task_id: Option<TaskId>,
    pub subject: LineageRecordSubject,
    pub authority: DesignAuthority,
    pub state: LineageState,
    pub summary: String,
    pub detail: Option<String>,
    #[serde(default)]
    pub citations: Vec<FactCitation>,
    #[serde(default)]
    pub upstream_refs: Vec<LineageRef>,
    #[serde(default)]
    pub supporting_fact_ids: Vec<FactId>,
    #[serde(default)]
    pub downstream_task_ids: Vec<TaskId>,
    #[serde(default)]
    pub downstream_wave_ids: Vec<u32>,
    #[serde(default)]
    pub required_human_input_request_ids: Vec<HumanInputRequestId>,
    pub introduced_by_event_id: Option<String>,
}

impl LineageRecord {
    pub fn question_id(&self) -> Option<&QuestionId> {
        match &self.subject {
            LineageRecordSubject::Question { question_id } => Some(question_id),
            _ => None,
        }
    }

    pub fn assumption_id(&self) -> Option<&AssumptionId> {
        match &self.subject {
            LineageRecordSubject::Assumption { assumption_id } => Some(assumption_id),
            _ => None,
        }
    }

    pub fn decision_id(&self) -> Option<&DecisionId> {
        match &self.subject {
            LineageRecordSubject::Decision { decision_id }
            | LineageRecordSubject::SupersededDecision { decision_id, .. } => Some(decision_id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactRecord {
    pub fact_id: FactId,
    pub wave_id: u32,
    pub task_id: Option<TaskId>,
    pub attempt_id: Option<AttemptId>,
    pub state: FactState,
    pub summary: String,
    pub detail: Option<String>,
    pub source_artifact: Option<String>,
    pub introduced_by_event_id: Option<String>,
    pub citations: Vec<FactCitation>,
    pub contradiction_ids: Vec<ContradictionId>,
    pub superseded_by_fact_id: Option<FactId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContradictionRecord {
    pub contradiction_id: ContradictionId,
    pub wave_id: u32,
    #[serde(default)]
    pub task_ids: Vec<TaskId>,
    #[serde(default)]
    pub fact_ids: Vec<FactId>,
    pub state: ContradictionState,
    pub summary: String,
    pub detail: Option<String>,
    pub introduced_by_event_id: Option<String>,
    #[serde(default)]
    pub invalidated_refs: Vec<LineageRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofArtifact {
    pub path: String,
    pub kind: ArtifactKind,
    pub digest: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofBundle {
    pub proof_bundle_id: ProofBundleId,
    pub wave_id: u32,
    pub task_id: Option<TaskId>,
    pub attempt_id: Option<AttemptId>,
    pub status: ProofStatus,
    pub summary: String,
    pub artifacts: Vec<ProofArtifact>,
    pub supporting_fact_ids: Vec<FactId>,
    pub contradiction_ids: Vec<ContradictionId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateDisposition {
    Pass,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateVerdict {
    pub gate_id: GateId,
    pub wave_id: u32,
    pub task_id: Option<TaskId>,
    pub attempt_id: Option<AttemptId>,
    pub disposition: GateDisposition,
    pub blocking_reasons: Vec<String>,
    pub satisfied_fact_ids: Vec<FactId>,
    pub contradiction_ids: Vec<ContradictionId>,
    pub required_human_input_request_ids: Vec<HumanInputRequestId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RerunRequest {
    pub request_id: RerunRequestId,
    pub wave_id: u32,
    pub task_ids: Vec<TaskId>,
    pub requested_attempt_id: Option<AttemptId>,
    pub requested_by: String,
    pub reason: String,
    #[serde(default)]
    pub scope: RerunScope,
    pub state: RerunState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveClosureOverrideRecord {
    pub override_id: String,
    pub wave_id: u32,
    pub status: WaveClosureOverrideStatus,
    pub reason: String,
    pub requested_by: String,
    pub source_run_id: String,
    #[serde(default)]
    pub evidence_paths: Vec<String>,
    pub detail: Option<String>,
    pub applied_at_ms: u128,
    pub cleared_at_ms: Option<u128>,
}

impl WaveClosureOverrideRecord {
    pub fn is_active(&self) -> bool {
        matches!(self.status, WaveClosureOverrideStatus::Applied)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanInputRequest {
    pub request_id: HumanInputRequestId,
    pub wave_id: u32,
    pub task_id: Option<TaskId>,
    pub state: HumanInputState,
    #[serde(default)]
    pub workflow_kind: HumanInputWorkflowKind,
    pub prompt: String,
    pub route: String,
    pub requested_by: String,
    pub answer: Option<String>,
}

impl HumanInputRequest {
    pub fn effective_workflow_kind(&self) -> HumanInputWorkflowKind {
        self.workflow_kind.effective(&self.route)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultEnvelope {
    pub result_envelope_id: ResultEnvelopeId,
    pub wave_id: u32,
    pub task_id: TaskId,
    pub attempt_id: AttemptId,
    pub agent_id: String,
    pub task_role: TaskRole,
    pub closure_role: Option<ClosureRole>,
    #[serde(default)]
    pub source: ResultEnvelopeSource,
    pub attempt_state: AttemptState,
    pub disposition: ResultDisposition,
    pub summary: Option<String>,
    pub output_text: Option<String>,
    #[serde(default)]
    pub proof: ProofEnvelope,
    #[serde(default)]
    pub doc_delta: DocDeltaEnvelope,
    #[serde(default)]
    pub closure_input: ClosureInputEnvelope,
    pub closure: ClosureState,
    #[serde(default)]
    pub runtime: Option<RuntimeExecutionRecord>,
    pub created_at_ms: u128,
}

impl ResultEnvelope {
    pub fn is_terminal(&self) -> bool {
        self.attempt_state.is_terminal()
    }

    pub fn is_completed_or_failed(&self) -> bool {
        matches!(
            self.disposition,
            ResultDisposition::Completed | ResultDisposition::Failed
        )
    }

    pub fn expected_disposition(&self) -> ResultDisposition {
        ResultDisposition::from_attempt_state(
            self.attempt_state,
            self.closure_input.final_markers.missing.len(),
        )
    }

    pub fn should_surface_as_latest_relevant(&self) -> bool {
        matches!(
            self.disposition,
            ResultDisposition::Completed | ResultDisposition::Failed
        )
    }
}

impl AttemptState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Aborted | Self::Refused
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerEventPayload {
    #[default]
    None,
    WaveClaimUpdated {
        claim: WaveClaimRecord,
    },
    TaskLeaseUpdated {
        lease: TaskLeaseRecord,
    },
    SchedulerBudgetUpdated {
        budget: SchedulerBudgetRecord,
    },
    WaveWorktreeUpdated {
        worktree: WaveWorktreeRecord,
    },
    WavePromotionUpdated {
        promotion: WavePromotionRecord,
    },
    WaveSchedulingUpdated {
        scheduling: WaveSchedulingRecord,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ControlEventPayload {
    #[default]
    None,
    DeclaredWavePlan {
        plan: DeclaredWavePlan,
    },
    TaskSeeded {
        task: TaskSeed,
    },
    AttemptUpdated {
        attempt: AttemptRecord,
    },
    GateEvaluated {
        gate: GateVerdict,
    },
    FactObserved {
        fact: FactRecord,
    },
    LineageUpdated {
        lineage: LineageRecord,
    },
    ContradictionUpdated {
        contradiction: ContradictionRecord,
    },
    ProofRecorded {
        proof: ProofBundle,
    },
    RerunRequested {
        rerun: RerunRequest,
    },
    ClosureOverrideUpdated {
        closure_override: WaveClosureOverrideRecord,
    },
    HumanInputUpdated {
        request: HumanInputRequest,
    },
    ControlDirectiveRecorded {
        directive: ControlDirectiveRecord,
    },
    DirectiveDeliveryUpdated {
        delivery: DirectiveDeliveryRecord,
    },
    OrchestratorSessionUpdated {
        session: OrchestratorSessionRecord,
    },
    OperatorShellSessionUpdated {
        session: OperatorShellSessionRecord,
    },
    OperatorShellTurnRecorded {
        turn: OperatorShellTurnRecord,
    },
    HeadProposalUpdated {
        proposal: HeadProposalRecord,
    },
    RecoveryPlanUpdated {
        recovery_plan: RecoveryPlanRecord,
    },
    RecoveryActionRecorded {
        recovery_action: RecoveryActionRecord,
    },
    AgentSandboxUpdated {
        sandbox: AgentSandboxRecord,
    },
    MergeIntentRecorded {
        merge_intent: MergeIntentRecord,
    },
    MergeResultRecorded {
        merge_result: MergeResultRecord,
    },
    InvalidationRecorded {
        invalidation: InvalidationRecord,
    },
    ResultEnvelopeRecorded {
        result: ResultEnvelope,
    },
}

pub fn declared_wave_plan(wave: &WaveDocument) -> DeclaredWavePlan {
    let task_seeds = declaration_task_seeds(wave);
    DeclaredWavePlan {
        wave_id: wave.metadata.id,
        slug: wave.metadata.slug.clone(),
        title: wave.metadata.title.clone(),
        commit_message: wave.commit_message.clone(),
        depends_on: wave.metadata.depends_on.clone(),
        validation: wave.metadata.validation.clone(),
        rollback: wave.metadata.rollback.clone(),
        proof: wave.metadata.proof.clone(),
        wave_class: format!("{:?}", wave.metadata.wave_class).to_ascii_lowercase(),
        intent: wave
            .metadata
            .intent
            .map(|intent| format!("{intent:?}").to_ascii_lowercase()),
        delivery: wave
            .metadata
            .delivery
            .as_ref()
            .map(|delivery| DeclaredWaveDeliveryLink {
                initiative_id: delivery.initiative_id.clone(),
                release_id: delivery.release_id.clone(),
                acceptance_package_id: delivery.acceptance_package_id.clone(),
            }),
        execution_model: match wave.metadata.execution_model {
            wave_spec::WaveExecutionModel::Serial => WaveExecutionModel::Serial,
            wave_spec::WaveExecutionModel::MultiAgent => WaveExecutionModel::MultiAgent,
        },
        concurrency_budget: WaveConcurrencyBudgetPlan {
            max_concurrent_implementation_agents: wave
                .metadata
                .concurrency_budget
                .max_concurrent_implementation_agents,
            max_concurrent_report_only_agents: wave
                .metadata
                .concurrency_budget
                .max_concurrent_report_only_agents,
            max_merge_operations: wave.metadata.concurrency_budget.max_merge_operations,
            max_conflict_resolution_agents: wave
                .metadata
                .concurrency_budget
                .max_conflict_resolution_agents,
        },
        design_gate: wave
            .metadata
            .design_gate
            .as_ref()
            .map(|gate| DesignGatePlan {
                agent_ids: gate.agent_ids.clone(),
                ready_marker: Some(gate.ready_marker.clone()),
            }),
        task_seeds,
    }
}

pub fn declaration_task_seeds(wave: &WaveDocument) -> Vec<TaskSeed> {
    let agent_task_ids = wave
        .agents
        .iter()
        .map(|agent| {
            (
                agent.id.clone(),
                task_id_for_agent(wave.metadata.id, &agent.id),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let artifact_producers = artifact_producers(wave);
    let wave_dependency_task_ids = wave
        .metadata
        .depends_on
        .iter()
        .map(|wave_id| task_id_for_agent(*wave_id, "A0"))
        .collect::<Vec<_>>();
    let implementation_task_ids = wave
        .implementation_agents()
        .map(|agent| task_id_for_agent(wave.metadata.id, &agent.id))
        .collect::<Vec<_>>();
    let eval_task_id = wave
        .agents
        .iter()
        .find(|agent| agent.id == "E0")
        .map(|agent| task_id_for_agent(wave.metadata.id, &agent.id));
    let design_review_task_id = wave
        .agents
        .iter()
        .find(|agent| agent.id == "A6")
        .map(|agent| task_id_for_agent(wave.metadata.id, &agent.id));
    let security_review_task_id = wave
        .agents
        .iter()
        .find(|agent| agent.id == "A7")
        .map(|agent| task_id_for_agent(wave.metadata.id, &agent.id));
    let integration_task_id = wave
        .agents
        .iter()
        .find(|agent| agent.id == "A8")
        .map(|agent| task_id_for_agent(wave.metadata.id, &agent.id));
    let documentation_task_id = wave
        .agents
        .iter()
        .find(|agent| agent.id == "A9")
        .map(|agent| task_id_for_agent(wave.metadata.id, &agent.id));
    let design_gate_task_ids = wave
        .design_gate_agent_ids()
        .iter()
        .filter_map(|agent_id| {
            wave.agents
                .iter()
                .find(|agent| agent.id == *agent_id)
                .map(|agent| task_id_for_agent(wave.metadata.id, &agent.id))
        })
        .collect::<Vec<_>>();

    wave.agents
        .iter()
        .map(|agent| TaskSeed {
            task_id: task_id_for_agent(wave.metadata.id, &agent.id),
            wave_id: wave.metadata.id,
            wave_slug: wave.metadata.slug.clone(),
            wave_title: wave.metadata.title.clone(),
            agent_id: agent.id.clone(),
            agent_title: agent.title.clone(),
            execution_model: match wave.metadata.execution_model {
                wave_spec::WaveExecutionModel::Serial => WaveExecutionModel::Serial,
                wave_spec::WaveExecutionModel::MultiAgent => WaveExecutionModel::MultiAgent,
            },
            role: task_role(agent),
            closure_role: closure_role(agent),
            state: TaskState::Declared,
            executor: task_executor(agent),
            context7: task_context7(agent),
            skills: agent.skills.clone(),
            components: agent.components.clone(),
            capabilities: agent.capabilities.clone(),
            exit_contract: task_exit_contract(agent),
            wave_dependencies: wave.metadata.depends_on.clone(),
            dependencies: task_dependencies(
                wave,
                agent,
                &wave_dependency_task_ids,
                &implementation_task_ids,
                eval_task_id.as_ref(),
                design_review_task_id.as_ref(),
                security_review_task_id.as_ref(),
                integration_task_id.as_ref(),
                documentation_task_id.as_ref(),
                &design_gate_task_ids,
                &agent_task_ids,
                &artifact_producers,
            ),
            depends_on_agent_ids: agent.depends_on_agents.clone(),
            reads_artifacts_from: agent
                .reads_artifacts_from
                .iter()
                .map(|artifact| ArtifactDependency {
                    artifact: artifact.clone(),
                    source_agent_id: unique_artifact_source_agent_id(&artifact_producers, artifact),
                })
                .collect(),
            writes_artifacts: agent.writes_artifacts.clone(),
            barrier_class: match agent.barrier_class {
                wave_spec::BarrierClass::Independent => BarrierClass::Independent,
                wave_spec::BarrierClass::MergeAfter => BarrierClass::MergeAfter,
                wave_spec::BarrierClass::IntegrationBarrier => BarrierClass::IntegrationBarrier,
                wave_spec::BarrierClass::ClosureBarrier => BarrierClass::ClosureBarrier,
                wave_spec::BarrierClass::ReportOnly => BarrierClass::ReportOnly,
            },
            parallel_safety: match agent.parallel_safety {
                wave_spec::ParallelSafetyClass::Derived => ParallelSafetyClass::Derived,
                wave_spec::ParallelSafetyClass::ParallelSafe => ParallelSafetyClass::ParallelSafe,
                wave_spec::ParallelSafetyClass::Serialized => ParallelSafetyClass::Serialized,
            },
            exclusive_resources: agent.exclusive_resources.clone(),
            parallel_with: agent.parallel_with.clone(),
            required_role_prompts: agent
                .expected_role_prompts()
                .iter()
                .map(|path| (*path).to_string())
                .collect(),
            owned_paths: agent.file_ownership.clone(),
            deliverables: agent.deliverables.clone(),
            declared_final_markers: agent.final_markers.clone(),
            expected_final_markers: agent
                .expected_final_markers()
                .iter()
                .map(|marker| (*marker).to_string())
                .collect(),
        })
        .collect()
}

pub fn task_id_for_agent(wave_id: u32, agent_id: &str) -> TaskId {
    TaskId::new(format!(
        "wave-{wave_id:02}:agent-{}",
        agent_id.to_ascii_lowercase()
    ))
}

pub fn inferred_closure_role_for_agent(agent_id: &str) -> Option<ClosureRole> {
    match agent_id {
        "E0" => Some(ClosureRole::ContEval),
        "A6" => Some(ClosureRole::DesignReview),
        "A7" => Some(ClosureRole::SecurityReview),
        "A8" => Some(ClosureRole::Integration),
        "A9" => Some(ClosureRole::Documentation),
        "A0" => Some(ClosureRole::ContQa),
        _ => None,
    }
}

fn closure_role(agent: &WaveAgent) -> Option<ClosureRole> {
    inferred_closure_role_for_agent(agent.id.as_str())
}

pub fn inferred_task_role_for_agent(agent_id: &str, skills: &[String]) -> TaskRole {
    match agent_id {
        "A6" => TaskRole::Design,
        "A7" => TaskRole::Security,
        "A8" => TaskRole::Integration,
        "A9" => TaskRole::Documentation,
        "A0" => TaskRole::ContQa,
        "E0" => TaskRole::ContEval,
        _ if skills.iter().any(|skill| skill == "role-security") => TaskRole::Security,
        _ if skills.iter().any(|skill| skill == "role-design") => TaskRole::Design,
        _ if skills.iter().any(|skill| skill == "role-infra") => TaskRole::Infra,
        _ if skills.iter().any(|skill| skill == "role-deploy") => TaskRole::Deploy,
        _ if skills.iter().any(|skill| skill == "role-research") => TaskRole::Research,
        _ => TaskRole::Implementation,
    }
}

fn task_role(agent: &WaveAgent) -> TaskRole {
    inferred_task_role_for_agent(agent.id.as_str(), &agent.skills)
}

fn dedup_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        if !deduped.iter().any(|existing| existing == &value) {
            deduped.push(value);
        }
    }
    deduped
}

fn task_dependencies(
    wave: &WaveDocument,
    agent: &WaveAgent,
    wave_dependency_task_ids: &[TaskId],
    implementation_task_ids: &[TaskId],
    eval_task_id: Option<&TaskId>,
    design_review_task_id: Option<&TaskId>,
    security_review_task_id: Option<&TaskId>,
    integration_task_id: Option<&TaskId>,
    documentation_task_id: Option<&TaskId>,
    design_gate_task_ids: &[TaskId],
    agent_task_ids: &BTreeMap<String, TaskId>,
    artifact_producers: &BTreeMap<String, Vec<String>>,
) -> Vec<TaskDependency> {
    let mut dependencies = wave_dependency_task_ids
        .iter()
        .cloned()
        .map(|task_id| TaskDependency {
            task_id,
            kind: TaskDependencyKind::WaveClosure,
        })
        .collect::<Vec<_>>();

    let specific_dependencies = match agent.id.as_str() {
        "E0" => implementation_task_ids
            .iter()
            .cloned()
            .map(|task_id| TaskDependency {
                task_id,
                kind: TaskDependencyKind::ImplementationSlice,
            })
            .collect(),
        "A6" => {
            let mut dependencies = implementation_task_ids
                .iter()
                .cloned()
                .map(|task_id| TaskDependency {
                    task_id,
                    kind: TaskDependencyKind::ImplementationSlice,
                })
                .collect::<Vec<_>>();
            if let Some(task_id) = eval_task_id {
                dependencies.push(TaskDependency {
                    task_id: task_id.clone(),
                    kind: TaskDependencyKind::ContEvalVerdict,
                });
            }
            dependencies
        }
        "A7" => {
            let mut dependencies = implementation_task_ids
                .iter()
                .cloned()
                .map(|task_id| TaskDependency {
                    task_id,
                    kind: TaskDependencyKind::ImplementationSlice,
                })
                .collect::<Vec<_>>();
            if let Some(task_id) = eval_task_id {
                dependencies.push(TaskDependency {
                    task_id: task_id.clone(),
                    kind: TaskDependencyKind::ContEvalVerdict,
                });
            }
            dependencies
        }
        "A8" => {
            let mut dependencies = implementation_task_ids
                .iter()
                .cloned()
                .map(|task_id| TaskDependency {
                    task_id,
                    kind: TaskDependencyKind::ImplementationSlice,
                })
                .collect::<Vec<_>>();
            if let Some(task_id) = eval_task_id {
                dependencies.push(TaskDependency {
                    task_id: task_id.clone(),
                    kind: TaskDependencyKind::ContEvalVerdict,
                });
            }
            if let Some(task_id) = design_review_task_id {
                dependencies.push(TaskDependency {
                    task_id: task_id.clone(),
                    kind: TaskDependencyKind::DesignReviewVerdict,
                });
            }
            if let Some(task_id) = security_review_task_id {
                dependencies.push(TaskDependency {
                    task_id: task_id.clone(),
                    kind: TaskDependencyKind::SecurityReviewVerdict,
                });
            }
            dependencies
        }
        "A9" => integration_task_id
            .into_iter()
            .cloned()
            .map(|task_id| TaskDependency {
                task_id,
                kind: TaskDependencyKind::IntegrationClosure,
            })
            .collect(),
        "A0" => documentation_task_id
            .into_iter()
            .cloned()
            .map(|task_id| TaskDependency {
                task_id,
                kind: TaskDependencyKind::DocumentationClosure,
            })
            .collect(),
        _ => Vec::new(),
    };

    dependencies.extend(specific_dependencies);
    if wave.is_multi_agent() {
        extend_multi_agent_dependencies(
            wave,
            agent,
            agent_task_ids,
            artifact_producers,
            &mut dependencies,
        );
    }
    if !agent.is_closure_agent() && !agent.is_design_worker() {
        for task_id in design_gate_task_ids {
            push_dependency(
                &mut dependencies,
                task_id.clone(),
                TaskDependencyKind::DesignApproval,
            );
        }
    }
    dependencies
}

fn extend_multi_agent_dependencies(
    wave: &WaveDocument,
    agent: &WaveAgent,
    agent_task_ids: &BTreeMap<String, TaskId>,
    artifact_producers: &BTreeMap<String, Vec<String>>,
    dependencies: &mut Vec<TaskDependency>,
) {
    for dependency_agent_id in &agent.depends_on_agents {
        if let Some(task_id) = agent_task_ids.get(dependency_agent_id) {
            push_dependency(
                dependencies,
                task_id.clone(),
                TaskDependencyKind::AgentGraph,
            );
        }
    }

    for artifact in &agent.reads_artifacts_from {
        if let Some(task_id) =
            unique_artifact_source_task_id(agent_task_ids, artifact_producers, artifact)
        {
            push_dependency(dependencies, task_id, TaskDependencyKind::ArtifactFlow);
        }
    }

    match agent.barrier_class {
        wave_spec::BarrierClass::IntegrationBarrier => {
            for barrier_agent in wave
                .agents
                .iter()
                .filter(|candidate| candidate.id != agent.id)
                .filter(|candidate| candidate.blocks_integration_barrier())
            {
                if let Some(task_id) = agent_task_ids.get(&barrier_agent.id) {
                    push_dependency(dependencies, task_id.clone(), TaskDependencyKind::Barrier);
                }
            }
        }
        wave_spec::BarrierClass::ClosureBarrier => {
            for barrier_agent in wave
                .agents
                .iter()
                .filter(|candidate| candidate.id != agent.id)
                .filter(|candidate| {
                    !matches!(candidate.barrier_class, wave_spec::BarrierClass::ReportOnly)
                })
            {
                if let Some(task_id) = agent_task_ids.get(&barrier_agent.id) {
                    push_dependency(dependencies, task_id.clone(), TaskDependencyKind::Barrier);
                }
            }
        }
        _ => {}
    }
}

fn artifact_producers(wave: &WaveDocument) -> BTreeMap<String, Vec<String>> {
    let mut producers = BTreeMap::<String, Vec<String>>::new();
    for agent in &wave.agents {
        for artifact in &agent.writes_artifacts {
            producers
                .entry(artifact.clone())
                .or_default()
                .push(agent.id.clone());
        }
    }
    producers
}

fn unique_artifact_source_agent_id(
    artifact_producers: &BTreeMap<String, Vec<String>>,
    artifact: &str,
) -> Option<String> {
    match artifact_producers.get(artifact) {
        Some(agent_ids) if agent_ids.len() == 1 => agent_ids.first().cloned(),
        _ => None,
    }
}

fn unique_artifact_source_task_id(
    agent_task_ids: &BTreeMap<String, TaskId>,
    artifact_producers: &BTreeMap<String, Vec<String>>,
    artifact: &str,
) -> Option<TaskId> {
    let agent_id = unique_artifact_source_agent_id(artifact_producers, artifact)?;
    agent_task_ids.get(&agent_id).cloned()
}

fn push_dependency(
    dependencies: &mut Vec<TaskDependency>,
    task_id: TaskId,
    kind: TaskDependencyKind,
) {
    if dependencies
        .iter()
        .any(|dependency| dependency.task_id == task_id && dependency.kind == kind)
    {
        return;
    }
    dependencies.push(TaskDependency { task_id, kind });
}

fn task_executor(agent: &WaveAgent) -> TaskExecutor {
    TaskExecutor {
        runtime_id: authored_runtime_id(agent),
        fallback_runtimes: authored_runtime_fallbacks(agent),
        profile: agent.executor.get("profile").cloned(),
        model: agent.executor.get("model").cloned(),
        params: agent
            .executor
            .iter()
            .filter(|(key, _)| is_runtime_neutral_executor_key(key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    }
}

pub fn runtime_selection_policy_for_agent(agent: &WaveAgent) -> RuntimeSelectionPolicy {
    let (requested_runtime, selection_source) = authored_runtime_selection(agent);
    let fallback_runtimes = authored_runtime_fallbacks(agent);
    let mut allowed_runtimes = Vec::new();
    if let Some(runtime) = requested_runtime {
        allowed_runtimes.push(runtime);
    }
    allowed_runtimes.extend(fallback_runtimes.iter().copied());

    RuntimeSelectionPolicy {
        requested_runtime,
        allowed_runtimes: dedup_runtime_ids(allowed_runtimes),
        fallback_runtimes,
        selection_source,
    }
}

fn authored_runtime_id(agent: &WaveAgent) -> Option<RuntimeId> {
    authored_runtime_selection(agent).0
}

fn authored_runtime_selection(agent: &WaveAgent) -> (Option<RuntimeId>, Option<String>) {
    if let Some(runtime) = agent
        .executor
        .get("id")
        .and_then(|value| RuntimeId::parse(value))
    {
        return (Some(runtime), Some("executor.id".to_string()));
    }
    if let Some(runtime) = agent.executor.keys().find_map(|key| {
        key.split_once('.')
            .and_then(|(prefix, _)| RuntimeId::parse(prefix))
    }) {
        return (
            Some(runtime),
            Some(format!("executor.{}-fields", runtime.as_str())),
        );
    }
    if let Some(runtime) = agent
        .executor
        .get("profile")
        .and_then(|profile| runtime_in_profile_name(profile))
    {
        return (Some(runtime), Some("executor.profile".to_string()));
    }
    (None, None)
}

fn authored_runtime_fallbacks(agent: &WaveAgent) -> Vec<RuntimeId> {
    agent
        .executor
        .get("fallbacks")
        .map(|value| parse_runtime_id_list(value))
        .unwrap_or_default()
}

fn is_runtime_neutral_executor_key(key: &str) -> bool {
    key != "profile"
        && key != "model"
        && key != "id"
        && key != "fallbacks"
        && key
            .split_once('.')
            .and_then(|(prefix, _)| RuntimeId::parse(prefix))
            .is_none()
}

fn runtime_in_profile_name(profile: &str) -> Option<RuntimeId> {
    profile.split(['-', '_']).find_map(RuntimeId::parse)
}

fn parse_runtime_id_list(raw: &str) -> Vec<RuntimeId> {
    dedup_runtime_ids(
        raw.split([',', ' ', '\n', '\t'])
            .filter_map(RuntimeId::parse)
            .collect(),
    )
}

fn dedup_runtime_ids(values: Vec<RuntimeId>) -> Vec<RuntimeId> {
    let mut deduped = Vec::new();
    for value in values {
        if !deduped.iter().any(|existing| existing == &value) {
            deduped.push(value);
        }
    }
    deduped
}

fn dedup_string_values(values: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        if !deduped.iter().any(|existing| existing == &value) {
            deduped.push(value);
        }
    }
    deduped
}

fn task_context7(agent: &WaveAgent) -> Option<TaskContext7> {
    agent.context7.as_ref().map(|context7| TaskContext7 {
        bundle: context7.bundle.clone(),
        query: context7.query.clone(),
    })
}

fn task_exit_contract(agent: &WaveAgent) -> Option<TaskExitContract> {
    agent
        .exit_contract
        .as_ref()
        .map(|contract| TaskExitContract {
            completion: match contract.completion {
                wave_spec::CompletionLevel::Contract => TaskCompletionLevel::Contract,
                wave_spec::CompletionLevel::Integrated => TaskCompletionLevel::Integrated,
                wave_spec::CompletionLevel::Closure => TaskCompletionLevel::Closure,
            },
            durability: match contract.durability {
                wave_spec::DurabilityLevel::None => TaskDurabilityLevel::None,
                wave_spec::DurabilityLevel::Ephemeral => TaskDurabilityLevel::Ephemeral,
                wave_spec::DurabilityLevel::Durable => TaskDurabilityLevel::Durable,
            },
            proof: match contract.proof {
                wave_spec::ProofLevel::Unit => TaskProofLevel::Unit,
                wave_spec::ProofLevel::Integration => TaskProofLevel::Integration,
                wave_spec::ProofLevel::Live => TaskProofLevel::Live,
                wave_spec::ProofLevel::Review => TaskProofLevel::Review,
            },
            doc_impact: match contract.doc_impact {
                wave_spec::DocImpact::None => TaskDocImpact::None,
                wave_spec::DocImpact::Owned => TaskDocImpact::Owned,
                wave_spec::DocImpact::SharedPlan => TaskDocImpact::SharedPlan,
            },
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use wave_config::ExecutionMode;
    use wave_spec::ExitContract;
    use wave_spec::WaveMetadata;

    fn default_wave_metadata() -> WaveMetadata {
        WaveMetadata {
            id: 0,
            slug: String::new(),
            title: String::new(),
            mode: ExecutionMode::DarkFactory,
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

    #[test]
    fn declaration_mapping_builds_explicit_closure_dependencies() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/10-authority-reset.md"),
            metadata: WaveMetadata {
                id: 10,
                slug: "authority-reset".to_string(),
                title: "Authority reset".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["planner".to_string()],
                depends_on: vec![9],
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["docs/implementation/rust-wave-0.2-architecture.md".to_string()],
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 10".to_string()),
            commit_message: Some("Feat: authority core".to_string()),
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![
                agent(
                    "A1",
                    "Implementation",
                    vec!["role-implementation"],
                    vec!["crates/wave-domain/src/lib.rs"],
                ),
                agent(
                    "E0",
                    "Eval",
                    vec!["role-cont-eval"],
                    vec![".wave/evals/wave-10.md"],
                ),
                agent(
                    "A8",
                    "Integration",
                    vec!["role-integration"],
                    vec![".wave/integration/wave-10.md"],
                ),
                agent(
                    "A9",
                    "Docs",
                    vec!["role-documentation"],
                    vec!["docs/plans/master-plan.md"],
                ),
                agent(
                    "A0",
                    "QA",
                    vec!["role-cont-qa"],
                    vec![".wave/reviews/wave-10.md"],
                ),
            ],
        };

        let plan = declared_wave_plan(&wave);
        assert_eq!(plan.task_seeds.len(), 5);
        assert_eq!(plan.commit_message.as_deref(), Some("Feat: authority core"));
        assert_eq!(plan.depends_on, vec![9]);

        let implementation = plan
            .task_seeds
            .iter()
            .find(|task| task.agent_id == "A1")
            .expect("implementation");
        assert_eq!(implementation.closure_role, None);
        assert_eq!(implementation.state, TaskState::Declared);
        assert_eq!(
            implementation.dependencies,
            vec![TaskDependency {
                task_id: task_id_for_agent(9, "A0"),
                kind: TaskDependencyKind::WaveClosure,
            }]
        );
        assert_eq!(
            implementation.executor.profile.as_deref(),
            Some("implement-codex")
        );
        assert!(implementation.required_role_prompts.is_empty());
        assert_eq!(
            implementation.expected_final_markers,
            vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ]
        );
        assert_eq!(
            implementation.declared_task_record().closure,
            ClosureState::declared(vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ])
        );

        let eval = plan
            .task_seeds
            .iter()
            .find(|task| task.agent_id == "E0")
            .expect("eval");
        assert_eq!(eval.role, TaskRole::ContEval);
        assert_eq!(eval.closure_role, Some(ClosureRole::ContEval));
        assert_eq!(
            eval.dependencies,
            vec![
                TaskDependency {
                    task_id: task_id_for_agent(9, "A0"),
                    kind: TaskDependencyKind::WaveClosure,
                },
                TaskDependency {
                    task_id: task_id_for_agent(10, "A1"),
                    kind: TaskDependencyKind::ImplementationSlice,
                },
            ]
        );
        assert_eq!(
            eval.depends_on_task_ids(),
            vec![task_id_for_agent(9, "A0"), task_id_for_agent(10, "A1")]
        );

        let integration = plan
            .task_seeds
            .iter()
            .find(|task| task.agent_id == "A8")
            .expect("integration");
        assert_eq!(
            integration.dependencies,
            vec![
                TaskDependency {
                    task_id: task_id_for_agent(9, "A0"),
                    kind: TaskDependencyKind::WaveClosure,
                },
                TaskDependency {
                    task_id: task_id_for_agent(10, "A1"),
                    kind: TaskDependencyKind::ImplementationSlice,
                },
                TaskDependency {
                    task_id: task_id_for_agent(10, "E0"),
                    kind: TaskDependencyKind::ContEvalVerdict,
                },
            ]
        );

        let documentation = plan
            .task_seeds
            .iter()
            .find(|task| task.agent_id == "A9")
            .expect("documentation");
        assert_eq!(
            documentation.dependencies,
            vec![
                TaskDependency {
                    task_id: task_id_for_agent(9, "A0"),
                    kind: TaskDependencyKind::WaveClosure,
                },
                TaskDependency {
                    task_id: task_id_for_agent(10, "A8"),
                    kind: TaskDependencyKind::IntegrationClosure,
                },
            ]
        );

        let qa = plan
            .task_seeds
            .iter()
            .find(|task| task.agent_id == "A0")
            .expect("qa");
        assert_eq!(
            qa.dependencies,
            vec![
                TaskDependency {
                    task_id: task_id_for_agent(9, "A0"),
                    kind: TaskDependencyKind::WaveClosure,
                },
                TaskDependency {
                    task_id: task_id_for_agent(10, "A9"),
                    kind: TaskDependencyKind::DocumentationClosure,
                },
            ]
        );
        assert_eq!(
            qa.required_role_prompts,
            vec!["docs/agents/wave-cont-qa-role.md".to_string()]
        );
    }

    #[test]
    fn declaration_mapping_compiles_multi_agent_contract_into_dependency_edges() {
        let mut a1 = agent(
            "A1",
            "Runtime substrate",
            vec!["role-implementation"],
            vec!["src/runtime_a.rs"],
        );
        a1.writes_artifacts = vec!["runtime-a-state".to_string()];
        a1.parallel_safety = wave_spec::ParallelSafetyClass::ParallelSafe;

        let mut a2 = agent(
            "A2",
            "Operator shell",
            vec!["role-implementation"],
            vec!["src/runtime_b.rs"],
        );
        a2.depends_on_agents = vec!["A1".to_string()];
        a2.reads_artifacts_from = vec!["runtime-a-state".to_string()];
        a2.writes_artifacts = vec!["runtime-b-state".to_string()];
        a2.parallel_safety = wave_spec::ParallelSafetyClass::ParallelSafe;

        let mut e0 = agent(
            "E0",
            "Continuous eval",
            vec!["role-cont-eval"],
            vec![".wave/eval/wave-18.md"],
        );
        e0.reads_artifacts_from = vec!["runtime-a-state".to_string()];

        let mut a8 = agent(
            "A8",
            "Integration",
            vec!["role-integration"],
            vec![".wave/integration/wave-18.md"],
        );
        a8.barrier_class = wave_spec::BarrierClass::IntegrationBarrier;

        let wave = WaveDocument {
            path: PathBuf::from("waves/18.md"),
            metadata: WaveMetadata {
                id: 18,
                slug: "wave-18".to_string(),
                title: "Wave 18".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["runtime".to_string()],
                depends_on: Vec::new(),
                validation: Vec::new(),
                rollback: Vec::new(),
                proof: Vec::new(),
                wave_class: wave_spec::WaveClass::Implementation,
                intent: None,
                delivery: None,
                design_gate: None,
                execution_model: wave_spec::WaveExecutionModel::MultiAgent,
                concurrency_budget: wave_spec::WaveConcurrencyBudget::default(),
            },
            heading_title: Some("Wave 18 - MAS".to_string()),
            commit_message: Some("Feat: MAS".to_string()),
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![a1, a2, e0, a8],
        };

        let plan = declaration_task_seeds(&wave);
        let a2 = plan
            .iter()
            .find(|task| task.agent_id == "A2")
            .expect("A2 task seed");
        assert!(a2.dependencies.iter().any(|dependency| {
            dependency.task_id == task_id_for_agent(18, "A1")
                && dependency.kind == TaskDependencyKind::AgentGraph
        }));
        assert!(a2.dependencies.iter().any(|dependency| {
            dependency.task_id == task_id_for_agent(18, "A1")
                && dependency.kind == TaskDependencyKind::ArtifactFlow
        }));
        assert_eq!(
            a2.reads_artifacts_from,
            vec![ArtifactDependency {
                artifact: "runtime-a-state".to_string(),
                source_agent_id: Some("A1".to_string()),
            }]
        );

        let a8 = plan
            .iter()
            .find(|task| task.agent_id == "A8")
            .expect("A8 task seed");
        assert!(a8.dependencies.iter().any(|dependency| {
            dependency.task_id == task_id_for_agent(18, "A1")
                && dependency.kind == TaskDependencyKind::Barrier
        }));
        assert!(a8.dependencies.iter().any(|dependency| {
            dependency.task_id == task_id_for_agent(18, "A2")
                && dependency.kind == TaskDependencyKind::Barrier
        }));
        assert!(a8.dependencies.iter().any(|dependency| {
            dependency.task_id == task_id_for_agent(18, "E0")
                && dependency.kind == TaskDependencyKind::Barrier
        }));
    }

    #[test]
    fn task_seed_captures_executor_contract_and_context() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/10-authority-core.md"),
            metadata: WaveMetadata {
                id: 10,
                slug: "authority-core".to_string(),
                title: "Authority core".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["planner".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["trace.json".to_string()],
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 10".to_string()),
            commit_message: Some("Feat: authority core".to_string()),
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![WaveAgent {
                id: "A1".to_string(),
                title: "Authority Domain And Durable Logs".to_string(),
                role_prompts: Vec::new(),
                executor: BTreeMap::from([
                    ("profile".to_string(), "implement-codex".to_string()),
                    ("model".to_string(), "gpt-5.4".to_string()),
                    ("workspace".to_string(), "repo-local".to_string()),
                    (
                        "codex.config".to_string(),
                        "model_reasoning_effort=xhigh".to_string(),
                    ),
                ]),
                context7: Some(wave_spec::Context7Defaults {
                    bundle: "rust-control-plane".to_string(),
                    query: Some("Typed task seeds".to_string()),
                }),
                skills: vec!["wave-core".to_string(), "role-implementation".to_string()],
                components: vec!["authority-core-domain".to_string()],
                capabilities: vec!["typed-task-seeds".to_string()],
                exit_contract: Some(ExitContract {
                    completion: wave_spec::CompletionLevel::Integrated,
                    durability: wave_spec::DurabilityLevel::Durable,
                    proof: wave_spec::ProofLevel::Integration,
                    doc_impact: wave_spec::DocImpact::Owned,
                }),
                deliverables: vec!["crates/wave-domain/src/lib.rs".to_string()],
                file_ownership: vec!["crates/wave-domain/src/lib.rs".to_string()],
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: Vec::new(),
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                exclusive_resources: Vec::new(),
                parallel_with: Vec::new(),
                final_markers: vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                prompt: String::new(),
            }],
        };

        let seed = declaration_task_seeds(&wave).remove(0);
        assert_eq!(seed.executor.runtime_id, Some(RuntimeId::Codex));
        assert_eq!(seed.executor.profile.as_deref(), Some("implement-codex"));
        assert_eq!(seed.executor.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(
            seed.executor.params.get("workspace").map(String::as_str),
            Some("repo-local")
        );
        assert!(!seed.executor.params.contains_key("codex.config"));
        assert_eq!(
            seed.context7,
            Some(TaskContext7 {
                bundle: "rust-control-plane".to_string(),
                query: Some("Typed task seeds".to_string()),
            })
        );
        assert_eq!(seed.components, vec!["authority-core-domain".to_string()]);
        assert_eq!(seed.capabilities, vec!["typed-task-seeds".to_string()]);
        assert_eq!(
            seed.exit_contract,
            Some(TaskExitContract {
                completion: TaskCompletionLevel::Integrated,
                durability: TaskDurabilityLevel::Durable,
                proof: TaskProofLevel::Integration,
                doc_impact: TaskDocImpact::Owned,
            })
        );
    }

    #[test]
    fn task_role_uses_closure_and_skill_conventions() {
        assert_eq!(
            task_role(&agent("A8", "Integration", vec![], vec![])),
            TaskRole::Integration
        );
        assert_eq!(
            task_role(&agent("A9", "Docs", vec![], vec![])),
            TaskRole::Documentation
        );
        assert_eq!(
            closure_role(&agent("A0", "QA", vec![], vec![])),
            Some(ClosureRole::ContQa)
        );
        assert_eq!(
            task_role(&agent("E0", "Eval", vec![], vec![])),
            TaskRole::ContEval
        );
        assert_eq!(
            task_role(&agent("A5", "Security", vec!["role-security"], vec![])),
            TaskRole::Security
        );
        assert_eq!(
            task_role(&agent(
                "A2",
                "Implementation",
                vec!["role-implementation"],
                vec![]
            )),
            TaskRole::Implementation
        );
    }

    #[test]
    fn result_disposition_tracks_attempt_state_and_marker_completeness() {
        assert_eq!(
            ResultDisposition::from_attempt_state(AttemptState::Succeeded, 0),
            ResultDisposition::Completed
        );
        assert_eq!(
            ResultDisposition::from_attempt_state(AttemptState::Succeeded, 1),
            ResultDisposition::Partial
        );
        assert_eq!(
            ResultDisposition::from_attempt_state(AttemptState::Running, 0),
            ResultDisposition::Partial
        );
        assert_eq!(
            ResultDisposition::from_attempt_state(AttemptState::Failed, 0),
            ResultDisposition::Failed
        );
        assert_eq!(
            ResultDisposition::from_attempt_state(AttemptState::Aborted, 0),
            ResultDisposition::Aborted
        );
        assert_eq!(
            ResultDisposition::from_attempt_state(AttemptState::Refused, 0),
            ResultDisposition::Refused
        );
    }

    #[test]
    fn scheduler_claim_and_lease_helpers_track_current_state() {
        let owner = SchedulerOwner {
            scheduler_id: "wave-runtime".to_string(),
            scheduler_path: "wave-runtime/codex".to_string(),
            runtime: Some("codex".to_string()),
            executor: Some("codex".to_string()),
            session_id: Some("wave-13-run".to_string()),
            process_id: None,
            process_started_at_ms: None,
        };
        assert_eq!(owner.display_label(), "wave-runtime/codex");
        assert!(WaveClaimState::Held.is_held());
        assert!(!WaveClaimState::Released.is_held());
        assert!(TaskLeaseState::Granted.is_active());
        assert!(!TaskLeaseState::Expired.is_active());
        assert!(TaskLeaseState::Revoked.is_terminal());

        let claim = WaveClaimRecord {
            claim_id: WaveClaimId::new("claim-wave-13"),
            wave_id: 13,
            state: WaveClaimState::Held,
            owner: owner.clone(),
            claimed_at_ms: 10,
            released_at_ms: None,
            detail: Some("launcher claimed wave".to_string()),
        };
        let lease = TaskLeaseRecord {
            lease_id: TaskLeaseId::new("lease-wave-13-a1"),
            wave_id: 13,
            task_id: task_id_for_agent(13, "A1"),
            claim_id: Some(claim.claim_id.clone()),
            state: TaskLeaseState::Granted,
            owner: owner.clone(),
            granted_at_ms: 11,
            heartbeat_at_ms: Some(12),
            expires_at_ms: Some(42),
            finished_at_ms: None,
            detail: Some("task lease granted".to_string()),
        };
        let budget = SchedulerBudgetRecord {
            budget_id: SchedulerBudgetId::new("budget-default"),
            budget: SchedulerBudget {
                max_active_wave_claims: Some(1),
                max_active_task_leases: Some(1),
                reserved_closure_task_leases: Some(1),
                preemption_enabled: true,
            },
            owner,
            updated_at_ms: 9,
            detail: Some("default serial safety budget".to_string()),
        };

        assert_eq!(claim.owner.display_label(), "wave-runtime/codex");
        assert_eq!(lease.expires_at_ms, Some(42));
        assert_eq!(budget.budget.max_active_wave_claims, Some(1));
    }

    #[test]
    fn closure_state_result_envelope_disposition_tracks_attempt_state_and_blockers() {
        let final_markers = FinalMarkerEnvelope::from_contract(
            vec!["[wave-proof]".to_string()],
            vec!["[wave-proof]".to_string()],
        );
        assert_eq!(
            ClosureState::expected_result_envelope_disposition(
                AttemptState::Succeeded,
                &final_markers,
                &[],
            ),
            ClosureDisposition::Ready
        );

        let blocked_reasons =
            vec!["cont-QA report is missing structured closure verdict".to_string()];
        assert_eq!(
            ClosureState::expected_result_envelope_disposition(
                AttemptState::Succeeded,
                &final_markers,
                &blocked_reasons,
            ),
            ClosureDisposition::Blocked
        );

        let closed = ClosureState {
            disposition: ClosureDisposition::Closed,
            required_final_markers: final_markers.required.clone(),
            observed_final_markers: final_markers.observed.clone(),
            blocking_reasons: Vec::new(),
            satisfied_fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            verdict: ClosureVerdictPayload::Documentation(DocumentationClosureVerdict {
                state: Some("closed".to_string()),
                paths: vec!["docs/implementation/rust-wave-0.3-notes.md".to_string()],
                detail: Some("machine readable closure is present".to_string()),
            }),
        };
        assert!(
            closed.matches_result_envelope_disposition(AttemptState::Succeeded, &final_markers)
        );
    }

    #[test]
    fn final_marker_envelope_deduplicates_and_tracks_missing_markers() {
        let final_markers = FinalMarkerEnvelope::from_contract(
            vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-proof]".to_string(),
            ],
            vec![
                "[wave-proof]".to_string(),
                "[wave-proof]".to_string(),
                "[wave-component]".to_string(),
            ],
        );

        assert_eq!(
            final_markers.required,
            vec!["[wave-proof]".to_string(), "[wave-doc-delta]".to_string()]
        );
        assert_eq!(
            final_markers.observed,
            vec!["[wave-proof]".to_string(), "[wave-component]".to_string()]
        );
        assert_eq!(final_markers.missing, vec!["[wave-doc-delta]".to_string()]);
        assert!(!final_markers.is_satisfied());
    }

    #[test]
    fn proof_and_closure_input_payloads_track_machine_readable_content() {
        let doc_delta = DocDeltaEnvelope {
            status: ResultPayloadStatus::Recorded,
            summary: Some("owned docs updated".to_string()),
            paths: vec!["docs/implementation/rust-wave-0.3-notes.md".to_string()],
        };
        let proof = ProofEnvelope {
            status: ResultPayloadStatus::Recorded,
            summary: Some("cargo test -p wave-results".to_string()),
            proof_bundle_ids: vec![ProofBundleId::new("proof-12-a1")],
            fact_ids: vec![FactId::new("fact-12-a1")],
            contradiction_ids: Vec::new(),
            artifacts: vec![ProofArtifact {
                path: "artifacts/proof.log".to_string(),
                kind: ArtifactKind::TestLog,
                digest: Some("abc123".to_string()),
                note: None,
            }],
        };
        assert!(proof.has_recorded_payload());
        assert!(doc_delta.has_recorded_payload());

        let closure_input = ClosureInputEnvelope {
            status: ResultPayloadStatus::EvidenceOnly,
            final_markers: FinalMarkerEnvelope::from_contract(
                vec!["[wave-proof]".to_string()],
                vec!["[wave-proof]".to_string()],
            ),
            marker_evidence: vec![MarkerEvidence {
                marker: "[wave-proof]".to_string(),
                line: "[wave-proof]".to_string(),
                source: Some("last-message.txt".to_string()),
            }],
        };
        assert!(closure_input.has_evidence());

        let closure = ClosureState {
            disposition: ClosureDisposition::Ready,
            required_final_markers: vec!["[wave-proof]".to_string()],
            observed_final_markers: vec!["[wave-proof]".to_string()],
            blocking_reasons: Vec::new(),
            satisfied_fact_ids: vec![FactId::new("fact-12-a1")],
            contradiction_ids: Vec::new(),
            verdict: ClosureVerdictPayload::Documentation(DocumentationClosureVerdict {
                state: Some("closed".to_string()),
                paths: vec!["docs/implementation/rust-wave-0.3-notes.md".to_string()],
                detail: None,
            }),
        };
        assert!(closure.verdict.is_present());
        assert!(closure.has_machine_readable_signal());

        let envelope = ResultEnvelope {
            result_envelope_id: ResultEnvelopeId::new("result-12-a9"),
            wave_id: 12,
            task_id: task_id_for_agent(12, "A9"),
            attempt_id: AttemptId::new("attempt-12-a9-1"),
            agent_id: "A9".to_string(),
            task_role: TaskRole::Documentation,
            closure_role: Some(ClosureRole::Documentation),
            source: ResultEnvelopeSource::Structured,
            attempt_state: AttemptState::Succeeded,
            disposition: ResultDisposition::Completed,
            summary: Some("documentation closure recorded".to_string()),
            output_text: None,
            proof,
            doc_delta,
            closure_input,
            closure,
            runtime: None,
            created_at_ms: 12,
        };
        assert!(envelope.is_completed_or_failed());
        assert_eq!(
            envelope.expected_disposition(),
            ResultDisposition::Completed
        );
        assert!(envelope.should_surface_as_latest_relevant());
    }

    #[test]
    fn proof_payload_only_records_machine_readable_artifacts() {
        let generic_runtime_payload = ProofEnvelope {
            status: ResultPayloadStatus::Missing,
            summary: None,
            proof_bundle_ids: Vec::new(),
            fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            artifacts: vec![
                ProofArtifact {
                    path: ".wave/state/build/specs/wave-12/agents/A1/last-message.txt".to_string(),
                    kind: ArtifactKind::Other,
                    digest: None,
                    note: Some("last-message".to_string()),
                },
                ProofArtifact {
                    path: ".wave/traces/runs/wave-12.json".to_string(),
                    kind: ArtifactKind::Trace,
                    digest: None,
                    note: Some("trace".to_string()),
                },
            ],
        };
        assert!(!generic_runtime_payload.has_recorded_payload());

        let machine_readable_payload = ProofEnvelope {
            artifacts: vec![ProofArtifact {
                path: "artifacts/proof.log".to_string(),
                kind: ArtifactKind::TestLog,
                digest: Some("abc123".to_string()),
                note: None,
            }],
            ..generic_runtime_payload
        };
        assert!(machine_readable_payload.has_recorded_payload());
    }

    #[test]
    fn runtime_selection_policy_preserves_requested_runtime_and_fallback_order() {
        let agent = WaveAgent {
            executor: BTreeMap::from([
                ("id".to_string(), "claude".to_string()),
                (
                    "fallbacks".to_string(),
                    "codex claude local codex".to_string(),
                ),
            ]),
            ..agent(
                "A1",
                "Implementation",
                vec!["wave-core"],
                vec!["src/lib.rs"],
            )
        };

        let policy = runtime_selection_policy_for_agent(&agent);

        assert_eq!(policy.requested_runtime, Some(RuntimeId::Claude));
        assert_eq!(
            policy.allowed_runtimes,
            vec![RuntimeId::Claude, RuntimeId::Codex, RuntimeId::Local]
        );
        assert_eq!(
            policy.fallback_runtimes,
            vec![RuntimeId::Codex, RuntimeId::Claude, RuntimeId::Local]
        );
        assert_eq!(policy.selection_source.as_deref(), Some("executor.id"));
    }

    #[test]
    fn runtime_selection_policy_infers_runtime_from_runtime_specific_executor_fields() {
        let agent = WaveAgent {
            executor: BTreeMap::from([
                ("profile".to_string(), "implement-codex".to_string()),
                ("claude.effort".to_string(), "high".to_string()),
            ]),
            ..agent(
                "A1",
                "Implementation",
                vec!["wave-core"],
                vec!["src/lib.rs"],
            )
        };

        let policy = runtime_selection_policy_for_agent(&agent);

        assert_eq!(policy.requested_runtime, Some(RuntimeId::Claude));
        assert_eq!(policy.allowed_runtimes, vec![RuntimeId::Claude]);
        assert_eq!(
            policy.selection_source.as_deref(),
            Some("executor.claude-fields")
        );
    }

    #[test]
    fn runtime_execution_record_normalization_preserves_runtime_choice_and_late_bound_skills() {
        let record = RuntimeExecutionRecord {
            policy: RuntimeSelectionPolicy {
                requested_runtime: Some(RuntimeId::Codex),
                allowed_runtimes: vec![RuntimeId::Codex, RuntimeId::Claude, RuntimeId::Claude],
                fallback_runtimes: vec![RuntimeId::Claude, RuntimeId::Claude],
                selection_source: Some("executor.id".to_string()),
            },
            selected_runtime: RuntimeId::Claude,
            selection_reason: "selected claude after fallback".to_string(),
            fallback: Some(RuntimeFallbackRecord {
                requested_runtime: RuntimeId::Codex,
                selected_runtime: RuntimeId::Claude,
                reason: "codex reported unavailable".to_string(),
            }),
            execution_identity: RuntimeExecutionIdentity {
                runtime: RuntimeId::Claude,
                adapter: "wave-runtime/claude".to_string(),
                binary: "/tmp/fake-claude".to_string(),
                provider: "anthropic-claude-code".to_string(),
                artifact_paths: BTreeMap::new(),
            },
            skill_projection: RuntimeSkillProjection {
                declared_skills: vec!["wave-core".to_string(), "wave-core".to_string()],
                projected_skills: vec!["runtime-claude".to_string()],
                dropped_skills: vec!["wave-core".to_string(), "wave-core".to_string()],
                auto_attached_skills: vec![
                    "runtime-claude".to_string(),
                    "runtime-claude".to_string(),
                ],
            },
        };

        let normalized = record.normalized();

        assert_eq!(
            normalized.policy.allowed_runtimes,
            vec![RuntimeId::Codex, RuntimeId::Claude]
        );
        assert_eq!(normalized.policy.fallback_runtimes, vec![RuntimeId::Claude]);
        assert_eq!(
            normalized.skill_projection.declared_skills,
            vec!["wave-core".to_string()]
        );
        assert_eq!(
            normalized.skill_projection.projected_skills,
            vec!["runtime-claude".to_string()]
        );
        assert_eq!(
            normalized.skill_projection.dropped_skills,
            vec!["wave-core".to_string()]
        );
        assert_eq!(
            normalized.skill_projection.auto_attached_skills,
            vec!["runtime-claude".to_string()]
        );
        assert!(normalized.uses_fallback());
    }

    #[test]
    fn runtime_id_exposes_runtime_skill_overlay_id() {
        assert_eq!(RuntimeId::Codex.skill_id(), "runtime-codex");
        assert_eq!(RuntimeId::Claude.skill_id(), "runtime-claude");
    }

    #[test]
    fn human_input_workflow_kind_prefers_explicit_kind_over_route_text() {
        assert_eq!(
            HumanInputWorkflowKind::OperatorApproval.effective("dependency:wave-15"),
            HumanInputWorkflowKind::OperatorApproval
        );
        assert!(
            !HumanInputWorkflowKind::OperatorApproval.is_dependency_handshake("dependency:wave-15")
        );
    }

    #[test]
    fn human_input_workflow_kind_uses_legacy_route_fallback_when_unspecified() {
        assert_eq!(
            HumanInputWorkflowKind::Unspecified.effective("dependency:wave-15"),
            HumanInputWorkflowKind::DependencyHandshake
        );
        assert_eq!(
            HumanInputWorkflowKind::Unspecified.effective("operator:approve"),
            HumanInputWorkflowKind::OperatorApproval
        );
    }

    #[test]
    fn portfolio_delivery_model_deduplicates_referenced_waves_across_layers() {
        let model = PortfolioDeliveryModel {
            initiatives: vec![PortfolioInitiative {
                initiative_id: InitiativeId::new("initiative-portfolio"),
                slug: "portfolio-release".to_string(),
                title: "Portfolio release".to_string(),
                summary: Some("Aggregate multiple waves into a single delivery view.".to_string()),
                wave_ids: vec![17, 18],
                milestone_ids: vec![MilestoneId::new("milestone-readiness")],
                release_train_id: Some(ReleaseTrainId::new("train-2026-03")),
                outcome_contract_ids: vec![OutcomeContractId::new("contract-rollout")],
            }],
            milestones: vec![PortfolioMilestone {
                milestone_id: MilestoneId::new("milestone-readiness"),
                initiative_id: InitiativeId::new("initiative-portfolio"),
                slug: "readiness".to_string(),
                title: "Readiness".to_string(),
                summary: None,
                wave_ids: vec![18, 19],
            }],
            release_trains: vec![ReleaseTrain {
                release_train_id: ReleaseTrainId::new("train-2026-03"),
                slug: "march-2026".to_string(),
                title: "March 2026".to_string(),
                summary: None,
                wave_ids: vec![19, 20],
                initiative_ids: vec![InitiativeId::new("initiative-portfolio")],
                milestone_ids: vec![MilestoneId::new("milestone-readiness")],
            }],
            outcome_contracts: vec![OutcomeContract {
                outcome_contract_id: OutcomeContractId::new("contract-rollout"),
                slug: "rollout-readiness".to_string(),
                title: "Rollout readiness".to_string(),
                summary: None,
                wave_ids: vec![20, 21],
                initiative_ids: vec![InitiativeId::new("initiative-portfolio")],
                milestone_ids: vec![MilestoneId::new("milestone-readiness")],
                release_train_id: Some(ReleaseTrainId::new("train-2026-03")),
            }],
        };

        assert_eq!(model.referenced_wave_ids(), vec![17, 18, 19, 20, 21]);
        assert!(!model.is_empty());
    }

    fn agent(id: &str, title: &str, skills: Vec<&str>, file_ownership: Vec<&str>) -> WaveAgent {
        WaveAgent {
            id: id.to_string(),
            title: title.to_string(),
            role_prompts: Vec::new(),
            executor: BTreeMap::from([("profile".to_string(), "implement-codex".to_string())]),
            context7: None,
            skills: skills.into_iter().map(ToString::to_string).collect(),
            components: Vec::new(),
            capabilities: Vec::new(),
            exit_contract: None,
            deliverables: file_ownership
                .iter()
                .map(|path| (*path).to_string())
                .collect(),
            file_ownership: file_ownership
                .into_iter()
                .map(ToString::to_string)
                .collect(),
            final_markers: match id {
                "A0" => vec!["[wave-gate]".to_string()],
                "A8" => vec!["[wave-integration]".to_string()],
                "A9" => vec!["[wave-doc-closure]".to_string()],
                "E0" => vec!["[wave-eval]".to_string()],
                _ => vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
            },
            depends_on_agents: Vec::new(),
            reads_artifacts_from: Vec::new(),
            writes_artifacts: Vec::new(),
            barrier_class: wave_spec::BarrierClass::Independent,
            parallel_safety: wave_spec::ParallelSafetyClass::Derived,
            exclusive_resources: Vec::new(),
            parallel_with: Vec::new(),
            prompt: String::new(),
        }
    }
}
