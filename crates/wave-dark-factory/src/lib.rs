use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use wave_spec::WaveAgent;
use wave_spec::WaveDocument;

const REQUIRED_PROMPT_SECTIONS: [&str; 4] = [
    "Primary goal",
    "Required context before coding",
    "Specific expectations",
    "File ownership (only touch these paths)",
];
const FILE_OWNERSHIP_PROMPT_SECTION: &str = "File ownership (only touch these paths)";

const REQUIRED_CLOSURE_AGENT_IDS: [&str; 3] = ["A0", "A8", "A9"];

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillCatalogIssue {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct SkillManifest {
    id: Option<String>,
    title: Option<String>,
    description: Option<String>,
    activation: Option<SkillActivation>,
}

#[derive(Debug, Deserialize)]
struct SkillActivation {
    when: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Context7BundleCatalog {
    bundles: HashMap<String, serde_json::Value>,
}

pub fn lint_project(root: &Path, waves: &[WaveDocument]) -> Vec<LintFinding> {
    let mut findings = Vec::new();
    let mut seen_wave_ids = HashSet::new();
    let known_wave_ids: HashSet<u32> = waves.iter().map(|wave| wave.metadata.id).collect();
    let known_skill_ids = discover_skill_ids(root);
    let known_context7_bundles = discover_context7_bundle_ids(root);

    for wave in waves {
        if !seen_wave_ids.insert(wave.metadata.id) {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "unique-wave-id",
                message: format!("wave {} appears more than once", wave.metadata.id),
            });
        }

        if wave
            .heading_title
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "heading-required",
                message: "wave must declare a markdown heading".to_string(),
            });
        }

        if wave
            .commit_message
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "commit-message-required",
                message: "wave must declare a commit message".to_string(),
            });
        }

        if wave.component_promotions.is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "component-promotions-required",
                message: "wave must declare at least one component promotion".to_string(),
            });
        }

        if wave.deploy_environments.is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "deploy-environments-required",
                message: "wave must declare at least one deploy environment".to_string(),
            });
        }

        let context7 = wave.context7_defaults.as_ref();
        if context7.is_none() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "context7-defaults-required",
                message: "wave must declare Context7 defaults".to_string(),
            });
        }

        if let Some(context7) = context7 {
            if context7.bundle.trim().is_empty() {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "context7-bundle-required",
                    message: "Context7 defaults must declare a bundle".to_string(),
                });
            }

            if context7.query.as_deref().unwrap_or("").trim().is_empty() {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "context7-query-required",
                    message: "Context7 defaults must declare a narrow query".to_string(),
                });
            }

            lint_context7_bundle_id(
                wave.metadata.id,
                "wave",
                &context7.bundle,
                &known_context7_bundles,
                &mut findings,
            );
        }

        for dependency in &wave.metadata.depends_on {
            if !known_wave_ids.contains(dependency) {
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

        if wave.agents.is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "agents-required",
                message: "wave must declare at least one agent".to_string(),
            });
            continue;
        }

        let mut seen_agent_ids = HashSet::new();
        let mut present_required_closure_agents = HashSet::new();
        let mut owned_paths: Vec<(String, String)> = Vec::new();

        for agent in &wave.agents {
            lint_agent(
                root,
                wave.metadata.id,
                agent,
                &known_skill_ids,
                &known_context7_bundles,
                &mut findings,
                &mut seen_agent_ids,
                &mut present_required_closure_agents,
                &mut owned_paths,
            );
        }

        for required_id in REQUIRED_CLOSURE_AGENT_IDS {
            if !present_required_closure_agents.contains(required_id) {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "required-closure-agent",
                    message: format!("wave must declare closure agent {required_id}"),
                });
            }
        }

        for left in 0..owned_paths.len() {
            for right in left + 1..owned_paths.len() {
                let (left_agent, left_path) = &owned_paths[left];
                let (right_agent, right_path) = &owned_paths[right];
                if ownership_conflict(left_path, right_path) {
                    findings.push(LintFinding {
                        wave_id: wave.metadata.id,
                        severity: FindingSeverity::Error,
                        rule: "file-ownership-overlap",
                        message: format!(
                            "agents {left_agent} and {right_agent} declare overlapping ownership: {left_path} vs {right_path}"
                        ),
                    });
                }
            }
        }
    }

    findings.sort_by_key(|finding| (finding.wave_id, finding.rule, finding.message.clone()));
    findings
}

