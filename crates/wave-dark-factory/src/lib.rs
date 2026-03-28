use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use wave_spec::WaveAgent;
use wave_spec::WaveClass;
use wave_spec::WaveDocument;

const REQUIRED_PROMPT_SECTIONS: [&str; 4] = [
    "Primary goal",
    "Required context before coding",
    "Specific expectations",
    "File ownership (only touch these paths)",
];
const CONTEXT7_NONE_BUNDLE: &str = "none";
const FILE_OWNERSHIP_PROMPT_SECTION: &str = "File ownership (only touch these paths)";
const SPECIFIC_EXPECTATIONS_PROMPT_SECTION: &str = "Specific expectations";

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Context7CatalogIssue {
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
#[serde(rename_all = "camelCase")]
struct Context7BundleCatalog {
    #[serde(default)]
    version: Option<u32>,
    #[serde(default)]
    default_bundle: Option<String>,
    #[serde(default)]
    lane_defaults: BTreeMap<String, String>,
    bundles: BTreeMap<String, Context7BundleSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Context7BundleSpec {
    description: String,
    libraries: Vec<Context7LibrarySpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Context7LibrarySpec {
    #[serde(default)]
    library_id: Option<String>,
    library_name: String,
    query_hint: String,
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

        if wave.metadata.slug.trim().is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "wave-slug-required",
                message: "wave front matter must declare a non-empty slug".to_string(),
            });
        }

        if wave.metadata.title.trim().is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "wave-title-required",
                message: "wave front matter must declare a non-empty title".to_string(),
            });
        }

        if !has_non_empty_items(&wave.metadata.owners) {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "wave-owners-required",
                message: "wave front matter must declare at least one non-empty owner".to_string(),
            });
        }

        if wave.metadata.wave_class != WaveClass::Implementation && wave.metadata.intent.is_none() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "wave-intent-required",
                message: "non-implementation waves must declare an intent".to_string(),
            });
        }

        if wave.metadata.delivery.is_some() && wave.metadata.intent.is_none() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "delivery-intent-required",
                message: "delivery-aware waves must declare an intent".to_string(),
            });
        }

        if let Some(delivery) = wave.metadata.delivery.as_ref() {
            if delivery
                .initiative_id
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
                && delivery
                    .release_id
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
                && delivery
                    .acceptance_package_id
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
            {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "delivery-link-required",
                    message: "delivery-aware waves must link at least one initiative, release, or acceptance package id".to_string(),
                });
            }
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
        } else {
            let mut seen_environment_names = BTreeSet::new();
            for environment in &wave.deploy_environments {
                if environment.name.trim().is_empty() {
                    findings.push(LintFinding {
                        wave_id: wave.metadata.id,
                        severity: FindingSeverity::Error,
                        rule: "deploy-environment-name-required",
                        message: "deploy environment name must not be empty".to_string(),
                    });
                } else if !seen_environment_names.insert(environment.name.trim().to_string()) {
                    findings.push(LintFinding {
                        wave_id: wave.metadata.id,
                        severity: FindingSeverity::Error,
                        rule: "deploy-environment-name-duplicate",
                        message: format!(
                            "wave declares deploy environment {} more than once",
                            environment.name.trim()
                        ),
                    });
                }

                if environment.detail.trim().is_empty() {
                    findings.push(LintFinding {
                        wave_id: wave.metadata.id,
                        severity: FindingSeverity::Error,
                        rule: "deploy-environment-detail-required",
                        message: format!(
                            "deploy environment {} must declare a non-empty detail",
                            environment.name.trim()
                        ),
                    });
                }
            }
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

            if context7.bundle.trim() == CONTEXT7_NONE_BUNDLE {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "context7-default-bundle-weak",
                    message: "wave Context7 defaults must not use the none bundle".to_string(),
                });
            }

            lint_context7_query_strength(
                wave.metadata.id,
                "wave",
                &context7.bundle,
                context7.query.as_deref(),
                &mut findings,
            );
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
        } else {
            lint_machine_readable_command_list(
                wave.metadata.id,
                "validation",
                &wave.metadata.validation,
                &mut findings,
            );
        }

        if wave.metadata.rollback.is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "dark-factory-rollback",
                message: "dark-factory wave is missing rollback guidance".to_string(),
            });
        } else {
            lint_machine_readable_command_list(
                wave.metadata.id,
                "rollback",
                &wave.metadata.rollback,
                &mut findings,
            );
        }

        if wave.metadata.proof.is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "dark-factory-proof",
                message: "dark-factory wave is missing proof artifacts".to_string(),
            });
        } else {
            lint_machine_readable_command_list(
                wave.metadata.id,
                "proof",
                &wave.metadata.proof,
                &mut findings,
            );
        }

        if wave.implementation_agents().next().is_none() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "implementation-agent-required",
                message: "wave must declare at least one implementation agent".to_string(),
            });
        }

        if wave.metadata.wave_class == WaveClass::Implementation
            && wave.code_implementation_agents().next().is_none()
        {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "code-implementation-agent-required",
                message: "implementation waves must declare at least one non-design code agent"
                    .to_string(),
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

        lint_design_gate(wave, &mut findings);
        lint_multi_agent_contract(wave, &mut findings);

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

pub fn validate_context7_bundle_catalog(root: &Path) -> Vec<Context7CatalogIssue> {
    match load_context7_bundle_catalog(root) {
        Ok((issues, _)) => issues,
        Err(message) => vec![Context7CatalogIssue {
            path: root
                .join("docs/context7/bundles.json")
                .display()
                .to_string(),
            message,
        }],
    }
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
        let mut seen_owned_paths = BTreeSet::new();
        for path in &agent.file_ownership {
            let normalized = normalize_owned_path(path);
            if !seen_owned_paths.insert(normalized.clone()) {
                findings.push(LintFinding {
                    wave_id,
                    severity: FindingSeverity::Error,
                    rule: "agent-file-ownership-duplicate",
                    message: format!(
                        "agent {} declares owned path {} more than once",
                        agent.id, normalized
                    ),
                });
            }
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

    let mut seen_skill_ids = HashSet::new();
    for skill_id in &agent.skills {
        if !seen_skill_ids.insert(skill_id.as_str()) {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "agent-skill-duplicate",
                message: format!(
                    "agent {} references skill {} more than once",
                    agent.id, skill_id
                ),
            });
        }

        if !known_skill_ids.contains(skill_id) {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "known-skill-id",
                message: format!("agent {} references unknown skill {}", agent.id, skill_id),
            });
        }
    }

    if agent.skills.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-skills-required",
            message: format!("agent {} must declare skills", agent.id),
        });
    }

    if agent.is_closure_agent() {
        lint_closure_agent_contract(wave_id, agent, findings);
        return;
    }

    if !agent.role_prompts.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "implementation-role-prompts-unexpected",
            message: format!(
                "implementation agent {} must not declare closure role prompts",
                agent.id
            ),
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
    } else {
        let mut seen_deliverables = BTreeSet::new();
        for deliverable in &agent.deliverables {
            let normalized = normalize_owned_path(deliverable);
            if !seen_deliverables.insert(normalized.clone()) {
                findings.push(LintFinding {
                    wave_id,
                    severity: FindingSeverity::Error,
                    rule: "agent-deliverable-duplicate",
                    message: format!(
                        "implementation agent {} declares deliverable {} more than once",
                        agent.id, normalized
                    ),
                });
            }

            if !agent.owns_path(deliverable) {
                findings.push(LintFinding {
                    wave_id,
                    severity: FindingSeverity::Error,
                    rule: "agent-deliverable-owned-path-required",
                    message: format!(
                        "implementation agent {} declares deliverable {} outside its owned paths",
                        agent.id, normalized
                    ),
                });
            }
        }
    }
}

