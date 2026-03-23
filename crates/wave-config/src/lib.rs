use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub const DEFAULT_CONFIG_PATH: &str = "wave.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionMode {
    Oversight,
    DarkFactory,
}

impl fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Oversight => write!(f, "oversight"),
            Self::DarkFactory => write!(f, "dark-factory"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DarkFactoryPolicy {
    pub require_validation: bool,
    pub require_rollback: bool,
    pub require_proof: bool,
    pub require_closure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LaneConfig {
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub version: u32,
    pub project_name: String,
    pub default_lane: String,
    pub default_mode: ExecutionMode,
    pub waves_dir: PathBuf,
    pub project_codex_home: PathBuf,
    pub state_dir: PathBuf,
    pub trace_dir: PathBuf,
    pub codex_vendor_dir: PathBuf,
    pub reference_wave_repo_dir: PathBuf,
    pub dark_factory: DarkFactoryPolicy,
    #[serde(default)]
    pub lanes: BTreeMap<String, LaneConfig>,
}

impl ProjectConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config at {}", path.display()))?;
        let config = toml::from_str::<Self>(&raw)
            .with_context(|| format!("failed to parse config at {}", path.display()))?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_mode() {
        let config = ProjectConfig::load(Path::new("/home/coder/codex-wave-mode/wave.toml"))
            .expect("config should parse");
        assert_eq!(config.default_mode, ExecutionMode::DarkFactory);
        assert_eq!(config.default_lane, "main");
    }
}