pub fn validate_skill_catalog(root: &Path) -> Vec<SkillCatalogIssue> {
    load_skill_catalog(root).0
}

pub fn has_errors(findings: &[LintFinding]) -> bool {
    findings
        .iter()
        .any(|finding| matches!(finding.severity, FindingSeverity::Error))
}

fn lint_agent(
    root: &Path,
    wave_id: u32,
    agent: &WaveAgent,
    known_skill_ids: &HashSet<String>,
    known_context7_bundles: &HashSet<String>,
    findings: &mut Vec<LintFinding>,
    seen_agent_ids: &mut HashSet<String>,
    present_required_closure_agents: &mut HashSet<String>,
    owned_paths: &mut Vec<(String, String)>,
) {
    if !seen_agent_ids.insert(agent.id.clone()) {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "unique-agent-id",
            message: format!("agent {} appears more than once", agent.id),
        });
    }

    if agent.title.trim().is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-title-required",
            message: format!("agent {} must declare a title", agent.id),
        });
    }

    if agent.is_required_closure_agent() {
        present_required_closure_agents.insert(agent.id.clone());
    }

    if agent.executor.is_empty()
        || !(agent.executor.contains_key("profile") || agent.executor.contains_key("id"))
    {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-executor-required",
            message: format!("agent {} must declare an executor profile", agent.id),
        });
    }

    lint_role_prompts(root, wave_id, agent, findings);
    lint_context7(wave_id, agent, known_context7_bundles, findings);
    lint_prompt(wave_id, agent, findings);

    if agent.file_ownership.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-file-ownership-required",
            message: format!("agent {} must declare file ownership", agent.id),
        });
    } else {
        for path in &agent.file_ownership {
            owned_paths.push((agent.id.clone(), path.clone()));
        }
    }

    if agent.final_markers.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-final-markers-required",
            message: format!("agent {} must declare final markers", agent.id),
        });
    }

    let expected_markers = agent.expected_final_markers();
    let expected_marker_set = expected_markers.iter().copied().collect::<HashSet<_>>();
    let mut seen_final_markers = HashSet::new();

    for marker in &agent.final_markers {
        if !seen_final_markers.insert(marker.as_str()) {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "agent-final-marker-duplicate",
                message: format!(
                    "agent {} declares final marker {} more than once",
                    agent.id, marker
                ),
            });
        }

        if !expected_marker_set.contains(marker.as_str()) {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "agent-final-marker-unexpected",
                message: format!(
                    "agent {} declares unexpected final marker {}",
                    agent.id, marker
                ),
            });
        }
    }

    for expected_marker in expected_markers {
        if !agent
            .final_markers
            .iter()
            .any(|marker| marker == expected_marker)
        {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "agent-final-marker-missing",
                message: format!(
                    "agent {} must declare final marker {}",
                    agent.id, expected_marker
                ),
            });
        }
    }

    for skill_id in &agent.skills {
        if !known_skill_ids.contains(skill_id) {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "known-skill-id",
                message: format!("agent {} references unknown skill {}", agent.id, skill_id),
            });
        }
    }

    if agent.is_closure_agent() {
        return;
    }

    if agent.skills.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-skills-required",
            message: format!("implementation agent {} must declare skills", agent.id),
        });
    }

    if agent.components.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-components-required",
            message: format!("implementation agent {} must declare components", agent.id),
        });
    }

    if agent.capabilities.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-capabilities-required",
            message: format!(
                "implementation agent {} must declare capabilities",
                agent.id
            ),
        });
    }

    if agent.exit_contract.is_none() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-exit-contract-required",
            message: format!(
                "implementation agent {} must declare an exit contract",
                agent.id
            ),
        });
    }

    if agent.deliverables.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-deliverables-required",
            message: format!(
                "implementation agent {} must declare deliverables",
                agent.id
            ),
        });
    }
}

