use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
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
const SCHEDULER_LOCK_RETRY_DELAY_MS: u64 = 50;
const SCHEDULER_LOCK_TIMEOUT_MS: u64 = 10_000;
const SCHEDULER_LOAD_RETRY_DELAY_MS: u64 = 10;
const SCHEDULER_LOAD_RETRY_ATTEMPTS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SchedulerMutationLockMetadata {
    pid: u32,
    process_started_at_ms: Option<u128>,
    acquired_at_ms: u128,
}

#[derive(Debug)]
pub struct SchedulerMutationLock {
    path: PathBuf,
    metadata: SchedulerMutationLockMetadata,
}

impl SchedulerMutationLock {
    pub fn acquire(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let started = Instant::now();
        loop {
            let metadata = SchedulerMutationLockMetadata {
                pid: std::process::id(),
                process_started_at_ms: current_process_started_at_ms(),
                acquired_at_ms: unix_epoch_ms(),
            };
            match try_acquire_scheduler_mutation_lock(&path, &metadata) {
                Ok(()) => {
                    return Ok(Self { path, metadata });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    if scheduler_mutation_lock_is_stale(&path) {
                        match fs::remove_file(&path) {
                            Ok(()) => continue,
                            Err(remove_error) if remove_error.kind() == ErrorKind::NotFound => {}
                            Err(remove_error) => {
                                return Err(remove_error).with_context(|| {
                                    format!(
                                        "failed to remove stale scheduler lock {}",
                                        path.display()
                                    )
                                });
                            }
                        }
                    }
                    if started.elapsed() > Duration::from_millis(SCHEDULER_LOCK_TIMEOUT_MS) {
                        anyhow::bail!(
                            "timed out waiting for scheduler mutation lock {}",
                            path.display()
                        );
                    }
                    thread::sleep(Duration::from_millis(SCHEDULER_LOCK_RETRY_DELAY_MS));
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to acquire {}", path.display()));
                }
            }
        }
    }
}

impl Drop for SchedulerMutationLock {
    fn drop(&mut self) {
        let current = read_scheduler_mutation_lock_metadata(&self.path).ok();
        if current.as_ref() != Some(&self.metadata) {
            return;
        }
        let _ = fs::remove_file(&self.path);
    }
}

pub fn with_scheduler_mutation_lock<T>(
    path: impl Into<PathBuf>,
    f: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let _lock = SchedulerMutationLock::acquire(path)?;
    f()
}

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
    WaveWorktreeUpdated,
    WavePromotionUpdated,
    WaveSchedulingUpdated,
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
        let mut line = serde_json::to_vec(event)
            .with_context(|| format!("failed to serialize event into {}", path.display()))?;
        line.push(b'\n');
        file.write_all(&line)
            .with_context(|| format!("failed to append event into {}", path.display()))?;
    }
    Ok(())
}

pub fn load_scheduler_events(path: &Path) -> Result<Vec<SchedulerEvent>> {
    let mut attempts = 0;
    loop {
        match load_scheduler_events_once(path) {
            Ok(events) => return Ok(events),
            Err(error) => {
                let retryable = error
                    .downcast_ref::<SchedulerLogParseError>()
                    .map(|parse| parse.retryable_last_line)
                    .unwrap_or(false);
                if retryable && attempts < SCHEDULER_LOAD_RETRY_ATTEMPTS {
                    attempts += 1;
                    thread::sleep(Duration::from_millis(SCHEDULER_LOAD_RETRY_DELAY_MS));
                    continue;
                }
                return Err(error);
            }
        }
    }
}

#[derive(Debug)]
struct SchedulerLogParseError {
    path: PathBuf,
    line_number: usize,
    retryable_last_line: bool,
    source: serde_json::Error,
}

impl std::fmt::Display for SchedulerLogParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "failed to parse scheduler event {}:{}",
            self.path.display(),
            self.line_number
        )
    }
}

impl std::error::Error for SchedulerLogParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

