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
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
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
use wave_domain::WaveExecutionPhase;
use wave_domain::WavePromotionId;
use wave_domain::WavePromotionRecord;
use wave_domain::WavePromotionState;
use wave_domain::WaveSchedulerPriority;
use wave_domain::WaveSchedulingRecord;
use wave_domain::WaveSchedulingState;
use wave_domain::WaveWorktreeId;
use wave_domain::WaveWorktreeRecord;
use wave_domain::WaveWorktreeScope;
use wave_domain::WaveWorktreeState;
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
    pub worktree: Option<WaveWorktreeRecord>,
    pub promotion: Option<WavePromotionRecord>,
    pub scheduling: Option<WaveSchedulingRecord>,
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

const DEFAULT_LEASE_HEARTBEAT_INTERVAL_MS: u64 = 5_000;
const DEFAULT_LEASE_TTL_MS: u64 = 20_000;
const DEFAULT_AGENT_POLL_INTERVAL_MS: u64 = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LeaseTiming {
    heartbeat_interval_ms: u64,
    ttl_ms: u64,
    poll_interval_ms: u64,
}

impl Default for LeaseTiming {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: DEFAULT_LEASE_HEARTBEAT_INTERVAL_MS,
            ttl_ms: DEFAULT_LEASE_TTL_MS,
            poll_interval_ms: DEFAULT_AGENT_POLL_INTERVAL_MS,
        }
    }
}

#[derive(Debug, Clone)]
struct ExecutedAgent {
    record: AgentRunRecord,
    lease: TaskLeaseRecord,
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

#[derive(Debug)]
struct SchedulerAdmissionError {
    wave_id: u32,
    detail: String,
}

impl fmt::Display for SchedulerAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wave {} is not claimable: {}", self.wave_id, self.detail)
    }
}

impl std::error::Error for SchedulerAdmissionError {}