fn lint_role_prompts(
    root: &Path,
    wave_id: u32,
    agent: &WaveAgent,
    findings: &mut Vec<LintFinding>,
) {
    let required_prompt = match agent.id.as_str() {
        "A0" => Some("docs/agents/wave-cont-qa-role.md"),
        "A8" => Some("docs/agents/wave-integration-role.md"),
        "A9" => Some("docs/agents/wave-documentation-role.md"),
        "E0" => Some("docs/agents/wave-cont-eval-role.md"),
        _ => None,
    };

    if agent.is_closure_agent() && agent.role_prompts.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "closure-role-prompts-required",
            message: format!("closure agent {} must declare role prompts", agent.id),
        });
    }

    if let Some(required_prompt) = required_prompt {
        if !agent
            .role_prompts
            .iter()
            .any(|prompt| prompt == required_prompt)
        {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "closure-role-prompt-required",
                message: format!(
                    "closure agent {} must include role prompt {}",
                    agent.id, required_prompt
                ),
            });
        }
    }

    for role_prompt in &agent.role_prompts {
        let resolved = root.join(role_prompt);
        if !resolved.exists() {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "role-prompt-path-exists",
                message: format!(
                    "agent {} references missing role prompt {}",
                    agent.id,
                    resolved.display()
                ),
            });
        }
    }
}

fn lint_context7(
    wave_id: u32,
    agent: &WaveAgent,
    known_context7_bundles: &HashSet<String>,
    findings: &mut Vec<LintFinding>,
) {
    let Some(context7) = agent.context7.as_ref() else {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-context7-required",
            message: format!("agent {} must declare Context7 defaults", agent.id),
        });
        return;
    };

    if context7.bundle.trim().is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-context7-bundle-required",
            message: format!("agent {} must declare a Context7 bundle", agent.id),
        });
    }

    if context7.query.as_deref().unwrap_or("").trim().is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-context7-query-required",
            message: format!("agent {} must declare a Context7 query", agent.id),
        });
    }

    lint_context7_bundle_id(
        wave_id,
        &format!("agent {}", agent.id),
        &context7.bundle,
        known_context7_bundles,
        findings,
    );
}

fn lint_prompt(wave_id: u32, agent: &WaveAgent, findings: &mut Vec<LintFinding>) {
    if agent.prompt.trim().is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-prompt-required",
            message: format!("agent {} must declare a prompt", agent.id),
        });
        return;
    }

    for section in REQUIRED_PROMPT_SECTIONS {
        let items = agent.prompt_list_section(section);
        if items.is_empty() {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "agent-prompt-section-required",
                message: format!(
                    "agent {} prompt must declare a non-empty {} section",
                    agent.id, section
                ),
            });
        }
    }

    let prompt_owned_paths = agent.prompt_list_section(FILE_OWNERSHIP_PROMPT_SECTION);
    if !prompt_owned_paths.is_empty() {
        lint_prompt_owned_paths(wave_id, agent, &prompt_owned_paths, findings);
    }

    for expected_marker in agent.expected_final_markers() {
        if !agent.prompt.contains(expected_marker) {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "agent-prompt-final-marker-required",
                message: format!(
                    "agent {} prompt must mention final marker {}",
                    agent.id, expected_marker
                ),
            });
        }
    }
}

fn lint_prompt_owned_paths(
    wave_id: u32,
    agent: &WaveAgent,
    prompt_owned_paths: &[String],
    findings: &mut Vec<LintFinding>,
) {
    let declared_paths = normalized_owned_path_set(&agent.file_ownership);
    let prompt_paths = normalized_owned_path_set(prompt_owned_paths);

    for missing_path in declared_paths.difference(&prompt_paths) {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-prompt-owned-path-missing",
            message: format!(
                "agent {} prompt is missing owned path {}",
                agent.id, missing_path
            ),
        });
    }

    for unexpected_path in prompt_paths.difference(&declared_paths) {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-prompt-owned-path-unexpected",
            message: format!(
                "agent {} prompt declares unexpected owned path {}",
                agent.id, unexpected_path
            ),
        });
    }
}