fn lint_role_prompts(
    root: &Path,
    wave_id: u32,
    agent: &WaveAgent,
    findings: &mut Vec<LintFinding>,
) {
    if agent.is_closure_agent() && agent.role_prompts.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "closure-role-prompts-required",
            message: format!("closure agent {} must declare role prompts", agent.id),
        });
    }

    let expected_role_prompts = agent.expected_role_prompts();
    if agent.is_closure_agent() && agent.role_prompts.len() != expected_role_prompts.len() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "closure-role-prompts-exact",
            message: format!(
                "closure agent {} must declare exactly the expected role prompts",
                agent.id
            ),
        });
    }

    for required_prompt in expected_role_prompts {
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

    if agent.is_closure_agent() {
        for role_prompt in &agent.role_prompts {
            if !expected_role_prompts
                .iter()
                .any(|expected| expected == role_prompt)
            {
                findings.push(LintFinding {
                    wave_id,
                    severity: FindingSeverity::Error,
                    rule: "closure-role-prompt-unexpected",
                    message: format!(
                        "closure agent {} must not declare unexpected role prompt {}",
                        agent.id, role_prompt
                    ),
                });
            }
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

fn lint_closure_agent_contract(wave_id: u32, agent: &WaveAgent, findings: &mut Vec<LintFinding>) {
    if !agent.components.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "closure-agent-components-unexpected",
            message: format!(
                "closure agent {} must not declare implementation components",
                agent.id
            ),
        });
    }

    if !agent.capabilities.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "closure-agent-capabilities-unexpected",
            message: format!(
                "closure agent {} must not declare implementation capabilities",
                agent.id
            ),
        });
    }

    if agent.exit_contract.is_some() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "closure-agent-exit-contract-unexpected",
            message: format!(
                "closure agent {} must not declare an implementation exit contract",
                agent.id
            ),
        });
    }

    if !agent.deliverables.is_empty() {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "closure-agent-deliverables-unexpected",
            message: format!(
                "closure agent {} must not declare implementation deliverables",
                agent.id
            ),
        });
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

    if !agent.is_closure_agent() && context7.bundle.trim() == CONTEXT7_NONE_BUNDLE {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-context7-bundle-weak",
            message: format!(
                "agent {} Context7 bundle must not be the none bundle",
                agent.id
            ),
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

    lint_context7_query_strength(
        wave_id,
        &format!("agent {}", agent.id),
        &context7.bundle,
        context7.query.as_deref(),
        findings,
    );
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

    let prompt_contract = agent.prompt_contract();
    let prompt_headings = prompt_contract
        .sections
        .iter()
        .map(|section| section.heading.as_str())
        .collect::<Vec<_>>();
    if prompt_headings != REQUIRED_PROMPT_SECTIONS {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-prompt-section-order-required",
            message: format!(
                "agent {} prompt must declare the required sections in order",
                agent.id
            ),
        });
    }

    let mut seen_sections = HashSet::new();
    for section in &prompt_contract.sections {
        let section_key = section.heading.to_ascii_lowercase();
        if !seen_sections.insert(section_key) {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "agent-prompt-section-duplicate",
                message: format!(
                    "agent {} prompt declares section {} more than once",
                    agent.id, section.heading
                ),
            });
        }

        if !REQUIRED_PROMPT_SECTIONS
            .iter()
            .any(|required| section.heading.eq_ignore_ascii_case(required))
        {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "agent-prompt-section-unexpected",
                message: format!(
                    "agent {} prompt declares unexpected section {}",
                    agent.id, section.heading
                ),
            });
        }
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

    let specific_expectations = agent
        .prompt_section_text(SPECIFIC_EXPECTATIONS_PROMPT_SECTION)
        .unwrap_or_default();
    for expected_marker in agent.expected_final_markers() {
        if !specific_expectations.contains(expected_marker) {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "agent-prompt-final-marker-required",
                message: format!(
                    "agent {} prompt must mention final marker {} inside Specific expectations",
                    agent.id, expected_marker
                ),
            });
        }
    }

    if !has_plain_line_marker_instruction(&specific_expectations) {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "agent-prompt-final-marker-format-required",
            message: format!(
                "agent {} prompt must require final markers on plain lines or a plain last line",
                agent.id
            ),
        });
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
    let mut seen_prompt_paths = BTreeSet::new();
    for prompt_path in prompt_owned_paths {
        let normalized = normalize_owned_path(prompt_path);
        if !seen_prompt_paths.insert(normalized.clone()) {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "agent-prompt-owned-path-duplicate",
                message: format!(
                    "agent {} prompt declares owned path {} more than once",
                    agent.id, normalized
                ),
            });
        }
    }

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

fn has_plain_line_marker_instruction(section: &str) -> bool {
    let section = section.to_ascii_lowercase();
    section.contains("plain")
        && (section.contains("last line") || section.contains("lines by themselves"))
}

fn lint_machine_readable_command_list(
    wave_id: u32,
    list_name: &str,
    commands: &[String],
    findings: &mut Vec<LintFinding>,
) {
    let mut seen_commands = BTreeSet::new();
    for (index, command) in commands.iter().enumerate() {
        let normalized = command.trim();
        if normalized.is_empty() {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "dark-factory-command-empty",
                message: format!("{list_name} entry {} must not be empty", index + 1),
            });
            continue;
        }

        if !seen_commands.insert(normalized.to_string()) {
            findings.push(LintFinding {
                wave_id,
                severity: FindingSeverity::Error,
                rule: "dark-factory-command-duplicate",
                message: format!(
                    "{list_name} entry {} is duplicated: {}",
                    index + 1,
                    normalized
                ),
            });
        }
    }
}

