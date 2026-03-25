//! Codex-first runtime helpers for the Wave workspace.
//!
//! The crate owns file-backed launch, rerun, draft, and replay data plumbing
//! that the CLI and operator surfaces build on. Runtime state stays rooted
//! under the project-scoped paths declared in `wave.toml`, and launched agents
//! persist structured result envelopes through the `wave-results` boundary.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use wave_config::ProjectConfig;
use wave_control_plane::PlanningStatus;
use wave_domain::AttemptId;
use wave_domain::AttemptRecord;
use wave_domain::AttemptState;
use wave_domain::ClosureDisposition;
use wave_domain::ControlEventPayload;
use wave_domain::RerunRequest;
use wave_domain::RerunRequestId;
use wave_domain::RerunState;
use wave_domain::ResultEnvelope;
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
use wave_domain::task_id_for_agent;
use wave_events::ControlEvent;
use wave_events::ControlEventKind;
use wave_events::ControlEventLog;
use wave_events::SchedulerEvent;
use wave_events::SchedulerEventKind;
use wave_events::SchedulerEventLog;
use wave_results::ResultEnvelopeStore;
use wave_results::build_structured_result_envelope;
use wave_results::closure_contract_error as result_closure_contract_error;
use wave_spec::WaveAgent;
use wave_spec::WaveDocument;
use wave_trace::AgentRunRecord;
use wave_trace::CompiledAgentPrompt;
use wave_trace::DraftBundle;
use wave_trace::WaveRunRecord;
use wave_trace::WaveRunStatus;
use wave_trace::load_latest_run_records_by_wave;
use wave_trace::load_run_record;
use wave_trace::now_epoch_ms;
use wave_trace::write_run_record;
use wave_trace::write_trace_bundle;

