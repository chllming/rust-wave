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
pub const DEFAULT_VERSION: u32 = 1;
pub const DEFAULT_PROJECT_NAME: &str = "Codex Wave Mode";
pub const DEFAULT_DEFAULT_LANE: &str = "main";
pub const DEFAULT_WAVES_DIR: &str = "waves";
pub const DEFAULT_PROJECT_CODEX_HOME: &str = ".wave/codex";
pub const DEFAULT_STATE_DIR: &str = ".wave/state";
pub const DEFAULT_STATE_RUNS_DIR: &str = ".wave/state/runs";
pub const DEFAULT_STATE_CONTROL_DIR: &str = ".wave/state/control";
pub const DEFAULT_TRACE_DIR: &str = ".wave/traces";
pub const DEFAULT_TRACE_RUNS_DIR: &str = ".wave/traces/runs";
pub const DEFAULT_CODEX_VENDOR_DIR: &str = "third_party/codex-rs";
pub const DEFAULT_REFERENCE_WAVE_REPO_DIR: &str = "third_party/agent-wave-orchestrator";

fn default_version() -> u32 {
    DEFAULT_VERSION
}

fn default_project_name() -> String {
    DEFAULT_PROJECT_NAME.to_string()
}

fn default_default_lane() -> String {
    DEFAULT_DEFAULT_LANE.to_string()
}

fn default_waves_dir() -> PathBuf {
    PathBuf::from(DEFAULT_WAVES_DIR)
}

fn default_project_codex_home() -> PathBuf {
    PathBuf::from(DEFAULT_PROJECT_CODEX_HOME)
}

fn default_state_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_DIR)
}

fn default_state_runs_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_RUNS_DIR)
}

fn default_state_control_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_CONTROL_DIR)
}

fn default_trace_dir() -> PathBuf {
    PathBuf::from(DEFAULT_TRACE_DIR)
}

fn default_trace_runs_dir() -> PathBuf {
    PathBuf::from(DEFAULT_TRACE_RUNS_DIR)
}

fn default_codex_vendor_dir() -> PathBuf {
    PathBuf::from(DEFAULT_CODEX_VENDOR_DIR)
}

