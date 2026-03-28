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
pub const DEFAULT_STATE_BUILD_SPECS_DIR: &str = ".wave/state/build/specs";
pub const DEFAULT_STATE_RUNS_DIR: &str = ".wave/state/runs";
pub const DEFAULT_STATE_CONTROL_DIR: &str = ".wave/state/control";
pub const DEFAULT_TRACE_DIR: &str = ".wave/traces";
pub const DEFAULT_TRACE_RUNS_DIR: &str = ".wave/traces/runs";
pub const DEFAULT_DOCS_DIR: &str = "docs";
pub const DEFAULT_SKILLS_DIR: &str = "skills";
pub const DEFAULT_ROLE_PROMPT_DIR: &str = "docs/agents";
pub const DEFAULT_CONT_QA_AGENT_ID: &str = "A0";
pub const DEFAULT_CONT_EVAL_AGENT_ID: &str = "E0";
pub const DEFAULT_INTEGRATION_AGENT_ID: &str = "A8";
pub const DEFAULT_DOCUMENTATION_AGENT_ID: &str = "A9";
pub const DEFAULT_DESIGN_ROLE_PROMPT_PATH: &str = "docs/agents/wave-design-role.md";
pub const DEFAULT_CONT_QA_ROLE_PROMPT_PATH: &str = "docs/agents/wave-cont-qa-role.md";
pub const DEFAULT_CONT_EVAL_ROLE_PROMPT_PATH: &str = "docs/agents/wave-cont-eval-role.md";
pub const DEFAULT_INTEGRATION_ROLE_PROMPT_PATH: &str = "docs/agents/wave-integration-role.md";
pub const DEFAULT_DOCUMENTATION_ROLE_PROMPT_PATH: &str = "docs/agents/wave-documentation-role.md";
pub const DEFAULT_SECURITY_ROLE_PROMPT_PATH: &str = "docs/agents/wave-security-role.md";
pub const DEFAULT_CONTEXT7_BUNDLE_INDEX_PATH: &str = "docs/context7/bundles.json";
pub const DEFAULT_BENCHMARK_CATALOG_PATH: &str = "docs/evals/benchmark-catalog.json";
pub const DEFAULT_COMPONENT_CUTOVER_MATRIX_DOC_PATH: &str =
    "docs/plans/component-cutover-matrix.md";
pub const DEFAULT_COMPONENT_CUTOVER_MATRIX_JSON_PATH: &str =
    "docs/plans/component-cutover-matrix.json";
pub const DEFAULT_DELIVERY_CATALOG_PATH: &str = "docs/plans/delivery-catalog.json";
pub const DEFAULT_STATE_EVENTS_DIR: &str = ".wave/state/events";
pub const DEFAULT_STATE_EVENTS_SCHEDULER_DIR: &str = ".wave/state/events/scheduler";
pub const DEFAULT_STATE_EVENTS_CONTROL_DIR: &str = ".wave/state/events/control";
pub const DEFAULT_STATE_EVENTS_COORDINATION_DIR: &str = ".wave/state/events/coordination";
pub const DEFAULT_STATE_RESULTS_DIR: &str = ".wave/state/results";
pub const DEFAULT_STATE_DERIVED_DIR: &str = ".wave/state/derived";
pub const DEFAULT_STATE_PROJECTIONS_DIR: &str = ".wave/state/projections";
pub const DEFAULT_STATE_TRACES_DIR: &str = ".wave/state/traces";
pub const DEFAULT_STATE_WORKTREES_DIR: &str = ".wave/state/worktrees";
pub const DEFAULT_STATE_ADHOC_DIR: &str = ".wave/state/adhoc";
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

fn default_state_build_specs_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_BUILD_SPECS_DIR)
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

fn default_docs_dir() -> PathBuf {
    PathBuf::from(DEFAULT_DOCS_DIR)
}

fn default_skills_dir() -> PathBuf {
    PathBuf::from(DEFAULT_SKILLS_DIR)
}

fn default_role_prompt_dir() -> PathBuf {
    PathBuf::from(DEFAULT_ROLE_PROMPT_DIR)
}

fn default_cont_qa_agent_id() -> String {
    DEFAULT_CONT_QA_AGENT_ID.to_string()
}

fn default_cont_eval_agent_id() -> String {
    DEFAULT_CONT_EVAL_AGENT_ID.to_string()
}

