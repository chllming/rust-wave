use anyhow::Result;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use wave_config::ProjectConfig;
use wave_domain::AttemptState;
use wave_domain::ControlEventPayload;
use wave_domain::task_id_for_agent;
use wave_events::ControlEventLog;
use wave_events::SchedulerEvent;
use wave_events::SchedulerEventLog;
use wave_gates::CompatibilityAgentRunInput;
use wave_gates::CompatibilityRunInput;
use wave_spec::WaveAgent;
use wave_spec::WaveDocument;
use wave_trace::WaveRunStatus;

#[derive(Debug, Clone)]
struct CanonicalAgentAccumulator {
    agent_id: String,
    expected_final_markers: Vec<String>,
    observed_final_markers: Vec<String>,
    status: WaveRunStatus,
    error: Option<String>,
    last_updated_at_ms: u128,
    started_at_ms: Option<u128>,
    finished_at_ms: Option<u128>,
}

#[derive(Debug, Clone)]
struct CanonicalRunAccumulator {
    run_id: String,
    wave_id: u32,
    created_at_ms: u128,
    started_at_ms: Option<u128>,
    last_updated_at_ms: u128,
    agents: BTreeMap<String, CanonicalAgentAccumulator>,
}

impl CanonicalRunAccumulator {
    fn new(run_id: String, wave_id: u32, created_at_ms: u128) -> Self {
        Self {
            run_id,
            wave_id,
            created_at_ms,
            started_at_ms: None,
            last_updated_at_ms: created_at_ms,
            agents: BTreeMap::new(),
        }
    }

    fn agent_mut(
        &mut self,
        agent_id: &str,
        expected_final_markers: Vec<String>,
        created_at_ms: u128,
    ) -> &mut CanonicalAgentAccumulator {
        self.agents
            .entry(agent_id.to_string())
            .or_insert_with(|| CanonicalAgentAccumulator {
                agent_id: agent_id.to_string(),
                expected_final_markers,
                observed_final_markers: Vec::new(),
                status: WaveRunStatus::Planned,
                error: None,
                last_updated_at_ms: created_at_ms,
                started_at_ms: None,
                finished_at_ms: None,
            })
    }
}

