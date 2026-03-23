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
pub struct WaveDocument {
    pub path: PathBuf,
    pub metadata: WaveMetadata,
    pub goal: String,
    pub deliverables: Vec<String>,
    pub closure: Vec<String>,
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

    let sections = parse_sections(body);
    Ok(WaveDocument {
        path,
        metadata,
        goal: sections.get("goal").cloned().unwrap_or_default(),
        deliverables: parse_list_section(sections.get("deliverables")),
        closure: parse_list_section(sections.get("closure")),
    })
}

fn parse_sections(body: &str) -> BTreeMap<String, String> {
    let mut sections = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut buffer = String::new();

    for line in body.lines() {
        if let Some(section_name) = line.strip_prefix("## ") {
            if let Some(name) = current.take() {
                sections.insert(name, buffer.trim().to_string());
                buffer.clear();
            }
            current = Some(section_name.trim().to_lowercase());
            continue;
        }

        if current.is_some() {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }

    if let Some(name) = current {
        sections.insert(name, buffer.trim().to_string());
    }

    sections
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wave_document() {
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
## Goal
Build the queue view.

## Deliverables
- Queue reducer
- Queue widget

## Closure
- Lint passes
- Status renders
"#;

        let wave =
            parse_wave_document(PathBuf::from("waves/07-queue.md"), raw).expect("wave parses");
        assert_eq!(wave.metadata.id, 7);
        assert_eq!(wave.deliverables.len(), 2);
        assert_eq!(wave.closure.len(), 2);
    }
}
