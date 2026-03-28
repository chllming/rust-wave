use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use sha2::Digest;
use sha2::Sha256;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use wave_config::DEFAULT_STATE_RESULTS_DIR;
use wave_domain::AttemptId;
use wave_domain::AttemptState;
use wave_domain::ClosureDisposition;
use wave_domain::ClosureInputEnvelope;
use wave_domain::ClosureState;
use wave_domain::ClosureVerdictPayload;
use wave_domain::ContQaClosureVerdict;
use wave_domain::DesignClosureVerdict;
use wave_domain::DocDeltaEnvelope;
use wave_domain::DocumentationClosureVerdict;
use wave_domain::FinalMarkerEnvelope;
use wave_domain::IntegrationClosureVerdict;
use wave_domain::MarkerEvidence;
use wave_domain::ProofArtifact;
use wave_domain::ProofEnvelope;
use wave_domain::ResultDisposition;
use wave_domain::ResultEnvelope;
use wave_domain::ResultEnvelopeId;
use wave_domain::ResultEnvelopeSource;
use wave_domain::ResultPayloadStatus;
use wave_domain::SecurityClosureVerdict;
use wave_domain::TaskId;
use wave_domain::inferred_closure_role_for_agent;
use wave_domain::inferred_task_role_for_agent;
use wave_domain::task_id_for_agent;
use wave_spec::WaveAgent;
use wave_trace::AgentRunRecord;
use wave_trace::WaveRunRecord;
use wave_trace::WaveRunStatus;

pub const RESULT_ENVELOPE_FILE_NAME: &str = "agent_result_envelope.json";

pub mod compatibility {
    use super::*;