pub fn load_canonical_compatibility_runs(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
) -> Result<HashMap<u32, CompatibilityRunInput>> {
    let resolved_paths = config.resolved_paths(root);
    let control_log = ControlEventLog::new(resolved_paths.authority.state_events_control_dir);
    let mut canonical_runs = HashMap::new();

    for wave in waves {
        let authored_agents = authored_agents_by_id(wave);
        let task_to_agent = authored_agent_ids_by_task(wave);
        let mut runs_by_id = HashMap::<String, CanonicalRunAccumulator>::new();
        let mut events = control_log.load_wave(wave.metadata.id)?;
        events.sort_by_key(|event| (event.created_at_ms, event.event_id.clone()));

        for event in events {
            if matches!(event.payload, ControlEventPayload::None) {
                continue;
            }

            match &event.payload {
                ControlEventPayload::AttemptUpdated { attempt } => {
                    let Some(agent_id) = attempt
                        .task_id
                        .as_str()
                        .pipe(|task_id| task_to_agent.get(task_id))
                        .cloned()
                    else {
                        continue;
                    };
                    let Some(run_id) =
                        correlation_run_id(&event, &attempt.attempt_id, agent_id.as_str())
                    else {
                        continue;
                    };

                    let authored_agent = authored_agents.get(agent_id.as_str());
                    let expected_final_markers =
                        authored_expected_markers(authored_agent, &agent_id);
                    let created_at_ms = attempt.created_at_ms.max(event.created_at_ms);
                    let started_at_ms = attempt.started_at_ms;
                    let finished_at_ms = attempt.finished_at_ms;

                    let run = runs_by_id.entry(run_id.clone()).or_insert_with(|| {
                        CanonicalRunAccumulator::new(run_id, wave.metadata.id, created_at_ms)
                    });
                    run.created_at_ms = run.created_at_ms.min(created_at_ms);
                    run.last_updated_at_ms = run.last_updated_at_ms.max(event.created_at_ms);
                    run.started_at_ms = min_option(run.started_at_ms, started_at_ms);

                    let agent = run.agent_mut(&agent_id, expected_final_markers, created_at_ms);
                    agent.status = wave_status_from_attempt_state(attempt.state);
                    agent.last_updated_at_ms = agent.last_updated_at_ms.max(event.created_at_ms);
                    agent.started_at_ms = min_option(agent.started_at_ms, started_at_ms);
                    agent.finished_at_ms = max_option(agent.finished_at_ms, finished_at_ms);
                    if matches!(attempt.state, AttemptState::Failed | AttemptState::Aborted) {
                        agent.error = attempt.summary.clone();
                    }
                }
                ControlEventPayload::ResultEnvelopeRecorded { result } => {
                    let Some(run_id) =
                        correlation_run_id(&event, &result.attempt_id, result.agent_id.as_str())
                    else {
                        continue;
                    };
                    let authored_agent = authored_agents.get(result.agent_id.as_str());
                    let expected_final_markers =
                        if result.closure_input.final_markers.required.is_empty() {
                            authored_expected_markers(authored_agent, &result.agent_id)
                        } else {
                            result.closure_input.final_markers.required.clone()
                        };
                    let run = runs_by_id.entry(run_id.clone()).or_insert_with(|| {
                        CanonicalRunAccumulator::new(run_id, wave.metadata.id, result.created_at_ms)
                    });
                    run.created_at_ms = run.created_at_ms.min(result.created_at_ms);
                    run.last_updated_at_ms = run.last_updated_at_ms.max(result.created_at_ms);

                    let agent = run.agent_mut(
                        result.agent_id.as_str(),
                        expected_final_markers,
                        result.created_at_ms,
                    );
                    agent.status = wave_status_from_attempt_state(result.attempt_state);
                    agent.observed_final_markers =
                        result.closure_input.final_markers.observed.clone();
                    agent.error = match agent.status {
                        WaveRunStatus::Failed => result
                            .summary
                            .clone()
                            .or_else(|| result.closure.blocking_reasons.first().cloned()),
                        _ => result.summary.clone(),
                    };
                    agent.last_updated_at_ms = agent.last_updated_at_ms.max(result.created_at_ms);
                    if result.attempt_state.is_terminal() {
                        agent.finished_at_ms =
                            max_option(agent.finished_at_ms, Some(result.created_at_ms));
                    }
                }
                _ => {}
            }
        }

        let Some(latest_run) = runs_by_id
            .into_values()
            .map(|run| finalize_run(wave, run))
            .max_by(compare_runs)
        else {
            continue;
        };

        canonical_runs.insert(wave.metadata.id, latest_run);
    }

    Ok(canonical_runs)
}

pub fn load_scheduler_events(root: &Path, config: &ProjectConfig) -> Result<Vec<SchedulerEvent>> {
    let resolved_paths = config.resolved_paths(root);
    SchedulerEventLog::new(resolved_paths.authority.state_events_scheduler_dir).load_all()
}

fn authored_agents_by_id<'a>(wave: &'a WaveDocument) -> HashMap<String, &'a WaveAgent> {
    wave.agents
        .iter()
        .map(|agent| (agent.id.clone(), agent))
        .collect()
}

fn authored_agent_ids_by_task(wave: &WaveDocument) -> HashMap<String, String> {
    wave.agents
        .iter()
        .map(|agent| {
            (
                task_id_for_agent(wave.metadata.id, agent.id.as_str())
                    .as_str()
                    .to_string(),
                agent.id.clone(),
            )
        })
        .collect()
}

fn authored_expected_markers(agent: Option<&&WaveAgent>, agent_id: &str) -> Vec<String> {
    agent
        .map(|agent| {
            agent
                .expected_final_markers()
                .iter()
                .map(|marker| (*marker).to_string())
                .collect()
        })
        .unwrap_or_else(|| default_expected_markers(agent_id))
}

fn default_expected_markers(agent_id: &str) -> Vec<String> {
    match agent_id {
        "A0" => vec!["[wave-gate]".to_string()],
        "A8" => vec!["[wave-integration]".to_string()],
        "A9" => vec!["[wave-doc-closure]".to_string()],
        _ => Vec::new(),
    }
}

fn correlation_run_id(
    event: &wave_events::ControlEvent,
    attempt_id: &wave_domain::AttemptId,
    agent_id: &str,
) -> Option<String> {
    event
        .correlation_id
        .clone()
        .or_else(|| derive_run_id_from_attempt_id(attempt_id.as_str(), agent_id))
}

