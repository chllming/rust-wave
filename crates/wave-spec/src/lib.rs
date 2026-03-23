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

impl WaveAgent {
    pub fn is_closure_agent(&self) -> bool {
        matches!(self.id.as_str(), "A0" | "A8" | "A9" | "E0")
    }

    pub fn is_required_closure_agent(&self) -> bool {
        matches!(self.id.as_str(), "A0" | "A8" | "A9")
    }

    pub fn expected_final_markers(&self) -> &'static [&'static str] {
        match self.id.as_str() {
            "A0" => &["[wave-gate]"],
            "A8" => &["[wave-integration]"],
            "A9" => &["[wave-doc-closure]"],
            "E0" => &["[wave-eval]"],
            _ => &["[wave-proof]", "[wave-doc-delta]", "[wave-component]"],
        }
    }

    pub fn prompt_has_section(&self, heading: &str) -> bool {
        find_prompt_section(&split_prompt_sections(&self.prompt), heading).is_some()
    }

    pub fn prompt_list_section(&self, heading: &str) -> Vec<String> {
        parse_prompt_list_section(&self.prompt, heading)
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

    pub fn closure_agents(&self) -> impl Iterator<Item = &WaveAgent> {
        self.agents.iter().filter(|agent| agent.is_closure_agent())
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
        )),
        deploy_environments: parse_deploy_environments(find_section(
            &sections,
            "Deploy environments",
        )),
        context7_defaults: parse_context7(find_section(&sections, "Context7 defaults")),
        agents: parse_agents(&sections)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MarkdownSection {
    heading: String,
    body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptSection {
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

    Ok(WaveAgent {
        id,
        title,
        role_prompts: parse_list_section(find_section(&subsections, "Role prompts")),
        executor: parse_key_value_lines(find_section(&subsections, "Executor"))
            .into_iter()
            .collect(),
        context7: parse_context7(find_section(&subsections, "Context7")),
        skills: parse_list_section(find_section(&subsections, "Skills")),
        components: parse_list_section(find_section(&subsections, "Components")),
        capabilities: parse_list_section(find_section(&subsections, "Capabilities")),
        exit_contract: parse_exit_contract(find_section(&subsections, "Exit contract")),
        deliverables: parse_list_section(find_section(&subsections, "Deliverables")),
        file_ownership: parse_list_section(find_section(&subsections, "File ownership")),
        final_markers: parse_list_section(find_section(&subsections, "Final markers")),
        prompt: parse_prompt(find_section(&subsections, "Prompt")),
    })
}

fn parse_context7(value: Option<&String>) -> Option<Context7Defaults> {
    let section = value?;
    let mut bundle = None;
    let mut query = None;

    for (key, value) in parse_key_value_lines(Some(section)) {
        match key.as_str() {
            "bundle" if !value.is_empty() => bundle = Some(value),
            "query" if !value.is_empty() => query = Some(value),
            _ => {}
        }
    }

    bundle.map(|bundle| Context7Defaults { bundle, query })
}

fn parse_component_promotions(value: Option<&String>) -> Vec<ComponentPromotion> {
    parse_key_value_lines(value)
        .into_iter()
        .map(|(component, target)| ComponentPromotion { component, target })
        .collect()
}

fn parse_deploy_environments(value: Option<&String>) -> Vec<DeployEnvironment> {
    parse_key_value_lines(value)
        .into_iter()
        .map(|(name, detail)| DeployEnvironment { name, detail })
        .collect()
}

fn parse_list_section(value: Option<&String>) -> Vec<String> {
    value
        .map(|section| {
            section
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with("- "))
                .map(|line| line.trim_start_matches("- ").trim().to_string())
                .filter(|line| !line.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn parse_key_value_lines(value: Option<&String>) -> Vec<(String, String)> {
    value
        .map(|section| {
            section
                .lines()
                .map(str::trim)
                .filter(|line| line.starts_with("- "))
                .filter_map(|line| {
                    let entry = line.trim_start_matches("- ").trim();
                    let (key, value) = entry.split_once(':')?;
                    Some((key.trim().to_string(), unquote(value.trim())))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_exit_contract(value: Option<&String>) -> Option<ExitContract> {
    let entries = parse_key_value_lines(value);
    if entries.is_empty() {
        return None;
    }

    let map = entries.into_iter().collect::<BTreeMap<_, _>>();
    Some(ExitContract {
        completion: CompletionLevel::parse(map.get("completion")?)?,
        durability: DurabilityLevel::parse(map.get("durability")?)?,
        proof: ProofLevel::parse(map.get("proof")?)?,
        doc_impact: DocImpact::parse(map.get("doc-impact")?)?,
    })
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

fn split_prompt_sections(text: &str) -> Vec<PromptSection> {
    let mut sections = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_body = String::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if is_prompt_section_heading(trimmed) {
            if let Some(heading) = current_heading.take() {
                sections.push(PromptSection {
                    heading,
                    body: current_body.trim().to_string(),
                });
                current_body.clear();
            }
            current_heading = Some(trimmed.trim_end_matches(':').trim().to_string());
            continue;
        }

        if current_heading.is_some() {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }

    if let Some(heading) = current_heading {
        sections.push(PromptSection {
            heading,
            body: current_body.trim().to_string(),
        });
    }

    sections
}

fn is_prompt_section_heading(line: &str) -> bool {
    !line.is_empty() && !line.starts_with("- ") && line.ends_with(':')
}

fn find_prompt_section<'a>(
    sections: &'a [PromptSection],
    heading: &str,
) -> Option<&'a PromptSection> {
    sections
        .iter()
        .find(|section| section.heading.eq_ignore_ascii_case(heading))
}

fn parse_prompt_list_section(prompt: &str, heading: &str) -> Vec<String> {
    let sections = split_prompt_sections(prompt);
    let Some(section) = find_prompt_section(&sections, heading) else {
        return Vec::new();
    };

    section
        .body
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("- "))
        .map(|line| line.trim_start_matches("- ").trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
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
}
