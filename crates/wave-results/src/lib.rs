use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
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
use wave_domain::TaskId;
use wave_domain::inferred_closure_role_for_agent;
use wave_domain::inferred_task_role_for_agent;
use wave_domain::task_id_for_agent;
use wave_trace::AgentRunRecord;
use wave_trace::WaveRunRecord;
use wave_trace::WaveRunStatus;

pub const RESULT_ENVELOPE_FILE_NAME: &str = "agent_result_envelope.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultEnvelopeStore {
    root_dir: PathBuf,
    repo_root: Option<PathBuf>,
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
                &envelope.task_id == task_id
                    && matches!(
                        envelope.disposition,
                        ResultDisposition::Completed | ResultDisposition::Failed
                    )
            })
            .max_by(compare_envelopes))
    }
}

pub fn canonical_results_root(repo_root: &Path) -> PathBuf {
    repo_root.join(DEFAULT_STATE_RESULTS_DIR)
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
    if doc_delta.summary.is_some() || !doc_delta.paths.is_empty() {
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

    if closure_input.has_evidence()
        || closure.disposition != ClosureDisposition::Pending
        || !closure.blocking_reasons.is_empty()
        || !closure.satisfied_fact_ids.is_empty()
        || !closure.contradiction_ids.is_empty()
        || !matches!(closure.verdict, ClosureVerdictPayload::None)
    {
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

    let expected_disposition = ResultDisposition::from_attempt_state(
        envelope.attempt_state,
        envelope.closure_input.final_markers.missing.len(),
    );
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

pub fn adapt_legacy_run_record(
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
    let final_markers = FinalMarkerEnvelope::from_contract(
        agent.expected_markers.clone(),
        agent.observed_markers.clone(),
    );
    let last_message_path = resolve_path(repo_root, &agent.last_message_path);
    let output_text = read_optional_text(&last_message_path)?;
    let marker_evidence = collect_marker_evidence(
        output_text.as_deref(),
        &final_markers.observed,
        &last_message_path,
        repo_root,
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
            proof: legacy_proof_payload(&final_markers),
            doc_delta: legacy_doc_delta_payload(agent),
            closure_input: ClosureInputEnvelope {
                status: ResultPayloadStatus::EvidenceOnly,
                final_markers,
                marker_evidence,
            },
            closure,
            created_at_ms: run
                .completed_at_ms
                .or(run.started_at_ms)
                .unwrap_or(run.created_at_ms),
        },
        Some(repo_root),
    )
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

fn legacy_proof_payload(final_markers: &FinalMarkerEnvelope) -> ProofEnvelope {
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
        artifacts: Vec::new(),
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
    let fields = find_marker_fields(output_text, "[wave-integration]")
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

fn parse_documentation_verdict(output_text: &str) -> DocumentationClosureVerdict {
    let fields = find_marker_fields(output_text, "[wave-doc-closure]")
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

fn collect_marker_evidence(
    output_text: Option<&str>,
    observed_markers: &[String],
    source_path: &Path,
    repo_root: &Path,
    run_id: &str,
) -> Vec<MarkerEvidence> {
    let source = normalize_path_string(source_path, Some(repo_root));
    let mut evidence = Vec::new();

    if let Some(text) = output_text {
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
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
                source: Some(format!("legacy-run-record:{run_id}")),
            });
        }
    }

    normalize_marker_evidence(&evidence, Some(repo_root))
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
                expected_markers: vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                observed_markers: vec!["[wave-proof]".to_string(), "[wave-doc-delta]".to_string()],
                exit_code: Some(1),
                error: Some("missing component proof".to_string()),
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
                expected_markers: vec!["[wave-integration]".to_string()],
                observed_markers: vec!["[wave-integration]".to_string()],
                exit_code: Some(0),
                error: None,
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

    fn temp_root(label: &str) -> PathBuf {
        let counter = TEMP_ROOT_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        std::env::temp_dir().join(format!(
            "wave-results-{label}-{}-{counter}",
            std::process::id(),
        ))
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
            created_at_ms,
        }
    }
}