fn default_reference_wave_repo_dir() -> PathBuf {
    PathBuf::from(DEFAULT_REFERENCE_WAVE_REPO_DIR)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionMode {
    Oversight,
    DarkFactory,
}

impl Default for ExecutionMode {
    fn default() -> Self {
        Self::DarkFactory
    }
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
#[serde(deny_unknown_fields)]
pub struct DarkFactoryPolicy {
    #[serde(default = "default_true")]
    pub require_validation: bool,
    #[serde(default = "default_true")]
    pub require_rollback: bool,
    #[serde(default = "default_true")]
    pub require_proof: bool,
    #[serde(default = "default_true")]
    pub require_closure: bool,
}

fn default_true() -> bool {
    true
}

impl Default for DarkFactoryPolicy {
    fn default() -> Self {
        Self {
            require_validation: true,
            require_rollback: true,
            require_proof: true,
            require_closure: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct LaneConfig {
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_project_name")]
    pub project_name: String,
    #[serde(default = "default_default_lane")]
    pub default_lane: String,
    #[serde(default)]
    pub default_mode: ExecutionMode,
    #[serde(default = "default_waves_dir")]
    pub waves_dir: PathBuf,
    #[serde(default = "default_project_codex_home")]
    pub project_codex_home: PathBuf,
    #[serde(default = "default_state_dir")]
    pub state_dir: PathBuf,
    #[serde(default = "default_state_runs_dir")]
    pub state_runs_dir: PathBuf,
    #[serde(default = "default_state_control_dir")]
    pub state_control_dir: PathBuf,
    #[serde(default = "default_trace_dir")]
    pub trace_dir: PathBuf,
    #[serde(default = "default_trace_runs_dir")]
    pub trace_runs_dir: PathBuf,
    #[serde(default = "default_codex_vendor_dir")]
    pub codex_vendor_dir: PathBuf,
    #[serde(default = "default_reference_wave_repo_dir")]
    pub reference_wave_repo_dir: PathBuf,
    #[serde(default)]
    pub dark_factory: DarkFactoryPolicy,
    #[serde(default)]
    pub lanes: BTreeMap<String, LaneConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedProjectPaths {
    pub config_path: PathBuf,
    pub waves_dir: PathBuf,
    pub project_codex_home: PathBuf,
    pub state_dir: PathBuf,
    pub state_runs_dir: PathBuf,
    pub state_control_dir: PathBuf,
    pub trace_dir: PathBuf,
    pub trace_runs_dir: PathBuf,
    pub codex_vendor_dir: PathBuf,
    pub reference_wave_repo_dir: PathBuf,
}

impl ProjectConfig {
    pub fn load_from_repo_root(repo_root: &Path) -> Result<Self> {
        Self::load(&repo_root.join(DEFAULT_CONFIG_PATH))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config at {}", path.display()))?;
        let config = toml::from_str::<Self>(&raw)
            .with_context(|| format!("failed to parse config at {}", path.display()))?;
        Ok(config)
    }

    pub fn resolved_paths(&self, repo_root: &Path) -> ResolvedProjectPaths {
        let config_path = repo_root.join(DEFAULT_CONFIG_PATH);
        let waves_dir = repo_root.join(&self.waves_dir);
        let project_codex_home = repo_root.join(&self.project_codex_home);
        let state_dir = repo_root.join(&self.state_dir);
        let state_runs_dir = repo_root.join(&self.state_runs_dir);
        let state_control_dir = repo_root.join(&self.state_control_dir);
        let trace_dir = repo_root.join(&self.trace_dir);
        let trace_runs_dir = repo_root.join(&self.trace_runs_dir);
        let codex_vendor_dir = repo_root.join(&self.codex_vendor_dir);
        let reference_wave_repo_dir = repo_root.join(&self.reference_wave_repo_dir);

        ResolvedProjectPaths {
            config_path,
            waves_dir,
            project_codex_home,
            state_dir: state_dir.clone(),
            state_runs_dir,
            state_control_dir,
            trace_dir: trace_dir.clone(),
            trace_runs_dir,
            codex_vendor_dir,
            reference_wave_repo_dir,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults_from_minimal_config() {
        let raw = r#"
version = 1
project_name = "Codex Wave Mode"
"#;
        let config = toml::from_str::<ProjectConfig>(raw).expect("minimal config should parse");
        assert_eq!(config.version, DEFAULT_VERSION);
        assert_eq!(config.project_name, DEFAULT_PROJECT_NAME);
        assert_eq!(config.default_lane, DEFAULT_DEFAULT_LANE);
        assert_eq!(config.default_mode, ExecutionMode::DarkFactory);
        assert_eq!(config.waves_dir, PathBuf::from(DEFAULT_WAVES_DIR));
        assert_eq!(
            config.project_codex_home,
            PathBuf::from(DEFAULT_PROJECT_CODEX_HOME)
        );
        assert_eq!(config.state_dir, PathBuf::from(DEFAULT_STATE_DIR));
        assert_eq!(config.state_runs_dir, PathBuf::from(DEFAULT_STATE_RUNS_DIR));
        assert_eq!(
            config.state_control_dir,
            PathBuf::from(DEFAULT_STATE_CONTROL_DIR)
        );
        assert_eq!(config.trace_dir, PathBuf::from(DEFAULT_TRACE_DIR));
        assert_eq!(config.trace_runs_dir, PathBuf::from(DEFAULT_TRACE_RUNS_DIR));
        assert_eq!(
            config.codex_vendor_dir,
            PathBuf::from(DEFAULT_CODEX_VENDOR_DIR)
        );
        assert_eq!(
            config.reference_wave_repo_dir,
            PathBuf::from(DEFAULT_REFERENCE_WAVE_REPO_DIR)
        );
        assert_eq!(config.dark_factory, DarkFactoryPolicy::default());
    }

    #[test]
    fn rejects_unknown_config_fields() {
        let raw = r#"
version = 1
project_name = "Codex Wave Mode"
unexpected = true
"#;
        let err = toml::from_str::<ProjectConfig>(raw).expect_err("unknown keys should fail");
        let message = err.to_string();
        assert!(
            message.contains("unexpected"),
            "unexpected field should be named in the error: {message}"
        );
    }

    #[test]
    fn resolves_project_paths_from_repo_root() {
        let config = ProjectConfig::load_from_repo_root(Path::new("/home/coder/codex-wave-mode"))
            .expect("config should parse");
        let paths = config.resolved_paths(Path::new("/home/coder/codex-wave-mode"));

        assert_eq!(
            paths.config_path,
            PathBuf::from("/home/coder/codex-wave-mode/wave.toml")
        );
        assert_eq!(
            paths.waves_dir,
            PathBuf::from("/home/coder/codex-wave-mode/waves")
        );
        assert_eq!(
            paths.project_codex_home,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/codex")
        );
        assert_eq!(
            paths.state_runs_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/runs")
        );
        assert_eq!(
            paths.state_control_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/control")
        );
        assert_eq!(
            paths.trace_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/traces")
        );
        assert_eq!(
            paths.trace_runs_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/traces/runs")
        );
        assert_eq!(
            paths.codex_vendor_dir,
            PathBuf::from("/home/coder/codex-wave-mode/third_party/codex-rs")
        );
        assert_eq!(
            paths.reference_wave_repo_dir,
            PathBuf::from("/home/coder/codex-wave-mode/third_party/agent-wave-orchestrator")
        );
    }
}