pub fn codex_binary_available() -> bool {
    Command::new(resolved_codex_binary())
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn resolved_codex_binary() -> String {
    env::var("WAVE_CODEX_BIN").unwrap_or_else(|_| "codex".to_string())
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
        worktree: record.worktree.clone(),
        promotion: record.promotion.clone(),
        scheduling: record.scheduling.clone(),
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
                "wave {} is not claimable: {}",
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
    let claim = claim_wave_for_launch(root, config, waves, wave, &run_id, created_at_ms)?;
    let worktree = match allocate_wave_worktree(root, config, wave, &run_id, created_at_ms) {
        Ok(worktree) => worktree,
        Err(error) => {
            release_wave_claim(
                root,
                config,
                &claim,
                "launch aborted while allocating worktree",
            )?;
            return Err(error);
        }
    };
    let promotion = match initial_promotion_record(root, wave, &worktree)
        .and_then(|promotion| publish_promotion_record(root, config, promotion, &run_id))
    {
        Ok(promotion) => promotion,
        Err(error) => {
            let _ = release_wave_worktree(
                root,
                config,
                &worktree,
                &run_id,
                "launch aborted while recording promotion state",
            );
            release_wave_claim(
                root,
                config,
                &claim,
                "launch aborted while recording promotion state",
            )?;
            return Err(error);
        }
    };
    let scheduling = match publish_scheduling_record(
        root,
        config,
        WaveSchedulingRecord {
            wave_id: wave.metadata.id,
            phase: WaveExecutionPhase::Implementation,
            priority: WaveSchedulerPriority::Implementation,
            state: WaveSchedulingState::Admitted,
            fairness_rank: 0,
            waiting_since_ms: None,
            protected_closure_capacity: false,
            preemptible: true,
            last_decision: Some("wave admitted for repo-local execution".to_string()),
            updated_at_ms: created_at_ms,
        },
        &run_id,
    ) {
        Ok(scheduling) => scheduling,
        Err(error) => {
            let _ = release_wave_worktree(
                root,
                config,
                &worktree,
                &run_id,
                "launch aborted while recording scheduling state",
            );
            release_wave_claim(
                root,
                config,
                &claim,
                "launch aborted while recording scheduling state",
            )?;
            return Err(error);
        }
    };
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
        worktree: Some(worktree.clone()),
        promotion: Some(promotion.clone()),
        scheduling: Some(scheduling.clone()),
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
        let _ = release_wave_worktree(
            root,
            config,
            &worktree,
            &run_id,
            "launch aborted before run state could be recorded",
        );
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
        let _ = release_wave_worktree(
            root,
            config,
            &worktree,
            &run_id,
            "launch aborted while clearing rerun intent",
        );
        release_wave_claim(
            root,
            config,
            &claim,
            "launch aborted while clearing rerun intent",
        )?;
        return Err(error);
    }

    let execution_agents = ordered_agents(wave);
    let lease_timing = LeaseTiming::default();
    let execution_root = PathBuf::from(
        record
            .worktree
            .as_ref()
            .map(|worktree| worktree.path.clone())
            .unwrap_or_else(|| root.to_string_lossy().into_owned()),
    );
    let mut promotion_checked = false;
    for (index, agent) in execution_agents.iter().enumerate() {
        if is_closure_agent(agent.id.as_str()) && !promotion_checked {
            let promotion_scheduling = publish_scheduling_record(
                root,
                config,
                WaveSchedulingRecord {
                    wave_id: record.wave_id,
                    phase: WaveExecutionPhase::Promotion,
                    priority: WaveSchedulerPriority::Closure,
                    state: WaveSchedulingState::Running,
                    fairness_rank: 0,
                    waiting_since_ms: None,
                    protected_closure_capacity: true,
                    preemptible: false,
                    last_decision: Some(
                        "implementation complete; evaluating promotion candidate".to_string(),
                    ),
                    updated_at_ms: now_epoch_ms()?,
                },
                &record.run_id,
            )?;
            record.scheduling = Some(promotion_scheduling);
            let evaluated = evaluate_wave_promotion(
                root,
                config,
                record
                    .worktree
                    .as_ref()
                    .context("missing worktree while evaluating promotion")?,
                record
                    .promotion
                    .as_ref()
                    .context("missing promotion record while evaluating promotion")?,
                &record.run_id,
            )?;
            record.promotion = Some(evaluated.clone());
            write_run_record(&state_path, &record)?;
            promotion_checked = true;
            if evaluated.state != WavePromotionState::Ready {
                record.status = WaveRunStatus::Failed;
                record.error = evaluated.detail.clone();
                record.completed_at_ms = Some(now_epoch_ms()?);
                let released_worktree = release_wave_worktree(
                    root,
                    config,
                    record
                        .worktree
                        .as_ref()
                        .context("missing worktree while closing conflicted promotion")?,
                    &record.run_id,
                    "promotion blocked before closure",
                )?;
                record.worktree = Some(released_worktree);
                record.scheduling = Some(publish_scheduling_record(
                    root,
                    config,
                    WaveSchedulingRecord {
                        wave_id: record.wave_id,
                        phase: WaveExecutionPhase::Promotion,
                        priority: WaveSchedulerPriority::Closure,
                        state: WaveSchedulingState::Released,
                        fairness_rank: 0,
                        waiting_since_ms: None,
                        protected_closure_capacity: true,
                        preemptible: false,
                        last_decision: Some(
                            "closure blocked because promotion is not ready".to_string(),
                        ),
                        updated_at_ms: now_epoch_ms()?,
                    },
                    &record.run_id,
                )?);
                write_run_record(&state_path, &record)?;
                write_trace_bundle(&trace_path, &record)?;
                release_wave_claim(root, config, &claim, "promotion blocked; claim released")?;
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
        }
        record.scheduling = Some(publish_scheduling_record(
            root,
            config,
            WaveSchedulingRecord {
                wave_id: record.wave_id,
                phase: if is_closure_agent(agent.id.as_str()) {
                    WaveExecutionPhase::Closure
                } else {
                    WaveExecutionPhase::Implementation
                },
                priority: if is_closure_agent(agent.id.as_str()) {
                    WaveSchedulerPriority::Closure
                } else {
                    WaveSchedulerPriority::Implementation
                },
                state: WaveSchedulingState::Running,
                fairness_rank: 0,
                waiting_since_ms: None,
                protected_closure_capacity: is_closure_agent(agent.id.as_str()),
                preemptible: !is_closure_agent(agent.id.as_str()),
                last_decision: Some(format!("running {} in shared wave worktree", agent.id)),
                updated_at_ms: now_epoch_ms()?,
            },
            &record.run_id,
        )?);
        let lease = match grant_task_lease(root, config, &record, agent, &claim, lease_timing) {
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
                        error,
                    );
                }
            };
        let (agent_record, lease) = match execute_agent(
            root,
            config,
            &execution_root,
            &record,
            agent,
            &record.agents[index],
            &prompt,
            &codex_home,
            &lease,
            lease_timing,
        ) {
            Ok(execution) => (execution.record, execution.lease),
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
            if let Some(worktree) = record.worktree.clone() {
                record.worktree = Some(release_wave_worktree(
                    root,
                    config,
                    &worktree,
                    &record.run_id,
                    "wave failed; worktree released",
                )?);
            }
            record.scheduling = Some(publish_scheduling_record(
                root,
                config,
                WaveSchedulingRecord {
                    wave_id: record.wave_id,
                    phase: if is_closure_agent(agent.id.as_str()) {
                        WaveExecutionPhase::Closure
                    } else {
                        WaveExecutionPhase::Implementation
                    },
                    priority: if is_closure_agent(agent.id.as_str()) {
                        WaveSchedulerPriority::Closure
                    } else {
                        WaveSchedulerPriority::Implementation
                    },
                    state: WaveSchedulingState::Released,
                    fairness_rank: 0,
                    waiting_since_ms: None,
                    protected_closure_capacity: is_closure_agent(agent.id.as_str()),
                    preemptible: false,
                    last_decision: Some(format!("{} failed; run released", agent.id)),
                    updated_at_ms: now_epoch_ms()?,
                },
                &record.run_id,
            )?);
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

    if let Some(worktree) = record.worktree.clone() {
        record.worktree = Some(release_wave_worktree(
            root,
            config,
            &worktree,
            &record.run_id,
            "wave completed; worktree released",
        )?);
    }
    record.scheduling = Some(publish_scheduling_record(
        root,
        config,
        WaveSchedulingRecord {
            wave_id: record.wave_id,
            phase: if promotion_checked {
                WaveExecutionPhase::Closure
            } else {
                WaveExecutionPhase::Implementation
            },
            priority: if promotion_checked {
                WaveSchedulerPriority::Closure
            } else {
                WaveSchedulerPriority::Implementation
            },
            state: WaveSchedulingState::Released,
            fairness_rank: 0,
            waiting_since_ms: None,
            protected_closure_capacity: promotion_checked,
            preemptible: false,
            last_decision: Some("wave completed and released".to_string()),
            updated_at_ms: now_epoch_ms()?,
        },
        &record.run_id,
    )?);
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
    loop {
        if let Some(limit) = options.limit {
            if launched.len() >= limit {
                break;
            }
        }
        let batch_limit = options
            .limit
            .map(|limit| limit.saturating_sub(launched.len()))
            .unwrap_or(2)
            .min(2);
        let batch = next_parallel_wave_batch(waves, &status, batch_limit);
        if batch.is_empty() {
            break;
        }
        if options.dry_run || batch.len() == 1 {
            let wave_id = batch[0].wave_id;
            let report = match launch_wave(
                root,
                config,
                waves,
                &status,
                LaunchOptions {
                    wave_id: Some(wave_id),
                    dry_run: options.dry_run,
                },
            ) {
                Ok(report) => report,
                Err(error)
                    if error
                        .chain()
                        .any(|cause| cause.downcast_ref::<SchedulerAdmissionError>().is_some()) =>
                {
                    status = refresh_planning_status(root, config, waves)?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            let failed = report.status == WaveRunStatus::Failed;
            launched.push(report);
            status = refresh_planning_status(root, config, waves)?;
            if options.dry_run || failed {
                break;
            }
            continue;
        }

        let mut reports = Vec::new();
        let mut admission_retry = false;
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for selection in &batch {
                let root = root.to_path_buf();
                let config = config.clone();
                let waves = waves.to_vec();
                let status = status.clone();
                let wave_id = selection.wave_id;
                handles.push(scope.spawn(move || {
                    launch_wave(
                        &root,
                        &config,
                        &waves,
                        &status,
                        LaunchOptions {
                            wave_id: Some(wave_id),
                            dry_run: false,
                        },
                    )
                }));
            }
            for handle in handles {
                match handle.join().expect("parallel wave launch thread panicked") {
                    Ok(report) => reports.push(report),
                    Err(error)
                        if error.chain().any(|cause| {
                            cause.downcast_ref::<SchedulerAdmissionError>().is_some()
                        }) =>
                    {
                        admission_retry = true;
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok::<(), anyhow::Error>(())
        })?;
        if admission_retry && reports.is_empty() {
            status = refresh_planning_status(root, config, waves)?;
            continue;
        }
        let failed = reports
            .iter()
            .any(|report| report.status == WaveRunStatus::Failed);
        launched.extend(reports);
        status = refresh_planning_status(root, config, waves)?;
        if failed {
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

fn next_parallel_wave_batch(
    waves: &[WaveDocument],
    status: &PlanningStatus,
    max_batch_size: usize,
) -> Vec<AutonomousWaveSelection> {
    if max_batch_size == 0 {
        return Vec::new();
    }
    let mut selected = Vec::new();
    for wave_id in &status.queue.claimable_wave_ids {
        let Some(entry) = status.waves.iter().find(|entry| entry.id == *wave_id) else {
            continue;
        };
        let Some(candidate) = waves.iter().find(|wave| wave.metadata.id == *wave_id) else {
            continue;
        };
        if selected.iter().any(|selection: &AutonomousWaveSelection| {
            let selected_wave = waves
                .iter()
                .find(|wave| wave.metadata.id == selection.wave_id)
                .expect("selected wave definition");
            waves_conflict_for_parallel_admission(selected_wave, candidate)
        }) {
            continue;
        }
        selected.push(AutonomousWaveSelection {
            wave_id: entry.id,
            slug: entry.slug.clone(),
            title: entry.title.clone(),
            blocked_by: entry.blocked_by.clone(),
        });
        if selected.len() >= max_batch_size {
            break;
        }
    }
    selected
}

fn waves_conflict_for_parallel_admission(left: &WaveDocument, right: &WaveDocument) -> bool {
    let left_paths = implementation_owned_paths(left);
    let right_paths = implementation_owned_paths(right);
    left_paths.iter().any(|left_path| {
        right_paths
            .iter()
            .any(|right_path| path_scopes_conflict(left_path, right_path))
    })
}

fn implementation_owned_paths(wave: &WaveDocument) -> Vec<String> {
    let mut paths = Vec::new();
    for agent in wave.implementation_agents() {
        for path in &agent.file_ownership {
            let normalized = path.trim_matches('/');
            if normalized.is_empty() {
                continue;
            }
            paths.push(normalized.to_string());
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn path_scopes_conflict(left: &str, right: &str) -> bool {
    left == right
        || left.starts_with(&format!("{right}/"))
        || right.starts_with(&format!("{left}/"))
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

fn scheduler_mutation_lock_path(root: &Path, config: &ProjectConfig) -> PathBuf {
    config
        .resolved_paths(root)
        .authority
        .state_derived_dir
        .join("scheduler")
        .join("mutation.lock")
}

fn with_scheduler_mutation<T>(
    root: &Path,
    config: &ProjectConfig,
    f: impl FnOnce(&SchedulerEventLog) -> Result<T>,
) -> Result<T> {
    let log = scheduler_event_log(root, config);
    wave_events::with_scheduler_mutation_lock(scheduler_mutation_lock_path(root, config), || {
        f(&log)
    })
}

fn runtime_scheduler_owner(session_id: impl Into<String>) -> SchedulerOwner {
    SchedulerOwner {
        scheduler_id: "wave-runtime".to_string(),
        scheduler_path: "wave-runtime/codex".to_string(),
        runtime: Some("codex".to_string()),
        executor: Some("codex".to_string()),
        session_id: Some(session_id.into()),
        process_id: Some(std::process::id()),
        process_started_at_ms: current_process_started_at_ms(),
    }
}

#[cfg(test)]
fn ensure_default_scheduler_budget(root: &Path, config: &ProjectConfig) -> Result<()> {
    with_scheduler_mutation(root, config, ensure_default_scheduler_budget_in_log)
}

fn ensure_default_scheduler_budget_in_log(log: &SchedulerEventLog) -> Result<()> {
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
            max_active_wave_claims: Some(2),
            max_active_task_leases: Some(2),
            reserved_closure_task_leases: Some(1),
            preemption_enabled: true,
        },
        owner: runtime_scheduler_owner("budget-bootstrap"),
        updated_at_ms: created_at_ms,
        detail: Some("default parallel-wave scheduler budget".to_string()),
    };
    append_scheduler_event_in_log(
        log,
        &SchedulerEvent::new(
            format!("sched-budget-default-{created_at_ms}"),
            SchedulerEventKind::SchedulerBudgetUpdated,
        )
        .with_created_at_ms(created_at_ms)
        .with_correlation_id("scheduler-budget-default")
        .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated { budget }),
    )
}

fn publish_worktree_record(
    root: &Path,
    config: &ProjectConfig,
    worktree: WaveWorktreeRecord,
    correlation_id: &str,
) -> Result<WaveWorktreeRecord> {
    append_scheduler_event(
        root,
        config,
        SchedulerEvent::new(
            format!(
                "sched-worktree-{}-{}-{}",
                worktree.wave_id,
                worktree_state_label(worktree.state),
                worktree.allocated_at_ms
            ),
            SchedulerEventKind::WaveWorktreeUpdated,
        )
        .with_wave_id(worktree.wave_id)
        .with_created_at_ms(now_epoch_ms()?)
        .with_correlation_id(correlation_id.to_string())
        .with_payload(SchedulerEventPayload::WaveWorktreeUpdated {
            worktree: worktree.clone(),
        }),
    )?;
    Ok(worktree)
}

fn publish_promotion_record(
    root: &Path,
    config: &ProjectConfig,
    promotion: WavePromotionRecord,
    correlation_id: &str,
) -> Result<WavePromotionRecord> {
    append_scheduler_event(
        root,
        config,
        SchedulerEvent::new(
            format!(
                "sched-promotion-{}-{}-{}",
                promotion.wave_id,
                promotion_state_label(promotion.state),
                promotion.checked_at_ms
            ),
            SchedulerEventKind::WavePromotionUpdated,
        )
        .with_wave_id(promotion.wave_id)
        .with_created_at_ms(now_epoch_ms()?)
        .with_correlation_id(correlation_id.to_string())
        .with_payload(SchedulerEventPayload::WavePromotionUpdated {
            promotion: promotion.clone(),
        }),
    )?;
    Ok(promotion)
}

fn publish_scheduling_record(
    root: &Path,
    config: &ProjectConfig,
    scheduling: WaveSchedulingRecord,
    correlation_id: &str,
) -> Result<WaveSchedulingRecord> {
    append_scheduler_event(
        root,
        config,
        SchedulerEvent::new(
            format!(
                "sched-wave-scheduling-{}-{}-{}",
                scheduling.wave_id,
                scheduling_state_label(scheduling.state),
                scheduling.updated_at_ms
            ),
            SchedulerEventKind::WaveSchedulingUpdated,
        )
        .with_wave_id(scheduling.wave_id)
        .with_created_at_ms(now_epoch_ms()?)
        .with_correlation_id(correlation_id.to_string())
        .with_payload(SchedulerEventPayload::WaveSchedulingUpdated {
            scheduling: scheduling.clone(),
        }),
    )?;
    Ok(scheduling)
}

fn allocate_wave_worktree(
    root: &Path,
    config: &ProjectConfig,
    wave: &WaveDocument,
    run_id: &str,
    allocated_at_ms: u128,
) -> Result<WaveWorktreeRecord> {
    let snapshot_ref = create_workspace_snapshot_commit(root, config, run_id, "base")?;
    let worktree_path =
        state_worktrees_dir(root, config).join(format!("wave-{:02}-{run_id}", wave.metadata.id));
    if let Some(parent) = worktree_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if worktree_path.exists() {
        let _ = run_git(
            root,
            &[
                "worktree",
                "remove",
                "--force",
                worktree_path.to_string_lossy().as_ref(),
            ],
        );
        let _ = fs::remove_dir_all(&worktree_path);
    }
    run_git(
        root,
        &[
            "worktree",
            "add",
            "--detach",
            worktree_path.to_string_lossy().as_ref(),
            snapshot_ref.as_str(),
        ],
    )?;
    publish_worktree_record(
        root,
        config,
        WaveWorktreeRecord {
            worktree_id: WaveWorktreeId::new(format!(
                "worktree-wave-{:02}-{run_id}",
                wave.metadata.id
            )),
            wave_id: wave.metadata.id,
            state: WaveWorktreeState::Allocated,
            path: worktree_path.to_string_lossy().into_owned(),
            base_ref: current_head_label(root)?,
            snapshot_ref,
            branch_ref: None,
            shared_scope: WaveWorktreeScope::Wave,
            allocated_at_ms,
            released_at_ms: None,
            detail: Some("shared wave-local worktree".to_string()),
        },
        run_id,
    )
}

fn release_wave_worktree(
    root: &Path,
    config: &ProjectConfig,
    worktree: &WaveWorktreeRecord,
    correlation_id: &str,
    detail: impl Into<String>,
) -> Result<WaveWorktreeRecord> {
    publish_worktree_record(
        root,
        config,
        WaveWorktreeRecord {
            state: WaveWorktreeState::Released,
            released_at_ms: Some(now_epoch_ms()?),
            detail: Some(detail.into()),
            ..worktree.clone()
        },
        correlation_id,
    )
}

fn initial_promotion_record(
    root: &Path,
    wave: &WaveDocument,
    worktree: &WaveWorktreeRecord,
) -> Result<WavePromotionRecord> {
    Ok(WavePromotionRecord {
        promotion_id: WavePromotionId::new(format!(
            "promotion-wave-{:02}-{}",
            wave.metadata.id, worktree.snapshot_ref
        )),
        wave_id: wave.metadata.id,
        worktree_id: Some(worktree.worktree_id.clone()),
        state: WavePromotionState::NotStarted,
        target_ref: current_head_label(root)?,
        snapshot_ref: worktree.snapshot_ref.clone(),
        candidate_ref: None,
        candidate_tree: None,
        conflict_paths: Vec::new(),
        checked_at_ms: worktree.allocated_at_ms,
        completed_at_ms: None,
        detail: Some("promotion not yet evaluated".to_string()),
    })
}

fn evaluate_wave_promotion(
    root: &Path,
    config: &ProjectConfig,
    worktree: &WaveWorktreeRecord,
    promotion: &WavePromotionRecord,
    correlation_id: &str,
) -> Result<WavePromotionRecord> {
    let checked_at_ms = now_epoch_ms()?;
    let pending = publish_promotion_record(
        root,
        config,
        WavePromotionRecord {
            state: WavePromotionState::Pending,
            checked_at_ms,
            completed_at_ms: None,
            detail: Some("evaluating promotion candidate".to_string()),
            ..promotion.clone()
        },
        correlation_id,
    )?;
    let worktree_root = Path::new(worktree.path.as_str());
    let candidate_ref =
        create_workspace_snapshot_commit(worktree_root, config, correlation_id, "candidate")?;
    let target_snapshot_ref =
        create_workspace_snapshot_commit(root, config, correlation_id, "target")?;
    let candidate_tree = git_output(
        worktree_root,
        &["rev-parse", &format!("{candidate_ref}^{{tree}}")],
    )?;
    let candidate_paths = git_diff_name_only(root, &pending.snapshot_ref, &candidate_ref)?;
    let target_changed: HashSet<String> =
        git_diff_name_only(root, &pending.snapshot_ref, &target_snapshot_ref)?
            .into_iter()
            .collect();
    let mut conflict_paths = Vec::new();
    for path in candidate_paths {
        if target_changed.contains(&path)
            && !git_status(
                root,
                &[
                    "diff",
                    "--quiet",
                    &target_snapshot_ref,
                    &candidate_ref,
                    "--",
                    &path,
                ],
            )?
        {
            conflict_paths.push(path);
        }
    }
    conflict_paths.sort();
    conflict_paths.dedup();
    let state = if conflict_paths.is_empty() {
        WavePromotionState::Ready
    } else {
        WavePromotionState::Conflicted
    };
    publish_promotion_record(
        root,
        config,
        WavePromotionRecord {
            state,
            target_ref: current_head_label(root)?,
            candidate_ref: Some(candidate_ref),
            candidate_tree: Some(candidate_tree),
            conflict_paths: conflict_paths.clone(),
            checked_at_ms,
            completed_at_ms: Some(now_epoch_ms()?),
            detail: Some(if conflict_paths.is_empty() {
                "promotion candidate is ready".to_string()
            } else {
                format!("promotion blocked by {}", conflict_paths.join(", "))
            }),
            ..pending
        },
        correlation_id,
    )
}

fn create_workspace_snapshot_commit(
    workspace_root: &Path,
    config: &ProjectConfig,
    run_id: &str,
    label: &str,
) -> Result<String> {
    let resolved_paths = config.resolved_paths(workspace_root);
    let derived_dir = resolved_paths.authority.state_derived_dir.join("git");
    fs::create_dir_all(&derived_dir)
        .with_context(|| format!("failed to create {}", derived_dir.display()))?;
    let index_path = derived_dir.join(format!("{run_id}-{label}.index"));
    if index_path.exists() {
        let _ = fs::remove_file(&index_path);
    }
    let envs = [("GIT_INDEX_FILE", index_path.as_path())];
    run_git_with_env(workspace_root, &["read-tree", "HEAD"], &envs)?;
    let state_derived_rel = config.authority.state_derived_dir.display().to_string();
    let state_worktrees_rel = config.authority.state_worktrees_dir.display().to_string();
    let exclude_derived = format!(":(exclude){state_derived_rel}");
    let exclude_worktrees = format!(":(exclude){state_worktrees_rel}");
    let add_args = [
        "add",
        "-A",
        "--",
        ".",
        exclude_derived.as_str(),
        exclude_worktrees.as_str(),
    ];
    run_git_with_env(workspace_root, &add_args, &envs)?;
    let tree = git_output_with_env(workspace_root, &["write-tree"], &envs)?;
    let parent = git_output(workspace_root, &["rev-parse", "HEAD"])?;
    let commit = git_output_with_env(
        workspace_root,
        &[
            "commit-tree",
            tree.as_str(),
            "-p",
            parent.as_str(),
            "-m",
            &format!("wave snapshot {run_id} {label}"),
        ],
        &envs,
    )?;
    let _ = fs::remove_file(index_path);
    Ok(commit)
}

fn current_head_label(root: &Path) -> Result<String> {
    let branch = git_output(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch == "HEAD" {
        git_output(root, &["rev-parse", "HEAD"])
    } else {
        Ok(branch)
    }
}

fn git_diff_name_only(root: &Path, base: &str, other: &str) -> Result<Vec<String>> {
    Ok(git_output(root, &["diff", "--name-only", base, other])?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn append_scheduler_event(
    root: &Path,
    config: &ProjectConfig,
    event: SchedulerEvent,
) -> Result<()> {
    with_scheduler_mutation(root, config, |log| {
        append_scheduler_event_in_log(log, &event)
    })
}

fn append_scheduler_event_in_log(log: &SchedulerEventLog, event: &SchedulerEvent) -> Result<()> {
    log.append(event)
}

fn claim_wave_for_launch(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    wave: &WaveDocument,
    run_id: &str,
    created_at_ms: u128,
) -> Result<WaveClaimRecord> {
    with_scheduler_mutation(root, config, |log| {
        ensure_default_scheduler_budget_in_log(log)?;
        let refreshed_status = refresh_planning_status(root, config, waves)?;
        if !is_claimable_wave(&refreshed_status, wave.metadata.id) {
            let detail = refreshed_status
                .waves
                .iter()
                .find(|entry| entry.id == wave.metadata.id)
                .map(queue_entry_reason)
                .unwrap_or_else(|| refreshed_status.queue.queue_ready_reason.clone());
            return Err(SchedulerAdmissionError {
                wave_id: wave.metadata.id,
                detail,
            }
            .into());
        }

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
        append_scheduler_event_in_log(
            log,
            &SchedulerEvent::new(
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
    })
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
    timing: LeaseTiming,
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
        expires_at_ms: Some(lease_expiry_ms(granted_at_ms, timing)),
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

fn renew_task_lease(
    root: &Path,
    config: &ProjectConfig,
    lease: &TaskLeaseRecord,
    timing: LeaseTiming,
    detail: impl Into<String>,
) -> Result<TaskLeaseRecord> {
    let renewed_at_ms = now_epoch_ms()?;
    let mut renewed = lease.clone();
    renewed.state = TaskLeaseState::Granted;
    renewed.heartbeat_at_ms = Some(renewed_at_ms);
    renewed.expires_at_ms = Some(lease_expiry_ms(renewed_at_ms, timing));
    renewed.finished_at_ms = None;
    renewed.detail = Some(detail.into());

    let mut event = SchedulerEvent::new(
        format!("sched-lease-renewed-{}-{renewed_at_ms}", lease.task_id),
        SchedulerEventKind::TaskLeaseRenewed,
    )
    .with_wave_id(lease.wave_id)
    .with_task_id(lease.task_id.clone())
    .with_lease_id(lease.lease_id.clone())
    .with_created_at_ms(renewed_at_ms)
    .with_correlation_id(
        renewed
            .owner
            .session_id
            .clone()
            .unwrap_or_else(|| lease.lease_id.as_str().to_string()),
    )
    .with_payload(SchedulerEventPayload::TaskLeaseUpdated {
        lease: renewed.clone(),
    });
    if let Some(claim_id) = renewed.claim_id.clone() {
        event = event.with_claim_id(claim_id);
    }
    append_scheduler_event(root, config, event)?;
    Ok(renewed)
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
    if matches!(state, TaskLeaseState::Expired) && closed.expires_at_ms.is_none() {
        closed.expires_at_ms = Some(finished_at_ms);
    }
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

fn worktree_state_label(state: WaveWorktreeState) -> &'static str {
    match state {
        WaveWorktreeState::Allocated => "allocated",
        WaveWorktreeState::Released => "released",
    }
}

fn promotion_state_label(state: WavePromotionState) -> &'static str {
    match state {
        WavePromotionState::NotStarted => "not_started",
        WavePromotionState::Pending => "pending",
        WavePromotionState::Ready => "ready",
        WavePromotionState::Conflicted => "conflicted",
        WavePromotionState::Failed => "failed",
    }
}

fn scheduling_state_label(state: WaveSchedulingState) -> &'static str {
    match state {
        WaveSchedulingState::Waiting => "waiting",
        WaveSchedulingState::Admitted => "admitted",
        WaveSchedulingState::Running => "running",
        WaveSchedulingState::Protected => "protected",
        WaveSchedulingState::Preempted => "preempted",
        WaveSchedulingState::Released => "released",
    }
}

fn is_closure_agent(agent_id: &str) -> bool {
    matches!(agent_id, "A0" | "A8" | "A9")
}

fn lease_expiry_ms(heartbeat_at_ms: u128, timing: LeaseTiming) -> u128 {
    heartbeat_at_ms + u128::from(timing.ttl_ms)
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
    if let Some(worktree) = record.worktree.clone() {
        record.worktree = Some(release_wave_worktree(
            root,
            config,
            &worktree,
            &record.run_id,
            format!("launch failed: {reason}"),
        )?);
    }
    if let Some(scheduling) = record.scheduling.clone() {
        record.scheduling = Some(publish_scheduling_record(
            root,
            config,
            WaveSchedulingRecord {
                state: WaveSchedulingState::Released,
                preemptible: false,
                last_decision: Some(format!("launch failed: {reason}")),
                updated_at_ms: now_epoch_ms()?,
                ..scheduling
            },
            &record.run_id,
        )?);
    }
    append_attempt_event(
        root,
        config,
        record,
        agent,
        AttemptState::Failed,
        record.created_at_ms,
        record.started_at_ms,
    )?;
    cleanup_scheduler_ownership_for_run(root, config, record, &format!("launch failed: {reason}"))?;
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

    let now_ms = now_epoch_ms()?;
    for lease in leases.into_values() {
        let state = if lease_is_expired(&lease, now_ms) {
            TaskLeaseState::Expired
        } else {
            TaskLeaseState::Revoked
        };
        close_task_lease(root, config, &lease, state, detail.to_string())?;
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
    config: &ProjectConfig,
    execution_root: &Path,
    run: &WaveRunRecord,
    agent: &WaveAgent,
    base_record: &AgentRunRecord,
    prompt: &str,
    codex_home: &Path,
    initial_lease: &TaskLeaseRecord,
    timing: LeaseTiming,
) -> Result<ExecutedAgent> {
    let agent_dir = base_record
        .prompt_path
        .parent()
        .context("agent prompt path has no parent directory")?;
    fs::create_dir_all(agent_dir)
        .with_context(|| format!("failed to create {}", agent_dir.display()))?;
    let stdout = File::create(&base_record.events_path)
        .with_context(|| format!("failed to create {}", base_record.events_path.display()))?;
    let stderr = File::create(&base_record.stderr_path)
        .with_context(|| format!("failed to create {}", base_record.stderr_path.display()))?;

    let mut command = Command::new(resolved_codex_binary());
    command
        .arg("exec")
        .arg("--json")
        .arg("--skip-git-repo-check")
        .arg("--dangerously-bypass-approvals-and-sandbox")
        .arg("--color")
        .arg("never")
        .arg("-C")
        .arg(execution_root)
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
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    let mut child = command.spawn().context("failed to start codex exec")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .context("failed to write prompt to codex exec stdin")?;
    }
    let (status, lease) = wait_for_agent_exit_with_lease(
        root,
        config,
        agent.id.as_str(),
        &mut child,
        initial_lease,
        timing,
    )?;

    let initial_error = if status.success() {
        None
    } else {
        Some(format!(
            "codex exec exited with {}",
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string())
        ))
    };
    let provisional_record = AgentRunRecord {
        status: if status.success() {
            WaveRunStatus::Succeeded
        } else {
            WaveRunStatus::Failed
        },
        exit_code: status.code(),
        error: initial_error.clone(),
        observed_markers: Vec::new(),
        ..base_record.clone()
    };
    let envelope =
        build_structured_result_envelope(root, run, agent, &provisional_record, now_epoch_ms()?)?;
    let observed_markers = envelope.closure_input.final_markers.observed.clone();

    if !status.success() {
        return Ok(ExecutedAgent {
            record: AgentRunRecord {
                status: WaveRunStatus::Failed,
                exit_code: status.code(),
                error: initial_error,
                observed_markers,
                ..base_record.clone()
            },
            lease,
        });
    }

    if envelope.closure.disposition != ClosureDisposition::Ready {
        return Ok(ExecutedAgent {
            record: AgentRunRecord {
                status: WaveRunStatus::Failed,
                exit_code: status.code(),
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
            },
            lease,
        });
    }

    if let Some(error) = result_closure_contract_error(agent.id.as_str(), &envelope.closure) {
        return Ok(ExecutedAgent {
            record: AgentRunRecord {
                status: WaveRunStatus::Failed,
                exit_code: status.code(),
                error: Some(error),
                observed_markers,
                ..base_record.clone()
            },
            lease,
        });
    }

    Ok(ExecutedAgent {
        record: AgentRunRecord {
            status: WaveRunStatus::Succeeded,
            exit_code: status.code(),
            error: None,
            observed_markers,
            ..base_record.clone()
        },
        lease,
    })
}

fn wait_for_agent_exit_with_lease(
    root: &Path,
    config: &ProjectConfig,
    agent_id: &str,
    child: &mut Child,
    initial_lease: &TaskLeaseRecord,
    timing: LeaseTiming,
) -> Result<(ExitStatus, TaskLeaseRecord)> {
    let mut lease = initial_lease.clone();
    let mut next_heartbeat_at_ms = lease.granted_at_ms + u128::from(timing.heartbeat_interval_ms);
    loop {
        if let Some(status) = child
            .try_wait()
            .context("failed while waiting for codex exec")?
        {
            return Ok((status, lease));
        }

        let now = now_epoch_ms()?;
        if now >= next_heartbeat_at_ms {
            if lease_is_expired(&lease, now) {
                terminate_child(child).context("failed to stop codex exec after lease expiry")?;
                close_task_lease(
                    root,
                    config,
                    &lease,
                    TaskLeaseState::Expired,
                    format!("lease expired while agent {agent_id} was still running"),
                )?;
                bail!("agent {agent_id} lost its lease before completion");
            }
            lease = match renew_task_lease(
                root,
                config,
                &lease,
                timing,
                format!("lease heartbeat renewed for agent {agent_id}"),
            ) {
                Ok(lease) => lease,
                Err(error) => {
                    terminate_child(child)
                        .context("failed to stop codex exec after lease renewal failure")?;
                    return Err(error).context(format!(
                        "lease renewal failed while agent {agent_id} was still running"
                    ));
                }
            };
            next_heartbeat_at_ms = now + u128::from(timing.heartbeat_interval_ms);
        }

        thread::sleep(Duration::from_millis(timing.poll_interval_ms));
    }
}

fn terminate_child(child: &mut Child) -> Result<()> {
    match child.kill() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
        Err(error) => return Err(error).context("failed to kill codex exec"),
    }
    let _ = child.wait();
    Ok(())
}

fn lease_is_expired(lease: &TaskLeaseRecord, now_ms: u128) -> bool {
    lease
        .expires_at_ms
        .map(|expires_at_ms| now_ms >= expires_at_ms)
        .unwrap_or(false)
}

fn ordered_agents(wave: &WaveDocument) -> Vec<&WaveAgent> {
    let mut agents = wave.agents.iter().collect::<Vec<_>>();
    agents.sort_by_key(|agent| match agent.id.as_str() {
        "E0" => (1_u8, agent.id.as_str()),
        "A6" => (2_u8, agent.id.as_str()),
        "A7" => (3_u8, agent.id.as_str()),
        "A8" => (4_u8, agent.id.as_str()),
        "A9" => (5_u8, agent.id.as_str()),
        "A0" => (6_u8, agent.id.as_str()),
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
    config
        .resolved_paths(root)
        .authority
        .materialize_canonical_state_tree()
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

fn state_worktrees_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    config.resolved_paths(root).authority.state_worktrees_dir
}

fn state_control_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    config.resolved_paths(root).authority.state_control_dir
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_status(root: &Path, args: &[&str]) -> Result<bool> {
    Ok(Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?
        .success())
}

fn git_output_with_env(root: &Path, args: &[&str], envs: &[(&str, &Path)]) -> Result<String> {
    let mut command = Command::new("git");
    command.current_dir(root).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git(root: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !status.success() {
        bail!("git {} failed with status {status}", args.join(" "));
    }
    Ok(())
}

fn run_git_with_env(root: &Path, args: &[&str], envs: &[(&str, &Path)]) -> Result<()> {
    let mut command = Command::new("git");
    command.current_dir(root).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let status = command
        .status()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !status.success() {
        bail!("git {} failed with status {status}", args.join(" "));
    }
    Ok(())
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
    use serde::Serialize;
    use std::collections::BTreeMap;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use wave_config::AuthorityConfig;
    use wave_config::ExecutionMode;
    use wave_control_plane::build_planning_status_with_state;
    use wave_events::SchedulerEventKind;
    use wave_events::SchedulerEventLog;
    use wave_spec::CompletionLevel;
    use wave_spec::ComponentPromotion;
    use wave_spec::Context7Defaults;
    use wave_spec::DeployEnvironment;
    use wave_spec::DocImpact;
    use wave_spec::DurabilityLevel;
    use wave_spec::ExitContract;
    use wave_spec::ProofLevel;
    use wave_spec::WaveMetadata;

    static FAKE_CODEX_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

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
            component_promotions: vec![ComponentPromotion {
                component: "runtime-fixture".to_string(),
                target: "baseline-proved".to_string(),
            }],
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
            worktree: None,
            promotion: None,
            scheduling: None,
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
            worktree: None,
            promotion: None,
            scheduling: None,
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
            worktree: None,
            promotion: None,
            scheduling: None,
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
            worktree: None,
            promotion: None,
            scheduling: None,
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
    fn concurrent_claimers_only_allow_one_live_claim() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-atomic-claim-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        seed_lint_context(&root);
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave = launchable_test_wave(4);
        let waves = vec![wave.clone()];
        let findings = wave_dark_factory::lint_project(&root, &waves);
        assert!(
            findings.is_empty(),
            "unexpected lint findings: {findings:?}"
        );
        let barrier = Arc::new(Barrier::new(2));
        let mut handles = Vec::new();

        for run_suffix in ["a", "b"] {
            let root = root.clone();
            let config = config.clone();
            let wave = wave.clone();
            let waves = waves.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                claim_wave_for_launch(
                    &root,
                    &config,
                    &waves,
                    &wave,
                    &format!("wave-04-run-{run_suffix}"),
                    now_epoch_ms().expect("timestamp"),
                )
                .map(|claim| claim.claim_id.as_str().to_string())
                .map_err(|error| error.to_string())
            }));
        }

        let results = handles
            .into_iter()
            .map(|handle| handle.join().expect("join claim thread"))
            .collect::<Vec<_>>();
        assert_eq!(
            results.iter().filter(|result| result.is_ok()).count(),
            1,
            "claim results: {results:?}"
        );
        assert_eq!(
            results.iter().filter(|result| result.is_err()).count(),
            1,
            "claim results: {results:?}"
        );
        assert!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .any(|error| error.contains("not claimable"))
        );

        let events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == SchedulerEventKind::WaveClaimAcquired)
                .count(),
            1,
            "exactly one claim acquisition event should exist"
        );
        assert!(
            load_latest_runs(&root, &config)
                .expect("latest runs")
                .is_empty()
        );
        assert!(!state_runs_dir(&root, &config).exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn third_wave_claim_is_budget_blocked_until_a_parallel_claim_releases() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-budget-block-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        seed_lint_context(&root);
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave_a = launchable_test_wave(5);
        let wave_b = launchable_test_wave(6);
        let wave_c = launchable_test_wave(7);
        let waves = vec![wave_a.clone(), wave_b.clone(), wave_c.clone()];

        let claim_a = claim_wave_for_launch(&root, &config, &waves, &wave_a, "wave-05-run", 1)
            .expect("claim a");
        let claim_b = claim_wave_for_launch(&root, &config, &waves, &wave_b, "wave-06-run", 2)
            .expect("claim b");
        let error = claim_wave_for_launch(&root, &config, &waves, &wave_c, "wave-07-run", 3)
            .expect_err("budget should block third claim");
        assert!(error.to_string().contains("budget"));

        release_wave_claim(&root, &config, &claim_a, "wave complete").expect("release claim a");
        let claim_c = claim_wave_for_launch(&root, &config, &waves, &wave_c, "wave-07-run", 4)
            .expect("claim c");
        assert_eq!(claim_b.wave_id, 6);
        assert_eq!(claim_c.wave_id, 7);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn wave_scoped_worktree_allocation_is_distinct_per_wave() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-worktree-allocation-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        seed_lint_context(&root);
        fs::write(root.join("src/wave5.rs"), "fn wave5() {}\n").expect("write wave5");
        fs::write(root.join("src/wave6.rs"), "fn wave6() {}\n").expect("write wave6");
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave_a = parallel_launchable_test_wave(5, "src/wave5.rs");
        let wave_b = parallel_launchable_test_wave(6, "src/wave6.rs");
        let worktree_a = allocate_wave_worktree(&root, &config, &wave_a, "wave-05-proof", 1)
            .expect("worktree a");
        let worktree_b = allocate_wave_worktree(&root, &config, &wave_b, "wave-06-proof", 2)
            .expect("worktree b");

        assert_ne!(worktree_a.path, worktree_b.path);
        assert_eq!(worktree_a.shared_scope, WaveWorktreeScope::Wave);
        assert_eq!(worktree_b.shared_scope, WaveWorktreeScope::Wave);
        assert!(Path::new(&worktree_a.path).is_dir());
        assert!(Path::new(&worktree_b.path).is_dir());

        let events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == SchedulerEventKind::WaveWorktreeUpdated)
                .count(),
            2
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn promotion_conflict_is_explicit_before_closure() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-promotion-conflict-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create root");
        seed_lint_context(&root);
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");
        let wave = launchable_test_wave(12);
        let worktree =
            allocate_wave_worktree(&root, &config, &wave, "wave-12-proof", 1).expect("worktree");
        let initial = publish_promotion_record(
            &root,
            &config,
            initial_promotion_record(&root, &wave, &worktree).expect("initial promotion"),
            "wave-12-proof",
        )
        .expect("publish initial promotion");

        fs::write(root.join("README.md"), "# root changed\n").expect("change root readme");
        fs::write(
            Path::new(&worktree.path).join("README.md"),
            "# worktree changed\n",
        )
        .expect("change worktree readme");

        let evaluated =
            evaluate_wave_promotion(&root, &config, &worktree, &initial, "wave-12-proof")
                .expect("evaluate promotion");
        assert_eq!(evaluated.state, WavePromotionState::Conflicted);
        assert_eq!(evaluated.conflict_paths, vec!["README.md".to_string()]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn autonomous_launch_runs_two_non_conflicting_waves_in_parallel_with_distinct_worktrees() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-parallel-autonomous-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::create_dir_all(root.join("waves")).expect("create waves dir");
        seed_lint_context(&root);
        fs::write(root.join("src/wave5.rs"), "fn wave5() {}\n").expect("write wave5");
        fs::write(root.join("src/wave6.rs"), "fn wave6() {}\n").expect("write wave6");
        fs::write(root.join("waves/05.md"), "# Wave 5\n").expect("write wave 05");
        fs::write(root.join("waves/06.md"), "# Wave 6\n").expect("write wave 06");
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let waves = vec![
            parallel_launchable_test_wave(5, "src/wave5.rs"),
            parallel_launchable_test_wave(6, "src/wave6.rs"),
        ];
        let status = build_planning_status_with_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
        );
        let lint_messages = wave_dark_factory::lint_project(&root, &waves)
            .into_iter()
            .map(|finding| (finding.wave_id, finding.rule, finding.message))
            .collect::<Vec<_>>();
        let refreshed = refresh_planning_status(&root, &config, &waves).expect("refresh status");
        assert!(
            refreshed.queue.claimable_wave_ids.len() >= 2,
            "refreshed status blocked waves: {:?}; lint={:?}",
            refreshed
                .waves
                .iter()
                .map(|wave| (wave.id, wave.blocked_by.clone()))
                .collect::<Vec<_>>(),
            lint_messages
        );
        let reports = with_fake_codex(&root, "parallel", || {
            autonomous_launch(
                &root,
                &config,
                &waves,
                status.clone(),
                AutonomousOptions {
                    limit: Some(2),
                    dry_run: false,
                },
            )
        })
        .expect("parallel autonomous launch");
        assert_eq!(reports.len(), 2);
        assert!(
            reports
                .iter()
                .all(|report| report.status == WaveRunStatus::Succeeded)
        );

        let latest_runs = load_latest_runs(&root, &config).expect("latest runs");
        let run_a = latest_runs.get(&5).expect("run a");
        let run_b = latest_runs.get(&6).expect("run b");
        let worktree_a = run_a.worktree.as_ref().expect("run a worktree");
        let worktree_b = run_b.worktree.as_ref().expect("run b worktree");
        assert_ne!(worktree_a.path, worktree_b.path);
        assert_eq!(
            run_a.promotion.as_ref().map(|promotion| promotion.state),
            Some(WavePromotionState::Ready)
        );
        assert_eq!(
            run_b.promotion.as_ref().map(|promotion| promotion.state),
            Some(WavePromotionState::Ready)
        );
        assert!(
            run_a
                .scheduling
                .as_ref()
                .map(|record| record.protected_closure_capacity)
                .unwrap_or(false)
        );
        assert!(
            run_b
                .scheduling
                .as_ref()
                .map(|record| record.protected_closure_capacity)
                .unwrap_or(false)
        );

        for (worktree, run) in [(worktree_a, run_a), (worktree_b, run_b)] {
            for agent in ["A1", "A8", "A9", "A0"] {
                let seen = fs::read_to_string(
                    Path::new(&worktree.path).join(format!(".wave-{agent}-worktree.txt")),
                )
                .expect("agent worktree marker");
                assert_eq!(seen.trim(), worktree.path);
            }
            assert_eq!(
                events_for_wave_worktree_allocations(&root, &config, run.wave_id),
                1,
                "each wave should allocate exactly one shared worktree"
            );
        }

        let timing_a = read_agent_timing(Path::new(&worktree_a.path).join(".wave-A1-timing.txt"));
        let timing_b = read_agent_timing(Path::new(&worktree_b.path).join(".wave-A1-timing.txt"));
        assert!(timing_a.0 < timing_b.1 && timing_b.0 < timing_a.1);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn heartbeat_renewal_updates_live_lease_state() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-heartbeat-renewal-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        seed_lint_context(&root);
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave = launchable_test_wave(7);
        let run = WaveRunRecord {
            run_id: "wave-07-run".to_string(),
            wave_id: 7,
            slug: wave.metadata.slug.clone(),
            title: wave.metadata.title.clone(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-07-run"),
            trace_path: root.join(".wave/traces/runs/wave-07-run.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(1),
            launcher_pid: Some(std::process::id()),
            launcher_started_at_ms: current_process_started_at_ms(),
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        };
        let timing = LeaseTiming {
            heartbeat_interval_ms: 25,
            ttl_ms: 200,
            poll_interval_ms: 10,
        };

        let claim = claim_wave_for_launch(&root, &config, &[wave.clone()], &wave, &run.run_id, 1)
            .expect("claim");
        let lease =
            grant_task_lease(&root, &config, &run, &wave.agents[0], &claim, timing).expect("lease");
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 0.12")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn helper");

        let (status, renewed_lease) = wait_for_agent_exit_with_lease(
            &root,
            &config,
            wave.agents[0].id.as_str(),
            &mut child,
            &lease,
            timing,
        )
        .expect("wait with heartbeat");
        assert!(status.success());
        assert!(
            renewed_lease.heartbeat_at_ms.expect("heartbeat")
                > lease.heartbeat_at_ms.expect("initial heartbeat")
        );
        close_task_lease(
            &root,
            &config,
            &renewed_lease,
            TaskLeaseState::Released,
            "agent completed",
        )
        .expect("release lease");

        let events = SchedulerEventLog::new(
            config
                .resolved_paths(&root)
                .authority
                .state_events_scheduler_dir,
        )
        .load_all()
        .expect("scheduler events");
        assert!(
            events
                .iter()
                .any(|event| event.kind == SchedulerEventKind::TaskLeaseRenewed)
        );
        assert!(
            events
                .iter()
                .any(|event| event.kind == SchedulerEventKind::TaskLeaseReleased)
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn overdue_live_lease_expires_and_fails_closed() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-heartbeat-expiry-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        seed_lint_context(&root);
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave = launchable_test_wave(8);
        let run = WaveRunRecord {
            run_id: "wave-08-run".to_string(),
            wave_id: 8,
            slug: wave.metadata.slug.clone(),
            title: wave.metadata.title.clone(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-08-run"),
            trace_path: root.join(".wave/traces/runs/wave-08-run.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(1),
            launcher_pid: Some(std::process::id()),
            launcher_started_at_ms: current_process_started_at_ms(),
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        };
        let timing = LeaseTiming {
            heartbeat_interval_ms: 120,
            ttl_ms: 50,
            poll_interval_ms: 10,
        };

        let claim = claim_wave_for_launch(&root, &config, &[wave.clone()], &wave, &run.run_id, 1)
            .expect("claim");
        let lease =
            grant_task_lease(&root, &config, &run, &wave.agents[0], &claim, timing).expect("lease");
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn helper");

        let error = wait_for_agent_exit_with_lease(
            &root,
            &config,
            wave.agents[0].id.as_str(),
            &mut child,
            &lease,
            timing,
        )
        .expect_err("lease should expire");
        assert!(error.to_string().contains("lost its lease"));

        let events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events");
        assert!(
            events
                .iter()
                .any(|event| event.kind == SchedulerEventKind::TaskLeaseExpired)
        );

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
        seed_lint_context(&root);
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
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        };

        let claim = claim_wave_for_launch(&root, &config, &[wave.clone()], &wave, &run.run_id, 1)
            .expect("claim");
        let lease = grant_task_lease(
            &root,
            &config,
            &run,
            &wave.agents[0],
            &claim,
            LeaseTiming::default(),
        )
        .expect("lease");
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

    #[test]
    #[ignore = "writes the Wave 14 live-proof bundle and local trace seed"]
    fn generate_phase_2_parallel_wave_execution_live_proof_bundle() {
        #[derive(Debug, Serialize)]
        struct TimingWindow {
            start_ms: u128,
            end_ms: u128,
        }

        #[derive(Debug, Serialize)]
        struct ParallelWaveProof {
            wave_id: u32,
            run_id: String,
            worktree: WaveWorktreeRecord,
            promotion: WavePromotionRecord,
            scheduling: WaveSchedulingRecord,
            agent_worktree_markers: BTreeMap<String, String>,
            timing_window: TimingWindow,
        }

        #[derive(Debug, Serialize)]
        struct ParallelRuntimeProofBundle {
            generated_at_ms: u128,
            fixture_root: String,
            overlap_observed: bool,
            distinct_worktrees: bool,
            per_agent_worktrees_used: bool,
            waves: Vec<ParallelWaveProof>,
        }

        #[derive(Debug, Serialize)]
        struct ProjectionSnapshotBundle {
            planning: wave_control_plane::PlanningStatus,
            control_status: wave_control_plane::ControlStatusReadModel,
        }

        #[derive(Debug, Serialize)]
        struct PromotionConflictBundle {
            wave_id: u32,
            worktree: WaveWorktreeRecord,
            initial_promotion: WavePromotionRecord,
            evaluated_promotion: WavePromotionRecord,
        }

        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let proof_dir =
            workspace_root.join("docs/implementation/live-proofs/phase-2-parallel-wave-execution");
        fs::create_dir_all(&proof_dir).expect("create proof dir");

        let fixture_root =
            workspace_root.join(".wave/state/live-proofs/phase-2-parallel-wave-execution-fixture");
        if fixture_root.exists() {
            fs::remove_dir_all(&fixture_root).expect("clear prior fixture");
        }
        fs::create_dir_all(fixture_root.join("src")).expect("create fixture src dir");
        fs::create_dir_all(fixture_root.join("waves")).expect("create fixture waves dir");
        seed_lint_context(&fixture_root);
        fs::write(fixture_root.join("src/wave5.rs"), "fn wave5() {}\n").expect("write wave5");
        fs::write(fixture_root.join("src/wave6.rs"), "fn wave6() {}\n").expect("write wave6");
        fs::write(fixture_root.join("waves/05.md"), "# Wave 5\n").expect("write wave 05");
        fs::write(fixture_root.join("waves/06.md"), "# Wave 6\n").expect("write wave 06");
        init_git_repo(&fixture_root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&fixture_root, &config).expect("bootstrap fixture authority");

        let waves = vec![
            parallel_launchable_test_wave(5, "src/wave5.rs"),
            parallel_launchable_test_wave(6, "src/wave6.rs"),
        ];
        let planning = build_planning_status_with_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
        );
        with_fake_codex(&fixture_root, "parallel", || {
            autonomous_launch(
                &fixture_root,
                &config,
                &waves,
                planning,
                AutonomousOptions {
                    limit: Some(2),
                    dry_run: false,
                },
            )
        })
        .expect("run parallel proof fixture");

        let latest_runs = load_latest_runs(&fixture_root, &config).expect("fixture latest runs");
        let run_a = latest_runs.get(&5).expect("fixture run a").clone();
        let run_b = latest_runs.get(&6).expect("fixture run b").clone();
        let worktree_a = run_a.worktree.clone().expect("run a worktree");
        let worktree_b = run_b.worktree.clone().expect("run b worktree");
        let promotion_a = run_a.promotion.clone().expect("run a promotion");
        let promotion_b = run_b.promotion.clone().expect("run b promotion");
        let scheduling_a = run_a.scheduling.clone().expect("run a scheduling");
        let scheduling_b = run_b.scheduling.clone().expect("run b scheduling");
        let timing_a = read_agent_timing(Path::new(&worktree_a.path).join(".wave-A1-timing.txt"));
        let timing_b = read_agent_timing(Path::new(&worktree_b.path).join(".wave-A1-timing.txt"));

        let parallel_bundle = ParallelRuntimeProofBundle {
            generated_at_ms: now_epoch_ms().expect("proof timestamp"),
            fixture_root: fixture_root.display().to_string(),
            overlap_observed: timing_a.0 < timing_b.1 && timing_b.0 < timing_a.1,
            distinct_worktrees: worktree_a.path != worktree_b.path,
            per_agent_worktrees_used: false,
            waves: vec![
                ParallelWaveProof {
                    wave_id: run_a.wave_id,
                    run_id: run_a.run_id.clone(),
                    worktree: worktree_a.clone(),
                    promotion: promotion_a.clone(),
                    scheduling: scheduling_a.clone(),
                    agent_worktree_markers: ["A1", "A8", "A9", "A0"]
                        .into_iter()
                        .map(|agent| {
                            (
                                agent.to_string(),
                                fs::read_to_string(
                                    Path::new(&worktree_a.path)
                                        .join(format!(".wave-{agent}-worktree.txt")),
                                )
                                .expect("read agent worktree marker")
                                .trim()
                                .to_string(),
                            )
                        })
                        .collect(),
                    timing_window: TimingWindow {
                        start_ms: timing_a.0,
                        end_ms: timing_a.1,
                    },
                },
                ParallelWaveProof {
                    wave_id: run_b.wave_id,
                    run_id: run_b.run_id.clone(),
                    worktree: worktree_b.clone(),
                    promotion: promotion_b.clone(),
                    scheduling: scheduling_b.clone(),
                    agent_worktree_markers: ["A1", "A8", "A9", "A0"]
                        .into_iter()
                        .map(|agent| {
                            (
                                agent.to_string(),
                                fs::read_to_string(
                                    Path::new(&worktree_b.path)
                                        .join(format!(".wave-{agent}-worktree.txt")),
                                )
                                .expect("read agent worktree marker")
                                .trim()
                                .to_string(),
                            )
                        })
                        .collect(),
                    timing_window: TimingWindow {
                        start_ms: timing_b.0,
                        end_ms: timing_b.1,
                    },
                },
            ],
        };
        fs::write(
            proof_dir.join("parallel-runtime-proof.json"),
            serde_json::to_string_pretty(&parallel_bundle).expect("serialize parallel proof"),
        )
        .expect("write parallel proof");

        let findings = wave_dark_factory::lint_project(&fixture_root, &waves);
        let skill_catalog_issues = wave_dark_factory::validate_skill_catalog(&fixture_root);
        let spine = wave_control_plane::build_projection_spine_from_authority(
            &fixture_root,
            &config,
            &waves,
            &findings,
            &skill_catalog_issues,
            &latest_runs,
            &HashSet::new(),
            true,
        )
        .expect("build proof projection spine");
        let projection_bundle = ProjectionSnapshotBundle {
            planning: spine.planning.status.clone(),
            control_status: wave_control_plane::build_control_status_read_model_from_spine(&spine),
        };
        fs::write(
            proof_dir.join("projection-snapshot.json"),
            serde_json::to_string_pretty(&projection_bundle)
                .expect("serialize projection snapshot"),
        )
        .expect("write projection snapshot");

        let scheduler_events = scheduler_event_log(&fixture_root, &config)
            .load_all()
            .expect("load fixture scheduler events");
        fs::write(
            proof_dir.join("scheduler-events.jsonl"),
            scheduler_events
                .iter()
                .map(|event| serde_json::to_string(event).expect("serialize event"))
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .expect("write scheduler events");

        let conflict_root =
            workspace_root.join(".wave/state/live-proofs/phase-2-parallel-wave-execution-conflict");
        if conflict_root.exists() {
            fs::remove_dir_all(&conflict_root).expect("clear prior conflict fixture");
        }
        fs::create_dir_all(&conflict_root).expect("create conflict fixture");
        seed_lint_context(&conflict_root);
        init_git_repo(&conflict_root);
        bootstrap_authority_roots(&conflict_root, &config).expect("bootstrap conflict fixture");
        let conflict_wave = launchable_test_wave(12);
        let conflict_worktree =
            allocate_wave_worktree(&conflict_root, &config, &conflict_wave, "wave-12-proof", 1)
                .expect("conflict worktree");
        let initial_promotion = publish_promotion_record(
            &conflict_root,
            &config,
            initial_promotion_record(&conflict_root, &conflict_wave, &conflict_worktree)
                .expect("initial promotion"),
            "wave-12-proof",
        )
        .expect("publish initial conflict promotion");
        fs::write(conflict_root.join("README.md"), "# root changed\n").expect("change root readme");
        fs::write(
            Path::new(&conflict_worktree.path).join("README.md"),
            "# worktree changed\n",
        )
        .expect("change worktree readme");
        let evaluated_promotion = evaluate_wave_promotion(
            &conflict_root,
            &config,
            &conflict_worktree,
            &initial_promotion,
            "wave-12-proof",
        )
        .expect("evaluate conflict promotion");
        fs::write(
            proof_dir.join("promotion-conflict.json"),
            serde_json::to_string_pretty(&PromotionConflictBundle {
                wave_id: conflict_wave.metadata.id,
                worktree: conflict_worktree,
                initial_promotion,
                evaluated_promotion: evaluated_promotion.clone(),
            })
            .expect("serialize conflict proof"),
        )
        .expect("write conflict proof");

        let workspace_config =
            ProjectConfig::load_from_repo_root(&workspace_root).expect("load workspace config");
        bootstrap_authority_roots(&workspace_root, &workspace_config)
            .expect("bootstrap workspace authority");
        let trace_seed_run_id = "wave-14-live-proof".to_string();
        let state_path = state_runs_dir(&workspace_root, &workspace_config)
            .join(format!("{trace_seed_run_id}.json"));
        let trace_path = trace_runs_dir(&workspace_root, &workspace_config)
            .join(format!("{trace_seed_run_id}.json"));
        let mut trace_seed = run_a.clone();
        trace_seed.run_id = trace_seed_run_id;
        trace_seed.wave_id = 14;
        trace_seed.slug = "parallel-wave-execution-and-merge-discipline-live-proof".to_string();
        trace_seed.title = "Wave 14 live-proof fixture".to_string();
        trace_seed.status = WaveRunStatus::DryRun;
        trace_seed.dry_run = true;
        trace_seed.error = Some(
            "local Wave 14 live-proof trace seed; not an authored-wave completion record"
                .to_string(),
        );
        trace_seed.trace_path = trace_path.clone();
        trace_seed.created_at_ms = now_epoch_ms().expect("trace seed timestamp");
        trace_seed.started_at_ms = Some(trace_seed.created_at_ms);
        trace_seed.completed_at_ms = Some(trace_seed.created_at_ms + 1);
        let trace_seed_worktree_id = WaveWorktreeId::new("worktree-wave-14-live-proof".to_string());
        if let Some(worktree) = trace_seed.worktree.as_mut() {
            worktree.wave_id = 14;
            worktree.worktree_id = trace_seed_worktree_id.clone();
        }
        if let Some(promotion) = trace_seed.promotion.as_mut() {
            promotion.wave_id = 14;
            promotion.promotion_id =
                WavePromotionId::new("promotion-wave-14-live-proof".to_string());
            promotion.worktree_id = Some(trace_seed_worktree_id.clone());
        }
        if let Some(scheduling) = trace_seed.scheduling.as_mut() {
            scheduling.wave_id = 14;
        }
        write_run_record(&state_path, &trace_seed).expect("write wave 14 proof run");
        write_trace_bundle(&trace_path, &trace_seed).expect("write wave 14 proof trace");

        fs::write(
            proof_dir.join("trace-latest-wave-14.json"),
            serde_json::to_string_pretty(&dogfood_evidence_report(&trace_seed))
                .expect("serialize latest trace"),
        )
        .expect("write latest trace proof");
        fs::write(
            proof_dir.join("trace-replay-wave-14.json"),
            serde_json::to_string_pretty(&trace_inspection_report(&trace_seed).replay)
                .expect("serialize replay trace"),
        )
        .expect("write replay trace proof");
    }

    fn test_agent(id: &str) -> WaveAgent {
        match id {
            "A0" | "A6" | "A7" | "A8" | "A9" | "E0" => closure_test_agent(id),
            _ => WaveAgent {
                id: id.to_string(),
                title: format!("Implementation {id}"),
                role_prompts: Vec::new(),
                executor: BTreeMap::from([("profile".to_string(), "codex".to_string())]),
                context7: Some(Context7Defaults {
                    bundle: "rust-control-plane".to_string(),
                    query: Some(
                        "runtime fixture for scheduler claims leases and queue behavior"
                            .to_string(),
                    ),
                }),
                skills: vec!["wave-core".to_string()],
                components: vec!["runtime".to_string()],
                capabilities: vec!["testing".to_string()],
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Contract,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Unit,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec![format!("src/{id}.rs")],
                file_ownership: vec![format!("src/{id}.rs")],
                final_markers: vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                prompt: format!(
                    "Primary goal:\n- implement fixture work\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output.\n\nFile ownership (only touch these paths):\n- src/{id}.rs"
                ),
            },
        }
    }

    fn closure_test_agent(id: &str) -> WaveAgent {
        let (role_prompt, owned_path, final_marker) = match id {
            "E0" => (
                "docs/agents/wave-cont-eval-role.md",
                ".wave/eval/test-wave.md",
                "[wave-eval]",
            ),
            "A6" => (
                "docs/agents/wave-design-role.md",
                ".wave/design/test-wave.md",
                "[wave-design]",
            ),
            "A7" => (
                "docs/agents/wave-security-role.md",
                ".wave/security/test-wave.md",
                "[wave-security]",
            ),
            "A0" => (
                "docs/agents/wave-cont-qa-role.md",
                ".wave/reviews/test-cont-qa.md",
                "[wave-gate]",
            ),
            "A8" => (
                "docs/agents/wave-integration-role.md",
                ".wave/integration/test-wave.md",
                "[wave-integration]",
            ),
            "A9" => (
                "docs/agents/wave-documentation-role.md",
                ".wave/docs/test-wave.md",
                "[wave-doc-closure]",
            ),
            other => panic!("unexpected closure agent {other}"),
        };
        WaveAgent {
            id: id.to_string(),
            title: format!("Closure {id}"),
            role_prompts: vec![role_prompt.to_string()],
            executor: BTreeMap::from([("profile".to_string(), "review-codex".to_string())]),
            context7: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some(
                    "closure fixture for integration documentation and qa review".to_string(),
                ),
            }),
            skills: vec!["wave-core".to_string()],
            components: Vec::new(),
            capabilities: Vec::new(),
            exit_contract: None,
            deliverables: Vec::new(),
            file_ownership: vec![owned_path.to_string()],
            final_markers: vec![final_marker.to_string()],
            prompt: format!(
                "Primary goal:\n- close the fixture wave\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final {final_marker} marker as a plain last line.\n\nFile ownership (only touch these paths):\n- {owned_path}"
            ),
        }
    }

    fn launchable_test_wave(id: u32) -> WaveDocument {
        let implementation_agent = WaveAgent {
            id: "A1".to_string(),
            title: "Implementation".to_string(),
            role_prompts: Vec::new(),
            executor: BTreeMap::from([("profile".to_string(), "codex".to_string())]),
            context7: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some(
                    "runtime fixture for scheduler claims leases and queue behavior".to_string(),
                ),
            }),
            skills: vec!["wave-core".to_string()],
            components: vec!["runtime".to_string()],
            capabilities: vec!["testing".to_string()],
            exit_contract: Some(ExitContract {
                completion: CompletionLevel::Contract,
                durability: DurabilityLevel::Durable,
                proof: ProofLevel::Unit,
                doc_impact: DocImpact::Owned,
            }),
            deliverables: vec!["README.md".to_string()],
            file_ownership: vec!["README.md".to_string()],
            final_markers: vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            prompt: "Primary goal:\n- land the runtime fixture\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output.\n\nFile ownership (only touch these paths):\n- README.md".to_string(),
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
            component_promotions: vec![ComponentPromotion {
                component: "runtime-fixture".to_string(),
                target: "baseline-proved".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "Local validation".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some(
                    "runtime fixture for scheduler claims leases and queue behavior".to_string(),
                ),
            }),
            agents: vec![
                implementation_agent,
                closure_test_agent("A8"),
                closure_test_agent("A9"),
                closure_test_agent("A0"),
            ],
        }
    }

    fn seed_lint_context(root: &Path) {
        fs::write(root.join("README.md"), "# fixture\n").expect("write readme");

        let skills_dir = root.join("skills/wave-core");
        fs::create_dir_all(&skills_dir).expect("create skills dir");
        fs::write(
            skills_dir.join("skill.json"),
            r#"{"id":"wave-core","title":"Wave Core","description":"Fixture skill","activation":{"when":"Always"}}"#,
        )
        .expect("write skill manifest");
        fs::write(skills_dir.join("SKILL.md"), "# Wave Core\n").expect("write skill body");

        let context7_dir = root.join("docs/context7");
        fs::create_dir_all(&context7_dir).expect("create context7 dir");
        fs::write(
            context7_dir.join("bundles.json"),
            r#"{"version":1,"defaultBundle":"rust-control-plane","laneDefaults":{},"bundles":{"rust-control-plane":{"description":"Fixture bundle","libraries":[{"libraryName":"fixture-lib","queryHint":"scheduler claims leases queue projection fixture"}]}}}"#,
        )
        .expect("write context7 bundle catalog");

        let agent_dir = root.join("docs/agents");
        fs::create_dir_all(&agent_dir).expect("create role prompt dir");
        for path in [
            "wave-cont-qa-role.md",
            "wave-integration-role.md",
            "wave-documentation-role.md",
        ] {
            fs::write(agent_dir.join(path), "# role prompt\n").expect("write role prompt");
        }
    }

    fn init_git_repo(root: &Path) {
        run_git(root, &["init", "-b", "main"]).expect("git init");
        run_git(root, &["config", "user.email", "wave-tests@example.com"]).expect("git email");
        run_git(root, &["config", "user.name", "Wave Tests"]).expect("git name");
        run_git(root, &["add", "-A"]).expect("git add");
        run_git(root, &["commit", "-m", "initial fixture"]).expect("git commit");
    }

    fn fake_codex_env_lock() -> &'static Mutex<()> {
        FAKE_CODEX_ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn install_fake_codex(root: &Path) -> PathBuf {
        let bin_dir = root.join(".wave/test-bin");
        fs::create_dir_all(&bin_dir).expect("create fake codex bin dir");
        let script_path = bin_dir.join("codex");
        fs::write(
            &script_path,
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "--version" ]]; then
  echo "codex-test"
  exit 0
fi
workdir=""
output=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -C)
      workdir="$2"
      shift 2
      ;;
    -o)
      output="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
agent="$(basename "$(dirname "$output")")"
wave_tag="$(basename "$workdir" | cut -d- -f1-2)"
mkdir -p "$(dirname "$output")"
mkdir -p "$workdir"
if [[ "${WAVE_FAKE_CODEX_SCENARIO:-}" == "parallel" && "$agent" == "A1" ]]; then
  printf 'start=%s\n' "$(date +%s%3N)" > "$workdir/.wave-${agent}-timing.txt"
  sleep 0.5
  printf 'end=%s\n' "$(date +%s%3N)" >> "$workdir/.wave-${agent}-timing.txt"
fi
echo "$workdir" > "$workdir/.wave-${agent}-worktree.txt"
case "$agent" in
  A8)
    mkdir -p "$workdir/.wave/integration"
    printf '%s\n' '[wave-integration] state=ready-for-doc-closure claims=1 conflicts=0 blockers=0 detail=ok' > "$workdir/.wave/integration/${wave_tag}.md"
    printf '%s\n' '[wave-integration] state=ready-for-doc-closure claims=1 conflicts=0 blockers=0 detail=ok' > "$output"
    ;;
  A9)
    mkdir -p "$workdir/.wave/docs"
    printf '%s\n' '[wave-doc-closure] state=closed paths=docs/implementation/live.md detail=ok' > "$workdir/.wave/docs/${wave_tag}.md"
    printf '%s\n' '[wave-doc-closure] state=closed paths=docs/implementation/live.md detail=ok' > "$output"
    ;;
  A0)
    mkdir -p "$workdir/.wave/reviews"
    cat > "$workdir/.wave/reviews/${wave_tag}.md" <<'EOF'
[wave-gate] architecture=pass integration=pass durability=pass live=pass docs=pass detail=ok
Verdict: PASS
EOF
    cat > "$output" <<'EOF'
[wave-gate] architecture=pass integration=pass durability=pass live=pass docs=pass detail=ok
Verdict: PASS
EOF
    ;;
  *)
    printf 'touched by %s\n' "$agent" >> "$workdir/README.md"
    cat > "$output" <<'EOF'
[wave-proof]
[wave-doc-delta]
[wave-component]
EOF
    ;;
esac
printf '{"event":"ok","agent":"%s"}\n' "$agent"
"#,
        )
        .expect("write fake codex script");
        let mut permissions = fs::metadata(&script_path)
            .expect("fake codex metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("chmod fake codex");
        script_path
    }

    fn with_fake_codex<T>(root: &Path, scenario: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let _guard = fake_codex_env_lock().lock().expect("fake codex env lock");
        let binary = install_fake_codex(root);
        let previous_binary = env::var("WAVE_CODEX_BIN").ok();
        let previous_scenario = env::var("WAVE_FAKE_CODEX_SCENARIO").ok();
        unsafe {
            env::set_var("WAVE_CODEX_BIN", &binary);
            env::set_var("WAVE_FAKE_CODEX_SCENARIO", scenario);
        }
        let result = f();
        match previous_binary {
            Some(value) => unsafe { env::set_var("WAVE_CODEX_BIN", value) },
            None => unsafe { env::remove_var("WAVE_CODEX_BIN") },
        }
        match previous_scenario {
            Some(value) => unsafe { env::set_var("WAVE_FAKE_CODEX_SCENARIO", value) },
            None => unsafe { env::remove_var("WAVE_FAKE_CODEX_SCENARIO") },
        }
        result
    }

    fn parallel_launchable_test_wave(id: u32, owned_path: &str) -> WaveDocument {
        let mut wave = launchable_test_wave(id);
        wave.agents[0].deliverables = vec![owned_path.to_string()];
        wave.agents[0].file_ownership = vec![owned_path.to_string()];
        wave.agents[0].prompt = format!(
            "Primary goal:\n- land the runtime fixture\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output.\n\nFile ownership (only touch these paths):\n- {owned_path}"
        );
        wave.agents[1].file_ownership = vec![format!(".wave/integration/wave-{id:02}.md")];
        wave.agents[1].prompt = format!(
            "Primary goal:\n- integration close the fixture wave\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-integration] marker as a plain last line.\n\nFile ownership (only touch these paths):\n- .wave/integration/wave-{id:02}.md"
        );
        wave.agents[2].file_ownership = vec![format!(".wave/docs/wave-{id:02}.md")];
        wave.agents[2].prompt = format!(
            "Primary goal:\n- documentation close the fixture wave\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-doc-closure] marker as a plain last line.\n\nFile ownership (only touch these paths):\n- .wave/docs/wave-{id:02}.md"
        );
        wave.agents[3].file_ownership = vec![format!(".wave/reviews/wave-{id:02}.md")];
        wave.agents[3].prompt = format!(
            "Primary goal:\n- qa close the fixture wave\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-gate] marker as a plain last line before Verdict: PASS.\n\nFile ownership (only touch these paths):\n- .wave/reviews/wave-{id:02}.md"
        );
        wave.metadata.proof = vec![owned_path.to_string()];
        wave
    }

    fn events_for_wave_worktree_allocations(
        root: &Path,
        config: &ProjectConfig,
        wave_id: u32,
    ) -> usize {
        scheduler_event_log(root, config)
            .load_all()
            .expect("scheduler events")
            .into_iter()
            .filter(|event| match &event.payload {
                SchedulerEventPayload::WaveWorktreeUpdated { worktree } => {
                    worktree.wave_id == wave_id
                        && event.kind == SchedulerEventKind::WaveWorktreeUpdated
                        && worktree.state == WaveWorktreeState::Allocated
                }
                _ => false,
            })
            .count()
    }

    fn read_agent_timing(path: PathBuf) -> (u128, u128) {
        let raw = fs::read_to_string(path).expect("read timing");
        let mut start = None;
        let mut end = None;
        for line in raw.lines() {
            if let Some(value) = line.strip_prefix("start=") {
                start = value.parse::<u128>().ok();
            }
            if let Some(value) = line.strip_prefix("end=") {
                end = value.parse::<u128>().ok();
            }
        }
        (start.expect("start"), end.expect("end"))
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