fn has_non_empty_items(items: &[String]) -> bool {
    items.iter().any(|item| !item.trim().is_empty())
}

fn normalized_owned_path_set(paths: &[String]) -> BTreeSet<String> {
    paths
        .iter()
        .map(|path| normalize_owned_path(path))
        .collect()
}

fn lint_design_gate(wave: &WaveDocument, findings: &mut Vec<LintFinding>) {
    let Some(design_gate) = wave.metadata.design_gate.as_ref() else {
        return;
    };

    if wave.metadata.wave_class != WaveClass::Implementation {
        findings.push(LintFinding {
            wave_id: wave.metadata.id,
            severity: FindingSeverity::Error,
            rule: "design-gate-wave-class",
            message: "design gates are only supported on implementation waves".to_string(),
        });
    }

    if design_gate.agent_ids.is_empty() {
        findings.push(LintFinding {
            wave_id: wave.metadata.id,
            severity: FindingSeverity::Error,
            rule: "design-gate-agents-required",
            message: "design gates must list at least one non-closure design worker".to_string(),
        });
    }

    if design_gate.ready_marker.trim().is_empty() {
        findings.push(LintFinding {
            wave_id: wave.metadata.id,
            severity: FindingSeverity::Error,
            rule: "design-gate-ready-marker-required",
            message: "design gates must declare a non-empty ready marker".to_string(),
        });
    }

    let mut seen_agent_ids = HashSet::new();
    for agent_id in &design_gate.agent_ids {
        let normalized = agent_id.trim();
        if normalized.is_empty() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "design-gate-agent-id-required",
                message: "design gate agent ids must not be empty".to_string(),
            });
            continue;
        }

        if !seen_agent_ids.insert(normalized.to_string()) {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "design-gate-agent-id-duplicate",
                message: format!("design gate agent {} is listed more than once", normalized),
            });
            continue;
        }

        let Some(agent) = wave.agents.iter().find(|agent| agent.id == normalized) else {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "design-gate-agent-known",
                message: format!("design gate references unknown agent {}", normalized),
            });
            continue;
        };

        if agent.id == "A6" || agent.is_closure_agent() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "design-gate-agent-closure",
                message: format!(
                    "design gate agent {} must be a pre-implementation design worker, not a closure reviewer",
                    normalized
                ),
            });
        }

        if !agent.is_design_worker() {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "design-gate-agent-role",
                message: format!(
                    "design gate agent {} must declare the role-design skill",
                    normalized
                ),
            });
        }
    }
}

fn lint_multi_agent_contract(wave: &WaveDocument, findings: &mut Vec<LintFinding>) {
    if !wave.is_multi_agent() {
        return;
    }

    let known_agent_ids = wave
        .agents
        .iter()
        .map(|agent| agent.id.as_str())
        .collect::<HashSet<_>>();
    let mut artifact_writers = BTreeMap::<String, Vec<String>>::new();
    for agent in &wave.agents {
        let mut seen_writes = BTreeSet::new();
        for artifact in &agent.writes_artifacts {
            let normalized = artifact.trim();
            if normalized.is_empty() {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-writes-artifact-required",
                    message: format!("agent {} declares an empty written artifact", agent.id),
                });
                continue;
            }
            if !seen_writes.insert(normalized.to_string()) {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-writes-artifact-duplicate",
                    message: format!(
                        "agent {} declares written artifact {} more than once",
                        agent.id, normalized
                    ),
                });
            }
            artifact_writers
                .entry(normalized.to_string())
                .or_default()
                .push(agent.id.clone());
        }

        let mut seen_dependencies = BTreeSet::new();
        for dependency_agent_id in &agent.depends_on_agents {
            let normalized = dependency_agent_id.trim();
            if normalized.is_empty() {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-depends-on-required",
                    message: format!("agent {} declares an empty MAS dependency", agent.id),
                });
                continue;
            }
            if !seen_dependencies.insert(normalized.to_string()) {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-depends-on-duplicate",
                    message: format!(
                        "agent {} declares MAS dependency {} more than once",
                        agent.id, normalized
                    ),
                });
                continue;
            }
            if normalized == agent.id {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-depends-on-self",
                    message: format!("agent {} must not depend on itself", agent.id),
                });
                continue;
            }
            if !known_agent_ids.contains(normalized) {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-depends-on-known",
                    message: format!("agent {} depends on unknown agent {}", agent.id, normalized),
                });
            }
        }

        let mut seen_reads = BTreeSet::new();
        for artifact in &agent.reads_artifacts_from {
            let normalized = artifact.trim();
            if normalized.is_empty() {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-reads-artifact-required",
                    message: format!("agent {} declares an empty read artifact", agent.id),
                });
                continue;
            }
            if !seen_reads.insert(normalized.to_string()) {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-reads-artifact-duplicate",
                    message: format!(
                        "agent {} declares read artifact {} more than once",
                        agent.id, normalized
                    ),
                });
            }
        }

        let mut seen_resources = BTreeSet::new();
        for resource in &agent.exclusive_resources {
            let normalized = resource.trim();
            if normalized.is_empty() {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-exclusive-resource-required",
                    message: format!("agent {} declares an empty exclusive resource", agent.id),
                });
                continue;
            }
            if !seen_resources.insert(normalized.to_string()) {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-exclusive-resource-duplicate",
                    message: format!(
                        "agent {} declares exclusive resource {} more than once",
                        agent.id, normalized
                    ),
                });
            }
        }

        let mut seen_parallel_with = BTreeSet::new();
        for peer_id in &agent.parallel_with {
            let normalized = peer_id.trim();
            if normalized.is_empty() {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-parallel-with-required",
                    message: format!("agent {} declares an empty parallel_with entry", agent.id),
                });
                continue;
            }
            if !seen_parallel_with.insert(normalized.to_string()) {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-parallel-with-duplicate",
                    message: format!(
                        "agent {} declares parallel_with {} more than once",
                        agent.id, normalized
                    ),
                });
                continue;
            }
            if normalized == agent.id {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-parallel-with-self",
                    message: format!("agent {} must not list itself in parallel_with", agent.id),
                });
                continue;
            }
            if !known_agent_ids.contains(normalized) {
                findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-parallel-with-known",
                    message: format!(
                        "agent {} parallel_with references unknown agent {}",
                        agent.id, normalized
                    ),
                });
            }
        }
    }

    for (artifact, writers) in &artifact_writers {
        if writers.len() > 1 {
            findings.push(LintFinding {
                wave_id: wave.metadata.id,
                severity: FindingSeverity::Error,
                rule: "agent-writes-artifact-ambiguous",
                message: format!(
                    "artifact {} is written by multiple agents: {}",
                    artifact,
                    writers.join(", ")
                ),
            });
        }
    }

    for agent in &wave.agents {
        for artifact in &agent.reads_artifacts_from {
            let normalized = artifact.trim();
            if normalized.is_empty() {
                continue;
            }
            match artifact_writers.get(normalized) {
                Some(writers) if writers.len() == 1 => {}
                Some(_) => {}
                None => findings.push(LintFinding {
                    wave_id: wave.metadata.id,
                    severity: FindingSeverity::Error,
                    rule: "agent-reads-artifact-known",
                    message: format!(
                        "agent {} reads artifact {} but no agent writes it",
                        agent.id, normalized
                    ),
                }),
            }
        }
    }

    let cycle = wave_spec::compiled_multi_agent_dependency_cycle(wave);
    if !cycle.is_empty() {
        findings.push(LintFinding {
            wave_id: wave.metadata.id,
            severity: FindingSeverity::Error,
            rule: "agent-dependency-cycle",
            message: format!(
                "multi-agent dependency cycle detected: {}",
                cycle.join(" -> ")
            ),
        });
    }
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
    load_context7_bundle_catalog(root)
        .map(|(_, ids)| ids)
        .unwrap_or_default()
}

