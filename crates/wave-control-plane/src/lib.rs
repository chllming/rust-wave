use serde::Serialize;
use std::collections::HashMap;
use wave_config::ExecutionMode;
use wave_config::ProjectConfig;
use wave_dark_factory::LintFinding;
use wave_dark_factory::has_errors;
use wave_spec::WaveDocument;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveQueueEntry {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub depends_on: Vec<u32>,
    pub blocked_by: Vec<String>,
    pub lint_errors: usize,
    pub ready: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningStatus {
    pub project_name: String,
    pub default_mode: ExecutionMode,
    pub next_ready_wave_ids: Vec<u32>,
    pub waves: Vec<WaveQueueEntry>,
    pub has_errors: bool,
}

pub fn build_planning_status(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
) -> PlanningStatus {
    let mut findings_by_wave: HashMap<u32, usize> = HashMap::new();
    for finding in findings {
        *findings_by_wave.entry(finding.wave_id).or_default() += 1;
    }

    let mut entries = Vec::new();
    for wave in waves {
        let lint_errors = findings_by_wave
            .get(&wave.metadata.id)
            .copied()
            .unwrap_or_default();
        let mut blocked_by = wave
            .metadata
            .depends_on
            .iter()
            .map(|dependency| format!("wave:{dependency}"))
            .collect::<Vec<_>>();
        if lint_errors > 0 {
            blocked_by.push("lint:error".to_string());
        }

        entries.push(WaveQueueEntry {
            id: wave.metadata.id,
            slug: wave.metadata.slug.clone(),
            title: wave.metadata.title.clone(),
            depends_on: wave.metadata.depends_on.clone(),
            blocked_by,
            lint_errors,
            ready: wave.metadata.depends_on.is_empty() && lint_errors == 0,
        });
    }

    let next_ready_wave_ids = entries
        .iter()
        .filter(|entry| entry.ready)
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    PlanningStatus {
        project_name: config.project_name.clone(),
        default_mode: config.default_mode,
        next_ready_wave_ids,
        waves: entries,
        has_errors: has_errors(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use wave_config::DarkFactoryPolicy;
    use wave_config::LaneConfig;
    use wave_dark_factory::FindingSeverity;
    use wave_spec::WaveMetadata;

    #[test]
    fn ready_wave_requires_no_dependencies_or_errors() {
        let config = ProjectConfig {
            version: 1,
            project_name: "Test".to_string(),
            default_lane: "main".to_string(),
            default_mode: ExecutionMode::DarkFactory,
            waves_dir: PathBuf::from("waves"),
            project_codex_home: PathBuf::from(".wave/codex"),
            state_dir: PathBuf::from(".wave/state"),
            trace_dir: PathBuf::from(".wave/traces"),
            codex_vendor_dir: PathBuf::from("third_party/codex-rs"),
            reference_wave_repo_dir: PathBuf::from("third_party/agent-wave-orchestrator"),
            dark_factory: DarkFactoryPolicy {
                require_validation: true,
                require_rollback: true,
                require_proof: true,
                require_closure: true,
            },
            lanes: BTreeMap::<String, LaneConfig>::new(),
        };
        let waves = vec![WaveDocument {
            path: PathBuf::from("waves/00.md"),
            metadata: WaveMetadata {
                id: 0,
                slug: "bootstrap".to_string(),
                title: "Bootstrap".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["revert".to_string()],
                proof: vec!["proof".to_string()],
            },
            goal: "goal".to_string(),
            deliverables: vec!["deliverable".to_string()],
            closure: vec!["closure".to_string()],
        }];

        let findings = vec![LintFinding {
            wave_id: 0,
            severity: FindingSeverity::Error,
            rule: "lint",
            message: "error".to_string(),
        }];

        let status = build_planning_status(&config, &waves, &findings);
        assert!(!status.waves[0].ready);
    }
}