fn normalized_owned_path_set(paths: &[String]) -> BTreeSet<String> {
    paths
        .iter()
        .map(|path| normalize_owned_path(path))
        .collect()
}

fn load_skill_catalog(root: &Path) -> (Vec<SkillCatalogIssue>, HashSet<String>) {
    let skills_dir = root.join("skills");
    let mut issues = Vec::new();
    let mut ids = HashSet::new();

    let entries = match fs::read_dir(&skills_dir) {
        Ok(entries) => entries,
        Err(error) => {
            issues.push(SkillCatalogIssue {
                path: skills_dir.display().to_string(),
                message: format!("failed to read skills directory: {error}"),
            });
            return (issues, ids);
        }
    };

    let mut saw_manifest = false;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                issues.push(SkillCatalogIssue {
                    path: skills_dir.display().to_string(),
                    message: format!("failed to read skills entry: {error}"),
                });
                continue;
            }
        };

        let manifest_path = entry.path().join("skill.json");
        if !manifest_path.exists() {
            continue;
        }
        saw_manifest = true;

        let skill_body_path = entry.path().join("SKILL.md");
        let mut manifest_valid = true;
        if !skill_body_path.exists() {
            issues.push(SkillCatalogIssue {
                path: skill_body_path.display().to_string(),
                message: "skill bundle is missing SKILL.md".to_string(),
            });
            manifest_valid = false;
        }

        let manifest = match read_skill_manifest(&manifest_path) {
            Ok(manifest) => manifest,
            Err(message) => {
                issues.push(SkillCatalogIssue {
                    path: manifest_path.display().to_string(),
                    message,
                });
                continue;
            }
        };

        let Some(manifest_id) = manifest_field(manifest.id.as_deref()) else {
            issues.push(SkillCatalogIssue {
                path: manifest_path.display().to_string(),
                message: "skill manifest id must not be empty".to_string(),
            });
            continue;
        };

        if manifest_field(manifest.title.as_deref()).is_none() {
            issues.push(SkillCatalogIssue {
                path: manifest_path.display().to_string(),
                message: "skill manifest title must not be empty".to_string(),
            });
            manifest_valid = false;
        }

        if manifest_field(manifest.description.as_deref()).is_none() {
            issues.push(SkillCatalogIssue {
                path: manifest_path.display().to_string(),
                message: "skill manifest description must not be empty".to_string(),
            });
            manifest_valid = false;
        }

        if manifest_field(
            manifest
                .activation
                .as_ref()
                .and_then(|activation| activation.when.as_deref()),
        )
        .is_none()
        {
            issues.push(SkillCatalogIssue {
                path: manifest_path.display().to_string(),
                message: "skill manifest activation.when must not be empty".to_string(),
            });
            manifest_valid = false;
        }

        if entry.file_name().to_string_lossy() != manifest_id {
            issues.push(SkillCatalogIssue {
                path: manifest_path.display().to_string(),
                message: format!(
                    "skill manifest id {} does not match directory {}",
                    manifest_id,
                    entry.file_name().to_string_lossy()
                ),
            });
            manifest_valid = false;
        }

        if !manifest_valid {
            continue;
        }

        if !ids.insert(manifest_id.to_string()) {
            issues.push(SkillCatalogIssue {
                path: manifest_path.display().to_string(),
                message: format!("duplicate skill id {}", manifest_id),
            });
        }
    }

    if !saw_manifest {
        issues.push(SkillCatalogIssue {
            path: skills_dir.display().to_string(),
            message: "no skill manifests were discovered".to_string(),
        });
    }

    (issues, ids)
}