/// Stable label for the runtime landing zone.
pub const RUNTIME_LANDING_ZONE: &str = "launch-and-replay-bootstrap";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchReport {
    pub run_id: String,
    pub wave_id: u32,
    pub status: WaveRunStatus,
    pub state_path: PathBuf,
    pub trace_path: PathBuf,
    pub bundle_dir: PathBuf,
    pub preflight_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TraceInspectionReport {
    pub wave_id: u32,
    pub run_id: String,
    pub trace_path: PathBuf,
    pub recorded: bool,
    pub replay: wave_trace::ReplayReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DogfoodEvidenceReport {
    pub wave_id: u32,
    pub run_id: String,
    pub trace_path: PathBuf,
    pub recorded: bool,
    pub replay: wave_trace::ReplayReport,
    pub operator_help_required: bool,
    pub help_items: Vec<wave_trace::SelfHostEvidenceItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutonomousWaveSelection {
    pub wave_id: u32,
    pub slug: String,
    pub title: String,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchOptions {
    pub wave_id: Option<u32>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutonomousOptions {
    pub limit: Option<usize>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutonomousQueueDecision {
    pub selected: Option<AutonomousWaveSelection>,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RerunIntentStatus {
    Requested,
    Cleared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RerunIntentRecord {
    #[serde(default)]
    pub request_id: Option<String>,
    pub wave_id: u32,
    pub reason: String,
    pub requested_by: String,
    pub status: RerunIntentStatus,
    pub requested_at_ms: u128,
    pub cleared_at_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchPreflightCheck {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchPreflightDiagnostic {
    pub contract: &'static str,
    pub required: bool,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchRefusal {
    pub wave_id: u32,
    pub wave_slug: String,
    pub detail: String,
    pub failed_contracts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchPreflightReport {
    pub wave_id: u32,
    pub wave_slug: String,
    pub dry_run: bool,
    pub ok: bool,
    pub checks: Vec<LaunchPreflightCheck>,
    pub diagnostics: Vec<LaunchPreflightDiagnostic>,
    pub refusal: Option<LaunchRefusal>,
}

#[derive(Debug)]
pub struct LaunchPreflightError {
    pub report: LaunchPreflightReport,
}

impl fmt::Display for LaunchPreflightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let refusal = self
            .report
            .refusal
            .as_ref()
            .map(|refusal| refusal.detail.as_str())
            .unwrap_or("launch preflight failed");
        write!(f, "{refusal}")
    }
}

impl std::error::Error for LaunchPreflightError {}

impl LaunchPreflightError {
    pub fn report(&self) -> &LaunchPreflightReport {
        &self.report
    }
}

pub fn codex_binary_available() -> bool {
    Command::new("codex")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn load_latest_runs(
    root: &Path,
    config: &ProjectConfig,
) -> Result<HashMap<u32, WaveRunRecord>> {
    load_latest_run_records_by_wave(&state_runs_dir(root, config))
}

pub fn load_relevant_runs(
    root: &Path,
    config: &ProjectConfig,
) -> Result<HashMap<u32, WaveRunRecord>> {
    let runs_dir = state_runs_dir(root, config);
    let mut relevant = HashMap::new();
    if !runs_dir.exists() {
        return Ok(relevant);
    }

    for entry in
        fs::read_dir(&runs_dir).with_context(|| format!("failed to read {}", runs_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", runs_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let record = load_run_record(&path)?;
        match relevant.get(&record.wave_id) {
            Some(current) if !is_more_relevant_run(&record, current) => {}
            _ => {
                relevant.insert(record.wave_id, record);
            }
        }
    }

    Ok(relevant)
}

pub fn list_rerun_intents(
    root: &Path,
    config: &ProjectConfig,
) -> Result<HashMap<u32, RerunIntentRecord>> {
    let reruns_dir = control_reruns_dir(root, config);
    let mut intents = HashMap::new();
    if !reruns_dir.exists() {
        return Ok(intents);
    }

    for entry in fs::read_dir(&reruns_dir)
        .with_context(|| format!("failed to read {}", reruns_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", reruns_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read rerun intent {}", path.display()))?;
        let record = serde_json::from_str::<RerunIntentRecord>(&raw)
            .with_context(|| format!("failed to parse rerun intent {}", path.display()))?;
        intents.insert(record.wave_id, record);
    }

    Ok(intents)
}

pub fn pending_rerun_wave_ids(root: &Path, config: &ProjectConfig) -> Result<HashSet<u32>> {
    Ok(list_rerun_intents(root, config)?
        .into_values()
        .filter(|record| record.status == RerunIntentStatus::Requested)
        .map(|record| record.wave_id)
        .collect())
}

pub fn latest_trace_reports(
    root: &Path,
    config: &ProjectConfig,
) -> Result<HashMap<u32, TraceInspectionReport>> {
    let latest_runs = load_latest_runs(root, config)?;
    let mut reports = HashMap::new();
    for (wave_id, record) in latest_runs {
        reports.insert(wave_id, trace_inspection_report(&record));
    }
    Ok(reports)
}

fn is_more_relevant_run(candidate: &WaveRunRecord, current: &WaveRunRecord) -> bool {
    (
        relevance_rank(candidate.status),
        candidate.created_at_ms,
        candidate.started_at_ms.unwrap_or_default(),
        candidate.completed_at_ms.unwrap_or_default(),
    ) > (
        relevance_rank(current.status),
        current.created_at_ms,
        current.started_at_ms.unwrap_or_default(),
        current.completed_at_ms.unwrap_or_default(),
    )
}

fn relevance_rank(status: WaveRunStatus) -> u8 {
    match status {
        WaveRunStatus::Running | WaveRunStatus::Planned => 3,
        WaveRunStatus::Succeeded | WaveRunStatus::Failed => 2,
        WaveRunStatus::DryRun => 1,
    }
}

pub fn trace_inspection_report(record: &WaveRunRecord) -> TraceInspectionReport {
    let recorded = record.trace_path.exists();
    let replay = wave_trace::validate_replay(record);
    TraceInspectionReport {
        wave_id: record.wave_id,
        run_id: record.run_id.clone(),
        trace_path: record.trace_path.clone(),
        recorded,
        replay,
    }
}

pub fn dogfood_evidence_report(record: &WaveRunRecord) -> DogfoodEvidenceReport {
    let evidence = wave_trace::self_host_evidence(record);
    DogfoodEvidenceReport {
        wave_id: evidence.wave_id,
        run_id: evidence.run_id,
        trace_path: record.trace_path.clone(),
        recorded: evidence.recorded,
        replay: evidence.replay,
        operator_help_required: evidence.operator_help_required,
        help_items: evidence.help_items,
    }
}

pub fn request_rerun(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
    reason: impl Into<String>,
) -> Result<RerunIntentRecord> {
    let requested_at_ms = now_epoch_ms()?;
    let record = RerunIntentRecord {
        request_id: Some(format!("rerun-wave-{wave_id:02}-{requested_at_ms}")),
        wave_id,
        reason: reason.into(),
        requested_by: "operator".to_string(),
        status: RerunIntentStatus::Requested,
        requested_at_ms,
        cleared_at_ms: None,
    };
    write_rerun_intent(root, config, &record)?;
    append_control_event(
        root,
        config,
        ControlEvent::new(
            format!("evt-rerun-requested-{wave_id:02}-{requested_at_ms}"),
            ControlEventKind::RerunRequested,
            wave_id,
        )
        .with_created_at_ms(requested_at_ms)
        .with_correlation_id(format!("rerun-wave-{wave_id:02}"))
        .with_payload(ControlEventPayload::RerunRequested {
            rerun: rerun_request_payload(&record, RerunState::Requested),
        }),
    )?;
    Ok(record)
}

pub fn clear_rerun(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
) -> Result<Option<RerunIntentRecord>> {
    clear_rerun_with_state(root, config, wave_id, RerunState::Cancelled)
}

fn clear_rerun_with_state(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
    rerun_state: RerunState,
) -> Result<Option<RerunIntentRecord>> {
    let mut intents = list_rerun_intents(root, config)?;
    let Some(mut record) = intents.remove(&wave_id) else {
        return Ok(None);
    };
    record.status = RerunIntentStatus::Cleared;
    let cleared_at_ms = now_epoch_ms()?;
    record.cleared_at_ms = Some(cleared_at_ms);
    write_rerun_intent(root, config, &record)?;
    append_control_event(
        root,
        config,
        ControlEvent::new(
            format!("evt-rerun-cleared-{wave_id:02}-{cleared_at_ms}"),
            ControlEventKind::RerunCleared,
            wave_id,
        )
        .with_created_at_ms(cleared_at_ms)
        .with_correlation_id(format!("rerun-wave-{wave_id:02}"))
        .with_payload(ControlEventPayload::RerunRequested {
            rerun: rerun_request_payload(&record, rerun_state),
        }),
    )?;
    Ok(Some(record))
}

pub fn select_wave<'a>(
    waves: &'a [WaveDocument],
    status: &PlanningStatus,
    requested_wave_id: Option<u32>,
) -> Result<&'a WaveDocument> {
    if let Some(wave_id) = requested_wave_id {
        let wave = waves
            .iter()
            .find(|wave| wave.metadata.id == wave_id)
            .with_context(|| format!("unknown wave {}", wave_id))?;
        let entry = status
            .waves
            .iter()
            .find(|entry| entry.id == wave_id)
            .with_context(|| format!("missing status entry for wave {}", wave_id))?;
        if !is_claimable_wave(status, wave_id) {
            bail!(
                "wave {} is not ready: {}",
                wave_id,
                queue_entry_reason(entry)
            );
        }
        return Ok(wave);
    }

    let Some(wave_id) = next_claimable_wave_id(status) else {
        bail!("{}", queue_unavailable_reason(status));
    };
    waves
        .iter()
        .find(|wave| wave.metadata.id == wave_id)
        .with_context(|| format!("missing wave definition for ready wave {}", wave_id))
}

pub fn compile_wave_bundle(
    root: &Path,
    config: &ProjectConfig,
    wave: &WaveDocument,
    run_id: &str,
) -> Result<DraftBundle> {
    bootstrap_authority_roots(root, config)?;
    let bundle_dir = build_specs_dir(root, config).join(run_id);
    let agents_dir = bundle_dir.join("agents");
    fs::create_dir_all(&agents_dir)
        .with_context(|| format!("failed to create {}", agents_dir.display()))?;

    let ordered_agents = ordered_agents(wave);
    let mut agents = Vec::new();
    for agent in &ordered_agents {
        let agent_dir = agents_dir.join(&agent.id);
        fs::create_dir_all(&agent_dir)
            .with_context(|| format!("failed to create {}", agent_dir.display()))?;
        let prompt_path = agent_dir.join("prompt.md");
        let prompt = render_agent_prompt(root, wave, agent, &ordered_agents);
        fs::write(&prompt_path, prompt)
            .with_context(|| format!("failed to write {}", prompt_path.display()))?;
        agents.push(CompiledAgentPrompt {
            id: agent.id.clone(),
            title: agent.title.clone(),
            prompt_path,
            expected_markers: agent
                .expected_final_markers()
                .iter()
                .map(|marker| (*marker).to_string())
                .collect(),
        });
    }

    let bundle = DraftBundle {
        run_id: run_id.to_string(),
        wave_id: wave.metadata.id,
        slug: wave.metadata.slug.clone(),
        title: wave.metadata.title.clone(),
        bundle_dir: bundle_dir.clone(),
        agents,
    };
    let manifest_path = bundle_dir.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_string_pretty(&bundle)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    Ok(bundle)
}

pub fn draft_wave(
    root: &Path,
    waves: &[WaveDocument],
    status: &PlanningStatus,
    wave_id: Option<u32>,
) -> Result<DraftBundle> {
    let config = ProjectConfig::load_from_repo_root(root)?;
    let wave = select_wave(waves, status, wave_id)?;
    let run_id = format!("wave-{:02}-{}", wave.metadata.id, now_epoch_ms()?);
    compile_wave_bundle(root, &config, wave, &run_id)
}

pub fn launch_wave(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    status: &PlanningStatus,
    options: LaunchOptions,
) -> Result<LaunchReport> {
    if !options.dry_run {
        let _ = repair_orphaned_runs(root, config)?;
    }
    let planning_status = if options.dry_run || options.wave_id.is_some() {
        status.clone()
    } else {
        refresh_planning_status(root, config, waves)?
    };
    let wave = select_wave(waves, &planning_status, options.wave_id)?;
    let run_id = format!("wave-{:02}-{}", wave.metadata.id, now_epoch_ms()?);
    let bundle = compile_wave_bundle(root, config, wave, &run_id)?;
    let preflight = build_launch_preflight(wave, options.dry_run);
    let preflight_path = bundle.bundle_dir.join("preflight.json");
    fs::write(&preflight_path, serde_json::to_string_pretty(&preflight)?)
        .with_context(|| format!("failed to write {}", preflight_path.display()))?;
    if !preflight.ok {
        append_control_event(
            root,
            config,
            ControlEvent::new(
                format!(
                    "evt-launch-refused-{}-{}",
                    wave.metadata.id,
                    now_epoch_ms()?
                ),
                ControlEventKind::LaunchRefused,
                wave.metadata.id,
            )
            .with_created_at_ms(now_epoch_ms()?)
            .with_correlation_id(run_id.clone()),
        )?;
        return Err(LaunchPreflightError { report: preflight }.into());
    }

    let trace_path = trace_runs_dir(root, config).join(format!("{run_id}.json"));
    let state_path = state_runs_dir(root, config).join(format!("{run_id}.json"));

    if options.dry_run {
        return Ok(LaunchReport {
            run_id,
            wave_id: wave.metadata.id,
            status: WaveRunStatus::DryRun,
            state_path,
            trace_path,
            bundle_dir: bundle.bundle_dir,
            preflight_path,
        });
    }

    if !codex_binary_available() {
        bail!("codex binary is not available on PATH");
    }
    ensure_default_scheduler_budget(root, config)?;
    let refreshed_status = refresh_planning_status(root, config, waves)?;
    if !is_claimable_wave(&refreshed_status, wave.metadata.id) {
        let detail = refreshed_status
            .waves
            .iter()
            .find(|entry| entry.id == wave.metadata.id)
            .map(queue_entry_reason)
            .unwrap_or_else(|| refreshed_status.queue.queue_ready_reason.clone());
        bail!("wave {} is not claimable: {detail}", wave.metadata.id);
    }

    let codex_home = bootstrap_project_codex_home(root, config)?;
    fs::create_dir_all(trace_runs_dir(root, config)).with_context(|| {
        format!(
            "failed to create {}",
            trace_runs_dir(root, config).display()
        )
    })?;
    fs::create_dir_all(state_runs_dir(root, config)).with_context(|| {
        format!(
            "failed to create {}",
            state_runs_dir(root, config).display()
        )
    })?;

    let created_at_ms = now_epoch_ms()?;
    let claim = acquire_wave_claim(root, config, wave, &run_id, created_at_ms)?;
    let launcher_pid = std::process::id();
    let mut record = WaveRunRecord {
        run_id: run_id.clone(),
        wave_id: wave.metadata.id,
        slug: wave.metadata.slug.clone(),
        title: wave.metadata.title.clone(),
        status: WaveRunStatus::Planned,
        dry_run: options.dry_run,
        bundle_dir: bundle.bundle_dir.clone(),
        trace_path: trace_path.clone(),
        codex_home: codex_home.clone(),
        created_at_ms,
        started_at_ms: None,
        launcher_pid: Some(launcher_pid),
        launcher_started_at_ms: current_process_started_at_ms(),
        completed_at_ms: None,
        agents: bundle
            .agents
            .iter()
            .map(|agent| AgentRunRecord {
                id: agent.id.clone(),
                title: agent.title.clone(),
                status: WaveRunStatus::Planned,
                prompt_path: agent.prompt_path.clone(),
                last_message_path: agent.prompt_path.parent().unwrap().join("last-message.txt"),
                events_path: agent.prompt_path.parent().unwrap().join("events.jsonl"),
                stderr_path: agent.prompt_path.parent().unwrap().join("stderr.txt"),
                result_envelope_path: None,
                expected_markers: agent.expected_markers.clone(),
                observed_markers: Vec::new(),
                exit_code: None,
                error: None,
            })
            .collect(),
        error: None,
    };
    if let Err(error) = write_run_record(&state_path, &record) {
        release_wave_claim(
            root,
            config,
            &claim,
            "launch aborted before run state could be recorded",
        )?;
        return Err(error);
    }
    if let Err(error) =
        clear_rerun_with_state(root, config, wave.metadata.id, RerunState::Completed)
    {
        release_wave_claim(
            root,
            config,
            &claim,
            "launch aborted while clearing rerun intent",
        )?;
        return Err(error);
    }

    let execution_agents = ordered_agents(wave);
    for (index, agent) in execution_agents.iter().enumerate() {
        let lease = match grant_task_lease(root, config, &record, agent, &claim) {
            Ok(lease) => lease,
            Err(error) => {
                return finish_failed_launch(
                    root,
                    config,
                    &bundle,
                    &preflight_path,
                    &state_path,
                    &trace_path,
                    &mut record,
                    agent,
                    index,
                    &claim,
                    None,
                    error,
                );
            }
        };
        append_attempt_event(
            root,
            config,
            &record,
            agent,
            AttemptState::Planned,
            record.created_at_ms,
            None,
        )?;
        record.agents[index].status = WaveRunStatus::Running;
        if record.started_at_ms.is_none() {
            record.status = WaveRunStatus::Running;
            record.started_at_ms = Some(now_epoch_ms()?);
        }
        write_run_record(&state_path, &record)?;
        append_attempt_event(
            root,
            config,
            &record,
            agent,
            AttemptState::Running,
            record.created_at_ms,
            record.started_at_ms,
        )?;
        let prompt =
            match fs::read_to_string(&record.agents[index].prompt_path).with_context(|| {
                format!(
                    "failed to read {}",
                    record.agents[index].prompt_path.display()
                )
            }) {
                Ok(prompt) => prompt,
                Err(error) => {
                    return finish_failed_launch(
                        root,
                        config,
                        &bundle,
                        &preflight_path,
                        &state_path,
                        &trace_path,
                        &mut record,
                        agent,
                        index,
                        &claim,
                        Some(&lease),
                        error,
                    );
                }
            };
        let agent_record = match execute_agent(
            root,
            &record,
            agent,
            &record.agents[index],
            &prompt,
            &codex_home,
        ) {
            Ok(agent_record) => agent_record,
            Err(error) => {
                return finish_failed_launch(
                    root,
                    config,
                    &bundle,
                    &preflight_path,
                    &state_path,
                    &trace_path,
                    &mut record,
                    agent,
                    index,
                    &claim,
                    Some(&lease),
                    error,
                );
            }
        };
        let agent_record =
            match persist_agent_result_envelope(root, config, &record, agent, &agent_record) {
                Ok(agent_record) => agent_record,
                Err(error) => {
                    return finish_failed_launch(
                        root,
                        config,
                        &bundle,
                        &preflight_path,
                        &state_path,
                        &trace_path,
                        &mut record,
                        agent,
                        index,
                        &claim,
                        Some(&lease),
                        error,
                    );
                }
            };
        if agent_record.status == WaveRunStatus::Succeeded {
            if let Err(error) = close_task_lease(
                root,
                config,
                &lease,
                TaskLeaseState::Released,
                format!("agent {} completed", agent.id),
            ) {
                return finish_failed_launch(
                    root,
                    config,
                    &bundle,
                    &preflight_path,
                    &state_path,
                    &trace_path,
                    &mut record,
                    agent,
                    index,
                    &claim,
                    Some(&lease),
                    error,
                );
            }
        }
        record.agents[index] = agent_record.clone();
        append_attempt_event(
            root,
            config,
            &record,
            agent,
            attempt_state_from_agent_status(agent_record.status),
            record.created_at_ms,
            record.started_at_ms,
        )?;
        if agent_record.status == WaveRunStatus::Failed {
            close_task_lease(
                root,
                config,
                &lease,
                TaskLeaseState::Revoked,
                format!("agent {} failed", agent.id),
            )?;
            record.status = WaveRunStatus::Failed;
            record.error = agent_record.error.clone();
            record.completed_at_ms = Some(now_epoch_ms()?);
            write_run_record(&state_path, &record)?;
            write_trace_bundle(&trace_path, &record)?;
            release_wave_claim(root, config, &claim, "wave failed; claim released")?;
            return Ok(LaunchReport {
                run_id,
                wave_id: wave.metadata.id,
                status: record.status,
                state_path,
                trace_path,
                bundle_dir: bundle.bundle_dir,
                preflight_path,
            });
        }
        write_run_record(&state_path, &record)?;
    }

    release_wave_claim(root, config, &claim, "wave completed; claim released")?;
    record.status = WaveRunStatus::Succeeded;
    record.completed_at_ms = Some(now_epoch_ms()?);
    write_run_record(&state_path, &record)?;
    write_trace_bundle(&trace_path, &record)?;

    Ok(LaunchReport {
        run_id,
        wave_id: wave.metadata.id,
        status: record.status,
        state_path,
        trace_path,
        bundle_dir: bundle.bundle_dir,
        preflight_path,
    })
}

pub fn autonomous_launch(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    mut status: PlanningStatus,
    options: AutonomousOptions,
) -> Result<Vec<LaunchReport>> {
    let mut launched = Vec::new();
    if matches!(options.limit, Some(0)) {
        return Ok(launched);
    }
    if !options.dry_run {
        let _ = repair_orphaned_runs(root, config)?;
        status = refresh_planning_status(root, config, waves)?;
    }
    while let Some(selection) = next_claimable_wave_selection(&status) {
        if let Some(limit) = options.limit {
            if launched.len() >= limit {
                break;
            }
        }

        let wave_id = selection.wave_id;
        let report = launch_wave(
            root,
            config,
            waves,
            &status,
            LaunchOptions {
                wave_id: Some(wave_id),
                dry_run: options.dry_run,
            },
        )?;
        let failed = report.status == WaveRunStatus::Failed;
        launched.push(report);

        status = refresh_planning_status(root, config, waves)?;
        if options.dry_run || failed {
            break;
        }
    }
    if launched.is_empty() {
        bail!("{}", queue_unavailable_reason(&status));
    }
    Ok(launched)
}

fn next_claimable_wave_id(status: &PlanningStatus) -> Option<u32> {
    next_claimable_wave_selection(status).map(|selection| selection.wave_id)
}

fn next_claimable_wave_selection(status: &PlanningStatus) -> Option<AutonomousWaveSelection> {
    status
        .queue
        .claimable_wave_ids
        .iter()
        .copied()
        .find_map(|wave_id| {
            let entry = status.waves.iter().find(|entry| entry.id == wave_id)?;
            Some(AutonomousWaveSelection {
                wave_id: entry.id,
                slug: entry.slug.clone(),
                title: entry.title.clone(),
                blocked_by: entry.blocked_by.clone(),
            })
        })
}

fn queue_unavailable_reason(status: &PlanningStatus) -> String {
    if let Some(selection) = next_claimable_wave_selection(status) {
        return format!(
            "wave {} is ready but could not be claimed: {}",
            selection.wave_id,
            queue_entry_reason_from_blockers(&selection.blocked_by)
        );
    }

    let blocked_wave_ids = status
        .waves
        .iter()
        .filter(|entry| !entry.ready)
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    if blocked_wave_ids.is_empty() {
        return status.queue.queue_ready_reason.clone();
    }

    format!(
        "{}; blocked waves: {}",
        status.queue.queue_ready_reason,
        blocked_wave_ids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn refresh_planning_status(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
) -> Result<PlanningStatus> {
    let latest_runs = load_latest_runs(root, config)?;
    let findings = wave_dark_factory::lint_project(root, waves);
    let rerun_wave_ids = pending_rerun_wave_ids(root, config)?;
    wave_control_plane::build_planning_status_from_authority(
        root,
        config,
        waves,
        &findings,
        &[],
        &latest_runs,
        &rerun_wave_ids,
    )
}

fn is_claimable_wave(status: &PlanningStatus, wave_id: u32) -> bool {
    status.queue.claimable_wave_ids.contains(&wave_id)
}

fn queue_entry_reason(entry: &wave_control_plane::WaveQueueEntry) -> String {
    queue_entry_reason_from_blockers(&entry.blocked_by)
}

fn queue_entry_reason_from_blockers(blocked_by: &[String]) -> String {
    if blocked_by.is_empty() {
        "unknown blocker".to_string()
    } else {
        blocked_by.join(", ")
    }
}

pub fn repair_orphaned_runs(root: &Path, config: &ProjectConfig) -> Result<Vec<WaveRunRecord>> {
    let runs_dir = state_runs_dir(root, config);
    if !runs_dir.exists() {
        return Ok(Vec::new());
    }

    let mut repaired = Vec::new();
    for entry in
        fs::read_dir(&runs_dir).with_context(|| format!("failed to read {}", runs_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", runs_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let mut record = load_run_record(&path)?;
        if !reconcile_orphaned_run_record(&mut record)? {
            continue;
        }
        write_run_record(&path, &record)?;
        write_trace_bundle(&record.trace_path, &record)?;
        cleanup_scheduler_ownership_for_run(
            root,
            config,
            &record,
            "launcher orphaned; scheduler ownership revoked",
        )?;
        repaired.push(record);
    }

    Ok(repaired)
}

fn append_control_event(root: &Path, config: &ProjectConfig, event: ControlEvent) -> Result<()> {
    control_event_log(root, config).append(&event)
}

fn control_event_log(root: &Path, config: &ProjectConfig) -> ControlEventLog {
    ControlEventLog::new(
        config
            .resolved_paths(root)
            .authority
            .state_events_control_dir,
    )
}

fn scheduler_event_log(root: &Path, config: &ProjectConfig) -> SchedulerEventLog {
    SchedulerEventLog::new(
        config
            .resolved_paths(root)
            .authority
            .state_events_scheduler_dir,
    )
}

fn runtime_scheduler_owner(session_id: impl Into<String>) -> SchedulerOwner {
    SchedulerOwner {
        scheduler_id: "wave-runtime".to_string(),
        scheduler_path: "wave-runtime/codex".to_string(),
        runtime: Some("codex".to_string()),
        executor: Some("codex".to_string()),
        session_id: Some(session_id.into()),
    }
}

fn ensure_default_scheduler_budget(root: &Path, config: &ProjectConfig) -> Result<()> {
    let log = scheduler_event_log(root, config);
    if log
        .load_all()?
        .iter()
        .any(|event| matches!(event.kind, SchedulerEventKind::SchedulerBudgetUpdated))
    {
        return Ok(());
    }

    let created_at_ms = now_epoch_ms()?;
    let budget = SchedulerBudgetRecord {
        budget_id: SchedulerBudgetId::new("budget-default"),
        budget: SchedulerBudget {
            max_active_wave_claims: Some(1),
            max_active_task_leases: Some(1),
        },
        owner: runtime_scheduler_owner("budget-bootstrap"),
        updated_at_ms: created_at_ms,
        detail: Some("default serial scheduler budget".to_string()),
    };
    log.append(
        &SchedulerEvent::new(
            format!("sched-budget-default-{created_at_ms}"),
            SchedulerEventKind::SchedulerBudgetUpdated,
        )
        .with_created_at_ms(created_at_ms)
        .with_correlation_id("scheduler-budget-default")
        .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated { budget }),
    )
}

fn append_scheduler_event(
    root: &Path,
    config: &ProjectConfig,
    event: SchedulerEvent,
) -> Result<()> {
    scheduler_event_log(root, config).append(&event)
}

fn acquire_wave_claim(
    root: &Path,
    config: &ProjectConfig,
    wave: &WaveDocument,
    run_id: &str,
    created_at_ms: u128,
) -> Result<WaveClaimRecord> {
    let claim = WaveClaimRecord {
        claim_id: WaveClaimId::new(format!("claim-wave-{:02}-{run_id}", wave.metadata.id)),
        wave_id: wave.metadata.id,
        state: WaveClaimState::Held,
        owner: runtime_scheduler_owner(run_id),
        claimed_at_ms: created_at_ms,
        released_at_ms: None,
        detail: Some(format!(
            "wave {} claimed for runtime launch",
            wave.metadata.id
        )),
    };
    append_scheduler_event(
        root,
        config,
        SchedulerEvent::new(
            format!("sched-claim-acquired-{}-{created_at_ms}", wave.metadata.id),
            SchedulerEventKind::WaveClaimAcquired,
        )
        .with_wave_id(wave.metadata.id)
        .with_claim_id(claim.claim_id.clone())
        .with_created_at_ms(created_at_ms)
        .with_correlation_id(run_id.to_string())
        .with_payload(SchedulerEventPayload::WaveClaimUpdated {
            claim: claim.clone(),
        }),
    )?;
    Ok(claim)
}

fn release_wave_claim(
    root: &Path,
    config: &ProjectConfig,
    claim: &WaveClaimRecord,
    detail: impl Into<String>,
) -> Result<()> {
    let released_at_ms = now_epoch_ms()?;
    let mut released = claim.clone();
    released.state = WaveClaimState::Released;
    released.released_at_ms = Some(released_at_ms);
    released.detail = Some(detail.into());
    append_scheduler_event(
        root,
        config,
        SchedulerEvent::new(
            format!("sched-claim-released-{}-{released_at_ms}", claim.wave_id),
            SchedulerEventKind::WaveClaimReleased,
        )
        .with_wave_id(claim.wave_id)
        .with_claim_id(claim.claim_id.clone())
        .with_created_at_ms(released_at_ms)
        .with_correlation_id(
            released
                .owner
                .session_id
                .clone()
                .unwrap_or_else(|| claim.claim_id.as_str().to_string()),
        )
        .with_payload(SchedulerEventPayload::WaveClaimUpdated { claim: released }),
    )
}

fn grant_task_lease(
    root: &Path,
    config: &ProjectConfig,
    run: &WaveRunRecord,
    agent: &WaveAgent,
    claim: &WaveClaimRecord,
) -> Result<TaskLeaseRecord> {
    let granted_at_ms = now_epoch_ms()?;
    let lease = TaskLeaseRecord {
        lease_id: TaskLeaseId::new(format!(
            "lease-wave-{:02}-{}",
            run.wave_id,
            agent.id.to_ascii_lowercase()
        )),
        wave_id: run.wave_id,
        task_id: task_id_for_agent(run.wave_id, agent.id.as_str()),
        claim_id: Some(claim.claim_id.clone()),
        state: TaskLeaseState::Granted,
        owner: runtime_scheduler_owner(run.run_id.clone()),
        granted_at_ms,
        heartbeat_at_ms: Some(granted_at_ms),
        expires_at_ms: None,
        finished_at_ms: None,
        detail: Some(format!("lease granted for agent {}", agent.id)),
    };
    append_scheduler_event(
        root,
        config,
        SchedulerEvent::new(
            format!("sched-lease-granted-{}-{granted_at_ms}", lease.task_id),
            SchedulerEventKind::TaskLeaseGranted,
        )
        .with_wave_id(run.wave_id)
        .with_task_id(lease.task_id.clone())
        .with_claim_id(claim.claim_id.clone())
        .with_lease_id(lease.lease_id.clone())
        .with_created_at_ms(granted_at_ms)
        .with_correlation_id(run.run_id.clone())
        .with_payload(SchedulerEventPayload::TaskLeaseUpdated {
            lease: lease.clone(),
        }),
    )?;
    Ok(lease)
}

fn close_task_lease(
    root: &Path,
    config: &ProjectConfig,
    lease: &TaskLeaseRecord,
    state: TaskLeaseState,
    detail: impl Into<String>,
) -> Result<()> {
    let finished_at_ms = now_epoch_ms()?;
    let mut closed = lease.clone();
    closed.state = state;
    closed.finished_at_ms = Some(finished_at_ms);
    closed.heartbeat_at_ms = Some(finished_at_ms);
    closed.detail = Some(detail.into());
    let kind = match state {
        TaskLeaseState::Granted => SchedulerEventKind::TaskLeaseRenewed,
        TaskLeaseState::Released => SchedulerEventKind::TaskLeaseReleased,
        TaskLeaseState::Expired => SchedulerEventKind::TaskLeaseExpired,
        TaskLeaseState::Revoked => SchedulerEventKind::TaskLeaseRevoked,
    };
    let mut event = SchedulerEvent::new(
        format!(
            "sched-lease-{}-{}-{finished_at_ms}",
            lease_state_label(state),
            lease.task_id
        ),
        kind,
    )
    .with_wave_id(lease.wave_id)
    .with_task_id(lease.task_id.clone())
    .with_lease_id(lease.lease_id.clone())
    .with_created_at_ms(finished_at_ms)
    .with_correlation_id(
        closed
            .owner
            .session_id
            .clone()
            .unwrap_or_else(|| lease.lease_id.as_str().to_string()),
    )
    .with_payload(SchedulerEventPayload::TaskLeaseUpdated {
        lease: closed.clone(),
    });
    if let Some(claim_id) = closed.claim_id.clone() {
        event = event.with_claim_id(claim_id);
    }
    append_scheduler_event(root, config, event)
}

fn lease_state_label(state: TaskLeaseState) -> &'static str {
    match state {
        TaskLeaseState::Granted => "granted",
        TaskLeaseState::Released => "released",
        TaskLeaseState::Expired => "expired",
        TaskLeaseState::Revoked => "revoked",
    }
}

fn rerun_request_payload(record: &RerunIntentRecord, state: RerunState) -> RerunRequest {
    RerunRequest {
        request_id: RerunRequestId::new(record.request_id.clone().unwrap_or_else(|| {
            format!(
                "rerun-wave-{:02}-{}",
                record.wave_id, record.requested_at_ms
            )
        })),
        wave_id: record.wave_id,
        task_ids: Vec::new(),
        requested_attempt_id: None,
        requested_by: record.requested_by.clone(),
        reason: record.reason.clone(),
        state,
    }
}

fn append_attempt_event(
    root: &Path,
    config: &ProjectConfig,
    run: &WaveRunRecord,
    agent: &WaveAgent,
    state: AttemptState,
    created_at_ms: u128,
    started_at_ms: Option<u128>,
) -> Result<()> {
    let task_id = task_id_for_agent(run.wave_id, agent.id.as_str());
    let attempt_id = attempt_id_for_run_agent(run.run_id.as_str(), agent.id.as_str());
    let event_created_at_ms = now_epoch_ms()?;
    let event_kind = match state {
        AttemptState::Planned => ControlEventKind::AttemptPlanned,
        AttemptState::Running => ControlEventKind::AttemptStarted,
        AttemptState::Succeeded
        | AttemptState::Failed
        | AttemptState::Aborted
        | AttemptState::Refused => ControlEventKind::AttemptFinished,
    };
    let attempt = AttemptRecord {
        attempt_id: attempt_id.clone(),
        wave_id: run.wave_id,
        task_id: task_id.clone(),
        attempt_number: 1,
        state,
        executor: resolved_codex_model(agent).unwrap_or_else(|| "codex".to_string()),
        created_at_ms,
        started_at_ms,
        finished_at_ms: state.is_terminal().then_some(event_created_at_ms),
        summary: None,
        proof_bundle_ids: Vec::new(),
        result_envelope_id: None,
    };

    append_control_event(
        root,
        config,
        ControlEvent::new(
            format!(
                "evt-attempt-{}-{}-{}",
                state_label(state),
                run.wave_id,
                event_created_at_ms
            ),
            event_kind,
            run.wave_id,
        )
        .with_task_id(task_id)
        .with_attempt_id(attempt_id)
        .with_created_at_ms(event_created_at_ms)
        .with_correlation_id(run.run_id.clone())
        .with_payload(ControlEventPayload::AttemptUpdated { attempt }),
    )
}

fn attempt_id_for_run_agent(run_id: &str, agent_id: &str) -> AttemptId {
    AttemptId::new(format!("{run_id}-{}", agent_id.to_ascii_lowercase()))
}

fn control_event_for_result_envelope(
    run: &WaveRunRecord,
    agent: &WaveAgent,
    envelope: &ResultEnvelope,
) -> ControlEvent {
    ControlEvent::new(
        format!(
            "evt-result-envelope-{}-{}",
            run.wave_id, envelope.created_at_ms
        ),
        ControlEventKind::ResultEnvelopeRecorded,
        run.wave_id,
    )
    .with_task_id(task_id_for_agent(run.wave_id, agent.id.as_str()))
    .with_attempt_id(envelope.attempt_id.clone())
    .with_created_at_ms(envelope.created_at_ms)
    .with_correlation_id(run.run_id.clone())
    .with_payload(ControlEventPayload::ResultEnvelopeRecorded {
        result: envelope.clone(),
    })
}

fn attempt_state_from_agent_status(status: WaveRunStatus) -> AttemptState {
    match status {
        WaveRunStatus::Planned => AttemptState::Planned,
        WaveRunStatus::Running => AttemptState::Running,
        WaveRunStatus::Succeeded => AttemptState::Succeeded,
        WaveRunStatus::Failed => AttemptState::Failed,
        WaveRunStatus::DryRun => AttemptState::Refused,
    }
}

fn state_label(state: AttemptState) -> &'static str {
    match state {
        AttemptState::Planned => "planned",
        AttemptState::Running => "started",
        AttemptState::Succeeded => "succeeded",
        AttemptState::Failed => "failed",
        AttemptState::Aborted => "aborted",
        AttemptState::Refused => "refused",
    }
}

fn finish_failed_launch(
    root: &Path,
    config: &ProjectConfig,
    bundle: &DraftBundle,
    preflight_path: &Path,
    state_path: &Path,
    trace_path: &Path,
    record: &mut WaveRunRecord,
    agent: &WaveAgent,
    agent_index: usize,
    claim: &WaveClaimRecord,
    lease: Option<&TaskLeaseRecord>,
    error: anyhow::Error,
) -> Result<LaunchReport> {
    let reason = error.to_string();
    if record.started_at_ms.is_none() {
        record.started_at_ms = Some(now_epoch_ms()?);
    }
    record.agents[agent_index].status = WaveRunStatus::Failed;
    record.agents[agent_index].exit_code = None;
    record.agents[agent_index].error = Some(reason.clone());
    record.agents[agent_index].observed_markers.clear();
    ensure_orphan_agent_artifacts(&record.agents[agent_index], &reason)?;

    record.status = WaveRunStatus::Failed;
    record.error = Some(reason.clone());
    record.completed_at_ms = Some(now_epoch_ms()?);
    append_attempt_event(
        root,
        config,
        record,
        agent,
        AttemptState::Failed,
        record.created_at_ms,
        record.started_at_ms,
    )?;
    if let Some(lease) = lease {
        close_task_lease(
            root,
            config,
            lease,
            TaskLeaseState::Revoked,
            format!("lease revoked because launch failed: {reason}"),
        )?;
    }
    release_wave_claim(root, config, claim, format!("launch failed: {reason}"))?;
    write_run_record(state_path, record)?;
    write_trace_bundle(trace_path, record)?;

    Ok(LaunchReport {
        run_id: record.run_id.clone(),
        wave_id: record.wave_id,
        status: record.status,
        state_path: state_path.to_path_buf(),
        trace_path: trace_path.to_path_buf(),
        bundle_dir: bundle.bundle_dir.clone(),
        preflight_path: preflight_path.to_path_buf(),
    })
}

fn reconcile_orphaned_run_record(record: &mut WaveRunRecord) -> Result<bool> {
    let Some(reason) = orphaned_run_reason(record) else {
        return Ok(false);
    };
    mark_orphaned_run_failed(record, &reason)?;
    Ok(true)
}

fn orphaned_run_reason(record: &WaveRunRecord) -> Option<String> {
    if record.dry_run || record.completed_at_ms.is_some() {
        return None;
    }
    if !matches!(
        record.status,
        WaveRunStatus::Planned | WaveRunStatus::Running
    ) {
        return None;
    }

    let launcher_pid = record.launcher_pid?;
    match launcher_liveness(record) {
        LauncherLiveness::Alive => return None,
        LauncherLiveness::Missing => {}
        LauncherLiveness::MismatchedIdentity {
            observed_started_at_ms,
        } => {
            return Some(format!(
                "launcher process {} no longer matches recorded session (observed start={})",
                launcher_pid, observed_started_at_ms
            ));
        }
    }

    Some(format!(
        "launcher process {} exited before run completion was recorded",
        launcher_pid
    ))
}

fn mark_orphaned_run_failed(record: &mut WaveRunRecord, reason: &str) -> Result<()> {
    let completed_at_ms = now_epoch_ms()?;
    record.status = WaveRunStatus::Failed;
    record.error = Some(reason.to_string());
    if record.started_at_ms.is_none() {
        record.started_at_ms = Some(completed_at_ms);
    }
    record.completed_at_ms = Some(completed_at_ms);

    if let Some(agent_index) = record.agents.iter().position(|agent| {
        matches!(
            agent.status,
            WaveRunStatus::Running | WaveRunStatus::Planned
        )
    }) {
        let agent = &mut record.agents[agent_index];
        agent.status = WaveRunStatus::Failed;
        agent.exit_code = None;
        agent.error = Some(reason.to_string());
        agent.observed_markers.clear();
        ensure_orphan_agent_artifacts(agent, reason)?;
    }

    Ok(())
}

fn ensure_orphan_agent_artifacts(agent: &AgentRunRecord, reason: &str) -> Result<()> {
    write_missing_text_artifact(&agent.last_message_path, &format!("{reason}\n"))?;
    write_missing_text_artifact(&agent.events_path, "")?;
    write_missing_text_artifact(&agent.stderr_path, &format!("{reason}\n"))?;
    Ok(())
}

fn cleanup_scheduler_ownership_for_run(
    root: &Path,
    config: &ProjectConfig,
    run: &WaveRunRecord,
    detail: &str,
) -> Result<()> {
    let mut claim = None;
    let mut leases = HashMap::new();
    let mut events = scheduler_event_log(root, config).load_all()?;
    events.sort_by_key(|event| (event.created_at_ms, event.event_id.clone()));

    for event in events {
        match event.payload {
            SchedulerEventPayload::WaveClaimUpdated { claim: record }
                if record.wave_id == run.wave_id
                    && record.owner.session_id.as_deref() == Some(run.run_id.as_str()) =>
            {
                claim = record.state.is_held().then_some(record);
            }
            SchedulerEventPayload::TaskLeaseUpdated { lease }
                if lease.wave_id == run.wave_id
                    && lease.owner.session_id.as_deref() == Some(run.run_id.as_str()) =>
            {
                if lease.state.is_active() {
                    leases.insert(lease.lease_id.clone(), lease);
                } else {
                    leases.remove(&lease.lease_id);
                }
            }
            _ => {}
        }
    }

    for lease in leases.into_values() {
        close_task_lease(
            root,
            config,
            &lease,
            TaskLeaseState::Revoked,
            detail.to_string(),
        )?;
    }
    if let Some(claim) = claim.as_ref() {
        release_wave_claim(root, config, claim, detail.to_string())?;
    }
    Ok(())
}

fn write_missing_text_artifact(path: &Path, contents: &str) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    if path.exists() {
        return Ok(());
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn process_is_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new("/proc").join(pid.to_string()).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LauncherLiveness {
    Alive,
    Missing,
    MismatchedIdentity { observed_started_at_ms: u128 },
}

fn launcher_liveness(record: &WaveRunRecord) -> LauncherLiveness {
    const START_TIME_TOLERANCE_MS: u128 = 1_000;
    let Some(launcher_pid) = record.launcher_pid else {
        return LauncherLiveness::Missing;
    };
    if !process_is_alive(launcher_pid) {
        return LauncherLiveness::Missing;
    }

    let observed_started_at_ms = process_started_at_ms(launcher_pid);
    if let (Some(expected), Some(observed)) =
        (record.launcher_started_at_ms, observed_started_at_ms)
    {
        if expected.abs_diff(observed) <= START_TIME_TOLERANCE_MS {
            return LauncherLiveness::Alive;
        }
        return LauncherLiveness::MismatchedIdentity {
            observed_started_at_ms: observed,
        };
    }

    if let Some(observed) = observed_started_at_ms {
        if observed > record.created_at_ms.saturating_add(1_000) {
            return LauncherLiveness::MismatchedIdentity {
                observed_started_at_ms: observed,
            };
        }
    }

    LauncherLiveness::Alive
}

fn current_process_started_at_ms() -> Option<u128> {
    process_started_at_ms(std::process::id())
}

#[cfg(target_os = "linux")]
fn process_started_at_ms(pid: u32) -> Option<u128> {
    let stat = fs::read_to_string(Path::new("/proc").join(pid.to_string()).join("stat")).ok()?;
    let close_paren = stat.rfind(')')?;
    let remainder = stat.get(close_paren + 2..)?;
    let fields = remainder.split_whitespace().collect::<Vec<_>>();
    let start_ticks = fields.get(19)?.parse::<u128>().ok()?;
    let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks_per_second <= 0 {
        return None;
    }
    let ticks_per_second = u128::try_from(ticks_per_second).ok()?;
    let uptime = fs::read_to_string("/proc/uptime").ok()?;
    let uptime_secs = uptime.split_whitespace().next()?.parse::<f64>().ok()?;
    let uptime_ms = (uptime_secs * 1000.0) as u128;
    let boot_time_ms = now_epoch_ms().ok()?.checked_sub(uptime_ms)?;
    Some(boot_time_ms + (start_ticks * 1000 / ticks_per_second))
}

#[cfg(not(target_os = "linux"))]
fn process_started_at_ms(_pid: u32) -> Option<u128> {
    None
}

fn persist_agent_result_envelope(
    root: &Path,
    config: &ProjectConfig,
    run: &WaveRunRecord,
    declared_agent: &WaveAgent,
    agent_record: &AgentRunRecord,
) -> Result<AgentRunRecord> {
    let envelope =
        build_structured_result_envelope(root, run, declared_agent, agent_record, now_epoch_ms()?)?;
    let envelope_path = ResultEnvelopeStore::under_repo(root).write_envelope(&envelope)?;
    append_control_event(
        root,
        config,
        control_event_for_result_envelope(run, declared_agent, &envelope),
    )?;

    let mut updated = agent_record.clone();
    updated.result_envelope_path = Some(envelope_path);
    Ok(updated)
}

fn execute_agent(
    root: &Path,
    run: &WaveRunRecord,
    agent: &WaveAgent,
    base_record: &AgentRunRecord,
    prompt: &str,
    codex_home: &Path,
) -> Result<AgentRunRecord> {
    let agent_dir = base_record
        .prompt_path
        .parent()
        .context("agent prompt path has no parent directory")?;
    fs::create_dir_all(agent_dir)
        .with_context(|| format!("failed to create {}", agent_dir.display()))?;

    let mut command = Command::new("codex");
    command
        .arg("exec")
        .arg("--json")
        .arg("--skip-git-repo-check")
        .arg("--dangerously-bypass-approvals-and-sandbox")
        .arg("--color")
        .arg("never")
        .arg("-C")
        .arg(root)
        .arg("-o")
        .arg(&base_record.last_message_path);

    if let Some(model) = resolved_codex_model(agent) {
        command.arg("--model").arg(model);
    }
    for entry in resolved_codex_config_entries(agent) {
        command.arg("-c").arg(entry);
    }

    command
        .env("CODEX_HOME", codex_home)
        .env("CODEX_SQLITE_HOME", codex_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().context("failed to start codex exec")?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(prompt.as_bytes())
            .context("failed to write prompt to codex exec stdin")?;
    }
    let output = child
        .wait_with_output()
        .context("failed while waiting for codex exec")?;

    fs::write(&base_record.events_path, &output.stdout)
        .with_context(|| format!("failed to write {}", base_record.events_path.display()))?;
    fs::write(&base_record.stderr_path, &output.stderr)
        .with_context(|| format!("failed to write {}", base_record.stderr_path.display()))?;

    let initial_error = if output.status.success() {
        None
    } else {
        Some(format!(
            "codex exec exited with {}",
            output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string())
        ))
    };
    let provisional_record = AgentRunRecord {
        status: if output.status.success() {
            WaveRunStatus::Succeeded
        } else {
            WaveRunStatus::Failed
        },
        exit_code: output.status.code(),
        error: initial_error.clone(),
        observed_markers: Vec::new(),
        ..base_record.clone()
    };
    let envelope =
        build_structured_result_envelope(root, run, agent, &provisional_record, now_epoch_ms()?)?;
    let observed_markers = envelope.closure_input.final_markers.observed.clone();

    if !output.status.success() {
        return Ok(AgentRunRecord {
            status: WaveRunStatus::Failed,
            exit_code: output.status.code(),
            error: initial_error,
            observed_markers,
            ..base_record.clone()
        });
    }

    if envelope.closure.disposition != ClosureDisposition::Ready {
        return Ok(AgentRunRecord {
            status: WaveRunStatus::Failed,
            exit_code: output.status.code(),
            error: Some(
                envelope
                    .closure
                    .blocking_reasons
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "structured result envelope is not ready".to_string()),
            ),
            observed_markers,
            ..base_record.clone()
        });
    }

    if let Some(error) = result_closure_contract_error(agent.id.as_str(), &envelope.closure) {
        return Ok(AgentRunRecord {
            status: WaveRunStatus::Failed,
            exit_code: output.status.code(),
            error: Some(error),
            observed_markers,
            ..base_record.clone()
        });
    }

    Ok(AgentRunRecord {
        status: WaveRunStatus::Succeeded,
        exit_code: output.status.code(),
        error: None,
        observed_markers,
        ..base_record.clone()
    })
}

fn ordered_agents(wave: &WaveDocument) -> Vec<&WaveAgent> {
    let mut agents = wave.agents.iter().collect::<Vec<_>>();
    agents.sort_by_key(|agent| match agent.id.as_str() {
        "E0" => (1_u8, agent.id.as_str()),
        "A8" => (2_u8, agent.id.as_str()),
        "A9" => (3_u8, agent.id.as_str()),
        "A0" => (4_u8, agent.id.as_str()),
        _ => (0_u8, agent.id.as_str()),
    });
    agents
}

fn render_agent_prompt(
    root: &Path,
    wave: &WaveDocument,
    agent: &WaveAgent,
    ordered_agents: &[&WaveAgent],
) -> String {
    let mut prompt = Vec::new();
    prompt.push(format!(
        "# {}",
        wave.heading_title
            .as_deref()
            .unwrap_or(&wave.metadata.title)
    ));
    prompt.push(String::new());
    if let Some(commit_message) = wave.commit_message.as_deref() {
        prompt.push(format!("Commit message: `{commit_message}`"));
        prompt.push(String::new());
    }
    prompt.push("## Wave context".to_string());
    prompt.push(format!("- wave id: {}", wave.metadata.id));
    prompt.push(format!("- slug: {}", wave.metadata.slug));
    prompt.push(format!("- mode: {}", wave.metadata.mode));
    prompt.push(format!(
        "- component promotions: {}",
        wave.component_promotions
            .iter()
            .map(|promotion| format!("{}={}", promotion.component, promotion.target))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    prompt.push(format!(
        "- deploy environments: {}",
        wave.deploy_environments
            .iter()
            .map(|environment| format!("{}={}", environment.name, environment.detail))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if let Some(context7) = wave.context7_defaults.as_ref() {
        prompt.push(format!("- wave Context7 bundle: {}", context7.bundle));
        if let Some(query) = context7.query.as_deref() {
            prompt.push(format!("- wave Context7 query: {query}"));
        }
    }
    prompt.push(String::new());
    prompt.push("## Current agent".to_string());
    prompt.push(format!("- id: {}", agent.id));
    prompt.push(format!("- title: {}", agent.title));
    if !agent.role_prompts.is_empty() {
        prompt.push(format!("- role prompts: {}", agent.role_prompts.join(", ")));
    }
    if !agent.skills.is_empty() {
        prompt.push(format!("- skills: {}", agent.skills.join(", ")));
    }
    if let Some(context7) = agent.context7.as_ref() {
        prompt.push(format!("- agent Context7 bundle: {}", context7.bundle));
        if let Some(query) = context7.query.as_deref() {
            prompt.push(format!("- agent Context7 query: {query}"));
        }
    }
    if !agent.deliverables.is_empty() {
        prompt.push(format!("- deliverables: {}", agent.deliverables.join(", ")));
    }
    prompt.push(format!(
        "- expected final markers: {}",
        agent.expected_final_markers().join(", ")
    ));
    prompt.push(String::new());
    prompt.push("## Execution order".to_string());
    for (index, candidate) in ordered_agents.iter().enumerate() {
        prompt.push(format!(
            "{}. {}: {}",
            index + 1,
            candidate.id,
            candidate.title
        ));
    }
    prompt.push(String::new());
    prompt.push("## Local references".to_string());
    if !agent.role_prompts.is_empty() {
        for role_prompt in &agent.role_prompts {
            prompt.push(format!(
                "- role prompt path: {}",
                root.join(role_prompt).display()
            ));
        }
    }
    if !agent.skills.is_empty() {
        for skill_id in &agent.skills {
            prompt.push(format!(
                "- skill path: {}",
                root.join("skills")
                    .join(skill_id)
                    .join("SKILL.md")
                    .display()
            ));
        }
    }
    prompt.push(format!("- repo root: {}", root.display()));
    prompt.push(String::new());
    prompt.push("## Assignment".to_string());
    prompt.push(agent.prompt.trim().to_string());
    prompt.push(String::new());
    prompt.push("## Output contract".to_string());
    prompt.push("- Work directly in the repository.".to_string());
    prompt.push("- Respect the owned paths named in the assignment.".to_string());
    prompt.push("- End with the required final markers as plain lines.".to_string());
    prompt.push(
        "- If a required marker cannot be emitted honestly, explain the blocker and stop."
            .to_string(),
    );
    prompt.push(String::new());
    prompt.join("\n")
}

fn resolved_codex_model(agent: &WaveAgent) -> Option<String> {
    env::var("WAVE_CODEX_MODEL_OVERRIDE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| agent.executor.get("model").cloned())
}

fn resolved_codex_config_entries(agent: &WaveAgent) -> Vec<String> {
    env::var("WAVE_CODEX_CONFIG_OVERRIDE")
        .ok()
        .or_else(|| agent.executor.get("codex.config").cloned())
        .map(|raw| parse_codex_config_entries(&raw))
        .unwrap_or_default()
}

fn parse_codex_config_entries(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn build_launch_preflight(wave: &WaveDocument, dry_run: bool) -> LaunchPreflightReport {
    let required_closure_agents = ["A0", "A8", "A9"];
    let checks = vec![
        LaunchPreflightCheck {
            name: "validation-contract",
            ok: !wave.metadata.validation.is_empty(),
            detail: format!("{} validation commands declared", wave.metadata.validation.len()),
        },
        LaunchPreflightCheck {
            name: "rollback-contract",
            ok: !wave.metadata.rollback.is_empty(),
            detail: format!("{} rollback entries declared", wave.metadata.rollback.len()),
        },
        LaunchPreflightCheck {
            name: "proof-contract",
            ok: !wave.metadata.proof.is_empty(),
            detail: format!("{} proof artifacts declared", wave.metadata.proof.len()),
        },
        LaunchPreflightCheck {
            name: "deploy-environments",
            ok: !wave.deploy_environments.is_empty(),
            detail: format!("{} deploy environments declared", wave.deploy_environments.len()),
        },
        LaunchPreflightCheck {
            name: "closure-agents",
            ok: required_closure_agents
                .iter()
                .all(|agent_id| wave.agents.iter().any(|agent| agent.id == *agent_id)),
            detail: format!("required closure agents: {}", required_closure_agents.join(", ")),
        },
        LaunchPreflightCheck {
            name: "implementation-exit-contracts",
            ok: wave.implementation_agents().all(|agent| {
                agent.exit_contract.is_some()
                    && !agent.deliverables.is_empty()
                    && !agent.file_ownership.is_empty()
                    && !agent.final_markers.is_empty()
                    && agent.context7.is_some()
            }),
            detail: "implementation agents must declare exit contract, deliverables, ownership, markers, and Context7"
                .to_string(),
        },
        LaunchPreflightCheck {
            name: "codex-binary",
            ok: dry_run || codex_binary_available(),
            detail: if dry_run {
                "dry run skips Codex binary enforcement".to_string()
            } else {
                "checked `codex --version`".to_string()
            },
        },
    ];
    let diagnostics = checks
        .iter()
        .map(|check| LaunchPreflightDiagnostic {
            contract: check.name,
            required: check.name != "codex-binary" || !dry_run,
            ok: check.ok,
            detail: check.detail.clone(),
        })
        .collect::<Vec<_>>();
    let failed_contracts = diagnostics
        .iter()
        .filter(|diagnostic| !diagnostic.ok && diagnostic.required)
        .map(|diagnostic| diagnostic.contract.to_string())
        .collect::<Vec<_>>();
    let refusal = if failed_contracts.is_empty() {
        None
    } else {
        Some(LaunchRefusal {
            wave_id: wave.metadata.id,
            wave_slug: wave.metadata.slug.clone(),
            detail: format!(
                "launch refused for wave {} ({}): missing required contracts: {}",
                wave.metadata.id,
                wave.metadata.slug,
                failed_contracts.join(", ")
            ),
            failed_contracts,
        })
    };

    LaunchPreflightReport {
        wave_id: wave.metadata.id,
        wave_slug: wave.metadata.slug.clone(),
        dry_run,
        ok: diagnostics.iter().all(|diagnostic| diagnostic.ok),
        checks,
        diagnostics,
        refusal,
    }
}

fn bootstrap_project_codex_home(root: &Path, config: &ProjectConfig) -> Result<PathBuf> {
    let project_codex_home = config.resolved_paths(root).authority.project_codex_home;
    fs::create_dir_all(&project_codex_home)
        .with_context(|| format!("failed to create {}", project_codex_home.display()))?;

    let global_codex_home = global_codex_home();
    for relative in ["auth.json", ".credentials.json", "config.toml"] {
        let source = global_codex_home.join(relative);
        let target = project_codex_home.join(relative);
        if source.exists() && !target.exists() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::copy(&source, &target).with_context(|| {
                format!(
                    "failed to seed project Codex home from {} to {}",
                    source.display(),
                    target.display()
                )
            })?;
        }
    }

    Ok(project_codex_home)
}

fn bootstrap_authority_roots(root: &Path, config: &ProjectConfig) -> Result<()> {
    let authority = config.resolved_paths(root).authority;
    for path in [
        authority.state_dir,
        authority.state_build_specs_dir,
        authority.state_events_dir,
        authority.state_events_control_dir,
        authority.state_events_coordination_dir,
        authority.state_events_scheduler_dir,
        authority.state_results_dir,
        authority.state_derived_dir,
        authority.state_projections_dir,
        authority.state_traces_dir,
    ] {
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
    }
    Ok(())
}

fn global_codex_home() -> PathBuf {
    if let Ok(codex_home) = env::var("CODEX_HOME") {
        return PathBuf::from(codex_home);
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".codex");
    }
    PathBuf::from(".codex")
}

fn build_specs_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    config.resolved_paths(root).authority.state_build_specs_dir
}

fn state_runs_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    config.resolved_paths(root).authority.state_runs_dir
}

fn trace_runs_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    config.resolved_paths(root).authority.trace_runs_dir
}

fn state_control_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    config.resolved_paths(root).authority.state_control_dir
}

fn control_reruns_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    state_control_dir(root, config).join("reruns")
}

fn rerun_intent_path(root: &Path, config: &ProjectConfig, wave_id: u32) -> PathBuf {
    control_reruns_dir(root, config).join(format!("wave-{wave_id:02}.json"))
}

fn write_rerun_intent(
    root: &Path,
    config: &ProjectConfig,
    record: &RerunIntentRecord,
) -> Result<()> {
    let path = rerun_intent_path(root, config, record.wave_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, serde_json::to_string_pretty(record)?)
        .with_context(|| format!("failed to write rerun intent {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::collections::HashSet;
    use wave_config::AuthorityConfig;
    use wave_config::ExecutionMode;
    use wave_control_plane::build_planning_status_with_state;
    use wave_events::SchedulerEventKind;
    use wave_spec::CompletionLevel;
    use wave_spec::Context7Defaults;
    use wave_spec::DeployEnvironment;
    use wave_spec::DocImpact;
    use wave_spec::DurabilityLevel;
    use wave_spec::ExitContract;
    use wave_spec::ProofLevel;
    use wave_spec::WaveMetadata;

    #[test]
    fn closure_agents_run_after_implementation_agents() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/00.md"),
            metadata: WaveMetadata {
                id: 0,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof".to_string()],
            },
            heading_title: Some("Wave 0".to_string()),
            commit_message: Some("Feat: test".to_string()),
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![
                test_agent("A0"),
                test_agent("A8"),
                test_agent("A9"),
                test_agent("A2"),
                test_agent("A1"),
            ],
        };

        let ordered = ordered_agents(&wave);
        assert_eq!(
            ordered
                .iter()
                .map(|agent| agent.id.as_str())
                .collect::<Vec<_>>(),
            vec!["A1", "A2", "A8", "A9", "A0"]
        );
    }

    #[test]
    fn persist_agent_result_envelope_uses_owned_closure_artifacts() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-owned-closure-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".wave/integration")).expect("create integration dir");
        fs::create_dir_all(root.join(".wave/codex")).expect("create codex dir");
        let bundle_dir = root.join(".wave/state/build/specs/wave-12-1");
        let agent_dir = bundle_dir.join("agents/A8");
        fs::create_dir_all(&agent_dir).expect("create agent dir");
        fs::write(
            root.join(".wave/integration/wave-12.md"),
            "# Integration\n\n[wave-integration] state=ready-for-doc-closure claims=2 conflicts=0 blockers=0 detail=owned summary is authoritative\n",
        )
        .expect("write integration summary");
        fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        fs::write(agent_dir.join("last-message.txt"), "summary only\n").expect("write message");
        fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        let run = WaveRunRecord {
            run_id: "wave-12-1".to_string(),
            wave_id: 12,
            slug: "result-envelope-proof-lifecycle".to_string(),
            title: "Result Envelope Proof Lifecycle".to_string(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: root.join(".wave/traces/runs/wave-12-1.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        };
        let mut declared_agent = test_agent("A8");
        declared_agent.file_ownership = vec![".wave/integration/wave-12.md".to_string()];
        declared_agent.final_markers = vec!["[wave-integration]".to_string()];
        let agent_record = AgentRunRecord {
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
        };

        let updated =
            persist_agent_result_envelope(&root, &config, &run, &declared_agent, &agent_record)
                .expect("persist envelope");
        let envelope_path = updated
            .result_envelope_path
            .clone()
            .expect("result envelope path");
        let envelope = ResultEnvelopeStore::under_repo(&root)
            .load_envelope(&envelope_path)
            .expect("load envelope");

        assert_eq!(
            envelope.closure_input.final_markers.observed,
            vec!["[wave-integration]".to_string()]
        );
        assert_eq!(
            envelope.closure.disposition,
            wave_domain::ClosureDisposition::Ready
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn persist_agent_result_envelope_writes_canonical_result_path() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-envelope-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        let bundle_dir = root.join(".wave/state/build/specs/wave-12-1");
        let agent_dir = bundle_dir.join("agents/A2");
        fs::create_dir_all(&agent_dir).expect("create agent dir");
        fs::create_dir_all(root.join(".wave/traces/runs")).expect("create trace dir");
        fs::create_dir_all(root.join(".wave/codex")).expect("create codex dir");
        fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        fs::write(
            agent_dir.join("last-message.txt"),
            "[wave-proof]\n[wave-doc-delta]\n[wave-component]\n",
        )
        .expect("write message");
        fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");

        let run = WaveRunRecord {
            run_id: "wave-12-1".to_string(),
            wave_id: 12,
            slug: "result-envelope-proof-lifecycle".to_string(),
            title: "Result Envelope Proof Lifecycle".to_string(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: root.join(".wave/traces/runs/wave-12-1.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        };
        let mut declared_agent = test_agent("A2");
        declared_agent.file_ownership = vec![
            "docs/reference/runtime-config/README.md".to_string(),
            "crates/wave-runtime/src/lib.rs".to_string(),
        ];
        declared_agent.final_markers = vec![
            "[wave-proof]".to_string(),
            "[wave-doc-delta]".to_string(),
            "[wave-component]".to_string(),
        ];
        let agent_record = AgentRunRecord {
            id: "A2".to_string(),
            title: "Implementation".to_string(),
            status: WaveRunStatus::Succeeded,
            prompt_path: agent_dir.join("prompt.md"),
            last_message_path: agent_dir.join("last-message.txt"),
            events_path: agent_dir.join("events.jsonl"),
            stderr_path: agent_dir.join("stderr.txt"),
            result_envelope_path: None,
            expected_markers: declared_agent.final_markers.clone(),
            observed_markers: declared_agent.final_markers.clone(),
            exit_code: Some(0),
            error: None,
        };

        let updated =
            persist_agent_result_envelope(&root, &config, &run, &declared_agent, &agent_record)
                .expect("persist result envelope");
        let envelope_path = updated
            .result_envelope_path
            .as_ref()
            .expect("envelope path")
            .clone();
        let envelope =
            wave_trace::load_result_envelope(&envelope_path).expect("load result envelope");

        assert_eq!(
            envelope_path,
            root.join(".wave/state/results/wave-12/wave-12-1-a2/agent_result_envelope.json")
        );
        assert_eq!(
            envelope.source,
            wave_trace::ResultEnvelopeSource::Structured
        );
        assert_eq!(envelope.final_markers.missing, Vec::<String>::new());
        assert_eq!(
            envelope.doc_delta.status,
            wave_trace::ResultPayloadStatus::Recorded
        );
        assert_eq!(
            envelope.doc_delta.paths,
            vec![
                root.join("docs/reference/runtime-config/README.md")
                    .to_string_lossy()
                    .into_owned()
            ]
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn blocks_non_pass_cont_qa_verdicts() {
        let agent = test_agent("A0");
        let closure = wave_results::build_structured_closure_state(
            agent.id.as_str(),
            wave_domain::AttemptState::Succeeded,
            &wave_domain::FinalMarkerEnvelope::default(),
            None,
            Some(
                "[wave-gate] architecture=blocked integration=pass durability=pass live=pass docs=pass detail=test\nVerdict: BLOCKED\n",
            ),
        );

        assert_eq!(
            result_closure_contract_error(agent.id.as_str(), &closure),
            Some("cont-QA verdict is BLOCKED, not PASS".to_string())
        );
    }

    #[test]
    fn blocks_integration_that_is_not_ready_for_doc_closure() {
        let agent = test_agent("A8");
        let closure = wave_results::build_structured_closure_state(
            agent.id.as_str(),
            wave_domain::AttemptState::Succeeded,
            &wave_domain::FinalMarkerEnvelope::default(),
            None,
            Some(
                "[wave-integration] state=needs-more-work claims=0 conflicts=1 blockers=1 detail=test\n",
            ),
        );

        assert_eq!(
            result_closure_contract_error(agent.id.as_str(), &closure),
            Some("integration state is needs-more-work, not ready-for-doc-closure".to_string())
        );
    }

    #[test]
    fn blocks_doc_closure_deltas() {
        let agent = test_agent("A9");
        let closure = wave_results::build_structured_closure_state(
            agent.id.as_str(),
            wave_domain::AttemptState::Succeeded,
            &wave_domain::FinalMarkerEnvelope::default(),
            None,
            Some("[wave-doc-closure] state=delta paths=README.md detail=test\n"),
        );

        assert_eq!(
            result_closure_contract_error(agent.id.as_str(), &closure),
            Some("documentation closure state is delta, not closed or no-change".to_string())
        );
    }

    #[test]
    fn build_closure_state_records_structured_integration_verdict() {
        let agent = test_agent("A8");
        let final_markers = wave_domain::FinalMarkerEnvelope::from_contract(
            vec!["[wave-integration]".to_string()],
            vec!["[wave-integration]".to_string()],
        );

        let closure = wave_results::build_structured_closure_state(
            agent.id.as_str(),
            wave_domain::AttemptState::Succeeded,
            &final_markers,
            None,
            Some(
                "[wave-integration] state=ready-for-doc-closure claims=2 conflicts=0 blockers=0 detail=ok\n",
            ),
        );

        assert_eq!(closure.disposition, wave_domain::ClosureDisposition::Ready);
        match closure.verdict {
            wave_domain::ClosureVerdictPayload::Integration(verdict) => {
                assert_eq!(verdict.state.as_deref(), Some("ready-for-doc-closure"));
                assert_eq!(verdict.claims, Some(2));
                assert_eq!(verdict.conflicts, Some(0));
                assert_eq!(verdict.blockers, Some(0));
            }
            other => panic!("expected integration verdict, got {other:?}"),
        }
    }

    #[test]
    fn launch_preflight_refuses_missing_required_contracts_with_diagnostics() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/06.md"),
            metadata: WaveMetadata {
                id: 6,
                slug: "dark-factory-enforcement".to_string(),
                title: "Make dark-factory an enforced execution profile".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A2".to_string()],
                depends_on: Vec::new(),
                validation: Vec::new(),
                rollback: Vec::new(),
                proof: Vec::new(),
            },
            heading_title: Some("Wave 6".to_string()),
            commit_message: Some(
                "Feat: land dark-factory preflight and fail-closed policy".to_string(),
            ),
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![test_agent("A2")],
        };

        let report = build_launch_preflight(&wave, false);

        assert!(!report.ok);
        assert!(report.refusal.is_some());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contract == "validation-contract" && !diagnostic.ok)
        );
        assert!(
            report
                .refusal
                .as_ref()
                .expect("refusal")
                .detail
                .contains("validation-contract")
        );
    }

    #[test]
    fn preflight_refusal_keeps_rerun_intent_and_skips_run_state() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-preflight-refusal-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        let mut wave = launchable_test_wave(0);
        wave.metadata.validation.clear();
        let waves = vec![wave];
        let status = build_planning_status_with_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
        );

        request_rerun(&root, &config, 0, "retry after failed preflight").expect("request rerun");
        let error = launch_wave(
            &root,
            &config,
            &waves,
            &status,
            LaunchOptions {
                wave_id: Some(0),
                dry_run: false,
            },
        )
        .expect_err("preflight should fail");
        let report = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<LaunchPreflightError>())
            .expect("launch preflight error")
            .report();

        assert!(!report.ok);
        assert!(
            report
                .refusal
                .as_ref()
                .expect("refusal")
                .failed_contracts
                .iter()
                .any(|contract| contract == "validation-contract")
        );
        assert!(
            pending_rerun_wave_ids(&root, &config)
                .expect("pending reruns")
                .contains(&0)
        );
        assert!(
            load_latest_runs(&root, &config)
                .expect("latest runs")
                .is_empty()
        );
        assert!(!state_runs_dir(&root, &config).exists());
        assert!(!trace_runs_dir(&root, &config).exists());
        assert!(
            scheduler_event_log(&root, &config)
                .load_all()
                .expect("scheduler events")
                .is_empty()
        );
        let build_entries = fs::read_dir(build_specs_dir(&root, &config))
            .expect("build specs dir")
            .collect::<Result<Vec<_>, _>>()
            .expect("build entries");
        assert_eq!(build_entries.len(), 1);
        assert!(build_entries[0].path().join("preflight.json").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn orphaned_runs_fail_closed_when_launcher_process_is_gone() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-orphan-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");

        let mut record = WaveRunRecord {
            run_id: "wave-5-1".to_string(),
            wave_id: 5,
            slug: "tui-right-panel".to_string(),
            title: "TUI".to_string(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: root.join("bundle"),
            trace_path: root.join("trace.json"),
            codex_home: root.join("codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: Some(u32::MAX),
            launcher_started_at_ms: Some(0),
            completed_at_ms: None,
            agents: vec![AgentRunRecord {
                id: "A1".to_string(),
                title: "Impl".to_string(),
                status: WaveRunStatus::Running,
                prompt_path: root.join("bundle/agents/A1/prompt.md"),
                last_message_path: root.join("bundle/agents/A1/last-message.txt"),
                events_path: root.join("bundle/agents/A1/events.jsonl"),
                stderr_path: root.join("bundle/agents/A1/stderr.txt"),
                result_envelope_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: None,
                error: None,
            }],
            error: None,
        };
        fs::create_dir_all(root.join("bundle/agents/A1")).expect("create agent dir");
        fs::write(root.join("bundle/agents/A1/prompt.md"), "# prompt\n").expect("write prompt");

        let changed = reconcile_orphaned_run_record(&mut record).expect("reconcile orphan");

        assert!(changed);
        assert_eq!(record.status, WaveRunStatus::Failed);
        assert!(record.completed_at_ms.is_some());
        assert_eq!(record.agents[0].status, WaveRunStatus::Failed);
        assert!(
            record.agents[0]
                .error
                .as_deref()
                .unwrap()
                .contains("launcher process")
        );
        assert!(record.agents[0].last_message_path.exists());
        assert!(record.agents[0].events_path.exists());
        assert!(record.agents[0].stderr_path.exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn live_launcher_pid_is_not_treated_as_orphaned() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-live-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");

        let mut record = WaveRunRecord {
            run_id: "wave-5-2".to_string(),
            wave_id: 5,
            slug: "tui-right-panel".to_string(),
            title: "TUI".to_string(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: root.join("bundle"),
            trace_path: root.join("trace.json"),
            codex_home: root.join("codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: Some(std::process::id()),
            launcher_started_at_ms: current_process_started_at_ms(),
            completed_at_ms: None,
            agents: vec![AgentRunRecord {
                id: "A1".to_string(),
                title: "Impl".to_string(),
                status: WaveRunStatus::Running,
                prompt_path: root.join("bundle/agents/A1/prompt.md"),
                last_message_path: root.join("bundle/agents/A1/last-message.txt"),
                events_path: root.join("bundle/agents/A1/events.jsonl"),
                stderr_path: root.join("bundle/agents/A1/stderr.txt"),
                result_envelope_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: None,
                error: None,
            }],
            error: None,
        };

        let changed = reconcile_orphaned_run_record(&mut record).expect("reconcile running");

        assert!(!changed);
        assert_eq!(record.status, WaveRunStatus::Running);
        assert!(record.completed_at_ms.is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dry_run_launch_keeps_rerun_intent_and_skips_run_state() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-dry-run-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        let waves = vec![launchable_test_wave(0)];
        let status = build_planning_status_with_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
        );

        request_rerun(&root, &config, 0, "repair projection parity").expect("request rerun");
        let report = launch_wave(
            &root,
            &config,
            &waves,
            &status,
            LaunchOptions {
                wave_id: Some(0),
                dry_run: true,
            },
        )
        .expect("dry-run launch");

        assert_eq!(report.status, WaveRunStatus::DryRun);
        assert!(report.bundle_dir.is_dir());
        assert!(report.preflight_path.exists());
        assert!(!report.state_path.exists());
        assert!(!report.trace_path.exists());
        assert!(
            pending_rerun_wave_ids(&root, &config)
                .expect("pending reruns")
                .contains(&0)
        );
        assert!(
            load_latest_runs(&root, &config)
                .expect("latest runs")
                .is_empty()
        );
        assert!(
            scheduler_event_log(&root, &config)
                .load_all()
                .expect("scheduler events")
                .is_empty()
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn default_scheduler_budget_is_emitted_once() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-budget-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };

        ensure_default_scheduler_budget(&root, &config).expect("first budget");
        ensure_default_scheduler_budget(&root, &config).expect("second budget");
        let events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, SchedulerEventKind::SchedulerBudgetUpdated);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn orphan_cleanup_revokes_active_lease_and_releases_claim() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-scheduler-cleanup-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");
        ensure_default_scheduler_budget(&root, &config).expect("budget");

        let wave = launchable_test_wave(3);
        let run = WaveRunRecord {
            run_id: "wave-03-run".to_string(),
            wave_id: 3,
            slug: wave.metadata.slug.clone(),
            title: wave.metadata.title.clone(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-03-run"),
            trace_path: root.join(".wave/traces/runs/wave-03-run.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(1),
            launcher_pid: Some(std::process::id()),
            launcher_started_at_ms: current_process_started_at_ms(),
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        };

        let claim = acquire_wave_claim(&root, &config, &wave, &run.run_id, 1).expect("claim");
        let lease = grant_task_lease(&root, &config, &run, &wave.agents[0], &claim).expect("lease");
        assert_eq!(lease.state, TaskLeaseState::Granted);

        cleanup_scheduler_ownership_for_run(&root, &config, &run, "repair").expect("cleanup");
        let events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events");

        assert!(
            events
                .iter()
                .any(|event| event.kind == SchedulerEventKind::TaskLeaseRevoked)
        );
        assert!(
            events
                .iter()
                .any(|event| event.kind == SchedulerEventKind::WaveClaimReleased)
        );

        let _ = fs::remove_dir_all(&root);
    }

    fn test_agent(id: &str) -> WaveAgent {
        WaveAgent {
            id: id.to_string(),
            title: id.to_string(),
            role_prompts: Vec::new(),
            executor: BTreeMap::from([("model".to_string(), "gpt-5.4".to_string())]),
            context7: Some(Context7Defaults {
                bundle: "none".to_string(),
                query: Some("noop".to_string()),
            }),
            skills: Vec::new(),
            components: Vec::new(),
            capabilities: Vec::new(),
            exit_contract: None,
            deliverables: Vec::new(),
            file_ownership: Vec::new(),
            final_markers: Vec::new(),
            prompt: "Primary goal:\n- noop\n\nRequired context before coding:\n- Read README.md.\n\nFile ownership (only touch these paths):\n- README.md".to_string(),
        }
    }

    fn launchable_test_wave(id: u32) -> WaveDocument {
        let implementation_agent = WaveAgent {
            id: "A1".to_string(),
            title: "Implementation".to_string(),
            role_prompts: Vec::new(),
            executor: BTreeMap::from([("model".to_string(), "gpt-5.4".to_string())]),
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
            prompt: "Primary goal:\n- noop\n\nRequired context before coding:\n- Read README.md.\n\nFile ownership (only touch these paths):\n- README.md".to_string(),
        };

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
                proof: vec!["README.md".to_string()],
            },
            heading_title: Some(format!("Wave {id}")),
            commit_message: Some(format!("Feat: wave {id}")),
            component_promotions: Vec::new(),
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "Local validation".to_string(),
            }],
            context7_defaults: None,
            agents: vec![
                implementation_agent,
                test_agent("A8"),
                test_agent("A9"),
                test_agent("A0"),
            ],
        }
    }

    #[test]
    fn build_specs_are_rooted_in_project_state_dir() {
        let config = ProjectConfig {
            version: 1,
            project_name: "Codex Wave Mode".to_string(),
            default_lane: "main".to_string(),
            default_mode: ExecutionMode::DarkFactory,
            waves_dir: PathBuf::from("waves"),
            authority: AuthorityConfig {
                project_codex_home: PathBuf::from(".wave/codex"),
                state_dir: PathBuf::from(".wave/state"),
                state_build_specs_dir: PathBuf::from(".wave/state/build/specs"),
                state_runs_dir: PathBuf::from(".wave/state/runs"),
                state_control_dir: PathBuf::from(".wave/state/control"),
                trace_dir: PathBuf::from(".wave/traces"),
                trace_runs_dir: PathBuf::from(".wave/traces/runs"),
                ..AuthorityConfig::default()
            },
            codex_vendor_dir: PathBuf::from("third_party/codex-rs"),
            reference_wave_repo_dir: PathBuf::from("third_party/agent-wave-orchestrator"),
            dark_factory: Default::default(),
            lanes: BTreeMap::new(),
            ..ProjectConfig::default()
        };

        assert_eq!(
            build_specs_dir(Path::new("/repo"), &config),
            PathBuf::from("/repo/.wave/state/build/specs")
        );
    }

    #[test]
    fn bootstrap_authority_roots_materializes_canonical_state_dirs() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-authority-roots-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };

        bootstrap_authority_roots(&root, &config).expect("bootstrap authority roots");
        let authority = config.resolved_paths(&root).authority;

        for path in [
            authority.state_dir,
            authority.state_build_specs_dir,
            authority.state_events_dir,
            authority.state_events_control_dir,
            authority.state_events_coordination_dir,
            authority.state_results_dir,
            authority.state_derived_dir,
            authority.state_projections_dir,
            authority.state_traces_dir,
        ] {
            assert!(path.is_dir(), "{} should exist", path.display());
        }

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "materializes repo-local authority roots for verification"]
    fn repo_local_bootstrap_materializes_authority_roots() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("canonical repo root");
        let config = ProjectConfig::load_from_repo_root(&root).expect("load repo config");

        bootstrap_authority_roots(&root, &config).expect("bootstrap repo authority roots");
        let authority = config.resolved_paths(&root).authority;

        for path in [
            authority.state_dir,
            authority.state_build_specs_dir,
            authority.state_events_dir,
            authority.state_events_control_dir,
            authority.state_events_coordination_dir,
            authority.state_results_dir,
            authority.state_derived_dir,
            authority.state_projections_dir,
            authority.state_traces_dir,
        ] {
            assert!(path.is_dir(), "{} should exist", path.display());
        }
    }
}
