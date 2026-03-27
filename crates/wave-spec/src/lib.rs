use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use wave_config::ExecutionMode;
use wave_config::ProjectConfig;

const FRONT_MATTER_START: &str = "+++\n";
const FRONT_MATTER_END: &str = "\n+++\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveMetadata {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub mode: ExecutionMode,
    pub owners: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<u32>,
    #[serde(default)]
    pub validation: Vec<String>,
    #[serde(default)]
    pub rollback: Vec<String>,
    #[serde(default)]
    pub proof: Vec<String>,
    #[serde(default)]
    pub wave_class: WaveClass,
    #[serde(default)]
    pub intent: Option<WaveIntent>,
    #[serde(default)]
    pub delivery: Option<WaveDeliveryLink>,
    #[serde(default)]
    pub design_gate: Option<DesignGateSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WaveClass {
    #[default]
    Implementation,
    Design,
    Delivery,
    Acceptance,
    Investigation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaveIntent {
    Implementation,
    Delivery,
    Acceptance,
    Investigation,
    Design,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WaveDeliveryLink {
    pub initiative_id: Option<String>,
    pub release_id: Option<String>,
    pub acceptance_package_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignGateSpec {
    #[serde(default)]
    pub agent_ids: Vec<String>,
    #[serde(default = "default_design_gate_ready_marker")]
    pub ready_marker: String,
}

fn default_design_gate_ready_marker() -> String {
    "ready-for-implementation".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Context7Defaults {
    pub bundle: String,
    pub query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComponentPromotion {
    pub component: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeployEnvironment {
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompletionLevel {
    Contract,
    Integrated,
    Closure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DurabilityLevel {
    None,
    Ephemeral,
    Durable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProofLevel {
    Unit,
    Integration,
    Live,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocImpact {
    None,
    Owned,
    SharedPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExitContract {
    pub completion: CompletionLevel,
    pub durability: DurabilityLevel,
    pub proof: ProofLevel,
    pub doc_impact: DocImpact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveAgent {
    pub id: String,
    pub title: String,
    pub role_prompts: Vec<String>,
    pub executor: BTreeMap<String, String>,
    pub context7: Option<Context7Defaults>,
    pub skills: Vec<String>,
    pub components: Vec<String>,
    pub capabilities: Vec<String>,
    pub exit_contract: Option<ExitContract>,
    pub deliverables: Vec<String>,
    pub file_ownership: Vec<String>,
    pub final_markers: Vec<String>,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromptContract {
    pub sections: Vec<PromptContractSection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromptContractSection {
    pub heading: String,
    pub body: String,
    pub items: Vec<String>,
}

impl PromptContract {
    pub fn section(&self, heading: &str) -> Option<&PromptContractSection> {
        find_prompt_section(&self.sections, heading)
    }

    pub fn restated_file_ownership(&self) -> Vec<String> {
        self.section("File ownership (only touch these paths)")
            .map(|section| section.items.clone())
            .unwrap_or_default()
    }

    pub fn has_required_implementation_sections(&self) -> bool {
        self.section("Primary goal").is_some()
            && self.section("Required context before coding").is_some()
            && self
                .section("File ownership (only touch these paths)")
                .is_some()
    }
}

impl WaveAgent {
    pub fn is_closure_agent(&self) -> bool {
        matches!(self.id.as_str(), "A0" | "A6" | "A7" | "A8" | "A9" | "E0")
    }

    pub fn is_required_closure_agent(&self) -> bool {
        matches!(self.id.as_str(), "A0" | "A8" | "A9")
    }

    pub fn expected_final_markers(&self) -> &'static [&'static str] {
        match self.id.as_str() {
            "A0" => &["[wave-gate]"],
            "A6" => &["[wave-design]"],
            "A7" => &["[wave-security]"],
            "A8" => &["[wave-integration]"],
            "A9" => &["[wave-doc-closure]"],
            "E0" => &["[wave-eval]"],
            _ => &["[wave-proof]", "[wave-doc-delta]", "[wave-component]"],
        }
    }

    pub fn expected_role_prompts(&self) -> &'static [&'static str] {
        match self.id.as_str() {
            "A0" => &["docs/agents/wave-cont-qa-role.md"],
            "A6" => &["docs/agents/wave-design-role.md"],
            "A7" => &["docs/agents/wave-security-role.md"],
            "A8" => &["docs/agents/wave-integration-role.md"],
            "A9" => &["docs/agents/wave-documentation-role.md"],
            "E0" => &["docs/agents/wave-cont-eval-role.md"],
            _ => &[],
        }
    }

    pub fn prompt_contract(&self) -> PromptContract {
        parse_prompt_contract(&self.prompt)
    }

    pub fn prompt_has_section(&self, heading: &str) -> bool {
        self.prompt_contract().section(heading).is_some()
    }

    pub fn prompt_section_text(&self, heading: &str) -> Option<String> {
        self.prompt_contract()
            .section(heading)
            .map(|section| section.body.clone())
    }

    pub fn prompt_list_section(&self, heading: &str) -> Vec<String> {
        parse_prompt_list_section(&self.prompt, heading)
    }

    pub fn prompt_restated_file_ownership(&self) -> Vec<String> {
        self.prompt_contract().restated_file_ownership()
    }

    pub fn prompt_has_required_implementation_sections(&self) -> bool {
        self.prompt_contract()
            .has_required_implementation_sections()
    }

    pub fn owns_path(&self, path: &str) -> bool {
        self.file_ownership
            .iter()
            .any(|owned_path| path_is_owned_by(path, owned_path))
    }

    pub fn is_design_worker(&self) -> bool {
        !self.is_closure_agent()
            && self
                .skills
                .iter()
                .any(|skill| skill.eq_ignore_ascii_case("role-design"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveDocument {
    pub path: PathBuf,
    pub metadata: WaveMetadata,
    pub heading_title: Option<String>,
    pub commit_message: Option<String>,
    pub component_promotions: Vec<ComponentPromotion>,
    pub deploy_environments: Vec<DeployEnvironment>,
    pub context7_defaults: Option<Context7Defaults>,
    pub agents: Vec<WaveAgent>,
}

impl WaveDocument {
    pub fn implementation_agents(&self) -> impl Iterator<Item = &WaveAgent> {
        self.agents.iter().filter(|agent| !agent.is_closure_agent())
    }

    pub fn design_agents(&self) -> impl Iterator<Item = &WaveAgent> {
        self.agents.iter().filter(|agent| agent.is_design_worker())
    }

    pub fn code_implementation_agents(&self) -> impl Iterator<Item = &WaveAgent> {
        self.implementation_agents()
            .filter(|agent| !agent.is_design_worker())
    }

    pub fn closure_agents(&self) -> impl Iterator<Item = &WaveAgent> {
        self.agents.iter().filter(|agent| agent.is_closure_agent())
    }

    pub fn design_gate_agent_ids(&self) -> &[String] {
        self.metadata
            .design_gate
            .as_ref()
            .map(|gate| gate.agent_ids.as_slice())
            .unwrap_or(&[])
    }
}

impl CompletionLevel {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "contract" => Some(Self::Contract),
            "integrated" => Some(Self::Integrated),
            "closure" => Some(Self::Closure),
            _ => None,
        }
    }
}

impl DurabilityLevel {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "none" => Some(Self::None),
            "ephemeral" => Some(Self::Ephemeral),
            "durable" => Some(Self::Durable),
            _ => None,
        }
    }
}

impl ProofLevel {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "unit" => Some(Self::Unit),
            "integration" => Some(Self::Integration),
            "live" => Some(Self::Live),
            "review" => Some(Self::Review),
            _ => None,
        }
    }
}

impl DocImpact {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "none" => Some(Self::None),
            "owned" => Some(Self::Owned),
            "shared-plan" => Some(Self::SharedPlan),
            _ => None,
        }
    }
}

pub fn load_wave_documents(config: &ProjectConfig, root: &Path) -> Result<Vec<WaveDocument>> {
    let waves_dir = root.join(&config.waves_dir);
    let entries = fs::read_dir(&waves_dir)
        .with_context(|| format!("failed to read waves dir {}", waves_dir.display()))?;
    let mut waves = Vec::new();

    for entry in entries {
        let entry = entry
            .with_context(|| format!("failed to read dir entry in {}", waves_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read wave {}", path.display()))?;
        waves.push(parse_wave_document(path, &contents)?);
    }

    waves.sort_by_key(|wave| wave.metadata.id);
    Ok(waves)
}

pub fn parse_wave_document(path: PathBuf, contents: &str) -> Result<WaveDocument> {
    let body = contents
        .strip_prefix(FRONT_MATTER_START)
        .with_context(|| format!("wave {} is missing TOML front matter", path.display()))?;
    let (front_matter, body) = body.split_once(FRONT_MATTER_END).with_context(|| {
        format!(
            "wave {} is missing a closing front matter delimiter",
            path.display()
        )
    })?;
    let metadata = toml::from_str::<WaveMetadata>(front_matter)
        .with_context(|| format!("wave {} has invalid front matter", path.display()))?;

    let (heading_title, body_without_title) = extract_title_and_remainder(body);
    let (preamble, sections) = split_sections_at_level(&body_without_title, 2);

    Ok(WaveDocument {
        path,
        metadata,
        heading_title,
        commit_message: parse_commit_message(&preamble),
        component_promotions: parse_component_promotions(find_section(
            &sections,
            "Component promotions",
        ))?,
        deploy_environments: parse_deploy_environments(find_section(
            &sections,
            "Deploy environments",
        ))?,
        context7_defaults: parse_context7(find_section(&sections, "Context7 defaults"))?,
        agents: parse_agents(&sections)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MarkdownSection {
    heading: String,
    body: String,
}

fn extract_title_and_remainder(body: &str) -> (Option<String>, String) {
    let mut lines = body.lines();
    let mut prefix = Vec::new();

    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            prefix.push(line.to_string());
            continue;
        }
        if let Some(title) = line.strip_prefix("# ") {
            let remainder = lines.collect::<Vec<_>>().join("\n");
            return (Some(title.trim().to_string()), remainder);
        }
        prefix.push(line.to_string());
        break;
    }

    (None, body.to_string())
}

fn split_sections_at_level(text: &str, level: usize) -> (String, Vec<MarkdownSection>) {
    let heading_prefix = "#".repeat(level) + " ";
    let mut preamble = String::new();
    let mut sections = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_body = String::new();
    let mut in_fence = false;

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
        }

        if !in_fence && trimmed.starts_with(&heading_prefix) {
            if let Some(heading) = current_heading.take() {
                sections.push(MarkdownSection {
                    heading,
                    body: current_body.trim().to_string(),
                });
                current_body.clear();
            }
            current_heading = Some(trimmed[heading_prefix.len()..].trim().to_string());
            continue;
        }

        if current_heading.is_some() {
            current_body.push_str(line);
            current_body.push('\n');
        } else {
            preamble.push_str(line);
            preamble.push('\n');
        }
    }

    if let Some(heading) = current_heading {
        sections.push(MarkdownSection {
            heading,
            body: current_body.trim().to_string(),
        });
    }

    (preamble.trim().to_string(), sections)
}

fn find_section<'a>(sections: &'a [MarkdownSection], heading: &str) -> Option<&'a String> {
    sections
        .iter()
        .find(|section| section.heading.eq_ignore_ascii_case(heading))
        .map(|section| &section.body)
}

fn parse_commit_message(preamble: &str) -> Option<String> {
    preamble
        .lines()
        .find_map(parse_commit_message_line)
        .map(|value| {
            let value = value.trim();
            if value.starts_with('`') && value.ends_with('`') && value.len() >= 2 {
                value[1..value.len() - 1].to_string()
            } else {
                value.to_string()
            }
        })
}

fn parse_commit_message_line(line: &str) -> Option<&str> {
    let line = line.trim();
    line.strip_prefix("**Commit message**:")
        .or_else(|| line.strip_prefix("Commit message:"))
}

fn parse_agents(sections: &[MarkdownSection]) -> Result<Vec<WaveAgent>> {
    sections
        .iter()
        .filter(|section| section.heading.starts_with("Agent "))
        .map(parse_agent)
        .collect()
}

fn parse_agent(section: &MarkdownSection) -> Result<WaveAgent> {
    let spec = section
        .heading
        .strip_prefix("Agent ")
        .with_context(|| format!("invalid agent heading: {}", section.heading))?;
    let (id, title) = spec
        .split_once(':')
        .map(|(id, title)| (id.trim().to_string(), title.trim().to_string()))
        .unwrap_or_else(|| (spec.trim().to_string(), String::new()));
    let (_, subsections) = split_sections_at_level(&section.body, 3);
    let agent_id = id.clone();
    let executor_agent_id = agent_id.clone();
    let context7_agent_id = agent_id.clone();

    Ok(WaveAgent {
        id,
        title,
        role_prompts: parse_bullet_section(find_section(&subsections, "Role prompts"))?,
        executor: parse_key_value_section(find_section(&subsections, "Executor"))
            .with_context(|| format!("agent {} has invalid Executor section", executor_agent_id))?
            .into_iter()
            .collect(),
        context7: parse_context7(find_section(&subsections, "Context7"))
            .with_context(|| format!("agent {} has invalid Context7 section", context7_agent_id))?,
        skills: parse_bullet_section(find_section(&subsections, "Skills"))?,
        components: parse_bullet_section(find_section(&subsections, "Components"))?,
        capabilities: parse_bullet_section(find_section(&subsections, "Capabilities"))?,
        exit_contract: parse_exit_contract(find_section(&subsections, "Exit contract"))?,
        deliverables: parse_bullet_section(find_section(&subsections, "Deliverables"))?,
        file_ownership: parse_bullet_section(find_section(&subsections, "File ownership"))?,
        final_markers: parse_bullet_section(find_section(&subsections, "Final markers"))?,
        prompt: parse_prompt(find_section(&subsections, "Prompt")),
    })
}

fn parse_context7(value: Option<&String>) -> Result<Option<Context7Defaults>> {
    let Some(section) = value else {
        return Ok(None);
    };
    let entries = parse_key_value_section(Some(section)).context("invalid Context7 section")?;
    let map = entries.into_iter().collect::<BTreeMap<_, _>>();
    let Some(bundle) = map
        .get("bundle")
        .cloned()
        .filter(|bundle| !bundle.is_empty())
    else {
        anyhow::bail!("Context7 section is missing a bundle");
    };
    let query = map.get("query").cloned().filter(|query| !query.is_empty());
    Ok(Some(Context7Defaults { bundle, query }))
}

fn parse_component_promotions(value: Option<&String>) -> Result<Vec<ComponentPromotion>> {
    parse_key_value_section(value).map(|entries| {
        entries
            .into_iter()
            .map(|(component, target)| ComponentPromotion { component, target })
            .collect()
    })
}

fn parse_deploy_environments(value: Option<&String>) -> Result<Vec<DeployEnvironment>> {
    parse_key_value_section(value).map(|entries| {
        entries
            .into_iter()
            .map(|(name, detail)| DeployEnvironment { name, detail })
            .collect()
    })
}

fn parse_bullet_section(value: Option<&String>) -> Result<Vec<String>> {
    let Some(section) = value else {
        return Ok(Vec::new());
    };

    let mut items = Vec::new();
    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with("- ") {
            anyhow::bail!("expected bullet list item, found `{trimmed}`");
        }
        let item = trimmed.trim_start_matches("- ").trim();
        if item.is_empty() {
            anyhow::bail!("empty bullet list item");
        }
        items.push(item.to_string());
    }

    Ok(items)
}

fn parse_key_value_section(value: Option<&String>) -> Result<Vec<(String, String)>> {
    let Some(section) = value else {
        return Ok(Vec::new());
    };

    let mut entries = Vec::new();
    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !trimmed.starts_with("- ") {
            anyhow::bail!("expected key/value bullet, found `{trimmed}`");
        }
        let entry = trimmed.trim_start_matches("- ").trim();
        let Some((key, value)) = entry.split_once(':') else {
            anyhow::bail!("expected `key: value` bullet, found `{entry}`");
        };
        let key = key.trim();
        if key.is_empty() {
            anyhow::bail!("empty key in key/value bullet");
        }
        entries.push((key.to_string(), unquote(value.trim())));
    }

    Ok(entries)
}

fn parse_exit_contract(value: Option<&String>) -> Result<Option<ExitContract>> {
    let Some(section) = value else {
        return Ok(None);
    };
    let entries =
        parse_key_value_section(Some(section)).context("invalid Exit contract section")?;
    if entries.is_empty() {
        return Ok(None);
    }

    let map = entries.into_iter().collect::<BTreeMap<_, _>>();
    let completion = map
        .get("completion")
        .context("exit contract missing completion")?;
    let durability = map
        .get("durability")
        .context("exit contract missing durability")?;
    let proof = map.get("proof").context("exit contract missing proof")?;
    let doc_impact = map
        .get("doc-impact")
        .context("exit contract missing doc-impact")?;
    Ok(Some(ExitContract {
        completion: CompletionLevel::parse(completion)
            .ok_or_else(|| anyhow::anyhow!("invalid exit contract completion"))?,
        durability: DurabilityLevel::parse(durability)
            .ok_or_else(|| anyhow::anyhow!("invalid exit contract durability"))?,
        proof: ProofLevel::parse(proof)
            .ok_or_else(|| anyhow::anyhow!("invalid exit contract proof"))?,
        doc_impact: DocImpact::parse(doc_impact)
            .ok_or_else(|| anyhow::anyhow!("invalid exit contract doc-impact"))?,
    }))
}

fn parse_prompt(value: Option<&String>) -> String {
    let Some(section) = value else {
        return String::new();
    };
    let mut in_fence = false;
    let mut body = String::new();
    let mut saw_fence = false;

    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            saw_fence = true;
            continue;
        }
        if in_fence {
            body.push_str(line);
            body.push('\n');
        }
    }

    if saw_fence {
        body.trim().to_string()
    } else {
        section.trim().to_string()
    }
}

fn parse_prompt_contract(text: &str) -> PromptContract {
    PromptContract {
        sections: split_prompt_sections(text),
    }
}

fn split_prompt_sections(text: &str) -> Vec<PromptContractSection> {
    let mut sections = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_lines = Vec::new();
    let mut in_fence = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
        }

        if !in_fence && is_prompt_section_heading(trimmed) {
            if let Some(heading) = current_heading.take() {
                sections.push(build_prompt_section(heading, &current_lines));
                current_lines.clear();
            }
            current_heading = Some(trimmed.trim_end_matches(':').trim().to_string());
            continue;
        }

        if current_heading.is_some() {
            current_lines.push(line.to_string());
        }
    }

    if let Some(heading) = current_heading {
        sections.push(build_prompt_section(heading, &current_lines));
    }

    sections
}

fn build_prompt_section(heading: String, lines: &[String]) -> PromptContractSection {
    let body = lines.join("\n");
    let mut items = Vec::new();
    let mut in_fence = false;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }

        if !in_fence && trimmed.starts_with("- ") {
            let item = trimmed.trim_start_matches("- ").trim();
            if !item.is_empty() {
                items.push(item.to_string());
            }
        }
    }

    PromptContractSection {
        heading,
        body: body.trim().to_string(),
        items,
    }
}