fn load_scheduler_events_once(path: &Path) -> Result<Vec<SchedulerEvent>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to open {}", path.display()))?;
    let lines = contents
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            (!trimmed.is_empty()).then_some((index + 1, trimmed))
        })
        .collect::<Vec<_>>();
    let last_non_empty_line_number = lines.last().map(|(line_number, _)| *line_number);
    let mut events = Vec::with_capacity(lines.len());
    for (line_number, line) in lines {
        let event = serde_json::from_str::<SchedulerEvent>(line).map_err(|source| {
            SchedulerLogParseError {
                path: path.to_path_buf(),
                line_number,
                retryable_last_line: Some(line_number) == last_non_empty_line_number,
                source,
            }
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
        SchedulerEventPayload::WaveWorktreeUpdated { .. } => None,
        SchedulerEventPayload::WavePromotionUpdated { .. } => None,
        SchedulerEventPayload::WaveSchedulingUpdated { .. } => None,
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
        SchedulerEventPayload::WaveWorktreeUpdated { .. } => None,
        SchedulerEventPayload::WavePromotionUpdated { .. } => None,
        SchedulerEventPayload::WaveSchedulingUpdated { .. } => None,
        SchedulerEventPayload::TaskLeaseUpdated { lease } => lease.owner.session_id.as_deref(),
        SchedulerEventPayload::SchedulerBudgetUpdated { budget } => {
            budget.owner.session_id.as_deref()
        }
        SchedulerEventPayload::None => None,
    }
}

fn try_acquire_scheduler_mutation_lock(
    path: &Path,
    metadata: &SchedulerMutationLockMetadata,
) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    serde_json::to_writer(&mut file, metadata).map_err(std::io::Error::other)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn read_scheduler_mutation_lock_metadata(path: &Path) -> Result<SchedulerMutationLockMetadata> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(raw.trim()).with_context(|| {
        format!(
            "failed to parse scheduler mutation lock metadata {}",
            path.display()
        )
    })
}

fn scheduler_mutation_lock_is_stale(path: &Path) -> bool {
    let Ok(metadata) = read_scheduler_mutation_lock_metadata(path) else {
        return false;
    };
    if !process_is_alive(metadata.pid) {
        return true;
    }
    match (
        metadata.process_started_at_ms,
        process_started_at_ms(metadata.pid),
    ) {
        (Some(expected), Some(observed)) => expected.abs_diff(observed) > 1_000,
        _ => false,
    }
}

fn unix_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn current_process_started_at_ms() -> Option<u128> {
    process_started_at_ms(std::process::id())
}

#[cfg(target_os = "linux")]
fn process_is_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(not(target_os = "linux"))]
fn process_is_alive(_pid: u32) -> bool {
    false
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
    let boot_time_ms = unix_epoch_ms().checked_sub(uptime_ms)?;
    Some(boot_time_ms + (start_ticks * 1000 / ticks_per_second))
}

