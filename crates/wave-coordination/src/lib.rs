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
use wave_config::DEFAULT_STATE_EVENTS_COORDINATION_DIR;
use wave_domain::ContradictionId;
use wave_domain::FactId;
use wave_domain::HumanInputRequestId;
use wave_domain::TaskId;

pub const COORDINATION_RECORD_SCHEMA_VERSION: u32 = 1;
const COORDINATION_LOG_FILE_PREFIX: &str = "wave-";
const COORDINATION_LOG_FILE_SUFFIX: &str = ".jsonl";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinationRecordKind {
    Claim,
    Evidence,
    Blocker,
    Clarification,
    Handoff,
    Contradiction,
    Escalation,
    Decision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinationCitation {
    pub path: String,
    pub line: Option<u32>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinationRecord {
    pub schema_version: u32,
    pub record_id: String,
    pub kind: CoordinationRecordKind,
    pub wave_id: u32,
    pub task_id: Option<TaskId>,
    pub agent_id: Option<String>,
    pub created_at_ms: u128,
    pub summary: String,
    pub detail: Option<String>,
    #[serde(default)]
    pub citations: Vec<CoordinationCitation>,
    #[serde(default)]
    pub fact_ids: Vec<FactId>,
    #[serde(default)]
    pub contradiction_ids: Vec<ContradictionId>,
    pub human_input_request_id: Option<HumanInputRequestId>,
    #[serde(default)]
    pub related_record_ids: Vec<String>,
}

impl CoordinationRecord {
    pub fn new(
        record_id: impl Into<String>,
        kind: CoordinationRecordKind,
        wave_id: u32,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: COORDINATION_RECORD_SCHEMA_VERSION,
            record_id: record_id.into(),
            kind,
            wave_id,
            task_id: None,
            agent_id: None,
            created_at_ms: 0,
            summary: summary.into(),
            detail: None,
            citations: Vec::new(),
            fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            human_input_request_id: None,
            related_record_ids: Vec::new(),
        }
    }

    pub fn with_task_id(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    pub fn with_created_at_ms(mut self, created_at_ms: u128) -> Self {
        self.created_at_ms = created_at_ms;
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_citations(mut self, citations: Vec<CoordinationCitation>) -> Self {
        self.citations = citations;
        self
    }

    pub fn with_fact_ids(mut self, fact_ids: Vec<FactId>) -> Self {
        self.fact_ids = fact_ids;
        self
    }

    pub fn with_contradiction_ids(mut self, contradiction_ids: Vec<ContradictionId>) -> Self {
        self.contradiction_ids = contradiction_ids;
        self
    }

    pub fn with_human_input_request_id(
        mut self,
        human_input_request_id: HumanInputRequestId,
    ) -> Self {
        self.human_input_request_id = Some(human_input_request_id);
        self
    }

    pub fn with_related_record_ids(mut self, related_record_ids: Vec<String>) -> Self {
        self.related_record_ids = related_record_ids;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoordinationRecordQuery {
    pub wave_id: Option<u32>,
    pub task_id: Option<TaskId>,
    pub agent_id: Option<String>,
    pub record_id: Option<String>,
    pub kind: Option<CoordinationRecordKind>,
    pub fact_id: Option<FactId>,
    pub contradiction_id: Option<ContradictionId>,
    pub human_input_request_id: Option<HumanInputRequestId>,
    pub related_record_id: Option<String>,
}

impl CoordinationRecordQuery {
    pub fn for_wave(wave_id: u32) -> Self {
        Self {
            wave_id: Some(wave_id),
            ..Self::default()
        }
    }

    fn matches(&self, record: &CoordinationRecord) -> bool {
        if let Some(wave_id) = self.wave_id {
            if record.wave_id != wave_id {
                return false;
            }
        }
        if let Some(task_id) = self.task_id.as_ref() {
            if record.task_id.as_ref() != Some(task_id) {
                return false;
            }
        }
        if let Some(agent_id) = self.agent_id.as_ref() {
            if record.agent_id.as_ref() != Some(agent_id) {
                return false;
            }
        }
        if let Some(record_id) = self.record_id.as_ref() {
            if &record.record_id != record_id {
                return false;
            }
        }
        if let Some(kind) = self.kind.as_ref() {
            if &record.kind != kind {
                return false;
            }
        }
        if let Some(fact_id) = self.fact_id.as_ref() {
            if !record.fact_ids.contains(fact_id) {
                return false;
            }
        }
        if let Some(contradiction_id) = self.contradiction_id.as_ref() {
            if !record.contradiction_ids.contains(contradiction_id) {
                return false;
            }
        }
        if let Some(human_input_request_id) = self.human_input_request_id.as_ref() {
            if record.human_input_request_id.as_ref() != Some(human_input_request_id) {
                return false;
            }
        }
        if let Some(related_record_id) = self.related_record_id.as_ref() {
            if !record.related_record_ids.contains(related_record_id) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinationLog {
    root_dir: PathBuf,
}

impl CoordinationLog {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
        }
    }

    pub fn under_repo(repo_root: &Path) -> Self {
        Self::new(canonical_coordination_log_root(repo_root))
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn wave_path(&self, wave_id: u32) -> PathBuf {
        canonical_coordination_log_path(&self.root_dir, wave_id)
    }

    pub fn append(&self, record: &CoordinationRecord) -> Result<()> {
        append_coordination_record(&self.wave_path(record.wave_id), record)
    }

    pub fn append_many(&self, records: &[CoordinationRecord]) -> Result<()> {
        let mut records_by_wave = BTreeMap::<u32, Vec<CoordinationRecord>>::new();
        for record in records {
            records_by_wave
                .entry(record.wave_id)
                .or_default()
                .push(record.clone());
        }
        for (wave_id, wave_records) in records_by_wave {
            append_coordination_records(&self.wave_path(wave_id), &wave_records)?;
        }
        Ok(())
    }

    pub fn load_wave(&self, wave_id: u32) -> Result<Vec<CoordinationRecord>> {
        load_coordination_records(&self.wave_path(wave_id))
    }

    pub fn load_all(&self) -> Result<Vec<CoordinationRecord>> {
        load_coordination_records_under(&self.root_dir)
    }

    pub fn latest_wave(&self, wave_id: u32) -> Result<Option<CoordinationRecord>> {
        latest_coordination_record(&self.wave_path(wave_id))
    }

    pub fn query(&self, query: &CoordinationRecordQuery) -> Result<Vec<CoordinationRecord>> {
        if let Some(wave_id) = query.wave_id {
            return query_coordination_records(&self.wave_path(wave_id), query);
        }
        self.query_all(query)
    }

    pub fn query_all(&self, query: &CoordinationRecordQuery) -> Result<Vec<CoordinationRecord>> {
        query_coordination_records_under(&self.root_dir, query)
    }

    pub fn list_waves(&self) -> Result<Vec<u32>> {
        coordination_wave_ids_under(&self.root_dir)
    }
}

pub fn canonical_coordination_log_root(repo_root: &Path) -> PathBuf {
    repo_root.join(DEFAULT_STATE_EVENTS_COORDINATION_DIR)
}

pub fn canonical_coordination_log_path(dir: &Path, wave_id: u32) -> PathBuf {
    dir.join(format!("wave-{wave_id:02}.jsonl"))
}

pub fn canonical_coordination_log_path_under(repo_root: &Path, wave_id: u32) -> PathBuf {
    canonical_coordination_log_path(&canonical_coordination_log_root(repo_root), wave_id)
}

pub fn append_coordination_record(path: &Path, record: &CoordinationRecord) -> Result<()> {
    append_coordination_records(path, std::slice::from_ref(record))
}

pub fn append_coordination_records(path: &Path, records: &[CoordinationRecord]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    for record in records {
        serde_json::to_writer(&mut file, record).with_context(|| {
            format!("failed to serialize coordination record {}", path.display())
        })?;
        file.write_all(b"\n")
            .with_context(|| format!("failed to append newline to {}", path.display()))?;
    }
    Ok(())
}

pub fn load_coordination_records(path: &Path) -> Result<Vec<CoordinationRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();
    for (line_number, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("failed to read {}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let record = serde_json::from_str::<CoordinationRecord>(&line).with_context(|| {
            format!(
                "failed to parse coordination record {}:{}",
                path.display(),
                line_number + 1
            )
        })?;
        records.push(record);
    }
    Ok(records)
}

pub fn load_coordination_records_under(dir: &Path) -> Result<Vec<CoordinationRecord>> {
    let mut records = Vec::new();
    for wave_id in coordination_wave_ids_under(dir)? {
        records.extend(load_coordination_records(
            &canonical_coordination_log_path(dir, wave_id),
        )?);
    }
    Ok(records)
}

pub fn latest_coordination_record(path: &Path) -> Result<Option<CoordinationRecord>> {
    Ok(load_coordination_records(path)?.into_iter().last())
}

pub fn query_coordination_records(
    path: &Path,
    query: &CoordinationRecordQuery,
) -> Result<Vec<CoordinationRecord>> {
    Ok(load_coordination_records(path)?
        .into_iter()
        .filter(|record| query.matches(record))
        .collect())
}

pub fn query_coordination_records_under(
    dir: &Path,
    query: &CoordinationRecordQuery,
) -> Result<Vec<CoordinationRecord>> {
    Ok(load_coordination_records_under(dir)?
        .into_iter()
        .filter(|record| query.matches(record))
        .collect())
}

pub fn coordination_wave_ids_under(dir: &Path) -> Result<Vec<u32>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut wave_ids = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter_map(|path| parse_coordination_wave_id(&path))
        .collect::<Vec<_>>();
    wave_ids.sort_unstable();
    Ok(wave_ids)
}

pub fn parse_coordination_wave_id(path: &Path) -> Option<u32> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(COORDINATION_LOG_FILE_PREFIX))
        .and_then(|name| name.strip_suffix(COORDINATION_LOG_FILE_SUFFIX))
        .and_then(|wave_id| wave_id.parse::<u32>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    #[test]
    fn appends_and_loads_coordination_jsonl() {
        let root = temp_root("coordination");
        let log = CoordinationLog::under_repo(&root);
        let contradiction_id = ContradictionId::new("contradiction-1");
        let fact_id = FactId::new("fact-1");

        log.append(
            &CoordinationRecord::new(
                "rec-1",
                CoordinationRecordKind::Claim,
                10,
                "Authority core landed",
            )
            .with_task_id(TaskId::new("wave-10:agent-a1"))
            .with_agent_id("A1")
            .with_created_at_ms(1)
            .with_detail("typed domain and event store compile")
            .with_citations(vec![CoordinationCitation {
                path: "crates/wave-domain/src/lib.rs".to_string(),
                line: Some(1),
                note: Some("task seeds".to_string()),
            }])
            .with_fact_ids(vec![fact_id.clone()])
            .with_contradiction_ids(vec![contradiction_id.clone()]),
        )
        .expect("append");
        log.append(
            &CoordinationRecord::new(
                "rec-2",
                CoordinationRecordKind::Evidence,
                10,
                "Wave 10 integration is unblocked",
            )
            .with_task_id(TaskId::new("wave-10:agent-a8"))
            .with_agent_id("A8")
            .with_created_at_ms(2)
            .with_related_record_ids(vec!["rec-1".to_string()]),
        )
        .expect("append");

        let path = log.wave_path(10);
        let records = load_coordination_records(&path).expect("load");
        assert_eq!(records.len(), 2);
        assert_eq!(
            records[0].schema_version,
            COORDINATION_RECORD_SCHEMA_VERSION
        );
        assert_eq!(records[0].fact_ids, vec![fact_id]);
        assert_eq!(records[0].contradiction_ids, vec![contradiction_id]);
        assert_eq!(records[0].record_id, "rec-1");
        assert_eq!(
            log.latest_wave(10)
                .expect("latest")
                .expect("some")
                .record_id,
            "rec-2"
        );
        assert_eq!(
            canonical_coordination_log_path_under(&root, 10),
            root.join(".wave/state/events/coordination/wave-10.jsonl")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn queries_coordination_records_by_fact_and_escalation() {
        let root = temp_root("coordination-query");
        let log = CoordinationLog::new(root.join("coordination"));
        let fact_id = FactId::new("fact-1");
        let request_id = HumanInputRequestId::new("human-1");

        log.append_many(&[
            CoordinationRecord::new(
                "rec-1",
                CoordinationRecordKind::Evidence,
                10,
                "fact observed",
            )
            .with_task_id(TaskId::new("wave-10:agent-a1"))
            .with_fact_ids(vec![fact_id.clone()]),
            CoordinationRecord::new(
                "rec-2",
                CoordinationRecordKind::Escalation,
                10,
                "need human decision",
            )
            .with_human_input_request_id(request_id.clone()),
            CoordinationRecord::new("rec-3", CoordinationRecordKind::Evidence, 11, "other wave"),
        ])
        .expect("append many");

        let mut fact_query = CoordinationRecordQuery::for_wave(10);
        fact_query.fact_id = Some(fact_id);
        let fact_records = log.query(&fact_query).expect("fact query");
        assert_eq!(fact_records.len(), 1);
        assert_eq!(fact_records[0].record_id, "rec-1");

        let mut escalation_query = CoordinationRecordQuery::for_wave(10);
        escalation_query.kind = Some(CoordinationRecordKind::Escalation);
        escalation_query.human_input_request_id = Some(request_id);
        let escalations = log.query(&escalation_query).expect("escalation query");
        assert_eq!(escalations.len(), 1);
        assert_eq!(escalations[0].record_id, "rec-2");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lists_coordination_streams_and_queries_across_root() {
        let root = temp_root("coordination-root");
        let log = CoordinationLog::under_repo(&root);

        log.append_many(&[
            CoordinationRecord::new("rec-1", CoordinationRecordKind::Claim, 10, "claim"),
            CoordinationRecord::new("rec-2", CoordinationRecordKind::Decision, 11, "decision")
                .with_related_record_ids(vec!["rec-1".to_string()]),
        ])
        .expect("append many");

        assert_eq!(log.list_waves().expect("waves"), vec![10, 11]);
        assert_eq!(log.load_all().expect("all").len(), 2);

        let query = CoordinationRecordQuery {
            wave_id: None,
            task_id: None,
            agent_id: None,
            record_id: None,
            kind: Some(CoordinationRecordKind::Decision),
            fact_id: None,
            contradiction_id: None,
            human_input_request_id: None,
            related_record_id: Some("rec-1".to_string()),
        };
        let records = log.query(&query).expect("query all");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].wave_id, 11);

        let _ = fs::remove_dir_all(root);
    }

    fn temp_root(label: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis();
        std::env::temp_dir().join(format!("wave-{label}-{}-{millis}", std::process::id()))
    }
}