fn load_context7_bundle_catalog(
    root: &Path,
) -> Result<(Vec<Context7CatalogIssue>, HashSet<String>), String> {
    let path = root.join("docs/context7/bundles.json");
    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let catalog = serde_json::from_str::<Context7BundleCatalog>(&raw)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    let mut issues = Vec::new();
    let mut ids = HashSet::new();

    if catalog.bundles.is_empty() {
        issues.push(Context7CatalogIssue {
            path: path.display().to_string(),
            message: "no Context7 bundles were discovered".to_string(),
        });
    }

    if catalog.version != Some(1) {
        issues.push(Context7CatalogIssue {
            path: path.display().to_string(),
            message: format!(
                "Context7 bundle catalog must declare version 1, found {:?}",
                catalog.version
            ),
        });
    }

    if let Some(default_bundle) = catalog.default_bundle.as_deref() {
        let default_bundle = default_bundle.trim();
        if default_bundle.is_empty() {
            issues.push(Context7CatalogIssue {
                path: path.display().to_string(),
                message: "default Context7 bundle must not be empty".to_string(),
            });
        } else if default_bundle == CONTEXT7_NONE_BUNDLE {
            issues.push(Context7CatalogIssue {
                path: path.display().to_string(),
                message: "default Context7 bundle must not be the none bundle".to_string(),
            });
        } else if !catalog.bundles.contains_key(default_bundle) {
            issues.push(Context7CatalogIssue {
                path: path.display().to_string(),
                message: format!("default Context7 bundle {} is not declared", default_bundle),
            });
        }
    }

    for (lane, bundle) in &catalog.lane_defaults {
        let bundle = bundle.trim();
        if bundle.is_empty() {
            issues.push(Context7CatalogIssue {
                path: path.display().to_string(),
                message: format!("lane default {} must not be empty", lane),
            });
            continue;
        }

        if bundle == CONTEXT7_NONE_BUNDLE {
            issues.push(Context7CatalogIssue {
                path: path.display().to_string(),
                message: format!("lane default {} must not use the none bundle", lane),
            });
            continue;
        }

        if !catalog.bundles.contains_key(bundle) {
            issues.push(Context7CatalogIssue {
                path: path.display().to_string(),
                message: format!(
                    "lane default {} references unknown Context7 bundle {}",
                    lane, bundle
                ),
            });
        }
    }

    for (bundle_id, bundle_spec) in catalog.bundles {
        let bundle_name = bundle_id.trim();
        let bundle_path = format!("{}#{}", path.display(), bundle_name);
        if bundle_name.is_empty() {
            issues.push(Context7CatalogIssue {
                path: bundle_path,
                message: "Context7 bundle id must not be empty".to_string(),
            });
            continue;
        }

        if manifest_field(Some(bundle_spec.description.as_str())).is_none() {
            issues.push(Context7CatalogIssue {
                path: bundle_path.clone(),
                message: format!("Context7 bundle {} must declare a description", bundle_name),
            });
        }

        if bundle_name != "none" && bundle_spec.libraries.is_empty() {
            issues.push(Context7CatalogIssue {
                path: bundle_path.clone(),
                message: format!(
                    "Context7 bundle {} must declare at least one library",
                    bundle_name
                ),
            });
        }

        for library in bundle_spec.libraries {
            let library_path = format!(
                "{}#{}:{}",
                path.display(),
                bundle_name,
                library.library_name
            );

            if manifest_field(Some(library.library_name.as_str())).is_none() {
                issues.push(Context7CatalogIssue {
                    path: library_path.clone(),
                    message: format!(
                        "Context7 library in bundle {} must declare libraryName",
                        bundle_name
                    ),
                });
            }

            if manifest_field(Some(library.query_hint.as_str())).is_none() {
                issues.push(Context7CatalogIssue {
                    path: library_path.clone(),
                    message: format!(
                        "Context7 library in bundle {} must declare queryHint",
                        bundle_name
                    ),
                });
            } else if is_weak_context7_query(&library.query_hint) {
                issues.push(Context7CatalogIssue {
                    path: library_path.clone(),
                    message: format!(
                        "Context7 library query hint in bundle {} is too broad: {}",
                        bundle_name, library.query_hint
                    ),
                });
            }

            if let Some(library_id) = library.library_id.as_deref() {
                if manifest_field(Some(library_id)).is_none() {
                    issues.push(Context7CatalogIssue {
                        path: library_path,
                        message: format!(
                            "Context7 library in bundle {} must not declare an empty libraryId",
                            bundle_name
                        ),
                    });
                }
            }
        }

        ids.insert(bundle_name.to_string());
    }

    Ok((issues, ids))
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

fn lint_context7_query_strength(
    wave_id: u32,
    scope: &str,
    bundle: &str,
    query: Option<&str>,
    findings: &mut Vec<LintFinding>,
) {
    let bundle = bundle.trim();
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return;
    };

    if bundle == "none" {
        return;
    }

    if is_weak_context7_query(query) {
        findings.push(LintFinding {
            wave_id,
            severity: FindingSeverity::Error,
            rule: "context7-query-too-weak",
            message: format!("{scope} Context7 query is too broad: {query}"),
        });
    }
}

