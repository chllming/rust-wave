use serde::Serialize;
use std::collections::HashSet;
use wave_spec::WaveDocument;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LintFinding {
    pub wave_id: u32,
    pub severity: FindingSeverity,
    pub rule: &'static str,
    pub message: String,
}

pub fn lint_project(waves: &[WaveDocument]) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    let mut seen = HashSet::new();
    let known_ids: HashSet<u32> = waves.iter().map(|wave| wave.metadata.id).collect();

    for wave in waves {
        if !seen.insert(wave.metadata.id) {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "unique-wave-id",
                message: format!("wave {} appears more than once", wave.metadata.id),
            });
        }

        if wave.goal.trim().is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "goal-required",
                message: "wave goal section must not be empty".to_string(),
            });
        }

        if wave.deliverables.is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "deliverables-required",
                message: "wave deliverables must declare at least one bullet".to_string(),
            });
        }

        if wave.closure.is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "closure-required",
                message: "wave closure must declare at least one bullet".to_string(),
            });
        }

        for dependency in &wave.metadata.depends_on {
            if !known_ids.contains(dependency) {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "dependency-known",
                    message: format!("wave depends on unknown wave {}", dependency),
                });
            }
        }

        if wave.metadata.validation.is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "dark-factory-validation",
                message: "dark-factory wave is missing validation commands".to_string(),
            });
        }

        if wave.metadata.rollback.is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "dark-factory-rollback",
                message: "dark-factory wave is missing rollback guidance".to_string(),
            });
        }

        if wave.metadata.proof.is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "dark-factory-proof",
                message: "dark-factory wave is missing proof artifacts".to_string(),
            });
        }
    }

    findings.sort_by_key(|finding| (finding.wave_id, finding.rule));
    findings
}

pub fn has_errors(findings: &[LintFinding]) -> bool {
    findings
        .iter()
        .any(|finding| matches!(finding.severity, FindingSeverity::Error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use wave_config::ExecutionMode;
    use wave_spec::WaveDocument;
    use wave_spec::WaveMetadata;

    #[test]
    fn flags_empty_dark_factory_sections() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/00.md"),
            metadata: WaveMetadata {
                id: 0,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: Vec::new(),
                rollback: Vec::new(),
                proof: Vec::new(),
            },
            goal: String::new(),
            deliverables: Vec::new(),
            closure: Vec::new(),
        };

        let findings = lint_project(&[wave]);
        assert!(has_errors(&findings));
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "dark-factory-validation")
        );
    }
}