fn derive_run_id_from_attempt_id(attempt_id: &str, agent_id: &str) -> Option<String> {
    let suffix = format!("-{}", agent_id.to_ascii_lowercase());
    attempt_id
        .strip_suffix(suffix.as_str())
        .map(ToString::to_string)
}

fn finalize_run(wave: &WaveDocument, run: CanonicalRunAccumulator) -> CompatibilityRunInput {
    let status = derive_wave_status(wave, &run.agents);
    let completed_at_ms = match status {
        WaveRunStatus::Running | WaveRunStatus::Planned => None,
        _ => Some(
            run.agents
                .values()
                .filter_map(|agent| agent.finished_at_ms)
                .max()
                .unwrap_or(run.last_updated_at_ms),
        ),
    };

    CompatibilityRunInput {
        run_id: run.run_id,
        wave_id: run.wave_id,
        status,
        created_at_ms: run.created_at_ms,
        started_at_ms: run.started_at_ms,
        completed_at_ms,
        completed_successfully: matches!(status, WaveRunStatus::Succeeded),
        agents: run
            .agents
            .into_values()
            .map(|agent| CompatibilityAgentRunInput {
                agent_id: agent.agent_id,
                status: agent.status,
                expected_final_markers: agent.expected_final_markers,
                observed_final_markers: agent.observed_final_markers,
                error: agent.error,
            })
            .collect(),
    }
}

fn derive_wave_status(
    wave: &WaveDocument,
    agents: &BTreeMap<String, CanonicalAgentAccumulator>,
) -> WaveRunStatus {
    if agents
        .values()
        .any(|agent| matches!(agent.status, WaveRunStatus::Running))
    {
        return WaveRunStatus::Running;
    }
    if agents
        .values()
        .any(|agent| matches!(agent.status, WaveRunStatus::Planned))
    {
        return WaveRunStatus::Planned;
    }
    if agents
        .values()
        .any(|agent| matches!(agent.status, WaveRunStatus::Failed))
    {
        return WaveRunStatus::Failed;
    }

    let all_authored_agents_terminal = wave.agents.iter().all(|authored| {
        agents
            .get(authored.id.as_str())
            .map(|agent| is_terminal_status(agent.status))
            .unwrap_or(false)
    });
    if !all_authored_agents_terminal {
        return WaveRunStatus::Failed;
    }

    if agents
        .values()
        .all(|agent| matches!(agent.status, WaveRunStatus::DryRun))
    {
        WaveRunStatus::DryRun
    } else {
        WaveRunStatus::Succeeded
    }
}

fn is_terminal_status(status: WaveRunStatus) -> bool {
    matches!(
        status,
        WaveRunStatus::Succeeded | WaveRunStatus::Failed | WaveRunStatus::DryRun
    )
}

fn wave_status_from_attempt_state(state: AttemptState) -> WaveRunStatus {
    match state {
        AttemptState::Planned => WaveRunStatus::Planned,
        AttemptState::Running => WaveRunStatus::Running,
        AttemptState::Succeeded => WaveRunStatus::Succeeded,
        AttemptState::Failed | AttemptState::Aborted => WaveRunStatus::Failed,
        AttemptState::Refused => WaveRunStatus::DryRun,
    }
}

fn compare_runs(left: &CompatibilityRunInput, right: &CompatibilityRunInput) -> std::cmp::Ordering {
    (
        relevance_rank(left.status),
        left.created_at_ms,
        left.started_at_ms.unwrap_or_default(),
        left.completed_at_ms.unwrap_or_default(),
    )
        .cmp(&(
            relevance_rank(right.status),
            right.created_at_ms,
            right.started_at_ms.unwrap_or_default(),
            right.completed_at_ms.unwrap_or_default(),
        ))
}

fn relevance_rank(status: WaveRunStatus) -> u8 {
    match status {
        WaveRunStatus::Running | WaveRunStatus::Planned => 3,
        WaveRunStatus::Succeeded | WaveRunStatus::Failed => 2,
        WaveRunStatus::DryRun => 1,
    }
}

fn min_option(left: Option<u128>, right: Option<u128>) -> Option<u128> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn max_option(left: Option<u128>, right: Option<u128>) -> Option<u128> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}
