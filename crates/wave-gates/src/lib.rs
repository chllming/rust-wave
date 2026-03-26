//! Typed gate and closure helpers for compatibility-backed planning inputs.

use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use wave_domain::ClosureDisposition;
use wave_domain::ClosureState;
use wave_domain::GateDisposition;
use wave_domain::GateId;
use wave_domain::GateVerdict;
use wave_spec::WaveDocument;
use wave_trace::WaveRunRecord;
use wave_trace::WaveRunStatus;

pub const REQUIRED_CLOSURE_AGENT_IDS: [&str; 3] = ["A0", "A8", "A9"];
const OPTIONAL_CLOSURE_AGENT_IDS: [&str; 3] = ["E0", "A6", "A7"];
pub type PlanningGateVerdict = GateVerdict;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompatibilityAgentRunInput {
    pub agent_id: String,
    pub status: WaveRunStatus,
    pub expected_final_markers: Vec<String>,
    pub observed_final_markers: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompatibilityRunInput {
    pub run_id: String,
    pub wave_id: u32,
    pub status: WaveRunStatus,
    pub created_at_ms: u128,
    pub started_at_ms: Option<u128>,
    pub completed_at_ms: Option<u128>,
    pub completed_successfully: bool,
    #[serde(default)]
    pub agents: Vec<CompatibilityAgentRunInput>,
}

impl CompatibilityRunInput {
    pub fn agent(&self, agent_id: &str) -> Option<&CompatibilityAgentRunInput> {
        self.agents.iter().find(|agent| agent.agent_id == agent_id)
    }

    pub fn is_active(&self) -> bool {
        matches!(self.status, WaveRunStatus::Running | WaveRunStatus::Planned)
    }

    pub fn is_authoritative_completion(&self) -> bool {
        self.completed_successfully
    }

    pub fn satisfies_dependency_gate(&self) -> bool {
        self.is_authoritative_completion() || matches!(self.status, WaveRunStatus::DryRun)
    }

    pub fn supports_closure_completion(&self) -> bool {
        self.is_authoritative_completion() || matches!(self.status, WaveRunStatus::DryRun)
    }
}

impl From<&WaveRunRecord> for CompatibilityRunInput {
    fn from(record: &WaveRunRecord) -> Self {
        Self {
            run_id: record.run_id.clone(),
            wave_id: record.wave_id,
            status: record.status,
            created_at_ms: record.created_at_ms,
            started_at_ms: record.started_at_ms,
            completed_at_ms: record.completed_at_ms,
            completed_successfully: record.completed_successfully(),
            agents: record
                .agents
                .iter()
                .map(|agent| compatibility_agent_run_input(record, agent))
                .collect(),
        }
    }
}

fn compatibility_agent_run_input(
    record: &WaveRunRecord,
    agent: &wave_trace::AgentRunRecord,
) -> CompatibilityAgentRunInput {
    match resolve_effective_result_envelope(record, agent) {
        Ok(result) => CompatibilityAgentRunInput {
            agent_id: agent.id.clone(),
            status: attempt_state_status(result.attempt_state, agent.status),
            expected_final_markers: result.required_final_markers,
            observed_final_markers: result.observed_final_markers,
            error: agent.error.clone().or(result.summary),
        },
        Err(_) => CompatibilityAgentRunInput {
            agent_id: agent.id.clone(),
            status: agent.status,
            expected_final_markers: agent.expected_markers.clone(),
            observed_final_markers: agent.observed_markers.clone(),
            error: agent.error.clone(),
        },
    }
}

#[derive(Debug, Clone)]
struct ResolvedResultEnvelope {
    attempt_state: wave_domain::AttemptState,
    required_final_markers: Vec<String>,
    observed_final_markers: Vec<String>,
    summary: Option<String>,
}

fn resolve_effective_result_envelope(
    record: &WaveRunRecord,
    agent: &wave_trace::AgentRunRecord,
) -> Result<ResolvedResultEnvelope> {
    let repo_root = repo_root_from_run_record(record).ok_or_else(|| {
        anyhow::anyhow!(
            "failed to resolve repo root for wave {} run {}",
            record.wave_id,
            record.run_id
        )
    })?;
    let envelope = wave_results::resolve_effective_result_envelope_view(&repo_root, record, agent)?;
    Ok(ResolvedResultEnvelope {
        attempt_state: envelope.attempt_state,
        required_final_markers: envelope.required_final_markers,
        observed_final_markers: envelope.observed_final_markers,
        summary: envelope.summary,
    })
}

fn repo_root_from_run_record(run: &WaveRunRecord) -> Option<PathBuf> {
    repo_root_from_authority_path(&run.bundle_dir)
        .or_else(|| repo_root_from_authority_path(&run.trace_path))
        .or_else(|| {
            run.agents
                .iter()
                .find_map(|agent| repo_root_from_authority_path(&agent.prompt_path))
        })
}

fn repo_root_from_authority_path(path: &Path) -> Option<PathBuf> {
    let anchor = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    for ancestor in anchor.ancestors() {
        if ancestor.file_name().and_then(|name| name.to_str()) == Some(".wave") {
            return ancestor.parent().map(Path::to_path_buf);
        }
    }
    None
}

fn attempt_state_status(
    state: wave_domain::AttemptState,
    fallback: WaveRunStatus,
) -> WaveRunStatus {
    match state {
        wave_domain::AttemptState::Planned => WaveRunStatus::Planned,
        wave_domain::AttemptState::Running => WaveRunStatus::Running,
        wave_domain::AttemptState::Succeeded => WaveRunStatus::Succeeded,
        wave_domain::AttemptState::Failed | wave_domain::AttemptState::Aborted => {
            WaveRunStatus::Failed
        }
        wave_domain::AttemptState::Refused => {
            if matches!(fallback, WaveRunStatus::DryRun) {
                WaveRunStatus::DryRun
            } else {
                WaveRunStatus::DryRun
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompatibilityRunFacts {
    pub latest_run: Option<CompatibilityRunInput>,
    pub actively_running: bool,
    pub completed: bool,
    pub rerun_requested: bool,
    pub closure_override_applied: bool,
    pub gate: GateVerdict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DependencyGateVerdict {
    pub dependency_wave_id: u32,
    pub satisfied: bool,
    pub latest_run: Option<CompatibilityRunInput>,
    pub blocker_token: Option<String>,
    pub gate: GateVerdict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClosureAgentContract {
    pub agent_id: String,
    pub required_final_markers: Vec<String>,
    pub present: bool,
    pub latest_run_status: Option<WaveRunStatus>,
    pub observed_final_markers: Vec<String>,
    pub missing_final_markers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveClosureFacts {
    pub complete: bool,
    pub closed: bool,
    pub agents: Vec<ClosureAgentContract>,
    pub required_agent_ids: Vec<String>,
    pub present_agent_ids: Vec<String>,
    pub missing_agent_ids: Vec<String>,
    pub observed_final_markers: Vec<String>,
    pub missing_final_markers: Vec<String>,
    pub closure: ClosureState,
    pub gate: GateVerdict,
    pub completion_gate: GateVerdict,
}

pub fn compatibility_run_inputs_by_wave(
    records: &HashMap<u32, WaveRunRecord>,
) -> HashMap<u32, CompatibilityRunInput> {
    records
        .iter()
        .map(|(wave_id, record)| (*wave_id, CompatibilityRunInput::from(record)))
        .collect()
}

pub fn compatibility_run_facts(
    wave_id: u32,
    latest_run: Option<&CompatibilityRunInput>,
    rerun_requested: bool,
    closure_override_applied: bool,
) -> CompatibilityRunFacts {
    let latest_run = latest_run.cloned();
    let actively_running = latest_run
        .as_ref()
        .map(CompatibilityRunInput::is_active)
        .unwrap_or(false);
    let authoritative_completion = latest_run
        .as_ref()
        .map(CompatibilityRunInput::is_authoritative_completion)
        .unwrap_or(false);
    let completed =
        closure_override_applied || (authoritative_completion && !rerun_requested);

    let disposition = if actively_running {
        GateDisposition::Blocked
    } else if rerun_requested && authoritative_completion && !closure_override_applied {
        GateDisposition::Blocked
    } else if closure_override_applied {
        GateDisposition::Pass
    } else if let Some(run) = latest_run.as_ref() {
        if run.supports_closure_completion() {
            GateDisposition::Pass
        } else if matches!(run.status, WaveRunStatus::Failed) {
            GateDisposition::Failed
        } else {
            GateDisposition::Blocked
        }
    } else {
        GateDisposition::Blocked
    };

    let blocking_reasons = if actively_running {
        vec![format!(
            "active-run:{}",
            latest_run
                .as_ref()
                .map(|run| run.status)
                .unwrap_or(WaveRunStatus::Running)
        )]
    } else if rerun_requested && authoritative_completion && !closure_override_applied {
        vec!["rerun:requested".to_string()]
    } else if closure_override_applied {
        Vec::new()
    } else if let Some(run) = latest_run.as_ref() {
        if run.supports_closure_completion() {
            Vec::new()
        } else {
            vec![format!("run:{}", run.status)]
        }
    } else {
        vec!["run:pending".to_string()]
    };

    CompatibilityRunFacts {
        latest_run,
        actively_running,
        completed,
        rerun_requested,
        closure_override_applied,
        gate: GateVerdict {
            gate_id: gate_id(wave_id, "compatibility-run"),
            wave_id,
            task_id: None,
            attempt_id: None,
            disposition,
            blocking_reasons,
            satisfied_fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            required_human_input_request_ids: Vec::new(),
        },
    }
}

pub fn planning_gate_verdict(
    wave_id: u32,
    lint_error_count: usize,
    dependency_gates: &[DependencyGateVerdict],
    closure: &WaveClosureFacts,
    run_facts: &CompatibilityRunFacts,
) -> PlanningGateVerdict {
    let mut blocking_reasons = dependency_gates
        .iter()
        .filter_map(|verdict| verdict.blocker_token.clone())
        .collect::<Vec<_>>();
    if lint_error_count > 0 {
        blocking_reasons.push("lint:error".to_string());
    }
    blocking_reasons.extend(closure.gate.blocking_reasons.iter().cloned());
    if run_facts.completed {
        blocking_reasons.push("already-completed".to_string());
    }
    if run_facts.actively_running {
        let status = run_facts
            .latest_run
            .as_ref()
            .map(|run| run.status)
            .unwrap_or(WaveRunStatus::Running);
        blocking_reasons.push(format!("active-run:{status}"));
    }

    let disposition = if blocking_reasons.is_empty() {
        GateDisposition::Pass
    } else if dependency_gates
        .iter()
        .any(|verdict| matches!(verdict.gate.disposition, GateDisposition::Failed))
    {
        GateDisposition::Failed
    } else {
        GateDisposition::Blocked
    };

    GateVerdict {
        gate_id: gate_id(wave_id, "planning-readiness"),
        wave_id,
        task_id: None,
        attempt_id: None,
        disposition,
        blocking_reasons,
        satisfied_fact_ids: Vec::new(),
        contradiction_ids: Vec::new(),
        required_human_input_request_ids: Vec::new(),
    }
}

pub fn dependency_gate_verdict(
    dependency_wave_id: u32,
    latest_run: Option<&CompatibilityRunInput>,
) -> DependencyGateVerdict {
    dependency_gate_verdict_for_wave(dependency_wave_id, dependency_wave_id, latest_run, false)
}

pub fn dependency_gate_verdict_for_wave(
    wave_id: u32,
    dependency_wave_id: u32,
    latest_run: Option<&CompatibilityRunInput>,
    closure_override_applied: bool,
) -> DependencyGateVerdict {
    let latest_run = latest_run.cloned();
    let blocker_token = if closure_override_applied {
        None
    } else {
        match latest_run.as_ref() {
        Some(run) if run.satisfies_dependency_gate() => None,
        Some(run) => Some(format!("wave:{dependency_wave_id}:{}", run.status)),
        None => Some(format!("wave:{dependency_wave_id}:pending")),
        }
    };
    let satisfied = blocker_token.is_none();
    let disposition = if satisfied {
        GateDisposition::Pass
    } else if matches!(
        latest_run.as_ref().map(|run| run.status),
        Some(WaveRunStatus::Failed)
    ) {
        GateDisposition::Failed
    } else {
        GateDisposition::Blocked
    };
    let blocking_reasons = blocker_token.iter().cloned().collect();

    DependencyGateVerdict {
        dependency_wave_id,
        satisfied,
        latest_run,
        blocker_token,
        gate: GateVerdict {
            gate_id: dependency_gate_id(wave_id, dependency_wave_id),
            wave_id,
            task_id: None,
            attempt_id: None,
            disposition,
            blocking_reasons,
            satisfied_fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            required_human_input_request_ids: Vec::new(),
        },
    }
}

pub fn wave_closure_facts(wave: &WaveDocument) -> WaveClosureFacts {
    wave_closure_facts_with_run(wave, None)
}

pub fn wave_closure_facts_with_run(
    wave: &WaveDocument,
    latest_run: Option<&CompatibilityRunInput>,
) -> WaveClosureFacts {
    let declared_optional_agents = OPTIONAL_CLOSURE_AGENT_IDS
        .iter()
        .filter(|agent_id| wave.agents.iter().any(|agent| agent.id == **agent_id))
        .copied()
        .collect::<Vec<_>>();
    let closure_contract_agent_ids = REQUIRED_CLOSURE_AGENT_IDS
        .iter()
        .copied()
        .chain(declared_optional_agents)
        .collect::<Vec<_>>();
    let agents = closure_contract_agent_ids
        .iter()
        .map(|agent_id| {
            let present = wave.agents.iter().any(|agent| agent.id == *agent_id);
            let required_final_markers = authored_or_expected_closure_markers(wave, agent_id);
            let observed_final_markers = latest_run
                .and_then(|run| run.agent(agent_id))
                .map(|agent| {
                    filter_required_markers(&required_final_markers, &agent.observed_final_markers)
                })
                .unwrap_or_default();
            let missing_final_markers = required_final_markers
                .iter()
                .filter(|marker| {
                    !observed_final_markers
                        .iter()
                        .any(|observed| observed == *marker)
                })
                .cloned()
                .collect::<Vec<_>>();

            ClosureAgentContract {
                agent_id: (*agent_id).to_string(),
                required_final_markers,
                present,
                latest_run_status: latest_run
                    .and_then(|run| run.agent(agent_id))
                    .map(|agent| agent.status),
                observed_final_markers,
                missing_final_markers,
            }
        })
        .collect::<Vec<_>>();

    let required_agent_ids = agents
        .iter()
        .map(|contract| contract.agent_id.clone())
        .collect::<Vec<_>>();
    let present_agent_ids = agents
        .iter()
        .filter(|contract| contract.present)
        .map(|contract| contract.agent_id.clone())
        .collect::<Vec<_>>();
    let missing_agent_ids = agents
        .iter()
        .filter(|contract| !contract.present)
        .map(|contract| contract.agent_id.clone())
        .collect::<Vec<_>>();
    let complete = missing_agent_ids.is_empty();
    let observed_final_markers = unique_markers(
        agents
            .iter()
            .flat_map(|contract| contract.observed_final_markers.iter().cloned())
            .collect(),
    );
    let missing_final_markers = unique_markers(
        agents
            .iter()
            .flat_map(|contract| contract.missing_final_markers.iter().cloned())
            .collect(),
    );
    let closed = complete
        && missing_final_markers.is_empty()
        && latest_run
            .map(CompatibilityRunInput::is_authoritative_completion)
            .unwrap_or(false);
    let blocking_reasons = missing_agent_ids
        .iter()
        .map(|agent_id| format!("closure:{agent_id}:missing"))
        .collect::<Vec<_>>();
    let completion_blocking_reasons =
        closure_completion_blocking_reasons(&missing_agent_ids, &missing_final_markers, latest_run);
    let required_final_markers = agents
        .iter()
        .flat_map(|contract| contract.required_final_markers.iter().cloned())
        .collect::<Vec<_>>();
    let closure = ClosureState {
        disposition: if closed {
            ClosureDisposition::Closed
        } else if complete {
            ClosureDisposition::Ready
        } else {
            ClosureDisposition::Blocked
        },
        required_final_markers,
        observed_final_markers: observed_final_markers.clone(),
        blocking_reasons: blocking_reasons.clone(),
        satisfied_fact_ids: Vec::new(),
        contradiction_ids: Vec::new(),
        verdict: wave_domain::ClosureVerdictPayload::None,
    };

    WaveClosureFacts {
        complete,
        closed,
        agents,
        required_agent_ids,
        present_agent_ids,
        missing_agent_ids,
        observed_final_markers,
        missing_final_markers,
        closure,
        gate: GateVerdict {
            gate_id: gate_id(wave.metadata.id, "closure-contract"),
            wave_id: wave.metadata.id,
            task_id: None,
            attempt_id: None,
            disposition: if complete {
                GateDisposition::Pass
            } else {
                GateDisposition::Blocked
            },
            blocking_reasons,
            satisfied_fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            required_human_input_request_ids: Vec::new(),
        },
        completion_gate: GateVerdict {
            gate_id: gate_id(wave.metadata.id, "closure-completion"),
            wave_id: wave.metadata.id,
            task_id: None,
            attempt_id: None,
            disposition: if completion_blocking_reasons.is_empty() {
                GateDisposition::Pass
            } else if matches!(
                latest_run.as_ref().map(|run| run.status),
                Some(WaveRunStatus::Failed)
            ) {
                GateDisposition::Failed
            } else {
                GateDisposition::Blocked
            },
            blocking_reasons: completion_blocking_reasons,
            satisfied_fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            required_human_input_request_ids: Vec::new(),
        },
    }
}

fn gate_id(wave_id: u32, scope: &str) -> GateId {
    GateId::new(format!("wave-{wave_id:02}:{scope}"))
}

fn dependency_gate_id(wave_id: u32, dependency_wave_id: u32) -> GateId {
    GateId::new(format!(
        "wave-{wave_id:02}:dependency-on-{dependency_wave_id:02}"
    ))
}

fn required_closure_markers(agent_id: &str) -> Vec<String> {
    match agent_id {
        "A0" => vec!["[wave-gate]".to_string()],
        "A6" => vec!["[wave-design]".to_string()],
        "A7" => vec!["[wave-security]".to_string()],
        "A8" => vec!["[wave-integration]".to_string()],
        "A9" => vec!["[wave-doc-closure]".to_string()],
        "E0" => vec!["[wave-eval]".to_string()],
        _ => Vec::new(),
    }
}

fn authored_or_expected_closure_markers(wave: &WaveDocument, agent_id: &str) -> Vec<String> {
    wave.agents
        .iter()
        .find(|agent| agent.id == agent_id)
        .map(|agent| {
            if agent.final_markers.is_empty() {
                required_closure_markers(agent_id)
            } else {
                agent.final_markers.clone()
            }
        })
        .unwrap_or_else(|| required_closure_markers(agent_id))
}

fn filter_required_markers(required: &[String], observed: &[String]) -> Vec<String> {
    let mut filtered = Vec::new();
    for marker in observed {
        if required
            .iter()
            .any(|required_marker| required_marker == marker)
            && !filtered.iter().any(|existing| existing == marker)
        {
            filtered.push(marker.clone());
        }
    }
    filtered
}

fn closure_completion_blocking_reasons(
    missing_agent_ids: &[String],
    missing_final_markers: &[String],
    latest_run: Option<&CompatibilityRunInput>,
) -> Vec<String> {
    let mut reasons = missing_agent_ids
        .iter()
        .map(|agent_id| format!("closure:{agent_id}:missing"))
        .collect::<Vec<_>>();

    match latest_run {
        None => reasons.push("closure:run:pending".to_string()),
        Some(run) if run.is_active() => reasons.push(format!("closure:run:{}", run.status)),
        Some(run) if matches!(run.status, WaveRunStatus::Failed) => {
            reasons.push("closure:run:failed".to_string())
        }
        Some(run) if run.supports_closure_completion() => {
            if missing_agent_ids.is_empty() {
                reasons.extend(
                    missing_final_markers
                        .iter()
                        .map(|marker| format!("closure:marker:{marker}:missing")),
                );
            }
        }
        Some(run) => reasons.push(format!("closure:run:{}", run.status)),
    }

    reasons
}

fn unique_markers(markers: Vec<String>) -> Vec<String> {
    let mut unique = Vec::new();
    for marker in markers {
        if !unique.iter().any(|existing| existing == &marker) {
            unique.push(marker);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use wave_config::ExecutionMode;
    use wave_spec::ComponentPromotion;
    use wave_spec::Context7Defaults;
    use wave_spec::DeployEnvironment;
    use wave_spec::WaveAgent;
    use wave_spec::WaveMetadata;

    #[test]
    fn closure_contract_marks_missing_required_agents() {
        let mut wave = test_wave(11);
        wave.agents.retain(|agent| agent.id != "A9");

        let closure = wave_closure_facts(&wave);

        assert!(!closure.complete);
        assert_eq!(
            closure.present_agent_ids,
            vec!["A0".to_string(), "A8".to_string()]
        );
        assert_eq!(closure.missing_agent_ids, vec!["A9".to_string()]);
        assert!(!closure.closed);
        assert_eq!(
            closure.missing_final_markers,
            vec![
                "[wave-gate]".to_string(),
                "[wave-integration]".to_string(),
                "[wave-doc-closure]".to_string()
            ]
        );
        assert_eq!(closure.closure.disposition, ClosureDisposition::Blocked);
        assert_eq!(
            closure.gate.blocking_reasons,
            vec!["closure:A9:missing".to_string()]
        );
        assert_eq!(
            closure.completion_gate.disposition,
            GateDisposition::Blocked
        );
        assert_eq!(
            closure.completion_gate.blocking_reasons,
            vec![
                "closure:A9:missing".to_string(),
                "closure:run:pending".to_string()
            ]
        );
        assert_eq!(
            closure.closure.required_final_markers,
            vec![
                "[wave-gate]".to_string(),
                "[wave-integration]".to_string(),
                "[wave-doc-closure]".to_string()
            ]
        );
    }

    #[test]
    fn closure_facts_capture_observed_markers_from_compatibility_run() {
        let wave = test_wave(11);
        let closure = wave_closure_facts_with_run(
            &wave,
            Some(&run_input_with_agents(
                11,
                WaveRunStatus::Succeeded,
                vec![
                    agent_run("A0", WaveRunStatus::Succeeded, &["[wave-gate]"]),
                    agent_run("A8", WaveRunStatus::Succeeded, &["[wave-integration]"]),
                    agent_run("A9", WaveRunStatus::Succeeded, &["[wave-doc-closure]"]),
                ],
            )),
        );

        assert!(closure.complete);
        assert!(closure.closed);
        assert_eq!(closure.closure.disposition, ClosureDisposition::Closed);
        assert_eq!(
            closure.observed_final_markers,
            vec![
                "[wave-gate]".to_string(),
                "[wave-integration]".to_string(),
                "[wave-doc-closure]".to_string()
            ]
        );
        assert!(closure.missing_final_markers.is_empty());
        assert_eq!(
            closure.agents[0].latest_run_status,
            Some(WaveRunStatus::Succeeded)
        );
        assert_eq!(closure.completion_gate.disposition, GateDisposition::Pass);
        assert!(closure.completion_gate.blocking_reasons.is_empty());
    }

    #[test]
    fn closure_completion_gate_blocks_when_successful_run_is_missing_markers() {
        let wave = test_wave(11);
        let closure = wave_closure_facts_with_run(
            &wave,
            Some(&run_input_with_agents(
                11,
                WaveRunStatus::Succeeded,
                vec![
                    agent_run("A0", WaveRunStatus::Succeeded, &["[wave-gate]"]),
                    agent_run("A8", WaveRunStatus::Succeeded, &["[wave-integration]"]),
                    agent_run("A9", WaveRunStatus::Succeeded, &[]),
                ],
            )),
        );

        assert!(closure.complete);
        assert!(!closure.closed);
        assert_eq!(
            closure.completion_gate.disposition,
            GateDisposition::Blocked
        );
        assert_eq!(
            closure.completion_gate.blocking_reasons,
            vec!["closure:marker:[wave-doc-closure]:missing".to_string()]
        );
    }

    #[test]
    fn closure_completion_gate_marks_failed_runs_as_failed() {
        let wave = test_wave(11);
        let closure = wave_closure_facts_with_run(
            &wave,
            Some(&run_input_with_agents(
                11,
                WaveRunStatus::Failed,
                vec![
                    agent_run("A0", WaveRunStatus::Failed, &[]),
                    agent_run("A8", WaveRunStatus::Failed, &[]),
                    agent_run("A9", WaveRunStatus::Failed, &[]),
                ],
            )),
        );

        assert!(closure.complete);
        assert!(!closure.closed);
        assert_eq!(closure.completion_gate.disposition, GateDisposition::Failed);
        assert_eq!(
            closure.completion_gate.blocking_reasons,
            vec!["closure:run:failed".to_string()]
        );
    }

    #[test]
    fn dependency_gate_blocks_until_success() {
        let pending = dependency_gate_verdict(10, None);
        assert!(!pending.satisfied);
        assert_eq!(pending.gate.disposition, GateDisposition::Blocked);
        assert_eq!(pending.blocker_token, Some("wave:10:pending".to_string()));

        let failed = dependency_gate_verdict(10, Some(&run_input(10, WaveRunStatus::Failed)));
        assert!(!failed.satisfied);
        assert_eq!(failed.gate.disposition, GateDisposition::Failed);
        assert_eq!(failed.blocker_token, Some("wave:10:failed".to_string()));

        let succeeded = dependency_gate_verdict(10, Some(&run_input(10, WaveRunStatus::Succeeded)));
        assert!(succeeded.satisfied);
        assert_eq!(succeeded.gate.disposition, GateDisposition::Pass);
        assert_eq!(succeeded.blocker_token, None);
    }

    #[test]
    fn dependency_gate_can_be_scoped_to_the_wave_being_evaluated() {
        let verdict =
            dependency_gate_verdict_for_wave(
                11,
                10,
                Some(&run_input(10, WaveRunStatus::Running)),
                false,
            );

        assert_eq!(verdict.dependency_wave_id, 10);
        assert_eq!(verdict.gate.wave_id, 11);
        assert_eq!(verdict.gate.gate_id.as_str(), "wave-11:dependency-on-10");
        assert_eq!(
            verdict.gate.blocking_reasons,
            vec!["wave:10:running".to_string()]
        );
    }

    #[test]
    fn compatibility_run_facts_treat_rerun_as_reopened_state() {
        let succeeded = run_input(11, WaveRunStatus::Succeeded);
        let reopened = compatibility_run_facts(11, Some(&succeeded), true, false);

        assert!(!reopened.completed);
        assert!(!reopened.actively_running);
        assert!(reopened.rerun_requested);
        assert_eq!(reopened.gate.disposition, GateDisposition::Blocked);
        assert_eq!(
            reopened.gate.blocking_reasons,
            vec!["rerun:requested".to_string()]
        );
    }

    #[test]
    fn compatibility_run_facts_treat_planned_runs_as_active() {
        let planned = compatibility_run_facts(
            11,
            Some(&run_input(11, WaveRunStatus::Planned)),
            false,
            false,
        );

        assert!(planned.actively_running);
        assert!(!planned.completed);
        assert_eq!(planned.gate.disposition, GateDisposition::Blocked);
        assert_eq!(
            planned.gate.blocking_reasons,
            vec!["active-run:planned".to_string()]
        );
    }

    #[test]
    fn compatibility_run_facts_treat_dry_runs_as_non_authoritative_but_non_blocking() {
        let dry_run = compatibility_run_facts(
            11,
            Some(&run_input(11, WaveRunStatus::DryRun)),
            false,
            false,
        );

        assert!(!dry_run.completed);
        assert!(!dry_run.actively_running);
        assert_eq!(dry_run.gate.disposition, GateDisposition::Pass);
        assert!(dry_run.gate.blocking_reasons.is_empty());
    }

    #[test]
    fn compatibility_run_facts_treat_manual_close_override_as_authoritative_completion() {
        let failed = run_input(15, WaveRunStatus::Failed);
        let facts = compatibility_run_facts(15, Some(&failed), false, true);

        assert!(facts.completed);
        assert!(!facts.actively_running);
        assert!(facts.closure_override_applied);
        assert_eq!(facts.latest_run.as_ref().map(|run| run.status), Some(WaveRunStatus::Failed));
        assert_eq!(facts.gate.disposition, GateDisposition::Pass);
        assert!(facts.gate.blocking_reasons.is_empty());
    }

    #[test]
    fn dependency_gate_accepts_manual_close_override_for_failed_dependency() {
        let verdict = dependency_gate_verdict_for_wave(
            16,
            15,
            Some(&run_input(15, WaveRunStatus::Failed)),
            true,
        );

        assert!(verdict.satisfied);
        assert_eq!(verdict.gate.disposition, GateDisposition::Pass);
        assert!(verdict.blocker_token.is_none());
    }

    #[test]
    fn planning_gate_preserves_queue_blocker_order_and_failure_semantics() {
        let dependency_gates = vec![dependency_gate_verdict_for_wave(
            11,
            10,
            Some(&run_input(10, WaveRunStatus::Failed)),
            false,
        )];
        let mut wave = test_wave(11);
        wave.agents.retain(|agent| agent.id != "A9");
        let closure = wave_closure_facts(&wave);
        let run_facts = compatibility_run_facts(
            11,
            Some(&run_input(11, WaveRunStatus::Running)),
            false,
            false,
        );

        let gate = planning_gate_verdict(11, 1, &dependency_gates, &closure, &run_facts);

        assert_eq!(gate.disposition, GateDisposition::Failed);
        assert_eq!(
            gate.blocking_reasons,
            vec![
                "wave:10:failed".to_string(),
                "lint:error".to_string(),
                "closure:A9:missing".to_string(),
                "active-run:running".to_string()
            ]
        );
    }

    #[test]
    fn dependency_gate_accepts_dry_runs_as_satisfied() {
        let verdict = dependency_gate_verdict(10, Some(&run_input(10, WaveRunStatus::DryRun)));

        assert!(verdict.satisfied);
        assert_eq!(verdict.gate.disposition, GateDisposition::Pass);
        assert_eq!(verdict.blocker_token, None);
    }

    #[test]
    fn closure_completion_accepts_dry_run_markers_without_marking_wave_closed() {
        let wave = test_wave(11);
        let closure = wave_closure_facts_with_run(
            &wave,
            Some(&run_input_with_agents(
                11,
                WaveRunStatus::DryRun,
                vec![
                    agent_run("A0", WaveRunStatus::DryRun, &["[wave-gate]"]),
                    agent_run("A8", WaveRunStatus::DryRun, &["[wave-integration]"]),
                    agent_run("A9", WaveRunStatus::DryRun, &["[wave-doc-closure]"]),
                ],
            )),
        );

        assert!(closure.complete);
        assert!(!closure.closed);
        assert_eq!(closure.closure.disposition, ClosureDisposition::Ready);
        assert_eq!(closure.completion_gate.disposition, GateDisposition::Pass);
        assert!(closure.completion_gate.blocking_reasons.is_empty());
    }

    #[test]
    fn compatibility_run_input_prefers_structured_result_envelope_markers() {
        let root = std::env::temp_dir().join(format!(
            "wave-gates-envelope-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        let envelope_path =
            root.join(".wave/state/results/wave-11/attempt-a0/agent_result_envelope.json");
        fs::create_dir_all(envelope_path.parent().expect("envelope parent")).expect("mkdir");

        wave_trace::write_result_envelope(
            &envelope_path,
            &wave_trace::ResultEnvelopeRecord {
                result_envelope_id: "result:wave-11-1:a0".to_string(),
                wave_id: 11,
                task_id: "wave-11:agent-a0".to_string(),
                attempt_id: "attempt-a0".to_string(),
                agent_id: "A0".to_string(),
                task_role: "cont_qa".to_string(),
                closure_role: Some("cont_qa".to_string()),
                source: wave_trace::ResultEnvelopeSource::Structured,
                attempt_state: wave_trace::AttemptState::Succeeded,
                disposition: wave_trace::ResultDisposition::Completed,
                summary: Some("structured".to_string()),
                output_text: Some("[wave-gate] pass".to_string()),
                final_markers: wave_trace::FinalMarkerEnvelope::from_contract(
                    vec!["[wave-gate]".to_string()],
                    vec!["[wave-gate]".to_string()],
                ),
                proof_bundle_ids: Vec::new(),
                fact_ids: Vec::new(),
                contradiction_ids: Vec::new(),
                artifacts: Vec::new(),
                doc_delta: wave_trace::DocDeltaEnvelope::default(),
                marker_evidence: Vec::new(),
                closure: wave_trace::ClosureState {
                    disposition: wave_trace::ClosureDisposition::Ready,
                    required_final_markers: vec!["[wave-gate]".to_string()],
                    observed_final_markers: vec!["[wave-gate]".to_string()],
                    blocking_reasons: Vec::new(),
                    satisfied_fact_ids: Vec::new(),
                    contradiction_ids: Vec::new(),
                    verdict: wave_trace::ClosureVerdictPayload::None,
                },
                runtime: None,
                created_at_ms: 2,
            },
        )
        .expect("write envelope");

        let input = CompatibilityRunInput::from(&WaveRunRecord {
            run_id: "wave-11-1".to_string(),
            wave_id: 11,
            slug: "wave-11".to_string(),
            title: "Wave 11".to_string(),
            status: WaveRunStatus::Succeeded,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-11-1"),
            trace_path: root.join(".wave/traces/runs/wave-11-1.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(1),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(2),
            agents: vec![wave_trace::AgentRunRecord {
                id: "A0".to_string(),
                title: "Closure".to_string(),
                status: WaveRunStatus::Succeeded,
                prompt_path: root.join(".wave/state/build/specs/prompt.md"),
                last_message_path: root.join(".wave/state/build/specs/last-message.txt"),
                events_path: root.join(".wave/state/build/specs/events.jsonl"),
                stderr_path: root.join(".wave/state/build/specs/stderr.txt"),
                result_envelope_path: Some(envelope_path.clone()),
                runtime_detail_path: None,
                expected_markers: vec!["[wave-gate]".to_string()],
                observed_markers: Vec::new(),
                exit_code: Some(0),
                error: None,
                runtime: None,
            }],
            error: None,
        });

        assert_eq!(
            input.agents[0].observed_final_markers,
            vec!["[wave-gate]".to_string()]
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn compatibility_run_input_uses_wave_results_legacy_adapter_for_owned_closure_artifacts() {
        let root = std::env::temp_dir().join(format!(
            "wave-gates-legacy-adapter-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        let bundle_dir = root.join(".wave/state/build/specs/wave-12-1");
        let agent_dir = bundle_dir.join("agents/A8");
        std::fs::create_dir_all(&agent_dir).expect("agent dir");
        std::fs::create_dir_all(root.join(".wave/integration")).expect("integration dir");
        std::fs::create_dir_all(root.join(".wave/codex")).expect("codex dir");
        std::fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        std::fs::write(agent_dir.join("last-message.txt"), "summary only\n")
            .expect("write last message");
        std::fs::write(
            root.join(".wave/integration/wave-12.md"),
            "# Integration\n\n[wave-integration] state=ready-for-doc-closure claims=3 conflicts=0 blockers=0 detail=owned summary is authoritative\n",
        )
        .expect("write integration summary");

        let mut record = WaveRunRecord {
            run_id: "wave-12-1".to_string(),
            wave_id: 12,
            slug: "wave-12".to_string(),
            title: "Wave 12".to_string(),
            status: WaveRunStatus::Succeeded,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: root.join(".wave/traces/runs/wave-12-1.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(1),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(2),
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
        std::fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        std::fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");

        let wave = test_wave(12);
        let a8 = wave
            .agents
            .iter()
            .find(|agent| agent.id == "A8")
            .expect("A8");
        record.agents[0].expected_markers = a8
            .expected_final_markers()
            .iter()
            .map(|marker| (*marker).to_string())
            .collect();

        let input = CompatibilityRunInput::from(&record);

        assert_eq!(input.agents[0].status, WaveRunStatus::Succeeded);
        assert_eq!(
            input.agents[0].observed_final_markers,
            vec!["[wave-integration]".to_string()]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    fn run_input(wave_id: u32, status: WaveRunStatus) -> CompatibilityRunInput {
        run_input_with_agents(wave_id, status, Vec::new())
    }

    fn run_input_with_agents(
        wave_id: u32,
        status: WaveRunStatus,
        agents: Vec<CompatibilityAgentRunInput>,
    ) -> CompatibilityRunInput {
        CompatibilityRunInput::from(&WaveRunRecord {
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
            completed_at_ms: Some(2),
            agents: agents
                .into_iter()
                .map(|agent| wave_trace::AgentRunRecord {
                    id: agent.agent_id,
                    title: "Agent".to_string(),
                    status: agent.status,
                    prompt_path: PathBuf::from(".wave/state/build/specs/prompt.md"),
                    last_message_path: PathBuf::from(".wave/state/runs/last-message.txt"),
                    events_path: PathBuf::from(".wave/state/runs/events.jsonl"),
                    stderr_path: PathBuf::from(".wave/state/runs/stderr.txt"),
                    result_envelope_path: None,
                    runtime_detail_path: None,
                    expected_markers: agent.expected_final_markers,
                    observed_markers: agent.observed_final_markers,
                    exit_code: Some(0),
                    error: agent.error,
                    runtime: None,
                })
                .collect(),
            error: None,
        })
    }

    fn agent_run(
        agent_id: &str,
        status: WaveRunStatus,
        observed_final_markers: &[&str],
    ) -> CompatibilityAgentRunInput {
        CompatibilityAgentRunInput {
            agent_id: agent_id.to_string(),
            status,
            expected_final_markers: required_closure_markers(agent_id),
            observed_final_markers: observed_final_markers
                .iter()
                .map(|marker| (*marker).to_string())
                .collect(),
            error: None,
        }
    }

    fn test_wave(id: u32) -> WaveDocument {
        WaveDocument {
            path: PathBuf::from(format!("waves/{id:02}.md")),
            metadata: WaveMetadata {
                id,
                slug: format!("wave-{id}"),
                title: format!("Wave {id}"),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
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
            prompt: "Primary goal:\n- Close the wave honestly.".to_string(),
        }
    }
}
