//! Codex-first runtime helpers for the Wave workspace.
//!
//! The crate owns file-backed launch, rerun, draft, and replay data plumbing
//! that the CLI and operator surfaces build on. Runtime state stays rooted
//! under the project-scoped paths declared in `wave.toml`.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
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
use wave_spec::WaveAgent;
use wave_spec::WaveDocument;
use wave_trace::AgentRunRecord;
use wave_trace::ArtifactKind;
use wave_trace::AttemptState;
use wave_trace::ClosureDisposition;
use wave_trace::ClosureState;
use wave_trace::ClosureVerdictPayload;
use wave_trace::CompiledAgentPrompt;
use wave_trace::ContQaClosureVerdict;
use wave_trace::DocDeltaEnvelope;
use wave_trace::DocumentationClosureVerdict;
use wave_trace::DraftBundle;
use wave_trace::FinalMarkerEnvelope;
use wave_trace::IntegrationClosureVerdict;
use wave_trace::MarkerEvidence;
use wave_trace::ProofArtifact;
use wave_trace::RESULT_ENVELOPE_FILE_NAME;
use wave_trace::ResultDisposition;
use wave_trace::ResultEnvelopeRecord;
use wave_trace::ResultEnvelopeSource;
use wave_trace::ResultPayloadStatus;
use wave_trace::WaveRunRecord;
use wave_trace::WaveRunStatus;
use wave_trace::load_latest_run_records_by_wave;
use wave_trace::load_run_record;
use wave_trace::now_epoch_ms;
use wave_trace::write_result_envelope;
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
    reconcile_orphaned_run_records(root, config)?;
    load_latest_run_records_by_wave(&state_runs_dir(root, config))
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
    let record = RerunIntentRecord {
        wave_id,
        reason: reason.into(),
        requested_by: "operator".to_string(),
        status: RerunIntentStatus::Requested,
        requested_at_ms: now_epoch_ms()?,
        cleared_at_ms: None,
    };
    write_rerun_intent(root, config, &record)?;
    Ok(record)
}

