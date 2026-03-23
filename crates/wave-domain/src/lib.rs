use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use wave_spec::WaveAgent;
use wave_spec::WaveDocument;

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
string_id!(ProofBundleId);
string_id!(RerunRequestId);
string_id!(HumanInputRequestId);
string_id!(ResultEnvelopeId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskRole {
    Implementation,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionState {
    Detected,
    Acknowledged,
    RepairInProgress,
    Resolved,
    Waived,
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
#[serde(rename_all = "snake_case")]
pub enum TaskDependencyKind {
    WaveClosure,
    ImplementationSlice,
    ContEvalVerdict,
    IntegrationClosure,
    DocumentationClosure,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskExecutor {
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
        }
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
    pub task_ids: Vec<TaskId>,
    pub fact_ids: Vec<FactId>,
    pub state: ContradictionState,
    pub summary: String,
    pub detail: Option<String>,
    pub introduced_by_event_id: Option<String>,
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
    pub state: RerunState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanInputRequest {
    pub request_id: HumanInputRequestId,
    pub wave_id: u32,
    pub task_id: Option<TaskId>,
    pub state: HumanInputState,
    pub prompt: String,
    pub route: String,
    pub requested_by: String,
    pub answer: Option<String>,
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
    pub attempt_state: AttemptState,
    pub summary: Option<String>,
    pub output_text: Option<String>,
    pub final_markers: Vec<String>,
    pub proof_bundle_ids: Vec<ProofBundleId>,
    pub fact_ids: Vec<FactId>,
    pub contradiction_ids: Vec<ContradictionId>,
    pub artifacts: Vec<ProofArtifact>,
    pub closure: ClosureState,
    pub created_at_ms: u128,
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
    ContradictionUpdated {
        contradiction: ContradictionRecord,
    },
    ProofRecorded {
        proof: ProofBundle,
    },
    RerunRequested {
        rerun: RerunRequest,
    },
    HumanInputUpdated {
        request: HumanInputRequest,
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
        task_seeds,
    }
}

pub fn declaration_task_seeds(wave: &WaveDocument) -> Vec<TaskSeed> {
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

    wave.agents
        .iter()
        .map(|agent| TaskSeed {
            task_id: task_id_for_agent(wave.metadata.id, &agent.id),
            wave_id: wave.metadata.id,
            wave_slug: wave.metadata.slug.clone(),
            wave_title: wave.metadata.title.clone(),
            agent_id: agent.id.clone(),
            agent_title: agent.title.clone(),
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
                agent,
                &wave_dependency_task_ids,
                &implementation_task_ids,
                eval_task_id.as_ref(),
                integration_task_id.as_ref(),
                documentation_task_id.as_ref(),
            ),
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

fn closure_role(agent: &WaveAgent) -> Option<ClosureRole> {
    match agent.id.as_str() {
        "E0" => Some(ClosureRole::ContEval),
        "A8" => Some(ClosureRole::Integration),
        "A9" => Some(ClosureRole::Documentation),
        "A0" => Some(ClosureRole::ContQa),
        _ => None,
    }
}

fn task_role(agent: &WaveAgent) -> TaskRole {
    match agent.id.as_str() {
        "A8" => TaskRole::Integration,
        "A9" => TaskRole::Documentation,
        "A0" => TaskRole::ContQa,
        "E0" => TaskRole::ContEval,
        _ if agent.skills.iter().any(|skill| skill == "role-security") => TaskRole::Security,
        _ if agent.skills.iter().any(|skill| skill == "role-infra") => TaskRole::Infra,
        _ if agent.skills.iter().any(|skill| skill == "role-deploy") => TaskRole::Deploy,
        _ if agent.skills.iter().any(|skill| skill == "role-research") => TaskRole::Research,
        _ => TaskRole::Implementation,
    }
}

fn task_dependencies(
    agent: &WaveAgent,
    wave_dependency_task_ids: &[TaskId],
    implementation_task_ids: &[TaskId],
    eval_task_id: Option<&TaskId>,
    integration_task_id: Option<&TaskId>,
    documentation_task_id: Option<&TaskId>,
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
    dependencies
}

fn task_executor(agent: &WaveAgent) -> TaskExecutor {
    TaskExecutor {
        profile: agent.executor.get("profile").cloned(),
        model: agent.executor.get("model").cloned(),
        params: agent
            .executor
            .iter()
            .filter(|(key, _)| key.as_str() != "profile" && key.as_str() != "model")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    }
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
                final_markers: vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                prompt: String::new(),
            }],
        };

        let seed = declaration_task_seeds(&wave).remove(0);
        assert_eq!(seed.executor.profile.as_deref(), Some("implement-codex"));
        assert_eq!(seed.executor.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(
            seed.executor.params.get("codex.config").map(String::as_str),
            Some("model_reasoning_effort=xhigh")
        );
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
            prompt: String::new(),
        }
    }
}