fn read_skill_manifest(path: &Path) -> Result<SkillManifest, String> {
    let raw =
        fs::read_to_string(path).map_err(|error| format!("failed to read manifest: {error}"))?;
    serde_json::from_str::<SkillManifest>(&raw)
        .map_err(|error| format!("failed to parse skill manifest: {error}"))
}

fn manifest_field(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.is_empty() { None } else { Some(value) }
}

fn discover_skill_ids(root: &Path) -> HashSet<String> {
    load_skill_catalog(root).1
}

fn discover_context7_bundle_ids(root: &Path) -> HashSet<String> {
    load_context7_bundle_catalog(root).unwrap_or_default()
}

fn load_context7_bundle_catalog(root: &Path) -> Result<HashSet<String>, String> {
    let path = root.join("docs/context7/bundles.json");
    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let catalog = serde_json::from_str::<Context7BundleCatalog>(&raw)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    Ok(catalog.bundles.into_keys().collect())
}

fn lint_context7_bundle_id(
    wave_id: u32,
    scope: &str,
    bundle: &str,
    known_context7_bundles: &HashSet<String>,
    findings: &mut Vec<LintFinding>,
) {
    let bundle = bundle.trim();
    if bundle.is_empty() || known_context7_bundles.contains(bundle) {
        return;
    }

    findings.push(LintFinding {
        wave_id,
        severity: FindingSeverity::Error,
        rule: "context7-bundle-known",
        message: format!("{scope} references unknown Context7 bundle {bundle}"),
    });
}

fn ownership_conflict(left: &str, right: &str) -> bool {
    let left = normalize_owned_path(left);
    let right = normalize_owned_path(right);

    left == right
        || left.starts_with(&(right.clone() + "/"))
        || right.starts_with(&(left.clone() + "/"))
}