pub fn clear_rerun(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
) -> Result<Option<RerunIntentRecord>> {
    let mut intents = list_rerun_intents(root, config)?;
    let Some(mut record) = intents.remove(&wave_id) else {
        return Ok(None);
    };
    record.status = RerunIntentStatus::Cleared;
    record.cleared_at_ms = Some(now_epoch_ms()?);
    write_rerun_intent(root, config, &record)?;
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
    let wave = select_wave(waves, status, options.wave_id)?;
    let run_id = format!("wave-{:02}-{}", wave.metadata.id, now_epoch_ms()?);
    let bundle = compile_wave_bundle(root, config, wave, &run_id)?;
    let preflight = build_launch_preflight(wave, options.dry_run);
    let preflight_path = bundle.bundle_dir.join("preflight.json");
    fs::write(&preflight_path, serde_json::to_string_pretty(&preflight)?)
        .with_context(|| format!("failed to write {}", preflight_path.display()))?;
    if !preflight.ok {
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
    let _ = clear_rerun(root, config, wave.metadata.id)?;

    let created_at_ms = now_epoch_ms()?;
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
        launcher_pid: Some(std::process::id()),
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

    if !codex_binary_available() {
        bail!("codex binary is not available on PATH");
    }

    record.status = WaveRunStatus::Running;
    record.started_at_ms = Some(now_epoch_ms()?);
    write_run_record(&state_path, &record)?;

    let execution_agents = ordered_agents(wave);
    for (index, agent) in execution_agents.iter().enumerate() {
        record.agents[index].status = WaveRunStatus::Running;
        write_run_record(&state_path, &record)?;
        let prompt = fs::read_to_string(&record.agents[index].prompt_path).with_context(|| {
            format!(
                "failed to read {}",
                record.agents[index].prompt_path.display()
            )
        })?;
        let agent_record = execute_agent(root, agent, &record.agents[index], &prompt, &codex_home)?;
        let agent_record =
            persist_agent_result_envelope(root, config, &record, agent, &agent_record)?;
        record.agents[index] = agent_record.clone();
        if agent_record.status == WaveRunStatus::Failed {
            record.status = WaveRunStatus::Failed;
            record.error = agent_record.error.clone();
            record.completed_at_ms = Some(now_epoch_ms()?);
            write_run_record(&state_path, &record)?;
            write_trace_bundle(&trace_path, &record)?;
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
    Ok(wave_control_plane::build_planning_status_with_state(
        config,
        waves,
        &findings,
        &[],
        &latest_runs,
        &rerun_wave_ids,
    ))
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

fn reconcile_orphaned_run_records(root: &Path, config: &ProjectConfig) -> Result<()> {
    let runs_dir = state_runs_dir(root, config);
    if !runs_dir.exists() {
        return Ok(());
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

        let mut record = load_run_record(&path)?;
        if !reconcile_orphaned_run_record(&mut record)? {
            continue;
        }
        write_run_record(&path, &record)?;
        write_trace_bundle(&record.trace_path, &record)?;
    }

    Ok(())
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
    if process_is_alive(launcher_pid) {
        return None;
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

fn persist_agent_result_envelope(
    root: &Path,
    config: &ProjectConfig,
    run: &WaveRunRecord,
    declared_agent: &WaveAgent,
    agent_record: &AgentRunRecord,
) -> Result<AgentRunRecord> {
    let envelope = build_result_envelope(root, run, declared_agent, agent_record)?;
    let resolved = config.resolved_paths(root);
    let attempt_dir = resolved.wave_attempt_results_dir(run.wave_id, &envelope.attempt_id);
    let envelope_path = attempt_dir.join(RESULT_ENVELOPE_FILE_NAME);
    write_result_envelope(&envelope_path, &envelope)?;

    let mut updated = agent_record.clone();
    updated.result_envelope_path = Some(envelope_path);
    Ok(updated)
}

fn build_result_envelope(
    root: &Path,
    run: &WaveRunRecord,
    declared_agent: &WaveAgent,
    agent_record: &AgentRunRecord,
) -> Result<ResultEnvelopeRecord> {
    let attempt_state = AttemptState::from_run_status(agent_record.status, run.dry_run);
    let final_markers = FinalMarkerEnvelope::from_contract(
        agent_record.expected_markers.clone(),
        agent_record.observed_markers.clone(),
    );
    let output_text = fs::read_to_string(&agent_record.last_message_path).ok();
    let text_artifacts = collect_text_artifacts(
        root,
        declared_agent,
        output_text.as_deref().unwrap_or_default(),
    );
    let marker_evidence = collect_marker_evidence_for_envelope(
        &text_artifacts,
        &final_markers.observed,
        &agent_record.last_message_path,
    );
    let doc_delta_paths = declared_agent
        .file_ownership
        .iter()
        .filter(|path| looks_like_doc_path(path))
        .cloned()
        .collect::<Vec<_>>();
    let doc_delta_status = if final_markers
        .observed
        .iter()
        .any(|marker| marker == "[wave-doc-delta]")
    {
        if doc_delta_paths.is_empty() {
            ResultPayloadStatus::EvidenceOnly
        } else {
            ResultPayloadStatus::Recorded
        }
    } else if declared_agent
        .expected_final_markers()
        .iter()
        .any(|marker| *marker == "[wave-doc-delta]")
    {
        ResultPayloadStatus::Missing
    } else {
        ResultPayloadStatus::Missing
    };
    let closure = build_closure_state(
        declared_agent,
        attempt_state,
        &final_markers,
        agent_record.error.as_deref(),
        &text_artifacts,
    );

    Ok(ResultEnvelopeRecord {
        result_envelope_id: format!(
            "result:{}:{}",
            run.run_id,
            declared_agent.id.to_ascii_lowercase()
        ),
        wave_id: run.wave_id,
        task_id: task_id_for_agent(run.wave_id, &declared_agent.id),
        attempt_id: structured_attempt_id(run, declared_agent),
        agent_id: declared_agent.id.clone(),
        task_role: inferred_task_role_for_agent(declared_agent),
        closure_role: inferred_closure_role_for_agent(declared_agent),
        source: ResultEnvelopeSource::Structured,
        attempt_state,
        disposition: ResultDisposition::from_attempt_state(
            attempt_state,
            final_markers.missing.len(),
        ),
        summary: agent_record.error.clone().or_else(|| {
            Some(format!(
                "structured result envelope for {}",
                declared_agent.id
            ))
        }),
        output_text,
        final_markers: final_markers.clone(),
        proof_bundle_ids: Vec::new(),
        fact_ids: Vec::new(),
        contradiction_ids: Vec::new(),
        artifacts: build_result_artifacts(run, agent_record),
        doc_delta: DocDeltaEnvelope {
            status: doc_delta_status,
            summary: if doc_delta_paths.is_empty() {
                None
            } else {
                Some(format!("doc delta paths: {}", doc_delta_paths.join(", ")))
            },
            paths: doc_delta_paths,
        },
        marker_evidence,
        closure,
        created_at_ms: now_epoch_ms()?,
    })
}

fn build_result_artifacts(
    run: &WaveRunRecord,
    agent_record: &AgentRunRecord,
) -> Vec<ProofArtifact> {
    vec![
        ProofArtifact {
            path: agent_record
                .last_message_path
                .to_string_lossy()
                .into_owned(),
            kind: ArtifactKind::Other,
            digest: None,
            note: Some("last-message".to_string()),
        },
        ProofArtifact {
            path: agent_record.events_path.to_string_lossy().into_owned(),
            kind: ArtifactKind::Other,
            digest: None,
            note: Some("events".to_string()),
        },
        ProofArtifact {
            path: agent_record.stderr_path.to_string_lossy().into_owned(),
            kind: ArtifactKind::Other,
            digest: None,
            note: Some("stderr".to_string()),
        },
        ProofArtifact {
            path: run.trace_path.to_string_lossy().into_owned(),
            kind: ArtifactKind::Trace,
            digest: None,
            note: Some(run.run_id.clone()),
        },
    ]
}

fn collect_marker_evidence_for_envelope(
    text_artifacts: &[String],
    observed_markers: &[String],
    last_message_path: &Path,
) -> Vec<MarkerEvidence> {
    let mut evidence = Vec::new();
    for text in text_artifacts {
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            for marker in observed_markers {
                if line == marker || line.starts_with(&(marker.clone() + " ")) {
                    evidence.push(MarkerEvidence {
                        marker: marker.clone(),
                        line: line.to_string(),
                        source: Some(last_message_path.to_string_lossy().replace('\\', "/")),
                    });
                }
            }
        }
    }
    for marker in observed_markers {
        if !evidence.iter().any(|item| item.marker == *marker) {
            evidence.push(MarkerEvidence {
                marker: marker.clone(),
                line: marker.clone(),
                source: Some(last_message_path.to_string_lossy().replace('\\', "/")),
            });
        }
    }
    evidence.sort_by(|left, right| {
        (
            left.marker.as_str(),
            left.line.as_str(),
            left.source.as_deref().unwrap_or(""),
        )
            .cmp(&(
                right.marker.as_str(),
                right.line.as_str(),
                right.source.as_deref().unwrap_or(""),
            ))
    });
    evidence.dedup_by(|left, right| {
        left.marker == right.marker && left.line == right.line && left.source == right.source
    });
    evidence
}

fn structured_attempt_id(run: &WaveRunRecord, agent: &WaveAgent) -> String {
    format!("{}-{}", run.run_id, agent.id.to_ascii_lowercase())
}

fn task_id_for_agent(wave_id: u32, agent_id: &str) -> String {
    format!("wave-{wave_id:02}:agent-{}", agent_id.to_ascii_lowercase())
}

fn inferred_task_role_for_agent(agent: &WaveAgent) -> String {
    match agent.id.as_str() {
        "A8" => "integration",
        "A9" => "documentation",
        "A0" => "cont_qa",
        "E0" => "cont_eval",
        _ => "implementation",
    }
    .to_string()
}

fn inferred_closure_role_for_agent(agent: &WaveAgent) -> Option<String> {
    match agent.id.as_str() {
        "E0" => Some("cont_eval".to_string()),
        "A8" => Some("integration".to_string()),
        "A9" => Some("documentation".to_string()),
        "A0" => Some("cont_qa".to_string()),
        _ => None,
    }
}

fn looks_like_doc_path(path: &str) -> bool {
    path == "README.md"
        || path.starts_with("docs/")
        || path.ends_with(".md")
            && (path.contains("/docs/") || path.starts_with("docs/") || !path.contains('/'))
}

fn execute_agent(
    root: &Path,
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

    let last_message = fs::read_to_string(&base_record.last_message_path).unwrap_or_default();
    let text_artifacts = collect_text_artifacts(root, agent, &last_message);
    let observed_markers =
        collect_observed_markers(root, agent, &last_message, &base_record.expected_markers);
    let missing_markers = base_record
        .expected_markers
        .iter()
        .filter(|marker| !observed_markers.iter().any(|observed| observed == *marker))
        .cloned()
        .collect::<Vec<_>>();

    if !output.status.success() {
        return Ok(AgentRunRecord {
            status: WaveRunStatus::Failed,
            exit_code: output.status.code(),
            error: Some(format!(
                "codex exec exited with {}",
                output
                    .status
                    .code()
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "signal".to_string())
            )),
            observed_markers,
            ..base_record.clone()
        });
    }

    if !missing_markers.is_empty() {
        return Ok(AgentRunRecord {
            status: WaveRunStatus::Failed,
            exit_code: output.status.code(),
            error: Some(format!(
                "agent {} is missing final markers: {}",
                agent.id,
                missing_markers.join(", ")
            )),
            observed_markers,
            ..base_record.clone()
        });
    }

    let closure = build_closure_state(
        agent,
        AttemptState::Succeeded,
        &FinalMarkerEnvelope::from_contract(
            base_record.expected_markers.clone(),
            observed_markers.clone(),
        ),
        None,
        &text_artifacts,
    );

    if let Some(error) = closure_contract_error(agent, &closure) {
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

fn collect_observed_markers(
    root: &Path,
    agent: &WaveAgent,
    last_message: &str,
    expected_markers: &[String],
) -> Vec<String> {
    let text_artifacts = collect_text_artifacts(root, agent, last_message);
    observed_markers_in_texts(&text_artifacts, expected_markers)
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

fn collect_text_artifacts(root: &Path, agent: &WaveAgent, last_message: &str) -> Vec<String> {
    let mut texts = Vec::new();
    if !last_message.is_empty() {
        texts.push(last_message.to_string());
    }

    for owned_path in &agent.file_ownership {
        let candidate = root.join(owned_path);
        if !is_marker_fallback_path(&candidate) || !candidate.exists() {
            continue;
        }

        let Ok(contents) = fs::read_to_string(&candidate) else {
            continue;
        };
        if !contents.is_empty() {
            texts.push(contents);
        }
    }

    texts
}

fn observed_markers_in_texts(texts: &[String], expected_markers: &[String]) -> Vec<String> {
    let mut observed = Vec::new();
    for text in texts {
        for marker in observed_markers_in_text(text, expected_markers) {
            if !observed.iter().any(|existing| existing == &marker) {
                observed.push(marker);
            }
        }
    }
    observed
}

fn observed_markers_in_text(text: &str, expected_markers: &[String]) -> Vec<String> {
    let mut observed = Vec::new();
    for line in text.lines().map(str::trim) {
        for marker in expected_markers {
            if line == marker || line.starts_with(&(marker.clone() + " ")) {
                if !observed.iter().any(|existing| existing == marker) {
                    observed.push(marker.clone());
                }
            }
        }
    }
    observed
}

fn build_closure_state(
    agent: &WaveAgent,
    attempt_state: AttemptState,
    final_markers: &FinalMarkerEnvelope,
    agent_error: Option<&str>,
    texts: &[String],
) -> ClosureState {
    let verdict = derive_closure_verdict_payload(agent.id.as_str(), texts);
    let mut blocking_reasons = Vec::new();
    if !final_markers.missing.is_empty() {
        blocking_reasons.push(format!(
            "missing final markers: {}",
            final_markers.missing.join(", ")
        ));
    }
    if let Some(error) = agent_error {
        blocking_reasons.push(error.to_string());
    }
    if let Some(error) = closure_contract_error(
        agent,
        &ClosureState {
            disposition: ClosureDisposition::Pending,
            required_final_markers: final_markers.required.clone(),
            observed_final_markers: final_markers.observed.clone(),
            blocking_reasons: Vec::new(),
            satisfied_fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            verdict: verdict.clone(),
        },
    ) {
        blocking_reasons.push(error);
    }

    let disposition = match attempt_state {
        AttemptState::Succeeded if final_markers.is_satisfied() && blocking_reasons.is_empty() => {
            ClosureDisposition::Ready
        }
        AttemptState::Planned | AttemptState::Running => ClosureDisposition::Pending,
        _ => ClosureDisposition::Blocked,
    };

    ClosureState {
        disposition,
        required_final_markers: final_markers.required.clone(),
        observed_final_markers: final_markers.observed.clone(),
        blocking_reasons,
        satisfied_fact_ids: Vec::new(),
        contradiction_ids: Vec::new(),
        verdict,
    }
}

fn closure_contract_error(agent: &WaveAgent, closure: &ClosureState) -> Option<String> {
    match (agent.id.as_str(), &closure.verdict) {
        ("A0", ClosureVerdictPayload::ContQa(verdict)) => {
            let Some(result) = verdict.verdict.as_deref() else {
                return Some("cont-QA report is missing final Verdict line".to_string());
            };
            if result != "PASS" {
                return Some(format!("cont-QA verdict is {result}, not PASS"));
            }
            let Some(gate_state) = verdict.gate_state.as_deref() else {
                return Some("cont-QA report is missing final [wave-gate] line".to_string());
            };
            if gate_state != "pass" {
                return Some("cont-QA gate marker is not fully pass".to_string());
            }
            None
        }
        ("A0", _) => Some("cont-QA report is missing structured closure verdict".to_string()),
        ("A8", ClosureVerdictPayload::Integration(verdict)) => match verdict.state.as_deref() {
            Some("ready-for-doc-closure") => None,
            Some(state) => Some(format!(
                "integration state is {state}, not ready-for-doc-closure"
            )),
            None => Some("integration report is missing state=<...>".to_string()),
        },
        ("A8", _) => Some("integration report is missing structured closure verdict".to_string()),
        ("A9", ClosureVerdictPayload::Documentation(verdict)) => match verdict.state.as_deref() {
            Some("closed") | Some("no-change") => None,
            Some(state) => Some(format!(
                "documentation closure state is {state}, not closed or no-change"
            )),
            None => Some("documentation closure report is missing state=<...>".to_string()),
        },
        ("A9", _) => Some("documentation report is missing structured closure verdict".to_string()),
        _ => None,
    }
}

fn derive_closure_verdict_payload(agent_id: &str, texts: &[String]) -> ClosureVerdictPayload {
    match agent_id {
        "A0" => ClosureVerdictPayload::ContQa(parse_cont_qa_verdict(texts)),
        "A8" => ClosureVerdictPayload::Integration(parse_integration_verdict(texts)),
        "A9" => ClosureVerdictPayload::Documentation(parse_documentation_verdict(texts)),
        _ => ClosureVerdictPayload::None,
    }
}

fn parse_cont_qa_verdict(texts: &[String]) -> ContQaClosureVerdict {
    let verdict = texts
        .iter()
        .flat_map(|text| text.lines())
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("Verdict:"))
        .map(str::trim)
        .map(|value| value.to_ascii_uppercase())
        .last();
    let (gate_line, gate_fields) = find_marker_fields(texts, "[wave-gate]")
        .map(|(line, fields)| (Some(line), fields))
        .unwrap_or_else(|| (None, BTreeMap::new()));
    let detail = gate_fields.get("detail").cloned();
    let gate_dimensions = gate_fields
        .iter()
        .filter(|(key, _)| key.as_str() != "detail")
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    let gate_state = cont_qa_gate_state(gate_line.as_deref(), &gate_dimensions);

    ContQaClosureVerdict {
        verdict,
        gate_state,
        gate_line,
        gate_dimensions,
        detail,
    }
}

fn parse_integration_verdict(texts: &[String]) -> IntegrationClosureVerdict {
    let fields = find_marker_fields(texts, "[wave-integration]")
        .map(|(_, fields)| fields)
        .unwrap_or_default();
    IntegrationClosureVerdict {
        state: fields.get("state").cloned(),
        claims: parse_marker_u32(&fields, "claims"),
        conflicts: parse_marker_u32(&fields, "conflicts"),
        blockers: parse_marker_u32(&fields, "blockers"),
        detail: fields.get("detail").cloned(),
    }
}

fn parse_documentation_verdict(texts: &[String]) -> DocumentationClosureVerdict {
    let fields = find_marker_fields(texts, "[wave-doc-closure]")
        .map(|(_, fields)| fields)
        .unwrap_or_default();
    DocumentationClosureVerdict {
        state: fields.get("state").cloned(),
        paths: fields
            .get("paths")
            .map(|value| split_csv(value))
            .unwrap_or_default(),
        detail: fields.get("detail").cloned(),
    }
}

fn find_marker_fields(
    texts: &[String],
    marker: &str,
) -> Option<(String, BTreeMap<String, String>)> {
    texts
        .iter()
        .flat_map(|text| text.lines())
        .map(str::trim)
        .filter(|line| *line == marker || line.starts_with(&(marker.to_string() + " ")))
        .map(|line| (line.to_string(), parse_marker_fields(line, marker)))
        .last()
}

fn parse_marker_fields(line: &str, marker: &str) -> BTreeMap<String, String> {
    line.strip_prefix(marker)
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(|token| token.split_once('='))
        .map(|(key, value)| {
            (
                key.to_string(),
                value.trim().trim_end_matches(',').to_string(),
            )
        })
        .collect()
}

fn parse_marker_u32(fields: &BTreeMap<String, String>, key: &str) -> Option<u32> {
    fields.get(key).and_then(|value| value.parse::<u32>().ok())
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn cont_qa_gate_state(
    gate_line: Option<&str>,
    gate_dimensions: &BTreeMap<String, String>,
) -> Option<String> {
    if gate_dimensions.values().any(|value| value == "blocked") {
        return Some("blocked".to_string());
    }
    if gate_dimensions.values().any(|value| value == "concerns") {
        return Some("concerns".to_string());
    }
    gate_line.map(|line| {
        let lowered = line.to_ascii_lowercase();
        if lowered.contains("blocked") {
            "blocked".to_string()
        } else if lowered.contains("concerns") {
            "concerns".to_string()
        } else {
            "pass".to_string()
        }
    })
}

fn is_marker_fallback_path(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    matches!(extension, "md" | "txt" | "json")
        && path
            .components()
            .any(|component| component.as_os_str() == ".wave")
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
    fn detects_markers_with_extra_payload() {
        let observed = observed_markers_in_text(
            "[wave-gate] detail=ok\nVerdict: PASS\n[wave-proof]",
            &["[wave-gate]".to_string(), "[wave-proof]".to_string()],
        );
        assert_eq!(
            observed,
            vec!["[wave-gate]".to_string(), "[wave-proof]".to_string()]
        );
    }

    #[test]
    fn falls_back_to_wave_owned_reports_for_markers() {
        let root =
            std::env::temp_dir().join(format!("wave-runtime-marker-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".wave/reviews")).expect("create temp review dir");
        fs::write(
            root.join(".wave/reviews/wave-0-cont-qa.md"),
            "# Review\n\n[wave-gate] detail=artifact\nVerdict: PASS\n",
        )
        .expect("write review");

        let mut agent = test_agent("A0");
        agent.file_ownership = vec![".wave/reviews/wave-0-cont-qa.md".to_string()];

        let observed = collect_observed_markers(
            &root,
            &agent,
            "Recorded review only.\n",
            &["[wave-gate]".to_string()],
        );

        assert_eq!(observed, vec!["[wave-gate]".to_string()]);
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
        assert_eq!(envelope.source, ResultEnvelopeSource::Structured);
        assert_eq!(envelope.final_markers.missing, Vec::<String>::new());
        assert_eq!(envelope.doc_delta.status, ResultPayloadStatus::Recorded);
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
        let texts = vec![
            "[wave-gate] architecture=blocked integration=pass durability=pass live=pass docs=pass detail=test\nVerdict: BLOCKED\n"
                .to_string(),
        ];
        let closure = build_closure_state(
            &agent,
            AttemptState::Succeeded,
            &FinalMarkerEnvelope::default(),
            None,
            &texts,
        );

        assert_eq!(
            closure_contract_error(&agent, &closure),
            Some("cont-QA verdict is BLOCKED, not PASS".to_string())
        );
    }

    #[test]
    fn blocks_integration_that_is_not_ready_for_doc_closure() {
        let agent = test_agent("A8");
        let texts = vec![
            "[wave-integration] state=needs-more-work claims=0 conflicts=1 blockers=1 detail=test\n"
                .to_string(),
        ];
        let closure = build_closure_state(
            &agent,
            AttemptState::Succeeded,
            &FinalMarkerEnvelope::default(),
            None,
            &texts,
        );

        assert_eq!(
            closure_contract_error(&agent, &closure),
            Some("integration state is needs-more-work, not ready-for-doc-closure".to_string())
        );
    }

    #[test]
    fn blocks_doc_closure_deltas() {
        let agent = test_agent("A9");
        let texts =
            vec!["[wave-doc-closure] state=delta paths=README.md detail=test\n".to_string()];
        let closure = build_closure_state(
            &agent,
            AttemptState::Succeeded,
            &FinalMarkerEnvelope::default(),
            None,
            &texts,
        );

        assert_eq!(
            closure_contract_error(&agent, &closure),
            Some("documentation closure state is delta, not closed or no-change".to_string())
        );
    }

    #[test]
    fn build_closure_state_records_structured_integration_verdict() {
        let agent = test_agent("A8");
        let texts = vec![
            "[wave-integration] state=ready-for-doc-closure claims=2 conflicts=0 blockers=0 detail=ok\n"
                .to_string(),
        ];
        let final_markers = FinalMarkerEnvelope::from_contract(
            vec!["[wave-integration]".to_string()],
            vec!["[wave-integration]".to_string()],
        );

        let closure = build_closure_state(
            &agent,
            AttemptState::Succeeded,
            &final_markers,
            None,
            &texts,
        );

        assert_eq!(closure.disposition, ClosureDisposition::Ready);
        match closure.verdict {
            ClosureVerdictPayload::Integration(verdict) => {
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
