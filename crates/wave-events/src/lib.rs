use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use wave_config::DEFAULT_STATE_EVENTS_CONTROL_DIR;
use wave_config::DEFAULT_STATE_EVENTS_SCHEDULER_DIR;
use wave_domain::AttemptId;
use wave_domain::ControlEventPayload;
use wave_domain::SchedulerEventPayload;
use wave_domain::TaskId;
use wave_domain::TaskLeaseId;
use wave_domain::WaveClaimId;

pub const CONTROL_EVENT_SCHEMA_VERSION: u32 = 1;
const CONTROL_LOG_FILE_PREFIX: &str = "wave-";
const CONTROL_LOG_FILE_SUFFIX: &str = ".jsonl";
pub const SCHEDULER_EVENT_SCHEMA_VERSION: u32 = 1;
pub const SCHEDULER_LOG_FILE_NAME: &str = "scheduler.jsonl";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlEventKind {
    WaveDeclared,
    WaveSelected,
    TaskSeeded,
    AttemptPlanned,
    AttemptStarted,
    AttemptObserved,
    AttemptFinished,
    LaunchRefused,
    GateEvaluated,
    ClosureBlocked,
    FactObserved,
    ContradictionUpdated,
    ProofRecorded,
    RerunRequested,
    RerunCleared,
    HumanInputUpdated,
    ResultEnvelopeRecorded,
    WaveCompleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlEvent {
    pub schema_version: u32,
    pub event_id: String,
    pub kind: ControlEventKind,
    pub wave_id: u32,
    pub task_id: Option<TaskId>,
    pub attempt_id: Option<AttemptId>,
    pub created_at_ms: u128,
    pub causation_event_id: Option<String>,
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub payload: ControlEventPayload,
}

impl ControlEvent {
    pub fn new(event_id: impl Into<String>, kind: ControlEventKind, wave_id: u32) -> Self {
        Self {
            schema_version: CONTROL_EVENT_SCHEMA_VERSION,
            event_id: event_id.into(),
            kind,
            wave_id,
            task_id: None,
            attempt_id: None,
            created_at_ms: 0,
            causation_event_id: None,
            correlation_id: None,
            payload: ControlEventPayload::None,
        }
    }

    pub fn with_task_id(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn with_attempt_id(mut self, attempt_id: AttemptId) -> Self {
        self.attempt_id = Some(attempt_id);
        self
    }

    pub fn with_created_at_ms(mut self, created_at_ms: u128) -> Self {
        self.created_at_ms = created_at_ms;
        self
    }

    pub fn with_causation_event_id(mut self, causation_event_id: impl Into<String>) -> Self {
        self.causation_event_id = Some(causation_event_id.into());
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    pub fn with_payload(mut self, payload: ControlEventPayload) -> Self {
        self.payload = payload;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ControlEventQuery {
    pub wave_id: Option<u32>,
    pub task_id: Option<TaskId>,
    pub attempt_id: Option<AttemptId>,
    pub event_id: Option<String>,
    pub kind: Option<ControlEventKind>,
    pub causation_event_id: Option<String>,
    pub correlation_id: Option<String>,
}

impl ControlEventQuery {
    pub fn for_wave(wave_id: u32) -> Self {
        Self {
            wave_id: Some(wave_id),
            ..Self::default()
        }
    }

    fn matches(&self, event: &ControlEvent) -> bool {
        if let Some(wave_id) = self.wave_id {
            if event.wave_id != wave_id {
                return false;
            }
        }
        if let Some(task_id) = self.task_id.as_ref() {
            if event.task_id.as_ref() != Some(task_id) {
                return false;
            }
        }
        if let Some(attempt_id) = self.attempt_id.as_ref() {
            if event.attempt_id.as_ref() != Some(attempt_id) {
                return false;
            }
        }
        if let Some(event_id) = self.event_id.as_ref() {
            if &event.event_id != event_id {
                return false;
            }
        }
        if let Some(kind) = self.kind.as_ref() {
            if &event.kind != kind {
                return false;
            }
        }
        if let Some(causation_event_id) = self.causation_event_id.as_ref() {
            if event.causation_event_id.as_ref() != Some(causation_event_id) {
                return false;
            }
        }
        if let Some(correlation_id) = self.correlation_id.as_ref() {
            if event.correlation_id.as_ref() != Some(correlation_id) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlEventLog {
    root_dir: PathBuf,
}

impl ControlEventLog {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn under_repo(repo_root: &Path) -> Self {
        Self::new(canonical_control_log_root(repo_root))
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn wave_path(&self, wave_id: u32) -> PathBuf {
        canonical_control_log_path(&self.root_dir, wave_id)
    }

    pub fn append(&self, event: &ControlEvent) -> Result<()> {
        append_control_event(&self.wave_path(event.wave_id), event)
    }

    pub fn append_many(&self, events: &[ControlEvent]) -> Result<()> {
        let mut events_by_wave = BTreeMap::<u32, Vec<ControlEvent>>::new();
        for event in events {
            events_by_wave
                .entry(event.wave_id)
                .or_default()
                .push(event.clone());
        }
        for (wave_id, wave_events) in events_by_wave {
            append_control_events(&self.wave_path(wave_id), &wave_events)?;
        }
        Ok(())
    }

    pub fn load_wave(&self, wave_id: u32) -> Result<Vec<ControlEvent>> {
        load_control_events(&self.wave_path(wave_id))
    }

    pub fn load_all(&self) -> Result<Vec<ControlEvent>> {
        load_control_events_under(&self.root_dir)
    }

    pub fn latest_wave(&self, wave_id: u32) -> Result<Option<ControlEvent>> {
        latest_control_event(&self.wave_path(wave_id))
    }

    pub fn query(&self, query: &ControlEventQuery) -> Result<Vec<ControlEvent>> {
        if let Some(wave_id) = query.wave_id {
            return query_control_events(&self.wave_path(wave_id), query);
        }
        self.query_all(query)
    }

    pub fn query_all(&self, query: &ControlEventQuery) -> Result<Vec<ControlEvent>> {
        query_control_events_under(&self.root_dir, query)
    }

    pub fn list_waves(&self) -> Result<Vec<u32>> {
        control_wave_ids_under(&self.root_dir)
    }
}

pub fn canonical_control_log_root(repo_root: &Path) -> PathBuf {
    repo_root.join(DEFAULT_STATE_EVENTS_CONTROL_DIR)
}

pub fn canonical_control_log_path(dir: &Path, wave_id: u32) -> PathBuf {
    dir.join(format!("wave-{wave_id:02}.jsonl"))
}

pub fn canonical_control_log_path_under(repo_root: &Path, wave_id: u32) -> PathBuf {
    canonical_control_log_path(&canonical_control_log_root(repo_root), wave_id)
}

pub fn append_control_event(path: &Path, event: &ControlEvent) -> Result<()> {
    append_control_events(path, std::slice::from_ref(event))
}

pub fn append_control_events(path: &Path, events: &[ControlEvent]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    for event in events {
        serde_json::to_writer(&mut file, event)
            .with_context(|| format!("failed to serialize event into {}", path.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("failed to append newline to {}", path.display()))?;
    }
    Ok(())
}

pub fn load_control_events(path: &Path) -> Result<Vec<ControlEvent>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for (line_number, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let event = serde_json::from_str::<ControlEvent>(&line).with_context(|| {
            format!(
                "failed to parse control event {}:{}",
                path.display(),
                line_number + 1
            )
        })?;
        events.push(event);
    }
    Ok(events)
}

pub fn load_control_events_under(dir: &Path) -> Result<Vec<ControlEvent>> {
    let mut events = Vec::new();
    for wave_id in control_wave_ids_under(dir)? {
        events.extend(load_control_events(&canonical_control_log_path(
            dir, wave_id,
        ))?);
    }
    Ok(events)
}

pub fn latest_control_event(path: &Path) -> Result<Option<ControlEvent>> {
    Ok(load_control_events(path)?.into_iter().last())
}

pub fn query_control_events(path: &Path, query: &ControlEventQuery) -> Result<Vec<ControlEvent>> {
    Ok(load_control_events(path)?
        .into_iter()
        .filter(|event| query.matches(event))
        .collect())
}

pub fn query_control_events_under(
    dir: &Path,
    query: &ControlEventQuery,
) -> Result<Vec<ControlEvent>> {
    Ok(load_control_events_under(dir)?
        .into_iter()
        .filter(|event| query.matches(event))
        .collect())
}

pub fn control_wave_ids_under(dir: &Path) -> Result<Vec<u32>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut wave_ids = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter_map(|path| parse_control_wave_id(&path))
        .collect::<Vec<_>>();
    wave_ids.sort_unstable();
    Ok(wave_ids)
}

pub fn parse_control_wave_id(path: &Path) -> Option<u32> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(CONTROL_LOG_FILE_PREFIX))
        .and_then(|name| name.strip_suffix(CONTROL_LOG_FILE_SUFFIX))
        .and_then(|wave_id| wave_id.parse::<u32>().ok())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerEventKind {
    WaveClaimAcquired,
    WaveClaimReleased,
    TaskLeaseGranted,
    TaskLeaseRenewed,
    TaskLeaseReleased,
    TaskLeaseExpired,
    TaskLeaseRevoked,
    SchedulerBudgetUpdated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerEvent {
    pub schema_version: u32,
    pub event_id: String,
    pub kind: SchedulerEventKind,
    pub wave_id: Option<u32>,
    pub task_id: Option<TaskId>,
    pub claim_id: Option<WaveClaimId>,
    pub lease_id: Option<TaskLeaseId>,
    pub created_at_ms: u128,
    pub causation_event_id: Option<String>,
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub payload: SchedulerEventPayload,
}

impl SchedulerEvent {
    pub fn new(event_id: impl Into<String>, kind: SchedulerEventKind) -> Self {
        Self {
            schema_version: SCHEDULER_EVENT_SCHEMA_VERSION,
            event_id: event_id.into(),
            kind,
            wave_id: None,
            task_id: None,
            claim_id: None,
            lease_id: None,
            created_at_ms: 0,
            causation_event_id: None,
            correlation_id: None,
            payload: SchedulerEventPayload::None,
        }
    }

    pub fn with_wave_id(mut self, wave_id: u32) -> Self {
        self.wave_id = Some(wave_id);
        self
    }

    pub fn with_task_id(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn with_claim_id(mut self, claim_id: WaveClaimId) -> Self {
        self.claim_id = Some(claim_id);
        self
    }

    pub fn with_lease_id(mut self, lease_id: TaskLeaseId) -> Self {
        self.lease_id = Some(lease_id);
        self
    }

    pub fn with_created_at_ms(mut self, created_at_ms: u128) -> Self {
        self.created_at_ms = created_at_ms;
        self
    }

    pub fn with_causation_event_id(mut self, causation_event_id: impl Into<String>) -> Self {
        self.causation_event_id = Some(causation_event_id.into());
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    pub fn with_payload(mut self, payload: SchedulerEventPayload) -> Self {
        self.payload = payload;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchedulerEventQuery {
    pub wave_id: Option<u32>,
    pub task_id: Option<TaskId>,
    pub claim_id: Option<WaveClaimId>,
    pub lease_id: Option<TaskLeaseId>,
    pub owner_path: Option<String>,
    pub owner_session_id: Option<String>,
    pub event_id: Option<String>,
    pub kind: Option<SchedulerEventKind>,
    pub causation_event_id: Option<String>,
    pub correlation_id: Option<String>,
}

impl SchedulerEventQuery {
    fn matches(&self, event: &SchedulerEvent) -> bool {
        if let Some(wave_id) = self.wave_id {
            if event.wave_id != Some(wave_id) {
                return false;
            }
        }
        if let Some(task_id) = self.task_id.as_ref() {
            if event.task_id.as_ref() != Some(task_id) {
                return false;
            }
        }
        if let Some(claim_id) = self.claim_id.as_ref() {
            if event.claim_id.as_ref() != Some(claim_id) {
                return false;
            }
        }
        if let Some(lease_id) = self.lease_id.as_ref() {
            if event.lease_id.as_ref() != Some(lease_id) {
                return false;
            }
        }
        if let Some(owner_path) = self.owner_path.as_ref() {
            if scheduler_owner_path(event) != Some(owner_path.as_str()) {
                return false;
            }
        }
        if let Some(owner_session_id) = self.owner_session_id.as_ref() {
            if scheduler_owner_session_id(event) != Some(owner_session_id.as_str()) {
                return false;
            }
        }
        if let Some(event_id) = self.event_id.as_ref() {
            if &event.event_id != event_id {
                return false;
            }
        }
        if let Some(kind) = self.kind.as_ref() {
            if &event.kind != kind {
                return false;
            }
        }
        if let Some(causation_event_id) = self.causation_event_id.as_ref() {
            if event.causation_event_id.as_ref() != Some(causation_event_id) {
                return false;
            }
        }
        if let Some(correlation_id) = self.correlation_id.as_ref() {
            if event.correlation_id.as_ref() != Some(correlation_id) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerEventLog {
    root_dir: PathBuf,
}

impl SchedulerEventLog {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn under_repo(repo_root: &Path) -> Self {
        Self::new(canonical_scheduler_log_root(repo_root))
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn path(&self) -> PathBuf {
        canonical_scheduler_log_path(&self.root_dir)
    }

    pub fn append(&self, event: &SchedulerEvent) -> Result<()> {
        append_scheduler_event(&self.path(), event)
    }

    pub fn append_many(&self, events: &[SchedulerEvent]) -> Result<()> {
        append_scheduler_events(&self.path(), events)
    }

    pub fn load_all(&self) -> Result<Vec<SchedulerEvent>> {
        load_scheduler_events(&self.path())
    }

    pub fn latest(&self) -> Result<Option<SchedulerEvent>> {
        latest_scheduler_event(&self.path())
    }

    pub fn query(&self, query: &SchedulerEventQuery) -> Result<Vec<SchedulerEvent>> {
        query_scheduler_events(&self.path(), query)
    }
}

pub fn canonical_scheduler_log_root(repo_root: &Path) -> PathBuf {
    repo_root.join(DEFAULT_STATE_EVENTS_SCHEDULER_DIR)
}

pub fn canonical_scheduler_log_path(dir: &Path) -> PathBuf {
    dir.join(SCHEDULER_LOG_FILE_NAME)
}

pub fn canonical_scheduler_log_path_under(repo_root: &Path) -> PathBuf {
    canonical_scheduler_log_path(&canonical_scheduler_log_root(repo_root))
}

pub fn append_scheduler_event(path: &Path, event: &SchedulerEvent) -> Result<()> {
    append_scheduler_events(path, std::slice::from_ref(event))
}

pub fn append_scheduler_events(path: &Path, events: &[SchedulerEvent]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    for event in events {
        serde_json::to_writer(&mut file, event)
            .with_context(|| format!("failed to serialize event into {}", path.display()))?;
        file.write_all(b"\n")
            .with_context(|| format!("failed to append newline to {}", path.display()))?;
    }
    Ok(())
}

pub fn load_scheduler_events(path: &Path) -> Result<Vec<SchedulerEvent>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for (line_number, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let event = serde_json::from_str::<SchedulerEvent>(&line).with_context(|| {
            format!(
                "failed to parse scheduler event {}:{}",
                path.display(),
                line_number + 1
            )
        })?;
        events.push(event);
    }
    Ok(events)
}

pub fn latest_scheduler_event(path: &Path) -> Result<Option<SchedulerEvent>> {
    Ok(load_scheduler_events(path)?.into_iter().last())
}

pub fn query_scheduler_events(
    path: &Path,
    query: &SchedulerEventQuery,
) -> Result<Vec<SchedulerEvent>> {
    Ok(load_scheduler_events(path)?
        .into_iter()
        .filter(|event| query.matches(event))
        .collect())
}

fn scheduler_owner_path(event: &SchedulerEvent) -> Option<&str> {
    match &event.payload {
        SchedulerEventPayload::WaveClaimUpdated { claim } => {
            Some(claim.owner.scheduler_path.as_str())
        }
        SchedulerEventPayload::TaskLeaseUpdated { lease } => {
            Some(lease.owner.scheduler_path.as_str())
        }
        SchedulerEventPayload::SchedulerBudgetUpdated { budget } => {
            Some(budget.owner.scheduler_path.as_str())
        }
        SchedulerEventPayload::None => None,
    }
}

fn scheduler_owner_session_id(event: &SchedulerEvent) -> Option<&str> {
    match &event.payload {
        SchedulerEventPayload::WaveClaimUpdated { claim } => claim.owner.session_id.as_deref(),
        SchedulerEventPayload::TaskLeaseUpdated { lease } => lease.owner.session_id.as_deref(),
        SchedulerEventPayload::SchedulerBudgetUpdated { budget } => {
            budget.owner.session_id.as_deref()
        }
        SchedulerEventPayload::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;
    use wave_domain::AttemptRecord;
    use wave_domain::AttemptState;
    use wave_domain::ProofBundleId;
    use wave_domain::SchedulerBudget;
    use wave_domain::SchedulerBudgetId;
    use wave_domain::SchedulerBudgetRecord;
    use wave_domain::SchedulerOwner;
    use wave_domain::TaskLeaseRecord;
    use wave_domain::TaskLeaseState;
    use wave_domain::WaveClaimRecord;
    use wave_domain::WaveClaimState;

    #[test]
    fn appends_and_loads_typed_jsonl_events() {
        let root = temp_root("events");
        let log = ControlEventLog::under_repo(&root);
        let task_id = TaskId::new("wave-10:agent-a1");
        let attempt_id = AttemptId::new("attempt-1");
        let started_attempt = sample_attempt(AttemptState::Running);
        let finished_attempt = sample_attempt(AttemptState::Succeeded);

        log.append(
            &ControlEvent::new("evt-1", ControlEventKind::AttemptStarted, 10)
                .with_task_id(task_id.clone())
                .with_attempt_id(attempt_id.clone())
                .with_created_at_ms(1)
                .with_correlation_id("corr-1")
                .with_payload(ControlEventPayload::AttemptUpdated {
                    attempt: started_attempt,
                }),
        )
        .expect("append");
        log.append(
            &ControlEvent::new("evt-2", ControlEventKind::AttemptFinished, 10)
                .with_task_id(task_id)
                .with_attempt_id(attempt_id)
                .with_created_at_ms(2)
                .with_causation_event_id("evt-1")
                .with_correlation_id("corr-1")
                .with_payload(ControlEventPayload::AttemptUpdated {
                    attempt: finished_attempt,
                }),
        )
        .expect("append");

        let path = log.wave_path(10);
        let events = load_control_events(&path).expect("load");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].schema_version, CONTROL_EVENT_SCHEMA_VERSION);
        assert_eq!(events[0].event_id, "evt-1");
        assert_eq!(
            log.latest_wave(10).expect("latest").expect("some").event_id,
            "evt-2"
        );
        assert_eq!(
            canonical_control_log_path_under(&root, 10),
            root.join(".wave/state/events/control/wave-10.jsonl")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn queries_control_events_by_kind_and_attempt() {
        let root = temp_root("control-query");
        let log = ControlEventLog::new(root.join("control"));
        let task_id = TaskId::new("wave-10:agent-a1");
        let attempt_id = AttemptId::new("attempt-1");

        log.append_many(&[
            ControlEvent::new("evt-1", ControlEventKind::AttemptStarted, 10)
                .with_task_id(task_id.clone())
                .with_attempt_id(attempt_id.clone()),
            ControlEvent::new("evt-2", ControlEventKind::GateEvaluated, 10)
                .with_task_id(task_id.clone()),
            ControlEvent::new("evt-3", ControlEventKind::AttemptStarted, 11),
        ])
        .expect("append many");

        let mut query = ControlEventQuery::for_wave(10);
        query.task_id = Some(task_id);
        query.attempt_id = Some(attempt_id);
        query.kind = Some(ControlEventKind::AttemptStarted);
        let events = log.query(&query).expect("query");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, "evt-1");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lists_control_streams_and_queries_across_root() {
        let root = temp_root("control-root");
        let log = ControlEventLog::under_repo(&root);

        log.append_many(&[
            ControlEvent::new("evt-1", ControlEventKind::AttemptStarted, 10)
                .with_correlation_id("corr-10"),
            ControlEvent::new("evt-2", ControlEventKind::FactObserved, 11)
                .with_causation_event_id("evt-1")
                .with_correlation_id("corr-11"),
        ])
        .expect("append many");

        assert_eq!(log.list_waves().expect("waves"), vec![10, 11]);
        assert_eq!(log.load_all().expect("all").len(), 2);

        let query = ControlEventQuery {
            wave_id: None,
            task_id: None,
            attempt_id: None,
            event_id: None,
            kind: Some(ControlEventKind::FactObserved),
            causation_event_id: Some("evt-1".to_string()),
            correlation_id: Some("corr-11".to_string()),
        };
        let events = log.query(&query).expect("query all");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].wave_id, 11);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn appends_and_queries_scheduler_events() {
        let root = temp_root("scheduler");
        let log = SchedulerEventLog::under_repo(&root);
        let owner = SchedulerOwner {
            scheduler_id: "wave-runtime".to_string(),
            scheduler_path: "wave-runtime/codex".to_string(),
            runtime: Some("codex".to_string()),
            executor: Some("codex".to_string()),
            session_id: Some("wave-13-run".to_string()),
        };
        let claim = WaveClaimRecord {
            claim_id: WaveClaimId::new("claim-wave-13"),
            wave_id: 13,
            state: WaveClaimState::Held,
            owner: owner.clone(),
            claimed_at_ms: 10,
            released_at_ms: None,
            detail: Some("claim acquired".to_string()),
        };
        let lease = TaskLeaseRecord {
            lease_id: TaskLeaseId::new("lease-wave-13-a1"),
            wave_id: 13,
            task_id: TaskId::new("wave-13:agent-a1"),
            claim_id: Some(claim.claim_id.clone()),
            state: TaskLeaseState::Granted,
            owner: owner.clone(),
            granted_at_ms: 11,
            heartbeat_at_ms: Some(12),
            expires_at_ms: Some(42),
            finished_at_ms: None,
            detail: Some("lease granted".to_string()),
        };

        log.append_many(&[
            SchedulerEvent::new("sched-1", SchedulerEventKind::WaveClaimAcquired)
                .with_wave_id(13)
                .with_claim_id(claim.claim_id.clone())
                .with_created_at_ms(10)
                .with_correlation_id("wave-13-run")
                .with_payload(SchedulerEventPayload::WaveClaimUpdated {
                    claim: claim.clone(),
                }),
            SchedulerEvent::new("sched-2", SchedulerEventKind::TaskLeaseGranted)
                .with_wave_id(13)
                .with_task_id(lease.task_id.clone())
                .with_claim_id(claim.claim_id.clone())
                .with_lease_id(lease.lease_id.clone())
                .with_created_at_ms(11)
                .with_correlation_id("wave-13-run")
                .with_payload(SchedulerEventPayload::TaskLeaseUpdated {
                    lease: lease.clone(),
                }),
        ])
        .expect("append many");

        let path = log.path();
        assert_eq!(
            canonical_scheduler_log_path_under(&root),
            root.join(".wave/state/events/scheduler/scheduler.jsonl")
        );
        assert_eq!(load_scheduler_events(&path).expect("load").len(), 2);
        assert_eq!(
            log.latest().expect("latest").expect("some").event_id,
            "sched-2"
        );

        let query = SchedulerEventQuery {
            wave_id: Some(13),
            task_id: Some(lease.task_id.clone()),
            claim_id: Some(claim.claim_id.clone()),
            lease_id: Some(lease.lease_id.clone()),
            owner_path: Some("wave-runtime/codex".to_string()),
            owner_session_id: Some("wave-13-run".to_string()),
            event_id: None,
            kind: Some(SchedulerEventKind::TaskLeaseGranted),
            causation_event_id: None,
            correlation_id: Some("wave-13-run".to_string()),
        };
        let events = log.query(&query).expect("query");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_id, "sched-2");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scheduler_log_surfaces_budget_updates() {
        let root = temp_root("scheduler-budget");
        let log = SchedulerEventLog::new(root.join("scheduler"));
        let budget = SchedulerBudgetRecord {
            budget_id: SchedulerBudgetId::new("budget-default"),
            budget: SchedulerBudget {
                max_active_wave_claims: Some(1),
                max_active_task_leases: Some(1),
            },
            owner: SchedulerOwner {
                scheduler_id: "wave-runtime".to_string(),
                scheduler_path: "wave-runtime/codex".to_string(),
                runtime: Some("codex".to_string()),
                executor: Some("codex".to_string()),
                session_id: Some("budget-bootstrap".to_string()),
            },
            updated_at_ms: 1,
            detail: Some("default serial budget".to_string()),
        };

        log.append(
            &SchedulerEvent::new("sched-budget", SchedulerEventKind::SchedulerBudgetUpdated)
                .with_created_at_ms(1)
                .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated { budget }),
        )
        .expect("append");

        let events = log.load_all().expect("load");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, SchedulerEventKind::SchedulerBudgetUpdated);

        let _ = fs::remove_dir_all(root);
    }

    fn sample_attempt(state: AttemptState) -> AttemptRecord {
        AttemptRecord {
            attempt_id: AttemptId::new("attempt-1"),
            wave_id: 10,
            task_id: TaskId::new("wave-10:agent-a1"),
            attempt_number: 1,
            state,
            executor: "implement-codex".to_string(),
            created_at_ms: 1,
            started_at_ms: Some(1),
            finished_at_ms: Some(2),
            summary: Some("authority core".to_string()),
            proof_bundle_ids: vec![ProofBundleId::new("proof-1")],
            result_envelope_id: None,
        }
    }

    fn temp_root(label: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis();
        std::env::temp_dir().join(format!("wave-{label}-{}-{millis}", std::process::id()))
    }
}