    pub fn adapt_legacy_run_record(
        repo_root: &Path,
        run: &WaveRunRecord,
    ) -> Result<Vec<ResultEnvelope>> {
        super::adapt_legacy_run_record_impl(repo_root, run)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultEnvelopeStore {
    root_dir: PathBuf,
    repo_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct ClosureTextArtifact {
    path: PathBuf,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RuntimeDetailSnapshot {
    runtime: Option<wave_domain::RuntimeExecutionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveResultEnvelopeView {
    pub attempt_state: AttemptState,
    pub disposition: ResultDisposition,
    pub source: ResultEnvelopeSource,
    pub required_final_markers: Vec<String>,
    pub observed_final_markers: Vec<String>,
    pub summary: Option<String>,
    pub runtime: Option<wave_domain::RuntimeExecutionRecord>,
}

impl ResultEnvelopeStore {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
            repo_root: None,
        }
    }

    pub fn under_repo(repo_root: &Path) -> Self {
        Self {
            root_dir: canonical_results_root(repo_root),
            repo_root: Some(repo_root.to_path_buf()),
        }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn wave_path(&self, wave_id: u32) -> PathBuf {
        self.root_dir.join(format!("wave-{wave_id:02}"))
    }

    pub fn attempt_path(&self, wave_id: u32, attempt_id: &AttemptId) -> PathBuf {
        self.wave_path(wave_id).join(attempt_id.as_str())
    }

    pub fn envelope_path(&self, wave_id: u32, attempt_id: &AttemptId) -> PathBuf {
        self.attempt_path(wave_id, attempt_id)
            .join(RESULT_ENVELOPE_FILE_NAME)
    }

    pub fn envelope_path_for(&self, envelope: &ResultEnvelope) -> PathBuf {
        self.envelope_path(envelope.wave_id, &envelope.attempt_id)
    }

    pub fn write_envelope(&self, envelope: &ResultEnvelope) -> Result<PathBuf> {
        let normalized = normalize_result_envelope(envelope, self.repo_root.as_deref())?;
        let path = self.envelope_path_for(&normalized);
        if path.exists() {
            let existing = self.load_envelope(&path)?;
            if existing == normalized {
                return Ok(path);
            }
            bail!(
                "result envelope storage is immutable for attempt {}; existing envelope at {} differs from the new payload",
                normalized.attempt_id,
                path.display()
            );
        }
        fs::create_dir_all(
            path.parent()
                .context("result envelope path is missing a parent directory")?,
        )
        .with_context(|| format!("failed to create {}", path.display()))?;
        fs::write(&path, serde_json::to_string_pretty(&normalized)?)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(path)
    }

    pub fn load_envelope(&self, path: &Path) -> Result<ResultEnvelope> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read result envelope {}", path.display()))?;
        let envelope = serde_json::from_str::<ResultEnvelope>(&raw)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        validate_result_envelope(&envelope)?;
        Ok(envelope)
    }

    pub fn load_attempt_envelope(
        &self,
        wave_id: u32,
        attempt_id: &AttemptId,
    ) -> Result<Option<ResultEnvelope>> {
        let path = self.envelope_path(wave_id, attempt_id);
        if !path.exists() {
            return Ok(None);
        }
        self.load_envelope(&path).map(Some)
    }

    pub fn load_wave_envelopes(&self, wave_id: u32) -> Result<Vec<ResultEnvelope>> {
        let wave_dir = self.wave_path(wave_id);
        if !wave_dir.exists() {
            return Ok(Vec::new());
        }

        let mut envelopes = Vec::new();
        for entry in fs::read_dir(&wave_dir)
            .with_context(|| format!("failed to read {}", wave_dir.display()))?
        {
            let entry =
                entry.with_context(|| format!("failed to read entry in {}", wave_dir.display()))?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let envelope_path = path.join(RESULT_ENVELOPE_FILE_NAME);
            if envelope_path.exists() {
                envelopes.push(self.load_envelope(&envelope_path)?);
            }
        }

        envelopes.sort_by(compare_envelopes);
        Ok(envelopes)
    }

    pub fn latest_task_envelope(
        &self,
        wave_id: u32,
        task_id: &TaskId,
    ) -> Result<Option<ResultEnvelope>> {
        Ok(self
            .load_wave_envelopes(wave_id)?
            .into_iter()
            .filter(|envelope| &envelope.task_id == task_id)
            .max_by(compare_envelopes))
    }

    pub fn latest_terminal_task_envelope(
        &self,
        wave_id: u32,
        task_id: &TaskId,
    ) -> Result<Option<ResultEnvelope>> {
        Ok(self
            .load_wave_envelopes(wave_id)?
            .into_iter()
            .filter(|envelope| &envelope.task_id == task_id && envelope.is_terminal())
            .max_by(compare_envelopes))
    }

    pub fn latest_completed_or_failed_task_envelope(
        &self,
        wave_id: u32,
        task_id: &TaskId,
    ) -> Result<Option<ResultEnvelope>> {
        Ok(self
            .load_wave_envelopes(wave_id)?
            .into_iter()
            .filter(|envelope| {
                &envelope.task_id == task_id && envelope.should_surface_as_latest_relevant()
            })
            .max_by(compare_envelopes))
    }
}

pub fn canonical_results_root(repo_root: &Path) -> PathBuf {
    repo_root.join(DEFAULT_STATE_RESULTS_DIR)
}

pub fn build_structured_result_envelope(
    repo_root: &Path,
    run: &WaveRunRecord,
    declared_agent: &WaveAgent,
    agent_record: &AgentRunRecord,
    created_at_ms: u128,
) -> Result<ResultEnvelope> {
    let attempt_state = attempt_state_from_status(run.dry_run, agent_record.status);
    let required_final_markers = declared_agent
        .expected_final_markers()
        .iter()
        .map(|marker| (*marker).to_string())
        .collect::<Vec<_>>();
    let closure_root = closure_execution_root(repo_root, run);
    let last_message_path = resolve_path(repo_root, &agent_record.last_message_path);
    let output_text = read_optional_text(&last_message_path)?;
    let closure_text_artifacts = collect_structured_closure_text_artifacts(
        &closure_root,
        run.wave_id,
        declared_agent.id.as_str(),
        &last_message_path,
        output_text.as_deref(),
    )?;
    let inferred_observed_markers = merge_markers(
        agent_record.observed_markers.clone(),
        observed_markers_in_text_artifacts(&closure_text_artifacts, &required_final_markers),
    );
    let final_markers =
        FinalMarkerEnvelope::from_contract(required_final_markers, inferred_observed_markers);
    let marker_evidence = collect_marker_evidence_from_text_artifacts(
        &closure_text_artifacts,
        &final_markers.observed,
        repo_root,
        None,
    );
    let closure = build_structured_closure_state_from_text_artifacts(
        declared_agent.id.as_str(),
        attempt_state,
        &final_markers,
        agent_record.error.as_deref(),
        &closure_text_artifacts,
    );

    normalize_result_envelope(
        &ResultEnvelope {
            result_envelope_id: ResultEnvelopeId::new(format!(
                "result:{}:{}",
                run.run_id,
                declared_agent.id.to_ascii_lowercase()
            )),
            wave_id: run.wave_id,
            task_id: task_id_for_agent(run.wave_id, &declared_agent.id),
            attempt_id: AttemptId::new(format!(
                "{}-{}",
                run.run_id,
                declared_agent.id.to_ascii_lowercase()
            )),
            agent_id: declared_agent.id.clone(),
            task_role: inferred_task_role_for_agent(
                declared_agent.id.as_str(),
                &declared_agent.skills,
            ),
            closure_role: inferred_closure_role_for_agent(declared_agent.id.as_str()),
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
            proof: ProofEnvelope {
                status: ResultPayloadStatus::Missing,
                summary: None,
                proof_bundle_ids: Vec::new(),
                fact_ids: Vec::new(),
                contradiction_ids: Vec::new(),
                artifacts: build_structured_result_artifacts(
                    repo_root,
                    run,
                    agent_record,
                    declared_agent.id.as_str(),
                ),
            },
            doc_delta: build_structured_doc_delta(repo_root, declared_agent, &final_markers),
            closure_input: ClosureInputEnvelope {
                status: ResultPayloadStatus::Missing,
                final_markers,
                marker_evidence,
            },
            closure,
            runtime: agent_record.runtime.clone(),
            created_at_ms,
        },
        Some(repo_root),
    )
}

pub fn build_structured_closure_state(
    agent_id: &str,
    attempt_state: AttemptState,
    final_markers: &FinalMarkerEnvelope,
    agent_error: Option<&str>,
    output_text: Option<&str>,
) -> ClosureState {
    let text_artifacts = output_text
        .map(|text| {
            vec![ClosureTextArtifact {
                path: PathBuf::new(),
                text: text.to_string(),
            }]
        })
        .unwrap_or_default();
    build_structured_closure_state_from_text_artifacts(
        agent_id,
        attempt_state,
        final_markers,
        agent_error,
        &text_artifacts,
    )
}

fn build_structured_closure_state_from_text_artifacts(
    agent_id: &str,
    attempt_state: AttemptState,
    final_markers: &FinalMarkerEnvelope,
    agent_error: Option<&str>,
    text_artifacts: &[ClosureTextArtifact],
) -> ClosureState {
    let verdict = derive_closure_verdict_payload(agent_id, text_artifacts);
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
    if let Some(error) = closure_contract_issue(agent_id, final_markers, &verdict) {
        blocking_reasons.push(error);
    }

    let disposition = ClosureState::expected_result_envelope_disposition(
        attempt_state,
        final_markers,
        &blocking_reasons,
    );

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

fn closure_contract_issue(
    agent_id: &str,
    final_markers: &FinalMarkerEnvelope,
    verdict: &ClosureVerdictPayload,
) -> Option<String> {
    closure_contract_error(
        agent_id,
        &ClosureState {
            disposition: ClosureDisposition::Pending,
            required_final_markers: final_markers.required.clone(),
            observed_final_markers: final_markers.observed.clone(),
            blocking_reasons: Vec::new(),
            satisfied_fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            verdict: verdict.clone(),
        },
    )
}

pub fn closure_contract_error(agent_id: &str, closure: &ClosureState) -> Option<String> {
    match (agent_id, &closure.verdict) {
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
        ("A6", ClosureVerdictPayload::Design(verdict)) => match verdict.state.as_deref() {
            Some("aligned") | Some("concerns") => None,
            Some(state) => Some(format!(
                "design review state is {state}, not aligned or concerns"
            )),
            None => Some("design review report is missing state=<...>".to_string()),
        },
        ("A6", _) => Some("design review report is missing structured closure verdict".to_string()),
        ("A7", ClosureVerdictPayload::Security(verdict)) => match verdict.state.as_deref() {
            Some("clear") | Some("concerns") => None,
            Some(state) => Some(format!(
                "security review state is {state}, not clear or concerns"
            )),
            None => Some("security review report is missing state=<...>".to_string()),
        },
        ("A7", _) => {
            Some("security review report is missing structured closure verdict".to_string())
        }
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

fn attempt_state_from_status(dry_run: bool, status: WaveRunStatus) -> AttemptState {
    if dry_run {
        return AttemptState::Refused;
    }

    match status {
        WaveRunStatus::Planned => AttemptState::Planned,
        WaveRunStatus::Running => AttemptState::Running,
        WaveRunStatus::Succeeded => AttemptState::Succeeded,
        WaveRunStatus::Failed => AttemptState::Failed,
        WaveRunStatus::DryRun => AttemptState::Refused,
    }
}

fn build_structured_doc_delta(
    repo_root: &Path,
    declared_agent: &WaveAgent,
    final_markers: &FinalMarkerEnvelope,
) -> DocDeltaEnvelope {
    let observed = final_markers
        .observed
        .iter()
        .any(|marker| marker == "[wave-doc-delta]");
    let doc_delta_paths = declared_agent
        .file_ownership
        .iter()
        .filter(|path| looks_like_doc_path(path))
        .map(|path| normalize_path_string(&PathBuf::from(path), Some(repo_root)))
        .collect::<Vec<_>>();
    let status = if observed {
        if doc_delta_paths.is_empty() {
            ResultPayloadStatus::EvidenceOnly
        } else {
            ResultPayloadStatus::Recorded
        }
    } else {
        ResultPayloadStatus::Missing
    };

    DocDeltaEnvelope {
        status,
        summary: if !observed || doc_delta_paths.is_empty() {
            None
        } else {
            Some(format!("doc delta paths: {}", doc_delta_paths.join(", ")))
        },
        paths: if observed {
            doc_delta_paths
        } else {
            Vec::new()
        },
    }
}

fn build_structured_result_artifacts(
    repo_root: &Path,
    run: &WaveRunRecord,
    agent_record: &AgentRunRecord,
    agent_id: &str,
) -> Vec<ProofArtifact> {
    let closure_root = closure_execution_root(repo_root, run);
    let mut artifacts = vec![
        ProofArtifact {
            path: resolve_path(repo_root, &agent_record.last_message_path)
                .to_string_lossy()
                .into_owned(),
            kind: wave_domain::ArtifactKind::Other,
            digest: None,
            note: Some("last-message".to_string()),
        },
        ProofArtifact {
            path: resolve_path(repo_root, &agent_record.events_path)
                .to_string_lossy()
                .into_owned(),
            kind: wave_domain::ArtifactKind::Other,
            digest: None,
            note: Some("events".to_string()),
        },
        ProofArtifact {
            path: resolve_path(repo_root, &agent_record.stderr_path)
                .to_string_lossy()
                .into_owned(),
            kind: wave_domain::ArtifactKind::Other,
            digest: None,
            note: Some("stderr".to_string()),
        },
        ProofArtifact {
            path: resolve_path(repo_root, &run.trace_path)
                .to_string_lossy()
                .into_owned(),
            kind: wave_domain::ArtifactKind::Trace,
            digest: None,
            note: Some(run.run_id.clone()),
        },
    ];

    for (path, note) in structured_closure_artifact_paths(&closure_root, run.wave_id, agent_id) {
        if !path.exists() {
            continue;
        }
        artifacts.push(ProofArtifact {
            path: path.to_string_lossy().into_owned(),
            kind: wave_domain::ArtifactKind::Review,
            digest: None,
            note: Some(note.to_string()),
        });
    }

    artifacts
}

fn collect_marker_evidence_from_text_artifacts(
    text_artifacts: &[ClosureTextArtifact],
    observed_markers: &[String],
    repo_root: &Path,
    synthetic_source: Option<&str>,
) -> Vec<MarkerEvidence> {
    let mut evidence = Vec::new();

    for artifact in text_artifacts {
        let source = normalize_path_string(&artifact.path, Some(repo_root));
        for line in artifact
            .text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            for marker in observed_markers {
                if line == marker || line.starts_with(&(marker.clone() + " ")) {
                    evidence.push(MarkerEvidence {
                        marker: marker.clone(),
                        line: line.to_string(),
                        source: Some(source.clone()),
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
                source: synthetic_source.map(ToString::to_string),
            });
        }
    }

    normalize_marker_evidence(&evidence, Some(repo_root))
}

fn collect_structured_closure_text_artifacts(
    execution_root: &Path,
    wave_id: u32,
    agent_id: &str,
    last_message_path: &Path,
    output_text: Option<&str>,
) -> Result<Vec<ClosureTextArtifact>> {
    let mut artifacts = Vec::new();
    if let Some(text) = output_text {
        artifacts.push(ClosureTextArtifact {
            path: last_message_path.to_path_buf(),
            text: text.to_string(),
        });
    }

    for (path, _) in structured_closure_artifact_paths(execution_root, wave_id, agent_id) {
        if let Some(text) = read_optional_text(&path)? {
            artifacts.push(ClosureTextArtifact { path, text });
        }
    }

    Ok(artifacts)
}

fn structured_closure_artifact_paths(
    execution_root: &Path,
    wave_id: u32,
    agent_id: &str,
) -> Vec<(PathBuf, &'static str)> {
    match agent_id {
        "A0" => vec![(
            execution_root.join(format!(".wave/reviews/wave-{wave_id}-cont-qa.md")),
            "cont-qa-review",
        )],
        "A6" => vec![(
            execution_root.join(format!(".wave/design/wave-{wave_id}.md")),
            "design-review",
        )],
        "A7" => vec![(
            execution_root.join(format!(".wave/security/wave-{wave_id}.md")),
            "security-review",
        )],
        "A8" => vec![
            (
                execution_root.join(format!(".wave/integration/wave-{wave_id}.md")),
                "integration-summary",
            ),
            (
                execution_root.join(format!(".wave/integration/wave-{wave_id}.json")),
                "integration-summary-json",
            ),
        ],
        _ => Vec::new(),
    }
}

fn closure_execution_root(repo_root: &Path, run: &WaveRunRecord) -> PathBuf {
    run.worktree
        .as_ref()
        .map(|worktree| resolve_path(repo_root, Path::new(worktree.path.as_str())))
        .unwrap_or_else(|| repo_root.to_path_buf())
}

fn looks_like_doc_path(path: &str) -> bool {
    path == "README.md"
        || path.starts_with("docs/")
        || path.ends_with(".md")
            && (path.contains("/docs/") || path.starts_with("docs/") || !path.contains('/'))
}

fn normalize_proof_status(
    proof: &ProofEnvelope,
    closure_input: &ClosureInputEnvelope,
) -> ResultPayloadStatus {
    if proof.has_recorded_payload() {
        ResultPayloadStatus::Recorded
    } else if marker_was_observed(closure_input, "[wave-proof]") {
        ResultPayloadStatus::EvidenceOnly
    } else {
        ResultPayloadStatus::Missing
    }
}

fn normalize_doc_delta_status(
    doc_delta: &DocDeltaEnvelope,
    closure_input: &ClosureInputEnvelope,
) -> ResultPayloadStatus {
    if doc_delta.has_recorded_payload() {
        ResultPayloadStatus::Recorded
    } else if marker_was_observed(closure_input, "[wave-doc-delta]") {
        ResultPayloadStatus::EvidenceOnly
    } else {
        ResultPayloadStatus::Missing
    }
}

fn normalize_closure_input_status(
    source: ResultEnvelopeSource,
    closure_input: &ClosureInputEnvelope,
    closure: &ClosureState,
) -> ResultPayloadStatus {
    if source == ResultEnvelopeSource::Structured && closure_input.has_evidence() {
        return ResultPayloadStatus::Recorded;
    }

    if closure_input.has_evidence() || closure.has_machine_readable_signal() {
        ResultPayloadStatus::EvidenceOnly
    } else {
        ResultPayloadStatus::Missing
    }
}

fn marker_was_observed(closure_input: &ClosureInputEnvelope, marker: &str) -> bool {
    closure_input
        .final_markers
        .observed
        .iter()
        .any(|observed| observed == marker)
        || closure_input
            .marker_evidence
            .iter()
            .any(|evidence| evidence.marker == marker)
}

pub fn normalize_result_envelope(
    envelope: &ResultEnvelope,
    repo_root: Option<&Path>,
) -> Result<ResultEnvelope> {
    let mut normalized = envelope.clone();
    normalized.closure_input.final_markers = FinalMarkerEnvelope::from_contract(
        normalized.closure_input.final_markers.required.clone(),
        normalized.closure_input.final_markers.observed.clone(),
    );
    normalized.proof.artifacts = normalize_proof_artifacts(&normalized.proof.artifacts, repo_root)?;
    normalized.doc_delta.paths = normalize_paths(&normalized.doc_delta.paths, repo_root);
    normalized.closure_input.marker_evidence =
        normalize_marker_evidence(&normalized.closure_input.marker_evidence, repo_root);
    normalized.proof.status = normalize_proof_status(&normalized.proof, &normalized.closure_input);
    normalized.doc_delta.status =
        normalize_doc_delta_status(&normalized.doc_delta, &normalized.closure_input);
    normalized.closure_input.status = normalize_closure_input_status(
        normalized.source,
        &normalized.closure_input,
        &normalized.closure,
    );
    normalized.closure.required_final_markers =
        normalized.closure_input.final_markers.required.clone();
    normalized.closure.observed_final_markers =
        normalized.closure_input.final_markers.observed.clone();
    if let Some(runtime) = &mut normalized.runtime {
        *runtime = runtime.normalized();
        for path in runtime.execution_identity.artifact_paths.values_mut() {
            *path = normalize_path_string(&PathBuf::from(path.as_str()), repo_root);
        }
    }
    validate_result_envelope(&normalized)?;
    Ok(normalized)
}

pub fn validate_result_envelope(envelope: &ResultEnvelope) -> Result<()> {
    let mut issues = Vec::new();

    if envelope.result_envelope_id.as_str().trim().is_empty() {
        issues.push("result envelope id must not be empty".to_string());
    }
    if envelope.task_id.as_str().trim().is_empty() {
        issues.push("task id must not be empty".to_string());
    }
    if envelope.attempt_id.as_str().trim().is_empty() {
        issues.push("attempt id must not be empty".to_string());
    }
    if envelope.agent_id.trim().is_empty() {
        issues.push("agent id must not be empty".to_string());
    }

    let normalized_markers = FinalMarkerEnvelope::from_contract(
        envelope.closure_input.final_markers.required.clone(),
        envelope.closure_input.final_markers.observed.clone(),
    );
    if envelope.closure_input.final_markers != normalized_markers {
        issues.push("final marker payload must be normalized and deduplicated".to_string());
    }
    if envelope.closure.required_final_markers != envelope.closure_input.final_markers.required {
        issues.push(
            "closure.required_final_markers must match closure_input.final_markers.required"
                .to_string(),
        );
    }
    if envelope.closure.observed_final_markers != envelope.closure_input.final_markers.observed {
        issues.push(
            "closure.observed_final_markers must match closure_input.final_markers.observed"
                .to_string(),
        );
    }
    let closure_payload_matches_role = matches!(
        (&envelope.closure_role, &envelope.closure.verdict),
        (None, ClosureVerdictPayload::None)
            | (
                Some(wave_domain::ClosureRole::ContQa),
                ClosureVerdictPayload::ContQa(_)
            )
            | (
                Some(wave_domain::ClosureRole::DesignReview),
                ClosureVerdictPayload::Design(_)
            )
            | (
                Some(wave_domain::ClosureRole::SecurityReview),
                ClosureVerdictPayload::Security(_)
            )
            | (
                Some(wave_domain::ClosureRole::Integration),
                ClosureVerdictPayload::Integration(_)
            )
            | (
                Some(wave_domain::ClosureRole::Documentation),
                ClosureVerdictPayload::Documentation(_)
            )
            | (
                Some(wave_domain::ClosureRole::ContEval),
                ClosureVerdictPayload::None
            )
    );
    if !closure_payload_matches_role {
        issues.push("closure.verdict must match closure_role".to_string());
    }
    if !envelope.closure.matches_result_envelope_disposition(
        envelope.attempt_state,
        &envelope.closure_input.final_markers,
    ) {
        issues.push(format!(
            "closure.disposition {:?} does not match attempt_state {:?}, final markers, and blocking reasons",
            envelope.closure.disposition, envelope.attempt_state
        ));
    }
    if !matches!(
        envelope.attempt_state,
        AttemptState::Planned | AttemptState::Running
    ) {
        if let Some(issue) = closure_contract_issue(
            envelope.agent_id.as_str(),
            &envelope.closure_input.final_markers,
            &envelope.closure.verdict,
        ) {
            if !envelope
                .closure
                .blocking_reasons
                .iter()
                .any(|reason| reason == &issue)
            {
                issues.push(format!(
                    "closure.blocking_reasons must include closure contract issue: {issue}"
                ));
            }
        }
    }
    if let Some(runtime) = &envelope.runtime {
        if runtime.policy != runtime.policy.normalized() {
            issues.push("runtime.policy must be normalized".to_string());
        }
        if runtime.skill_projection != runtime.skill_projection.normalized() {
            issues.push("runtime.skill_projection must be normalized".to_string());
        }
        let expected_adapter = format!("wave-runtime/{}", runtime.selected_runtime.as_str());
        if runtime.execution_identity.adapter != expected_adapter {
            issues.push(format!(
                "runtime.execution_identity.adapter must match {}",
                expected_adapter
            ));
        }
        if runtime.execution_identity.runtime != runtime.selected_runtime {
            issues.push(
                "runtime.execution_identity.runtime must match runtime.selected_runtime"
                    .to_string(),
            );
        }
        let expected_runtime_skill = runtime.selected_runtime.skill_id();
        for skill in runtime_specific_overlay_skills(
            &runtime.skill_projection.projected_skills,
            &expected_runtime_skill,
        ) {
            issues.push(format!(
                "runtime.skill_projection.projected_skills contains runtime overlay {skill} that does not match runtime.selected_runtime"
            ));
        }
        for skill in runtime_specific_overlay_skills(
            &runtime.skill_projection.auto_attached_skills,
            &expected_runtime_skill,
        ) {
            issues.push(format!(
                "runtime.skill_projection.auto_attached_skills contains runtime overlay {skill} that does not match runtime.selected_runtime"
            ));
        }
        if let Some(fallback) = &runtime.fallback {
            if runtime.policy.requested_runtime != Some(fallback.requested_runtime) {
                issues.push(
                    "runtime.fallback.requested_runtime must match runtime.policy.requested_runtime"
                        .to_string(),
                );
            }
            if fallback.selected_runtime != runtime.selected_runtime {
                issues.push(
                    "runtime.fallback.selected_runtime must match runtime.selected_runtime"
                        .to_string(),
                );
            }
        }
    }

    let expected_disposition = envelope.expected_disposition();
    if envelope.disposition != expected_disposition {
        issues.push(format!(
            "result disposition {:?} does not match attempt_state {:?} and {} missing markers",
            envelope.disposition,
            envelope.attempt_state,
            envelope.closure_input.final_markers.missing.len()
        ));
    }
    let expected_proof_status = normalize_proof_status(&envelope.proof, &envelope.closure_input);
    if envelope.proof.status != expected_proof_status {
        issues.push(format!(
            "proof.status {:?} does not match normalized status {:?}",
            envelope.proof.status, expected_proof_status
        ));
    }
    let expected_doc_delta_status =
        normalize_doc_delta_status(&envelope.doc_delta, &envelope.closure_input);
    if envelope.doc_delta.status != expected_doc_delta_status {
        issues.push(format!(
            "doc_delta.status {:?} does not match normalized status {:?}",
            envelope.doc_delta.status, expected_doc_delta_status
        ));
    }
    let expected_closure_input_status =
        normalize_closure_input_status(envelope.source, &envelope.closure_input, &envelope.closure);
    if envelope.closure_input.status != expected_closure_input_status {
        issues.push(format!(
            "closure_input.status {:?} does not match normalized status {:?}",
            envelope.closure_input.status, expected_closure_input_status
        ));
    }

    if !issues.is_empty() {
        bail!(issues.join("; "));
    }

    Ok(())
}

fn runtime_specific_overlay_skills(skills: &[String], expected_runtime_skill: &str) -> Vec<String> {
    skills
        .iter()
        .filter(|skill| skill.starts_with("runtime-") && skill.as_str() != expected_runtime_skill)
        .cloned()
        .collect()
}

pub fn adapt_legacy_run_record(
    repo_root: &Path,
    run: &WaveRunRecord,
) -> Result<Vec<ResultEnvelope>> {
    compatibility::adapt_legacy_run_record(repo_root, run)
}

pub fn resolve_effective_result_envelope_view(
    repo_root: &Path,
    run: &WaveRunRecord,
    agent: &AgentRunRecord,
) -> Result<EffectiveResultEnvelopeView> {
    if let Some(path) = agent
        .result_envelope_path
        .as_ref()
        .filter(|path| path.exists())
    {
        if let Ok(envelope) = ResultEnvelopeStore::under_repo(repo_root).load_envelope(path) {
            return Ok(effective_view_from_domain_envelope(envelope));
        }

        let envelope = wave_trace::load_result_envelope(path)?;
        return Ok(effective_view_from_trace_envelope(envelope));
    }

    if let Some(envelope) = adapt_legacy_run_record(repo_root, run)?
        .into_iter()
        .find(|envelope| envelope.agent_id == agent.id)
    {
        return Ok(effective_view_from_domain_envelope(envelope));
    }

    bail!(
        "no effective result envelope found for wave {} agent {}",
        run.wave_id,
        agent.id
    );
}

fn adapt_legacy_run_record_impl(
    repo_root: &Path,
    run: &WaveRunRecord,
) -> Result<Vec<ResultEnvelope>> {
    run.agents
        .iter()
        .map(|agent| adapt_legacy_agent_run(repo_root, run, agent))
        .collect()
}

fn adapt_legacy_agent_run(
    repo_root: &Path,
    run: &WaveRunRecord,
    agent: &AgentRunRecord,
) -> Result<ResultEnvelope> {
    let attempt_state = legacy_attempt_state(run, agent);
    let closure_root = closure_execution_root(repo_root, run);
    let last_message_path = resolve_path(repo_root, &agent.last_message_path);
    let output_text = read_optional_text(&last_message_path)?;
    let legacy_text_artifacts = collect_structured_closure_text_artifacts(
        &closure_root,
        run.wave_id,
        agent.id.as_str(),
        &last_message_path,
        output_text.as_deref(),
    )?;
    let inferred_observed_markers = merge_markers(
        agent.observed_markers.clone(),
        observed_markers_in_text_artifacts(&legacy_text_artifacts, &agent.expected_markers),
    );
    let final_markers = FinalMarkerEnvelope::from_contract(
        agent.expected_markers.clone(),
        inferred_observed_markers,
    );
    let verdict = derive_closure_verdict_payload(agent.id.as_str(), &legacy_text_artifacts);
    let synthetic_source = format!("legacy-run-record:{}", run.run_id);
    let marker_evidence = collect_marker_evidence_from_text_artifacts(
        &legacy_text_artifacts,
        &final_markers.observed,
        repo_root,
        Some(&synthetic_source),
    );
    let blocking_reasons = legacy_blocking_reasons(attempt_state, &final_markers, agent, &verdict);
    let doc_delta = legacy_doc_delta_payload(agent, &verdict);
    let closure = ClosureState {
        disposition: ClosureState::expected_result_envelope_disposition(
            attempt_state,
            &final_markers,
            &blocking_reasons,
        ),
        required_final_markers: final_markers.required.clone(),
        observed_final_markers: final_markers.observed.clone(),
        blocking_reasons,
        satisfied_fact_ids: Vec::new(),
        contradiction_ids: Vec::new(),
        verdict,
    };
    let runtime = legacy_runtime_record(repo_root, agent)?;

    normalize_result_envelope(
        &ResultEnvelope {
            result_envelope_id: ResultEnvelopeId::new(format!(
                "legacy:{}:{}",
                run.run_id,
                agent.id.to_ascii_lowercase()
            )),
            wave_id: run.wave_id,
            task_id: task_id_for_agent(run.wave_id, &agent.id),
            attempt_id: AttemptId::new(format!(
                "legacy-{}-{}",
                run.run_id,
                agent.id.to_ascii_lowercase()
            )),
            agent_id: agent.id.clone(),
            task_role: inferred_task_role_for_agent(agent.id.as_str(), &[]),
            closure_role: inferred_closure_role_for_agent(agent.id.as_str()),
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
            proof: legacy_proof_payload(
                &final_markers,
                build_structured_result_artifacts(repo_root, run, agent, agent.id.as_str()),
            ),
            doc_delta,
            closure_input: ClosureInputEnvelope {
                status: ResultPayloadStatus::EvidenceOnly,
                final_markers,
                marker_evidence,
            },
            closure,
            runtime,
            created_at_ms: run
                .completed_at_ms
                .or(run.started_at_ms)
                .unwrap_or(run.created_at_ms),
        },
        Some(repo_root),
    )
}

fn legacy_runtime_record(
    repo_root: &Path,
    agent: &AgentRunRecord,
) -> Result<Option<wave_domain::RuntimeExecutionRecord>> {
    let runtime_detail_key = agent.runtime_detail_path.as_ref().map(|path| {
        (
            "runtime_detail".to_string(),
            normalize_path_string(path, Some(repo_root)),
        )
    });

    let runtime = if let Some(runtime) = &agent.runtime {
        Some(runtime.clone())
    } else {
        let Some(runtime_detail_path) = agent.runtime_detail_path.as_ref() else {
            return Ok(None);
        };
        load_runtime_record_from_detail(repo_root, runtime_detail_path)?
    };

    Ok(runtime.map(|mut runtime| {
        if let Some((key, value)) = runtime_detail_key {
            runtime
                .execution_identity
                .artifact_paths
                .entry(key)
                .or_insert(value);
        }
        runtime
    }))
}

fn load_runtime_record_from_detail(
    repo_root: &Path,
    runtime_detail_path: &Path,
) -> Result<Option<wave_domain::RuntimeExecutionRecord>> {
    let resolved_path = resolve_path(repo_root, runtime_detail_path);
    if !resolved_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&resolved_path)
        .with_context(|| format!("failed to read {}", resolved_path.display()))?;
    let snapshot = serde_json::from_str::<RuntimeDetailSnapshot>(&raw)
        .with_context(|| format!("failed to parse {}", resolved_path.display()))?;
    Ok(snapshot.runtime)
}

fn legacy_attempt_state(run: &WaveRunRecord, agent: &AgentRunRecord) -> AttemptState {
    if run.dry_run {
        return AttemptState::Refused;
    }

    match agent.status {
        WaveRunStatus::Planned => AttemptState::Planned,
        WaveRunStatus::Running => AttemptState::Running,
        WaveRunStatus::Succeeded => AttemptState::Succeeded,
        WaveRunStatus::Failed => AttemptState::Failed,
        WaveRunStatus::DryRun => AttemptState::Refused,
    }
}

fn legacy_blocking_reasons(
    attempt_state: AttemptState,
    final_markers: &FinalMarkerEnvelope,
    agent: &AgentRunRecord,
    verdict: &ClosureVerdictPayload,
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
    if let Some(error) = closure_contract_issue(agent.id.as_str(), final_markers, verdict) {
        reasons.push(error);
    }
    reasons
}

fn legacy_doc_delta_payload(
    agent: &AgentRunRecord,
    verdict: &ClosureVerdictPayload,
) -> DocDeltaEnvelope {
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

    let (summary, paths) = match verdict {
        ClosureVerdictPayload::Documentation(verdict) if !verdict.paths.is_empty() => (
            verdict.detail.clone().or_else(|| {
                Some(format!(
                    "documentation closure paths: {}",
                    verdict.paths.join(", ")
                ))
            }),
            verdict.paths.clone(),
        ),
        _ => (None, Vec::new()),
    };

    DocDeltaEnvelope {
        status: if !paths.is_empty() {
            ResultPayloadStatus::Recorded
        } else {
            status
        },
        summary,
        paths,
    }
}

fn effective_view_from_domain_envelope(envelope: ResultEnvelope) -> EffectiveResultEnvelopeView {
    EffectiveResultEnvelopeView {
        attempt_state: envelope.attempt_state,
        disposition: envelope.disposition,
        source: envelope.source,
        required_final_markers: envelope.closure_input.final_markers.required,
        observed_final_markers: envelope.closure_input.final_markers.observed,
        summary: envelope.summary,
        runtime: envelope.runtime,
    }
}

fn effective_view_from_trace_envelope(
    envelope: wave_trace::ResultEnvelopeRecord,
) -> EffectiveResultEnvelopeView {
    EffectiveResultEnvelopeView {
        attempt_state: domain_attempt_state_from_trace(envelope.attempt_state),
        disposition: domain_result_disposition_from_trace(envelope.disposition),
        source: domain_result_source_from_trace(envelope.source),
        required_final_markers: envelope.final_markers.required,
        observed_final_markers: envelope.final_markers.observed,
        summary: envelope.summary,
        runtime: envelope.runtime,
    }
}

fn domain_attempt_state_from_trace(state: wave_trace::AttemptState) -> AttemptState {
    match state {
        wave_trace::AttemptState::Planned => AttemptState::Planned,
        wave_trace::AttemptState::Running => AttemptState::Running,
        wave_trace::AttemptState::Succeeded => AttemptState::Succeeded,
        wave_trace::AttemptState::Failed => AttemptState::Failed,
        wave_trace::AttemptState::Aborted => AttemptState::Aborted,
        wave_trace::AttemptState::Refused => AttemptState::Refused,
    }
}

fn domain_result_disposition_from_trace(
    disposition: wave_trace::ResultDisposition,
) -> ResultDisposition {
    match disposition {
        wave_trace::ResultDisposition::Completed => ResultDisposition::Completed,
        wave_trace::ResultDisposition::Partial => ResultDisposition::Partial,
        wave_trace::ResultDisposition::Failed => ResultDisposition::Failed,
        wave_trace::ResultDisposition::Aborted => ResultDisposition::Aborted,
        wave_trace::ResultDisposition::Refused => ResultDisposition::Refused,
    }
}

fn domain_result_source_from_trace(
    source: wave_trace::ResultEnvelopeSource,
) -> ResultEnvelopeSource {
    match source {
        wave_trace::ResultEnvelopeSource::Structured => ResultEnvelopeSource::Structured,
        wave_trace::ResultEnvelopeSource::LegacyMarkerAdapter => {
            ResultEnvelopeSource::LegacyMarkerAdapter
        }
    }
}

fn legacy_proof_payload(
    final_markers: &FinalMarkerEnvelope,
    artifacts: Vec<ProofArtifact>,
) -> ProofEnvelope {
    ProofEnvelope {
        status: if final_markers
            .observed
            .iter()
            .any(|marker| marker == "[wave-proof]")
        {
            ResultPayloadStatus::EvidenceOnly
        } else {
            ResultPayloadStatus::Missing
        },
        summary: None,
        proof_bundle_ids: Vec::new(),
        fact_ids: Vec::new(),
        contradiction_ids: Vec::new(),
        artifacts,
    }
}

fn derive_closure_verdict_payload(
    agent_id: &str,
    text_artifacts: &[ClosureTextArtifact],
) -> ClosureVerdictPayload {
    if text_artifacts.is_empty() {
        return ClosureVerdictPayload::None;
    }

    match agent_id {
        "A0" => ClosureVerdictPayload::ContQa(parse_cont_qa_verdict(text_artifacts)),
        "A6" => ClosureVerdictPayload::Design(parse_design_verdict(text_artifacts)),
        "A7" => ClosureVerdictPayload::Security(parse_security_verdict(text_artifacts)),
        "A8" => ClosureVerdictPayload::Integration(parse_integration_verdict(text_artifacts)),
        "A9" => ClosureVerdictPayload::Documentation(parse_documentation_verdict(text_artifacts)),
        _ => ClosureVerdictPayload::None,
    }
}

fn observed_markers_in_text_artifacts(
    text_artifacts: &[ClosureTextArtifact],
    expected_markers: &[String],
) -> Vec<String> {
    let mut observed = Vec::new();
    for artifact in text_artifacts {
        for line in artifact.text.lines().map(str::trim) {
            for marker in expected_markers {
                if (line == marker || line.starts_with(&(marker.clone() + " ")))
                    && !observed.iter().any(|existing| existing == marker)
                {
                    observed.push(marker.clone());
                }
            }
        }
    }
    observed
}

fn merge_markers(mut markers: Vec<String>, additional: Vec<String>) -> Vec<String> {
    for marker in additional {
        if !markers.iter().any(|existing| existing == &marker) {
            markers.push(marker);
        }
    }
    markers
}

fn parse_cont_qa_verdict(text_artifacts: &[ClosureTextArtifact]) -> ContQaClosureVerdict {
    let verdict = text_artifacts
        .iter()
        .flat_map(|artifact| artifact.text.lines().map(str::trim))
        .filter_map(|line| line.strip_prefix("Verdict:"))
        .map(str::trim)
        .map(|value| value.to_ascii_uppercase())
        .last();
    let (gate_line, gate_fields) = find_marker_fields_in_texts(text_artifacts, "[wave-gate]")
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

fn parse_integration_verdict(text_artifacts: &[ClosureTextArtifact]) -> IntegrationClosureVerdict {
    let fields = find_marker_fields_in_texts(text_artifacts, "[wave-integration]")
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

fn parse_design_verdict(text_artifacts: &[ClosureTextArtifact]) -> DesignClosureVerdict {
    let fields = find_marker_fields_in_texts(text_artifacts, "[wave-design]")
        .map(|(_, fields)| fields)
        .unwrap_or_default();
    DesignClosureVerdict {
        state: fields.get("state").cloned(),
        findings: parse_marker_u32(&fields, "findings"),
        detail: fields.get("detail").cloned(),
    }
}

fn parse_security_verdict(text_artifacts: &[ClosureTextArtifact]) -> SecurityClosureVerdict {
    let fields = find_marker_fields_in_texts(text_artifacts, "[wave-security]")
        .map(|(_, fields)| fields)
        .unwrap_or_default();
    SecurityClosureVerdict {
        state: fields.get("state").cloned(),
        findings: parse_marker_u32(&fields, "findings"),
        approvals: parse_marker_u32(&fields, "approvals"),
        detail: fields.get("detail").cloned(),
    }
}

fn parse_documentation_verdict(
    text_artifacts: &[ClosureTextArtifact],
) -> DocumentationClosureVerdict {
    let fields = find_marker_fields_in_texts(text_artifacts, "[wave-doc-closure]")
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

fn find_marker_fields_in_texts(
    text_artifacts: &[ClosureTextArtifact],
    marker: &str,
) -> Option<(String, BTreeMap<String, String>)> {
    text_artifacts
        .iter()
        .filter_map(|artifact| find_marker_fields(&artifact.text, marker))
        .last()
}

fn find_marker_fields(text: &str, marker: &str) -> Option<(String, BTreeMap<String, String>)> {
    text.lines()
        .map(str::trim)
        .filter(|line| *line == marker || line.starts_with(&(marker.to_string() + " ")))
        .map(|line| (line.to_string(), parse_marker_fields(line, marker)))
        .last()
}

fn parse_marker_fields(line: &str, marker: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let mut rest = line.strip_prefix(marker).unwrap_or_default().trim();

    while !rest.is_empty() {
        let Some((key, tail)) = rest.split_once('=') else {
            break;
        };
        let key = key.trim();
        if key.is_empty() {
            break;
        }

        if key == "detail" {
            fields.insert(
                key.to_string(),
                tail.trim().trim_end_matches(',').to_string(),
            );
            break;
        }

        let value_end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let value = tail[..value_end].trim().trim_end_matches(',');
        fields.insert(key.to_string(), value.to_string());
        rest = tail[value_end..].trim_start();
    }

    fields
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

fn normalize_proof_artifacts(
    artifacts: &[ProofArtifact],
    repo_root: Option<&Path>,
) -> Result<Vec<ProofArtifact>> {
    let mut normalized = artifacts
        .iter()
        .map(|artifact| {
            let path = PathBuf::from(&artifact.path);
            let resolved = resolve_optional_path(repo_root, &path);
            let digest = match artifact.digest.clone() {
                Some(digest) => Some(digest),
                None if resolved.exists() => Some(hash_file(&resolved)?),
                None => None,
            };
            Ok(ProofArtifact {
                path: normalize_path_string(&path, repo_root),
                kind: artifact.kind,
                digest,
                note: artifact.note.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    normalized.sort_by(|left, right| artifact_sort_key(left).cmp(&artifact_sort_key(right)));
    normalized.dedup_by(|left, right| artifact_sort_key(left) == artifact_sort_key(right));
    Ok(normalized)
}

fn normalize_paths(paths: &[String], repo_root: Option<&Path>) -> Vec<String> {
    let mut normalized = paths
        .iter()
        .map(|path| normalize_path_string(&PathBuf::from(path), repo_root))
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn normalize_marker_evidence(
    marker_evidence: &[MarkerEvidence],
    repo_root: Option<&Path>,
) -> Vec<MarkerEvidence> {
    let mut normalized = marker_evidence
        .iter()
        .map(|evidence| MarkerEvidence {
            marker: evidence.marker.clone(),
            line: evidence.line.trim().to_string(),
            source: evidence
                .source
                .as_ref()
                .map(|source| normalize_path_string(&PathBuf::from(source), repo_root)),
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|left, right| {
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
    normalized.dedup_by(|left, right| {
        left.marker == right.marker && left.line == right.line && left.source == right.source
    });
    normalized
}

fn read_optional_text(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))
        .map(Some)
}

fn hash_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(hash_bytes_hex(&bytes))
}

fn hash_bytes_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn resolve_path(repo_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}

fn resolve_optional_path(repo_root: Option<&Path>, path: &Path) -> PathBuf {
    match repo_root {
        Some(root) => resolve_path(root, path),
        None => path.to_path_buf(),
    }
}

fn normalize_path_string(path: &Path, repo_root: Option<&Path>) -> String {
    let display_path = match repo_root {
        Some(root) if path.is_absolute() => path.strip_prefix(root).unwrap_or(path).to_path_buf(),
        _ => path.to_path_buf(),
    };
    display_path.to_string_lossy().replace('\\', "/")
}

fn compare_envelopes(left: &ResultEnvelope, right: &ResultEnvelope) -> Ordering {
    (
        left.created_at_ms,
        left.attempt_id.as_str(),
        left.result_envelope_id.as_str(),
    )
        .cmp(&(
            right.created_at_ms,
            right.attempt_id.as_str(),
            right.result_envelope_id.as_str(),
        ))
}

fn artifact_sort_key(artifact: &ProofArtifact) -> (String, &'static str, String, String) {
    (
        artifact.path.clone(),
        artifact_kind_key(artifact.kind),
        artifact.note.clone().unwrap_or_default(),
        artifact.digest.clone().unwrap_or_default(),
    )
}

fn artifact_kind_key(kind: wave_domain::ArtifactKind) -> &'static str {
    match kind {
        wave_domain::ArtifactKind::Patch => "patch",
        wave_domain::ArtifactKind::TestLog => "test_log",
        wave_domain::ArtifactKind::DocDelta => "doc_delta",
        wave_domain::ArtifactKind::Trace => "trace",
        wave_domain::ArtifactKind::Review => "review",
        wave_domain::ArtifactKind::ResultEnvelope => "result_envelope",
        wave_domain::ArtifactKind::Other => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::sync::atomic::Ordering as AtomicOrdering;
    use wave_domain::ArtifactKind;
    use wave_domain::ProofBundleId;

    static TEMP_ROOT_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn writes_and_loads_normalized_result_envelopes() {
        let root = temp_root("write");
        let store = ResultEnvelopeStore::under_repo(&root);

        let artifact_path = root.join("artifacts/proof.log");
        fs::create_dir_all(artifact_path.parent().expect("artifact parent")).expect("mkdir");
        fs::write(&artifact_path, "cargo test -p wave-results\n").expect("write artifact");

        let attempt_id = AttemptId::new("attempt-a1-1");
        let task_id = task_id_for_agent(12, "A1");
        let final_markers = FinalMarkerEnvelope::from_contract(
            vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            vec![
                "[wave-component]".to_string(),
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
            ],
        );
        let closure = ClosureState {
            disposition: ClosureDisposition::Ready,
            required_final_markers: final_markers.required.clone(),
            observed_final_markers: final_markers.observed.clone(),
            blocking_reasons: Vec::new(),
            satisfied_fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            verdict: ClosureVerdictPayload::None,
        };

        let path = store
            .write_envelope(&ResultEnvelope {
                result_envelope_id: ResultEnvelopeId::new("result-1"),
                wave_id: 12,
                task_id: task_id.clone(),
                attempt_id: attempt_id.clone(),
                agent_id: "A1".to_string(),
                task_role: inferred_task_role_for_agent("A1", &[]),
                closure_role: inferred_closure_role_for_agent("A1"),
                source: ResultEnvelopeSource::Structured,
                attempt_state: AttemptState::Succeeded,
                disposition: ResultDisposition::Completed,
                summary: Some("structured envelope landed".to_string()),
                output_text: Some("proof summary".to_string()),
                proof: ProofEnvelope {
                    status: ResultPayloadStatus::Missing,
                    summary: Some("proof artifact recorded".to_string()),
                    proof_bundle_ids: vec![ProofBundleId::new("proof-1")],
                    fact_ids: Vec::new(),
                    contradiction_ids: Vec::new(),
                    artifacts: vec![
                        ProofArtifact {
                            path: artifact_path.to_string_lossy().into_owned(),
                            kind: ArtifactKind::TestLog,
                            digest: None,
                            note: Some("cargo test -p wave-results".to_string()),
                        },
                        ProofArtifact {
                            path: artifact_path.to_string_lossy().into_owned(),
                            kind: ArtifactKind::TestLog,
                            digest: None,
                            note: Some("cargo test -p wave-results".to_string()),
                        },
                    ],
                },
                doc_delta: DocDeltaEnvelope {
                    status: ResultPayloadStatus::Missing,
                    summary: Some("doc delta summarized".to_string()),
                    paths: vec![
                        root.join("docs/owned-note.md")
                            .to_string_lossy()
                            .into_owned(),
                    ],
                },
                closure_input: ClosureInputEnvelope {
                    status: ResultPayloadStatus::Missing,
                    final_markers,
                    marker_evidence: vec![MarkerEvidence {
                        marker: "[wave-proof]".to_string(),
                        line: "[wave-proof]".to_string(),
                        source: Some(
                            root.join(".wave/state/build/specs/wave-12/agents/A1/last-message.txt")
                                .to_string_lossy()
                                .into_owned(),
                        ),
                    }],
                },
                closure,
                runtime: None,
                created_at_ms: 42,
            })
            .expect("write envelope");

        assert_eq!(
            path,
            root.join(".wave/state/results/wave-12/attempt-a1-1/agent_result_envelope.json")
        );

        let loaded = store
            .load_attempt_envelope(12, &attempt_id)
            .expect("load attempt")
            .expect("stored envelope");
        assert_eq!(loaded.disposition, ResultDisposition::Completed);
        assert!(loaded.closure_input.final_markers.missing.is_empty());
        assert_eq!(loaded.proof.status, ResultPayloadStatus::Recorded);
        assert_eq!(loaded.proof.artifacts.len(), 1);
        assert_eq!(loaded.proof.artifacts[0].path, "artifacts/proof.log");
        assert_eq!(
            loaded.proof.artifacts[0].digest.as_deref(),
            Some(hash_bytes_hex(b"cargo test -p wave-results\n").as_str())
        );
        assert_eq!(
            loaded.doc_delta.paths,
            vec!["docs/owned-note.md".to_string()]
        );
        assert_eq!(loaded.doc_delta.status, ResultPayloadStatus::Recorded);
        assert_eq!(
            loaded.closure_input.marker_evidence[0].source.as_deref(),
            Some(".wave/state/build/specs/wave-12/agents/A1/last-message.txt")
        );
        assert_eq!(loaded.closure_input.status, ResultPayloadStatus::Recorded);
        assert_eq!(
            store
                .latest_terminal_task_envelope(12, &task_id)
                .expect("latest terminal")
                .map(|envelope| envelope.result_envelope_id),
            Some(ResultEnvelopeId::new("result-1"))
        );
        assert_eq!(
            store
                .latest_completed_or_failed_task_envelope(12, &task_id)
                .expect("latest completed or failed")
                .map(|envelope| envelope.result_envelope_id),
            Some(ResultEnvelopeId::new("result-1"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn normalize_result_envelope_normalizes_runtime_artifact_paths() {
        let root = temp_root("runtime-artifacts");
        let runtime_detail_path =
            root.join(".wave/state/build/specs/wave-15/agents/A1/runtime-detail.json");
        let overlay_path =
            root.join(".wave/state/build/specs/wave-15/agents/A1/runtime-skill-overlay.md");
        fs::create_dir_all(runtime_detail_path.parent().expect("runtime detail parent"))
            .expect("create runtime artifact dir");
        fs::write(&runtime_detail_path, "{}\n").expect("write runtime detail");
        fs::write(&overlay_path, "overlay\n").expect("write runtime overlay");

        let mut envelope = structured_envelope(
            15,
            "A1",
            "attempt-runtime",
            "result-runtime",
            AttemptState::Succeeded,
            vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            7,
        );
        envelope.runtime = Some(wave_domain::RuntimeExecutionRecord {
            policy: wave_domain::RuntimeSelectionPolicy {
                requested_runtime: Some(wave_domain::RuntimeId::Claude),
                allowed_runtimes: vec![wave_domain::RuntimeId::Claude],
                fallback_runtimes: Vec::new(),
                selection_source: Some("executor.id".to_string()),
            },
            selected_runtime: wave_domain::RuntimeId::Claude,
            selection_reason: "selected claude from executor.id".to_string(),
            fallback: None,
            execution_identity: wave_domain::RuntimeExecutionIdentity {
                runtime: wave_domain::RuntimeId::Claude,
                adapter: "wave-runtime/claude".to_string(),
                binary: "/tmp/fake-claude".to_string(),
                provider: "anthropic-claude-code".to_string(),
                artifact_paths: std::collections::BTreeMap::from([
                    (
                        "runtime_detail".to_string(),
                        runtime_detail_path.to_string_lossy().into_owned(),
                    ),
                    (
                        "skill_overlay".to_string(),
                        overlay_path.to_string_lossy().into_owned(),
                    ),
                ]),
            },
            skill_projection: wave_domain::RuntimeSkillProjection {
                declared_skills: vec!["wave-core".to_string()],
                projected_skills: vec!["wave-core".to_string(), "runtime-claude".to_string()],
                dropped_skills: Vec::new(),
                auto_attached_skills: vec!["runtime-claude".to_string()],
            },
        });

        let normalized = normalize_result_envelope(&envelope, Some(&root)).expect("normalize");
        let runtime = normalized.runtime.expect("runtime");
        assert_eq!(
            runtime
                .execution_identity
                .artifact_paths
                .get("runtime_detail"),
            Some(&".wave/state/build/specs/wave-15/agents/A1/runtime-detail.json".to_string())
        );
        assert_eq!(
            runtime
                .execution_identity
                .artifact_paths
                .get("skill_overlay"),
            Some(&".wave/state/build/specs/wave-15/agents/A1/runtime-skill-overlay.md".to_string())
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn validate_result_envelope_accepts_design_review_verdict_for_design_role() {
        let mut envelope = structured_envelope(
            15,
            "A6",
            "attempt-design",
            "result-design",
            AttemptState::Succeeded,
            vec!["[wave-design]".to_string()],
            vec!["[wave-design]".to_string()],
            15,
        );
        envelope.closure.verdict = ClosureVerdictPayload::Design(DesignClosureVerdict {
            state: Some("aligned".to_string()),
            findings: Some(0),
            detail: Some("runtime decision cues are present".to_string()),
        });

        let normalized = normalize_result_envelope(&envelope, None).expect("normalize");

        validate_result_envelope(&normalized).expect("design verdict should validate");
    }

    #[test]
    fn refuses_to_mutate_existing_attempt_envelope_payload() {
        let root = temp_root("immutable-write");
        let store = ResultEnvelopeStore::under_repo(&root);
        let envelope = structured_envelope(
            12,
            "A1",
            "attempt-a1-immutable",
            "result-immutable",
            AttemptState::Succeeded,
            vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            5,
        );

        store.write_envelope(&envelope).expect("write envelope");

        let mut changed = envelope.clone();
        changed.summary = Some("rewritten payload".to_string());

        let error = store
            .write_envelope(&changed)
            .expect_err("attempt envelope should be immutable");
        assert!(error.to_string().contains("immutable"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn selects_latest_completed_or_failed_attempt_without_terminal_marker_heuristics() {
        let root = temp_root("selection");
        let store = ResultEnvelopeStore::under_repo(&root);
        let task_id = task_id_for_agent(12, "A1");

        store
            .write_envelope(&structured_envelope(
                12,
                "A1",
                "attempt-a1-1",
                "result-1",
                AttemptState::Succeeded,
                vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                1,
            ))
            .expect("write completed envelope");
        store
            .write_envelope(&structured_envelope(
                12,
                "A1",
                "attempt-a1-2",
                "result-2",
                AttemptState::Refused,
                vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                Vec::new(),
                2,
            ))
            .expect("write refused envelope");
        store
            .write_envelope(&structured_envelope(
                12,
                "A1",
                "attempt-a1-3",
                "result-3",
                AttemptState::Failed,
                vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                vec!["[wave-proof]".to_string()],
                3,
            ))
            .expect("write failed envelope");

        assert_eq!(
            store
                .latest_terminal_task_envelope(12, &task_id)
                .expect("latest terminal")
                .map(|envelope| envelope.result_envelope_id),
            Some(ResultEnvelopeId::new("result-3"))
        );
        assert_eq!(
            store
                .latest_completed_or_failed_task_envelope(12, &task_id)
                .expect("latest completed or failed")
                .map(|envelope| envelope.result_envelope_id),
            Some(ResultEnvelopeId::new("result-3"))
        );

        let later_refused = structured_envelope(
            12,
            "A1",
            "attempt-a1-4",
            "result-4",
            AttemptState::Refused,
            vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            Vec::new(),
            4,
        );
        store
            .write_envelope(&later_refused)
            .expect("write later refused envelope");

        assert_eq!(
            store
                .latest_terminal_task_envelope(12, &task_id)
                .expect("latest terminal")
                .map(|envelope| envelope.result_envelope_id),
            Some(ResultEnvelopeId::new("result-4"))
        );
        assert_eq!(
            store
                .latest_completed_or_failed_task_envelope(12, &task_id)
                .expect("latest completed or failed")
                .map(|envelope| envelope.result_envelope_id),
            Some(ResultEnvelopeId::new("result-3"))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn structured_doc_delta_stays_missing_without_doc_delta_marker() {
        let root = temp_root("doc-delta-missing");
        let mut run = structured_run(&root, 12, "A1", WaveRunStatus::Succeeded);
        run.agents[0].observed_markers =
            vec!["[wave-proof]".to_string(), "[wave-component]".to_string()];
        let agent_record = run.agents[0].clone();
        let agent = WaveAgent {
            id: "A1".to_string(),
            title: "Result Envelope Core".to_string(),
            role_prompts: Vec::new(),
            executor: BTreeMap::new(),
            context7: None,
            skills: Vec::new(),
            components: Vec::new(),
            capabilities: Vec::new(),
            exit_contract: None,
            deliverables: vec!["docs/implementation/rust-wave-0.3-notes.md".to_string()],
            file_ownership: vec!["docs/implementation/rust-wave-0.3-notes.md".to_string()],
            final_markers: vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            depends_on_agents: Vec::new(),
            reads_artifacts_from: Vec::new(),
            writes_artifacts: Vec::new(),
            barrier_class: wave_spec::BarrierClass::Independent,
            parallel_safety: wave_spec::ParallelSafetyClass::Derived,
            exclusive_resources: Vec::new(),
            parallel_with: Vec::new(),
            prompt: String::new(),
        };

        let envelope = build_structured_result_envelope(&root, &run, &agent, &agent_record, 12)
            .expect("build structured envelope");

        assert_eq!(envelope.doc_delta.status, ResultPayloadStatus::Missing);
        assert!(envelope.doc_delta.summary.is_none());
        assert!(envelope.doc_delta.paths.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn structured_builder_prefers_declared_agent_contract_over_runtime_marker_copy() {
        let root = temp_root("declared-contract");
        let mut run = structured_run(&root, 12, "A2", WaveRunStatus::Succeeded);
        run.agents[0].expected_markers = vec!["[wave-proof]".to_string()];
        run.agents[0].observed_markers = vec!["[wave-proof]".to_string()];
        let agent_record = run.agents[0].clone();
        let agent = WaveAgent {
            id: "A2".to_string(),
            title: "Security proof slice".to_string(),
            role_prompts: Vec::new(),
            executor: BTreeMap::new(),
            context7: None,
            skills: vec!["role-security".to_string()],
            components: Vec::new(),
            capabilities: Vec::new(),
            exit_contract: None,
            deliverables: Vec::new(),
            file_ownership: Vec::new(),
            final_markers: vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            depends_on_agents: Vec::new(),
            reads_artifacts_from: Vec::new(),
            writes_artifacts: Vec::new(),
            barrier_class: wave_spec::BarrierClass::Independent,
            parallel_safety: wave_spec::ParallelSafetyClass::Derived,
            exclusive_resources: Vec::new(),
            parallel_with: Vec::new(),
            prompt: String::new(),
        };

        let envelope = build_structured_result_envelope(&root, &run, &agent, &agent_record, 12)
            .expect("build structured envelope");

        assert_eq!(envelope.task_role, wave_domain::TaskRole::Security);
        assert_eq!(
            envelope.closure_input.final_markers.required,
            vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ]
        );
        assert_eq!(envelope.disposition, ResultDisposition::Partial);
        assert_eq!(
            envelope.closure_input.final_markers.missing,
            vec![
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string()
            ]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn adapts_legacy_marker_first_run_artifacts_into_typed_envelopes() {
        let root = temp_root("legacy");
        let last_message_path =
            root.join(".wave/state/build/specs/wave-12-legacy/agents/A1/last-message.txt");
        fs::create_dir_all(last_message_path.parent().expect("message parent")).expect("mkdir");
        fs::write(
            &last_message_path,
            "attempt failed\n[wave-proof]\n[wave-doc-delta]\n",
        )
        .expect("write message");

        let run = WaveRunRecord {
            run_id: "wave-12-legacy".to_string(),
            wave_id: 12,
            slug: "result-envelope-proof-lifecycle".to_string(),
            title: "Land result envelopes and proof lifecycle".to_string(),
            status: WaveRunStatus::Failed,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-12-legacy"),
            trace_path: root.join(".wave/traces/runs/wave-12-legacy.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(3),
            agents: vec![AgentRunRecord {
                id: "A1".to_string(),
                title: "Result Envelope Core".to_string(),
                status: WaveRunStatus::Failed,
                prompt_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A1/prompt.md"),
                last_message_path: PathBuf::from(
                    ".wave/state/build/specs/wave-12-legacy/agents/A1/last-message.txt",
                ),
                events_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A1/events.jsonl"),
                stderr_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A1/stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: None,
                expected_markers: vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                observed_markers: vec!["[wave-proof]".to_string(), "[wave-doc-delta]".to_string()],
                exit_code: Some(1),
                error: Some("missing component proof".to_string()),
                runtime: None,
            }],
            error: Some("agent failure".to_string()),
        };

        let envelopes = adapt_legacy_run_record(&root, &run).expect("adapt legacy run");
        assert_eq!(envelopes.len(), 1);

        let envelope = &envelopes[0];
        assert_eq!(envelope.source, ResultEnvelopeSource::LegacyMarkerAdapter);
        assert_eq!(envelope.attempt_state, AttemptState::Failed);
        assert_eq!(envelope.disposition, ResultDisposition::Failed);
        assert_eq!(envelope.attempt_id.as_str(), "legacy-wave-12-legacy-a1");
        assert_eq!(
            envelope.closure_input.final_markers.missing,
            vec!["[wave-component]".to_string()]
        );
        assert_eq!(envelope.proof.status, ResultPayloadStatus::EvidenceOnly);
        assert_eq!(envelope.doc_delta.status, ResultPayloadStatus::EvidenceOnly);
        assert_eq!(
            envelope.closure_input.status,
            ResultPayloadStatus::EvidenceOnly
        );
        assert_eq!(envelope.closure.disposition, ClosureDisposition::Blocked);
        assert!(
            envelope
                .closure
                .blocking_reasons
                .iter()
                .any(|reason| reason.contains("[wave-component]"))
        );
        assert_eq!(
            envelope.output_text.as_deref(),
            Some("attempt failed\n[wave-proof]\n[wave-doc-delta]\n")
        );
        assert_eq!(envelope.closure_input.marker_evidence.len(), 2);
        assert!(
            envelope
                .closure_input
                .marker_evidence
                .iter()
                .all(|evidence| {
                    evidence.source.as_deref()
                        == Some(".wave/state/build/specs/wave-12-legacy/agents/A1/last-message.txt")
                })
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_adapter_preserves_runtime_choice_and_fallback_metadata() {
        let root = temp_root("legacy-runtime");
        let agent_dir = root.join(".wave/state/build/specs/wave-15-legacy/agents/A1");
        fs::create_dir_all(&agent_dir).expect("create agent dir");
        fs::write(
            agent_dir.join("last-message.txt"),
            "[wave-proof]\n[wave-doc-delta]\n[wave-component]\n",
        )
        .expect("write message");
        fs::write(agent_dir.join("runtime-detail.json"), "{}\n").expect("write runtime detail");

        let run = WaveRunRecord {
            run_id: "wave-15-legacy".to_string(),
            wave_id: 15,
            slug: "runtime-policy-and-multi-runtime-adapters".to_string(),
            title: "Land runtime policy and multi-runtime adapters".to_string(),
            status: WaveRunStatus::Succeeded,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-15-legacy"),
            trace_path: root.join(".wave/traces/runs/wave-15-legacy.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(3),
            agents: vec![AgentRunRecord {
                id: "A1".to_string(),
                title: "Claude Adapter And Skill Projection".to_string(),
                status: WaveRunStatus::Succeeded,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: PathBuf::from(
                    ".wave/state/build/specs/wave-15-legacy/agents/A1/last-message.txt",
                ),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: Some(PathBuf::from(
                    ".wave/state/build/specs/wave-15-legacy/agents/A1/runtime-detail.json",
                )),
                expected_markers: vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                observed_markers: vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                exit_code: Some(0),
                error: None,
                runtime: Some(sample_claude_runtime_record(
                    &root,
                    ".wave/runtime/overlay.md",
                    true,
                )),
            }],
            error: None,
        };

        let envelope = adapt_legacy_run_record(&root, &run)
            .expect("adapt legacy run")
            .into_iter()
            .next()
            .expect("legacy envelope");
        let runtime = envelope.runtime.expect("runtime");

        assert_eq!(runtime.selected_runtime, wave_domain::RuntimeId::Claude);
        assert_eq!(
            runtime
                .fallback
                .as_ref()
                .expect("fallback")
                .requested_runtime,
            wave_domain::RuntimeId::Codex
        );
        assert_eq!(
            runtime
                .execution_identity
                .artifact_paths
                .get("runtime_detail"),
            Some(
                &".wave/state/build/specs/wave-15-legacy/agents/A1/runtime-detail.json".to_string()
            )
        );
        assert_eq!(
            runtime.skill_projection.projected_skills,
            vec!["wave-core".to_string(), "runtime-claude".to_string()]
        );
        assert_eq!(
            runtime.skill_projection.auto_attached_skills,
            vec!["runtime-claude".to_string()]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_adapter_loads_runtime_from_runtime_detail_snapshot_when_agent_runtime_is_missing() {
        let root = temp_root("legacy-runtime-detail");
        let agent_dir = root.join(".wave/state/build/specs/wave-15-legacy/agents/A1");
        fs::create_dir_all(&agent_dir).expect("create agent dir");
        fs::write(
            agent_dir.join("last-message.txt"),
            "[wave-proof]\n[wave-doc-delta]\n[wave-component]\n",
        )
        .expect("write message");

        let runtime = sample_claude_runtime_record(&root, ".wave/runtime/overlay.md", false);
        fs::write(
            agent_dir.join("runtime-detail.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "wave_id": 15,
                "run_id": "wave-15-legacy",
                "agent_id": "A1",
                "agent_title": "Claude Adapter And Skill Projection",
                "runtime": runtime,
            }))
            .expect("serialize runtime detail"),
        )
        .expect("write runtime detail");

        let run = WaveRunRecord {
            run_id: "wave-15-legacy".to_string(),
            wave_id: 15,
            slug: "runtime-policy-and-multi-runtime-adapters".to_string(),
            title: "Land runtime policy and multi-runtime adapters".to_string(),
            status: WaveRunStatus::Succeeded,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-15-legacy"),
            trace_path: root.join(".wave/traces/runs/wave-15-legacy.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(3),
            agents: vec![AgentRunRecord {
                id: "A1".to_string(),
                title: "Claude Adapter And Skill Projection".to_string(),
                status: WaveRunStatus::Succeeded,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: PathBuf::from(
                    ".wave/state/build/specs/wave-15-legacy/agents/A1/last-message.txt",
                ),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: Some(PathBuf::from(
                    ".wave/state/build/specs/wave-15-legacy/agents/A1/runtime-detail.json",
                )),
                expected_markers: vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                observed_markers: vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                exit_code: Some(0),
                error: None,
                runtime: None,
            }],
            error: None,
        };

        let envelope = adapt_legacy_run_record(&root, &run)
            .expect("adapt legacy run")
            .into_iter()
            .next()
            .expect("legacy envelope");
        let runtime = envelope.runtime.expect("runtime");

        assert_eq!(runtime.selected_runtime, wave_domain::RuntimeId::Claude);
        assert_eq!(runtime.execution_identity.adapter, "wave-runtime/claude");
        assert_eq!(
            runtime
                .execution_identity
                .artifact_paths
                .get("runtime_detail"),
            Some(
                &".wave/state/build/specs/wave-15-legacy/agents/A1/runtime-detail.json".to_string()
            )
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn adapts_legacy_closure_agents_into_typed_closure_verdicts() {
        let root = temp_root("legacy-closure");
        let last_message_path =
            root.join(".wave/state/build/specs/wave-12-legacy/agents/A8/last-message.txt");
        fs::create_dir_all(last_message_path.parent().expect("message parent")).expect("mkdir");
        fs::write(
            &last_message_path,
            "[wave-integration] state=ready-for-doc-closure claims=2 conflicts=0 blockers=0 detail=ok\n",
        )
        .expect("write message");

        let run = WaveRunRecord {
            run_id: "wave-12-legacy".to_string(),
            wave_id: 12,
            slug: "result-envelope-proof-lifecycle".to_string(),
            title: "Land result envelopes and proof lifecycle".to_string(),
            status: WaveRunStatus::Succeeded,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-12-legacy"),
            trace_path: root.join(".wave/traces/runs/wave-12-legacy.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(3),
            agents: vec![AgentRunRecord {
                id: "A8".to_string(),
                title: "Integration Steward".to_string(),
                status: WaveRunStatus::Succeeded,
                prompt_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A8/prompt.md"),
                last_message_path: PathBuf::from(
                    ".wave/state/build/specs/wave-12-legacy/agents/A8/last-message.txt",
                ),
                events_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A8/events.jsonl"),
                stderr_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A8/stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: None,
                expected_markers: vec!["[wave-integration]".to_string()],
                observed_markers: vec!["[wave-integration]".to_string()],
                exit_code: Some(0),
                error: None,
                runtime: None,
            }],
            error: None,
        };

        let envelope = adapt_legacy_run_record(&root, &run)
            .expect("adapt legacy run")
            .into_iter()
            .next()
            .expect("legacy envelope");

        match envelope.closure.verdict {
            ClosureVerdictPayload::Integration(verdict) => {
                assert_eq!(verdict.state.as_deref(), Some("ready-for-doc-closure"));
                assert_eq!(verdict.claims, Some(2));
                assert_eq!(verdict.conflicts, Some(0));
                assert_eq!(verdict.blockers, Some(0));
            }
            other => panic!("expected integration verdict, got {other:?}"),
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_closure_agents_require_machine_readable_payloads() {
        let root = temp_root("legacy-closure-gap");
        let last_message_path =
            root.join(".wave/state/build/specs/wave-12-legacy/agents/A8/last-message.txt");
        fs::create_dir_all(last_message_path.parent().expect("message parent")).expect("mkdir");
        fs::write(
            &last_message_path,
            "[wave-integration] detail=compatibility output without structured state\n",
        )
        .expect("write message");

        let run = WaveRunRecord {
            run_id: "wave-12-legacy".to_string(),
            wave_id: 12,
            slug: "result-envelope-proof-lifecycle".to_string(),
            title: "Land result envelopes and proof lifecycle".to_string(),
            status: WaveRunStatus::Succeeded,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-12-legacy"),
            trace_path: root.join(".wave/traces/runs/wave-12-legacy.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(3),
            agents: vec![AgentRunRecord {
                id: "A8".to_string(),
                title: "Integration Steward".to_string(),
                status: WaveRunStatus::Succeeded,
                prompt_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A8/prompt.md"),
                last_message_path: PathBuf::from(
                    ".wave/state/build/specs/wave-12-legacy/agents/A8/last-message.txt",
                ),
                events_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A8/events.jsonl"),
                stderr_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A8/stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: None,
                expected_markers: vec!["[wave-integration]".to_string()],
                observed_markers: vec!["[wave-integration]".to_string()],
                exit_code: Some(0),
                error: None,
                runtime: None,
            }],
            error: None,
        };

        let envelope = adapt_legacy_run_record(&root, &run)
            .expect("adapt legacy run")
            .into_iter()
            .next()
            .expect("legacy envelope");

        assert_eq!(envelope.closure.disposition, ClosureDisposition::Blocked);
        assert!(
            envelope
                .closure
                .blocking_reasons
                .iter()
                .any(|reason| reason == "integration report is missing state=<...>")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn legacy_adapter_uses_integration_summary_when_last_message_is_incomplete() {
        let root = temp_root("legacy-integration-fallback");
        let last_message_path =
            root.join(".wave/state/build/specs/wave-12-legacy/agents/A8/last-message.txt");
        fs::create_dir_all(last_message_path.parent().expect("message parent")).expect("mkdir");
        fs::write(
            &last_message_path,
            "integration notes without a terminal marker\n",
        )
        .expect("write message");
        let integration_summary_path = root.join(".wave/integration/wave-12.md");
        fs::create_dir_all(
            integration_summary_path
                .parent()
                .expect("integration parent"),
        )
        .expect("mkdir");
        fs::write(
            &integration_summary_path,
            "# Wave 12 Integration Summary\n\n[wave-integration] state=ready-for-doc-closure claims=4 conflicts=0 blockers=0 detail=legacy adapter reused owned integration summary\n",
        )
        .expect("write integration summary");

        let run = WaveRunRecord {
            run_id: "wave-12-legacy".to_string(),
            wave_id: 12,
            slug: "result-envelope-proof-lifecycle".to_string(),
            title: "Land result envelopes and proof lifecycle".to_string(),
            status: WaveRunStatus::Succeeded,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-12-legacy"),
            trace_path: root.join(".wave/traces/runs/wave-12-legacy.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(3),
            agents: vec![AgentRunRecord {
                id: "A8".to_string(),
                title: "Integration Steward".to_string(),
                status: WaveRunStatus::Succeeded,
                prompt_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A8/prompt.md"),
                last_message_path: PathBuf::from(
                    ".wave/state/build/specs/wave-12-legacy/agents/A8/last-message.txt",
                ),
                events_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A8/events.jsonl"),
                stderr_path: root
                    .join(".wave/state/build/specs/wave-12-legacy/agents/A8/stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: None,
                expected_markers: vec!["[wave-integration]".to_string()],
                observed_markers: vec!["[wave-integration]".to_string()],
                exit_code: Some(0),
                error: None,
                runtime: None,
            }],
            error: None,
        };

        let envelope = adapt_legacy_run_record(&root, &run)
            .expect("adapt legacy run")
            .into_iter()
            .next()
            .expect("legacy envelope");

        match envelope.closure.verdict {
            ClosureVerdictPayload::Integration(verdict) => {
                assert_eq!(verdict.state.as_deref(), Some("ready-for-doc-closure"));
                assert_eq!(verdict.claims, Some(4));
            }
            other => panic!("expected integration verdict, got {other:?}"),
        }
        assert_eq!(
            envelope.closure_input.final_markers.observed,
            vec!["[wave-integration]".to_string()]
        );
        assert!(
            envelope
                .closure_input
                .marker_evidence
                .iter()
                .any(|evidence| {
                    evidence.marker == "[wave-integration]"
                        && evidence.source.as_deref() == Some(".wave/integration/wave-12.md")
                })
        );
        assert!(
            envelope
                .proof
                .artifacts
                .iter()
                .any(|artifact| artifact.path == ".wave/integration/wave-12.md")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_marker_detail_payload_with_spaces() {
        let verdict = parse_integration_verdict(
            &[ClosureTextArtifact {
                path: PathBuf::from("integration.md"),
                text: "[wave-integration] state=ready-for-doc-closure claims=2 conflicts=0 blockers=0 detail=compatibility path remains evidence only".to_string(),
            }],
        );

        assert_eq!(
            verdict.detail.as_deref(),
            Some("compatibility path remains evidence only")
        );
    }

    #[test]
    fn structured_envelope_uses_integration_summary_when_last_message_is_incomplete() {
        let root = temp_root("structured-integration-fallback");
        let agent = declared_agent("A8", "Integration Steward", vec!["[wave-integration]"]);
        let run = structured_run(&root, 12, "A8", WaveRunStatus::Succeeded);
        let agent_record = &run.agents[0];
        let last_message_path = resolve_path(&root, &agent_record.last_message_path);
        fs::create_dir_all(last_message_path.parent().expect("message parent")).expect("mkdir");
        fs::write(
            &last_message_path,
            "Updated integration files and verified the worktree.\n",
        )
        .expect("write last message");
        let integration_summary_path = root.join(".wave/integration/wave-12.md");
        fs::create_dir_all(
            integration_summary_path
                .parent()
                .expect("integration parent"),
        )
        .expect("mkdir");
        fs::write(
            &integration_summary_path,
            "# Wave 12 Integration Summary\n\n[wave-integration] state=ready-for-doc-closure claims=3 conflicts=0 blockers=0 detail=envelope-first proof boundary is reconciled\n",
        )
        .expect("write integration summary");
        fs::write(
            root.join(".wave/integration/wave-12.json"),
            "{\"state\":\"ready-for-doc-closure\"}\n",
        )
        .expect("write integration json");

        let envelope = build_structured_result_envelope(&root, &run, &agent, agent_record, 12)
            .expect("build structured envelope");

        match envelope.closure.verdict {
            ClosureVerdictPayload::Integration(verdict) => {
                assert_eq!(verdict.state.as_deref(), Some("ready-for-doc-closure"));
                assert_eq!(verdict.claims, Some(3));
                assert_eq!(verdict.conflicts, Some(0));
                assert_eq!(verdict.blockers, Some(0));
            }
            other => panic!("expected integration verdict, got {other:?}"),
        }
        assert_eq!(envelope.closure.disposition, ClosureDisposition::Ready);
        assert!(envelope.closure.blocking_reasons.is_empty());
        assert!(
            envelope
                .closure_input
                .marker_evidence
                .iter()
                .any(|evidence| {
                    evidence.marker == "[wave-integration]"
                        && evidence.source.as_deref() == Some(".wave/integration/wave-12.md")
                })
        );
        assert!(
            envelope
                .proof
                .artifacts
                .iter()
                .any(|artifact| artifact.path == ".wave/integration/wave-12.md")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn structured_envelope_uses_worktree_rooted_design_review_when_last_message_is_incomplete() {
        let root = temp_root("structured-design-worktree");
        let agent = declared_agent("A6", "Design Review Steward", vec!["[wave-design]"]);
        let mut run = structured_run(&root, 15, "A6", WaveRunStatus::Succeeded);
        run.worktree = Some(wave_domain::WaveWorktreeRecord {
            worktree_id: wave_domain::WaveWorktreeId::new("worktree-wave-15-test".to_string()),
            wave_id: 15,
            path: ".wave/state/worktrees/wave-15-test-worktree".to_string(),
            base_ref: "main".to_string(),
            snapshot_ref: "snapshot-wave-15-test".to_string(),
            branch_ref: None,
            shared_scope: wave_domain::WaveWorktreeScope::Wave,
            state: wave_domain::WaveWorktreeState::Allocated,
            allocated_at_ms: 1,
            released_at_ms: None,
            detail: Some("worktree-rooted design review".to_string()),
        });
        let agent_record = &run.agents[0];
        let last_message_path = resolve_path(&root, &agent_record.last_message_path);
        fs::create_dir_all(last_message_path.parent().expect("message parent")).expect("mkdir");
        fs::write(
            &last_message_path,
            "Recorded the review in the owned design report.\n",
        )
        .expect("write last message");
        let design_review_path =
            root.join(".wave/state/worktrees/wave-15-test-worktree/.wave/design/wave-15.md");
        fs::create_dir_all(design_review_path.parent().expect("design parent")).expect("mkdir");
        fs::write(
            &design_review_path,
            "# Wave 15 Design Review\n\n[wave-design] state=concerns findings=1 detail=mixed runtime summaries remain explicit in operator surfaces\n",
        )
        .expect("write design review");

        let envelope = build_structured_result_envelope(&root, &run, &agent, agent_record, 15)
            .expect("build structured envelope");

        match envelope.closure.verdict {
            ClosureVerdictPayload::Design(verdict) => {
                assert_eq!(verdict.state.as_deref(), Some("concerns"));
                assert_eq!(verdict.findings, Some(1));
                assert_eq!(
                    verdict.detail.as_deref(),
                    Some("mixed runtime summaries remain explicit in operator surfaces")
                );
            }
            other => panic!("expected design verdict, got {other:?}"),
        }
        assert_eq!(envelope.closure.disposition, ClosureDisposition::Ready);
        assert!(envelope.closure.blocking_reasons.is_empty());
        assert!(envelope
            .closure_input
            .marker_evidence
            .iter()
            .any(|evidence| {
                evidence.marker == "[wave-design]"
                    && evidence.source.as_deref()
                        == Some(
                            ".wave/state/worktrees/wave-15-test-worktree/.wave/design/wave-15.md",
                        )
            }));
        assert!(envelope.proof.artifacts.iter().any(|artifact| {
            artifact.path == ".wave/state/worktrees/wave-15-test-worktree/.wave/design/wave-15.md"
        }));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn structured_envelope_uses_cont_qa_review_when_last_message_is_incomplete() {
        let root = temp_root("structured-cont-qa-fallback");
        let agent = declared_agent("A0", "Running cont-QA", vec!["[wave-gate]"]);
        let run = structured_run(&root, 12, "A0", WaveRunStatus::Succeeded);
        let agent_record = &run.agents[0];
        let last_message_path = resolve_path(&root, &agent_record.last_message_path);
        fs::create_dir_all(last_message_path.parent().expect("message parent")).expect("mkdir");
        fs::write(&last_message_path, "Reviewed the current worktree.\n")
            .expect("write last message");
        let review_path = root.join(".wave/reviews/wave-12-cont-qa.md");
        fs::create_dir_all(review_path.parent().expect("review parent")).expect("mkdir");
        fs::write(
            &review_path,
            "# Wave 12 cont-QA\n\n[wave-gate] architecture=pass integration=pass durability=pass live=pass docs=pass detail=envelope-first proof lifecycle is aligned\nVerdict: PASS\n",
        )
        .expect("write cont-qa review");

        let envelope = build_structured_result_envelope(&root, &run, &agent, agent_record, 12)
            .expect("build structured envelope");

        match envelope.closure.verdict {
            ClosureVerdictPayload::ContQa(verdict) => {
                assert_eq!(verdict.verdict.as_deref(), Some("PASS"));
                assert_eq!(verdict.gate_state.as_deref(), Some("pass"));
                assert_eq!(
                    verdict.detail.as_deref(),
                    Some("envelope-first proof lifecycle is aligned")
                );
            }
            other => panic!("expected cont-qa verdict, got {other:?}"),
        }
        assert_eq!(envelope.closure.disposition, ClosureDisposition::Ready);
        assert!(envelope.closure.blocking_reasons.is_empty());
        assert!(
            envelope
                .closure_input
                .marker_evidence
                .iter()
                .any(|evidence| {
                    evidence.marker == "[wave-gate]"
                        && evidence.source.as_deref() == Some(".wave/reviews/wave-12-cont-qa.md")
                })
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn synthetic_marker_evidence_does_not_falsely_claim_last_message_source() {
        let root = temp_root("synthetic-marker-source");
        let agent = declared_agent("A1", "Result Envelope Core", vec!["[wave-proof]"]);
        let run = structured_run(&root, 12, "A1", WaveRunStatus::Succeeded);
        let agent_record = &run.agents[0];
        let last_message_path = resolve_path(&root, &agent_record.last_message_path);
        fs::create_dir_all(last_message_path.parent().expect("message parent")).expect("mkdir");
        fs::write(&last_message_path, "summary without final marker\n")
            .expect("write last message");

        let envelope = build_structured_result_envelope(&root, &run, &agent, agent_record, 12)
            .expect("build structured envelope");

        assert!(
            envelope
                .closure_input
                .marker_evidence
                .iter()
                .any(|evidence| evidence.marker == "[wave-proof]" && evidence.source.is_none())
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn validate_result_envelope_rejects_inconsistent_closure_state() {
        let mut envelope = structured_envelope(
            12,
            "A8",
            "attempt-a8-1",
            "result-a8-1",
            AttemptState::Succeeded,
            vec!["[wave-integration]".to_string()],
            vec!["[wave-integration]".to_string()],
            8,
        );
        envelope.closure.verdict = ClosureVerdictPayload::Integration(IntegrationClosureVerdict {
            state: Some("ready-for-doc-closure".to_string()),
            claims: Some(1),
            conflicts: Some(0),
            blockers: Some(0),
            detail: Some("structured closure verdict is present".to_string()),
        });
        envelope.closure.disposition = ClosureDisposition::Pending;

        let error = validate_result_envelope(&envelope).expect_err("validation should fail");
        let message = error.to_string();
        assert!(message.contains("closure.disposition"));
        assert!(message.contains("does not match attempt_state"));
    }

    #[test]
    fn validate_result_envelope_rejects_inconsistent_runtime_record() {
        let mut envelope = structured_envelope(
            15,
            "A1",
            "attempt-runtime-invalid",
            "result-runtime-invalid",
            AttemptState::Succeeded,
            vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            15,
        );
        envelope.runtime = Some(wave_domain::RuntimeExecutionRecord {
            policy: wave_domain::RuntimeSelectionPolicy {
                requested_runtime: Some(wave_domain::RuntimeId::Codex),
                allowed_runtimes: vec![
                    wave_domain::RuntimeId::Codex,
                    wave_domain::RuntimeId::Claude,
                    wave_domain::RuntimeId::Claude,
                ],
                fallback_runtimes: vec![wave_domain::RuntimeId::Claude],
                selection_source: Some("executor.id".to_string()),
            },
            selected_runtime: wave_domain::RuntimeId::Claude,
            selection_reason: "selected claude after fallback".to_string(),
            fallback: Some(wave_domain::RuntimeFallbackRecord {
                requested_runtime: wave_domain::RuntimeId::Local,
                selected_runtime: wave_domain::RuntimeId::Codex,
                reason: "codex unavailable".to_string(),
            }),
            execution_identity: wave_domain::RuntimeExecutionIdentity {
                runtime: wave_domain::RuntimeId::Codex,
                adapter: "wave-runtime/codex".to_string(),
                binary: "/tmp/fake-codex".to_string(),
                provider: "openai-codex".to_string(),
                artifact_paths: BTreeMap::new(),
            },
            skill_projection: wave_domain::RuntimeSkillProjection {
                declared_skills: vec!["wave-core".to_string(), "wave-core".to_string()],
                projected_skills: vec!["wave-core".to_string(), "runtime-codex".to_string()],
                dropped_skills: Vec::new(),
                auto_attached_skills: vec!["runtime-claude".to_string()],
            },
        });

        let error = validate_result_envelope(&envelope).expect_err("validation should fail");
        let message = error.to_string();
        assert!(message.contains("runtime.policy"));
        assert!(message.contains("runtime.skill_projection"));
        assert!(message.contains("runtime.execution_identity.adapter"));
        assert!(message.contains("runtime.execution_identity.runtime"));
        assert!(message.contains("runtime.fallback.requested_runtime"));
        assert!(message.contains("runtime.fallback.selected_runtime"));
    }

    fn temp_root(label: &str) -> PathBuf {
        let counter = TEMP_ROOT_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        std::env::temp_dir().join(format!(
            "wave-results-{label}-{}-{counter}",
            std::process::id(),
        ))
    }

    fn declared_agent(id: &str, title: &str, final_markers: Vec<&str>) -> WaveAgent {
        WaveAgent {
            id: id.to_string(),
            title: title.to_string(),
            role_prompts: Vec::new(),
            executor: std::collections::BTreeMap::new(),
            context7: None,
            skills: Vec::new(),
            components: Vec::new(),
            capabilities: Vec::new(),
            exit_contract: None,
            deliverables: Vec::new(),
            file_ownership: Vec::new(),
            final_markers: final_markers.into_iter().map(str::to_string).collect(),
            depends_on_agents: Vec::new(),
            reads_artifacts_from: Vec::new(),
            writes_artifacts: Vec::new(),
            barrier_class: wave_spec::BarrierClass::Independent,
            parallel_safety: wave_spec::ParallelSafetyClass::Derived,
            exclusive_resources: Vec::new(),
            parallel_with: Vec::new(),
            prompt: String::new(),
        }
    }

    fn structured_run(
        root: &Path,
        wave_id: u32,
        agent_id: &str,
        status: WaveRunStatus,
    ) -> WaveRunRecord {
        let agent_dir = root.join(format!(
            ".wave/state/build/specs/wave-{wave_id}-test/agents/{agent_id}"
        ));
        WaveRunRecord {
            run_id: format!("wave-{wave_id}-test"),
            wave_id,
            slug: "result-envelope-proof-lifecycle".to_string(),
            title: "Land result envelopes and proof lifecycle".to_string(),
            status,
            dry_run: false,
            bundle_dir: root.join(format!(".wave/state/build/specs/wave-{wave_id}-test")),
            trace_path: root.join(format!(".wave/traces/runs/wave-{wave_id}-test.json")),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(3),
            agents: vec![AgentRunRecord {
                id: agent_id.to_string(),
                title: "Test agent".to_string(),
                status,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: PathBuf::from(format!(
                    ".wave/state/build/specs/wave-{wave_id}-test/agents/{agent_id}/last-message.txt"
                )),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: None,
                expected_markers: declared_agent(agent_id, "Test agent", vec![])
                    .expected_final_markers()
                    .iter()
                    .map(|marker| marker.to_string())
                    .collect(),
                observed_markers: declared_agent(agent_id, "Test agent", vec![])
                    .expected_final_markers()
                    .iter()
                    .map(|marker| marker.to_string())
                    .collect(),
                exit_code: Some(0),
                error: None,
                runtime: None,
            }],
            error: None,
        }
    }

    fn structured_envelope(
        wave_id: u32,
        agent_id: &str,
        attempt_id: &str,
        result_envelope_id: &str,
        attempt_state: AttemptState,
        required_markers: Vec<String>,
        observed_markers: Vec<String>,
        created_at_ms: u128,
    ) -> ResultEnvelope {
        let final_markers = FinalMarkerEnvelope::from_contract(required_markers, observed_markers);
        let closure = ClosureState {
            disposition: match attempt_state {
                AttemptState::Succeeded if final_markers.is_satisfied() => {
                    ClosureDisposition::Ready
                }
                AttemptState::Planned | AttemptState::Running => ClosureDisposition::Pending,
                _ => ClosureDisposition::Blocked,
            },
            required_final_markers: final_markers.required.clone(),
            observed_final_markers: final_markers.observed.clone(),
            blocking_reasons: final_markers
                .missing
                .iter()
                .map(|marker| format!("missing final marker: {marker}"))
                .collect(),
            satisfied_fact_ids: Vec::new(),
            contradiction_ids: Vec::new(),
            verdict: ClosureVerdictPayload::None,
        };

        ResultEnvelope {
            result_envelope_id: ResultEnvelopeId::new(result_envelope_id),
            wave_id,
            task_id: task_id_for_agent(wave_id, agent_id),
            attempt_id: AttemptId::new(attempt_id),
            agent_id: agent_id.to_string(),
            task_role: inferred_task_role_for_agent(agent_id, &[]),
            closure_role: inferred_closure_role_for_agent(agent_id),
            source: ResultEnvelopeSource::Structured,
            attempt_state,
            disposition: ResultDisposition::from_attempt_state(
                attempt_state,
                final_markers.missing.len(),
            ),
            summary: Some(format!("structured envelope for {agent_id}")),
            output_text: None,
            proof: ProofEnvelope::default(),
            doc_delta: DocDeltaEnvelope::default(),
            closure_input: ClosureInputEnvelope {
                status: ResultPayloadStatus::Missing,
                final_markers,
                marker_evidence: Vec::new(),
            },
            closure,
            runtime: None,
            created_at_ms,
        }
    }

    fn sample_claude_runtime_record(
        root: &Path,
        overlay_path: &str,
        include_runtime_detail_path: bool,
    ) -> wave_domain::RuntimeExecutionRecord {
        let mut artifact_paths = BTreeMap::from([(
            "skill_overlay".to_string(),
            root.join(overlay_path).to_string_lossy().into_owned(),
        )]);
        if include_runtime_detail_path {
            artifact_paths.insert(
                "runtime_detail".to_string(),
                root.join(".wave/state/build/specs/wave-15-legacy/agents/A1/runtime-detail.json")
                    .to_string_lossy()
                    .into_owned(),
            );
        }

        wave_domain::RuntimeExecutionRecord {
            policy: wave_domain::RuntimeSelectionPolicy {
                requested_runtime: Some(wave_domain::RuntimeId::Codex),
                allowed_runtimes: vec![
                    wave_domain::RuntimeId::Codex,
                    wave_domain::RuntimeId::Claude,
                    wave_domain::RuntimeId::Claude,
                ],
                fallback_runtimes: vec![
                    wave_domain::RuntimeId::Claude,
                    wave_domain::RuntimeId::Claude,
                ],
                selection_source: Some("executor.id".to_string()),
            },
            selected_runtime: wave_domain::RuntimeId::Claude,
            selection_reason:
                "selected claude after fallback because codex login status reported unavailable"
                    .to_string(),
            fallback: Some(wave_domain::RuntimeFallbackRecord {
                requested_runtime: wave_domain::RuntimeId::Codex,
                selected_runtime: wave_domain::RuntimeId::Claude,
                reason: "codex login status reported unavailable".to_string(),
            }),
            execution_identity: wave_domain::RuntimeExecutionIdentity {
                runtime: wave_domain::RuntimeId::Claude,
                adapter: "wave-runtime/claude".to_string(),
                binary: "/tmp/fake-claude".to_string(),
                provider: "anthropic-claude-code".to_string(),
                artifact_paths,
            },
            skill_projection: wave_domain::RuntimeSkillProjection {
                declared_skills: vec!["wave-core".to_string(), "wave-core".to_string()],
                projected_skills: vec!["wave-core".to_string(), "runtime-claude".to_string()],
                dropped_skills: Vec::new(),
                auto_attached_skills: vec![
                    "runtime-claude".to_string(),
                    "runtime-claude".to_string(),
                ],
            },
        }
    }
}