fn default_integration_agent_id() -> String {
    DEFAULT_INTEGRATION_AGENT_ID.to_string()
}

fn default_documentation_agent_id() -> String {
    DEFAULT_DOCUMENTATION_AGENT_ID.to_string()
}

fn default_cont_qa_role_prompt_path() -> PathBuf {
    PathBuf::from(DEFAULT_CONT_QA_ROLE_PROMPT_PATH)
}

fn default_cont_eval_role_prompt_path() -> PathBuf {
    PathBuf::from(DEFAULT_CONT_EVAL_ROLE_PROMPT_PATH)
}

fn default_integration_role_prompt_path() -> PathBuf {
    PathBuf::from(DEFAULT_INTEGRATION_ROLE_PROMPT_PATH)
}

fn default_documentation_role_prompt_path() -> PathBuf {
    PathBuf::from(DEFAULT_DOCUMENTATION_ROLE_PROMPT_PATH)
}

fn default_design_role_prompt_path() -> PathBuf {
    PathBuf::from(DEFAULT_DESIGN_ROLE_PROMPT_PATH)
}

fn default_security_role_prompt_path() -> PathBuf {
    PathBuf::from(DEFAULT_SECURITY_ROLE_PROMPT_PATH)
}

fn default_context7_bundle_index_path() -> PathBuf {
    PathBuf::from(DEFAULT_CONTEXT7_BUNDLE_INDEX_PATH)
}

fn default_benchmark_catalog_path() -> PathBuf {
    PathBuf::from(DEFAULT_BENCHMARK_CATALOG_PATH)
}

fn default_component_cutover_matrix_doc_path() -> PathBuf {
    PathBuf::from(DEFAULT_COMPONENT_CUTOVER_MATRIX_DOC_PATH)
}

fn default_component_cutover_matrix_json_path() -> PathBuf {
    PathBuf::from(DEFAULT_COMPONENT_CUTOVER_MATRIX_JSON_PATH)
}

fn default_delivery_catalog_path() -> PathBuf {
    PathBuf::from(DEFAULT_DELIVERY_CATALOG_PATH)
}

fn default_state_events_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_EVENTS_DIR)
}

fn default_state_events_scheduler_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_EVENTS_SCHEDULER_DIR)
}

fn default_state_events_control_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_EVENTS_CONTROL_DIR)
}

fn default_state_events_coordination_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_EVENTS_COORDINATION_DIR)
}

fn default_state_results_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_RESULTS_DIR)
}

fn default_state_derived_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_DERIVED_DIR)
}

fn default_state_projections_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_PROJECTIONS_DIR)
}

fn default_state_traces_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_TRACES_DIR)
}

fn default_state_worktrees_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_WORKTREES_DIR)
}

