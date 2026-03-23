use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const TRACE_SCHEMA_VERSION: &str = "wave-trace/v1";

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
        matches!(
            self.status,
            WaveRunStatus::Succeeded | WaveRunStatus::DryRun
        )
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
    write_json(path, record)
}

pub fn load_run_record(path: &Path) -> Result<WaveRunRecord> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read run record {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("failed to parse {}", path.display()))
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
    let bundle = TraceBundleV1 {
        schema_version: TRACE_SCHEMA_VERSION.to_string(),
        recorded_at_ms: now_epoch_ms()?,
        run: record.clone(),
        agent_artifacts: record.agents.iter().map(snapshot_agent_artifacts).collect(),
        run_artifacts: snapshot_run_artifacts(record),
    };
    write_json(path, &bundle)
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

fn replay_report(record: &WaveRunRecord, issues: Vec<ReplayIssue>) -> ReplayReport {
    ReplayReport {
        run_id: record.run_id.clone(),
        wave_id: record.wave_id,
        ok: issues.is_empty(),
        issues,
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

fn snapshot_agent_artifacts(agent: &AgentRunRecord) -> TraceAgentArtifactRecord {
    TraceAgentArtifactRecord {
        id: agent.id.clone(),
        prompt_exists: agent.prompt_path.exists(),
        last_message_exists: agent.last_message_path.exists(),
        events_exists: agent.events_path.exists(),
        stderr_exists: agent.stderr_path.exists(),
        artifacts: vec![
            snapshot_artifact("prompt", &agent.prompt_path),
            snapshot_artifact("last_message", &agent.last_message_path),
            snapshot_artifact("events", &agent.events_path),
            snapshot_artifact("stderr", &agent.stderr_path),
        ],
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