fn is_weak_context7_query(query: &str) -> bool {
    let normalized = query.trim().to_ascii_lowercase();
    if normalized.len() < 32 {
        return true;
    }

    let weak_phrases = [
        "context7 query",
        "implementation context",
        "reducer context",
        "prompt contract context",
        "ownership enforcement context",
        "closure skills context",
        "closure-only authored-wave contract",
        "repository docs remain canonical",
        "docs remain canonical",
        "shared-plan documentation only",
        "marker instructions must mention plain-line output",
    ];

    if weak_phrases
        .iter()
        .any(|phrase| normalized == *phrase || normalized.contains(phrase))
    {
        return true;
    }

    normalized.split_whitespace().count() < 6
}

fn ownership_conflict(left: &str, right: &str) -> bool {
    wave_spec::owned_path_conflict(left, right)
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

    fn default_wave_metadata() -> WaveMetadata {
        WaveMetadata {
            id: 0,
            slug: String::new(),
            title: String::new(),
            mode: ExecutionMode::DarkFactory,
            owners: Vec::new(),
            depends_on: Vec::new(),
            validation: Vec::new(),
            rollback: Vec::new(),
            proof: Vec::new(),
            wave_class: wave_spec::WaveClass::Implementation,
            intent: None,
            delivery: None,
            design_gate: None,
            execution_model: wave_spec::WaveExecutionModel::Serial,
            concurrency_budget: wave_spec::WaveConcurrencyBudget::default(),
        }
    }

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
                ..default_wave_metadata()
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
                ..default_wave_metadata()
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
                query: Some(
                    "Serde derive, TOML parsing, and rich markdown section parsing for typed config and authored-wave enforcement".to_string(),
                ),
            }),
            agents: vec![WaveAgent {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                role_prompts: Vec::new(),
                executor: BTreeMap::from([("profile".to_string(), "implement-codex".to_string())]),
                context7: Some(Context7Defaults {
                    bundle: "rust-cli-core".to_string(),
                    query: Some(
                        "Typed config loading, authored-wave parsing, and dark-factory lint enforcement for this implementation slice".to_string(),
                    ),
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
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: Vec::new(),
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                exclusive_resources: Vec::new(),
                parallel_with: Vec::new(),
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
    fn flags_closure_agents_without_skills() {
        let mut a0 = closure_agent(
            "A0",
            "docs/agents/wave-cont-qa-role.md",
            "[wave-gate]",
            ".wave/reviews/wave-1.md",
        );
        a0.skills.clear();
        let mut a8 = closure_agent(
            "A8",
            "docs/agents/wave-integration-role.md",
            "[wave-integration]",
            ".wave/integration/wave-1.md",
        );
        a8.skills.clear();
        let mut a9 = closure_agent(
            "A9",
            "docs/agents/wave-documentation-role.md",
            "[wave-doc-closure]",
            ".wave/docs/wave-1.md",
        );
        a9.skills.clear();

        let wave = WaveDocument {
            path: PathBuf::from("waves/01.md"),
            metadata: WaveMetadata {
                id: 1,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 1 - Closure Skills".to_string()),
            commit_message: Some("Feat: closure skills".to_string()),
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
                query: Some(
                    "Closure-agent skill coverage and prompt-role validation for authored-wave closure agents".to_string(),
                ),
            }),
            agents: vec![
                a0,
                a8,
                a9,
                WaveAgent {
                    id: "A1".to_string(),
                    title: "Implementation".to_string(),
                    role_prompts: Vec::new(),
                    executor: BTreeMap::from([(
                        "profile".to_string(),
                        "implement-codex".to_string(),
                    )]),
                    context7: Some(Context7Defaults {
                        bundle: "rust-cli-core".to_string(),
                        query: Some(
                            "Typed config loading, authored-wave parsing, and dark-factory lint enforcement for this implementation slice".to_string(),
                        ),
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
                    depends_on_agents: Vec::new(),
                    reads_artifacts_from: Vec::new(),
                    writes_artifacts: Vec::new(),
                    barrier_class: wave_spec::BarrierClass::Independent,
                    parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                    exclusive_resources: Vec::new(),
                    parallel_with: Vec::new(),
                    prompt: implementation_prompt("README.md"),
                },
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-skills-required"
                && finding.message == "agent A0 must declare skills"
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-skills-required"
                && finding.message == "agent A8 must declare skills"
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-skills-required"
                && finding.message == "agent A9 must declare skills"
        }));
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
                ..default_wave_metadata()
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
                query: Some(
                    "Queue reducer state, planning projection, and control-plane contract validation".to_string(),
                ),
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
                        query: Some(
                            "Typed config loading, authored-wave parsing, and dark-factory lint enforcement for this implementation slice".to_string(),
                        ),
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
                    depends_on_agents: Vec::new(),
                    reads_artifacts_from: Vec::new(),
                    writes_artifacts: Vec::new(),
                    barrier_class: wave_spec::BarrierClass::Independent,
                    parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                    exclusive_resources: Vec::new(),
                    parallel_with: Vec::new(),
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
                        query: Some(
                            "Typed config loading, authored-wave parsing, and dark-factory lint enforcement for this implementation slice".to_string(),
                        ),
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
                    depends_on_agents: Vec::new(),
                    reads_artifacts_from: Vec::new(),
                    writes_artifacts: Vec::new(),
                    barrier_class: wave_spec::BarrierClass::Independent,
                    parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                    exclusive_resources: Vec::new(),
                    parallel_with: Vec::new(),
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
    fn flags_waves_without_implementation_agents() {
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
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 2 - Closure Only".to_string()),
            commit_message: Some("Feat: closure only".to_string()),
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
                query: Some(
                    "Closure-agent coverage and prompt structure validation for authored-wave closure-only waves".to_string(),
                ),
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
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "implementation-agent-required")
        );
    }

    #[test]
    fn validates_skill_catalog() {
        let issues = validate_skill_catalog(&workspace_root());
        assert!(issues.is_empty(), "skill catalog issues: {issues:#?}");
    }

    #[test]
    fn validates_context7_bundle_catalog() {
        let issues = validate_context7_bundle_catalog(&workspace_root());
        assert!(issues.is_empty(), "context7 catalog issues: {issues:#?}");
    }

    #[test]
    fn flags_weak_context7_catalog_defaults() {
        let root = temp_workspace_root();
        let docs_dir = root.join("docs/context7");
        fs::create_dir_all(&docs_dir).expect("create docs dir");
        write_file(
            &docs_dir.join("bundles.json"),
            r#"{
  "version": 1,
  "defaultBundle": "none",
  "laneDefaults": {
    "main": "none"
  },
  "bundles": {
    "none": {
      "description": "Disable Context7 prefetch for closure-only and docs-only review steps.",
      "libraries": []
    }
  }
}"#,
        );

        let issues = validate_context7_bundle_catalog(&root);
        assert!(issues.iter().any(|issue| {
            issue.message == "default Context7 bundle must not be the none bundle"
        }));
        assert!(
            issues
                .iter()
                .any(|issue| issue.message == "lane default main must not use the none bundle")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn flags_weak_context7_defaults() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/06.md"),
            metadata: WaveMetadata {
                id: 6,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A1".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 6 - Weak Context7".to_string()),
            commit_message: Some("Feat: weak context7".to_string()),
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
                query: Some("Implementation context".to_string()),
            }),
            agents: vec![
                closure_agent(
                    "A0",
                    "docs/agents/wave-cont-qa-role.md",
                    "[wave-gate]",
                    ".wave/reviews/wave-6.md",
                ),
                closure_agent(
                    "A8",
                    "docs/agents/wave-integration-role.md",
                    "[wave-integration]",
                    ".wave/integration/wave-6.md",
                ),
                closure_agent(
                    "A9",
                    "docs/agents/wave-documentation-role.md",
                    "[wave-doc-closure]",
                    ".wave/docs/wave-6.md",
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
                    deliverables: vec!["crates/wave-dark-factory/src/lib.rs".to_string()],
                    file_ownership: vec!["crates/wave-dark-factory/src/lib.rs".to_string()],
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
                    prompt: implementation_prompt("crates/wave-dark-factory/src/lib.rs"),
                },
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(findings.iter().any(|finding| {
            finding.rule == "context7-query-too-weak"
                && finding.message == "wave Context7 query is too broad: Implementation context"
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "context7-query-too-weak"
                && finding.message == "agent A1 Context7 query is too broad: Implementation context"
        }));
    }

    #[test]
    fn flags_none_context7_defaults_for_non_closure_agents() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/06.md"),
            metadata: WaveMetadata {
                id: 6,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A1".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 6 - None Context7".to_string()),
            commit_message: Some("Feat: none context7".to_string()),
            component_promotions: vec![wave_spec::ComponentPromotion {
                component: "example".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: CONTEXT7_NONE_BUNDLE.to_string(),
                query: Some("Typed config loading for authored-wave parsing".to_string()),
            }),
            agents: vec![
                closure_agent(
                    "A0",
                    "docs/agents/wave-cont-qa-role.md",
                    "[wave-gate]",
                    ".wave/reviews/wave-6.md",
                ),
                closure_agent(
                    "A8",
                    "docs/agents/wave-integration-role.md",
                    "[wave-integration]",
                    ".wave/integration/wave-6.md",
                ),
                closure_agent(
                    "A9",
                    "docs/agents/wave-documentation-role.md",
                    "[wave-doc-closure]",
                    ".wave/docs/wave-6.md",
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
                        bundle: CONTEXT7_NONE_BUNDLE.to_string(),
                        query: Some("Typed config loading for authored-wave parsing".to_string()),
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
                    deliverables: vec!["crates/wave-dark-factory/src/lib.rs".to_string()],
                    file_ownership: vec!["crates/wave-dark-factory/src/lib.rs".to_string()],
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
                    prompt: implementation_prompt("crates/wave-dark-factory/src/lib.rs"),
                },
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(findings.iter().any(|finding| {
            finding.rule == "context7-default-bundle-weak"
                && finding.message == "wave Context7 defaults must not use the none bundle"
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-context7-bundle-weak"
                && finding.message == "agent A1 Context7 bundle must not be the none bundle"
        }));
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
                ..default_wave_metadata()
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
                query: Some(
                    "Prompt contract parsing, file ownership, and final-marker enforcement for authored-wave prompts".to_string(),
                ),
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
                        query: Some(
                            "Typed config loading, authored-wave parsing, and dark-factory lint enforcement for this implementation slice".to_string(),
                        ),
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
                    depends_on_agents: Vec::new(),
                    reads_artifacts_from: Vec::new(),
                    writes_artifacts: Vec::new(),
                    barrier_class: wave_spec::BarrierClass::Independent,
                    parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                    exclusive_resources: Vec::new(),
                    parallel_with: Vec::new(),
                    prompt: [
                        "Primary goal:",
                        "- Ship the implementation slice.",
                        "",
                        "Required context before coding:",
                        "- Read README.md.",
                        "",
                        "File ownership (only touch these paths):",
                        "- crates/wave-dark-factory/src/lib.rs",
                        "",
                        "Unexpected:",
                        "- hidden contract",
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
                .any(|finding| { finding.rule == "agent-prompt-final-marker-format-required" })
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "agent-prompt-section-unexpected")
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.rule == "agent-final-marker-unexpected")
        );
    }

    #[test]
    fn flags_empty_machine_readable_contract_entries() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/07.md"),
            metadata: WaveMetadata {
                id: 7,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A1".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string(), "  ".to_string()],
                rollback: vec!["git revert".to_string(), "git revert".to_string()],
                proof: vec!["proof.json".to_string(), "".to_string()],
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 7 - Contract Entries".to_string()),
            commit_message: Some("Feat: contract entries".to_string()),
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
                query: Some(
                    "Machine-readable dark-factory contract entry validation for environment, rollback, and proof fields".to_string(),
                ),
            }),
            agents: vec![
                closure_agent(
                    "A0",
                    "docs/agents/wave-cont-qa-role.md",
                    "[wave-gate]",
                    ".wave/reviews/wave-7.md",
                ),
                closure_agent(
                    "A8",
                    "docs/agents/wave-integration-role.md",
                    "[wave-integration]",
                    ".wave/integration/wave-7.md",
                ),
                closure_agent(
                    "A9",
                    "docs/agents/wave-documentation-role.md",
                    "[wave-doc-closure]",
                    ".wave/docs/wave-7.md",
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
                        query: Some(
                            "Typed config loading, authored-wave parsing, and dark-factory lint enforcement for this implementation slice".to_string(),
                        ),
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
                    deliverables: vec!["crates/wave-dark-factory/src/lib.rs".to_string()],
                    file_ownership: vec!["crates/wave-dark-factory/src/lib.rs".to_string()],
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
                    prompt: implementation_prompt("crates/wave-dark-factory/src/lib.rs"),
                },
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(findings.iter().any(|finding| {
            finding.rule == "dark-factory-command-empty"
                && finding.message == "validation entry 2 must not be empty"
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "dark-factory-command-duplicate"
                && finding.message == "rollback entry 2 is duplicated: git revert"
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "dark-factory-command-empty"
                && finding.message == "proof entry 2 must not be empty"
        }));
    }

    #[test]
    fn flags_prompt_section_order_mismatches() {
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
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 3 - Prompt Order".to_string()),
            commit_message: Some("Feat: prompt order".to_string()),
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
                query: Some(
                    "Prompt section ordering, markdown parsing, and authored-wave contract enforcement".to_string(),
                ),
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
                        query: Some(
                            "Prompt section ordering, markdown parsing, and authored-wave contract enforcement".to_string(),
                        ),
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
                    deliverables: vec!["crates/wave-dark-factory/src/lib.rs".to_string()],
                    file_ownership: vec!["crates/wave-dark-factory/src/lib.rs".to_string()],
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
                    prompt: [
                        "Primary goal:",
                        "- Ship the implementation slice.",
                        "",
                        "Specific expectations:",
                        "- Emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output.",
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
                .any(|finding| finding.rule == "agent-prompt-section-order-required")
        );
    }

    #[test]
    fn flags_deliverables_outside_owned_paths() {
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
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 4 - Ownership".to_string()),
            commit_message: Some("Feat: ownership contract".to_string()),
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
                query: Some(
                    "Deliverable ownership enforcement and path overlap checks for authored-wave slices".to_string(),
                ),
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
                        bundle: "rust-config-spec".to_string(),
                        query: Some(
                            "Deliverable ownership enforcement and path overlap checks for authored-wave slices".to_string(),
                        ),
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
                    file_ownership: vec!["crates/wave-dark-factory/src/lib.rs".to_string()],
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
                    prompt: implementation_prompt("crates/wave-dark-factory/src/lib.rs"),
                },
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-deliverable-owned-path-required"
                && finding.message
                    == "implementation agent A1 declares deliverable README.md outside its owned paths"
        }));
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
                ..default_wave_metadata()
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
                query: Some(
                    "Prompt contract parsing, file ownership, and final-marker enforcement for authored-wave prompts".to_string(),
                ),
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
                        query: Some(
                            "Typed config loading, authored-wave parsing, and dark-factory lint enforcement for this implementation slice".to_string(),
                        ),
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
                    depends_on_agents: Vec::new(),
                    reads_artifacts_from: Vec::new(),
                    writes_artifacts: Vec::new(),
                    barrier_class: wave_spec::BarrierClass::Independent,
                    parallel_safety: wave_spec::ParallelSafetyClass::Derived,
                    exclusive_resources: Vec::new(),
                    parallel_with: Vec::new(),
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
    fn flags_closure_agents_using_implementation_sections() {
        let mut a8 = closure_agent(
            "A8",
            "docs/agents/wave-integration-role.md",
            "[wave-integration]",
            ".wave/integration/wave-5.md",
        );
        a8.components = vec!["example".to_string()];
        a8.capabilities = vec!["capability".to_string()];
        a8.deliverables = vec!["crates/wave-spec/src/lib.rs".to_string()];
        a8.exit_contract = Some(ExitContract {
            completion: CompletionLevel::Integrated,
            durability: DurabilityLevel::Durable,
            proof: ProofLevel::Integration,
            doc_impact: DocImpact::Owned,
        });

        let wave = WaveDocument {
            path: PathBuf::from("waves/05.md"),
            metadata: WaveMetadata {
                id: 5,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A1".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 5 - Closure Contract".to_string()),
            commit_message: Some("Feat: closure contract".to_string()),
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
                query: Some(
                    "Closure sections stay distinct from implementation sections".to_string(),
                ),
            }),
            agents: vec![
                closure_agent(
                    "A0",
                    "docs/agents/wave-cont-qa-role.md",
                    "[wave-gate]",
                    ".wave/reviews/wave-5.md",
                ),
                a8,
                closure_agent(
                    "A9",
                    "docs/agents/wave-documentation-role.md",
                    "[wave-doc-closure]",
                    ".wave/docs/wave-5.md",
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
                        query: Some(
                            "Typed config loading, authored-wave parsing, and dark-factory lint enforcement for this implementation slice".to_string(),
                        ),
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
                    deliverables: vec!["crates/wave-spec/src/lib.rs".to_string()],
                    file_ownership: vec!["crates/wave-spec/src/lib.rs".to_string()],
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
                    prompt: implementation_prompt("crates/wave-spec/src/lib.rs"),
                },
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(findings.iter().any(|finding| {
            finding.rule == "closure-agent-components-unexpected"
                && finding.message == "closure agent A8 must not declare implementation components"
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "closure-agent-capabilities-unexpected"
                && finding.message
                    == "closure agent A8 must not declare implementation capabilities"
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "closure-agent-deliverables-unexpected"
                && finding.message
                    == "closure agent A8 must not declare implementation deliverables"
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "closure-agent-exit-contract-unexpected"
                && finding.message
                    == "closure agent A8 must not declare an implementation exit contract"
        }));
    }

    #[test]
    fn flags_marker_instructions_without_plain_line_guidance() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/06.md"),
            metadata: WaveMetadata {
                id: 6,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A1".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 6 - Marker Format".to_string()),
            commit_message: Some("Feat: marker format".to_string()),
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
                query: Some(
                    "Plain-line final marker instructions for wave-proof, wave-doc-delta, and wave-component output".to_string(),
                ),
            }),
            agents: vec![
                closure_agent(
                    "A0",
                    "docs/agents/wave-cont-qa-role.md",
                    "[wave-gate]",
                    ".wave/reviews/wave-6.md",
                ),
                closure_agent(
                    "A8",
                    "docs/agents/wave-integration-role.md",
                    "[wave-integration]",
                    ".wave/integration/wave-6.md",
                ),
                closure_agent(
                    "A9",
                    "docs/agents/wave-documentation-role.md",
                    "[wave-doc-closure]",
                    ".wave/docs/wave-6.md",
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
                        query: Some(
                            "Typed config loading, authored-wave parsing, and dark-factory lint enforcement for this implementation slice".to_string(),
                        ),
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
                    deliverables: vec!["crates/wave-spec/src/lib.rs".to_string()],
                    file_ownership: vec!["crates/wave-spec/src/lib.rs".to_string()],
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
                    prompt: [
                        "Primary goal:",
                        "- Ship the implementation slice.",
                        "",
                        "Required context before coding:",
                        "- Read README.md.",
                        "",
                        "Specific expectations:",
                        "- Emit [wave-proof], [wave-doc-delta], and [wave-component].",
                        "",
                        "File ownership (only touch these paths):",
                        "- crates/wave-spec/src/lib.rs",
                    ]
                    .join("\n"),
                },
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-prompt-final-marker-format-required"
                && finding.message
                    == "agent A1 prompt must require final markers on plain lines or a plain last line"
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

    #[test]
    fn flags_invalid_multi_agent_contract_fields() {
        let mut a1 = implementation_agent("A1", "src/runtime_a.rs");
        a1.depends_on_agents = vec!["A2".to_string()];
        a1.writes_artifacts = vec!["shared-state".to_string()];
        a1.parallel_with = vec!["missing-agent".to_string()];
        a1.exclusive_resources = vec!["runtime-core".to_string(), "runtime-core".to_string()];

        let mut a2 = implementation_agent("A2", "src/runtime_b.rs");
        a2.depends_on_agents = vec!["A1".to_string()];
        a2.writes_artifacts = vec!["shared-state".to_string()];

        let mut a8 = closure_agent(
            "A8",
            "docs/agents/wave-integration-role.md",
            "[wave-integration]",
            ".wave/integration/wave-18.md",
        );
        a8.reads_artifacts_from = vec!["shared-state".to_string(), "missing-state".to_string()];

        let wave = WaveDocument {
            path: PathBuf::from("waves/18.md"),
            metadata: WaveMetadata {
                id: 18,
                slug: "wave-18".to_string(),
                title: "Wave 18".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["runtime".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
                execution_model: wave_spec::WaveExecutionModel::MultiAgent,
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 18 - MAS".to_string()),
            commit_message: Some("Feat: MAS".to_string()),
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
                query: Some(
                    "Multi-agent authored-wave metadata, graph validation, and fail-closed runtime contracts".to_string(),
                ),
            }),
            agents: vec![
                closure_agent(
                    "A0",
                    "docs/agents/wave-cont-qa-role.md",
                    "[wave-gate]",
                    ".wave/reviews/wave-18.md",
                ),
                a8,
                closure_agent(
                    "A9",
                    "docs/agents/wave-documentation-role.md",
                    "[wave-doc-closure]",
                    "docs/plans/master-plan.md",
                ),
                a1,
                a2,
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-parallel-with-known" && finding.message.contains("missing-agent")
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-exclusive-resource-duplicate"
                && finding.message.contains("runtime-core")
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-writes-artifact-ambiguous"
                && finding.message.contains("shared-state")
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-reads-artifact-known"
                && finding.message.contains("missing-state")
        }));
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-dependency-cycle" && finding.message.contains("A1 -> A2 -> A1")
        }));
    }

    #[test]
    fn flags_barrier_expanded_cycles_in_multi_agent_graph() {
        let mut a1 = implementation_agent("A1", "src/runtime_a.rs");
        a1.depends_on_agents = vec!["A8".to_string()];

        let mut a8 = closure_agent(
            "A8",
            "docs/agents/wave-integration-role.md",
            "[wave-integration]",
            ".wave/integration/wave-18.md",
        );
        a8.barrier_class = wave_spec::BarrierClass::IntegrationBarrier;

        let wave = WaveDocument {
            path: PathBuf::from("waves/18.md"),
            metadata: WaveMetadata {
                id: 18,
                slug: "wave-18".to_string(),
                title: "Wave 18".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["runtime".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
                execution_model: wave_spec::WaveExecutionModel::MultiAgent,
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 18 - MAS".to_string()),
            commit_message: Some("Feat: MAS".to_string()),
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
                query: Some(
                    "Barrier-expanded MAS dependency graphs must still remain acyclic".to_string(),
                ),
            }),
            agents: vec![
                closure_agent(
                    "A0",
                    "docs/agents/wave-cont-qa-role.md",
                    "[wave-gate]",
                    ".wave/reviews/wave-18.md",
                ),
                a8,
                closure_agent(
                    "A9",
                    "docs/agents/wave-documentation-role.md",
                    "[wave-doc-closure]",
                    "docs/plans/master-plan.md",
                ),
                a1,
            ],
        };

        let findings = lint_project(&workspace_root(), &[wave]);
        assert!(findings.iter().any(|finding| {
            finding.rule == "agent-dependency-cycle"
                && finding.message.contains("A8")
                && finding.message.contains("A1")
        }));
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
            skills: vec!["wave-core".to_string()],
            components: Vec::new(),
            capabilities: Vec::new(),
            exit_contract: None,
            deliverables: Vec::new(),
            file_ownership: vec![owned_path.to_string()],
            final_markers: vec![marker.to_string()],
            depends_on_agents: Vec::new(),
            reads_artifacts_from: Vec::new(),
            writes_artifacts: Vec::new(),
            barrier_class: wave_spec::BarrierClass::Independent,
            parallel_safety: wave_spec::ParallelSafetyClass::Derived,
            exclusive_resources: Vec::new(),
            parallel_with: Vec::new(),
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

    fn implementation_agent(id: &str, owned_path: &str) -> WaveAgent {
        WaveAgent {
            id: id.to_string(),
            title: format!("Implementation {id}"),
            role_prompts: Vec::new(),
            executor: BTreeMap::from([("profile".to_string(), "implement-codex".to_string())]),
            context7: Some(Context7Defaults {
                bundle: "rust-config-spec".to_string(),
                query: Some(
                    "Typed config loading, authored-wave parsing, and dark-factory lint enforcement for this implementation slice".to_string(),
                ),
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
            deliverables: vec![owned_path.to_string()],
            file_ownership: vec![owned_path.to_string()],
            final_markers: vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            depends_on_agents: Vec::new(),
            reads_artifacts_from: Vec::new(),
            writes_artifacts: Vec::new(),
            barrier_class: wave_spec::BarrierClass::Independent,
            parallel_safety: wave_spec::ParallelSafetyClass::ParallelSafe,
            exclusive_resources: Vec::new(),
            parallel_with: Vec::new(),
            prompt: implementation_prompt(owned_path),
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