fn default_state_adhoc_dir() -> PathBuf {
    PathBuf::from(DEFAULT_STATE_ADHOC_DIR)
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
pub struct RolePromptConfig {
    #[serde(default = "default_role_prompt_dir")]
    pub dir: PathBuf,
    #[serde(default = "default_cont_qa_role_prompt_path")]
    pub cont_qa: PathBuf,
    #[serde(default = "default_cont_eval_role_prompt_path")]
    pub cont_eval: PathBuf,
    #[serde(default = "default_integration_role_prompt_path")]
    pub integration: PathBuf,
    #[serde(default = "default_documentation_role_prompt_path")]
    pub documentation: PathBuf,
    #[serde(default = "default_design_role_prompt_path")]
    pub design: PathBuf,
    #[serde(default = "default_security_role_prompt_path")]
    pub security: PathBuf,
}

impl Default for RolePromptConfig {
    fn default() -> Self {
        Self {
            dir: default_role_prompt_dir(),
            cont_qa: default_cont_qa_role_prompt_path(),
            cont_eval: default_cont_eval_role_prompt_path(),
            integration: default_integration_role_prompt_path(),
            documentation: default_documentation_role_prompt_path(),
            design: default_design_role_prompt_path(),
            security: default_security_role_prompt_path(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityConfig {
    #[serde(default = "default_project_codex_home")]
    pub project_codex_home: PathBuf,
    #[serde(default = "default_state_dir")]
    pub state_dir: PathBuf,
    #[serde(default = "default_state_build_specs_dir")]
    pub state_build_specs_dir: PathBuf,
    #[serde(default = "default_state_runs_dir")]
    pub state_runs_dir: PathBuf,
    #[serde(default = "default_state_control_dir")]
    pub state_control_dir: PathBuf,
    #[serde(default = "default_trace_dir")]
    pub trace_dir: PathBuf,
    #[serde(default = "default_trace_runs_dir")]
    pub trace_runs_dir: PathBuf,
    #[serde(default = "default_state_events_dir")]
    pub state_events_dir: PathBuf,
    #[serde(default = "default_state_events_scheduler_dir")]
    pub state_events_scheduler_dir: PathBuf,
    #[serde(default = "default_state_events_control_dir")]
    pub state_events_control_dir: PathBuf,
    #[serde(default = "default_state_events_coordination_dir")]
    pub state_events_coordination_dir: PathBuf,
    #[serde(default = "default_state_results_dir")]
    pub state_results_dir: PathBuf,
    #[serde(default = "default_state_derived_dir")]
    pub state_derived_dir: PathBuf,
    #[serde(default = "default_state_projections_dir")]
    pub state_projections_dir: PathBuf,
    #[serde(default = "default_state_traces_dir")]
    pub state_traces_dir: PathBuf,
    #[serde(default = "default_state_worktrees_dir")]
    pub state_worktrees_dir: PathBuf,
    #[serde(default = "default_state_adhoc_dir")]
    pub state_adhoc_dir: PathBuf,
}

impl Default for AuthorityConfig {
    fn default() -> Self {
        Self {
            project_codex_home: default_project_codex_home(),
            state_dir: default_state_dir(),
            state_build_specs_dir: default_state_build_specs_dir(),
            state_runs_dir: default_state_runs_dir(),
            state_control_dir: default_state_control_dir(),
            trace_dir: default_trace_dir(),
            trace_runs_dir: default_trace_runs_dir(),
            state_events_dir: default_state_events_dir(),
            state_events_scheduler_dir: default_state_events_scheduler_dir(),
            state_events_control_dir: default_state_events_control_dir(),
            state_events_coordination_dir: default_state_events_coordination_dir(),
            state_results_dir: default_state_results_dir(),
            state_derived_dir: default_state_derived_dir(),
            state_projections_dir: default_state_projections_dir(),
            state_traces_dir: default_state_traces_dir(),
            state_worktrees_dir: default_state_worktrees_dir(),
            state_adhoc_dir: default_state_adhoc_dir(),
        }
    }
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
    #[serde(default = "default_docs_dir")]
    pub docs_dir: PathBuf,
    #[serde(default = "default_skills_dir")]
    pub skills_dir: PathBuf,
    #[serde(default)]
    pub role_prompts: RolePromptConfig,
    #[serde(default)]
    pub authority: AuthorityConfig,
    #[serde(default = "default_cont_qa_agent_id")]
    pub cont_qa_agent_id: String,
    #[serde(default = "default_cont_eval_agent_id")]
    pub cont_eval_agent_id: String,
    #[serde(default = "default_integration_agent_id")]
    pub integration_agent_id: String,
    #[serde(default = "default_documentation_agent_id")]
    pub documentation_agent_id: String,
    #[serde(default = "default_context7_bundle_index_path")]
    pub context7_bundle_index_path: PathBuf,
    #[serde(default = "default_benchmark_catalog_path")]
    pub benchmark_catalog_path: PathBuf,
    #[serde(default = "default_component_cutover_matrix_doc_path")]
    pub component_cutover_matrix_doc_path: PathBuf,
    #[serde(default = "default_component_cutover_matrix_json_path")]
    pub component_cutover_matrix_json_path: PathBuf,
    #[serde(default = "default_delivery_catalog_path")]
    pub delivery_catalog_path: PathBuf,
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
pub struct ResolvedRolePromptPaths {
    pub dir: PathBuf,
    pub cont_qa: PathBuf,
    pub cont_eval: PathBuf,
    pub integration: PathBuf,
    pub documentation: PathBuf,
    pub design: PathBuf,
    pub security: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedAuthorityPaths {
    pub project_codex_home: PathBuf,
    pub state_dir: PathBuf,
    pub state_build_specs_dir: PathBuf,
    pub state_runs_dir: PathBuf,
    pub state_control_dir: PathBuf,
    pub trace_dir: PathBuf,
    pub trace_runs_dir: PathBuf,
    pub state_events_dir: PathBuf,
    pub state_events_scheduler_dir: PathBuf,
    pub state_events_control_dir: PathBuf,
    pub state_events_coordination_dir: PathBuf,
    pub state_results_dir: PathBuf,
    pub state_derived_dir: PathBuf,
    pub state_projections_dir: PathBuf,
    pub state_traces_dir: PathBuf,
    pub state_worktrees_dir: PathBuf,
    pub state_adhoc_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedProjectPaths {
    pub config_path: PathBuf,
    pub waves_dir: PathBuf,
    pub docs_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub role_prompts: ResolvedRolePromptPaths,
    pub authority: ResolvedAuthorityPaths,
    pub context7_bundle_index_path: PathBuf,
    pub benchmark_catalog_path: PathBuf,
    pub component_cutover_matrix_doc_path: PathBuf,
    pub component_cutover_matrix_json_path: PathBuf,
    pub delivery_catalog_path: PathBuf,
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
        let docs_dir = repo_root.join(&self.docs_dir);
        let skills_dir = repo_root.join(&self.skills_dir);
        let role_prompts = ResolvedRolePromptPaths {
            dir: repo_root.join(&self.role_prompts.dir),
            cont_qa: repo_root.join(&self.role_prompts.cont_qa),
            cont_eval: repo_root.join(&self.role_prompts.cont_eval),
            integration: repo_root.join(&self.role_prompts.integration),
            documentation: repo_root.join(&self.role_prompts.documentation),
            design: repo_root.join(&self.role_prompts.design),
            security: repo_root.join(&self.role_prompts.security),
        };
        let authority = ResolvedAuthorityPaths {
            project_codex_home: repo_root.join(&self.authority.project_codex_home),
            state_dir: repo_root.join(&self.authority.state_dir),
            state_build_specs_dir: repo_root.join(&self.authority.state_build_specs_dir),
            state_runs_dir: repo_root.join(&self.authority.state_runs_dir),
            state_control_dir: repo_root.join(&self.authority.state_control_dir),
            trace_dir: repo_root.join(&self.authority.trace_dir),
            trace_runs_dir: repo_root.join(&self.authority.trace_runs_dir),
            state_events_dir: repo_root.join(&self.authority.state_events_dir),
            state_events_scheduler_dir: repo_root.join(&self.authority.state_events_scheduler_dir),
            state_events_control_dir: repo_root.join(&self.authority.state_events_control_dir),
            state_events_coordination_dir: repo_root
                .join(&self.authority.state_events_coordination_dir),
            state_results_dir: repo_root.join(&self.authority.state_results_dir),
            state_derived_dir: repo_root.join(&self.authority.state_derived_dir),
            state_projections_dir: repo_root.join(&self.authority.state_projections_dir),
            state_traces_dir: repo_root.join(&self.authority.state_traces_dir),
            state_worktrees_dir: repo_root.join(&self.authority.state_worktrees_dir),
            state_adhoc_dir: repo_root.join(&self.authority.state_adhoc_dir),
        };
        let context7_bundle_index_path = repo_root.join(&self.context7_bundle_index_path);
        let benchmark_catalog_path = repo_root.join(&self.benchmark_catalog_path);
        let component_cutover_matrix_doc_path =
            repo_root.join(&self.component_cutover_matrix_doc_path);
        let component_cutover_matrix_json_path =
            repo_root.join(&self.component_cutover_matrix_json_path);
        let delivery_catalog_path = repo_root.join(&self.delivery_catalog_path);
        let codex_vendor_dir = repo_root.join(&self.codex_vendor_dir);
        let reference_wave_repo_dir = repo_root.join(&self.reference_wave_repo_dir);

        ResolvedProjectPaths {
            config_path,
            waves_dir,
            docs_dir,
            skills_dir,
            role_prompts,
            authority,
            context7_bundle_index_path,
            benchmark_catalog_path,
            component_cutover_matrix_doc_path,
            component_cutover_matrix_json_path,
            delivery_catalog_path,
            codex_vendor_dir,
            reference_wave_repo_dir,
        }
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            version: default_version(),
            project_name: default_project_name(),
            default_lane: default_default_lane(),
            default_mode: ExecutionMode::default(),
            waves_dir: default_waves_dir(),
            docs_dir: default_docs_dir(),
            skills_dir: default_skills_dir(),
            role_prompts: RolePromptConfig::default(),
            authority: AuthorityConfig::default(),
            cont_qa_agent_id: default_cont_qa_agent_id(),
            cont_eval_agent_id: default_cont_eval_agent_id(),
            integration_agent_id: default_integration_agent_id(),
            documentation_agent_id: default_documentation_agent_id(),
            context7_bundle_index_path: default_context7_bundle_index_path(),
            benchmark_catalog_path: default_benchmark_catalog_path(),
            component_cutover_matrix_doc_path: default_component_cutover_matrix_doc_path(),
            component_cutover_matrix_json_path: default_component_cutover_matrix_json_path(),
            delivery_catalog_path: default_delivery_catalog_path(),
            codex_vendor_dir: default_codex_vendor_dir(),
            reference_wave_repo_dir: default_reference_wave_repo_dir(),
            dark_factory: DarkFactoryPolicy::default(),
            lanes: BTreeMap::new(),
        }
    }
}

impl ResolvedRolePromptPaths {
    pub fn all_files(&self) -> [&Path; 6] {
        [
            self.cont_qa.as_path(),
            self.cont_eval.as_path(),
            self.integration.as_path(),
            self.documentation.as_path(),
            self.design.as_path(),
            self.security.as_path(),
        ]
    }
}

impl ResolvedAuthorityPaths {
    pub fn materialize_canonical_state_tree(&self) -> Result<()> {
        for path in std::iter::once(self.state_dir.as_path()).chain(self.canonical_root_paths()) {
            fs::create_dir_all(path)
                .with_context(|| format!("failed to create {}", path.display()))?;
        }
        Ok(())
    }

    pub fn canonical_root_paths(&self) -> [&Path; 11] {
        [
            self.state_build_specs_dir.as_path(),
            self.state_events_dir.as_path(),
            self.state_events_scheduler_dir.as_path(),
            self.state_events_control_dir.as_path(),
            self.state_events_coordination_dir.as_path(),
            self.state_results_dir.as_path(),
            self.state_derived_dir.as_path(),
            self.state_projections_dir.as_path(),
            self.state_traces_dir.as_path(),
            self.state_worktrees_dir.as_path(),
            self.state_adhoc_dir.as_path(),
        ]
    }

    pub fn canonical_roots_within_state_dir(&self) -> bool {
        self.canonical_root_paths()
            .iter()
            .all(|path| path.starts_with(&self.state_dir))
    }
}

impl ResolvedProjectPaths {
    pub fn control_events_log_path(&self, wave_id: u32) -> PathBuf {
        self.authority
            .state_events_control_dir
            .join(format!("wave-{wave_id:02}.jsonl"))
    }

    pub fn scheduler_events_log_path(&self) -> PathBuf {
        self.authority
            .state_events_scheduler_dir
            .join("scheduler.jsonl")
    }

    pub fn coordination_log_path(&self, wave_id: u32) -> PathBuf {
        self.authority
            .state_events_coordination_dir
            .join(format!("wave-{wave_id:02}.jsonl"))
    }

    pub fn wave_results_dir(&self, wave_id: u32) -> PathBuf {
        self.authority
            .state_results_dir
            .join(format!("wave-{wave_id:02}"))
    }

    pub fn wave_attempt_results_dir(&self, wave_id: u32, attempt_id: &str) -> PathBuf {
        self.wave_results_dir(wave_id).join(attempt_id)
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
            config.authority.project_codex_home,
            PathBuf::from(DEFAULT_PROJECT_CODEX_HOME)
        );
        assert_eq!(config.authority.state_dir, PathBuf::from(DEFAULT_STATE_DIR));
        assert_eq!(
            config.authority.state_build_specs_dir,
            PathBuf::from(DEFAULT_STATE_BUILD_SPECS_DIR)
        );
        assert_eq!(
            config.authority.state_runs_dir,
            PathBuf::from(DEFAULT_STATE_RUNS_DIR)
        );
        assert_eq!(
            config.authority.state_control_dir,
            PathBuf::from(DEFAULT_STATE_CONTROL_DIR)
        );
        assert_eq!(config.authority.trace_dir, PathBuf::from(DEFAULT_TRACE_DIR));
        assert_eq!(
            config.authority.trace_runs_dir,
            PathBuf::from(DEFAULT_TRACE_RUNS_DIR)
        );
        assert_eq!(config.docs_dir, PathBuf::from(DEFAULT_DOCS_DIR));
        assert_eq!(config.skills_dir, PathBuf::from(DEFAULT_SKILLS_DIR));
        assert_eq!(
            config.role_prompts.dir,
            PathBuf::from(DEFAULT_ROLE_PROMPT_DIR)
        );
        assert_eq!(config.cont_qa_agent_id, DEFAULT_CONT_QA_AGENT_ID);
        assert_eq!(config.cont_eval_agent_id, DEFAULT_CONT_EVAL_AGENT_ID);
        assert_eq!(config.integration_agent_id, DEFAULT_INTEGRATION_AGENT_ID);
        assert_eq!(
            config.documentation_agent_id,
            DEFAULT_DOCUMENTATION_AGENT_ID
        );
        assert_eq!(
            config.role_prompts.cont_qa,
            PathBuf::from(DEFAULT_CONT_QA_ROLE_PROMPT_PATH)
        );
        assert_eq!(
            config.role_prompts.cont_eval,
            PathBuf::from(DEFAULT_CONT_EVAL_ROLE_PROMPT_PATH)
        );
        assert_eq!(
            config.role_prompts.integration,
            PathBuf::from(DEFAULT_INTEGRATION_ROLE_PROMPT_PATH)
        );
        assert_eq!(
            config.role_prompts.documentation,
            PathBuf::from(DEFAULT_DOCUMENTATION_ROLE_PROMPT_PATH)
        );
        assert_eq!(
            config.role_prompts.security,
            PathBuf::from(DEFAULT_SECURITY_ROLE_PROMPT_PATH)
        );
        assert_eq!(
            config.context7_bundle_index_path,
            PathBuf::from(DEFAULT_CONTEXT7_BUNDLE_INDEX_PATH)
        );
        assert_eq!(
            config.benchmark_catalog_path,
            PathBuf::from(DEFAULT_BENCHMARK_CATALOG_PATH)
        );
        assert_eq!(
            config.component_cutover_matrix_doc_path,
            PathBuf::from(DEFAULT_COMPONENT_CUTOVER_MATRIX_DOC_PATH)
        );
        assert_eq!(
            config.component_cutover_matrix_json_path,
            PathBuf::from(DEFAULT_COMPONENT_CUTOVER_MATRIX_JSON_PATH)
        );
        assert_eq!(
            config.authority.state_events_dir,
            PathBuf::from(DEFAULT_STATE_EVENTS_DIR)
        );
        assert_eq!(
            config.authority.state_events_scheduler_dir,
            PathBuf::from(DEFAULT_STATE_EVENTS_SCHEDULER_DIR)
        );
        assert_eq!(
            config.authority.state_events_control_dir,
            PathBuf::from(DEFAULT_STATE_EVENTS_CONTROL_DIR)
        );
        assert_eq!(
            config.authority.state_events_coordination_dir,
            PathBuf::from(DEFAULT_STATE_EVENTS_COORDINATION_DIR)
        );
        assert_eq!(
            config.authority.state_results_dir,
            PathBuf::from(DEFAULT_STATE_RESULTS_DIR)
        );
        assert_eq!(
            config.authority.state_derived_dir,
            PathBuf::from(DEFAULT_STATE_DERIVED_DIR)
        );
        assert_eq!(
            config.authority.state_projections_dir,
            PathBuf::from(DEFAULT_STATE_PROJECTIONS_DIR)
        );
        assert_eq!(
            config.authority.state_traces_dir,
            PathBuf::from(DEFAULT_STATE_TRACES_DIR)
        );
        assert_eq!(
            config.authority.state_worktrees_dir,
            PathBuf::from(DEFAULT_STATE_WORKTREES_DIR)
        );
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
    fn rejects_unknown_nested_config_fields() {
        let raw = r#"
version = 1
project_name = "Codex Wave Mode"

[authority]
unexpected = true
"#;
        let err =
            toml::from_str::<ProjectConfig>(raw).expect_err("unknown nested keys should fail");
        let message = err.to_string();
        assert!(
            message.contains("unexpected"),
            "unexpected nested field should be named in the error: {message}"
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
            paths.authority.project_codex_home,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/codex")
        );
        assert_eq!(
            paths.authority.state_build_specs_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/build/specs")
        );
        assert_eq!(
            paths.authority.state_runs_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/runs")
        );
        assert_eq!(
            paths.authority.state_control_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/control")
        );
        assert_eq!(
            paths.authority.trace_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/traces")
        );
        assert_eq!(
            paths.authority.trace_runs_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/traces/runs")
        );
        assert_eq!(
            paths.docs_dir,
            PathBuf::from("/home/coder/codex-wave-mode/docs")
        );
        assert_eq!(
            paths.skills_dir,
            PathBuf::from("/home/coder/codex-wave-mode/skills")
        );
        assert_eq!(
            paths.role_prompts.dir,
            PathBuf::from("/home/coder/codex-wave-mode/docs/agents")
        );
        assert_eq!(
            paths.role_prompts.cont_qa,
            PathBuf::from("/home/coder/codex-wave-mode/docs/agents/wave-cont-qa-role.md")
        );
        assert_eq!(
            paths.context7_bundle_index_path,
            PathBuf::from("/home/coder/codex-wave-mode/docs/context7/bundles.json")
        );
        assert_eq!(
            paths.benchmark_catalog_path,
            PathBuf::from("/home/coder/codex-wave-mode/docs/evals/benchmark-catalog.json")
        );
        assert_eq!(
            paths.component_cutover_matrix_doc_path,
            PathBuf::from("/home/coder/codex-wave-mode/docs/plans/component-cutover-matrix.md")
        );
        assert_eq!(
            paths.component_cutover_matrix_json_path,
            PathBuf::from("/home/coder/codex-wave-mode/docs/plans/component-cutover-matrix.json")
        );
        assert_eq!(
            paths.authority.state_events_scheduler_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/events/scheduler")
        );
        assert_eq!(
            paths.authority.state_events_control_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/events/control")
        );
        assert_eq!(
            paths.authority.state_events_coordination_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/events/coordination")
        );
        assert_eq!(
            paths.authority.state_results_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/results")
        );
        assert_eq!(
            paths.authority.state_derived_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/derived")
        );
        assert_eq!(
            paths.authority.state_projections_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/projections")
        );
        assert_eq!(
            paths.authority.state_traces_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/traces")
        );
        assert_eq!(
            paths.authority.state_worktrees_dir,
            PathBuf::from("/home/coder/codex-wave-mode/.wave/state/worktrees")
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

    #[test]
    fn builds_canonical_authority_paths() {
        let config = ProjectConfig::default();
        let paths = config.resolved_paths(Path::new("/repo"));
        assert!(paths.authority.canonical_roots_within_state_dir());
        assert_eq!(
            paths.authority.state_build_specs_dir,
            PathBuf::from("/repo/.wave/state/build/specs")
        );
        assert_eq!(
            paths.scheduler_events_log_path(),
            PathBuf::from("/repo/.wave/state/events/scheduler/scheduler.jsonl")
        );
        assert_eq!(
            paths.control_events_log_path(10),
            PathBuf::from("/repo/.wave/state/events/control/wave-10.jsonl")
        );
        assert_eq!(
            paths.coordination_log_path(10),
            PathBuf::from("/repo/.wave/state/events/coordination/wave-10.jsonl")
        );
        assert_eq!(
            paths.wave_results_dir(10),
            PathBuf::from("/repo/.wave/state/results/wave-10")
        );
        assert_eq!(
            paths.wave_attempt_results_dir(10, "attempt-1"),
            PathBuf::from("/repo/.wave/state/results/wave-10/attempt-1")
        );
    }

    #[test]
    fn materializes_canonical_state_tree() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock before epoch")
            .as_millis();
        let root = std::env::temp_dir().join(format!(
            "wave-config-materialize-roots-{}-{}",
            std::process::id(),
            unique
        ));
        let config = ProjectConfig::default();
        let paths = config.resolved_paths(&root);

        paths
            .authority
            .materialize_canonical_state_tree()
            .expect("materialize authority roots");

        assert!(paths.authority.state_dir.exists());
        for path in paths.authority.canonical_root_paths() {
            assert!(path.exists(), "expected {} to exist", path.display());
        }

        let _ = fs::remove_dir_all(root);
    }
}
