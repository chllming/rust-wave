use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const TRACE_SCHEMA_VERSION: &str = "wave-trace/v1";
pub const RESULT_ENVELOPE_FILE_NAME: &str = "agent_result_envelope.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveRunStatus {
    Planned,
    Running,
    Succeeded,
    Failed,
    #[serde(alias = "dry-run")]
    DryRun,
}

impl fmt::Display for WaveRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Planned => "planned",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::DryRun => "dry_run",
        };
        write!(f, "{label}")
    }
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

impl AttemptState {
    pub fn from_run_status(status: WaveRunStatus, dry_run: bool) -> Self {
        if dry_run {
            return Self::Refused;
        }

        match status {
            WaveRunStatus::Planned => Self::Planned,
            WaveRunStatus::Running => Self::Running,
            WaveRunStatus::Succeeded => Self::Succeeded,
            WaveRunStatus::Failed => Self::Failed,
            WaveRunStatus::DryRun => Self::Refused,
        }
    }

    pub fn to_run_status(self) -> WaveRunStatus {
        match self {
            Self::Planned => WaveRunStatus::Planned,
            Self::Running => WaveRunStatus::Running,
            Self::Succeeded => WaveRunStatus::Succeeded,
            Self::Failed | Self::Aborted => WaveRunStatus::Failed,
            Self::Refused => WaveRunStatus::DryRun,
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosureDisposition {
    Pending,
    Ready,
    Blocked,
    Closed,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkerEvidence {
    pub marker: String,
    pub line: String,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClosureVerdictPayload {
    #[default]
    None,
    ContQa(ContQaClosureVerdict),
    Integration(IntegrationClosureVerdict),
    Documentation(DocumentationClosureVerdict),
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
pub struct DocumentationClosureVerdict {
    pub state: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofArtifact {
    pub path: String,
    pub kind: ArtifactKind,
    pub digest: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosureState {
    pub disposition: ClosureDisposition,
    pub required_final_markers: Vec<String>,
    pub observed_final_markers: Vec<String>,
    pub blocking_reasons: Vec<String>,
    pub satisfied_fact_ids: Vec<String>,
    pub contradiction_ids: Vec<String>,
    #[serde(default)]
    pub verdict: ClosureVerdictPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultEnvelopeRecord {
    pub result_envelope_id: String,
    pub wave_id: u32,
    pub task_id: String,
    pub attempt_id: String,
    pub agent_id: String,
    pub task_role: String,
    pub closure_role: Option<String>,
    #[serde(default)]
    pub source: ResultEnvelopeSource,
    pub attempt_state: AttemptState,
    pub disposition: ResultDisposition,
    pub summary: Option<String>,
    pub output_text: Option<String>,
    pub final_markers: FinalMarkerEnvelope,
    #[serde(default)]
    pub proof_bundle_ids: Vec<String>,
    #[serde(default)]
    pub fact_ids: Vec<String>,
    #[serde(default)]
    pub contradiction_ids: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<ProofArtifact>,
    #[serde(default)]
    pub doc_delta: DocDeltaEnvelope,
    #[serde(default)]
    pub marker_evidence: Vec<MarkerEvidence>,
    pub closure: ClosureState,
    pub created_at_ms: u128,
}

impl ResultEnvelopeRecord {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.attempt_state,
            AttemptState::Succeeded
                | AttemptState::Failed
                | AttemptState::Aborted
                | AttemptState::Refused
        )
    }

    pub fn proof_source_label(&self) -> &'static str {
        match self.source {
            ResultEnvelopeSource::Structured => "structured-envelope",
            ResultEnvelopeSource::LegacyMarkerAdapter => "compatibility-adapter",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveResultEnvelope {
    pub envelope: ResultEnvelopeRecord,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledAgentPrompt {
    pub id: String,
    pub title: String,
    pub prompt_path: PathBuf,
    pub expected_markers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftBundle {
    pub run_id: String,
    pub wave_id: u32,
    pub slug: String,
    pub title: String,
    pub bundle_dir: PathBuf,
    pub agents: Vec<CompiledAgentPrompt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunRecord {
    pub id: String,
    pub title: String,
    pub status: WaveRunStatus,
    pub prompt_path: PathBuf,
    pub last_message_path: PathBuf,
    pub events_path: PathBuf,
    pub stderr_path: PathBuf,
    #[serde(default)]
    pub result_envelope_path: Option<PathBuf>,
    pub expected_markers: Vec<String>,
    pub observed_markers: Vec<String>,
    pub exit_code: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveRunRecord {
    pub run_id: String,
    pub wave_id: u32,
    pub slug: String,
    pub title: String,
    pub status: WaveRunStatus,
    pub dry_run: bool,
    pub bundle_dir: PathBuf,
    pub trace_path: PathBuf,
    pub codex_home: PathBuf,
    pub created_at_ms: u128,
    pub started_at_ms: Option<u128>,
    #[serde(default)]
    pub launcher_pid: Option<u32>,
    pub completed_at_ms: Option<u128>,
    pub agents: Vec<AgentRunRecord>,
    pub error: Option<String>,
}

impl WaveRunRecord {
    pub fn completed_successfully(&self) -> bool {
        matches!(self.status, WaveRunStatus::Succeeded)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayIssue {
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayReport {
    pub run_id: String,
    pub wave_id: u32,
    pub ok: bool,
    pub issues: Vec<ReplayIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfHostEvidenceItem {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfHostEvidenceReport {
    pub wave_id: u32,
    pub run_id: String,
    pub recorded: bool,
    pub replay: ReplayReport,
    pub operator_help_required: bool,
    pub help_items: Vec<SelfHostEvidenceItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceAgentArtifactRecord {
    pub id: String,
    pub prompt_exists: bool,
    pub last_message_exists: bool,
    pub events_exists: bool,
    pub stderr_exists: bool,
    #[serde(default)]
    pub artifacts: Vec<TraceArtifactRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceArtifactRecord {
    pub kind: String,
    pub path: PathBuf,
    pub exists: bool,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceBundleV1 {
    pub schema_version: String,
    pub recorded_at_ms: u128,
    pub run: WaveRunRecord,
    #[serde(default)]
    pub self_host_evidence: Option<SelfHostEvidenceReport>,
    #[serde(default)]
    pub agent_artifacts: Vec<TraceAgentArtifactRecord>,
    #[serde(default)]
    pub run_artifacts: Vec<TraceArtifactRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StoredTraceBundle {
    V1(TraceBundleV1),
    LegacyRunRecord(WaveRunRecord),
}

impl StoredTraceBundle {
    fn run(&self) -> &WaveRunRecord {
        match self {
            Self::V1(bundle) => &bundle.run,
            Self::LegacyRunRecord(record) => record,
        }
    }
}

pub fn now_epoch_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the unix epoch")?
        .as_millis())
}

pub fn write_run_record(path: &Path, record: &WaveRunRecord) -> Result<()> {
    let mut normalized = record.clone();
    if let Some(repo_root) = repo_root_from_authority_path(path) {
        normalize_run_record_paths_for_storage(&mut normalized, &repo_root);
    }
    write_json(path, &normalized)
}

pub fn load_run_record(path: &Path) -> Result<WaveRunRecord> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read run record {}", path.display()))?;
    let mut record = serde_json::from_str::<WaveRunRecord>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if let Some(repo_root) = repo_root_from_authority_path(path) {
        normalize_run_record_paths(&mut record, &repo_root);
    }
    Ok(record)
}

pub fn load_latest_run_records_by_wave(dir: &Path) -> Result<HashMap<u32, WaveRunRecord>> {
    let mut latest = HashMap::new();
    if !dir.exists() {
        return Ok(latest);
    }

    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let record = load_run_record(&path)?;
        match latest.get(&record.wave_id) {
            Some(existing) if !is_newer_record(&record, existing) => {}
            _ => {
                latest.insert(record.wave_id, record);
            }
        }
    }

    Ok(latest)
}

pub fn write_trace_bundle(path: &Path, record: &WaveRunRecord) -> Result<()> {
    let mut bundle = TraceBundleV1 {
        schema_version: TRACE_SCHEMA_VERSION.to_string(),
        recorded_at_ms: now_epoch_ms()?,
        run: record.clone(),
        self_host_evidence: Some(self_host_evidence(record)),
        agent_artifacts: record.agents.iter().map(snapshot_agent_artifacts).collect(),
        run_artifacts: snapshot_run_artifacts(record),
    };
    if let Some(repo_root) = repo_root_from_authority_path(path) {
        normalize_trace_bundle_paths_for_storage(&mut bundle, &repo_root);
    }
    write_json(path, &bundle)
}

pub fn write_result_envelope(path: &Path, envelope: &ResultEnvelopeRecord) -> Result<()> {
    let mut normalized = envelope.clone();
    if let Some(repo_root) = repo_root_from_authority_path(path) {
        normalize_result_envelope_for_storage(&mut normalized, &repo_root);
    }
    write_json(path, &normalized)
}

pub fn load_result_envelope(path: &Path) -> Result<ResultEnvelopeRecord> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read result envelope {}", path.display()))?;
    let mut envelope = serde_json::from_str::<ResultEnvelopeRecord>(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    if let Some(repo_root) = repo_root_from_authority_path(path) {
        normalize_result_envelope_paths(&mut envelope, &repo_root);
    }
    Ok(envelope)
}

pub fn load_effective_result_envelope(
    run: &WaveRunRecord,
    agent: &AgentRunRecord,
) -> Result<EffectiveResultEnvelope> {
    if let Some(path) = agent
        .result_envelope_path
        .as_ref()
        .filter(|path| path.exists())
    {
        return Ok(EffectiveResultEnvelope {
            envelope: load_result_envelope(path)?,
            path: Some(path.clone()),
        });
    }

    Ok(EffectiveResultEnvelope {
        envelope: adapt_legacy_result_envelope(run, agent)?,
        path: agent.result_envelope_path.clone(),
    })
}

pub fn load_trace_bundle(path: &Path) -> Result<Option<TraceBundleV1>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read trace bundle {}", path.display()))?;
    let repo_root = repo_root_from_authority_path(path);
    if let Ok(mut bundle) = serde_json::from_str::<TraceBundleV1>(&raw) {
        if let Some(repo_root) = &repo_root {
            normalize_trace_bundle_paths(&mut bundle, repo_root);
        }
        return Ok(Some(bundle));
    }
    if let Ok(mut record) = serde_json::from_str::<WaveRunRecord>(&raw) {
        if let Some(repo_root) = &repo_root {
            normalize_run_record_paths(&mut record, repo_root);
        }
        return Ok(Some(TraceBundleV1 {
            schema_version: TRACE_SCHEMA_VERSION.to_string(),
            recorded_at_ms: record.completed_at_ms.unwrap_or(record.created_at_ms),
            self_host_evidence: Some(self_host_evidence(&record)),
            agent_artifacts: record.agents.iter().map(snapshot_agent_artifacts).collect(),
            run_artifacts: snapshot_run_artifacts(&record),
            run: record,
        }));
    }
    serde_json::from_str::<serde_json::Value>(&raw)
        .context("trace JSON is malformed")
        .and_then(|_| anyhow::bail!("trace JSON did not match v1 or legacy formats"))
}

pub fn validate_replay(record: &WaveRunRecord) -> ReplayReport {
    let mut issues = Vec::new();
    let trace_path = &record.trace_path;
    if !trace_path.exists() {
        if matches!(
            record.status,
            WaveRunStatus::Planned | WaveRunStatus::Running
        ) {
            return replay_report(record, issues);
        }
        issues.push(ReplayIssue {
            kind: "missing_trace_bundle".to_string(),
            detail: format!("expected {}", trace_path.display()),
        });
        return replay_report(record, issues);
    }

    let raw = match fs::read_to_string(trace_path) {
        Ok(raw) => raw,
        Err(error) => {
            issues.push(ReplayIssue {
                kind: "trace_bundle_read_failed".to_string(),
                detail: format!("{} ({error})", trace_path.display()),
            });
            return replay_report(record, issues);
        }
    };

    let stored = match parse_stored_trace_bundle(&raw) {
        Ok(bundle) => bundle,
        Err(error) => {
            issues.push(ReplayIssue {
                kind: "trace_bundle_parse_failed".to_string(),
                detail: format!("{} ({error})", trace_path.display()),
            });
            return replay_report(record, issues);
        }
    };

    if let StoredTraceBundle::V1(bundle) = &stored {
        if bundle.schema_version != TRACE_SCHEMA_VERSION {
            issues.push(ReplayIssue {
                kind: "trace_bundle_schema_mismatch".to_string(),
                detail: format!(
                    "expected {}, found {}",
                    TRACE_SCHEMA_VERSION, bundle.schema_version
                ),
            });
        }
    }

    compare_run_records(record, stored.run(), &mut issues);
    if let StoredTraceBundle::V1(bundle) = &stored {
        compare_artifact_snapshots(record, bundle, &mut issues);
        compare_run_artifacts(record, bundle, &mut issues);
    }

    replay_report(record, issues)
}

pub fn self_host_evidence(record: &WaveRunRecord) -> SelfHostEvidenceReport {
    let replay = validate_replay(record);
    let mut help_items = vec![
        SelfHostEvidenceItem {
            name: "codex-binary".to_string(),
            ok: true,
            detail: "runtime observed a completed local run record, not a synthetic fixture"
                .to_string(),
        },
        SelfHostEvidenceItem {
            name: "trace-bundle".to_string(),
            ok: record.trace_path.exists(),
            detail: format!("trace path {}", record.trace_path.display()),
        },
    ];

    for agent in &record.agents {
        help_items.push(SelfHostEvidenceItem {
            name: format!("agent-{}-artifacts", agent.id),
            ok: agent.prompt_path.exists()
                && agent.last_message_path.exists()
                && agent.events_path.exists()
                && agent.stderr_path.exists(),
            detail: format!(
                "prompt={} last_message={} events={} stderr={}",
                agent.prompt_path.display(),
                agent.last_message_path.display(),
                agent.events_path.display(),
                agent.stderr_path.display()
            ),
        });
        if let Some(result_envelope_path) = &agent.result_envelope_path {
            help_items.push(SelfHostEvidenceItem {
                name: format!("agent-{}-result-envelope", agent.id),
                ok: result_envelope_path.exists(),
                detail: format!("result envelope {}", result_envelope_path.display()),
            });
        }
    }

    if !record.completed_successfully() {
        help_items.push(SelfHostEvidenceItem {
            name: "operator-help".to_string(),
            ok: false,
            detail: "run did not complete cleanly; operator intervention still required"
                .to_string(),
        });
    }

    let operator_help_required = help_items.iter().any(|item| !item.ok) || !replay.ok;
    SelfHostEvidenceReport {
        wave_id: record.wave_id,
        run_id: record.run_id.clone(),
        recorded: record.trace_path.exists(),
        replay,
        operator_help_required,
        help_items,
    }
}

fn replay_report(record: &WaveRunRecord, issues: Vec<ReplayIssue>) -> ReplayReport {
    ReplayReport {
        run_id: record.run_id.clone(),
        wave_id: record.wave_id,
        ok: issues.is_empty(),
        issues,
    }
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

fn normalize_trace_bundle_paths_for_storage(bundle: &mut TraceBundleV1, repo_root: &Path) {
    normalize_run_record_paths_for_storage(&mut bundle.run, repo_root);
    for agent in &mut bundle.agent_artifacts {
        for artifact in &mut agent.artifacts {
            normalize_repo_path_for_storage(&mut artifact.path, repo_root);
        }
    }
    for artifact in &mut bundle.run_artifacts {
        normalize_repo_path_for_storage(&mut artifact.path, repo_root);
    }
}

fn normalize_trace_bundle_paths(bundle: &mut TraceBundleV1, repo_root: &Path) {
    normalize_run_record_paths(&mut bundle.run, repo_root);
    for agent in &mut bundle.agent_artifacts {
        for artifact in &mut agent.artifacts {
            normalize_repo_relative_path(&mut artifact.path, repo_root);
        }
    }
    for artifact in &mut bundle.run_artifacts {
        normalize_repo_relative_path(&mut artifact.path, repo_root);
    }
}

fn normalize_run_record_paths_for_storage(record: &mut WaveRunRecord, repo_root: &Path) {
    normalize_repo_path_for_storage(&mut record.bundle_dir, repo_root);
    normalize_repo_path_for_storage(&mut record.trace_path, repo_root);
    normalize_repo_path_for_storage(&mut record.codex_home, repo_root);
    for agent in &mut record.agents {
        normalize_repo_path_for_storage(&mut agent.prompt_path, repo_root);
        normalize_repo_path_for_storage(&mut agent.last_message_path, repo_root);
        normalize_repo_path_for_storage(&mut agent.events_path, repo_root);
        normalize_repo_path_for_storage(&mut agent.stderr_path, repo_root);
        if let Some(path) = &mut agent.result_envelope_path {
            normalize_repo_path_for_storage(path, repo_root);
        }
    }
}

fn normalize_run_record_paths(record: &mut WaveRunRecord, repo_root: &Path) {
    normalize_repo_relative_path(&mut record.bundle_dir, repo_root);
    normalize_repo_relative_path(&mut record.trace_path, repo_root);
    normalize_repo_relative_path(&mut record.codex_home, repo_root);
    for agent in &mut record.agents {
        normalize_repo_relative_path(&mut agent.prompt_path, repo_root);
        normalize_repo_relative_path(&mut agent.last_message_path, repo_root);
        normalize_repo_relative_path(&mut agent.events_path, repo_root);
        normalize_repo_relative_path(&mut agent.stderr_path, repo_root);
        if let Some(path) = &mut agent.result_envelope_path {
            normalize_repo_relative_path(path, repo_root);
        }
    }
}

fn normalize_repo_relative_path(path: &mut PathBuf, repo_root: &Path) {
    if path.is_relative() {
        *path = repo_root.join(path.as_path());
    }
}

fn normalize_repo_path_for_storage(path: &mut PathBuf, repo_root: &Path) {
    if path.is_absolute() {
        *path = path
            .strip_prefix(repo_root)
            .unwrap_or(path.as_path())
            .to_path_buf();
    }
}

fn normalize_result_envelope_for_storage(envelope: &mut ResultEnvelopeRecord, repo_root: &Path) {
    for artifact in &mut envelope.artifacts {
        normalize_string_path_for_storage(&mut artifact.path, repo_root);
    }
    for path in &mut envelope.doc_delta.paths {
        normalize_string_path_for_storage(path, repo_root);
    }
    for evidence in &mut envelope.marker_evidence {
        if let Some(source) = &mut evidence.source {
            normalize_string_path_for_storage(source, repo_root);
        }
    }
}

fn normalize_result_envelope_paths(envelope: &mut ResultEnvelopeRecord, repo_root: &Path) {
    for artifact in &mut envelope.artifacts {
        normalize_string_path_to_absolute(&mut artifact.path, repo_root);
    }
    for path in &mut envelope.doc_delta.paths {
        normalize_string_path_to_absolute(path, repo_root);
    }
    for evidence in &mut envelope.marker_evidence {
        if let Some(source) = &mut evidence.source {
            normalize_string_path_to_absolute(source, repo_root);
        }
    }
}

fn parse_stored_trace_bundle(raw: &str) -> Result<StoredTraceBundle> {
    if let Ok(bundle) = serde_json::from_str::<TraceBundleV1>(raw) {
        return Ok(StoredTraceBundle::V1(bundle));
    }
    if let Ok(record) = serde_json::from_str::<WaveRunRecord>(raw) {
        return Ok(StoredTraceBundle::LegacyRunRecord(record));
    }
    serde_json::from_str::<serde_json::Value>(raw)
        .context("trace JSON is malformed")
        .and_then(|_| anyhow::bail!("trace JSON did not match v1 or legacy formats"))
}

fn adapt_legacy_result_envelope(
    run: &WaveRunRecord,
    agent: &AgentRunRecord,
) -> Result<ResultEnvelopeRecord> {
    let attempt_state = AttemptState::from_run_status(agent.status, run.dry_run);
    let final_markers = FinalMarkerEnvelope::from_contract(
        agent.expected_markers.clone(),
        agent.observed_markers.clone(),
    );
    let output_text = read_optional_text(&agent.last_message_path)?;
    let marker_evidence = collect_marker_evidence(
        output_text.as_deref(),
        &final_markers.observed,
        &agent.last_message_path,
        &run.run_id,
    );
    let closure = ClosureState {
        disposition: legacy_closure_disposition(attempt_state, &final_markers),
        required_final_markers: final_markers.required.clone(),
        observed_final_markers: final_markers.observed.clone(),
        blocking_reasons: legacy_blocking_reasons(attempt_state, &final_markers, agent),
        satisfied_fact_ids: Vec::new(),
        contradiction_ids: Vec::new(),
        verdict: derive_closure_verdict_payload(agent.id.as_str(), output_text.as_deref()),
    };

    Ok(ResultEnvelopeRecord {
        result_envelope_id: format!("legacy:{}:{}", run.run_id, agent.id.to_ascii_lowercase()),
        wave_id: run.wave_id,
        task_id: task_id_for_agent(run.wave_id, &agent.id),
        attempt_id: format!("legacy-{}-{}", run.run_id, agent.id.to_ascii_lowercase()),
        agent_id: agent.id.clone(),
        task_role: inferred_task_role_for_agent(&agent.id),
        closure_role: inferred_closure_role_for_agent(&agent.id),
        source: ResultEnvelopeSource::LegacyMarkerAdapter,
        attempt_state,
        disposition: ResultDisposition::from_attempt_state(
            attempt_state,
            final_markers.missing.len(),
        ),
        summary: agent
            .error
            .clone()
            .or_else(|| Some(format!("adapted from legacy run {}", run.run_id))),
        output_text,
        final_markers,
        proof_bundle_ids: Vec::new(),
        fact_ids: Vec::new(),
        contradiction_ids: Vec::new(),
        artifacts: Vec::new(),
        doc_delta: legacy_doc_delta_payload(agent),
        marker_evidence,
        closure,
        created_at_ms: run
            .completed_at_ms
            .or(run.started_at_ms)
            .unwrap_or(run.created_at_ms),
    })
}

fn legacy_closure_disposition(
    attempt_state: AttemptState,
    final_markers: &FinalMarkerEnvelope,
) -> ClosureDisposition {
    match attempt_state {
        AttemptState::Succeeded if final_markers.is_satisfied() => ClosureDisposition::Ready,
        AttemptState::Planned | AttemptState::Running => ClosureDisposition::Pending,
        _ => ClosureDisposition::Blocked,
    }
}

fn derive_closure_verdict_payload(
    agent_id: &str,
    output_text: Option<&str>,
) -> ClosureVerdictPayload {
    let Some(output_text) = output_text else {
        return ClosureVerdictPayload::None;
    };

    match agent_id {
        "A0" => ClosureVerdictPayload::ContQa(parse_cont_qa_verdict(output_text)),
        "A8" => ClosureVerdictPayload::Integration(parse_integration_verdict(output_text)),
        "A9" => ClosureVerdictPayload::Documentation(parse_documentation_verdict(output_text)),
        _ => ClosureVerdictPayload::None,
    }
}

fn parse_cont_qa_verdict(output_text: &str) -> ContQaClosureVerdict {
    let verdict = output_text
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("Verdict:"))
        .map(str::trim)
        .map(|value| value.to_ascii_uppercase())
        .last();
    let (gate_line, gate_fields) = find_marker_fields(output_text, "[wave-gate]")
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

fn parse_integration_verdict(output_text: &str) -> IntegrationClosureVerdict {
    let (_, fields) = find_marker_fields(output_text, "[wave-integration]")
        .map(|(line, fields)| (Some(line), fields))
        .unwrap_or_else(|| (None, BTreeMap::new()));
    IntegrationClosureVerdict {
        state: fields.get("state").cloned(),
        claims: parse_marker_u32(&fields, "claims"),
        conflicts: parse_marker_u32(&fields, "conflicts"),
        blockers: parse_marker_u32(&fields, "blockers"),
        detail: fields.get("detail").cloned(),
    }
}

fn parse_documentation_verdict(output_text: &str) -> DocumentationClosureVerdict {
    let (_, fields) = find_marker_fields(output_text, "[wave-doc-closure]")
        .map(|(line, fields)| (Some(line), fields))
        .unwrap_or_else(|| (None, BTreeMap::new()));
    DocumentationClosureVerdict {
        state: fields.get("state").cloned(),
        paths: fields
            .get("paths")
            .map(|value| split_csv(value))
            .unwrap_or_default(),
        detail: fields.get("detail").cloned(),
    }
}

fn find_marker_fields(text: &str, marker: &str) -> Option<(String, BTreeMap<String, String>)> {
    text.lines()
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

fn legacy_blocking_reasons(
    attempt_state: AttemptState,
    final_markers: &FinalMarkerEnvelope,
    agent: &AgentRunRecord,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if !final_markers.missing.is_empty() {
        reasons.push(format!(
            "missing final markers: {}",
            final_markers.missing.join(", ")
        ));
    }
    match attempt_state {
        AttemptState::Failed => reasons.push("legacy attempt failed".to_string()),
        AttemptState::Aborted => reasons.push("legacy attempt aborted".to_string()),
        AttemptState::Refused => reasons.push("legacy attempt was refused".to_string()),
        AttemptState::Running => reasons.push("legacy attempt is still running".to_string()),
        AttemptState::Planned => reasons.push("legacy attempt did not start".to_string()),
        AttemptState::Succeeded => {}
    }
    if let Some(error) = &agent.error {
        reasons.push(error.clone());
    }
    reasons
}

fn legacy_doc_delta_payload(agent: &AgentRunRecord) -> DocDeltaEnvelope {
    let observed = agent
        .observed_markers
        .iter()
        .any(|marker| marker == "[wave-doc-delta]");
    let required = agent
        .expected_markers
        .iter()
        .any(|marker| marker == "[wave-doc-delta]");
    let status = if observed {
        ResultPayloadStatus::EvidenceOnly
    } else if required {
        ResultPayloadStatus::Missing
    } else {
        ResultPayloadStatus::Missing
    };

    DocDeltaEnvelope {
        status,
        summary: None,
        paths: Vec::new(),
    }
}

fn collect_marker_evidence(
    output_text: Option<&str>,
    observed_markers: &[String],
    source_path: &Path,
    run_id: &str,
) -> Vec<MarkerEvidence> {
    let source = Some(source_path.to_string_lossy().replace('\\', "/"));
    let mut evidence = Vec::new();

    if let Some(text) = output_text {
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            for marker in observed_markers {
                if line == marker || line.starts_with(&(marker.clone() + " ")) {
                    evidence.push(MarkerEvidence {
                        marker: marker.clone(),
                        line: line.to_string(),
                        source: source.clone(),
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
                source: Some(format!("legacy-run-record:{run_id}")),
            });
        }
    }

    normalize_marker_evidence(evidence)
}

fn normalize_marker_evidence(mut evidence: Vec<MarkerEvidence>) -> Vec<MarkerEvidence> {
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

fn read_optional_text(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))
        .map(Some)
}

fn task_id_for_agent(wave_id: u32, agent_id: &str) -> String {
    format!("wave-{wave_id:02}:agent-{}", agent_id.to_ascii_lowercase())
}

fn inferred_closure_role_for_agent(agent_id: &str) -> Option<String> {
    match agent_id {
        "E0" => Some("cont_eval".to_string()),
        "A8" => Some("integration".to_string()),
        "A9" => Some("documentation".to_string()),
        "A0" => Some("cont_qa".to_string()),
        _ => None,
    }
}

fn inferred_task_role_for_agent(agent_id: &str) -> String {
    match agent_id {
        "A8" => "integration",
        "A9" => "documentation",
        "A0" => "cont_qa",
        "E0" => "cont_eval",
        _ => "implementation",
    }
    .to_string()
}

fn normalize_string_path_for_storage(path: &mut String, repo_root: &Path) {
    let path_buf = PathBuf::from(path.as_str());
    if path_buf.is_absolute() {
        *path = path_buf
            .strip_prefix(repo_root)
            .unwrap_or(path_buf.as_path())
            .to_string_lossy()
            .replace('\\', "/");
    } else {
        *path = path_buf.to_string_lossy().replace('\\', "/");
    }
}

fn normalize_string_path_to_absolute(path: &mut String, repo_root: &Path) {
    let path_buf = PathBuf::from(path.as_str());
    if path_buf.is_relative()
        && !path.starts_with("legacy-run-record:")
        && !path.starts_with("inline:")
    {
        *path = repo_root
            .join(path_buf)
            .to_string_lossy()
            .replace('\\', "/");
    } else {
        *path = path_buf.to_string_lossy().replace('\\', "/");
    }
}

fn dedup_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut unique = Vec::new();
    for value in values {
        if !unique.iter().any(|existing| existing == &value) {
            unique.push(value);
        }
    }
    unique
}

fn snapshot_agent_artifacts(agent: &AgentRunRecord) -> TraceAgentArtifactRecord {
    let mut artifacts = vec![
        snapshot_artifact("prompt", &agent.prompt_path),
        snapshot_artifact("last_message", &agent.last_message_path),
        snapshot_artifact("events", &agent.events_path),
        snapshot_artifact("stderr", &agent.stderr_path),
    ];
    if let Some(result_envelope_path) = &agent.result_envelope_path {
        artifacts.push(snapshot_artifact("result_envelope", result_envelope_path));
    }

    TraceAgentArtifactRecord {
        id: agent.id.clone(),
        prompt_exists: agent.prompt_path.exists(),
        last_message_exists: agent.last_message_path.exists(),
        events_exists: agent.events_path.exists(),
        stderr_exists: agent.stderr_path.exists(),
        artifacts,
    }
}

fn snapshot_run_artifacts(record: &WaveRunRecord) -> Vec<TraceArtifactRecord> {
    vec![
        snapshot_artifact("bundle_dir", &record.bundle_dir),
        snapshot_artifact("codex_home", &record.codex_home),
    ]
}

fn snapshot_artifact(kind: &str, path: &Path) -> TraceArtifactRecord {
    let metadata = fs::metadata(path).ok();
    TraceArtifactRecord {
        kind: kind.to_string(),
        path: path.to_path_buf(),
        exists: metadata.is_some(),
        bytes: metadata.map(|meta| meta.len()),
    }
}

fn compare_run_records(
    current: &WaveRunRecord,
    stored: &WaveRunRecord,
    issues: &mut Vec<ReplayIssue>,
) {
    if current.run_id != stored.run_id {
        issues.push(ReplayIssue {
            kind: "trace_run_id_mismatch".to_string(),
            detail: format!("current={} stored={}", current.run_id, stored.run_id),
        });
    }
    if current.wave_id != stored.wave_id {
        issues.push(ReplayIssue {
            kind: "trace_wave_id_mismatch".to_string(),
            detail: format!("current={} stored={}", current.wave_id, stored.wave_id),
        });
    }
    if current.status != stored.status {
        issues.push(ReplayIssue {
            kind: "trace_status_mismatch".to_string(),
            detail: format!("current={} stored={}", current.status, stored.status),
        });
    }
    if current.agents.len() != stored.agents.len() {
        issues.push(ReplayIssue {
            kind: "trace_agent_count_mismatch".to_string(),
            detail: format!(
                "current={} stored={}",
                current.agents.len(),
                stored.agents.len()
            ),
        });
    }

    for (index, current_agent) in current.agents.iter().enumerate() {
        let Some(stored_agent) = stored.agents.get(index) else {
            break;
        };
        if current_agent.id != stored_agent.id {
            issues.push(ReplayIssue {
                kind: "trace_agent_id_mismatch".to_string(),
                detail: format!(
                    "index={} current={} stored={}",
                    index, current_agent.id, stored_agent.id
                ),
            });
        }
        if current_agent.status != stored_agent.status {
            issues.push(ReplayIssue {
                kind: "trace_agent_status_mismatch".to_string(),
                detail: format!(
                    "agent={} current={} stored={}",
                    current_agent.id, current_agent.status, stored_agent.status
                ),
            });
        }
        if current_agent.observed_markers != stored_agent.observed_markers {
            issues.push(ReplayIssue {
                kind: "trace_agent_marker_mismatch".to_string(),
                detail: format!("agent={} observed markers diverged", current_agent.id),
            });
        }
        if current_agent.exit_code != stored_agent.exit_code {
            issues.push(ReplayIssue {
                kind: "trace_agent_exit_code_mismatch".to_string(),
                detail: format!(
                    "agent={} current={:?} stored={:?}",
                    current_agent.id, current_agent.exit_code, stored_agent.exit_code
                ),
            });
        }
        if current_agent.result_envelope_path != stored_agent.result_envelope_path {
            issues.push(ReplayIssue {
                kind: "trace_agent_result_envelope_mismatch".to_string(),
                detail: format!("agent={} result envelope path diverged", current_agent.id),
            });
        }
    }
}

fn compare_artifact_snapshots(
    current: &WaveRunRecord,
    bundle: &TraceBundleV1,
    issues: &mut Vec<ReplayIssue>,
) {
    if current.agents.len() != bundle.agent_artifacts.len() {
        issues.push(ReplayIssue {
            kind: "trace_artifact_count_mismatch".to_string(),
            detail: format!(
                "current={} stored={}",
                current.agents.len(),
                bundle.agent_artifacts.len()
            ),
        });
        return;
    }

    for (agent, stored_artifacts) in current.agents.iter().zip(&bundle.agent_artifacts) {
        if agent.id != stored_artifacts.id {
            issues.push(ReplayIssue {
                kind: "trace_artifact_agent_mismatch".to_string(),
                detail: format!("expected agent {}, found {}", agent.id, stored_artifacts.id),
            });
            continue;
        }

        let current_artifacts = snapshot_agent_artifacts(agent);
        compare_artifact_flag(
            "prompt",
            &agent.id,
            stored_artifacts.prompt_exists,
            current_artifacts.prompt_exists,
            issues,
        );
        compare_artifact_flag(
            "last_message",
            &agent.id,
            stored_artifacts.last_message_exists,
            current_artifacts.last_message_exists,
            issues,
        );
        compare_artifact_flag(
            "events",
            &agent.id,
            stored_artifacts.events_exists,
            current_artifacts.events_exists,
            issues,
        );
        compare_artifact_flag(
            "stderr",
            &agent.id,
            stored_artifacts.stderr_exists,
            current_artifacts.stderr_exists,
            issues,
        );

        if artifact_required_for_status(agent.status) && !current_artifacts.last_message_exists {
            issues.push(ReplayIssue {
                kind: "trace_required_artifact_missing".to_string(),
                detail: format!("agent={} missing last_message.txt", agent.id),
            });
        }
        if artifact_required_for_status(agent.status) && !current_artifacts.events_exists {
            issues.push(ReplayIssue {
                kind: "trace_required_artifact_missing".to_string(),
                detail: format!("agent={} missing events.jsonl", agent.id),
            });
        }
        if artifact_required_for_status(agent.status) && !current_artifacts.stderr_exists {
            issues.push(ReplayIssue {
                kind: "trace_required_artifact_missing".to_string(),
                detail: format!("agent={} missing stderr.txt", agent.id),
            });
        }
    }
}

fn compare_run_artifacts(
    current: &WaveRunRecord,
    bundle: &TraceBundleV1,
    issues: &mut Vec<ReplayIssue>,
) {
    let expected_bundle = bundle
        .run_artifacts
        .iter()
        .find(|artifact| artifact.kind == "trace_bundle")
        .map(|artifact| artifact.path.clone())
        .unwrap_or_else(|| current.trace_path.clone());
    if current.trace_path != expected_bundle {
        issues.push(ReplayIssue {
            kind: "trace_bundle_path_mismatch".to_string(),
            detail: format!(
                "current={} stored={}",
                current.trace_path.display(),
                expected_bundle.display()
            ),
        });
    }

    for stored in &bundle.run_artifacts {
        let current_artifact = snapshot_artifact(&stored.kind, &stored.path);
        compare_run_artifact_flag(stored, &current_artifact, issues);
        if stored.bytes != current_artifact.bytes {
            issues.push(ReplayIssue {
                kind: "trace_artifact_size_mismatch".to_string(),
                detail: format!(
                    "kind={} path={} stored={:?} current={:?}",
                    stored.kind,
                    stored.path.display(),
                    stored.bytes,
                    current_artifact.bytes
                ),
            });
        }
    }
}

fn compare_run_artifact_flag(
    stored: &TraceArtifactRecord,
    current: &TraceArtifactRecord,
    issues: &mut Vec<ReplayIssue>,
) {
    if stored.exists != current.exists {
        issues.push(ReplayIssue {
            kind: "trace_artifact_mismatch".to_string(),
            detail: format!(
                "kind={} path={} stored={} current={}",
                stored.kind,
                stored.path.display(),
                stored.exists,
                current.exists
            ),
        });
    }
}

fn compare_artifact_flag(
    name: &str,
    agent_id: &str,
    stored: bool,
    current: bool,
    issues: &mut Vec<ReplayIssue>,
) {
    if stored != current {
        issues.push(ReplayIssue {
            kind: "trace_artifact_mismatch".to_string(),
            detail: format!(
                "agent={} artifact={} stored={} current={}",
                agent_id, name, stored, current
            ),
        });
    }
}

fn artifact_required_for_status(status: WaveRunStatus) -> bool {
    matches!(status, WaveRunStatus::Succeeded | WaveRunStatus::Failed)
}

fn is_newer_record(candidate: &WaveRunRecord, current: &WaveRunRecord) -> bool {
    (
        candidate.created_at_ms,
        candidate.started_at_ms.unwrap_or_default(),
        candidate.completed_at_ms.unwrap_or_default(),
    ) > (
        current.created_at_ms,
        current.started_at_ms.unwrap_or_default(),
        current.completed_at_ms.unwrap_or_default(),
    )
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(value)?;
    fs::write(path, raw).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_agent(root: &Path, status: WaveRunStatus) -> AgentRunRecord {
        let agent_dir = root.join("agents/A1");
        AgentRunRecord {
            id: "A1".to_string(),
            title: "Impl".to_string(),
            status,
            prompt_path: agent_dir.join("prompt.md"),
            last_message_path: agent_dir.join("last-message.txt"),
            events_path: agent_dir.join("events.jsonl"),
            stderr_path: agent_dir.join("stderr.txt"),
            result_envelope_path: None,
            expected_markers: vec!["[wave-proof]".to_string()],
            observed_markers: vec!["[wave-proof]".to_string()],
            exit_code: Some(0),
            error: None,
        }
    }

    fn sample_record(root: &Path, status: WaveRunStatus) -> WaveRunRecord {
        WaveRunRecord {
            run_id: "wave-8-1".to_string(),
            wave_id: 8,
            slug: "trace-replay".to_string(),
            title: "Trace Replay".to_string(),
            status,
            dry_run: false,
            bundle_dir: root.join("bundle"),
            trace_path: root.join("trace.json"),
            codex_home: root.join("codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: Some(1234),
            completed_at_ms: Some(3),
            agents: vec![sample_agent(root, status)],
            error: None,
        }
    }

    #[test]
    fn latest_run_records_choose_newest_created_at() {
        let dir = tempdir().expect("tempdir");
        let mut older = sample_record(dir.path(), WaveRunStatus::Succeeded);
        older.run_id = "wave-8-older".to_string();
        older.created_at_ms = 1;
        let mut newer = older.clone();
        newer.run_id = "wave-8-newer".to_string();
        newer.created_at_ms = 2;

        write_run_record(&dir.path().join("older.json"), &older).expect("write older");
        write_run_record(&dir.path().join("newer.json"), &newer).expect("write newer");

        let latest = load_latest_run_records_by_wave(dir.path()).expect("load latest");
        assert_eq!(
            latest.get(&8).map(|record| record.run_id.as_str()),
            Some("wave-8-newer")
        );
    }

    #[test]
    fn replay_accepts_legacy_trace_records() {
        let dir = tempdir().expect("tempdir");
        let record = sample_record(dir.path(), WaveRunStatus::Succeeded);
        fs::create_dir_all(record.prompt_path_parent()).expect("create agent dir");
        fs::write(&record.agents[0].prompt_path, "# prompt\n").expect("write prompt");
        fs::write(&record.agents[0].last_message_path, "done\n").expect("write last message");
        fs::write(&record.agents[0].events_path, "{}\n").expect("write events");
        fs::write(&record.agents[0].stderr_path, "").expect("write stderr");
        write_json(&record.trace_path, &record).expect("write legacy trace");

        let replay = validate_replay(&record);
        assert!(replay.ok, "{:#?}", replay.issues);
        assert!(replay.issues.is_empty());
    }

    #[test]
    fn replay_uses_v1_trace_bundle_and_checks_artifacts() {
        let dir = tempdir().expect("tempdir");
        let record = sample_record(dir.path(), WaveRunStatus::Succeeded);
        fs::create_dir_all(record.prompt_path_parent()).expect("create agent dir");
        fs::write(&record.agents[0].prompt_path, "# prompt\n").expect("write prompt");
        fs::write(&record.agents[0].last_message_path, "done\n").expect("write last message");
        fs::write(&record.agents[0].events_path, "{}\n").expect("write events");
        fs::write(&record.agents[0].stderr_path, "").expect("write stderr");
        write_trace_bundle(&record.trace_path, &record).expect("write trace bundle");

        let replay = validate_replay(&record);
        assert!(replay.ok, "{:#?}", replay.issues);
        assert!(replay.issues.is_empty());
    }

    #[test]
    fn completed_run_without_trace_bundle_fails_replay() {
        let dir = tempdir().expect("tempdir");
        let record = sample_record(dir.path(), WaveRunStatus::Succeeded);
        let replay = validate_replay(&record);
        assert!(!replay.ok);
        assert_eq!(replay.issues[0].kind, "missing_trace_bundle");
    }

    #[test]
    fn dry_run_is_not_treated_as_completed_success() {
        let record = sample_record(Path::new("/tmp"), WaveRunStatus::DryRun);
        assert!(!record.completed_successfully());
    }

    #[test]
    fn load_run_record_normalizes_repo_relative_paths() {
        let repo = tempdir().expect("tempdir");
        let runs_dir = repo.path().join(".wave/state/runs");
        fs::create_dir_all(&runs_dir).expect("create runs dir");

        let path = runs_dir.join("wave-8-1.json");
        let mut record = sample_record(repo.path(), WaveRunStatus::Succeeded);
        record.bundle_dir = PathBuf::from("./.wave/state/build/specs/wave-8-1");
        record.trace_path = PathBuf::from("./.wave/traces/runs/wave-8-1.json");
        record.codex_home = PathBuf::from("./.wave/codex");
        record.agents[0].prompt_path =
            PathBuf::from("./.wave/state/build/specs/wave-8-1/agents/A1/prompt.md");
        record.agents[0].last_message_path =
            PathBuf::from("./.wave/state/build/specs/wave-8-1/agents/A1/last-message.txt");
        record.agents[0].events_path =
            PathBuf::from("./.wave/state/build/specs/wave-8-1/agents/A1/events.jsonl");
        record.agents[0].stderr_path =
            PathBuf::from("./.wave/state/build/specs/wave-8-1/agents/A1/stderr.txt");
        write_run_record(&path, &record).expect("write record");

        let loaded = load_run_record(&path).expect("load record");

        assert_eq!(
            loaded.trace_path,
            repo.path().join(".wave/traces/runs/wave-8-1.json")
        );
        assert_eq!(
            loaded.agents[0].prompt_path,
            repo.path()
                .join(".wave/state/build/specs/wave-8-1/agents/A1/prompt.md")
        );
    }

    #[test]
    fn load_trace_bundle_normalizes_repo_relative_paths() {
        let repo = tempdir().expect("tempdir");
        let trace_dir = repo.path().join(".wave/traces/runs");
        fs::create_dir_all(&trace_dir).expect("create trace dir");

        let trace_path = trace_dir.join("wave-8-1.json");
        let bundle = TraceBundleV1 {
            schema_version: TRACE_SCHEMA_VERSION.to_string(),
            recorded_at_ms: 3,
            run: WaveRunRecord {
                trace_path: PathBuf::from("./.wave/traces/runs/wave-8-1.json"),
                bundle_dir: PathBuf::from("./.wave/state/build/specs/wave-8-1"),
                codex_home: PathBuf::from("./.wave/codex"),
                agents: vec![AgentRunRecord {
                    prompt_path: PathBuf::from(
                        "./.wave/state/build/specs/wave-8-1/agents/A1/prompt.md",
                    ),
                    last_message_path: PathBuf::from(
                        "./.wave/state/build/specs/wave-8-1/agents/A1/last-message.txt",
                    ),
                    events_path: PathBuf::from(
                        "./.wave/state/build/specs/wave-8-1/agents/A1/events.jsonl",
                    ),
                    stderr_path: PathBuf::from(
                        "./.wave/state/build/specs/wave-8-1/agents/A1/stderr.txt",
                    ),
                    ..sample_agent(repo.path(), WaveRunStatus::Succeeded)
                }],
                ..sample_record(repo.path(), WaveRunStatus::Succeeded)
            },
            self_host_evidence: None,
            agent_artifacts: vec![TraceAgentArtifactRecord {
                id: "A1".to_string(),
                prompt_exists: true,
                last_message_exists: true,
                events_exists: true,
                stderr_exists: true,
                artifacts: vec![TraceArtifactRecord {
                    kind: "prompt".to_string(),
                    path: PathBuf::from("./.wave/state/build/specs/wave-8-1/agents/A1/prompt.md"),
                    exists: true,
                    bytes: Some(8),
                }],
            }],
            run_artifacts: vec![TraceArtifactRecord {
                kind: "bundle_dir".to_string(),
                path: PathBuf::from("./.wave/state/build/specs/wave-8-1"),
                exists: true,
                bytes: None,
            }],
        };
        write_json(&trace_path, &bundle).expect("write trace bundle");

        let loaded = load_trace_bundle(&trace_path)
            .expect("load trace bundle")
            .expect("trace bundle exists");

        assert_eq!(
            loaded.run.trace_path,
            repo.path().join(".wave/traces/runs/wave-8-1.json")
        );
        assert_eq!(
            loaded.agent_artifacts[0].artifacts[0].path,
            repo.path()
                .join(".wave/state/build/specs/wave-8-1/agents/A1/prompt.md")
        );
        assert_eq!(
            loaded.run_artifacts[0].path,
            repo.path().join(".wave/state/build/specs/wave-8-1")
        );
    }

    #[test]
    fn result_envelope_round_trip_normalizes_paths() {
        let repo = tempdir().expect("tempdir");
        let envelope_path = repo
            .path()
            .join(".wave/state/results/wave-08/attempt-a1/agent_result_envelope.json");
        let absolute_doc = repo.path().join("docs/runtime.md");
        let absolute_log = repo.path().join("artifacts/proof.log");
        let absolute_marker = repo
            .path()
            .join(".wave/state/build/specs/wave-8-1/agents/A1/last-message.txt");

        let envelope = ResultEnvelopeRecord {
            result_envelope_id: "result:wave-8-1:a1".to_string(),
            wave_id: 8,
            task_id: "wave-08:agent-a1".to_string(),
            attempt_id: "attempt-a1".to_string(),
            agent_id: "A1".to_string(),
            task_role: "implementation".to_string(),
            closure_role: None,
            source: ResultEnvelopeSource::Structured,
            attempt_state: AttemptState::Succeeded,
            disposition: ResultDisposition::Completed,
            summary: Some("structured".to_string()),
            output_text: Some("[wave-proof]".to_string()),
            final_markers: FinalMarkerEnvelope::from_contract(
                vec!["[wave-proof]".to_string()],
                vec!["[wave-proof]".to_string()],
            ),
            proof_bundle_ids: Vec::new(),
            fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            artifacts: vec![ProofArtifact {
                path: absolute_log.to_string_lossy().into_owned(),
                kind: ArtifactKind::TestLog,
                digest: None,
                note: Some("test".to_string()),
            }],
            doc_delta: DocDeltaEnvelope {
                status: ResultPayloadStatus::Recorded,
                summary: Some("docs".to_string()),
                paths: vec![absolute_doc.to_string_lossy().into_owned()],
            },
            marker_evidence: vec![MarkerEvidence {
                marker: "[wave-proof]".to_string(),
                line: "[wave-proof]".to_string(),
                source: Some(absolute_marker.to_string_lossy().into_owned()),
            }],
            closure: ClosureState {
                disposition: ClosureDisposition::Ready,
                required_final_markers: vec!["[wave-proof]".to_string()],
                observed_final_markers: vec!["[wave-proof]".to_string()],
                blocking_reasons: Vec::new(),
                satisfied_fact_ids: Vec::new(),
                contradiction_ids: Vec::new(),
                verdict: ClosureVerdictPayload::None,
            },
            created_at_ms: 3,
        };
        write_result_envelope(&envelope_path, &envelope).expect("write result envelope");

        let stored = fs::read_to_string(&envelope_path).expect("read raw envelope");
        assert!(stored.contains("\"artifacts/proof.log\""));
        assert!(stored.contains("\"docs/runtime.md\""));

        let loaded = load_result_envelope(&envelope_path).expect("load envelope");
        assert_eq!(
            loaded.artifacts[0].path,
            repo.path().join("artifacts/proof.log").to_string_lossy()
        );
        assert_eq!(
            loaded.doc_delta.paths,
            vec![
                repo.path()
                    .join("docs/runtime.md")
                    .to_string_lossy()
                    .into_owned()
            ]
        );
        assert_eq!(
            loaded.marker_evidence[0].source.as_deref(),
            Some(
                repo.path()
                    .join(".wave/state/build/specs/wave-8-1/agents/A1/last-message.txt")
                    .to_string_lossy()
                    .as_ref()
            )
        );
    }

    trait AgentPathExt {
        fn prompt_path_parent(&self) -> PathBuf;
    }

    impl AgentPathExt for WaveRunRecord {
        fn prompt_path_parent(&self) -> PathBuf {
            self.agents[0]
                .prompt_path
                .parent()
                .expect("agent prompt parent")
                .to_path_buf()
        }
    }
}