fn normalize_owned_path(path: &str) -> String {
    path.trim().trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;
    use wave_config::ExecutionMode;
    use wave_spec::CompletionLevel;
    use wave_spec::Context7Defaults;
    use wave_spec::DeployEnvironment;
    use wave_spec::DocImpact;
    use wave_spec::DurabilityLevel;
    use wave_spec::ExitContract;
    use wave_spec::ProofLevel;
    use wave_spec::WaveMetadata;

    #[test]
    fn flags_missing_rich_wave_sections() {
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
            heading_title: None,
            commit_message: None,
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: Vec::new(),
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(has_errors(&findings));
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "commit-message-required")
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "component-promotions-required")
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "agents-required")
        );
    }

    #[test]
    fn flags_unknown_skills_and_missing_closure_agents() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/01.md"),
            metadata: WaveMetadata {
                id: 1,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A1".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
            },
            heading_title: Some("Wave 1 - Example".to_string()),
            commit_message: Some("Feat: example".to_string()),
            component_promotions: vec![wave_spec::ComponentPromotion {
                component: "example".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-cli-core".to_string(),
                query: Some("Context7 query".to_string()),
            }),
            agents: vec![WaveAgent {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                role_prompts: Vec::new(),
                executor: BTreeMap::from([("profile".to_string(), "implement-codex".to_string())]),
                context7: Some(Context7Defaults {
                    bundle: "rust-cli-core".to_string(),
                    query: Some("Implementation context".to_string()),
                }),
                skills: vec!["wave-core".to_string(), "missing-skill".to_string()],
                components: vec!["example".to_string()],
                capabilities: vec!["capability".to_string()],
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Integrated,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Integration,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec!["README.md".to_string()],
                file_ownership: vec!["README.md".to_string()],
                final_markers: vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                prompt: implementation_prompt("README.md"),
            }],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "known-skill-id")
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "required-closure-agent")
        );
    }

    #[test]
    fn flags_overlapping_file_ownership() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/02.md"),
            metadata: WaveMetadata {
                id: 2,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
            },
            heading_title: Some("Wave 2 - Example".to_string()),
            commit_message: Some("Feat: overlap".to_string()),
            component_promotions: vec![wave_spec::ComponentPromotion {
                component: "example".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some("Reducer context".to_string()),
            }),
            agents: vec![
                closure_agent(
                    "A0",
                    "docs/agents/wave-cont-qa-role.md",
                    "[wave-gate]",
                    ".wave/reviews/wave-2.md",
                ),
                closure_agent(
                    "A8",
                    "docs/agents/wave-integration-role.md",
                    "[wave-integration]",
                    ".wave/integration/wave-2.md",
                ),
                closure_agent(
                    "A9",
                    "docs/agents/wave-documentation-role.md",
                    "[wave-doc-closure]",
                    ".wave/docs/wave-2.md",
                ),
                WaveAgent {
                    id: "A1".to_string(),
                    title: "Implementation".to_string(),
                    role_prompts: Vec::new(),
                    executor: BTreeMap::from([(
                        "profile".to_string(),
                        "implement-codex".to_string(),
                    )]),
                    context7: Some(Context7Defaults {
                        bundle: "rust-control-plane".to_string(),
                        query: Some("Implementation context".to_string()),
                    }),
                    skills: vec!["wave-core".to_string()],
                    components: vec!["example".to_string()],
                    capabilities: vec!["capability".to_string()],
                    exit_contract: Some(ExitContract {
                        completion: CompletionLevel::Integrated,
                        durability: DurabilityLevel::Durable,
                        proof: ProofLevel::Integration,
                        doc_impact: DocImpact::Owned,
                    }),
                    deliverables: vec!["crates/wave-control-plane/src/lib.rs".to_string()],
                    file_ownership: vec!["crates/wave-control-plane/".to_string()],
                    final_markers: vec![
                        "[wave-proof]".to_string(),
                        "[wave-doc-delta]".to_string(),
                        "[wave-component]".to_string(),
                    ],
                    prompt: implementation_prompt("crates/wave-control-plane/"),
                },
                WaveAgent {
                    id: "A2".to_string(),
                    title: "Implementation".to_string(),
                    role_prompts: Vec::new(),
                    executor: BTreeMap::from([(
                        "profile".to_string(),
                        "implement-codex".to_string(),
                    )]),
                    context7: Some(Context7Defaults {
                        bundle: "rust-control-plane".to_string(),
                        query: Some("Implementation context".to_string()),
                    }),
                    skills: vec!["wave-core".to_string()],
                    components: vec!["example".to_string()],
                    capabilities: vec!["capability".to_string()],
                    exit_contract: Some(ExitContract {
                        completion: CompletionLevel::Integrated,
                        durability: DurabilityLevel::Durable,
                        proof: ProofLevel::Integration,
                        doc_impact: DocImpact::Owned,
                    }),
                    deliverables: vec!["crates/wave-control-plane/src/lib.rs".to_string()],
                    file_ownership: vec!["crates/wave-control-plane/src/lib.rs".to_string()],
                    final_markers: vec![
                        "[wave-proof]".to_string(),
                        "[wave-doc-delta]".to_string(),
                        "[wave-component]".to_string(),
                    ],
                    prompt: implementation_prompt("crates/wave-control-plane/src/lib.rs"),
                },
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "file-ownership-overlap")
        );
    }

    #[test]
    fn validates_skill_catalog() {
        let issues = validate_skill_catalog(&workspace_root());
        assert!(issues.is_empty(), "skill catalog issues: {issues:#?}");
    }

    #[test]
    fn flags_weak_prompt_contracts_and_unexpected_markers() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/03.md"),
            metadata: WaveMetadata {
                id: 3,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A1".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
            },
            heading_title: Some("Wave 3 - Example".to_string()),
            commit_message: Some("Feat: prompt contract".to_string()),
            component_promotions: vec![wave_spec::ComponentPromotion {
                component: "example".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-config-spec".to_string(),
                query: Some("Prompt contract context".to_string()),
            }),
            agents: vec![
                closure_agent(
                    "A0",
                    "docs/agents/wave-cont-qa-role.md",
                    "[wave-gate]",
                    ".wave/reviews/wave-3.md",
                ),
                closure_agent(
                    "A8",
                    "docs/agents/wave-integration-role.md",
                    "[wave-integration]",
                    ".wave/integration/wave-3.md",
                ),
                closure_agent(
                    "A9",
                    "docs/agents/wave-documentation-role.md",
                    "[wave-doc-closure]",
                    ".wave/docs/wave-3.md",
                ),
                WaveAgent {
                    id: "A1".to_string(),
                    title: "Implementation".to_string(),
                    role_prompts: Vec::new(),
                    executor: BTreeMap::from([(
                        "profile".to_string(),
                        "implement-codex".to_string(),
                    )]),
                    context7: Some(Context7Defaults {
                        bundle: "rust-config-spec".to_string(),
                        query: Some("Implementation context".to_string()),
                    }),
                    skills: vec!["wave-core".to_string()],
                    components: vec!["example".to_string()],
                    capabilities: vec!["capability".to_string()],
                    exit_contract: Some(ExitContract {
                        completion: CompletionLevel::Integrated,
                        durability: DurabilityLevel::Durable,
                        proof: ProofLevel::Integration,
                        doc_impact: DocImpact::Owned,
                    }),
                    deliverables: vec![
                        "crates/wave-dark-factory/src/lib.rs".to_string(),
                        "crates/wave-spec/src/lib.rs".to_string(),
                    ],
                    file_ownership: vec![
                        "crates/wave-dark-factory/src/lib.rs".to_string(),
                        "crates/wave-spec/src/lib.rs".to_string(),
                    ],
                    final_markers: vec![
                        "[wave-proof]".to_string(),
                        "[wave-doc-delta]".to_string(),
                        "[wave-component]".to_string(),
                        "[wave-gate]".to_string(),
                    ],
                    prompt: [
                        "Primary goal:",
                        "- Ship the implementation slice.",
                        "",
                        "Required context before coding:",
                        "- Read README.md.",
                        "",
                        "File ownership (only touch these paths):",
                        "- crates/wave-dark-factory/src/lib.rs",
                    ]
                    .join("\n"),
                },
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "agent-prompt-section-required")
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "agent-prompt-owned-path-missing")
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "agent-prompt-final-marker-required")
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "agent-final-marker-unexpected")
        );
    }

    #[test]
    fn flags_unknown_context7_bundles() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/04.md"),
            metadata: WaveMetadata {
                id: 4,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A1".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
            },
            heading_title: Some("Wave 4 - Context7".to_string()),
            commit_message: Some("Feat: context7 contract".to_string()),
            component_promotions: vec![wave_spec::ComponentPromotion {
                component: "example".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "missing-wave-bundle".to_string(),
                query: Some("Prompt contract context".to_string()),
            }),
            agents: vec![
                closure_agent(
                    "A0",
                    "docs/agents/wave-cont-qa-role.md",
                    "[wave-gate]",
                    ".wave/reviews/wave-4.md",
                ),
                closure_agent(
                    "A8",
                    "docs/agents/wave-integration-role.md",
                    "[wave-integration]",
                    ".wave/integration/wave-4.md",
                ),
                closure_agent(
                    "A9",
                    "docs/agents/wave-documentation-role.md",
                    "[wave-doc-closure]",
                    ".wave/docs/wave-4.md",
                ),
                WaveAgent {
                    id: "A1".to_string(),
                    title: "Implementation".to_string(),
                    role_prompts: Vec::new(),
                    executor: BTreeMap::from([(
                        "profile".to_string(),
                        "implement-codex".to_string(),
                    )]),
                    context7: Some(Context7Defaults {
                        bundle: "missing-agent-bundle".to_string(),
                        query: Some("Implementation context".to_string()),
                    }),
                    skills: vec!["wave-core".to_string()],
                    components: vec!["example".to_string()],
                    capabilities: vec!["capability".to_string()],
                    exit_contract: Some(ExitContract {
                        completion: CompletionLevel::Integrated,
                        durability: DurabilityLevel::Durable,
                        proof: ProofLevel::Integration,
                        doc_impact: DocImpact::Owned,
                    }),
                    deliverables: vec!["README.md".to_string()],
                    file_ownership: vec!["README.md".to_string()],
                    final_markers: vec![
                        "[wave-proof]".to_string(),
                        "[wave-doc-delta]".to_string(),
                        "[wave-component]".to_string(),
                    ],
                    prompt: implementation_prompt("README.md"),
                },
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(findings.iter().any(|finding| {
            finding.rule == "context7-bundle-known"
                && finding.message == "wave references unknown Context7 bundle missing-wave-bundle"
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "context7-bundle-known"
                && finding.message
                    == "agent A1 references unknown Context7 bundle missing-agent-bundle"
        }));
    }

    #[test]
    fn invalid_skill_manifests_do_not_become_known_skill_ids() {
        let root = temp_workspace_root();
        let skills_dir = root.join("skills");
        fs::create_dir_all(&skills_dir).expect("create skills dir");

        write_file(
            &skills_dir.join("valid-skill").join("skill.json"),
            r#"{
  "id": "valid-skill",
  "title": "Valid Skill",
  "description": "A valid skill bundle.",
  "activation": {
    "when": "Attach when the test needs a valid skill."
  }
}"#,
        );
        write_file(
            &skills_dir.join("valid-skill").join("SKILL.md"),
            "# Valid\n",
        );

        write_file(
            &skills_dir.join("invalid-skill").join("skill.json"),
            r#"{
  "id": "invalid-skill",
  "title": "",
  "description": "Missing a title on purpose.",
  "activation": {
    "when": "Attach when the test needs an invalid skill."
  }
}"#,
        );
        write_file(
            &skills_dir.join("invalid-skill").join("SKILL.md"),
            "# Invalid\n",
        );

        let issues = validate_skill_catalog(&root);
        assert!(issues.iter().any(|issue| {
            issue.path.ends_with("invalid-skill/skill.json")
                && issue.message == "skill manifest title must not be empty"
        }));

        let ids = discover_skill_ids(&root);
        assert!(ids.contains("valid-skill"));
        assert!(!ids.contains("invalid-skill"));

        let _ = fs::remove_dir_all(root);
    }

    fn closure_agent(id: &str, prompt_path: &str, marker: &str, owned_path: &str) -> WaveAgent {
        WaveAgent {
            id: id.to_string(),
            title: "Closure".to_string(),
            role_prompts: vec![prompt_path.to_string()],
            executor: BTreeMap::from([("profile".to_string(), "review-codex".to_string())]),
            context7: Some(Context7Defaults {
                bundle: "none".to_string(),
                query: Some("Repository docs remain canonical".to_string()),
            }),
            skills: Vec::new(),
            components: Vec::new(),
            capabilities: Vec::new(),
            exit_contract: None,
            deliverables: Vec::new(),
            file_ownership: vec![owned_path.to_string()],
            final_markers: vec![marker.to_string()],
            prompt: [
                "Primary goal:",
                "- Close the wave honestly.",
                "",
                "Required context before coding:",
                "- Read README.md.",
                "",
                "Specific expectations:",
                &format!("- Emit the final {marker} marker as a plain last line."),
                "",
                "File ownership (only touch these paths):",
                &format!("- {owned_path}"),
            ]
            .join("\n"),
        }
    }

    fn implementation_prompt(owned_path: &str) -> String {
        [
            "Primary goal:",
            "- Ship the implementation slice.",
            "",
            "Required context before coding:",
            "- Read README.md.",
            "",
            "Specific expectations:",
            "- Emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output.",
            "",
            "File ownership (only touch these paths):",
            &format!("- {owned_path}"),
        ]
        .join("\n")
    }

    fn temp_workspace_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("unix timestamp")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "wave-dark-factory-test-{}-{unique}",
            std::process::id()
        ))
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, contents).expect("write file");
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root")
    }
}