#[cfg(not(target_os = "linux"))]
fn process_started_at_ms(_pid: u32) -> Option<u128> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use std::time::Instant;
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
            process_id: None,
            process_started_at_ms: None,
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
                reserved_closure_task_leases: Some(1),
                preemption_enabled: true,
            },
            owner: SchedulerOwner {
                scheduler_id: "wave-runtime".to_string(),
                scheduler_path: "wave-runtime/codex".to_string(),
                runtime: Some("codex".to_string()),
                executor: Some("codex".to_string()),
                session_id: Some("budget-bootstrap".to_string()),
                process_id: None,
                process_started_at_ms: None,
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

    #[test]
    fn scheduler_log_retries_partial_last_line_until_append_completes() {
        let root = temp_root("scheduler-partial-last-line");
        let path = root.join("scheduler").join(SCHEDULER_LOG_FILE_NAME);
        fs::create_dir_all(path.parent().expect("scheduler parent")).expect("create scheduler dir");

        let event_one = SchedulerEvent::new("sched-1", SchedulerEventKind::SchedulerBudgetUpdated)
            .with_created_at_ms(1)
            .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated {
                budget: SchedulerBudgetRecord {
                    budget_id: SchedulerBudgetId::new("budget-a"),
                    budget: SchedulerBudget::default(),
                    owner: SchedulerOwner::default(),
                    updated_at_ms: 1,
                    detail: Some("budget a".to_string()),
                },
            });
        let event_two = SchedulerEvent::new("sched-2", SchedulerEventKind::WaveClaimAcquired)
            .with_wave_id(13)
            .with_created_at_ms(2)
            .with_payload(SchedulerEventPayload::WaveClaimUpdated {
                claim: WaveClaimRecord {
                    claim_id: WaveClaimId::new("claim-wave-13"),
                    wave_id: 13,
                    state: WaveClaimState::Held,
                    owner: SchedulerOwner::default(),
                    claimed_at_ms: 2,
                    released_at_ms: None,
                    detail: Some("claim".to_string()),
                },
            });

        let first_line = format!(
            "{}\n",
            serde_json::to_string(&event_one).expect("serialize first event")
        );
        let second_line = format!(
            "{}\n",
            serde_json::to_string(&event_two).expect("serialize second event")
        );
        fs::write(&path, &first_line).expect("write first line");

        let path_clone = path.clone();
        let second_line_clone = second_line.clone();
        let (partial_written_tx, partial_written_rx) = mpsc::channel();
        let writer = thread::spawn(move || {
            let midpoint = second_line_clone.len() / 2;
            let mut file = OpenOptions::new()
                .append(true)
                .open(&path_clone)
                .expect("open scheduler log for append");
            file.write_all(&second_line_clone.as_bytes()[..midpoint])
                .expect("write partial line");
            file.flush().expect("flush partial line");
            partial_written_tx
                .send(())
                .expect("signal partial line is visible");
            thread::sleep(Duration::from_millis(
                SCHEDULER_LOAD_RETRY_DELAY_MS.saturating_mul(2),
            ));
            file.write_all(&second_line_clone.as_bytes()[midpoint..])
                .expect("finish line");
            file.flush().expect("flush finished line");
        });

        partial_written_rx
            .recv()
            .expect("wait for partial line to be written");
        let events = load_scheduler_events(&path).expect("load scheduler events");
        writer.join().expect("join writer");

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_id, "sched-1");
        assert_eq!(events[1].event_id, "sched-2");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scheduler_log_still_fails_closed_on_malformed_interior_line() {
        let root = temp_root("scheduler-malformed-interior-line");
        let path = root.join("scheduler").join(SCHEDULER_LOG_FILE_NAME);
        fs::create_dir_all(path.parent().expect("scheduler parent")).expect("create scheduler dir");

        let event = SchedulerEvent::new("sched-1", SchedulerEventKind::SchedulerBudgetUpdated)
            .with_created_at_ms(1)
            .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated {
                budget: SchedulerBudgetRecord {
                    budget_id: SchedulerBudgetId::new("budget-a"),
                    budget: SchedulerBudget::default(),
                    owner: SchedulerOwner::default(),
                    updated_at_ms: 1,
                    detail: Some("budget a".to_string()),
                },
            });
        let line = serde_json::to_string(&event).expect("serialize event");
        fs::write(&path, format!("{line}\n{{not-json}}\n{line}\n")).expect("write log");

        let error = load_scheduler_events(&path).expect_err("malformed interior line must fail");
        let message = error.to_string();
        assert!(message.contains("failed to parse scheduler event"));
        assert!(message.contains(":2"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scheduler_mutation_lock_serializes_concurrent_writers() {
        let root = temp_root("scheduler-lock");
        let lock_path = root.join(".wave/state/derived/scheduler/mutation.lock");
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();

        let first_lock_path = lock_path.clone();
        let first = thread::spawn(move || {
            let _lock = SchedulerMutationLock::acquire(first_lock_path).expect("first lock");
            entered_tx.send(()).expect("entered");
            release_rx.recv().expect("release");
        });

        entered_rx.recv().expect("first lock entered");
        let second_lock_path = lock_path.clone();
        let second = thread::spawn(move || {
            let started = Instant::now();
            let _lock = SchedulerMutationLock::acquire(second_lock_path).expect("second lock");
            started.elapsed()
        });

        thread::sleep(Duration::from_millis(100));
        release_tx.send(()).expect("release first");
        let waited = second.join().expect("join second");
        first.join().expect("join first");

        assert!(waited >= Duration::from_millis(100));
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
