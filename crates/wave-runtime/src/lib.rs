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
    let _ = clear_rerun(root, config, wave.metadata.id)?;
    let run_id = format!("wave-{:02}-{}", wave.metadata.id, now_epoch_ms()?);
    let bundle = compile_wave_bundle(root, config, wave, &run_id)?;
    let preflight = build_launch_preflight(wave, options.dry_run);
    let preflight_path = bundle.bundle_dir.join("preflight.json");
    fs::write(&preflight_path, serde_json::to_string_pretty(&preflight)?)
        .with_context(|| format!("failed to write {}", preflight_path.display()))?;
    if !preflight.ok {
        return Err(LaunchPreflightError { report: preflight }.into());
    }

    let codex_home = bootstrap_project_codex_home(root, config)?;
    let trace_path = trace_runs_dir(root, config).join(format!("{run_id}.json"));
    let state_path = state_runs_dir(root, config).join(format!("{run_id}.json"));
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
    let mut record = WaveRunRecord {
        run_id: run_id.clone(),
        wave_id: wave.metadata.id,
        slug: wave.metadata.slug.clone(),
        title: wave.metadata.title.clone(),
        status: if options.dry_run {
            WaveRunStatus::DryRun
        } else {
            WaveRunStatus::Planned
        },
        dry_run: options.dry_run,
        bundle_dir: bundle.bundle_dir.clone(),
        trace_path: trace_path.clone(),
        codex_home: codex_home.clone(),
        created_at_ms,
        started_at_ms: None,
        launcher_pid: if options.dry_run {
            None
        } else {
            Some(std::process::id())
        },
        completed_at_ms: None,
        agents: bundle
            .agents
            .iter()
            .map(|agent| AgentRunRecord {
                id: agent.id.clone(),
                title: agent.title.clone(),
                status: if options.dry_run {
                    WaveRunStatus::DryRun
                } else {
                    WaveRunStatus::Planned
                },
                prompt_path: agent.prompt_path.clone(),
                last_message_path: agent.prompt_path.parent().unwrap().join("last-message.txt"),
                events_path: agent.prompt_path.parent().unwrap().join("events.jsonl"),
                stderr_path: agent.prompt_path.parent().unwrap().join("stderr.txt"),
                expected_markers: agent.expected_markers.clone(),
                observed_markers: Vec::new(),
                exit_code: None,
                error: None,
            })
            .collect(),
        error: None,
    };

    if options.dry_run {
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
        launched.push(report);

        status = refresh_planning_status(root, config, waves)?;
        if options.dry_run {
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

    if let Some(error) = closure_contract_error(agent, &text_artifacts) {
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

fn closure_contract_error(agent: &WaveAgent, texts: &[String]) -> Option<String> {
    match agent.id.as_str() {
        "A0" => {
            let Some(verdict) = find_last_verdict(texts) else {
                return Some("cont-QA report is missing final Verdict line".to_string());
            };
            if verdict != "PASS" {
                return Some(format!("cont-QA verdict is {verdict}, not PASS"));
            }

            let Some(gate_line) = find_last_marker_line(texts, "[wave-gate]") else {
                return Some("cont-QA report is missing final [wave-gate] line".to_string());
            };
            let lowered = gate_line.to_ascii_lowercase();
            if lowered.contains("concerns") || lowered.contains("blocked") {
                return Some("cont-QA gate marker is not fully pass".to_string());
            }
            None
        }
        "A8" => match find_marker_state(texts, "[wave-integration]").as_deref() {
            Some("ready-for-doc-closure") => None,
            Some(state) => Some(format!(
                "integration state is {state}, not ready-for-doc-closure"
            )),
            None => Some("integration report is missing state=<...>".to_string()),
        },
        "A9" => match find_marker_state(texts, "[wave-doc-closure]").as_deref() {
            Some("closed") | Some("no-change") => None,
            Some(state) => Some(format!(
                "documentation closure state is {state}, not closed or no-change"
            )),
            None => Some("documentation closure report is missing state=<...>".to_string()),
        },
        _ => None,
    }
}

fn find_last_verdict(texts: &[String]) -> Option<String> {
    texts
        .iter()
        .flat_map(|text| text.lines())
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("Verdict:"))
        .map(str::trim)
        .map(|value| value.to_ascii_uppercase())
        .last()
}

fn find_last_marker_line(texts: &[String], marker: &str) -> Option<String> {
    texts
        .iter()
        .flat_map(|text| text.lines())
        .map(str::trim)
        .filter(|line| *line == marker || line.starts_with(&(marker.to_string() + " ")))
        .map(ToString::to_string)
        .last()
}

fn find_marker_state(texts: &[String], marker: &str) -> Option<String> {
    let line = find_last_marker_line(texts, marker)?;
    line.split_whitespace()
        .find_map(|segment| segment.strip_prefix("state="))
        .map(|state| state.trim_end_matches(',').to_string())
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
    let project_codex_home = root.join(&config.project_codex_home);
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
    root.join(&config.state_dir).join("build").join("specs")
}

fn state_runs_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    root.join(&config.state_dir).join("runs")
}

fn trace_runs_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    root.join(&config.trace_dir).join("runs")
}

fn state_control_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    root.join(&config.state_dir).join("control")
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
    use wave_config::ExecutionMode;
    use wave_spec::Context7Defaults;
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
    fn blocks_non_pass_cont_qa_verdicts() {
        let agent = test_agent("A0");
        let texts = vec![
            "[wave-gate] architecture=blocked integration=pass durability=pass live=pass docs=pass detail=test\nVerdict: BLOCKED\n"
                .to_string(),
        ];

        assert_eq!(
            closure_contract_error(&agent, &texts),
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

        assert_eq!(
            closure_contract_error(&agent, &texts),
            Some("integration state is needs-more-work, not ready-for-doc-closure".to_string())
        );
    }

    #[test]
    fn blocks_doc_closure_deltas() {
        let agent = test_agent("A9");
        let texts =
            vec!["[wave-doc-closure] state=delta paths=README.md detail=test\n".to_string()];

        assert_eq!(
            closure_contract_error(&agent, &texts),
            Some("documentation closure state is delta, not closed or no-change".to_string())
        );
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

    #[test]
    fn build_specs_are_rooted_in_project_state_dir() {
        let config = ProjectConfig {
            version: 1,
            project_name: "Codex Wave Mode".to_string(),
            default_lane: "main".to_string(),
            default_mode: ExecutionMode::DarkFactory,
            waves_dir: PathBuf::from("waves"),
            project_codex_home: PathBuf::from(".wave/codex"),
            state_dir: PathBuf::from(".wave/state"),
            state_runs_dir: PathBuf::from(".wave/state/runs"),
            state_control_dir: PathBuf::from(".wave/state/control"),
            trace_dir: PathBuf::from(".wave/traces"),
            trace_runs_dir: PathBuf::from(".wave/traces/runs"),
            codex_vendor_dir: PathBuf::from("third_party/codex-rs"),
            reference_wave_repo_dir: PathBuf::from("third_party/agent-wave-orchestrator"),
            dark_factory: Default::default(),
            lanes: BTreeMap::new(),
        };

        assert_eq!(
            build_specs_dir(Path::new("/repo"), &config),
            PathBuf::from("/repo/.wave/state/build/specs")
        );
    }
}