fn is_prompt_section_heading(line: &str) -> bool {
    !line.is_empty() && !line.starts_with("- ") && line.ends_with(':')
}

fn find_prompt_section<'a>(
    sections: &'a [PromptContractSection],
    heading: &str,
) -> Option<&'a PromptContractSection> {
    sections
        .iter()
        .find(|section| section.heading.eq_ignore_ascii_case(heading))
}

fn parse_prompt_list_section(prompt: &str, heading: &str) -> Vec<String> {
    parse_prompt_contract(prompt)
        .section(heading)
        .map(|section| section.items.clone())
        .unwrap_or_default()
}

fn path_is_owned_by(path: &str, owned_path: &str) -> bool {
    let path = normalize_owned_path(path);
    let owned_path = normalize_owned_path(owned_path);

    path == owned_path || path.starts_with(&(owned_path + "/"))
}

fn normalize_owned_path(path: &str) -> String {
    path.trim().trim_end_matches('/').to_string()
}

fn unquote(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn parses_rich_wave_document() {
        let raw = r#"+++
id = 7
slug = "queue"
title = "Build the queue"
mode = "dark-factory"
owners = ["A7"]
depends_on = [3, 4]
validation = ["cargo test"]
rollback = ["git revert"]
proof = ["trace.json"]
+++
# Wave 7 - Build the queue

**Commit message**: `Feat: build queue`

## Component promotions
- queue-reducer: repo-landed

## Deploy environments
- repo-local: custom default (repo-local Rust queue work only)

## Context7 defaults
- bundle: rust-control-plane
- query: "Serde and serde_json reducer patterns for Rust control-plane state"

## Agent A0: Running cont-QA

### Role prompts
- docs/agents/wave-cont-qa-role.md

### Executor
- profile: review-codex
- model: gpt-5.4

### Context7
- bundle: none
- query: "Repository docs remain canonical for cont-QA"

### File ownership
- .wave/reviews/wave-7-cont-qa.md

### Final markers
- [wave-gate]

### Prompt
```text
Primary goal:
- Judge whether queue state and proofs really land together.

Required context before coding:
- Read README.md.

File ownership (only touch these paths):
- .wave/reviews/wave-7-cont-qa.md
```

## Agent A1: Queue reducer

### Executor
- profile: implement-codex
- model: gpt-5.4

### Context7
- bundle: rust-control-plane
- query: "Reducer state and queue projections"

### Skills
- wave-core
- role-implementation

### Components
- queue-reducer

### Capabilities
- queue-state

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-control-plane/src/lib.rs

### File ownership
- crates/wave-control-plane/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the reducer.

Required context before coding:
- Read README.md.

File ownership (only touch these paths):
- crates/wave-control-plane/src/lib.rs
```
"#;

        let wave =
            parse_wave_document(PathBuf::from("waves/07-queue.md"), raw).expect("wave parses");
        assert_eq!(wave.metadata.id, 7);
        assert_eq!(
            wave.heading_title.as_deref(),
            Some("Wave 7 - Build the queue")
        );
        assert_eq!(wave.commit_message.as_deref(), Some("Feat: build queue"));
        assert_eq!(wave.component_promotions.len(), 1);
        assert_eq!(wave.deploy_environments.len(), 1);
        assert_eq!(
            wave.context7_defaults
                .as_ref()
                .map(|context7| context7.bundle.as_str()),
            Some("rust-control-plane")
        );
        assert_eq!(wave.agents.len(), 2);
        assert_eq!(wave.agents[0].id, "A0");
        assert_eq!(wave.agents[1].id, "A1");
        assert_eq!(wave.agents[1].deliverables.len(), 1);
        assert_eq!(wave.agents[1].final_markers.len(), 3);
        assert!(wave.agents[1].prompt.contains("Primary goal:"));
        assert!(wave.agents[1].prompt_has_required_implementation_sections());
        assert_eq!(
            wave.agents[1].prompt_restated_file_ownership(),
            vec!["crates/wave-control-plane/src/lib.rs".to_string()]
        );
    }

    #[test]
    fn parses_prompt_sections_from_agent_prompt() {
        let prompt = [
            "Primary goal:",
            "- Ship the reducer.",
            "",
            "Required context before coding:",
            "- Read README.md.",
            "- Read docs/reference/skills.md.",
            "",
            "Specific expectations:",
            "- Emit [wave-proof] when the reducer lands.",
            "",
            "File ownership (only touch these paths):",
            "- crates/wave-control-plane/src/lib.rs",
            "- crates/wave-cli/src/main.rs",
        ]
        .join("\n");

        assert!(
            find_prompt_section(&split_prompt_sections(&prompt), "Specific expectations").is_some()
        );
        assert_eq!(
            parse_prompt_list_section(&prompt, "Required context before coding"),
            vec![
                "Read README.md.".to_string(),
                "Read docs/reference/skills.md.".to_string()
            ]
        );
        assert_eq!(
            parse_prompt_list_section(&prompt, "File ownership (only touch these paths)"),
            vec![
                "crates/wave-control-plane/src/lib.rs".to_string(),
                "crates/wave-cli/src/main.rs".to_string()
            ]
        );
    }

    #[test]
    fn parses_prompt_sections_with_fenced_content() {
        let prompt = [
            "Primary goal:",
            "- Ship the reducer.",
            "",
            "Specific expectations:",
            "```text",
            "Example heading:",
            "- not a section",
            "```",
            "- Emit [wave-proof] as a plain line.",
            "",
            "File ownership (only touch these paths):",
            "- crates/wave-control-plane/src/lib.rs",
        ]
        .join("\n");

        let contract = parse_prompt_contract(&prompt);
        let section = contract
            .section("Specific expectations")
            .expect("specific expectations section");
        assert!(section.body.contains("Example heading:"));
        assert_eq!(
            section.items,
            vec!["Emit [wave-proof] as a plain line.".to_string()]
        );
    }

    #[test]
    fn parses_plain_commit_message_label() {
        let raw = r#"+++
id = 8
slug = "plain-commit"
title = "Plain commit label"
mode = "dark-factory"
owners = ["A1"]
depends_on = []
validation = ["cargo test"]
rollback = ["git revert"]
proof = ["trace.json"]
+++
# Wave 8 - Plain commit label

Commit message: `Feat: plain label`

## Component promotions
- queue-reducer: repo-landed

## Deploy environments
- repo-local: custom default

## Context7 defaults
- bundle: rust-control-plane
- query: "Reducer state and queue projections"

## Agent A1: Queue reducer

### Executor
- profile: implement-codex

### Context7
- bundle: rust-control-plane
- query: "Reducer state and queue projections"

### Skills
- wave-core

### Components
- queue-reducer

### Capabilities
- queue-state

### Exit contract
- completion: integrated
- durability: durable
- proof: integration
- doc-impact: owned

### Deliverables
- crates/wave-control-plane/src/lib.rs

### File ownership
- crates/wave-control-plane/src/lib.rs

### Final markers
- [wave-proof]
- [wave-doc-delta]
- [wave-component]

### Prompt
```text
Primary goal:
- Land the reducer.

Required context before coding:
- Read README.md.

Specific expectations:
- Emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers.

File ownership (only touch these paths):
- crates/wave-control-plane/src/lib.rs
```
"#;

        let wave = parse_wave_document(PathBuf::from("waves/08-plain-commit.md"), raw)
            .expect("wave parses");
        assert_eq!(wave.commit_message.as_deref(), Some("Feat: plain label"));
    }

    #[test]
    fn parses_wave_two_authored_contract_sections() {
        let raw = include_str!("../../../waves/02-config-spec-lint.md");
        let wave =
            parse_wave_document(PathBuf::from("waves/02-config-spec-lint.md"), raw).expect("wave");

        assert_eq!(wave.metadata.id, 2);
        assert_eq!(
            wave.commit_message.as_deref(),
            Some("Feat: land typed config, authored-wave parser, and lint")
        );
        assert_eq!(wave.component_promotions.len(), 2);
        assert_eq!(wave.deploy_environments.len(), 1);
        assert_eq!(
            wave.context7_defaults
                .as_ref()
                .map(|context7| context7.bundle.as_str()),
            Some("rust-config-spec")
        );
        assert_eq!(wave.agents.len(), 6);
        let a2 = wave
            .agents
            .iter()
            .find(|agent| agent.id == "A2")
            .expect("A2 agent");
        assert!(a2.prompt_has_required_implementation_sections());
        assert_eq!(
            a2.prompt_restated_file_ownership(),
            vec!["crates/wave-spec/src/lib.rs".to_string()]
        );
        assert_eq!(
            a2.prompt_list_section("Specific expectations"),
            vec![
                "parse the authored-wave markdown structure directly instead of hiding meaning in freeform prose".to_string(),
                "keep the model explicit enough for lint, doctor, queue status, and later launcher compilation".to_string(),
                "emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output".to_string()
            ]
        );
    }

    #[test]
    fn rejects_context7_without_bundle() {
        let raw = r#"+++
id = 9
slug = "bad-context7"
title = "Bad context7"
mode = "dark-factory"
owners = ["A1"]
depends_on = []
validation = []
rollback = []
proof = []
+++
# Wave 9 - Bad context7

## Context7 defaults
- query: "missing bundle"
"#;

        let err = parse_wave_document(PathBuf::from("waves/09-bad-context7.md"), raw)
            .expect_err("wave should fail");
        assert!(
            err.to_string()
                .contains("Context7 section is missing a bundle")
        );
    }

    #[test]
    fn rejects_malformed_bullet_sections() {
        let raw = r#"+++
id = 10
slug = "bad-bullets"
title = "Bad bullets"
mode = "dark-factory"
owners = ["A1"]
depends_on = []
validation = []
rollback = []
proof = []
+++
# Wave 10 - Bad bullets

## Component promotions
queue-reducer: repo-landed
"#;

        let err = parse_wave_document(PathBuf::from("waves/10-bad-bullets.md"), raw)
            .expect_err("wave should fail");
        assert!(
            err.to_string()
                .contains("expected key/value bullet, found `queue-reducer: repo-landed`")
        );
    }

    #[test]
    fn wave_agent_helpers_expose_closure_contracts_and_owned_paths() {
        let agent = WaveAgent {
            id: "A8".to_string(),
            title: "Integration".to_string(),
            role_prompts: vec!["docs/agents/wave-integration-role.md".to_string()],
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
            file_ownership: vec![
                ".wave/integration/wave-0.md".to_string(),
                "docs/plans/".to_string(),
            ],
            final_markers: vec!["[wave-integration]".to_string()],
            prompt: [
                "Primary goal:",
                "- Reconcile the authored-wave slices.",
                "",
                "Required context before coding:",
                "- Read README.md.",
                "",
                "Specific expectations:",
                "- emit the final [wave-integration] marker as a plain last line",
                "",
                "File ownership (only touch these paths):",
                "- .wave/integration/wave-0.md",
                "- docs/plans/",
            ]
            .join("\n"),
        };

        assert_eq!(
            agent.expected_role_prompts(),
            &["docs/agents/wave-integration-role.md"]
        );
        assert_eq!(
            agent
                .prompt_section_text("Specific expectations")
                .as_deref(),
            Some("- emit the final [wave-integration] marker as a plain last line")
        );
        assert!(agent.owns_path(".wave/integration/wave-0.md"));
        assert!(agent.owns_path("docs/plans/master-plan.md"));
        assert!(!agent.owns_path("crates/wave-spec/src/lib.rs"));
    }
}
