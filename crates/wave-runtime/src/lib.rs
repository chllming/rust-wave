//! Runtime execution helpers for the Wave workspace.
//!
//! The crate owns file-backed launch, rerun, draft, and replay data plumbing
//! that the CLI and operator surfaces build on. Runtime state stays rooted
//! under the project-scoped paths declared in `wave.toml`, and launched agents
//! persist structured result envelopes through the `wave-results` boundary.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fmt;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::ExitStatus;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use wave_config::ProjectConfig;
use wave_control_plane::PlanningStatus;
use wave_domain::AttemptId;
use wave_domain::AttemptRecord;
use wave_domain::AttemptState;
use wave_domain::ClosureDisposition;
use wave_domain::ControlEventPayload;
use wave_domain::RerunRequest;
use wave_domain::RerunRequestId;
use wave_domain::RerunState;
use wave_domain::ResultEnvelope;
use wave_domain::RuntimeExecutionIdentity;
use wave_domain::RuntimeExecutionRecord;
use wave_domain::RuntimeFallbackRecord;
use wave_domain::RuntimeId;
use wave_domain::RuntimeSelectionPolicy;
use wave_domain::RuntimeSkillProjection;
use wave_domain::RerunScope;
use wave_domain::SchedulerBudget;
use wave_domain::SchedulerBudgetId;
use wave_domain::SchedulerBudgetRecord;
use wave_domain::SchedulerEventPayload;
use wave_domain::SchedulerOwner;
use wave_domain::TaskLeaseId;
use wave_domain::TaskLeaseRecord;
use wave_domain::TaskLeaseState;
use wave_domain::WaveClaimId;
use wave_domain::WaveClaimRecord;
use wave_domain::WaveClaimState;
use wave_domain::WaveClosureOverrideRecord;
use wave_domain::WaveClosureOverrideStatus;
use wave_domain::WaveExecutionPhase;
use wave_domain::WavePromotionId;
use wave_domain::WavePromotionRecord;
use wave_domain::WavePromotionState;
use wave_domain::WaveSchedulerPriority;
use wave_domain::WaveSchedulingRecord;
use wave_domain::WaveSchedulingState;
use wave_domain::WaveWorktreeId;
use wave_domain::WaveWorktreeRecord;
use wave_domain::WaveWorktreeScope;
use wave_domain::WaveWorktreeState;
use wave_domain::runtime_selection_policy_for_agent;
use wave_domain::task_id_for_agent;
use wave_events::ControlEvent;
use wave_events::ControlEventKind;
use wave_events::ControlEventLog;
use wave_events::SchedulerEvent;
use wave_events::SchedulerEventKind;
use wave_events::SchedulerEventLog;
use wave_results::ResultEnvelopeStore;
use wave_results::build_structured_result_envelope;
use wave_results::closure_contract_error as result_closure_contract_error;
use wave_spec::WaveAgent;
use wave_spec::WaveDocument;
use wave_trace::AgentRunRecord;
use wave_trace::CompiledAgentPrompt;
use wave_trace::DraftBundle;
use wave_trace::WaveRunRecord;
use wave_trace::WaveRunStatus;
use wave_trace::load_latest_run_records_by_wave;
use wave_trace::load_run_record;
use wave_trace::now_epoch_ms;
use wave_trace::write_run_record;
use wave_trace::write_trace_bundle;

/// Stable label for the runtime landing zone.
pub const RUNTIME_LANDING_ZONE: &str = "launch-and-replay-bootstrap";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchReport {
    pub run_id: String,
    pub wave_id: u32,
    pub status: WaveRunStatus,
    pub state_path: PathBuf,
    pub trace_path: PathBuf,
    pub bundle_dir: PathBuf,
    pub preflight_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TraceInspectionReport {
    pub wave_id: u32,
    pub run_id: String,
    pub trace_path: PathBuf,
    pub recorded: bool,
    pub replay: wave_trace::ReplayReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DogfoodEvidenceReport {
    pub wave_id: u32,
    pub run_id: String,
    pub trace_path: PathBuf,
    pub recorded: bool,
    pub replay: wave_trace::ReplayReport,
    pub operator_help_required: bool,
    pub help_items: Vec<wave_trace::SelfHostEvidenceItem>,
    pub worktree: Option<WaveWorktreeRecord>,
    pub promotion: Option<WavePromotionRecord>,
    pub scheduling: Option<WaveSchedulingRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutonomousWaveSelection {
    pub wave_id: u32,
    pub slug: String,
    pub title: String,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FifoOrderedClaimableWave {
    selection: AutonomousWaveSelection,
    claimable_order: usize,
    waiting_since_ms: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchOptions {
    pub wave_id: Option<u32>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutonomousOptions {
    pub limit: Option<usize>,
    pub dry_run: bool,
}

const DEFAULT_LEASE_HEARTBEAT_INTERVAL_MS: u64 = 5_000;
const DEFAULT_LEASE_TTL_MS: u64 = 20_000;
const DEFAULT_AGENT_POLL_INTERVAL_MS: u64 = 250;
const DEFAULT_RUNTIME_CHECK_TIMEOUT_MS: u64 = 2_000;
const DEFAULT_RUNTIME_CHECK_POLL_INTERVAL_MS: u64 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LeaseTiming {
    heartbeat_interval_ms: u64,
    ttl_ms: u64,
    poll_interval_ms: u64,
}

impl Default for LeaseTiming {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: DEFAULT_LEASE_HEARTBEAT_INTERVAL_MS,
            ttl_ms: DEFAULT_LEASE_TTL_MS,
            poll_interval_ms: DEFAULT_AGENT_POLL_INTERVAL_MS,
        }
    }
}

#[derive(Debug, Clone)]
struct ExecutedAgent {
    record: AgentRunRecord,
    lease: TaskLeaseRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeAvailability {
    pub runtime: RuntimeId,
    pub binary: String,
    pub available: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeBoundaryStatus {
    pub executor_boundary: &'static str,
    pub selection_policy: &'static str,
    pub fallback_policy: &'static str,
    pub runtimes: Vec<RuntimeAvailability>,
}

#[derive(Debug, Clone)]
struct ResolvedRuntimePlan {
    runtime: RuntimeExecutionRecord,
    launch: RuntimeLaunchSpec,
    adapter_config: RuntimeAdapterConfig,
}

#[derive(Debug)]
struct SpawnedRuntimeChild {
    child: Child,
    failure_label: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeLaunchSpec {
    agent_id: String,
    execution_root: PathBuf,
    prompt: String,
    last_message_path: PathBuf,
    events_path: PathBuf,
    stderr_path: PathBuf,
    projected_skill_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeAdapterConfig {
    Codex(CodexAdapterConfig),
    Claude(ClaudeAdapterConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexAdapterConfig {
    model: Option<String>,
    config_entries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClaudeAdapterConfig {
    model: Option<String>,
    agent: Option<String>,
    permission_mode: Option<String>,
    permission_prompt_tool: Option<String>,
    effort: Option<String>,
    max_turns: Option<String>,
    mcp_config_paths: Vec<PathBuf>,
    strict_mcp_config: bool,
    output_format: Option<String>,
    allowed_tools: Vec<String>,
    disallowed_tools: Vec<String>,
    system_prompt_path: PathBuf,
    settings_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeAdapterInvocation {
    Codex(CodexInvocation),
    Claude(ClaudeInvocation),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexInvocation {
    model: Option<String>,
    config_entries: Vec<String>,
    codex_home: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClaudeInvocation {
    model: Option<String>,
    agent: Option<String>,
    permission_mode: Option<String>,
    permission_prompt_tool: Option<String>,
    effort: Option<String>,
    max_turns: Option<String>,
    mcp_config_paths: Vec<PathBuf>,
    strict_mcp_config: bool,
    output_format: Option<String>,
    allowed_tools: Vec<String>,
    disallowed_tools: Vec<String>,
    system_prompt_path: PathBuf,
    settings_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RuntimeExecutionRequest {
    identity: RuntimeExecutionIdentity,
    launch: RuntimeLaunchSpec,
    invocation: RuntimeAdapterInvocation,
}

trait RuntimeAdapter {
    fn availability(&self) -> RuntimeAvailability;
    fn execute(&self, request: RuntimeExecutionRequest) -> Result<SpawnedRuntimeChild>;
}

#[derive(Debug, Clone)]
struct CodexRuntimeAdapter {
    binary: String,
}

#[derive(Debug, Clone)]
struct ClaudeRuntimeAdapter {
    binary: String,
}

#[derive(Debug, Clone)]
struct RuntimeAdapterRegistry {
    codex: CodexRuntimeAdapter,
    claude: ClaudeRuntimeAdapter,
}

#[derive(Debug, Deserialize)]
struct SkillManifest {
    activation: Option<SkillActivation>,
}

#[derive(Debug, Deserialize)]
struct SkillActivation {
    #[serde(default)]
    runtimes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RuntimeDetailSnapshot {
    wave_id: u32,
    run_id: String,
    agent_id: String,
    agent_title: String,
    runtime: RuntimeExecutionRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutonomousQueueDecision {
    pub selected: Option<AutonomousWaveSelection>,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RerunIntentStatus {
    Requested,
    Cleared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RerunIntentRecord {
    #[serde(default)]
    pub request_id: Option<String>,
    pub wave_id: u32,
    pub reason: String,
    pub requested_by: String,
    #[serde(default)]
    pub scope: RerunScope,
    pub status: RerunIntentStatus,
    pub requested_at_ms: u128,
    pub cleared_at_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchPreflightCheck {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchPreflightDiagnostic {
    pub contract: &'static str,
    pub required: bool,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchRefusal {
    pub wave_id: u32,
    pub wave_slug: String,
    pub detail: String,
    pub failed_contracts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchPreflightReport {
    pub wave_id: u32,
    pub wave_slug: String,
    pub dry_run: bool,
    pub ok: bool,
    pub checks: Vec<LaunchPreflightCheck>,
    pub diagnostics: Vec<LaunchPreflightDiagnostic>,
    pub refusal: Option<LaunchRefusal>,
}

#[derive(Debug)]
pub struct LaunchPreflightError {
    pub report: LaunchPreflightReport,
}

impl fmt::Display for LaunchPreflightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let refusal = self
            .report
            .refusal
            .as_ref()
            .map(|refusal| refusal.detail.as_str())
            .unwrap_or("launch preflight failed");
        write!(f, "{refusal}")
    }
}

impl std::error::Error for LaunchPreflightError {}

impl LaunchPreflightError {
    pub fn report(&self) -> &LaunchPreflightReport {
        &self.report
    }
}

#[derive(Debug)]
struct SchedulerAdmissionError {
    wave_id: u32,
    detail: String,
}

impl fmt::Display for SchedulerAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wave {} is not claimable: {}", self.wave_id, self.detail)
    }
}

impl std::error::Error for SchedulerAdmissionError {}

#[derive(Debug)]
struct TaskLeaseCapacityError {
    wave_id: u32,
    task_id: String,
    fairness_rank: u32,
    protected_closure_capacity: bool,
    detail: String,
}

impl fmt::Display for TaskLeaseCapacityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "wave {} task {} is waiting for scheduler capacity: {}",
            self.wave_id, self.task_id, self.detail
        )
    }
}

impl std::error::Error for TaskLeaseCapacityError {}

#[derive(Debug)]
struct LeaseRevokedError {
    wave_id: u32,
    lease_id: String,
    detail: String,
}

impl fmt::Display for LeaseRevokedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "wave {} lease {} was revoked: {}",
            self.wave_id, self.lease_id, self.detail
        )
    }
}

impl std::error::Error for LeaseRevokedError {}

#[derive(Debug, Clone, Default)]
struct RuntimeSchedulerSnapshot {
    budget: SchedulerBudget,
    latest_leases: HashMap<TaskLeaseId, TaskLeaseRecord>,
    active_leases: Vec<TaskLeaseRecord>,
    scheduling_by_wave: HashMap<u32, WaveSchedulingRecord>,
    active_implementation_task_leases: usize,
    active_closure_task_leases: usize,
    waiting_closure_waves: usize,
}

pub fn codex_binary_available() -> bool {
    Command::new(resolved_codex_binary())
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn claude_binary_available() -> bool {
    Command::new(resolved_claude_binary())
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn resolved_codex_binary() -> String {
    env::var("WAVE_CODEX_BIN").unwrap_or_else(|_| "codex".to_string())
}

fn resolved_claude_binary() -> String {
    env::var("WAVE_CLAUDE_BIN").unwrap_or_else(|_| "claude".to_string())
}

pub fn runtime_boundary_status() -> RuntimeBoundaryStatus {
    let registry = RuntimeAdapterRegistry::new();
    RuntimeBoundaryStatus {
        executor_boundary: "runtime-neutral launch spec plus adapter registry in wave-runtime",
        selection_policy: "explicit executor runtime selection with default codex and authored fallback order",
        fallback_policy: "fallback only when the selected runtime is unavailable before meaningful work starts",
        runtimes: registry.availability_report(),
    }
}

impl RuntimeAdapterRegistry {
    fn new() -> Self {
        Self {
            codex: CodexRuntimeAdapter {
                binary: resolved_codex_binary(),
            },
            claude: ClaudeRuntimeAdapter {
                binary: resolved_claude_binary(),
            },
        }
    }

    fn adapter(&self, runtime: RuntimeId) -> Result<&dyn RuntimeAdapter> {
        match runtime {
            RuntimeId::Codex => Ok(&self.codex),
            RuntimeId::Claude => Ok(&self.claude),
            other => bail!("runtime {other} is not implemented in Wave 15"),
        }
    }

    fn availability_report(&self) -> Vec<RuntimeAvailability> {
        let mut runtimes = vec![self.codex.availability(), self.claude.availability()];
        runtimes.sort_by_key(|entry| entry.runtime);
        runtimes
    }
}

impl RuntimeAdapter for CodexRuntimeAdapter {
    fn availability(&self) -> RuntimeAvailability {
        runtime_availability_from_checks(
            RuntimeId::Codex,
            self.binary.clone(),
            vec![
                runtime_check(self.binary.as_str(), &["--version"], |status, _, _| status),
                runtime_check(
                    self.binary.as_str(),
                    &["login", "status"],
                    |status, stdout, stderr| {
                        status && (stdout.contains("Logged in") || stderr.contains("Logged in"))
                    },
                ),
            ],
        )
    }

    fn execute(&self, request: RuntimeExecutionRequest) -> Result<SpawnedRuntimeChild> {
        let RuntimeAdapterInvocation::Codex(invocation) = request.invocation else {
            bail!("codex adapter received a non-codex invocation");
        };
        let stdout = File::create(&request.launch.events_path).with_context(|| {
            format!("failed to create {}", request.launch.events_path.display())
        })?;
        let stderr = File::create(&request.launch.stderr_path).with_context(|| {
            format!("failed to create {}", request.launch.stderr_path.display())
        })?;

        let mut command = Command::new(&self.binary);
        command
            .arg("exec")
            .arg("--json")
            .arg("--skip-git-repo-check")
            .arg("--dangerously-bypass-approvals-and-sandbox")
            .arg("--color")
            .arg("never")
            .arg("-C")
            .arg(&request.launch.execution_root)
            .arg("-o")
            .arg(&request.launch.last_message_path);

        if let Some(model) = invocation.model {
            command.arg("--model").arg(model);
        }
        for entry in invocation.config_entries {
            command.arg("-c").arg(entry);
        }
        for add_dir in &request.launch.projected_skill_dirs {
            command.arg("--add-dir").arg(add_dir);
        }

        command
            .env("CODEX_HOME", &invocation.codex_home)
            .env("CODEX_SQLITE_HOME", &invocation.codex_home)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));

        let mut child = command.spawn().context("failed to start codex exec")?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(request.launch.prompt.as_bytes())
                .context("failed to write prompt to codex exec stdin")?;
        }

        Ok(SpawnedRuntimeChild {
            child,
            failure_label: "codex exec",
        })
    }
}

impl RuntimeAdapter for ClaudeRuntimeAdapter {
    fn availability(&self) -> RuntimeAvailability {
        runtime_availability_from_checks(
            RuntimeId::Claude,
            self.binary.clone(),
            vec![
                runtime_check(self.binary.as_str(), &["--version"], |status, _, _| status),
                runtime_check(
                    self.binary.as_str(),
                    &["auth", "status", "--json"],
                    |status, stdout, _| {
                        status
                            && serde_json::from_str::<JsonValue>(stdout)
                                .ok()
                                .and_then(|value| {
                                    value.get("loggedIn").and_then(JsonValue::as_bool)
                                })
                                .unwrap_or(false)
                    },
                ),
            ],
        )
    }

    fn execute(&self, request: RuntimeExecutionRequest) -> Result<SpawnedRuntimeChild> {
        let RuntimeAdapterInvocation::Claude(invocation) = request.invocation else {
            bail!("claude adapter received a non-claude invocation");
        };
        let stdout = File::create(&request.launch.last_message_path).with_context(|| {
            format!(
                "failed to create {}",
                request.launch.last_message_path.display()
            )
        })?;
        let stderr = File::create(&request.launch.stderr_path).with_context(|| {
            format!("failed to create {}", request.launch.stderr_path.display())
        })?;
        write_claude_event(
            &request.launch.events_path,
            serde_json::json!({
                "event": "spawn",
                "runtime": request.identity.runtime.as_str(),
                "agent": request.launch.agent_id,
            }),
        )?;

        let mut command = Command::new(&self.binary);
        command
            .current_dir(&request.launch.execution_root)
            .arg("-p")
            .arg("--no-session-persistence")
            .arg("--append-system-prompt-file")
            .arg(&invocation.system_prompt_path)
            .arg(&request.launch.prompt)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));

        if let Some(model) = invocation.model {
            command.arg("--model").arg(model);
        }
        if let Some(value) = invocation.agent {
            command.arg("--agent").arg(value);
        }
        if let Some(value) = invocation.permission_mode {
            command.arg("--permission-mode").arg(value);
        }
        if let Some(value) = invocation.permission_prompt_tool {
            command.arg("--permission-prompt-tool").arg(value);
        }
        if let Some(value) = invocation.effort {
            command.arg("--effort").arg(value);
        }
        if let Some(value) = invocation.max_turns {
            command.arg("--max-turns").arg(value);
        }
        for path in invocation.mcp_config_paths {
            command.arg("--mcp-config").arg(path);
        }
        if invocation.strict_mcp_config {
            command.arg("--strict-mcp-config");
        }
        if let Some(value) = invocation.output_format {
            command.arg("--output-format").arg(value);
        }
        for tool in invocation.allowed_tools {
            command.arg("--allowedTools").arg(tool);
        }
        for tool in invocation.disallowed_tools {
            command.arg("--disallowedTools").arg(tool);
        }
        if let Some(settings_path) = invocation.settings_path {
            command.arg("--settings").arg(settings_path);
        }
        for add_dir in &request.launch.projected_skill_dirs {
            command.arg("--add-dir").arg(add_dir);
        }

        let child = command.spawn().context("failed to start claude -p")?;
        Ok(SpawnedRuntimeChild {
            child,
            failure_label: "claude -p",
        })
    }
}

fn runtime_availability_from_checks(
    runtime: RuntimeId,
    binary: String,
    checks: Vec<(bool, String)>,
) -> RuntimeAvailability {
    if let Some((_, detail)) = checks.iter().find(|(ok, _)| !*ok) {
        return RuntimeAvailability {
            runtime,
            binary,
            available: false,
            detail: detail.clone(),
        };
    }
    RuntimeAvailability {
        runtime,
        binary,
        available: true,
        detail: "available".to_string(),
    }
}

fn runtime_check(
    binary: &str,
    args: &[&str],
    predicate: impl Fn(bool, &str, &str) -> bool,
) -> (bool, String) {
    runtime_check_with_timeout(
        binary,
        args,
        Duration::from_millis(DEFAULT_RUNTIME_CHECK_TIMEOUT_MS),
        predicate,
    )
}

fn runtime_check_with_timeout(
    binary: &str,
    args: &[&str],
    timeout: Duration,
    predicate: impl Fn(bool, &str, &str) -> bool,
) -> (bool, String) {
    let mut child = match Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return (
                false,
                format!("{} {} failed to start: {error}", binary, args.join(" ")),
            );
        }
    };
    let stdout_handle = child.stdout.take().map(spawn_runtime_pipe_reader);
    let stderr_handle = child.stderr.take().map(spawn_runtime_pipe_reader);

    let started_at = std::time::Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {
                if started_at.elapsed() >= timeout {
                    timed_out = true;
                    let _ = child.kill();
                    break child.wait();
                }
                thread::sleep(Duration::from_millis(
                    DEFAULT_RUNTIME_CHECK_POLL_INTERVAL_MS,
                ));
            }
            Err(error) => break Err(error),
        }
    };

    let stdout = join_runtime_pipe_reader(stdout_handle);
    let stderr = join_runtime_pipe_reader(stderr_handle);
    if timed_out {
        return (
            false,
            format!(
                "{} {} timed out after {}ms",
                binary,
                args.join(" "),
                timeout.as_millis()
            ),
        );
    }

    match status {
        Ok(status) => {
            let ok = predicate(status.success(), stdout.as_str(), stderr.as_str());
            let detail = if ok {
                format!("ok: {} {}", binary, args.join(" "))
            } else if status.success() {
                format!("{} {} reported unavailable", binary, args.join(" "))
            } else if stderr.is_empty() {
                format!("{} {} failed", binary, args.join(" "))
            } else {
                format!("{} {} failed: {}", binary, args.join(" "), stderr)
            };
            (ok, detail)
        }
        Err(error) => (
            false,
            format!(
                "{} {} failed while waiting: {error}",
                binary,
                args.join(" ")
            ),
        ),
    }
}

fn spawn_runtime_pipe_reader<T>(mut pipe: T) -> thread::JoinHandle<String>
where
    T: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = String::new();
        let _ = pipe.read_to_string(&mut buffer);
        buffer.trim().to_string()
    })
}

fn join_runtime_pipe_reader(handle: Option<thread::JoinHandle<String>>) -> String {
    handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default()
}

fn write_claude_event(path: &Path, value: JsonValue) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to reset {}", path.display()))?;
    }
    append_json_event(path, value)
}

fn append_json_event(path: &Path, value: JsonValue) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{}", serde_json::to_string(&value)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn artifact_path_from_runtime(runtime: &RuntimeExecutionRecord, key: &str) -> Option<String> {
    runtime.execution_identity.artifact_paths.get(key).cloned()
}

fn projected_skill_dirs(execution_root: &Path, runtime: &RuntimeExecutionRecord) -> Vec<PathBuf> {
    runtime
        .skill_projection
        .projected_skills
        .iter()
        .map(|skill| execution_root.join("skills").join(skill))
        .filter(|path| path.exists())
        .collect()
}

fn parse_list_value(raw: &str) -> Vec<String> {
    raw.split([',', '\n', '\t'])
        .flat_map(|entry| entry.split_whitespace())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parse_truthy_flag(value: Option<&String>) -> bool {
    value
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

pub fn load_latest_runs(
    root: &Path,
    config: &ProjectConfig,
) -> Result<HashMap<u32, WaveRunRecord>> {
    load_latest_run_records_by_wave(&state_runs_dir(root, config))
}

pub fn load_relevant_runs(
    root: &Path,
    config: &ProjectConfig,
) -> Result<HashMap<u32, WaveRunRecord>> {
    let runs_dir = state_runs_dir(root, config);
    let mut relevant = HashMap::new();
    if !runs_dir.exists() {
        return Ok(relevant);
    }

    for entry in
        fs::read_dir(&runs_dir).with_context(|| format!("failed to read {}", runs_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", runs_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let record = load_run_record(&path)?;
        match relevant.get(&record.wave_id) {
            Some(current) if !is_more_relevant_run(&record, current) => {}
            _ => {
                relevant.insert(record.wave_id, record);
            }
        }
    }

    Ok(relevant)
}

pub fn list_rerun_intents(
    root: &Path,
    config: &ProjectConfig,
) -> Result<HashMap<u32, RerunIntentRecord>> {
    let reruns_dir = control_reruns_dir(root, config);
    let mut intents = HashMap::new();
    if !reruns_dir.exists() {
        return Ok(intents);
    }

    for entry in fs::read_dir(&reruns_dir)
        .with_context(|| format!("failed to read {}", reruns_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", reruns_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read rerun intent {}", path.display()))?;
        let record = serde_json::from_str::<RerunIntentRecord>(&raw)
            .with_context(|| format!("failed to parse rerun intent {}", path.display()))?;
        intents.insert(record.wave_id, record);
    }

    Ok(intents)
}

pub fn pending_rerun_wave_ids(root: &Path, config: &ProjectConfig) -> Result<HashSet<u32>> {
    Ok(list_rerun_intents(root, config)?
        .into_values()
        .filter(|record| record.status == RerunIntentStatus::Requested)
        .map(|record| record.wave_id)
        .collect())
}

pub fn list_closure_overrides(
    root: &Path,
    config: &ProjectConfig,
) -> Result<HashMap<u32, WaveClosureOverrideRecord>> {
    let overrides_dir = control_closure_overrides_dir(root, config);
    let mut overrides = HashMap::new();
    if !overrides_dir.exists() {
        return Ok(overrides);
    }

    for entry in fs::read_dir(&overrides_dir)
        .with_context(|| format!("failed to read {}", overrides_dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("failed to read entry in {}", overrides_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read closure override {}", path.display()))?;
        let record = serde_json::from_str::<WaveClosureOverrideRecord>(&raw)
            .with_context(|| format!("failed to parse closure override {}", path.display()))?;
        overrides.insert(record.wave_id, record);
    }

    Ok(overrides)
}

pub fn load_closure_override(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
) -> Result<Option<WaveClosureOverrideRecord>> {
    let mut overrides = list_closure_overrides(root, config)?;
    Ok(overrides.remove(&wave_id))
}

pub fn active_closure_override_wave_ids(
    root: &Path,
    config: &ProjectConfig,
) -> Result<HashSet<u32>> {
    Ok(list_closure_overrides(root, config)?
        .into_values()
        .filter(WaveClosureOverrideRecord::is_active)
        .map(|record| record.wave_id)
        .collect())
}

pub fn latest_trace_reports(
    root: &Path,
    config: &ProjectConfig,
) -> Result<HashMap<u32, TraceInspectionReport>> {
    let latest_runs = load_latest_runs(root, config)?;
    let mut reports = HashMap::new();
    for (wave_id, record) in latest_runs {
        reports.insert(wave_id, trace_inspection_report(&record));
    }
    Ok(reports)
}

fn is_more_relevant_run(candidate: &WaveRunRecord, current: &WaveRunRecord) -> bool {
    (
        relevance_rank(candidate.status),
        candidate.created_at_ms,
        candidate.started_at_ms.unwrap_or_default(),
        candidate.completed_at_ms.unwrap_or_default(),
    ) > (
        relevance_rank(current.status),
        current.created_at_ms,
        current.started_at_ms.unwrap_or_default(),
        current.completed_at_ms.unwrap_or_default(),
    )
}

fn relevance_rank(status: WaveRunStatus) -> u8 {
    match status {
        WaveRunStatus::Running | WaveRunStatus::Planned => 3,
        WaveRunStatus::Succeeded | WaveRunStatus::Failed => 2,
        WaveRunStatus::DryRun => 1,
    }
}

pub fn trace_inspection_report(record: &WaveRunRecord) -> TraceInspectionReport {
    let recorded = record.trace_path.exists();
    let replay = wave_trace::validate_replay(record);
    TraceInspectionReport {
        wave_id: record.wave_id,
        run_id: record.run_id.clone(),
        trace_path: record.trace_path.clone(),
        recorded,
        replay,
    }
}

pub fn dogfood_evidence_report(record: &WaveRunRecord) -> DogfoodEvidenceReport {
    let evidence = wave_trace::self_host_evidence(record);
    DogfoodEvidenceReport {
        wave_id: evidence.wave_id,
        run_id: evidence.run_id,
        trace_path: record.trace_path.clone(),
        recorded: evidence.recorded,
        replay: evidence.replay,
        operator_help_required: evidence.operator_help_required,
        help_items: evidence.help_items,
        worktree: record.worktree.clone(),
        promotion: record.promotion.clone(),
        scheduling: record.scheduling.clone(),
    }
}

pub fn request_rerun(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
    reason: impl Into<String>,
    scope: RerunScope,
) -> Result<RerunIntentRecord> {
    let requested_at_ms = now_epoch_ms()?;
    let record = RerunIntentRecord {
        request_id: Some(format!("rerun-wave-{wave_id:02}-{requested_at_ms}")),
        wave_id,
        reason: reason.into(),
        requested_by: "operator".to_string(),
        scope,
        status: RerunIntentStatus::Requested,
        requested_at_ms,
        cleared_at_ms: None,
    };
    write_rerun_intent(root, config, &record)?;
    append_control_event(
        root,
        config,
        ControlEvent::new(
            format!("evt-rerun-requested-{wave_id:02}-{requested_at_ms}"),
            ControlEventKind::RerunRequested,
            wave_id,
        )
        .with_created_at_ms(requested_at_ms)
        .with_correlation_id(format!("rerun-wave-{wave_id:02}"))
        .with_payload(ControlEventPayload::RerunRequested {
            rerun: rerun_request_payload(&record, RerunState::Requested),
        }),
    )?;
    Ok(record)
}

pub fn clear_rerun(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
) -> Result<Option<RerunIntentRecord>> {
    clear_rerun_with_state(root, config, wave_id, RerunState::Cancelled)
}

pub fn apply_closure_override(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
    reason: impl Into<String>,
    source_run_id: Option<&str>,
    evidence_paths: Vec<String>,
    detail: Option<String>,
) -> Result<WaveClosureOverrideRecord> {
    let reason = reason.into();
    if reason.trim().is_empty() {
        bail!("closure override reason must not be empty");
    }

    let latest_runs = load_latest_runs(root, config)?;
    if latest_runs
        .get(&wave_id)
        .map(|run| run.status)
        .is_some_and(|status| matches!(status, WaveRunStatus::Running | WaveRunStatus::Planned))
    {
        bail!("wave {wave_id} has an active run; clear it before applying a closure override");
    }

    let source_run_id = match source_run_id {
        Some(source_run_id) => {
            let run_path = state_runs_dir(root, config).join(format!("{source_run_id}.json"));
            if !run_path.exists() {
                bail!("source run {source_run_id} does not exist");
            }
            let run = load_run_record(&run_path)?;
            if run.wave_id != wave_id {
                bail!("source run {source_run_id} belongs to wave {}", run.wave_id);
            }
            if matches!(run.status, WaveRunStatus::Running | WaveRunStatus::Planned) {
                bail!("source run {source_run_id} is still active");
            }
            source_run_id.to_string()
        }
        None => latest_runs
            .get(&wave_id)
            .filter(|run| !matches!(run.status, WaveRunStatus::Running | WaveRunStatus::Planned))
            .map(|run| run.run_id.clone())
            .with_context(|| format!("wave {wave_id} has no terminal run to use as override evidence"))?,
    };

    let applied_at_ms = now_epoch_ms()?;
    let record = WaveClosureOverrideRecord {
        override_id: format!("closure-override-wave-{wave_id:02}-{applied_at_ms}"),
        wave_id,
        status: WaveClosureOverrideStatus::Applied,
        reason,
        requested_by: "operator".to_string(),
        source_run_id,
        evidence_paths,
        detail,
        applied_at_ms,
        cleared_at_ms: None,
    };
    write_closure_override(root, config, &record)?;
    let _ = clear_rerun_with_state(root, config, wave_id, RerunState::Cancelled);
    append_control_event(
        root,
        config,
        ControlEvent::new(
            format!("evt-closure-override-applied-{wave_id:02}-{applied_at_ms}"),
            ControlEventKind::ClosureOverrideApplied,
            wave_id,
        )
        .with_created_at_ms(applied_at_ms)
        .with_correlation_id(record.override_id.clone())
        .with_payload(ControlEventPayload::ClosureOverrideUpdated {
            closure_override: record.clone(),
        }),
    )?;
    Ok(record)
}

pub fn clear_closure_override(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
) -> Result<Option<WaveClosureOverrideRecord>> {
    let mut overrides = list_closure_overrides(root, config)?;
    let Some(mut record) = overrides.remove(&wave_id) else {
        return Ok(None);
    };
    if !record.is_active() {
        return Ok(Some(record));
    }
    let cleared_at_ms = now_epoch_ms()?;
    record.status = WaveClosureOverrideStatus::Cleared;
    record.cleared_at_ms = Some(cleared_at_ms);
    write_closure_override(root, config, &record)?;
    append_control_event(
        root,
        config,
        ControlEvent::new(
            format!("evt-closure-override-cleared-{wave_id:02}-{cleared_at_ms}"),
            ControlEventKind::ClosureOverrideCleared,
            wave_id,
        )
        .with_created_at_ms(cleared_at_ms)
        .with_correlation_id(record.override_id.clone())
        .with_payload(ControlEventPayload::ClosureOverrideUpdated {
            closure_override: record.clone(),
        }),
    )?;
    Ok(Some(record))
}

fn clear_rerun_with_state(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
    rerun_state: RerunState,
) -> Result<Option<RerunIntentRecord>> {
    let mut intents = list_rerun_intents(root, config)?;
    let Some(mut record) = intents.remove(&wave_id) else {
        return Ok(None);
    };
    record.status = RerunIntentStatus::Cleared;
    let cleared_at_ms = now_epoch_ms()?;
    record.cleared_at_ms = Some(cleared_at_ms);
    write_rerun_intent(root, config, &record)?;
    append_control_event(
        root,
        config,
        ControlEvent::new(
            format!("evt-rerun-cleared-{wave_id:02}-{cleared_at_ms}"),
            ControlEventKind::RerunCleared,
            wave_id,
        )
        .with_created_at_ms(cleared_at_ms)
        .with_correlation_id(format!("rerun-wave-{wave_id:02}"))
        .with_payload(ControlEventPayload::RerunRequested {
            rerun: rerun_request_payload(&record, rerun_state),
        }),
    )?;
    Ok(Some(record))
}

pub fn select_wave<'a>(
    waves: &'a [WaveDocument],
    status: &PlanningStatus,
    requested_wave_id: Option<u32>,
) -> Result<&'a WaveDocument> {
    if let Some(wave_id) = requested_wave_id {
        let wave = waves
            .iter()
            .find(|wave| wave.metadata.id == wave_id)
            .with_context(|| format!("unknown wave {}", wave_id))?;
        let entry = status
            .waves
            .iter()
            .find(|entry| entry.id == wave_id)
            .with_context(|| format!("missing status entry for wave {}", wave_id))?;
        if !is_claimable_wave(status, wave_id) {
            bail!(
                "wave {} is not claimable: {}",
                wave_id,
                queue_entry_reason(entry)
            );
        }
        return Ok(wave);
    }

    let Some(wave_id) = next_claimable_wave_id(status) else {
        bail!("{}", queue_unavailable_reason(status));
    };
    waves
        .iter()
        .find(|wave| wave.metadata.id == wave_id)
        .with_context(|| format!("missing wave definition for ready wave {}", wave_id))
}

pub fn compile_wave_bundle(
    root: &Path,
    config: &ProjectConfig,
    wave: &WaveDocument,
    run_id: &str,
) -> Result<DraftBundle> {
    bootstrap_authority_roots(root, config)?;
    let bundle_dir = build_specs_dir(root, config).join(run_id);
    let agents_dir = bundle_dir.join("agents");
    fs::create_dir_all(&agents_dir)
        .with_context(|| format!("failed to create {}", agents_dir.display()))?;

    let ordered_agents = ordered_agents(wave);
    let mut agents = Vec::new();
    for agent in &ordered_agents {
        let agent_dir = agents_dir.join(&agent.id);
        fs::create_dir_all(&agent_dir)
            .with_context(|| format!("failed to create {}", agent_dir.display()))?;
        let prompt_path = agent_dir.join("prompt.md");
        let prompt = render_agent_prompt(root, wave, agent, &ordered_agents);
        fs::write(&prompt_path, prompt)
            .with_context(|| format!("failed to write {}", prompt_path.display()))?;
        agents.push(CompiledAgentPrompt {
            id: agent.id.clone(),
            title: agent.title.clone(),
            prompt_path,
            expected_markers: agent
                .expected_final_markers()
                .iter()
                .map(|marker| (*marker).to_string())
                .collect(),
        });
    }

    let bundle = DraftBundle {
        run_id: run_id.to_string(),
        wave_id: wave.metadata.id,
        slug: wave.metadata.slug.clone(),
        title: wave.metadata.title.clone(),
        bundle_dir: bundle_dir.clone(),
        agents,
    };
    let manifest_path = bundle_dir.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_string_pretty(&bundle)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    Ok(bundle)
}

pub fn draft_wave(
    root: &Path,
    waves: &[WaveDocument],
    status: &PlanningStatus,
    wave_id: Option<u32>,
) -> Result<DraftBundle> {
    let config = ProjectConfig::load_from_repo_root(root)?;
    let wave = select_wave(waves, status, wave_id)?;
    let run_id = format!("wave-{:02}-{}", wave.metadata.id, now_epoch_ms()?);
    compile_wave_bundle(root, &config, wave, &run_id)
}

fn requested_rerun_intent(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
) -> Result<Option<RerunIntentRecord>> {
    Ok(list_rerun_intents(root, config)?
        .remove(&wave_id)
        .filter(|record| record.status == RerunIntentStatus::Requested))
}

fn planned_execution_indices(
    ordered_agents: &[&WaveAgent],
    prior_run: Option<&WaveRunRecord>,
    scope: RerunScope,
) -> Result<Vec<usize>> {
    match scope {
        RerunScope::Full => Ok((0..ordered_agents.len()).collect()),
        RerunScope::FromFirstIncomplete => {
            let prior_run = prior_run.context("from-first-incomplete rerun requires a prior run")?;
            let Some(start_index) = ordered_agents.iter().position(|agent| {
                prior_run
                    .agents
                    .iter()
                    .find(|record| record.id == agent.id)
                    .map(|record| record.status != WaveRunStatus::Succeeded)
                    .unwrap_or(true)
            }) else {
                bail!("from-first-incomplete rerun found no incomplete agents to resume");
            };
            Ok((start_index..ordered_agents.len()).collect())
        }
        RerunScope::ClosureOnly => {
            let indices = ordered_agents
                .iter()
                .enumerate()
                .filter(|(_, agent)| is_closure_followup_agent(agent.id.as_str()))
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if indices.is_empty() {
                bail!("closure-only rerun requires declared closure follow-up agents");
            }
            Ok(indices)
        }
        RerunScope::PromotionOnly => {
            let indices = ordered_agents
                .iter()
                .enumerate()
                .filter(|(_, agent)| is_promotion_gated_closure_agent(agent.id.as_str()))
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if indices.is_empty() {
                bail!("promotion-only rerun requires promotion-gated closure agents");
            }
            Ok(indices)
        }
    }
}

fn seed_reused_agent_records(
    current_agents: &mut [AgentRunRecord],
    ordered_agents: &[&WaveAgent],
    prior_run: Option<&WaveRunRecord>,
    execution_indices: &[usize],
    scope: RerunScope,
) -> Result<()> {
    if matches!(scope, RerunScope::Full) {
        return Ok(());
    }
    let prior_run = prior_run.with_context(|| {
        format!(
            "{} rerun requires a prior successful frontier to reuse",
            rerun_scope_label(scope)
        )
    })?;
    let execution_indices = execution_indices.iter().copied().collect::<HashSet<_>>();
    for (index, agent) in ordered_agents.iter().enumerate() {
        if execution_indices.contains(&index) {
            continue;
        }
        let prior_agent = prior_run
            .agents
            .iter()
            .find(|record| record.id == agent.id)
            .with_context(|| {
                format!(
                    "{} rerun cannot skip {} because the prior run has no matching agent record",
                    rerun_scope_label(scope),
                    agent.id
                )
            })?;
        if prior_agent.status != WaveRunStatus::Succeeded {
            bail!(
                "{} rerun cannot skip {} because the prior run status was {}",
                rerun_scope_label(scope),
                agent.id,
                prior_agent.status
            );
        }
        current_agents[index] = prior_agent.clone();
    }
    Ok(())
}

fn prepare_closure_artifact_placeholders(execution_root: &Path, wave: &WaveDocument) -> Result<()> {
    for agent in wave
        .agents
        .iter()
        .filter(|agent| is_closure_followup_agent(agent.id.as_str()))
    {
        for owned_path in &agent.file_ownership {
            let Some(relative_path) = normalize_owned_relative_path(owned_path) else {
                continue;
            };
            let artifact_path = execution_root.join(&relative_path);
            if let Some(parent) = artifact_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            if artifact_path.exists() {
                continue;
            }
            let placeholder = if artifact_path.extension().and_then(|ext| ext.to_str())
                == Some("json")
            {
                "{}\n"
            } else {
                ""
            };
            fs::write(&artifact_path, placeholder)
                .with_context(|| format!("failed to seed {}", artifact_path.display()))?;
        }
    }
    Ok(())
}

fn normalize_owned_relative_path(path: &str) -> Option<PathBuf> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return None;
    }
    let normalized = candidate.components().collect::<PathBuf>();
    if normalized
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(normalized)
}

fn rerun_scope_label(scope: RerunScope) -> &'static str {
    match scope {
        RerunScope::Full => "full",
        RerunScope::FromFirstIncomplete => "from-first-incomplete",
        RerunScope::ClosureOnly => "closure-only",
        RerunScope::PromotionOnly => "promotion-only",
    }
}

pub fn launch_wave(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    status: &PlanningStatus,
    options: LaunchOptions,
) -> Result<LaunchReport> {
    if !options.dry_run {
        let _ = repair_orphaned_runs(root, config)?;
    }
    let runtime_registry = RuntimeAdapterRegistry::new();
    let planning_status = if options.dry_run || options.wave_id.is_some() {
        status.clone()
    } else {
        refresh_planning_status(root, config, waves)?
    };
    let wave = select_wave(waves, &planning_status, options.wave_id)?;
    let rerun_intent = requested_rerun_intent(root, config, wave.metadata.id)?;
    let rerun_scope = rerun_intent
        .as_ref()
        .map(|record| record.scope)
        .unwrap_or(RerunScope::Full);
    let prior_run = load_latest_runs(root, config)?.remove(&wave.metadata.id);
    let ordered_agents = ordered_agents(wave);
    let execution_indices = planned_execution_indices(&ordered_agents, prior_run.as_ref(), rerun_scope)?;
    let run_id = format!("wave-{:02}-{}", wave.metadata.id, now_epoch_ms()?);
    let bundle = compile_wave_bundle(root, config, wave, &run_id)?;
    let mut agent_records = bundle
        .agents
        .iter()
        .map(|agent| AgentRunRecord {
            id: agent.id.clone(),
            title: agent.title.clone(),
            status: WaveRunStatus::Planned,
            prompt_path: agent.prompt_path.clone(),
            last_message_path: agent.prompt_path.parent().unwrap().join("last-message.txt"),
            events_path: agent.prompt_path.parent().unwrap().join("events.jsonl"),
            stderr_path: agent.prompt_path.parent().unwrap().join("stderr.txt"),
            result_envelope_path: None,
            runtime_detail_path: None,
            expected_markers: agent.expected_markers.clone(),
            observed_markers: Vec::new(),
            exit_code: None,
            error: None,
            runtime: None,
        })
        .collect::<Vec<_>>();
    seed_reused_agent_records(
        &mut agent_records,
        &ordered_agents,
        prior_run.as_ref(),
        &execution_indices,
        rerun_scope,
    )?;
    let preflight = build_launch_preflight(wave, options.dry_run, &runtime_registry);
    let preflight_path = bundle.bundle_dir.join("preflight.json");
    fs::write(&preflight_path, serde_json::to_string_pretty(&preflight)?)
        .with_context(|| format!("failed to write {}", preflight_path.display()))?;
    if !preflight.ok {
        append_control_event(
            root,
            config,
            ControlEvent::new(
                format!(
                    "evt-launch-refused-{}-{}",
                    wave.metadata.id,
                    now_epoch_ms()?
                ),
                ControlEventKind::LaunchRefused,
                wave.metadata.id,
            )
            .with_created_at_ms(now_epoch_ms()?)
            .with_correlation_id(run_id.clone()),
        )?;
        return Err(LaunchPreflightError { report: preflight }.into());
    }

    let trace_path = trace_runs_dir(root, config).join(format!("{run_id}.json"));
    let state_path = state_runs_dir(root, config).join(format!("{run_id}.json"));

    if options.dry_run {
        return Ok(LaunchReport {
            run_id,
            wave_id: wave.metadata.id,
            status: WaveRunStatus::DryRun,
            state_path,
            trace_path,
            bundle_dir: bundle.bundle_dir,
            preflight_path,
        });
    }

    let codex_home = bootstrap_project_codex_home(root, config)?;
    fs::create_dir_all(trace_runs_dir(root, config)).with_context(|| {
        format!(
            "failed to create {}",
            trace_runs_dir(root, config).display()
        )
    })?;
    fs::create_dir_all(state_runs_dir(root, config)).with_context(|| {
        format!(
            "failed to create {}",
            state_runs_dir(root, config).display()
        )
    })?;

    let created_at_ms = now_epoch_ms()?;
    let admitted_fairness_rank =
        fairness_rank_for_wave(&planning_status, wave.metadata.id, created_at_ms);
    let admitted_waiting_since_ms = waiting_since_for_wave(&planning_status, wave.metadata.id);
    let claim = claim_wave_for_launch(root, config, waves, wave, &run_id, created_at_ms)?;
    let worktree = match allocate_wave_worktree(root, config, wave, &run_id, created_at_ms) {
        Ok(worktree) => worktree,
        Err(error) => {
            release_wave_claim(
                root,
                config,
                &claim,
                "launch aborted while allocating worktree",
            )?;
            return Err(error);
        }
    };
    let promotion = match initial_promotion_record(root, wave, &worktree)
        .and_then(|promotion| publish_promotion_record(root, config, promotion, &run_id))
    {
        Ok(promotion) => promotion,
        Err(error) => {
            let _ = release_wave_worktree(
                root,
                config,
                &worktree,
                &run_id,
                "launch aborted while recording promotion state",
            );
            release_wave_claim(
                root,
                config,
                &claim,
                "launch aborted while recording promotion state",
            )?;
            return Err(error);
        }
    };
    let scheduling = match publish_scheduling_record(
        root,
        config,
        WaveSchedulingRecord {
            wave_id: wave.metadata.id,
            phase: WaveExecutionPhase::Implementation,
            priority: WaveSchedulerPriority::Implementation,
            state: WaveSchedulingState::Admitted,
            fairness_rank: admitted_fairness_rank,
            waiting_since_ms: admitted_waiting_since_ms,
            protected_closure_capacity: false,
            preemptible: true,
            last_decision: Some(format!(
                "wave admitted for repo-local execution with fairness rank {}",
                admitted_fairness_rank
            )),
            updated_at_ms: created_at_ms,
        },
        &run_id,
    ) {
        Ok(scheduling) => scheduling,
        Err(error) => {
            let _ = release_wave_worktree(
                root,
                config,
                &worktree,
                &run_id,
                "launch aborted while recording scheduling state",
            );
            release_wave_claim(
                root,
                config,
                &claim,
                "launch aborted while recording scheduling state",
            )?;
            return Err(error);
        }
    };
    let execution_root = PathBuf::from(worktree.path.clone());
    if let Err(error) = prepare_closure_artifact_placeholders(&execution_root, wave) {
        let _ = release_wave_worktree(
            root,
            config,
            &worktree,
            &run_id,
            "launch aborted while seeding closure artifact placeholders",
        );
        release_wave_claim(
            root,
            config,
            &claim,
            "launch aborted while seeding closure artifact placeholders",
        )?;
        return Err(error);
    }
    let launcher_pid = std::process::id();
    let mut record = WaveRunRecord {
        run_id: run_id.clone(),
        wave_id: wave.metadata.id,
        slug: wave.metadata.slug.clone(),
        title: wave.metadata.title.clone(),
        status: WaveRunStatus::Planned,
        dry_run: options.dry_run,
        bundle_dir: bundle.bundle_dir.clone(),
        trace_path: trace_path.clone(),
        codex_home: codex_home.clone(),
        created_at_ms,
        started_at_ms: None,
        launcher_pid: Some(launcher_pid),
        launcher_started_at_ms: current_process_started_at_ms(),
        worktree: Some(worktree.clone()),
        promotion: Some(promotion.clone()),
        scheduling: Some(scheduling.clone()),
        completed_at_ms: None,
        agents: agent_records,
        error: None,
    };
    if let Err(error) = write_run_record(&state_path, &record) {
        let _ = release_wave_worktree(
            root,
            config,
            &worktree,
            &run_id,
            "launch aborted before run state could be recorded",
        );
        release_wave_claim(
            root,
            config,
            &claim,
            "launch aborted before run state could be recorded",
        )?;
        return Err(error);
    }
    if let Err(error) =
        clear_rerun_with_state(root, config, wave.metadata.id, RerunState::Completed)
    {
        let _ = release_wave_worktree(
            root,
            config,
            &worktree,
            &run_id,
            "launch aborted while clearing rerun intent",
        );
        release_wave_claim(
            root,
            config,
            &claim,
            "launch aborted while clearing rerun intent",
        )?;
        return Err(error);
    }

    let lease_timing = LeaseTiming::default();
    let mut promotion_checked = false;
    for index in execution_indices {
        let agent = ordered_agents[index];
        if is_closure_agent(agent.id.as_str()) && !promotion_checked {
            let previous_scheduling = record.scheduling.clone();
            let promotion_scheduling = publish_scheduling_record(
                root,
                config,
                WaveSchedulingRecord {
                    wave_id: record.wave_id,
                    phase: WaveExecutionPhase::Promotion,
                    priority: WaveSchedulerPriority::Closure,
                    state: WaveSchedulingState::Running,
                    fairness_rank: previous_scheduling
                        .as_ref()
                        .map(|record| record.fairness_rank)
                        .filter(|rank| *rank > 0)
                        .unwrap_or(1),
                    waiting_since_ms: previous_scheduling
                        .as_ref()
                        .and_then(|record| record.waiting_since_ms),
                    protected_closure_capacity: true,
                    preemptible: false,
                    last_decision: Some(
                        "implementation complete; evaluating merge-validated promotion candidate"
                            .to_string(),
                    ),
                    updated_at_ms: now_epoch_ms()?,
                },
                &record.run_id,
            )?;
            record.scheduling = Some(promotion_scheduling);
            let evaluated = evaluate_wave_promotion(
                root,
                config,
                record
                    .worktree
                    .as_ref()
                    .context("missing worktree while evaluating promotion")?,
                record
                    .promotion
                    .as_ref()
                    .context("missing promotion record while evaluating promotion")?,
                &record.run_id,
            )?;
            record.promotion = Some(evaluated.clone());
            write_run_record(&state_path, &record)?;
            promotion_checked = true;
            if evaluated.state != WavePromotionState::Ready {
                record.status = WaveRunStatus::Failed;
                record.error = evaluated.detail.clone();
                record.completed_at_ms = Some(now_epoch_ms()?);
                let released_worktree = release_wave_worktree(
                    root,
                    config,
                    record
                        .worktree
                        .as_ref()
                        .context("missing worktree while closing conflicted promotion")?,
                    &record.run_id,
                    "promotion blocked before closure",
                )?;
                record.worktree = Some(released_worktree);
                record.scheduling = Some(publish_scheduling_record(
                    root,
                    config,
                    WaveSchedulingRecord {
                        wave_id: record.wave_id,
                        phase: WaveExecutionPhase::Promotion,
                        priority: WaveSchedulerPriority::Closure,
                        state: WaveSchedulingState::Released,
                        fairness_rank: record
                            .scheduling
                            .as_ref()
                            .map(|record| record.fairness_rank)
                            .filter(|rank| *rank > 0)
                            .unwrap_or(1),
                        waiting_since_ms: record
                            .scheduling
                            .as_ref()
                            .and_then(|record| record.waiting_since_ms),
                        protected_closure_capacity: true,
                        preemptible: false,
                        last_decision: Some(
                            "closure blocked because promotion is not ready".to_string(),
                        ),
                        updated_at_ms: now_epoch_ms()?,
                    },
                    &record.run_id,
                )?);
                write_run_record(&state_path, &record)?;
                write_trace_bundle(&trace_path, &record)?;
                release_wave_claim(root, config, &claim, "promotion blocked; claim released")?;
                return Ok(LaunchReport {
                    run_id,
                    wave_id: wave.metadata.id,
                    status: record.status,
                    state_path,
                    trace_path,
                    bundle_dir: bundle.bundle_dir,
                    preflight_path,
                });
            }
        }
        let base_prompt =
            match fs::read_to_string(&record.agents[index].prompt_path).with_context(|| {
                format!(
                    "failed to read {}",
                    record.agents[index].prompt_path.display()
                )
            }) {
                Ok(prompt) => prompt,
                Err(error) => {
                    return finish_failed_launch(
                        root,
                        config,
                        &bundle,
                        &preflight_path,
                        &state_path,
                        &trace_path,
                        &mut record,
                        agent,
                        index,
                        error,
                    );
                }
            };
        let runtime_plan = match resolve_runtime_plan(
            root,
            &execution_root,
            &record,
            agent,
            &record.agents[index],
            &base_prompt,
            &runtime_registry,
        ) {
            Ok(plan) => plan,
            Err(error) => {
                return finish_failed_launch(
                    root,
                    config,
                    &bundle,
                    &preflight_path,
                    &state_path,
                    &trace_path,
                    &mut record,
                    agent,
                    index,
                    error,
                );
            }
        };
        record.agents[index].runtime = Some(runtime_plan.runtime.clone());
        record.agents[index].runtime_detail_path =
            artifact_path_from_runtime(&runtime_plan.runtime, "runtime_detail").map(PathBuf::from);
        write_run_record(&state_path, &record)?;
        append_attempt_event(
            root,
            config,
            &record,
            agent,
            AttemptState::Planned,
            record.created_at_ms,
            None,
            Some(runtime_plan.runtime.clone()),
        )?;

        let (phase, priority, protected_closure_capacity, preemptible) =
            scheduling_axes_for_agent(agent.id.as_str());
        let (agent_record, lease) = loop {
            let lease = match acquire_task_lease_for_agent(
                root,
                config,
                &state_path,
                &mut record,
                agent,
                &claim,
                lease_timing,
            ) {
                Ok(lease) => lease,
                Err(error) => {
                    return finish_failed_launch(
                        root,
                        config,
                        &bundle,
                        &preflight_path,
                        &state_path,
                        &trace_path,
                        &mut record,
                        agent,
                        index,
                        error,
                    );
                }
            };
            record.scheduling = Some(publish_scheduling_record(
                root,
                config,
                WaveSchedulingRecord {
                    wave_id: record.wave_id,
                    phase,
                    priority,
                    state: WaveSchedulingState::Running,
                    fairness_rank: record
                        .scheduling
                        .as_ref()
                        .map(|record| record.fairness_rank)
                        .filter(|rank| *rank > 0)
                        .unwrap_or(1),
                    waiting_since_ms: record
                        .scheduling
                        .as_ref()
                        .and_then(|record| record.waiting_since_ms),
                    protected_closure_capacity,
                    preemptible,
                    last_decision: Some(format!("running {} in shared wave worktree", agent.id)),
                    updated_at_ms: now_epoch_ms()?,
                },
                &record.run_id,
            )?);
            record.agents[index].status = WaveRunStatus::Running;
            if record.started_at_ms.is_none() {
                record.status = WaveRunStatus::Running;
                record.started_at_ms = Some(now_epoch_ms()?);
            }
            write_run_record(&state_path, &record)?;
            append_attempt_event(
                root,
                config,
                &record,
                agent,
                AttemptState::Running,
                record.created_at_ms,
                record.started_at_ms,
                Some(runtime_plan.runtime.clone()),
            )?;

            match execute_agent(
                root,
                config,
                &record,
                agent,
                &record.agents[index],
                &runtime_plan,
                &codex_home,
                &lease,
                lease_timing,
                &runtime_registry,
            ) {
                Ok(execution) => break (execution.record, execution.lease),
                Err(error) => {
                    if let Some(revoked) = lease_revoked_error(&error) {
                        record.agents[index].status = WaveRunStatus::Planned;
                        record.scheduling = Some(publish_scheduling_record(
                            root,
                            config,
                            WaveSchedulingRecord {
                                wave_id: record.wave_id,
                                phase,
                                priority,
                                state: WaveSchedulingState::Preempted,
                                fairness_rank: record
                                    .scheduling
                                    .as_ref()
                                    .map(|record| record.fairness_rank)
                                    .filter(|rank| *rank > 0)
                                    .unwrap_or(1),
                                waiting_since_ms: record
                                    .scheduling
                                    .as_ref()
                                    .and_then(|record| record.waiting_since_ms)
                                    .or_else(|| Some(now_epoch_ms().unwrap_or_default())),
                                protected_closure_capacity: false,
                                preemptible: true,
                                last_decision: Some(revoked.detail.clone()),
                                updated_at_ms: now_epoch_ms()?,
                            },
                            &record.run_id,
                        )?);
                        write_run_record(&state_path, &record)?;
                        append_attempt_event(
                            root,
                            config,
                            &record,
                            agent,
                            AttemptState::Aborted,
                            record.created_at_ms,
                            record.started_at_ms,
                            Some(runtime_plan.runtime.clone()),
                        )?;
                        continue;
                    }
                    return finish_failed_launch(
                        root,
                        config,
                        &bundle,
                        &preflight_path,
                        &state_path,
                        &trace_path,
                        &mut record,
                        agent,
                        index,
                        error,
                    );
                }
            }
        };
        let agent_record =
            match persist_agent_result_envelope(root, config, &record, agent, &agent_record) {
                Ok(agent_record) => agent_record,
                Err(error) => {
                    return finish_failed_launch(
                        root,
                        config,
                        &bundle,
                        &preflight_path,
                        &state_path,
                        &trace_path,
                        &mut record,
                        agent,
                        index,
                        error,
                    );
                }
            };
        if agent_record.status == WaveRunStatus::Succeeded {
            if let Err(error) = close_task_lease(
                root,
                config,
                &lease,
                TaskLeaseState::Released,
                format!("agent {} completed", agent.id),
            ) {
                return finish_failed_launch(
                    root,
                    config,
                    &bundle,
                    &preflight_path,
                    &state_path,
                    &trace_path,
                    &mut record,
                    agent,
                    index,
                    error,
                );
            }
        }
        record.agents[index] = agent_record.clone();
        append_attempt_event(
            root,
            config,
            &record,
            agent,
            attempt_state_from_agent_status(agent_record.status),
            record.created_at_ms,
            record.started_at_ms,
            agent_record.runtime.clone(),
        )?;
        if agent_record.status == WaveRunStatus::Failed {
            close_task_lease(
                root,
                config,
                &lease,
                TaskLeaseState::Revoked,
                format!("agent {} failed", agent.id),
            )?;
            record.status = WaveRunStatus::Failed;
            record.error = agent_record.error.clone();
            record.completed_at_ms = Some(now_epoch_ms()?);
            if let Some(worktree) = record.worktree.clone() {
                record.worktree = Some(release_wave_worktree(
                    root,
                    config,
                    &worktree,
                    &record.run_id,
                    "wave failed; worktree released",
                )?);
            }
            record.scheduling = Some(publish_scheduling_record(
                root,
                config,
                WaveSchedulingRecord {
                    wave_id: record.wave_id,
                    phase: if is_closure_agent(agent.id.as_str()) {
                        WaveExecutionPhase::Closure
                    } else {
                        WaveExecutionPhase::Implementation
                    },
                    priority: if is_closure_agent(agent.id.as_str()) {
                        WaveSchedulerPriority::Closure
                    } else {
                        WaveSchedulerPriority::Implementation
                    },
                    state: WaveSchedulingState::Released,
                    fairness_rank: record
                        .scheduling
                        .as_ref()
                        .map(|record| record.fairness_rank)
                        .filter(|rank| *rank > 0)
                        .unwrap_or(1),
                    waiting_since_ms: record
                        .scheduling
                        .as_ref()
                        .and_then(|record| record.waiting_since_ms),
                    protected_closure_capacity: is_closure_agent(agent.id.as_str()),
                    preemptible: false,
                    last_decision: Some(format!("{} failed; run released", agent.id)),
                    updated_at_ms: now_epoch_ms()?,
                },
                &record.run_id,
            )?);
            write_run_record(&state_path, &record)?;
            write_trace_bundle(&trace_path, &record)?;
            release_wave_claim(root, config, &claim, "wave failed; claim released")?;
            return Ok(LaunchReport {
                run_id,
                wave_id: wave.metadata.id,
                status: record.status,
                state_path,
                trace_path,
                bundle_dir: bundle.bundle_dir,
                preflight_path,
            });
        }
        write_run_record(&state_path, &record)?;
    }

    if let Some(worktree) = record.worktree.clone() {
        record.worktree = Some(release_wave_worktree(
            root,
            config,
            &worktree,
            &record.run_id,
            "wave completed; worktree released",
        )?);
    }
    record.scheduling = Some(publish_scheduling_record(
        root,
        config,
        WaveSchedulingRecord {
            wave_id: record.wave_id,
            phase: if promotion_checked {
                WaveExecutionPhase::Closure
            } else {
                WaveExecutionPhase::Implementation
            },
            priority: if promotion_checked {
                WaveSchedulerPriority::Closure
            } else {
                WaveSchedulerPriority::Implementation
            },
            state: WaveSchedulingState::Released,
            fairness_rank: record
                .scheduling
                .as_ref()
                .map(|record| record.fairness_rank)
                .filter(|rank| *rank > 0)
                .unwrap_or(1),
            waiting_since_ms: record
                .scheduling
                .as_ref()
                .and_then(|record| record.waiting_since_ms),
            protected_closure_capacity: promotion_checked,
            preemptible: false,
            last_decision: Some("wave completed and released".to_string()),
            updated_at_ms: now_epoch_ms()?,
        },
        &record.run_id,
    )?);
    release_wave_claim(root, config, &claim, "wave completed; claim released")?;
    record.status = WaveRunStatus::Succeeded;
    record.completed_at_ms = Some(now_epoch_ms()?);
    write_run_record(&state_path, &record)?;
    write_trace_bundle(&trace_path, &record)?;

    Ok(LaunchReport {
        run_id,
        wave_id: wave.metadata.id,
        status: record.status,
        state_path,
        trace_path,
        bundle_dir: bundle.bundle_dir,
        preflight_path,
    })
}

pub fn autonomous_launch(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    mut status: PlanningStatus,
    options: AutonomousOptions,
) -> Result<Vec<LaunchReport>> {
    let mut launched = Vec::new();
    if matches!(options.limit, Some(0)) {
        return Ok(launched);
    }
    if !options.dry_run {
        let _ = repair_orphaned_runs(root, config)?;
        status = refresh_planning_status(root, config, waves)?;
    }
    loop {
        if let Some(limit) = options.limit {
            if launched.len() >= limit {
                break;
            }
        }
        let batch_limit = options
            .limit
            .map(|limit| limit.saturating_sub(launched.len()))
            .unwrap_or(2)
            .min(2);
        let batch = next_parallel_wave_batch(root, config, waves, &status, batch_limit)?;
        if batch.is_empty() {
            break;
        }
        if options.dry_run || batch.len() == 1 {
            let wave_id = batch[0].wave_id;
            let report = match launch_wave(
                root,
                config,
                waves,
                &status,
                LaunchOptions {
                    wave_id: Some(wave_id),
                    dry_run: options.dry_run,
                },
            ) {
                Ok(report) => report,
                Err(error)
                    if error
                        .chain()
                        .any(|cause| cause.downcast_ref::<SchedulerAdmissionError>().is_some()) =>
                {
                    status = refresh_planning_status(root, config, waves)?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            let failed = report.status == WaveRunStatus::Failed;
            launched.push(report);
            status = refresh_planning_status(root, config, waves)?;
            if options.dry_run || failed {
                break;
            }
            continue;
        }

        let mut reports = Vec::new();
        let mut admission_retry = false;
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for selection in &batch {
                let root = root.to_path_buf();
                let config = config.clone();
                let waves = waves.to_vec();
                let status = status.clone();
                let wave_id = selection.wave_id;
                handles.push(scope.spawn(move || {
                    launch_wave(
                        &root,
                        &config,
                        &waves,
                        &status,
                        LaunchOptions {
                            wave_id: Some(wave_id),
                            dry_run: false,
                        },
                    )
                }));
            }
            for handle in handles {
                match handle.join().expect("parallel wave launch thread panicked") {
                    Ok(report) => reports.push(report),
                    Err(error)
                        if error.chain().any(|cause| {
                            cause.downcast_ref::<SchedulerAdmissionError>().is_some()
                        }) =>
                    {
                        admission_retry = true;
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok::<(), anyhow::Error>(())
        })?;
        if admission_retry && reports.is_empty() {
            status = refresh_planning_status(root, config, waves)?;
            continue;
        }
        let failed = reports
            .iter()
            .any(|report| report.status == WaveRunStatus::Failed);
        launched.extend(reports);
        status = refresh_planning_status(root, config, waves)?;
        if failed {
            break;
        }
    }
    if launched.is_empty() {
        bail!("{}", queue_unavailable_reason(&status));
    }
    Ok(launched)
}

fn next_claimable_wave_id(status: &PlanningStatus) -> Option<u32> {
    next_claimable_wave_selection(status).map(|selection| selection.wave_id)
}

fn next_claimable_wave_selection(status: &PlanningStatus) -> Option<AutonomousWaveSelection> {
    fifo_ordered_claimable_implementation_waves(status, now_epoch_ms().unwrap_or_default())
        .into_iter()
        .map(|candidate| candidate.selection)
        .next()
}

fn next_parallel_wave_batch(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    status: &PlanningStatus,
    max_batch_size: usize,
) -> Result<Vec<AutonomousWaveSelection>> {
    if max_batch_size == 0 {
        return Ok(Vec::new());
    }
    let now_ms = now_epoch_ms()?;
    let effective_batch_size = effective_parallel_batch_size(status, max_batch_size);
    let fairness_candidates = fifo_ordered_claimable_implementation_waves(status, now_ms);
    let mut selected = Vec::new();
    let mut waiting_updates = Vec::new();
    for (index, ordered) in fairness_candidates.iter().enumerate() {
        let Some(entry) = status
            .waves
            .iter()
            .find(|entry| entry.id == ordered.selection.wave_id)
        else {
            continue;
        };
        let Some(candidate) = waves
            .iter()
            .find(|wave| wave.metadata.id == ordered.selection.wave_id)
        else {
            continue;
        };
        let fairness_rank = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let conflicting_with = selected
            .iter()
            .filter_map(|selection: &AutonomousWaveSelection| {
                let selected_wave = waves
                    .iter()
                    .find(|wave| wave.metadata.id == selection.wave_id)
                    .expect("selected wave definition");
                waves_conflict_for_parallel_admission(selected_wave, candidate)
                    .then_some(selection.wave_id)
            })
            .collect::<Vec<_>>();
        let at_capacity = selected.len() >= effective_batch_size;
        if !conflicting_with.is_empty() || at_capacity {
            let reason = if !conflicting_with.is_empty() {
                parallel_conflict_wait_reason(&conflicting_with)
            } else if status_closure_capacity_reserved(status) && at_capacity {
                "waiting because closure capacity is reserved ahead of new implementation work"
                    .to_string()
            } else if effective_batch_size == 0 {
                "waiting for an available parallel implementation slot".to_string()
            } else {
                "waiting for fairness turn behind older claimable waves".to_string()
            };
            waiting_updates.push(WaveSchedulingRecord {
                wave_id: entry.id,
                phase: WaveExecutionPhase::Implementation,
                priority: WaveSchedulerPriority::Implementation,
                state: WaveSchedulingState::Waiting,
                fairness_rank,
                waiting_since_ms: Some(ordered.waiting_since_ms),
                protected_closure_capacity: false,
                preemptible: true,
                last_decision: Some(reason),
                updated_at_ms: now_ms,
            });
            continue;
        }
        selected.push(ordered.selection.clone());
    }
    for update in waiting_updates {
        publish_scheduling_record(root, config, update, "autonomous-parallel-admission")?;
    }
    Ok(selected)
}

fn waves_conflict_for_parallel_admission(left: &WaveDocument, right: &WaveDocument) -> bool {
    let left_paths = implementation_owned_paths(left);
    let right_paths = implementation_owned_paths(right);
    left_paths.iter().any(|left_path| {
        right_paths
            .iter()
            .any(|right_path| path_scopes_conflict(left_path, right_path))
    })
}

// FIFO fairness only applies inside the claimable implementation-admission lane.
// Closure-protected work and lease-level preemption are handled above this helper.
fn fifo_ordered_claimable_implementation_waves(
    status: &PlanningStatus,
    reference_ms: u128,
) -> Vec<FifoOrderedClaimableWave> {
    let mut ordered = status
        .queue
        .claimable_wave_ids
        .iter()
        .enumerate()
        .filter_map(|(claimable_order, wave_id)| {
            let entry = status.waves.iter().find(|entry| entry.id == *wave_id)?;
            Some(FifoOrderedClaimableWave {
                selection: AutonomousWaveSelection {
                    wave_id: entry.id,
                    slug: entry.slug.clone(),
                    title: entry.title.clone(),
                    blocked_by: entry.blocked_by.clone(),
                },
                claimable_order,
                waiting_since_ms: waiting_since_for_claimable_entry(entry, reference_ms),
            })
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|entry| {
        (
            entry.waiting_since_ms,
            entry.claimable_order,
            entry.selection.wave_id,
        )
    });
    ordered
}

fn waiting_since_for_claimable_entry(
    entry: &wave_control_plane::WaveQueueEntry,
    reference_ms: u128,
) -> u128 {
    entry
        .execution
        .scheduling
        .as_ref()
        .and_then(|record| {
            matches!(
                record.state,
                WaveSchedulingState::Waiting
                    | WaveSchedulingState::Protected
                    | WaveSchedulingState::Preempted
            )
            .then_some(record.waiting_since_ms)
            .flatten()
        })
        .unwrap_or(reference_ms)
}

fn parallel_conflict_wait_reason(conflicting_with: &[u32]) -> String {
    let blocking = conflicting_with
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!("waiting for a non-conflicting parallel worktree slot behind wave {blocking}")
}

fn implementation_owned_paths(wave: &WaveDocument) -> Vec<String> {
    let mut paths = Vec::new();
    for agent in wave.implementation_agents() {
        for path in &agent.file_ownership {
            let normalized = path.trim_matches('/');
            if normalized.is_empty() {
                continue;
            }
            paths.push(normalized.to_string());
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn path_scopes_conflict(left: &str, right: &str) -> bool {
    left == right
        || left.starts_with(&format!("{right}/"))
        || right.starts_with(&format!("{left}/"))
}

fn effective_parallel_batch_size(status: &PlanningStatus, max_batch_size: usize) -> usize {
    let Some(budget) = shared_scheduler_budget_state(status) else {
        return max_batch_size;
    };
    let Some(max_active_task_leases) = budget.max_active_task_leases else {
        return max_batch_size;
    };

    let available = if budget.closure_capacity_reserved {
        let reserved =
            usize::try_from(budget.reserved_closure_task_leases.unwrap_or(0)).unwrap_or(usize::MAX);
        let implementation_cap = usize::try_from(max_active_task_leases)
            .unwrap_or(usize::MAX)
            .saturating_sub(reserved);
        implementation_cap.saturating_sub(budget.active_implementation_task_leases)
    } else {
        usize::try_from(max_active_task_leases)
            .unwrap_or(usize::MAX)
            .saturating_sub(budget.active_task_leases)
    };

    available.min(max_batch_size)
}

// The reducer projects one shared scheduler budget onto each wave entry. Reading the first
// entry is therefore intentional here until the projection grows a dedicated top-level budget.
fn shared_scheduler_budget_state(
    status: &PlanningStatus,
) -> Option<&wave_control_plane::SchedulerBudgetState> {
    status.waves.first().map(|entry| &entry.ownership.budget)
}

fn status_closure_capacity_reserved(status: &PlanningStatus) -> bool {
    shared_scheduler_budget_state(status)
        .map(|budget| budget.closure_capacity_reserved)
        .unwrap_or(false)
}

fn queue_unavailable_reason(status: &PlanningStatus) -> String {
    if let Some(selection) = next_claimable_wave_selection(status) {
        return format!(
            "wave {} is ready but could not be claimed: {}",
            selection.wave_id,
            queue_entry_reason_from_blockers(&selection.blocked_by)
        );
    }

    let blocked_wave_ids = status
        .waves
        .iter()
        .filter(|entry| !entry.ready)
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    if blocked_wave_ids.is_empty() {
        return status.queue.queue_ready_reason.clone();
    }

    format!(
        "{}; blocked waves: {}",
        status.queue.queue_ready_reason,
        blocked_wave_ids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn refresh_planning_status(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
) -> Result<PlanningStatus> {
    let latest_runs = load_latest_runs(root, config)?;
    let findings = wave_dark_factory::lint_project(root, waves);
    let rerun_wave_ids = pending_rerun_wave_ids(root, config)?;
    let closure_override_wave_ids = active_closure_override_wave_ids(root, config)?;
    wave_control_plane::build_planning_status_from_authority(
        root,
        config,
        waves,
        &findings,
        &[],
        &latest_runs,
        &rerun_wave_ids,
        &closure_override_wave_ids,
    )
}

fn is_claimable_wave(status: &PlanningStatus, wave_id: u32) -> bool {
    status.queue.claimable_wave_ids.contains(&wave_id)
}

fn fairness_rank_for_wave(status: &PlanningStatus, wave_id: u32, reference_ms: u128) -> u32 {
    fifo_ordered_claimable_implementation_waves(status, reference_ms)
        .iter()
        .position(|candidate| candidate.selection.wave_id == wave_id)
        .and_then(|index| u32::try_from(index + 1).ok())
        .or_else(|| {
            status
                .waves
                .iter()
                .find(|entry| entry.id == wave_id)
                .and_then(|entry| entry.execution.scheduling.as_ref())
                .map(|record| record.fairness_rank)
        })
        .filter(|rank| *rank > 0)
        .unwrap_or(1)
}

fn waiting_since_for_wave(status: &PlanningStatus, wave_id: u32) -> Option<u128> {
    status
        .waves
        .iter()
        .find(|entry| entry.id == wave_id)
        .and_then(|entry| entry.execution.scheduling.as_ref())
        .and_then(|record| record.waiting_since_ms)
}

fn queue_entry_reason(entry: &wave_control_plane::WaveQueueEntry) -> String {
    queue_entry_reason_from_blockers(&entry.blocked_by)
}

fn queue_entry_reason_from_blockers(blocked_by: &[String]) -> String {
    if blocked_by.is_empty() {
        "unknown blocker".to_string()
    } else {
        blocked_by.join(", ")
    }
}

pub fn repair_orphaned_runs(root: &Path, config: &ProjectConfig) -> Result<Vec<WaveRunRecord>> {
    let runs_dir = state_runs_dir(root, config);
    if !runs_dir.exists() {
        return Ok(Vec::new());
    }

    let mut repaired = Vec::new();
    for entry in
        fs::read_dir(&runs_dir).with_context(|| format!("failed to read {}", runs_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", runs_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let mut record = load_run_record(&path)?;
        if !reconcile_orphaned_run_record(&mut record)? {
            continue;
        }
        write_run_record(&path, &record)?;
        write_trace_bundle(&record.trace_path, &record)?;
        cleanup_scheduler_ownership_for_run(
            root,
            config,
            &record,
            "launcher orphaned; scheduler ownership revoked",
        )?;
        repaired.push(record);
    }

    Ok(repaired)
}

fn append_control_event(root: &Path, config: &ProjectConfig, event: ControlEvent) -> Result<()> {
    control_event_log(root, config).append(&event)
}

fn control_event_log(root: &Path, config: &ProjectConfig) -> ControlEventLog {
    ControlEventLog::new(
        config
            .resolved_paths(root)
            .authority
            .state_events_control_dir,
    )
}

fn scheduler_event_log(root: &Path, config: &ProjectConfig) -> SchedulerEventLog {
    SchedulerEventLog::new(
        config
            .resolved_paths(root)
            .authority
            .state_events_scheduler_dir,
    )
}

fn scheduler_mutation_lock_path(root: &Path, config: &ProjectConfig) -> PathBuf {
    config
        .resolved_paths(root)
        .authority
        .state_derived_dir
        .join("scheduler")
        .join("mutation.lock")
}

fn with_scheduler_mutation<T>(
    root: &Path,
    config: &ProjectConfig,
    f: impl FnOnce(&SchedulerEventLog) -> Result<T>,
) -> Result<T> {
    let log = scheduler_event_log(root, config);
    wave_events::with_scheduler_mutation_lock(scheduler_mutation_lock_path(root, config), || {
        f(&log)
    })
}

fn runtime_scheduler_owner(session_id: impl Into<String>) -> SchedulerOwner {
    SchedulerOwner {
        scheduler_id: "wave-runtime".to_string(),
        scheduler_path: "wave-runtime/launcher".to_string(),
        runtime: None,
        executor: None,
        session_id: Some(session_id.into()),
        process_id: Some(std::process::id()),
        process_started_at_ms: current_process_started_at_ms(),
    }
}

#[cfg(test)]
fn ensure_default_scheduler_budget(root: &Path, config: &ProjectConfig) -> Result<()> {
    with_scheduler_mutation(root, config, ensure_default_scheduler_budget_in_log)
}

fn ensure_default_scheduler_budget_in_log(log: &SchedulerEventLog) -> Result<()> {
    if log
        .load_all()?
        .iter()
        .any(|event| matches!(event.kind, SchedulerEventKind::SchedulerBudgetUpdated))
    {
        return Ok(());
    }

    let created_at_ms = now_epoch_ms()?;
    let budget = SchedulerBudgetRecord {
        budget_id: SchedulerBudgetId::new("budget-default"),
        budget: SchedulerBudget {
            max_active_wave_claims: Some(2),
            max_active_task_leases: Some(2),
            reserved_closure_task_leases: Some(1),
            preemption_enabled: true,
        },
        owner: runtime_scheduler_owner("budget-bootstrap"),
        updated_at_ms: created_at_ms,
        detail: Some("default parallel-wave scheduler budget".to_string()),
    };
    append_scheduler_event_in_log(
        log,
        &SchedulerEvent::new(
            format!("sched-budget-default-{created_at_ms}"),
            SchedulerEventKind::SchedulerBudgetUpdated,
        )
        .with_created_at_ms(created_at_ms)
        .with_correlation_id("scheduler-budget-default")
        .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated { budget }),
    )
}

fn build_runtime_scheduler_snapshot(log: &SchedulerEventLog) -> Result<RuntimeSchedulerSnapshot> {
    let mut latest_leases = HashMap::new();
    let mut scheduling_by_wave = HashMap::new();
    let mut budget = SchedulerBudget::default();
    let mut events = log.load_all()?;
    events.sort_by_key(|event| (event.created_at_ms, event.event_id.clone()));

    for event in events {
        match event.payload {
            SchedulerEventPayload::TaskLeaseUpdated { lease } => {
                latest_leases.insert(lease.lease_id.clone(), lease);
            }
            SchedulerEventPayload::WaveSchedulingUpdated { scheduling } => {
                scheduling_by_wave.insert(scheduling.wave_id, scheduling);
            }
            SchedulerEventPayload::SchedulerBudgetUpdated { budget: record } => {
                budget = record.budget;
            }
            _ => {}
        }
    }

    let now_ms = now_epoch_ms()?;
    let active_leases = latest_leases
        .values()
        .filter(|lease| lease.state.is_active() && !lease_is_expired(lease, now_ms))
        .cloned()
        .collect::<Vec<_>>();
    let active_implementation_task_leases = active_leases
        .iter()
        .filter(|lease| !task_id_is_closure(&lease.task_id))
        .count();
    let active_closure_task_leases = active_leases
        .iter()
        .filter(|lease| task_id_is_closure(&lease.task_id))
        .count();
    let waiting_closure_waves = scheduling_by_wave
        .values()
        .filter(|record| {
            matches!(record.phase, WaveExecutionPhase::Closure)
                && matches!(
                    record.state,
                    WaveSchedulingState::Waiting
                        | WaveSchedulingState::Protected
                        | WaveSchedulingState::Preempted
                )
        })
        .count();

    Ok(RuntimeSchedulerSnapshot {
        budget,
        latest_leases,
        active_leases,
        scheduling_by_wave,
        active_implementation_task_leases,
        active_closure_task_leases,
        waiting_closure_waves,
    })
}

fn task_id_is_closure(task_id: &wave_domain::TaskId) -> bool {
    task_id
        .as_str()
        .rsplit_once("agent-")
        .map(|(_, agent_id)| matches!(agent_id, "a0" | "a8" | "a9"))
        .unwrap_or(false)
}

fn implementation_task_capacity(snapshot: &RuntimeSchedulerSnapshot) -> Option<usize> {
    let max_active_task_leases = snapshot
        .budget
        .max_active_task_leases
        .and_then(|limit| usize::try_from(limit).ok())?;
    let reserved_closure_task_leases = snapshot
        .budget
        .reserved_closure_task_leases
        .and_then(|reserved| usize::try_from(reserved).ok())
        .unwrap_or(0);
    let committed_closure_slots = if snapshot.waiting_closure_waves > 0
        && snapshot.active_closure_task_leases < reserved_closure_task_leases
    {
        reserved_closure_task_leases
    } else {
        snapshot.active_closure_task_leases
    };

    Some(max_active_task_leases.saturating_sub(committed_closure_slots))
}

fn task_lease_event_kind(state: TaskLeaseState) -> SchedulerEventKind {
    match state {
        TaskLeaseState::Granted => SchedulerEventKind::TaskLeaseRenewed,
        TaskLeaseState::Released => SchedulerEventKind::TaskLeaseReleased,
        TaskLeaseState::Expired => SchedulerEventKind::TaskLeaseExpired,
        TaskLeaseState::Revoked => SchedulerEventKind::TaskLeaseRevoked,
    }
}

fn scheduler_event_for_lease(lease: &TaskLeaseRecord, kind: SchedulerEventKind) -> SchedulerEvent {
    let created_at_ms = lease
        .finished_at_ms
        .or(lease.heartbeat_at_ms)
        .unwrap_or(lease.granted_at_ms);
    let mut event = SchedulerEvent::new(
        format!(
            "sched-lease-{}-{}-{created_at_ms}",
            lease_state_label(lease.state),
            lease.task_id
        ),
        kind,
    )
    .with_wave_id(lease.wave_id)
    .with_task_id(lease.task_id.clone())
    .with_lease_id(lease.lease_id.clone())
    .with_created_at_ms(created_at_ms)
    .with_correlation_id(
        lease
            .owner
            .session_id
            .clone()
            .unwrap_or_else(|| lease.lease_id.as_str().to_string()),
    )
    .with_payload(SchedulerEventPayload::TaskLeaseUpdated {
        lease: lease.clone(),
    });
    if let Some(claim_id) = lease.claim_id.clone() {
        event = event.with_claim_id(claim_id);
    }
    event
}

fn close_task_lease_in_log(
    log: &SchedulerEventLog,
    lease: &TaskLeaseRecord,
    state: TaskLeaseState,
    detail: impl Into<String>,
) -> Result<TaskLeaseRecord> {
    let finished_at_ms = now_epoch_ms()?;
    let mut closed = lease.clone();
    closed.state = state;
    closed.finished_at_ms = Some(finished_at_ms);
    closed.heartbeat_at_ms = Some(finished_at_ms);
    if matches!(state, TaskLeaseState::Expired) && closed.expires_at_ms.is_none() {
        closed.expires_at_ms = Some(finished_at_ms);
    }
    closed.detail = Some(detail.into());
    append_scheduler_event_in_log(
        log,
        &scheduler_event_for_lease(&closed, task_lease_event_kind(state)),
    )?;
    Ok(closed)
}

fn publish_worktree_record(
    root: &Path,
    config: &ProjectConfig,
    worktree: WaveWorktreeRecord,
    correlation_id: &str,
) -> Result<WaveWorktreeRecord> {
    append_scheduler_event(
        root,
        config,
        SchedulerEvent::new(
            format!(
                "sched-worktree-{}-{}-{}",
                worktree.wave_id,
                worktree_state_label(worktree.state),
                worktree.allocated_at_ms
            ),
            SchedulerEventKind::WaveWorktreeUpdated,
        )
        .with_wave_id(worktree.wave_id)
        .with_created_at_ms(now_epoch_ms()?)
        .with_correlation_id(correlation_id.to_string())
        .with_payload(SchedulerEventPayload::WaveWorktreeUpdated {
            worktree: worktree.clone(),
        }),
    )?;
    Ok(worktree)
}

fn publish_promotion_record(
    root: &Path,
    config: &ProjectConfig,
    promotion: WavePromotionRecord,
    correlation_id: &str,
) -> Result<WavePromotionRecord> {
    append_scheduler_event(
        root,
        config,
        SchedulerEvent::new(
            format!(
                "sched-promotion-{}-{}-{}",
                promotion.wave_id,
                promotion_state_label(promotion.state),
                promotion.checked_at_ms
            ),
            SchedulerEventKind::WavePromotionUpdated,
        )
        .with_wave_id(promotion.wave_id)
        .with_created_at_ms(now_epoch_ms()?)
        .with_correlation_id(correlation_id.to_string())
        .with_payload(SchedulerEventPayload::WavePromotionUpdated {
            promotion: promotion.clone(),
        }),
    )?;
    Ok(promotion)
}

fn scheduler_event_for_scheduling(
    scheduling: &WaveSchedulingRecord,
    correlation_id: &str,
) -> SchedulerEvent {
    SchedulerEvent::new(
        format!(
            "sched-wave-scheduling-{}-{}-{}",
            scheduling.wave_id,
            scheduling_state_label(scheduling.state),
            scheduling.updated_at_ms
        ),
        SchedulerEventKind::WaveSchedulingUpdated,
    )
    .with_wave_id(scheduling.wave_id)
    .with_created_at_ms(scheduling.updated_at_ms)
    .with_correlation_id(correlation_id.to_string())
    .with_payload(SchedulerEventPayload::WaveSchedulingUpdated {
        scheduling: scheduling.clone(),
    })
}

fn publish_scheduling_record_in_log(
    log: &SchedulerEventLog,
    scheduling: &WaveSchedulingRecord,
    correlation_id: &str,
) -> Result<()> {
    append_scheduler_event_in_log(
        log,
        &scheduler_event_for_scheduling(scheduling, correlation_id),
    )
}

fn publish_scheduling_record(
    root: &Path,
    config: &ProjectConfig,
    scheduling: WaveSchedulingRecord,
    correlation_id: &str,
) -> Result<WaveSchedulingRecord> {
    append_scheduler_event(
        root,
        config,
        scheduler_event_for_scheduling(&scheduling, correlation_id),
    )?;
    Ok(scheduling)
}

fn allocate_wave_worktree(
    root: &Path,
    config: &ProjectConfig,
    wave: &WaveDocument,
    run_id: &str,
    allocated_at_ms: u128,
) -> Result<WaveWorktreeRecord> {
    let snapshot_ref = create_workspace_snapshot_commit(root, root, config, run_id, "base")?;
    let worktree_path =
        state_worktrees_dir(root, config).join(format!("wave-{:02}-{run_id}", wave.metadata.id));
    if let Some(parent) = worktree_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if worktree_path.exists() {
        let _ = run_git(
            root,
            &[
                "worktree",
                "remove",
                "--force",
                worktree_path.to_string_lossy().as_ref(),
            ],
        );
        let _ = fs::remove_dir_all(&worktree_path);
    }
    run_git(
        root,
        &[
            "worktree",
            "add",
            "--detach",
            worktree_path.to_string_lossy().as_ref(),
            snapshot_ref.as_str(),
        ],
    )?;
    publish_worktree_record(
        root,
        config,
        WaveWorktreeRecord {
            worktree_id: WaveWorktreeId::new(format!(
                "worktree-wave-{:02}-{run_id}",
                wave.metadata.id
            )),
            wave_id: wave.metadata.id,
            state: WaveWorktreeState::Allocated,
            path: worktree_path.to_string_lossy().into_owned(),
            base_ref: current_head_label(root)?,
            snapshot_ref,
            branch_ref: None,
            shared_scope: WaveWorktreeScope::Wave,
            allocated_at_ms,
            released_at_ms: None,
            detail: Some("shared wave-local worktree".to_string()),
        },
        run_id,
    )
}

fn release_wave_worktree(
    root: &Path,
    config: &ProjectConfig,
    worktree: &WaveWorktreeRecord,
    correlation_id: &str,
    detail: impl Into<String>,
) -> Result<WaveWorktreeRecord> {
    let worktree_path = Path::new(worktree.path.as_str());
    if git_worktree_registered(root, worktree_path)? {
        run_git(
            root,
            &[
                "worktree",
                "remove",
                "--force",
                worktree_path.to_string_lossy().as_ref(),
            ],
        )?;
    }
    if worktree_path.exists() {
        fs::remove_dir_all(worktree_path)
            .with_context(|| format!("failed to remove {}", worktree_path.display()))?;
    }
    if git_worktree_registered(root, worktree_path)? || worktree_path.exists() {
        bail!(
            "wave worktree {} still exists after release",
            worktree_path.display()
        );
    }
    publish_worktree_record(
        root,
        config,
        WaveWorktreeRecord {
            state: WaveWorktreeState::Released,
            released_at_ms: Some(now_epoch_ms()?),
            detail: Some(detail.into()),
            ..worktree.clone()
        },
        correlation_id,
    )
}

fn initial_promotion_record(
    root: &Path,
    wave: &WaveDocument,
    worktree: &WaveWorktreeRecord,
) -> Result<WavePromotionRecord> {
    Ok(WavePromotionRecord {
        promotion_id: WavePromotionId::new(format!(
            "promotion-wave-{:02}-{}",
            wave.metadata.id, worktree.snapshot_ref
        )),
        wave_id: wave.metadata.id,
        worktree_id: Some(worktree.worktree_id.clone()),
        state: WavePromotionState::NotStarted,
        target_ref: current_head_label(root)?,
        snapshot_ref: worktree.snapshot_ref.clone(),
        candidate_ref: None,
        candidate_tree: None,
        conflict_paths: Vec::new(),
        checked_at_ms: worktree.allocated_at_ms,
        completed_at_ms: None,
        detail: Some("promotion not yet evaluated".to_string()),
    })
}

fn evaluate_wave_promotion(
    root: &Path,
    config: &ProjectConfig,
    worktree: &WaveWorktreeRecord,
    promotion: &WavePromotionRecord,
    correlation_id: &str,
) -> Result<WavePromotionRecord> {
    let checked_at_ms = now_epoch_ms()?;
    let pending = publish_promotion_record(
        root,
        config,
        WavePromotionRecord {
            state: WavePromotionState::Pending,
            checked_at_ms,
            completed_at_ms: None,
            detail: Some("evaluating promotion candidate".to_string()),
            ..promotion.clone()
        },
        correlation_id,
    )?;
    let worktree_root = resolve_workspace_root(root, Path::new(worktree.path.as_str()));
    let candidate_ref = create_workspace_snapshot_commit(
        root,
        &worktree_root,
        config,
        correlation_id,
        "candidate",
    )?;
    let target_snapshot_ref =
        create_workspace_snapshot_commit(root, root, config, correlation_id, "target")?;
    let candidate_tree = git_output(
        &worktree_root,
        &["rev-parse", &format!("{candidate_ref}^{{tree}}")],
    )?;
    let (state, conflict_paths, detail) = validate_wave_promotion_merge(
        root,
        config,
        &target_snapshot_ref,
        &candidate_ref,
        correlation_id,
    )?;
    publish_promotion_record(
        root,
        config,
        WavePromotionRecord {
            state,
            target_ref: current_head_label(root)?,
            candidate_ref: Some(candidate_ref),
            candidate_tree: Some(candidate_tree),
            conflict_paths: conflict_paths.clone(),
            checked_at_ms,
            completed_at_ms: Some(now_epoch_ms()?),
            detail: Some(detail),
            ..pending
        },
        correlation_id,
    )
}

fn validate_wave_promotion_merge(
    root: &Path,
    config: &ProjectConfig,
    target_snapshot_ref: &str,
    candidate_ref: &str,
    correlation_id: &str,
) -> Result<(WavePromotionState, Vec<String>, String)> {
    let validation_root = config
        .resolved_paths(root)
        .authority
        .state_derived_dir
        .join("promotion-checks");
    fs::create_dir_all(&validation_root)
        .with_context(|| format!("failed to create {}", validation_root.display()))?;
    let scratch_root = validation_root.join(format!("{correlation_id}-{}", now_epoch_ms()?));
    if scratch_root.exists() {
        fs::remove_dir_all(&scratch_root)
            .with_context(|| format!("failed to clear {}", scratch_root.display()))?;
    }
    run_git(
        root,
        &[
            "worktree",
            "add",
            "--detach",
            scratch_root.to_string_lossy().as_ref(),
            target_snapshot_ref,
        ],
    )?;

    let output = Command::new("git")
        .current_dir(&scratch_root)
        .args(["merge", "--no-commit", "--no-ff", candidate_ref])
        .output()
        .with_context(|| {
            format!(
                "failed to run git merge in promotion scratch worktree {}",
                scratch_root.display()
            )
        })?;

    let conflict_paths = git_output(&scratch_root, &["diff", "--name-only", "--diff-filter=U"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    cleanup_promotion_validation_worktree(root, &scratch_root)?;

    if output.status.success() {
        return Ok((
            WavePromotionState::Ready,
            Vec::new(),
            "promotion candidate passed scratch merge validation".to_string(),
        ));
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !conflict_paths.is_empty() {
        return Ok((
            WavePromotionState::Conflicted,
            conflict_paths.clone(),
            format!(
                "promotion blocked by merge conflicts: {}",
                conflict_paths.join(", ")
            ),
        ));
    }

    Ok((
        WavePromotionState::Failed,
        Vec::new(),
        if stderr.is_empty() {
            "promotion merge validation failed".to_string()
        } else {
            format!("promotion merge validation failed: {stderr}")
        },
    ))
}

fn cleanup_promotion_validation_worktree(root: &Path, scratch_root: &Path) -> Result<()> {
    if git_worktree_registered(root, scratch_root)? {
        run_git(
            root,
            &[
                "worktree",
                "remove",
                "--force",
                scratch_root.to_string_lossy().as_ref(),
            ],
        )?;
    }
    if scratch_root.exists() {
        fs::remove_dir_all(scratch_root)
            .with_context(|| format!("failed to remove {}", scratch_root.display()))?;
    }
    Ok(())
}

fn create_workspace_snapshot_commit(
    repo_root: &Path,
    workspace_root: &Path,
    config: &ProjectConfig,
    run_id: &str,
    label: &str,
) -> Result<String> {
    let repo_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let workspace_root = resolve_workspace_root(&repo_root, workspace_root);
    let resolved_paths = config.resolved_paths(&repo_root);
    let derived_dir = resolved_paths.authority.state_derived_dir.join("git");
    fs::create_dir_all(&derived_dir)
        .with_context(|| format!("failed to create {}", derived_dir.display()))?;
    let index_path = derived_dir.join(format!("{run_id}-{label}.index"));
    if index_path.exists() {
        let _ = fs::remove_file(&index_path);
    }
    let envs = [("GIT_INDEX_FILE", index_path.as_path())];
    run_git_with_env(&workspace_root, &["read-tree", "HEAD"], &envs)?;
    // Stage tracked modifications/deletions first, then add only non-ignored
    // untracked files explicitly. `git add -A -- .` will still trip over the
    // ignored `.wave/*` state roots in a live workspace even when exclude
    // pathspecs are present.
    run_git_with_env(&workspace_root, &["add", "-u", "--", "."], &envs)?;
    let untracked_paths = git_output_bytes_with_env(
        &workspace_root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        &envs,
    )?;
    let excluded_prefixes = snapshot_excluded_prefixes(config);
    let filtered_untracked_paths =
        filter_snapshot_untracked_paths(&untracked_paths, &excluded_prefixes);
    if !filtered_untracked_paths.is_empty() {
        git_add_pathspecs_with_env(&workspace_root, &envs, &filtered_untracked_paths)?;
    }
    let tree = git_output_with_env(&workspace_root, &["write-tree"], &envs)?;
    let parent = git_output(&workspace_root, &["rev-parse", "HEAD"])?;
    let commit = git_output_with_env(
        &workspace_root,
        &[
            "commit-tree",
            tree.as_str(),
            "-p",
            parent.as_str(),
            "-m",
            &format!("wave snapshot {run_id} {label}"),
        ],
        &envs,
    )?;
    let _ = fs::remove_file(index_path);
    Ok(commit)
}

fn resolve_workspace_root(repo_root: &Path, workspace_root: &Path) -> PathBuf {
    let resolved = if workspace_root.is_absolute() {
        workspace_root.to_path_buf()
    } else {
        repo_root.join(workspace_root)
    };
    resolved.canonicalize().unwrap_or(resolved)
}

fn current_head_label(root: &Path) -> Result<String> {
    let branch = git_output(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch == "HEAD" {
        git_output(root, &["rev-parse", "HEAD"])
    } else {
        Ok(branch)
    }
}

fn git_worktree_registered(root: &Path, worktree_path: &Path) -> Result<bool> {
    let canonical_target = worktree_path
        .canonicalize()
        .unwrap_or_else(|_| worktree_path.to_path_buf());
    let listing = git_output(root, &["worktree", "list", "--porcelain"])?;
    Ok(listing
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .map(|path| path.canonicalize().unwrap_or(path))
        .any(|path| path == canonical_target))
}

fn append_scheduler_event(
    root: &Path,
    config: &ProjectConfig,
    event: SchedulerEvent,
) -> Result<()> {
    with_scheduler_mutation(root, config, |log| {
        append_scheduler_event_in_log(log, &event)
    })
}

fn append_scheduler_event_in_log(log: &SchedulerEventLog, event: &SchedulerEvent) -> Result<()> {
    log.append(event)
}

fn claim_wave_for_launch(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    wave: &WaveDocument,
    run_id: &str,
    created_at_ms: u128,
) -> Result<WaveClaimRecord> {
    with_scheduler_mutation(root, config, |log| {
        ensure_default_scheduler_budget_in_log(log)?;
        let refreshed_status = refresh_planning_status(root, config, waves)?;
        if !is_claimable_wave(&refreshed_status, wave.metadata.id) {
            let detail = refreshed_status
                .waves
                .iter()
                .find(|entry| entry.id == wave.metadata.id)
                .map(queue_entry_reason)
                .unwrap_or_else(|| refreshed_status.queue.queue_ready_reason.clone());
            return Err(SchedulerAdmissionError {
                wave_id: wave.metadata.id,
                detail,
            }
            .into());
        }

        let claim = WaveClaimRecord {
            claim_id: WaveClaimId::new(format!("claim-wave-{:02}-{run_id}", wave.metadata.id)),
            wave_id: wave.metadata.id,
            state: WaveClaimState::Held,
            owner: runtime_scheduler_owner(run_id),
            claimed_at_ms: created_at_ms,
            released_at_ms: None,
            detail: Some(format!(
                "wave {} claimed for runtime launch",
                wave.metadata.id
            )),
        };
        append_scheduler_event_in_log(
            log,
            &SchedulerEvent::new(
                format!("sched-claim-acquired-{}-{created_at_ms}", wave.metadata.id),
                SchedulerEventKind::WaveClaimAcquired,
            )
            .with_wave_id(wave.metadata.id)
            .with_claim_id(claim.claim_id.clone())
            .with_created_at_ms(created_at_ms)
            .with_correlation_id(run_id.to_string())
            .with_payload(SchedulerEventPayload::WaveClaimUpdated {
                claim: claim.clone(),
            }),
        )?;
        Ok(claim)
    })
}

fn release_wave_claim(
    root: &Path,
    config: &ProjectConfig,
    claim: &WaveClaimRecord,
    detail: impl Into<String>,
) -> Result<()> {
    let released_at_ms = now_epoch_ms()?;
    let mut released = claim.clone();
    released.state = WaveClaimState::Released;
    released.released_at_ms = Some(released_at_ms);
    released.detail = Some(detail.into());
    append_scheduler_event(
        root,
        config,
        SchedulerEvent::new(
            format!("sched-claim-released-{}-{released_at_ms}", claim.wave_id),
            SchedulerEventKind::WaveClaimReleased,
        )
        .with_wave_id(claim.wave_id)
        .with_claim_id(claim.claim_id.clone())
        .with_created_at_ms(released_at_ms)
        .with_correlation_id(
            released
                .owner
                .session_id
                .clone()
                .unwrap_or_else(|| claim.claim_id.as_str().to_string()),
        )
        .with_payload(SchedulerEventPayload::WaveClaimUpdated { claim: released }),
    )
}

fn grant_task_lease(
    root: &Path,
    config: &ProjectConfig,
    run: &WaveRunRecord,
    agent: &WaveAgent,
    claim: &WaveClaimRecord,
    timing: LeaseTiming,
) -> Result<TaskLeaseRecord> {
    with_scheduler_mutation(root, config, |log| {
        ensure_default_scheduler_budget_in_log(log)?;
        let snapshot = build_runtime_scheduler_snapshot(log)?;
        let is_closure = is_closure_agent(agent.id.as_str());
        let fairness_rank = snapshot
            .scheduling_by_wave
            .get(&run.wave_id)
            .map(|record| record.fairness_rank)
            .filter(|rank| *rank > 0)
            .unwrap_or(1);
        let task_id = task_id_for_agent(run.wave_id, agent.id.as_str());
        let reserved_closure_task_leases = snapshot
            .budget
            .reserved_closure_task_leases
            .and_then(|reserved| usize::try_from(reserved).ok())
            .unwrap_or(0);
        let closure_capacity_reserved = snapshot.waiting_closure_waves > 0
            && snapshot.active_closure_task_leases < reserved_closure_task_leases;

        if !is_closure {
            if let Some(capacity) = implementation_task_capacity(&snapshot) {
                if snapshot.active_implementation_task_leases >= capacity {
                    return Err(TaskLeaseCapacityError {
                        wave_id: run.wave_id,
                        task_id: task_id.as_str().to_string(),
                        fairness_rank,
                        protected_closure_capacity: closure_capacity_reserved,
                        detail: if closure_capacity_reserved {
                            "reserved closure capacity is holding back new implementation work"
                                .to_string()
                        } else {
                            "implementation capacity is saturated".to_string()
                        },
                    }
                    .into());
                }
            }
        }

        if let Some(limit) = snapshot
            .budget
            .max_active_task_leases
            .and_then(|value| usize::try_from(value).ok())
        {
            if snapshot.active_leases.len() >= limit {
                if is_closure && snapshot.budget.preemption_enabled {
                    if let Some(candidate) = snapshot
                        .active_leases
                        .iter()
                        .filter(|lease| !task_id_is_closure(&lease.task_id))
                        .max_by_key(|lease| {
                            (lease.granted_at_ms, lease.lease_id.as_str().to_string())
                        })
                        .cloned()
                    {
                        let revoked = close_task_lease_in_log(
                            log,
                            &candidate,
                            TaskLeaseState::Revoked,
                            format!(
                                "preempted to free closure capacity for wave {} agent {}",
                                run.wave_id, agent.id
                            ),
                        )?;
                        let previous = snapshot.scheduling_by_wave.get(&revoked.wave_id).cloned();
                        let preempted = WaveSchedulingRecord {
                            wave_id: revoked.wave_id,
                            phase: previous
                                .as_ref()
                                .map(|record| record.phase)
                                .unwrap_or(WaveExecutionPhase::Implementation),
                            priority: previous
                                .as_ref()
                                .map(|record| record.priority)
                                .unwrap_or(WaveSchedulerPriority::Implementation),
                            state: WaveSchedulingState::Preempted,
                            fairness_rank: previous
                                .as_ref()
                                .map(|record| record.fairness_rank)
                                .filter(|rank| *rank > 0)
                                .unwrap_or(1),
                            waiting_since_ms: previous
                                .as_ref()
                                .and_then(|record| record.waiting_since_ms)
                                .or_else(|| Some(now_epoch_ms().unwrap_or_default())),
                            protected_closure_capacity: false,
                            preemptible: true,
                            last_decision: Some(revoked.detail.clone().unwrap_or_else(|| {
                                format!(
                                    "preempted to free closure capacity for wave {}",
                                    run.wave_id
                                )
                            })),
                            updated_at_ms: now_epoch_ms()?,
                        };
                        publish_scheduling_record_in_log(log, &preempted, &run.run_id)?;
                    } else {
                        return Err(TaskLeaseCapacityError {
                            wave_id: run.wave_id,
                            task_id: task_id.as_str().to_string(),
                            fairness_rank,
                            protected_closure_capacity: true,
                            detail: "closure work is waiting for a non-preemptible task slot"
                                .to_string(),
                        }
                        .into());
                    }
                } else {
                    return Err(TaskLeaseCapacityError {
                        wave_id: run.wave_id,
                        task_id: task_id.as_str().to_string(),
                        fairness_rank,
                        protected_closure_capacity: is_closure,
                        detail: if is_closure {
                            "closure work is waiting for an available task slot".to_string()
                        } else {
                            "task lease budget is saturated".to_string()
                        },
                    }
                    .into());
                }
            }
        }

        let granted_at_ms = now_epoch_ms()?;
        let lease = TaskLeaseRecord {
            lease_id: TaskLeaseId::new(format!(
                "lease-wave-{:02}-{}",
                run.wave_id,
                agent.id.to_ascii_lowercase()
            )),
            wave_id: run.wave_id,
            task_id,
            claim_id: Some(claim.claim_id.clone()),
            state: TaskLeaseState::Granted,
            owner: runtime_scheduler_owner(run.run_id.clone()),
            granted_at_ms,
            heartbeat_at_ms: Some(granted_at_ms),
            expires_at_ms: Some(lease_expiry_ms(granted_at_ms, timing)),
            finished_at_ms: None,
            detail: Some(format!("lease granted for agent {}", agent.id)),
        };
        append_scheduler_event_in_log(
            log,
            &scheduler_event_for_lease(&lease, SchedulerEventKind::TaskLeaseGranted),
        )?;
        Ok(lease)
    })
}

fn renew_task_lease(
    root: &Path,
    config: &ProjectConfig,
    lease: &TaskLeaseRecord,
    timing: LeaseTiming,
    detail: impl Into<String>,
) -> Result<TaskLeaseRecord> {
    with_scheduler_mutation(root, config, |log| {
        let snapshot = build_runtime_scheduler_snapshot(log)?;
        let latest = snapshot
            .latest_leases
            .get(&lease.lease_id)
            .cloned()
            .ok_or_else(|| LeaseRevokedError {
                wave_id: lease.wave_id,
                lease_id: lease.lease_id.as_str().to_string(),
                detail: "lease disappeared from scheduler authority".to_string(),
            })?;
        if !latest.state.is_active() || lease_is_expired(&latest, now_epoch_ms()?) {
            return Err(LeaseRevokedError {
                wave_id: lease.wave_id,
                lease_id: lease.lease_id.as_str().to_string(),
                detail: latest.detail.unwrap_or_else(|| {
                    format!(
                        "scheduler authority recorded lease as {}",
                        lease_state_label(latest.state)
                    )
                }),
            }
            .into());
        }

        let renewed_at_ms = now_epoch_ms()?;
        let mut renewed = latest;
        renewed.state = TaskLeaseState::Granted;
        renewed.heartbeat_at_ms = Some(renewed_at_ms);
        renewed.expires_at_ms = Some(lease_expiry_ms(renewed_at_ms, timing));
        renewed.finished_at_ms = None;
        renewed.detail = Some(detail.into());
        append_scheduler_event_in_log(
            log,
            &scheduler_event_for_lease(&renewed, SchedulerEventKind::TaskLeaseRenewed),
        )?;
        Ok(renewed)
    })
}

fn close_task_lease(
    root: &Path,
    config: &ProjectConfig,
    lease: &TaskLeaseRecord,
    state: TaskLeaseState,
    detail: impl Into<String>,
) -> Result<()> {
    with_scheduler_mutation(root, config, |log| {
        close_task_lease_in_log(log, lease, state, detail)?;
        Ok(())
    })
}

fn lease_state_label(state: TaskLeaseState) -> &'static str {
    match state {
        TaskLeaseState::Granted => "granted",
        TaskLeaseState::Released => "released",
        TaskLeaseState::Expired => "expired",
        TaskLeaseState::Revoked => "revoked",
    }
}

fn worktree_state_label(state: WaveWorktreeState) -> &'static str {
    match state {
        WaveWorktreeState::Allocated => "allocated",
        WaveWorktreeState::Released => "released",
    }
}

fn promotion_state_label(state: WavePromotionState) -> &'static str {
    match state {
        WavePromotionState::NotStarted => "not_started",
        WavePromotionState::Pending => "pending",
        WavePromotionState::Ready => "ready",
        WavePromotionState::Conflicted => "conflicted",
        WavePromotionState::Failed => "failed",
    }
}

fn scheduling_state_label(state: WaveSchedulingState) -> &'static str {
    match state {
        WaveSchedulingState::Waiting => "waiting",
        WaveSchedulingState::Admitted => "admitted",
        WaveSchedulingState::Running => "running",
        WaveSchedulingState::Protected => "protected",
        WaveSchedulingState::Preempted => "preempted",
        WaveSchedulingState::Released => "released",
    }
}

fn is_closure_agent(agent_id: &str) -> bool {
    matches!(agent_id, "A0" | "A8" | "A9")
}

fn is_closure_followup_agent(agent_id: &str) -> bool {
    matches!(agent_id, "A6" | "A7" | "A8" | "A9" | "A0")
}

fn is_promotion_gated_closure_agent(agent_id: &str) -> bool {
    matches!(agent_id, "A8" | "A9" | "A0")
}

fn lease_expiry_ms(heartbeat_at_ms: u128, timing: LeaseTiming) -> u128 {
    heartbeat_at_ms + u128::from(timing.ttl_ms)
}

fn rerun_request_payload(record: &RerunIntentRecord, state: RerunState) -> RerunRequest {
    RerunRequest {
        request_id: RerunRequestId::new(record.request_id.clone().unwrap_or_else(|| {
            format!(
                "rerun-wave-{:02}-{}",
                record.wave_id, record.requested_at_ms
            )
        })),
        wave_id: record.wave_id,
        task_ids: Vec::new(),
        requested_attempt_id: None,
        requested_by: record.requested_by.clone(),
        reason: record.reason.clone(),
        scope: record.scope,
        state,
    }
}

fn append_attempt_event(
    root: &Path,
    config: &ProjectConfig,
    run: &WaveRunRecord,
    agent: &WaveAgent,
    state: AttemptState,
    created_at_ms: u128,
    started_at_ms: Option<u128>,
    runtime: Option<RuntimeExecutionRecord>,
) -> Result<()> {
    let task_id = task_id_for_agent(run.wave_id, agent.id.as_str());
    let attempt_id = attempt_id_for_run_agent(run.run_id.as_str(), agent.id.as_str());
    let event_created_at_ms = now_epoch_ms()?;
    let event_kind = match state {
        AttemptState::Planned => ControlEventKind::AttemptPlanned,
        AttemptState::Running => ControlEventKind::AttemptStarted,
        AttemptState::Succeeded
        | AttemptState::Failed
        | AttemptState::Aborted
        | AttemptState::Refused => ControlEventKind::AttemptFinished,
    };
    let attempt = AttemptRecord {
        attempt_id: attempt_id.clone(),
        wave_id: run.wave_id,
        task_id: task_id.clone(),
        attempt_number: 1,
        state,
        executor: runtime
            .as_ref()
            .map(|runtime| runtime.execution_identity.adapter.clone())
            .unwrap_or_else(|| {
                runtime_selection_policy_for_agent(agent)
                    .requested_runtime
                    .unwrap_or(RuntimeId::Codex)
                    .to_string()
            }),
        created_at_ms,
        started_at_ms,
        finished_at_ms: state.is_terminal().then_some(event_created_at_ms),
        summary: None,
        proof_bundle_ids: Vec::new(),
        result_envelope_id: None,
        runtime,
    };

    append_control_event(
        root,
        config,
        ControlEvent::new(
            format!(
                "evt-attempt-{}-{}-{}",
                state_label(state),
                run.wave_id,
                event_created_at_ms
            ),
            event_kind,
            run.wave_id,
        )
        .with_task_id(task_id)
        .with_attempt_id(attempt_id)
        .with_created_at_ms(event_created_at_ms)
        .with_correlation_id(run.run_id.clone())
        .with_payload(ControlEventPayload::AttemptUpdated { attempt }),
    )
}

fn attempt_id_for_run_agent(run_id: &str, agent_id: &str) -> AttemptId {
    AttemptId::new(format!("{run_id}-{}", agent_id.to_ascii_lowercase()))
}

fn control_event_for_result_envelope(
    run: &WaveRunRecord,
    agent: &WaveAgent,
    envelope: &ResultEnvelope,
) -> ControlEvent {
    ControlEvent::new(
        format!(
            "evt-result-envelope-{}-{}",
            run.wave_id, envelope.created_at_ms
        ),
        ControlEventKind::ResultEnvelopeRecorded,
        run.wave_id,
    )
    .with_task_id(task_id_for_agent(run.wave_id, agent.id.as_str()))
    .with_attempt_id(envelope.attempt_id.clone())
    .with_created_at_ms(envelope.created_at_ms)
    .with_correlation_id(run.run_id.clone())
    .with_payload(ControlEventPayload::ResultEnvelopeRecorded {
        result: envelope.clone(),
    })
}

fn attempt_state_from_agent_status(status: WaveRunStatus) -> AttemptState {
    match status {
        WaveRunStatus::Planned => AttemptState::Planned,
        WaveRunStatus::Running => AttemptState::Running,
        WaveRunStatus::Succeeded => AttemptState::Succeeded,
        WaveRunStatus::Failed => AttemptState::Failed,
        WaveRunStatus::DryRun => AttemptState::Refused,
    }
}

fn state_label(state: AttemptState) -> &'static str {
    match state {
        AttemptState::Planned => "planned",
        AttemptState::Running => "started",
        AttemptState::Succeeded => "succeeded",
        AttemptState::Failed => "failed",
        AttemptState::Aborted => "aborted",
        AttemptState::Refused => "refused",
    }
}

fn task_lease_capacity_error(error: &anyhow::Error) -> Option<&TaskLeaseCapacityError> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<TaskLeaseCapacityError>())
}

fn lease_revoked_error(error: &anyhow::Error) -> Option<&LeaseRevokedError> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<LeaseRevokedError>())
}

fn scheduling_axes_for_agent(
    agent_id: &str,
) -> (WaveExecutionPhase, WaveSchedulerPriority, bool, bool) {
    if is_closure_agent(agent_id) {
        (
            WaveExecutionPhase::Closure,
            WaveSchedulerPriority::Closure,
            true,
            false,
        )
    } else {
        (
            WaveExecutionPhase::Implementation,
            WaveSchedulerPriority::Implementation,
            false,
            true,
        )
    }
}

fn acquire_task_lease_for_agent(
    root: &Path,
    config: &ProjectConfig,
    state_path: &Path,
    record: &mut WaveRunRecord,
    agent: &WaveAgent,
    claim: &WaveClaimRecord,
    timing: LeaseTiming,
) -> Result<TaskLeaseRecord> {
    let (phase, priority, protected_closure_capacity, preemptible) =
        scheduling_axes_for_agent(agent.id.as_str());
    let waiting_since_ms = record
        .scheduling
        .as_ref()
        .and_then(|scheduling| scheduling.waiting_since_ms)
        .unwrap_or(now_epoch_ms()?);

    loop {
        match grant_task_lease(root, config, record, agent, claim, timing) {
            Ok(lease) => return Ok(lease),
            Err(error) => {
                let Some(capacity_error) = task_lease_capacity_error(&error) else {
                    return Err(error);
                };
                record.scheduling = Some(publish_scheduling_record(
                    root,
                    config,
                    WaveSchedulingRecord {
                        wave_id: record.wave_id,
                        phase,
                        priority,
                        state: if capacity_error.protected_closure_capacity {
                            WaveSchedulingState::Protected
                        } else {
                            WaveSchedulingState::Waiting
                        },
                        fairness_rank: capacity_error.fairness_rank.max(1),
                        waiting_since_ms: Some(waiting_since_ms),
                        protected_closure_capacity,
                        preemptible,
                        last_decision: Some(capacity_error.detail.clone()),
                        updated_at_ms: now_epoch_ms()?,
                    },
                    &record.run_id,
                )?);
                write_run_record(state_path, record)?;
                thread::sleep(Duration::from_millis(timing.poll_interval_ms));
            }
        }
    }
}

fn finish_failed_launch(
    root: &Path,
    config: &ProjectConfig,
    bundle: &DraftBundle,
    preflight_path: &Path,
    state_path: &Path,
    trace_path: &Path,
    record: &mut WaveRunRecord,
    agent: &WaveAgent,
    agent_index: usize,
    error: anyhow::Error,
) -> Result<LaunchReport> {
    let reason = error.to_string();
    if record.started_at_ms.is_none() {
        record.started_at_ms = Some(now_epoch_ms()?);
    }
    record.agents[agent_index].status = WaveRunStatus::Failed;
    record.agents[agent_index].exit_code = None;
    record.agents[agent_index].error = Some(reason.clone());
    record.agents[agent_index].observed_markers.clear();
    ensure_orphan_agent_artifacts(&record.agents[agent_index], &reason)?;

    record.status = WaveRunStatus::Failed;
    record.error = Some(reason.clone());
    record.completed_at_ms = Some(now_epoch_ms()?);
    if let Some(worktree) = record.worktree.clone() {
        record.worktree = Some(release_wave_worktree(
            root,
            config,
            &worktree,
            &record.run_id,
            format!("launch failed: {reason}"),
        )?);
    }
    if let Some(scheduling) = record.scheduling.clone() {
        record.scheduling = Some(publish_scheduling_record(
            root,
            config,
            WaveSchedulingRecord {
                state: WaveSchedulingState::Released,
                preemptible: false,
                last_decision: Some(format!("launch failed: {reason}")),
                updated_at_ms: now_epoch_ms()?,
                ..scheduling
            },
            &record.run_id,
        )?);
    }
    append_attempt_event(
        root,
        config,
        record,
        agent,
        AttemptState::Failed,
        record.created_at_ms,
        record.started_at_ms,
        record
            .agents
            .get(agent_index)
            .and_then(|agent_record| agent_record.runtime.clone()),
    )?;
    cleanup_scheduler_ownership_for_run(root, config, record, &format!("launch failed: {reason}"))?;
    write_run_record(state_path, record)?;
    write_trace_bundle(trace_path, record)?;

    Ok(LaunchReport {
        run_id: record.run_id.clone(),
        wave_id: record.wave_id,
        status: record.status,
        state_path: state_path.to_path_buf(),
        trace_path: trace_path.to_path_buf(),
        bundle_dir: bundle.bundle_dir.clone(),
        preflight_path: preflight_path.to_path_buf(),
    })
}

fn reconcile_orphaned_run_record(record: &mut WaveRunRecord) -> Result<bool> {
    let Some(reason) = orphaned_run_reason(record) else {
        return Ok(false);
    };
    mark_orphaned_run_failed(record, &reason)?;
    Ok(true)
}

fn orphaned_run_reason(record: &WaveRunRecord) -> Option<String> {
    if record.dry_run || record.completed_at_ms.is_some() {
        return None;
    }
    if !matches!(
        record.status,
        WaveRunStatus::Planned | WaveRunStatus::Running
    ) {
        return None;
    }

    let launcher_pid = record.launcher_pid?;
    match launcher_liveness(record) {
        LauncherLiveness::Alive => return None,
        LauncherLiveness::Missing => {}
        LauncherLiveness::MismatchedIdentity {
            observed_started_at_ms,
        } => {
            return Some(format!(
                "launcher process {} no longer matches recorded session (observed start={})",
                launcher_pid, observed_started_at_ms
            ));
        }
    }

    Some(format!(
        "launcher process {} exited before run completion was recorded",
        launcher_pid
    ))
}

fn mark_orphaned_run_failed(record: &mut WaveRunRecord, reason: &str) -> Result<()> {
    let completed_at_ms = now_epoch_ms()?;
    record.status = WaveRunStatus::Failed;
    record.error = Some(reason.to_string());
    if record.started_at_ms.is_none() {
        record.started_at_ms = Some(completed_at_ms);
    }
    record.completed_at_ms = Some(completed_at_ms);

    if let Some(agent_index) = record.agents.iter().position(|agent| {
        matches!(
            agent.status,
            WaveRunStatus::Running | WaveRunStatus::Planned
        )
    }) {
        let agent = &mut record.agents[agent_index];
        agent.status = WaveRunStatus::Failed;
        agent.exit_code = None;
        agent.error = Some(reason.to_string());
        agent.observed_markers.clear();
        ensure_orphan_agent_artifacts(agent, reason)?;
    }

    Ok(())
}

fn ensure_orphan_agent_artifacts(agent: &AgentRunRecord, reason: &str) -> Result<()> {
    write_missing_text_artifact(&agent.last_message_path, &format!("{reason}\n"))?;
    write_missing_text_artifact(&agent.events_path, "")?;
    write_missing_text_artifact(&agent.stderr_path, &format!("{reason}\n"))?;
    Ok(())
}

fn cleanup_scheduler_ownership_for_run(
    root: &Path,
    config: &ProjectConfig,
    run: &WaveRunRecord,
    detail: &str,
) -> Result<()> {
    let mut claim = None;
    let mut leases = HashMap::new();
    let mut events = scheduler_event_log(root, config).load_all()?;
    events.sort_by_key(|event| (event.created_at_ms, event.event_id.clone()));

    for event in events {
        match event.payload {
            SchedulerEventPayload::WaveClaimUpdated { claim: record }
                if record.wave_id == run.wave_id
                    && record.owner.session_id.as_deref() == Some(run.run_id.as_str()) =>
            {
                claim = record.state.is_held().then_some(record);
            }
            SchedulerEventPayload::TaskLeaseUpdated { lease }
                if lease.wave_id == run.wave_id
                    && lease.owner.session_id.as_deref() == Some(run.run_id.as_str()) =>
            {
                if lease.state.is_active() {
                    leases.insert(lease.lease_id.clone(), lease);
                } else {
                    leases.remove(&lease.lease_id);
                }
            }
            _ => {}
        }
    }

    let now_ms = now_epoch_ms()?;
    for lease in leases.into_values() {
        let state = if lease_is_expired(&lease, now_ms) {
            TaskLeaseState::Expired
        } else {
            TaskLeaseState::Revoked
        };
        close_task_lease(root, config, &lease, state, detail.to_string())?;
    }
    if let Some(claim) = claim.as_ref() {
        release_wave_claim(root, config, claim, detail.to_string())?;
    }
    Ok(())
}

fn write_missing_text_artifact(path: &Path, contents: &str) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    if path.exists() {
        return Ok(());
    }
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn process_is_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new("/proc").join(pid.to_string()).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LauncherLiveness {
    Alive,
    Missing,
    MismatchedIdentity { observed_started_at_ms: u128 },
}

fn launcher_liveness(record: &WaveRunRecord) -> LauncherLiveness {
    const START_TIME_TOLERANCE_MS: u128 = 1_000;
    let Some(launcher_pid) = record.launcher_pid else {
        return LauncherLiveness::Missing;
    };
    if !process_is_alive(launcher_pid) {
        return LauncherLiveness::Missing;
    }

    let observed_started_at_ms = process_started_at_ms(launcher_pid);
    if let (Some(expected), Some(observed)) =
        (record.launcher_started_at_ms, observed_started_at_ms)
    {
        if expected.abs_diff(observed) <= START_TIME_TOLERANCE_MS {
            return LauncherLiveness::Alive;
        }
        return LauncherLiveness::MismatchedIdentity {
            observed_started_at_ms: observed,
        };
    }

    if let Some(observed) = observed_started_at_ms {
        if observed > record.created_at_ms.saturating_add(1_000) {
            return LauncherLiveness::MismatchedIdentity {
                observed_started_at_ms: observed,
            };
        }
    }

    LauncherLiveness::Alive
}

fn current_process_started_at_ms() -> Option<u128> {
    process_started_at_ms(std::process::id())
}

#[cfg(target_os = "linux")]
fn process_started_at_ms(pid: u32) -> Option<u128> {
    let stat = fs::read_to_string(Path::new("/proc").join(pid.to_string()).join("stat")).ok()?;
    let close_paren = stat.rfind(')')?;
    let remainder = stat.get(close_paren + 2..)?;
    let fields = remainder.split_whitespace().collect::<Vec<_>>();
    let start_ticks = fields.get(19)?.parse::<u128>().ok()?;
    let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks_per_second <= 0 {
        return None;
    }
    let ticks_per_second = u128::try_from(ticks_per_second).ok()?;
    let uptime = fs::read_to_string("/proc/uptime").ok()?;
    let uptime_secs = uptime.split_whitespace().next()?.parse::<f64>().ok()?;
    let uptime_ms = (uptime_secs * 1000.0) as u128;
    let boot_time_ms = now_epoch_ms().ok()?.checked_sub(uptime_ms)?;
    Some(boot_time_ms + (start_ticks * 1000 / ticks_per_second))
}

#[cfg(not(target_os = "linux"))]
fn process_started_at_ms(_pid: u32) -> Option<u128> {
    None
}

fn persist_agent_result_envelope(
    root: &Path,
    config: &ProjectConfig,
    run: &WaveRunRecord,
    declared_agent: &WaveAgent,
    agent_record: &AgentRunRecord,
) -> Result<AgentRunRecord> {
    let envelope =
        build_structured_result_envelope(root, run, declared_agent, agent_record, now_epoch_ms()?)?;
    let envelope_path = ResultEnvelopeStore::under_repo(root).write_envelope(&envelope)?;
    append_control_event(
        root,
        config,
        control_event_for_result_envelope(run, declared_agent, &envelope),
    )?;

    let mut updated = agent_record.clone();
    updated.result_envelope_path = Some(envelope_path);
    Ok(updated)
}

fn execute_agent(
    root: &Path,
    config: &ProjectConfig,
    run: &WaveRunRecord,
    agent: &WaveAgent,
    base_record: &AgentRunRecord,
    runtime_plan: &ResolvedRuntimePlan,
    codex_home: &Path,
    initial_lease: &TaskLeaseRecord,
    timing: LeaseTiming,
    registry: &RuntimeAdapterRegistry,
) -> Result<ExecutedAgent> {
    let agent_dir = base_record
        .prompt_path
        .parent()
        .context("agent prompt path has no parent directory")?;
    fs::create_dir_all(agent_dir)
        .with_context(|| format!("failed to create {}", agent_dir.display()))?;
    let adapter = registry.adapter(runtime_plan.runtime.selected_runtime)?;
    let request = build_runtime_execution_request(runtime_plan, codex_home);
    let mut spawned = adapter.execute(request)?;
    let (status, lease) = wait_for_agent_exit_with_lease(
        root,
        config,
        agent.id.as_str(),
        &mut spawned.child,
        initial_lease,
        timing,
    )?;
    if runtime_plan.runtime.selected_runtime == RuntimeId::Claude {
        append_json_event(
            &base_record.events_path,
            serde_json::json!({
                "event": "exit",
                "runtime": "claude",
                "agent": agent.id,
                "exit_code": status.code(),
                "ok": status.success(),
            }),
        )?;
    }

    let initial_error = if status.success() {
        None
    } else {
        Some(format!(
            "{} exited with {}",
            spawned.failure_label,
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string())
        ))
    };
    let runtime_detail_path =
        artifact_path_from_runtime(&runtime_plan.runtime, "runtime_detail").map(PathBuf::from);
    let provisional_record = AgentRunRecord {
        status: if status.success() {
            WaveRunStatus::Succeeded
        } else {
            WaveRunStatus::Failed
        },
        exit_code: status.code(),
        error: initial_error.clone(),
        observed_markers: Vec::new(),
        runtime_detail_path: runtime_detail_path.clone(),
        runtime: Some(runtime_plan.runtime.clone()),
        ..base_record.clone()
    };
    let envelope =
        build_structured_result_envelope(root, run, agent, &provisional_record, now_epoch_ms()?)?;
    let observed_markers = envelope.closure_input.final_markers.observed.clone();

    if !status.success() {
        return Ok(ExecutedAgent {
            record: AgentRunRecord {
                status: WaveRunStatus::Failed,
                exit_code: status.code(),
                error: initial_error,
                observed_markers,
                runtime_detail_path: runtime_detail_path.clone(),
                runtime: Some(runtime_plan.runtime.clone()),
                ..base_record.clone()
            },
            lease,
        });
    }

    if envelope.closure.disposition != ClosureDisposition::Ready {
        return Ok(ExecutedAgent {
            record: AgentRunRecord {
                status: WaveRunStatus::Failed,
                exit_code: status.code(),
                error: Some(
                    envelope
                        .closure
                        .blocking_reasons
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "structured result envelope is not ready".to_string()),
                ),
                observed_markers,
                runtime_detail_path: runtime_detail_path.clone(),
                runtime: Some(runtime_plan.runtime.clone()),
                ..base_record.clone()
            },
            lease,
        });
    }

    if let Some(error) = result_closure_contract_error(agent.id.as_str(), &envelope.closure) {
        return Ok(ExecutedAgent {
            record: AgentRunRecord {
                status: WaveRunStatus::Failed,
                exit_code: status.code(),
                error: Some(error),
                observed_markers,
                runtime_detail_path: runtime_detail_path.clone(),
                runtime: Some(runtime_plan.runtime.clone()),
                ..base_record.clone()
            },
            lease,
        });
    }

    Ok(ExecutedAgent {
        record: AgentRunRecord {
            status: WaveRunStatus::Succeeded,
            exit_code: status.code(),
            error: None,
            observed_markers,
            runtime_detail_path,
            runtime: Some(runtime_plan.runtime.clone()),
            ..base_record.clone()
        },
        lease,
    })
}

fn build_runtime_execution_request(
    runtime_plan: &ResolvedRuntimePlan,
    codex_home: &Path,
) -> RuntimeExecutionRequest {
    let invocation = match &runtime_plan.adapter_config {
        RuntimeAdapterConfig::Codex(config) => RuntimeAdapterInvocation::Codex(CodexInvocation {
            model: config.model.clone(),
            config_entries: config.config_entries.clone(),
            codex_home: codex_home.to_path_buf(),
        }),
        RuntimeAdapterConfig::Claude(config) => {
            RuntimeAdapterInvocation::Claude(ClaudeInvocation {
                model: config.model.clone(),
                agent: config.agent.clone(),
                permission_mode: config.permission_mode.clone(),
                permission_prompt_tool: config.permission_prompt_tool.clone(),
                effort: config.effort.clone(),
                max_turns: config.max_turns.clone(),
                mcp_config_paths: config.mcp_config_paths.clone(),
                strict_mcp_config: config.strict_mcp_config,
                output_format: config.output_format.clone(),
                allowed_tools: config.allowed_tools.clone(),
                disallowed_tools: config.disallowed_tools.clone(),
                system_prompt_path: config.system_prompt_path.clone(),
                settings_path: config.settings_path.clone(),
            })
        }
    };

    RuntimeExecutionRequest {
        identity: runtime_plan.runtime.execution_identity.clone(),
        launch: runtime_plan.launch.clone(),
        invocation,
    }
}

fn latest_recorded_lease(
    root: &Path,
    config: &ProjectConfig,
    lease_id: &TaskLeaseId,
) -> Result<Option<TaskLeaseRecord>> {
    let snapshot = build_runtime_scheduler_snapshot(&scheduler_event_log(root, config))?;
    Ok(snapshot.latest_leases.get(lease_id).cloned())
}

fn wait_for_agent_exit_with_lease(
    root: &Path,
    config: &ProjectConfig,
    agent_id: &str,
    child: &mut Child,
    initial_lease: &TaskLeaseRecord,
    timing: LeaseTiming,
) -> Result<(ExitStatus, TaskLeaseRecord)> {
    let mut lease = initial_lease.clone();
    let mut next_heartbeat_at_ms = lease.granted_at_ms + u128::from(timing.heartbeat_interval_ms);
    loop {
        let now = now_epoch_ms()?;
        let latest = latest_recorded_lease(root, config, &lease.lease_id)?;
        if let Some(latest) = latest {
            if latest.state.is_active() && lease_is_expired(&latest, now) {
                terminate_child(child)
                    .context("failed to stop runtime process after lease expiry")?;
                close_task_lease(
                    root,
                    config,
                    &latest,
                    TaskLeaseState::Expired,
                    format!("lease expired while agent {agent_id} was still running"),
                )?;
                bail!("agent {agent_id} lost its lease before completion");
            }
            if !latest.state.is_active() {
                terminate_child(child)
                    .context("failed to stop runtime process after lease revocation")?;
                return Err(LeaseRevokedError {
                    wave_id: latest.wave_id,
                    lease_id: latest.lease_id.as_str().to_string(),
                    detail: latest.detail.unwrap_or_else(|| {
                        format!(
                            "scheduler authority recorded lease as {}",
                            lease_state_label(latest.state)
                        )
                    }),
                }
                .into());
            }
            lease = latest;
        }

        if let Some(status) = child
            .try_wait()
            .context("failed while waiting for runtime process")?
        {
            return Ok((status, lease));
        }

        if now >= next_heartbeat_at_ms {
            if lease_is_expired(&lease, now) {
                terminate_child(child)
                    .context("failed to stop runtime process after lease expiry")?;
                close_task_lease(
                    root,
                    config,
                    &lease,
                    TaskLeaseState::Expired,
                    format!("lease expired while agent {agent_id} was still running"),
                )?;
                bail!("agent {agent_id} lost its lease before completion");
            }
            lease = match renew_task_lease(
                root,
                config,
                &lease,
                timing,
                format!("lease heartbeat renewed for agent {agent_id}"),
            ) {
                Ok(lease) => lease,
                Err(error) => {
                    terminate_child(child)
                        .context("failed to stop runtime process after lease renewal failure")?;
                    return Err(error).context(format!(
                        "lease renewal failed while agent {agent_id} was still running"
                    ));
                }
            };
            next_heartbeat_at_ms = now + u128::from(timing.heartbeat_interval_ms);
        }

        thread::sleep(Duration::from_millis(timing.poll_interval_ms));
    }
}

fn terminate_child(child: &mut Child) -> Result<()> {
    match child.kill() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => {}
        Err(error) => return Err(error).context("failed to kill runtime process"),
    }
    let _ = child.wait();
    Ok(())
}

fn lease_is_expired(lease: &TaskLeaseRecord, now_ms: u128) -> bool {
    lease
        .expires_at_ms
        .map(|expires_at_ms| now_ms >= expires_at_ms)
        .unwrap_or(false)
}

fn resolve_runtime_plan(
    _root: &Path,
    execution_root: &Path,
    run: &WaveRunRecord,
    agent: &WaveAgent,
    base_record: &AgentRunRecord,
    base_prompt: &str,
    registry: &RuntimeAdapterRegistry,
) -> Result<ResolvedRuntimePlan> {
    let policy = normalized_runtime_policy(agent);
    let requested_runtime = policy
        .requested_runtime
        .expect("normalized runtime policy always sets requested runtime");
    let requested_reason = runtime_selection_reason(&policy, requested_runtime);

    let mut selected_runtime = None;
    let mut first_unavailable_detail = None;
    let mut availability_details = Vec::new();
    for candidate in &policy.allowed_runtimes {
        let availability = runtime_availability_for(registry, *candidate);
        if availability.available {
            selected_runtime = Some((candidate.to_owned(), availability));
            break;
        }
        availability_details.push(format!("{}: {}", candidate.as_str(), availability.detail));
        if first_unavailable_detail.is_none() {
            first_unavailable_detail = Some(availability.detail.clone());
        }
    }

    let (selected_runtime, availability) = selected_runtime.with_context(|| {
        if availability_details.is_empty() {
            format!("agent {} has no available runtime candidates", agent.id)
        } else {
            format!(
                "agent {} has no available runtime candidates: {}",
                agent.id,
                availability_details.join("; ")
            )
        }
    })?;

    let fallback = (selected_runtime != requested_runtime).then(|| RuntimeFallbackRecord {
        requested_runtime,
        selected_runtime,
        reason: first_unavailable_detail
            .unwrap_or_else(|| "requested runtime was unavailable".to_string()),
    });
    let selection_reason = fallback
        .as_ref()
        .map(|fallback| {
            format!(
                "selected {} after fallback because {}",
                fallback.selected_runtime, fallback.reason
            )
        })
        .unwrap_or(requested_reason);
    let skill_projection = project_runtime_skills(execution_root, agent, selected_runtime)?;
    let mut runtime = RuntimeExecutionRecord {
        policy,
        selected_runtime,
        selection_reason,
        fallback,
        execution_identity: RuntimeExecutionIdentity {
            runtime: selected_runtime,
            adapter: format!("wave-runtime/{}", selected_runtime.as_str()),
            binary: availability.binary,
            provider: runtime_provider_label(selected_runtime).to_string(),
            artifact_paths: BTreeMap::new(),
        },
        skill_projection,
    };
    let prompt = write_runtime_artifacts(
        execution_root,
        run,
        agent,
        base_record,
        base_prompt,
        &mut runtime,
    )?;
    let launch = build_runtime_launch_spec(execution_root, base_record, &prompt, &runtime);
    let adapter_config = resolve_runtime_adapter_config(execution_root, agent, &runtime)?;

    Ok(ResolvedRuntimePlan {
        runtime,
        launch,
        adapter_config,
    })
}

fn normalized_runtime_policy(agent: &WaveAgent) -> RuntimeSelectionPolicy {
    let mut policy = runtime_selection_policy_for_agent(agent);
    if policy.requested_runtime.is_none() {
        policy.requested_runtime = Some(RuntimeId::Codex);
        policy.selection_source = Some("default.codex".to_string());
    }
    if let Some(requested) = policy.requested_runtime {
        if !policy
            .allowed_runtimes
            .iter()
            .any(|runtime| runtime == &requested)
        {
            policy.allowed_runtimes.insert(0, requested);
        }
    }
    policy
}

fn build_runtime_launch_spec(
    execution_root: &Path,
    base_record: &AgentRunRecord,
    prompt: &str,
    runtime: &RuntimeExecutionRecord,
) -> RuntimeLaunchSpec {
    RuntimeLaunchSpec {
        agent_id: base_record.id.clone(),
        execution_root: execution_root.to_path_buf(),
        prompt: prompt.to_string(),
        last_message_path: base_record.last_message_path.clone(),
        events_path: base_record.events_path.clone(),
        stderr_path: base_record.stderr_path.clone(),
        projected_skill_dirs: projected_skill_dirs(execution_root, runtime),
    }
}

fn resolve_runtime_adapter_config(
    execution_root: &Path,
    agent: &WaveAgent,
    runtime: &RuntimeExecutionRecord,
) -> Result<RuntimeAdapterConfig> {
    match runtime.selected_runtime {
        RuntimeId::Codex => Ok(RuntimeAdapterConfig::Codex(CodexAdapterConfig {
            model: resolved_codex_model(agent),
            config_entries: resolved_codex_config_entries(agent),
        })),
        RuntimeId::Claude => Ok(RuntimeAdapterConfig::Claude(ClaudeAdapterConfig {
            model: agent.executor.get("model").cloned(),
            agent: agent.executor.get("claude.agent").cloned(),
            permission_mode: agent.executor.get("claude.permission_mode").cloned(),
            permission_prompt_tool: agent.executor.get("claude.permission_prompt_tool").cloned(),
            effort: agent.executor.get("claude.effort").cloned(),
            max_turns: agent.executor.get("claude.max_turns").cloned(),
            mcp_config_paths: agent
                .executor
                .get("claude.mcp_config")
                .map(|value| {
                    parse_list_value(value)
                        .into_iter()
                        .map(|path| resolve_runtime_file_path(execution_root, &path))
                        .collect()
                })
                .unwrap_or_default(),
            strict_mcp_config: parse_truthy_flag(agent.executor.get("claude.strict_mcp_config")),
            output_format: agent.executor.get("claude.output_format").cloned(),
            allowed_tools: agent
                .executor
                .get("claude.allowed_tools")
                .map(|value| parse_list_value(value))
                .unwrap_or_default(),
            disallowed_tools: agent
                .executor
                .get("claude.disallowed_tools")
                .map(|value| parse_list_value(value))
                .unwrap_or_default(),
            system_prompt_path: PathBuf::from(
                artifact_path_from_runtime(runtime, "system_prompt")
                    .context("missing Claude system prompt artifact")?,
            ),
            settings_path: artifact_path_from_runtime(runtime, "settings").map(PathBuf::from),
        })),
        other => bail!("runtime {other} is not implemented in Wave 15"),
    }
}

fn runtime_availability_for(
    registry: &RuntimeAdapterRegistry,
    runtime: RuntimeId,
) -> RuntimeAvailability {
    match registry.adapter(runtime) {
        Ok(adapter) => adapter.availability(),
        Err(error) => RuntimeAvailability {
            runtime,
            binary: runtime.as_str().to_string(),
            available: false,
            detail: error.to_string(),
        },
    }
}

fn runtime_selection_reason(policy: &RuntimeSelectionPolicy, runtime: RuntimeId) -> String {
    match policy.selection_source.as_deref() {
        Some("executor.id") => format!("selected {runtime} from executor.id"),
        Some("executor.profile") => format!("selected {runtime} from executor.profile"),
        Some("default.codex") => {
            "selected codex because the agent did not author an explicit runtime".to_string()
        }
        Some(source) if source.starts_with("executor.") && source.ends_with("-fields") => {
            format!("selected {runtime} because the executor declares runtime-specific fields")
        }
        Some(source) => format!("selected {runtime} from {source}"),
        None => format!("selected {runtime}"),
    }
}

fn runtime_provider_label(runtime: RuntimeId) -> &'static str {
    match runtime {
        RuntimeId::Codex => "openai-codex-cli",
        RuntimeId::Claude => "anthropic-claude-code",
        RuntimeId::Opencode => "opencode",
        RuntimeId::Local => "local",
    }
}

fn project_runtime_skills(
    execution_root: &Path,
    agent: &WaveAgent,
    selected_runtime: RuntimeId,
) -> Result<RuntimeSkillProjection> {
    let declared_skills = dedup_string_values(agent.skills.clone());
    let mut projected_skills = Vec::new();
    let mut dropped_skills = Vec::new();

    for skill in &declared_skills {
        if !skill_bundle_exists(execution_root, skill) {
            dropped_skills.push(skill.clone());
            continue;
        }
        let runtimes = skill_runtimes(execution_root, skill)?;
        if runtimes.is_empty() || runtimes.iter().any(|runtime| *runtime == selected_runtime) {
            projected_skills.push(skill.clone());
        } else {
            dropped_skills.push(skill.clone());
        }
    }

    let runtime_skill = format!("runtime-{}", selected_runtime.as_str());
    let mut auto_attached_skills = Vec::new();
    if skill_bundle_exists(execution_root, &runtime_skill)
        && !projected_skills.iter().any(|skill| skill == &runtime_skill)
    {
        projected_skills.push(runtime_skill.clone());
        auto_attached_skills.push(runtime_skill);
    }

    Ok(RuntimeSkillProjection {
        declared_skills,
        projected_skills: dedup_string_values(projected_skills),
        dropped_skills: dedup_string_values(dropped_skills),
        auto_attached_skills,
    })
}

fn skill_bundle_dir(execution_root: &Path, skill_id: &str) -> PathBuf {
    execution_root.join("skills").join(skill_id)
}

fn skill_bundle_exists(execution_root: &Path, skill_id: &str) -> bool {
    skill_bundle_dir(execution_root, skill_id).is_dir()
}

fn skill_runtimes(execution_root: &Path, skill_id: &str) -> Result<Vec<RuntimeId>> {
    let manifest_path = skill_bundle_dir(execution_root, skill_id).join("skill.json");
    if !manifest_path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest = serde_json::from_str::<SkillManifest>(&raw)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    let runtimes = manifest
        .activation
        .map(|activation| {
            activation
                .runtimes
                .into_iter()
                .filter_map(|runtime| RuntimeId::parse(&runtime))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(runtimes)
}

fn dedup_string_values(values: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        if !deduped.iter().any(|existing| existing == &value) {
            deduped.push(value);
        }
    }
    deduped
}

fn write_runtime_artifacts(
    execution_root: &Path,
    run: &WaveRunRecord,
    agent: &WaveAgent,
    base_record: &AgentRunRecord,
    base_prompt: &str,
    runtime: &mut RuntimeExecutionRecord,
) -> Result<String> {
    let agent_dir = base_record
        .prompt_path
        .parent()
        .context("agent prompt path has no parent directory")?;
    fs::create_dir_all(agent_dir)
        .with_context(|| format!("failed to create {}", agent_dir.display()))?;

    let overlay_text = render_runtime_overlay(execution_root, runtime);
    let overlay_path = agent_dir.join("runtime-skill-overlay.md");
    fs::write(&overlay_path, &overlay_text)
        .with_context(|| format!("failed to write {}", overlay_path.display()))?;
    runtime.execution_identity.artifact_paths.insert(
        "skill_overlay".to_string(),
        overlay_path.to_string_lossy().into_owned(),
    );

    let runtime_prompt = format!("{base_prompt}\n\n{overlay_text}");
    let runtime_prompt_path = agent_dir.join("runtime-prompt.md");
    fs::write(&runtime_prompt_path, &runtime_prompt)
        .with_context(|| format!("failed to write {}", runtime_prompt_path.display()))?;
    runtime.execution_identity.artifact_paths.insert(
        "prompt".to_string(),
        runtime_prompt_path.to_string_lossy().into_owned(),
    );

    if runtime.selected_runtime == RuntimeId::Claude {
        let system_prompt = render_claude_system_prompt(execution_root, runtime);
        let system_prompt_path = agent_dir.join("claude-system-prompt.txt");
        fs::write(&system_prompt_path, system_prompt)
            .with_context(|| format!("failed to write {}", system_prompt_path.display()))?;
        runtime.execution_identity.artifact_paths.insert(
            "system_prompt".to_string(),
            system_prompt_path.to_string_lossy().into_owned(),
        );

        let base_settings_path = resolved_claude_settings_path(execution_root, agent)?;
        if let Some(settings) = build_claude_settings_overlay(base_settings_path.as_deref(), agent)?
        {
            let settings_path = agent_dir.join("claude-settings.json");
            fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)
                .with_context(|| format!("failed to write {}", settings_path.display()))?;
            runtime.execution_identity.artifact_paths.insert(
                "settings".to_string(),
                settings_path.to_string_lossy().into_owned(),
            );
        } else if let Some(base_settings_path) = base_settings_path.as_ref() {
            runtime.execution_identity.artifact_paths.insert(
                "settings".to_string(),
                base_settings_path.to_string_lossy().into_owned(),
            );
        }
    }

    let runtime_detail_path = agent_dir.join("runtime-detail.json");
    runtime.execution_identity.artifact_paths.insert(
        "runtime_detail".to_string(),
        runtime_detail_path.to_string_lossy().into_owned(),
    );
    let snapshot = RuntimeDetailSnapshot {
        wave_id: run.wave_id,
        run_id: run.run_id.clone(),
        agent_id: agent.id.clone(),
        agent_title: agent.title.clone(),
        runtime: runtime.clone(),
    };
    fs::write(
        &runtime_detail_path,
        serde_json::to_string_pretty(&snapshot)?,
    )
    .with_context(|| format!("failed to write {}", runtime_detail_path.display()))?;

    Ok(runtime_prompt)
}

fn render_runtime_overlay(execution_root: &Path, runtime: &RuntimeExecutionRecord) -> String {
    let mut lines = vec![
        "## Runtime selection".to_string(),
        format!("- selected runtime: {}", runtime.selected_runtime),
        format!("- execution root: {}", execution_root.display()),
        format!("- selection reason: {}", runtime.selection_reason),
    ];
    if let Some(fallback) = runtime.fallback.as_ref() {
        lines.push(format!(
            "- fallback: {} -> {} ({})",
            fallback.requested_runtime, fallback.selected_runtime, fallback.reason
        ));
    } else {
        lines.push("- fallback: none".to_string());
    }
    lines.push(format!(
        "- projected skills: {}",
        if runtime.skill_projection.projected_skills.is_empty() {
            "none".to_string()
        } else {
            runtime.skill_projection.projected_skills.join(", ")
        }
    ));
    for skill in &runtime.skill_projection.projected_skills {
        lines.push(format!(
            "- skill path: {}",
            execution_root
                .join("skills")
                .join(skill)
                .join("SKILL.md")
                .display()
        ));
    }
    lines.join("\n")
}

fn render_claude_system_prompt(execution_root: &Path, runtime: &RuntimeExecutionRecord) -> String {
    let mut lines = vec![
        "Wave runtime harness for Claude.".to_string(),
        "Keep the authored assignment authoritative and preserve the required final markers exactly.".to_string(),
        format!("Selected runtime: {}.", runtime.selected_runtime),
        format!("Execution root: {}.", execution_root.display()),
    ];
    if !runtime.skill_projection.projected_skills.is_empty() {
        lines.push(format!(
            "Projected skills: {}.",
            runtime.skill_projection.projected_skills.join(", ")
        ));
        for skill in &runtime.skill_projection.projected_skills {
            lines.push(format!(
                "Skill file: {}",
                execution_root
                    .join("skills")
                    .join(skill)
                    .join("SKILL.md")
                    .display()
            ));
        }
    }
    lines.join("\n")
}

fn resolved_claude_settings_path(
    execution_root: &Path,
    agent: &WaveAgent,
) -> Result<Option<PathBuf>> {
    let Some(path) = agent.executor.get("claude.settings") else {
        return Ok(None);
    };
    let resolved = resolve_runtime_file_path(execution_root, path);
    if !resolved.exists() {
        bail!("Claude settings file {} does not exist", resolved.display());
    }
    Ok(Some(resolved))
}

fn build_claude_settings_overlay(
    base_settings_path: Option<&Path>,
    agent: &WaveAgent,
) -> Result<Option<JsonValue>> {
    let settings_json = agent.executor.get("claude.settings_json");
    let hooks_json = agent.executor.get("claude.hooks_json");
    let allowed_http_hook_urls = agent.executor.get("claude.allowed_http_hook_urls");

    let has_inline_overlay =
        settings_json.is_some() || hooks_json.is_some() || allowed_http_hook_urls.is_some();
    if !has_inline_overlay {
        return Ok(None);
    }

    let mut overlay = if let Some(path) = base_settings_path {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str::<JsonValue>(&raw)
            .with_context(|| format!("failed to parse {}", path.display()))?
    } else {
        JsonValue::Object(Default::default())
    };

    if let Some(raw) = settings_json {
        let value = serde_json::from_str::<JsonValue>(raw)
            .with_context(|| "failed to parse claude.settings_json".to_string())?;
        merge_json_value(&mut overlay, value);
    }
    if let Some(raw) = hooks_json {
        let value = serde_json::from_str::<JsonValue>(raw)
            .with_context(|| "failed to parse claude.hooks_json".to_string())?;
        overlay
            .as_object_mut()
            .context("Claude settings overlay must be a JSON object")?
            .insert("hooks".to_string(), value);
    }
    if let Some(raw) = allowed_http_hook_urls {
        let urls = parse_list_value(raw)
            .into_iter()
            .map(JsonValue::String)
            .collect::<Vec<_>>();
        overlay
            .as_object_mut()
            .context("Claude settings overlay must be a JSON object")?
            .insert("allowedHttpHookUrls".to_string(), JsonValue::Array(urls));
    }

    Ok(Some(overlay))
}

fn resolve_runtime_file_path(execution_root: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        execution_root.join(candidate)
    }
}

fn merge_json_value(target: &mut JsonValue, overlay: JsonValue) {
    match (target, overlay) {
        (JsonValue::Object(target_map), JsonValue::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                match target_map.get_mut(&key) {
                    Some(existing) => merge_json_value(existing, value),
                    None => {
                        target_map.insert(key, value);
                    }
                }
            }
        }
        (target, overlay) => *target = overlay,
    }
}

fn ordered_agents(wave: &WaveDocument) -> Vec<&WaveAgent> {
    let mut agents = wave.agents.iter().collect::<Vec<_>>();
    agents.sort_by_key(|agent| match agent.id.as_str() {
        "E0" => (1_u8, agent.id.as_str()),
        "A6" => (2_u8, agent.id.as_str()),
        "A7" => (3_u8, agent.id.as_str()),
        "A8" => (4_u8, agent.id.as_str()),
        "A9" => (5_u8, agent.id.as_str()),
        "A0" => (6_u8, agent.id.as_str()),
        _ => (0_u8, agent.id.as_str()),
    });
    agents
}

fn render_agent_prompt(
    root: &Path,
    wave: &WaveDocument,
    agent: &WaveAgent,
    ordered_agents: &[&WaveAgent],
) -> String {
    let mut prompt = Vec::new();
    prompt.push(format!(
        "# {}",
        wave.heading_title
            .as_deref()
            .unwrap_or(&wave.metadata.title)
    ));
    prompt.push(String::new());
    if let Some(commit_message) = wave.commit_message.as_deref() {
        prompt.push(format!("Commit message: `{commit_message}`"));
        prompt.push(String::new());
    }
    prompt.push("## Wave context".to_string());
    prompt.push(format!("- wave id: {}", wave.metadata.id));
    prompt.push(format!("- slug: {}", wave.metadata.slug));
    prompt.push(format!("- mode: {}", wave.metadata.mode));
    prompt.push(format!(
        "- component promotions: {}",
        wave.component_promotions
            .iter()
            .map(|promotion| format!("{}={}", promotion.component, promotion.target))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    prompt.push(format!(
        "- deploy environments: {}",
        wave.deploy_environments
            .iter()
            .map(|environment| format!("{}={}", environment.name, environment.detail))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if let Some(context7) = wave.context7_defaults.as_ref() {
        prompt.push(format!("- wave Context7 bundle: {}", context7.bundle));
        if let Some(query) = context7.query.as_deref() {
            prompt.push(format!("- wave Context7 query: {query}"));
        }
    }
    prompt.push(String::new());
    prompt.push("## Current agent".to_string());
    prompt.push(format!("- id: {}", agent.id));
    prompt.push(format!("- title: {}", agent.title));
    if !agent.role_prompts.is_empty() {
        prompt.push(format!("- role prompts: {}", agent.role_prompts.join(", ")));
    }
    if let Some(context7) = agent.context7.as_ref() {
        prompt.push(format!("- agent Context7 bundle: {}", context7.bundle));
        if let Some(query) = context7.query.as_deref() {
            prompt.push(format!("- agent Context7 query: {query}"));
        }
    }
    if !agent.deliverables.is_empty() {
        prompt.push(format!("- deliverables: {}", agent.deliverables.join(", ")));
    }
    prompt.push(format!(
        "- expected final markers: {}",
        agent.expected_final_markers().join(", ")
    ));
    prompt.push(String::new());
    prompt.push("## Execution order".to_string());
    for (index, candidate) in ordered_agents.iter().enumerate() {
        prompt.push(format!(
            "{}. {}: {}",
            index + 1,
            candidate.id,
            candidate.title
        ));
    }
    prompt.push(String::new());
    prompt.push("## Local references".to_string());
    if !agent.role_prompts.is_empty() {
        for role_prompt in &agent.role_prompts {
            prompt.push(format!(
                "- role prompt path: {}",
                root.join(role_prompt).display()
            ));
        }
    }
    prompt.push(format!("- contract source root: {}", root.display()));
    prompt.push(String::new());
    prompt.push("## Assignment".to_string());
    prompt.push(agent.prompt.trim().to_string());
    prompt.push(String::new());
    prompt.push("## Output contract".to_string());
    prompt.push("- Work directly in the repository.".to_string());
    prompt.push("- Respect the owned paths named in the assignment.".to_string());
    prompt.push("- End with the required final markers as plain lines.".to_string());
    prompt.push(
        "- If a required marker cannot be emitted honestly, explain the blocker and stop."
            .to_string(),
    );
    prompt.push(String::new());
    prompt.join("\n")
}

fn resolved_codex_model(agent: &WaveAgent) -> Option<String> {
    env::var("WAVE_CODEX_MODEL_OVERRIDE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| agent.executor.get("model").cloned())
}

fn resolved_codex_config_entries(agent: &WaveAgent) -> Vec<String> {
    env::var("WAVE_CODEX_CONFIG_OVERRIDE")
        .ok()
        .or_else(|| agent.executor.get("codex.config").cloned())
        .map(|raw| parse_codex_config_entries(&raw))
        .unwrap_or_default()
}

fn parse_codex_config_entries(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn build_launch_preflight(
    wave: &WaveDocument,
    dry_run: bool,
    registry: &RuntimeAdapterRegistry,
) -> LaunchPreflightReport {
    let required_closure_agents = ["A0", "A8", "A9"];
    let runtime_details = wave
        .agents
        .iter()
        .map(|agent| runtime_preflight_detail(agent, dry_run, registry))
        .collect::<Vec<_>>();
    let checks = vec![
        LaunchPreflightCheck {
            name: "validation-contract",
            ok: !wave.metadata.validation.is_empty(),
            detail: format!("{} validation commands declared", wave.metadata.validation.len()),
        },
        LaunchPreflightCheck {
            name: "rollback-contract",
            ok: !wave.metadata.rollback.is_empty(),
            detail: format!("{} rollback entries declared", wave.metadata.rollback.len()),
        },
        LaunchPreflightCheck {
            name: "proof-contract",
            ok: !wave.metadata.proof.is_empty(),
            detail: format!("{} proof artifacts declared", wave.metadata.proof.len()),
        },
        LaunchPreflightCheck {
            name: "deploy-environments",
            ok: !wave.deploy_environments.is_empty(),
            detail: format!("{} deploy environments declared", wave.deploy_environments.len()),
        },
        LaunchPreflightCheck {
            name: "closure-agents",
            ok: required_closure_agents
                .iter()
                .all(|agent_id| wave.agents.iter().any(|agent| agent.id == *agent_id)),
            detail: format!("required closure agents: {}", required_closure_agents.join(", ")),
        },
        LaunchPreflightCheck {
            name: "implementation-exit-contracts",
            ok: wave.implementation_agents().all(|agent| {
                agent.exit_contract.is_some()
                    && !agent.deliverables.is_empty()
                    && !agent.file_ownership.is_empty()
                    && !agent.final_markers.is_empty()
                    && agent.context7.is_some()
            }),
            detail: "implementation agents must declare exit contract, deliverables, ownership, markers, and Context7"
                .to_string(),
        },
        LaunchPreflightCheck {
            name: "runtime-availability",
            ok: runtime_details.iter().all(|detail| detail.0),
            detail: if dry_run {
                "dry run skips live runtime enforcement".to_string()
            } else {
                runtime_details
                    .iter()
                    .map(|detail| detail.1.clone())
                    .collect::<Vec<_>>()
                    .join(" | ")
            },
        },
    ];
    let diagnostics = checks
        .iter()
        .map(|check| LaunchPreflightDiagnostic {
            contract: check.name,
            required: check.name != "runtime-availability" || !dry_run,
            ok: check.ok,
            detail: check.detail.clone(),
        })
        .collect::<Vec<_>>();
    let failed_contracts = diagnostics
        .iter()
        .filter(|diagnostic| !diagnostic.ok && diagnostic.required)
        .map(|diagnostic| diagnostic.contract.to_string())
        .collect::<Vec<_>>();
    let refusal = if failed_contracts.is_empty() {
        None
    } else {
        Some(LaunchRefusal {
            wave_id: wave.metadata.id,
            wave_slug: wave.metadata.slug.clone(),
            detail: format!(
                "launch refused for wave {} ({}): missing required contracts: {}",
                wave.metadata.id,
                wave.metadata.slug,
                failed_contracts.join(", ")
            ),
            failed_contracts,
        })
    };

    LaunchPreflightReport {
        wave_id: wave.metadata.id,
        wave_slug: wave.metadata.slug.clone(),
        dry_run,
        ok: diagnostics.iter().all(|diagnostic| diagnostic.ok),
        checks,
        diagnostics,
        refusal,
    }
}

fn runtime_preflight_detail(
    agent: &WaveAgent,
    dry_run: bool,
    registry: &RuntimeAdapterRegistry,
) -> (bool, String) {
    if dry_run {
        return (
            true,
            format!("agent {} runtime check skipped in dry run", agent.id),
        );
    }

    let policy = normalized_runtime_policy(agent);
    let details = policy
        .allowed_runtimes
        .iter()
        .map(|runtime| runtime_availability_for(registry, *runtime))
        .collect::<Vec<_>>();
    if let Some(selected) = details.iter().find(|detail| detail.available) {
        return (
            true,
            format!(
                "agent {} -> {} ({})",
                agent.id, selected.runtime, selected.detail
            ),
        );
    }

    (
        false,
        format!(
            "agent {} has no available runtime: {}",
            agent.id,
            details
                .iter()
                .map(|detail| format!("{}: {}", detail.runtime, detail.detail))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )
}

fn bootstrap_project_codex_home(root: &Path, config: &ProjectConfig) -> Result<PathBuf> {
    let project_codex_home = config.resolved_paths(root).authority.project_codex_home;
    fs::create_dir_all(&project_codex_home)
        .with_context(|| format!("failed to create {}", project_codex_home.display()))?;

    let global_codex_home = global_codex_home();
    for relative in ["auth.json", ".credentials.json", "config.toml"] {
        let source = global_codex_home.join(relative);
        let target = project_codex_home.join(relative);
        if source.exists() && !target.exists() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::copy(&source, &target).with_context(|| {
                format!(
                    "failed to seed project Codex home from {} to {}",
                    source.display(),
                    target.display()
                )
            })?;
        }
    }

    Ok(project_codex_home)
}

fn bootstrap_authority_roots(root: &Path, config: &ProjectConfig) -> Result<()> {
    config
        .resolved_paths(root)
        .authority
        .materialize_canonical_state_tree()
}

fn global_codex_home() -> PathBuf {
    if let Ok(codex_home) = env::var("CODEX_HOME") {
        return PathBuf::from(codex_home);
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(".codex");
    }
    PathBuf::from(".codex")
}

fn build_specs_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    config.resolved_paths(root).authority.state_build_specs_dir
}

fn state_runs_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    config.resolved_paths(root).authority.state_runs_dir
}

fn trace_runs_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    config.resolved_paths(root).authority.trace_runs_dir
}

fn state_worktrees_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    config.resolved_paths(root).authority.state_worktrees_dir
}

fn state_control_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    config.resolved_paths(root).authority.state_control_dir
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_output_with_env(root: &Path, args: &[&str], envs: &[(&str, &Path)]) -> Result<String> {
    let mut command = Command::new("git");
    command.current_dir(root).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_output_bytes_with_env(
    root: &Path,
    args: &[&str],
    envs: &[(&str, &Path)],
) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    command.current_dir(root).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output.stdout)
}

fn git_add_pathspecs_with_env(root: &Path, envs: &[(&str, &Path)], pathspecs: &[u8]) -> Result<()> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(["add", "--pathspec-from-file=-", "--pathspec-file-nul"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command
        .spawn()
        .with_context(|| "failed to run git add --pathspec-from-file".to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(pathspecs)
            .with_context(|| "failed to write git pathspec input".to_string())?;
    }
    let output = child
        .wait_with_output()
        .with_context(|| "failed to wait for git add --pathspec-from-file".to_string())?;
    if !output.status.success() {
        bail!(
            "git add --pathspec-from-file failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn snapshot_excluded_prefixes(config: &ProjectConfig) -> Vec<String> {
    dedup_string_values(vec![
        normalize_snapshot_rel_path(&config.authority.state_dir.display().to_string()),
        normalize_snapshot_rel_path(&config.authority.trace_dir.display().to_string()),
        normalize_snapshot_rel_path(&config.authority.project_codex_home.display().to_string()),
    ])
}

fn normalize_snapshot_rel_path(path: &str) -> String {
    path.trim_start_matches("./")
        .trim_end_matches('/')
        .to_string()
}

fn filter_snapshot_untracked_paths(paths: &[u8], excluded_prefixes: &[String]) -> Vec<u8> {
    let mut filtered = Vec::new();
    for path in paths
        .split(|byte| *byte == b'\0')
        .filter(|path| !path.is_empty())
    {
        let path = String::from_utf8_lossy(path);
        let normalized = normalize_snapshot_rel_path(path.as_ref());
        let excluded = excluded_prefixes.iter().any(|prefix| {
            normalized == *prefix
                || normalized
                    .strip_prefix(prefix.as_str())
                    .is_some_and(|suffix| suffix.starts_with('/'))
        });
        if !excluded {
            filtered.extend_from_slice(normalized.as_bytes());
            filtered.push(b'\0');
        }
    }
    filtered
}

fn run_git(root: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !status.success() {
        bail!("git {} failed with status {status}", args.join(" "));
    }
    Ok(())
}

fn run_git_with_env(root: &Path, args: &[&str], envs: &[(&str, &Path)]) -> Result<()> {
    let mut command = Command::new("git");
    command.current_dir(root).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let status = command
        .status()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !status.success() {
        bail!("git {} failed with status {status}", args.join(" "));
    }
    Ok(())
}

fn control_reruns_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    state_control_dir(root, config).join("reruns")
}

fn rerun_intent_path(root: &Path, config: &ProjectConfig, wave_id: u32) -> PathBuf {
    control_reruns_dir(root, config).join(format!("wave-{wave_id:02}.json"))
}

fn control_closure_overrides_dir(root: &Path, config: &ProjectConfig) -> PathBuf {
    state_control_dir(root, config).join("closure-overrides")
}

fn closure_override_path(root: &Path, config: &ProjectConfig, wave_id: u32) -> PathBuf {
    control_closure_overrides_dir(root, config).join(format!("wave-{wave_id:02}.json"))
}

fn write_rerun_intent(
    root: &Path,
    config: &ProjectConfig,
    record: &RerunIntentRecord,
) -> Result<()> {
    let path = rerun_intent_path(root, config, record.wave_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, serde_json::to_string_pretty(record)?)
        .with_context(|| format!("failed to write rerun intent {}", path.display()))?;
    Ok(())
}

fn write_closure_override(
    root: &Path,
    config: &ProjectConfig,
    record: &WaveClosureOverrideRecord,
) -> Result<()> {
    let path = closure_override_path(root, config, record.wave_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&path, serde_json::to_string_pretty(record)?)
        .with_context(|| format!("failed to write closure override {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use std::collections::BTreeMap;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use wave_config::AuthorityConfig;
    use wave_config::ExecutionMode;
    use wave_control_plane::build_planning_status_with_state;
    use wave_events::SchedulerEventKind;
    use wave_events::SchedulerEventLog;
    use wave_spec::CompletionLevel;
    use wave_spec::ComponentPromotion;
    use wave_spec::Context7Defaults;
    use wave_spec::DeployEnvironment;
    use wave_spec::DocImpact;
    use wave_spec::DurabilityLevel;
    use wave_spec::ExitContract;
    use wave_spec::ProofLevel;
    use wave_spec::WaveMetadata;

    static FAKE_RUNTIME_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn closure_agents_run_after_implementation_agents() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/00.md"),
            metadata: WaveMetadata {
                id: 0,
                slug: "wave".to_string(),
                title: "Wave".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof".to_string()],
            },
            heading_title: Some("Wave 0".to_string()),
            commit_message: Some("Feat: test".to_string()),
            component_promotions: vec![ComponentPromotion {
                component: "runtime-fixture".to_string(),
                target: "baseline-proved".to_string(),
            }],
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![
                test_agent("A0"),
                test_agent("A8"),
                test_agent("A9"),
                test_agent("A2"),
                test_agent("A1"),
            ],
        };

        let ordered = ordered_agents(&wave);
        assert_eq!(
            ordered
                .iter()
                .map(|agent| agent.id.as_str())
                .collect::<Vec<_>>(),
            vec!["A1", "A2", "A8", "A9", "A0"]
        );
    }

    #[test]
    fn persist_agent_result_envelope_uses_owned_closure_artifacts() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-owned-closure-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".wave/integration")).expect("create integration dir");
        fs::create_dir_all(root.join(".wave/codex")).expect("create codex dir");
        let bundle_dir = root.join(".wave/state/build/specs/wave-12-1");
        let agent_dir = bundle_dir.join("agents/A8");
        fs::create_dir_all(&agent_dir).expect("create agent dir");
        fs::write(
            root.join(".wave/integration/wave-12.md"),
            "# Integration\n\n[wave-integration] state=ready-for-doc-closure claims=2 conflicts=0 blockers=0 detail=owned summary is authoritative\n",
        )
        .expect("write integration summary");
        fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        fs::write(agent_dir.join("last-message.txt"), "summary only\n").expect("write message");
        fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        let run = WaveRunRecord {
            run_id: "wave-12-1".to_string(),
            wave_id: 12,
            slug: "result-envelope-proof-lifecycle".to_string(),
            title: "Result Envelope Proof Lifecycle".to_string(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: root.join(".wave/traces/runs/wave-12-1.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        };
        let mut declared_agent = test_agent("A8");
        declared_agent.file_ownership = vec![".wave/integration/wave-12.md".to_string()];
        declared_agent.final_markers = vec!["[wave-integration]".to_string()];
        let agent_record = AgentRunRecord {
            id: "A8".to_string(),
            title: "Integration".to_string(),
            status: WaveRunStatus::Succeeded,
            prompt_path: agent_dir.join("prompt.md"),
            last_message_path: agent_dir.join("last-message.txt"),
            events_path: agent_dir.join("events.jsonl"),
            stderr_path: agent_dir.join("stderr.txt"),
            result_envelope_path: None,
            runtime_detail_path: None,
            expected_markers: vec!["[wave-integration]".to_string()],
            observed_markers: Vec::new(),
            exit_code: Some(0),
            error: None,
            runtime: None,
        };

        let updated =
            persist_agent_result_envelope(&root, &config, &run, &declared_agent, &agent_record)
                .expect("persist envelope");
        let envelope_path = updated
            .result_envelope_path
            .clone()
            .expect("result envelope path");
        let envelope = ResultEnvelopeStore::under_repo(&root)
            .load_envelope(&envelope_path)
            .expect("load envelope");

        assert_eq!(
            envelope.closure_input.final_markers.observed,
            vec!["[wave-integration]".to_string()]
        );
        assert_eq!(
            envelope.closure.disposition,
            wave_domain::ClosureDisposition::Ready
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn persist_agent_result_envelope_writes_canonical_result_path() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-envelope-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        let bundle_dir = root.join(".wave/state/build/specs/wave-12-1");
        let agent_dir = bundle_dir.join("agents/A2");
        fs::create_dir_all(&agent_dir).expect("create agent dir");
        fs::create_dir_all(root.join(".wave/traces/runs")).expect("create trace dir");
        fs::create_dir_all(root.join(".wave/codex")).expect("create codex dir");
        fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        fs::write(
            agent_dir.join("last-message.txt"),
            "[wave-proof]\n[wave-doc-delta]\n[wave-component]\n",
        )
        .expect("write message");
        fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");

        let run = WaveRunRecord {
            run_id: "wave-12-1".to_string(),
            wave_id: 12,
            slug: "result-envelope-proof-lifecycle".to_string(),
            title: "Result Envelope Proof Lifecycle".to_string(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: root.join(".wave/traces/runs/wave-12-1.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        };
        let mut declared_agent = test_agent("A2");
        declared_agent.file_ownership = vec![
            "docs/reference/runtime-config/README.md".to_string(),
            "crates/wave-runtime/src/lib.rs".to_string(),
        ];
        declared_agent.final_markers = vec![
            "[wave-proof]".to_string(),
            "[wave-doc-delta]".to_string(),
            "[wave-component]".to_string(),
        ];
        let agent_record = AgentRunRecord {
            id: "A2".to_string(),
            title: "Implementation".to_string(),
            status: WaveRunStatus::Succeeded,
            prompt_path: agent_dir.join("prompt.md"),
            last_message_path: agent_dir.join("last-message.txt"),
            events_path: agent_dir.join("events.jsonl"),
            stderr_path: agent_dir.join("stderr.txt"),
            result_envelope_path: None,
            runtime_detail_path: None,
            expected_markers: declared_agent.final_markers.clone(),
            observed_markers: declared_agent.final_markers.clone(),
            exit_code: Some(0),
            error: None,
            runtime: None,
        };

        let updated =
            persist_agent_result_envelope(&root, &config, &run, &declared_agent, &agent_record)
                .expect("persist result envelope");
        let envelope_path = updated
            .result_envelope_path
            .as_ref()
            .expect("envelope path")
            .clone();
        let envelope =
            wave_trace::load_result_envelope(&envelope_path).expect("load result envelope");

        assert_eq!(
            envelope_path,
            root.join(".wave/state/results/wave-12/wave-12-1-a2/agent_result_envelope.json")
        );
        assert_eq!(
            envelope.source,
            wave_trace::ResultEnvelopeSource::Structured
        );
        assert_eq!(envelope.final_markers.missing, Vec::<String>::new());
        assert_eq!(
            envelope.doc_delta.status,
            wave_trace::ResultPayloadStatus::Recorded
        );
        assert_eq!(
            envelope.doc_delta.paths,
            vec![
                root.join("docs/reference/runtime-config/README.md")
                    .to_string_lossy()
                    .into_owned()
            ]
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn blocks_non_pass_cont_qa_verdicts() {
        let agent = test_agent("A0");
        let closure = wave_results::build_structured_closure_state(
            agent.id.as_str(),
            wave_domain::AttemptState::Succeeded,
            &wave_domain::FinalMarkerEnvelope::default(),
            None,
            Some(
                "[wave-gate] architecture=blocked integration=pass durability=pass live=pass docs=pass detail=test\nVerdict: BLOCKED\n",
            ),
        );

        assert_eq!(
            result_closure_contract_error(agent.id.as_str(), &closure),
            Some("cont-QA verdict is BLOCKED, not PASS".to_string())
        );
    }

    #[test]
    fn blocks_integration_that_is_not_ready_for_doc_closure() {
        let agent = test_agent("A8");
        let closure = wave_results::build_structured_closure_state(
            agent.id.as_str(),
            wave_domain::AttemptState::Succeeded,
            &wave_domain::FinalMarkerEnvelope::default(),
            None,
            Some(
                "[wave-integration] state=needs-more-work claims=0 conflicts=1 blockers=1 detail=test\n",
            ),
        );

        assert_eq!(
            result_closure_contract_error(agent.id.as_str(), &closure),
            Some("integration state is needs-more-work, not ready-for-doc-closure".to_string())
        );
    }

    #[test]
    fn blocks_doc_closure_deltas() {
        let agent = test_agent("A9");
        let closure = wave_results::build_structured_closure_state(
            agent.id.as_str(),
            wave_domain::AttemptState::Succeeded,
            &wave_domain::FinalMarkerEnvelope::default(),
            None,
            Some("[wave-doc-closure] state=delta paths=README.md detail=test\n"),
        );

        assert_eq!(
            result_closure_contract_error(agent.id.as_str(), &closure),
            Some("documentation closure state is delta, not closed or no-change".to_string())
        );
    }

    #[test]
    fn build_closure_state_records_structured_integration_verdict() {
        let agent = test_agent("A8");
        let final_markers = wave_domain::FinalMarkerEnvelope::from_contract(
            vec!["[wave-integration]".to_string()],
            vec!["[wave-integration]".to_string()],
        );

        let closure = wave_results::build_structured_closure_state(
            agent.id.as_str(),
            wave_domain::AttemptState::Succeeded,
            &final_markers,
            None,
            Some(
                "[wave-integration] state=ready-for-doc-closure claims=2 conflicts=0 blockers=0 detail=ok\n",
            ),
        );

        assert_eq!(closure.disposition, wave_domain::ClosureDisposition::Ready);
        match closure.verdict {
            wave_domain::ClosureVerdictPayload::Integration(verdict) => {
                assert_eq!(verdict.state.as_deref(), Some("ready-for-doc-closure"));
                assert_eq!(verdict.claims, Some(2));
                assert_eq!(verdict.conflicts, Some(0));
                assert_eq!(verdict.blockers, Some(0));
            }
            other => panic!("expected integration verdict, got {other:?}"),
        }
    }

    #[test]
    fn launch_preflight_refuses_missing_required_contracts_with_diagnostics() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/06.md"),
            metadata: WaveMetadata {
                id: 6,
                slug: "dark-factory-enforcement".to_string(),
                title: "Make dark-factory an enforced execution profile".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A2".to_string()],
                depends_on: Vec::new(),
                validation: Vec::new(),
                rollback: Vec::new(),
                proof: Vec::new(),
            },
            heading_title: Some("Wave 6".to_string()),
            commit_message: Some(
                "Feat: land dark-factory preflight and fail-closed policy".to_string(),
            ),
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![test_agent("A2")],
        };

        let report = build_launch_preflight(&wave, false, &RuntimeAdapterRegistry::new());

        assert!(!report.ok);
        assert!(report.refusal.is_some());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contract == "validation-contract" && !diagnostic.ok)
        );
        assert!(
            report
                .refusal
                .as_ref()
                .expect("refusal")
                .detail
                .contains("validation-contract")
        );
    }

    #[test]
    fn preflight_refusal_keeps_rerun_intent_and_skips_run_state() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-preflight-refusal-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        let mut wave = launchable_test_wave(0);
        wave.metadata.validation.clear();
        let waves = vec![wave];
        let status = build_planning_status_with_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
        );

        request_rerun(
            &root,
            &config,
            0,
            "retry after failed preflight",
            RerunScope::Full,
        )
        .expect("request rerun");
        let error = launch_wave(
            &root,
            &config,
            &waves,
            &status,
            LaunchOptions {
                wave_id: Some(0),
                dry_run: false,
            },
        )
        .expect_err("preflight should fail");
        let report = error
            .chain()
            .find_map(|cause| cause.downcast_ref::<LaunchPreflightError>())
            .expect("launch preflight error")
            .report();

        assert!(!report.ok);
        assert!(
            report
                .refusal
                .as_ref()
                .expect("refusal")
                .failed_contracts
                .iter()
                .any(|contract| contract == "validation-contract")
        );
        assert!(
            pending_rerun_wave_ids(&root, &config)
                .expect("pending reruns")
                .contains(&0)
        );
        assert!(
            load_latest_runs(&root, &config)
                .expect("latest runs")
                .is_empty()
        );
        assert!(!state_runs_dir(&root, &config).exists());
        assert!(!trace_runs_dir(&root, &config).exists());
        assert!(
            scheduler_event_log(&root, &config)
                .load_all()
                .expect("scheduler events")
                .is_empty()
        );
        let build_entries = fs::read_dir(build_specs_dir(&root, &config))
            .expect("build specs dir")
            .collect::<Result<Vec<_>, _>>()
            .expect("build entries");
        assert_eq!(build_entries.len(), 1);
        assert!(build_entries[0].path().join("preflight.json").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn orphaned_runs_fail_closed_when_launcher_process_is_gone() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-orphan-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");

        let mut record = WaveRunRecord {
            run_id: "wave-5-1".to_string(),
            wave_id: 5,
            slug: "tui-right-panel".to_string(),
            title: "TUI".to_string(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: root.join("bundle"),
            trace_path: root.join("trace.json"),
            codex_home: root.join("codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: Some(u32::MAX),
            launcher_started_at_ms: Some(0),
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: vec![AgentRunRecord {
                id: "A1".to_string(),
                title: "Impl".to_string(),
                status: WaveRunStatus::Running,
                prompt_path: root.join("bundle/agents/A1/prompt.md"),
                last_message_path: root.join("bundle/agents/A1/last-message.txt"),
                events_path: root.join("bundle/agents/A1/events.jsonl"),
                stderr_path: root.join("bundle/agents/A1/stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: None,
                error: None,
                runtime: None,
            }],
            error: None,
        };
        fs::create_dir_all(root.join("bundle/agents/A1")).expect("create agent dir");
        fs::write(root.join("bundle/agents/A1/prompt.md"), "# prompt\n").expect("write prompt");

        let changed = reconcile_orphaned_run_record(&mut record).expect("reconcile orphan");

        assert!(changed);
        assert_eq!(record.status, WaveRunStatus::Failed);
        assert!(record.completed_at_ms.is_some());
        assert_eq!(record.agents[0].status, WaveRunStatus::Failed);
        assert!(
            record.agents[0]
                .error
                .as_deref()
                .unwrap()
                .contains("launcher process")
        );
        assert!(record.agents[0].last_message_path.exists());
        assert!(record.agents[0].events_path.exists());
        assert!(record.agents[0].stderr_path.exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn live_launcher_pid_is_not_treated_as_orphaned() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-live-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");

        let mut record = WaveRunRecord {
            run_id: "wave-5-2".to_string(),
            wave_id: 5,
            slug: "tui-right-panel".to_string(),
            title: "TUI".to_string(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: root.join("bundle"),
            trace_path: root.join("trace.json"),
            codex_home: root.join("codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: Some(std::process::id()),
            launcher_started_at_ms: current_process_started_at_ms(),
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: vec![AgentRunRecord {
                id: "A1".to_string(),
                title: "Impl".to_string(),
                status: WaveRunStatus::Running,
                prompt_path: root.join("bundle/agents/A1/prompt.md"),
                last_message_path: root.join("bundle/agents/A1/last-message.txt"),
                events_path: root.join("bundle/agents/A1/events.jsonl"),
                stderr_path: root.join("bundle/agents/A1/stderr.txt"),
                result_envelope_path: None,
                runtime_detail_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: None,
                error: None,
                runtime: None,
            }],
            error: None,
        };

        let changed = reconcile_orphaned_run_record(&mut record).expect("reconcile running");

        assert!(!changed);
        assert_eq!(record.status, WaveRunStatus::Running);
        assert!(record.completed_at_ms.is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dry_run_launch_keeps_rerun_intent_and_skips_run_state() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-dry-run-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        let waves = vec![launchable_test_wave(0)];
        let status = build_planning_status_with_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
        );

        request_rerun(
            &root,
            &config,
            0,
            "repair projection parity",
            RerunScope::Full,
        )
        .expect("request rerun");
        let report = launch_wave(
            &root,
            &config,
            &waves,
            &status,
            LaunchOptions {
                wave_id: Some(0),
                dry_run: true,
            },
        )
        .expect("dry-run launch");

        assert_eq!(report.status, WaveRunStatus::DryRun);
        assert!(report.bundle_dir.is_dir());
        assert!(report.preflight_path.exists());
        assert!(!report.state_path.exists());
        assert!(!report.trace_path.exists());
        assert!(
            pending_rerun_wave_ids(&root, &config)
                .expect("pending reruns")
                .contains(&0)
        );
        assert!(
            load_latest_runs(&root, &config)
                .expect("latest runs")
                .is_empty()
        );
        assert!(
            scheduler_event_log(&root, &config)
                .load_all()
                .expect("scheduler events")
                .is_empty()
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_closure_override_rejects_active_runs() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-closure-override-active-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave = launchable_test_wave(15);
        let active_run = scheduler_test_run(&root, &wave, "wave-15-active", 1);
        write_run_record_fixture(&root, &config, &active_run);

        let error = apply_closure_override(
            &root,
            &config,
            15,
            "manual close for promotion conflict review",
            None,
            Vec::new(),
            None,
        )
        .expect_err("active run should block manual close");

        assert!(error.to_string().contains("has an active run"));
        assert!(
            load_closure_override(&root, &config, 15)
                .expect("load override")
                .is_none()
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_closure_override_persists_record_and_clears_rerun_request() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-closure-override-rerun-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave = launchable_test_wave(15);
        let mut failed_run = scheduler_test_run(&root, &wave, "wave-15-failed", 1);
        failed_run.status = WaveRunStatus::Failed;
        failed_run.completed_at_ms = Some(2);
        failed_run.error = Some("promotion blocked by conflicts".to_string());
        write_run_record_fixture(&root, &config, &failed_run);

        request_rerun(
            &root,
            &config,
            15,
            "retry closure after promotion conflict",
            RerunScope::ClosureOnly,
        )
        .expect("request rerun");

        let record = apply_closure_override(
            &root,
            &config,
            15,
            "manual close accepted for Wave 15 baseline",
            None,
            vec![
                "docs/implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime/README.md"
                    .to_string(),
            ],
            Some("operator accepted failed promotion after inspection".to_string()),
        )
        .expect("apply closure override");

        assert!(record.is_active());
        assert_eq!(record.wave_id, 15);
        assert_eq!(record.source_run_id, "wave-15-failed");
        assert_eq!(
            record.evidence_paths,
            vec![
                "docs/implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime/README.md"
                    .to_string()
            ]
        );

        let stored = load_closure_override(&root, &config, 15)
            .expect("load closure override")
            .expect("stored override");
        assert_eq!(stored, record);

        let rerun = list_rerun_intents(&root, &config)
            .expect("load reruns")
            .remove(&15)
            .expect("rerun intent");
        assert_eq!(rerun.scope, RerunScope::ClosureOnly);
        assert_eq!(rerun.status, RerunIntentStatus::Cleared);
        assert!(rerun.cleared_at_ms.is_some());
        assert!(
            pending_rerun_wave_ids(&root, &config)
                .expect("pending reruns")
                .is_empty()
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rerun_scope_selection_distinguishes_closure_and_promotion_agents() {
        let wave = WaveDocument {
            path: PathBuf::from("waves/15.md"),
            metadata: WaveMetadata {
                id: 15,
                slug: "wave-15".to_string(),
                title: "Wave 15".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["README.md".to_string()],
            },
            heading_title: Some("Wave 15".to_string()),
            commit_message: Some("Feat: wave 15".to_string()),
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![
                test_agent("A1"),
                closure_test_agent("A6"),
                closure_test_agent("A7"),
                closure_test_agent("A8"),
                closure_test_agent("A9"),
                closure_test_agent("A0"),
            ],
        };
        let ordered = ordered_agents(&wave);
        assert_eq!(
            ordered.iter().map(|agent| agent.id.as_str()).collect::<Vec<_>>(),
            vec!["A1", "A6", "A7", "A8", "A9", "A0"]
        );

        assert_eq!(
            planned_execution_indices(&ordered, None, RerunScope::ClosureOnly)
                .expect("closure-only indices"),
            vec![1, 2, 3, 4, 5]
        );
        assert_eq!(
            planned_execution_indices(&ordered, None, RerunScope::PromotionOnly)
                .expect("promotion-only indices"),
            vec![3, 4, 5]
        );
    }

    #[test]
    fn from_first_incomplete_rerun_resumes_at_first_non_succeeded_agent() {
        let wave = launchable_test_wave(15);
        let ordered = ordered_agents(&wave);
        let prior_run = WaveRunRecord {
            run_id: "wave-15-prior".to_string(),
            wave_id: 15,
            slug: "wave-15".to_string(),
            title: "Wave 15".to_string(),
            status: WaveRunStatus::Failed,
            dry_run: false,
            bundle_dir: PathBuf::from(".wave/state/build/specs/wave-15-prior"),
            trace_path: PathBuf::from(".wave/traces/runs/wave-15-prior.json"),
            codex_home: PathBuf::from(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(1),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(2),
            agents: vec![
                AgentRunRecord {
                    id: "A1".to_string(),
                    title: "Implementation".to_string(),
                    status: WaveRunStatus::Succeeded,
                    prompt_path: PathBuf::from("prompt-a1.md"),
                    last_message_path: PathBuf::from("last-message-a1.txt"),
                    events_path: PathBuf::from("events-a1.jsonl"),
                    stderr_path: PathBuf::from("stderr-a1.txt"),
                    result_envelope_path: None,
                    runtime_detail_path: None,
                    expected_markers: vec!["[wave-proof]".to_string()],
                    observed_markers: vec!["[wave-proof]".to_string()],
                    exit_code: Some(0),
                    error: None,
                    runtime: None,
                },
                AgentRunRecord {
                    id: "A8".to_string(),
                    title: "Closure A8".to_string(),
                    status: WaveRunStatus::Failed,
                    prompt_path: PathBuf::from("prompt-a8.md"),
                    last_message_path: PathBuf::from("last-message-a8.txt"),
                    events_path: PathBuf::from("events-a8.jsonl"),
                    stderr_path: PathBuf::from("stderr-a8.txt"),
                    result_envelope_path: None,
                    runtime_detail_path: None,
                    expected_markers: vec!["[wave-integration]".to_string()],
                    observed_markers: Vec::new(),
                    exit_code: Some(1),
                    error: Some("failed".to_string()),
                    runtime: None,
                },
                AgentRunRecord {
                    id: "A9".to_string(),
                    title: "Closure A9".to_string(),
                    status: WaveRunStatus::Planned,
                    prompt_path: PathBuf::from("prompt-a9.md"),
                    last_message_path: PathBuf::from("last-message-a9.txt"),
                    events_path: PathBuf::from("events-a9.jsonl"),
                    stderr_path: PathBuf::from("stderr-a9.txt"),
                    result_envelope_path: None,
                    runtime_detail_path: None,
                    expected_markers: vec!["[wave-doc-closure]".to_string()],
                    observed_markers: Vec::new(),
                    exit_code: None,
                    error: None,
                    runtime: None,
                },
                AgentRunRecord {
                    id: "A0".to_string(),
                    title: "Closure A0".to_string(),
                    status: WaveRunStatus::Planned,
                    prompt_path: PathBuf::from("prompt-a0.md"),
                    last_message_path: PathBuf::from("last-message-a0.txt"),
                    events_path: PathBuf::from("events-a0.jsonl"),
                    stderr_path: PathBuf::from("stderr-a0.txt"),
                    result_envelope_path: None,
                    runtime_detail_path: None,
                    expected_markers: vec!["[wave-gate]".to_string()],
                    observed_markers: Vec::new(),
                    exit_code: None,
                    error: None,
                    runtime: None,
                },
            ],
            error: Some("failed".to_string()),
        };

        assert_eq!(
            planned_execution_indices(&ordered, Some(&prior_run), RerunScope::FromFirstIncomplete)
                .expect("resume indices"),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn closure_artifact_placeholders_are_seeded_before_followup_agents_run() {
        let execution_root = std::env::temp_dir().join(format!(
            "wave-runtime-closure-placeholders-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&execution_root).expect("create execution root");

        let mut wave = WaveDocument {
            path: PathBuf::from("waves/15.md"),
            metadata: WaveMetadata {
                id: 15,
                slug: "wave-15".to_string(),
                title: "Wave 15".to_string(),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: Vec::new(),
                rollback: Vec::new(),
                proof: Vec::new(),
            },
            heading_title: Some("Wave 15".to_string()),
            commit_message: Some("Feat: wave 15".to_string()),
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![
                test_agent("A1"),
                closure_test_agent("A6"),
                closure_test_agent("A7"),
                closure_test_agent("A8"),
                closure_test_agent("A9"),
                closure_test_agent("A0"),
            ],
        };
        wave.agents[1].file_ownership = vec![".wave/design/wave-15.md".to_string()];
        wave.agents[2].file_ownership = vec![".wave/security/wave-15.md".to_string()];
        wave.agents[3].file_ownership = vec![".wave/integration/wave-15.md".to_string()];
        wave.agents[4].file_ownership = vec![".wave/docs/wave-15.md".to_string()];
        wave.agents[5].file_ownership = vec!["reports/wave-15-summary.json".to_string()];

        let preserved_path = execution_root.join(".wave/docs/wave-15.md");
        fs::create_dir_all(
            preserved_path
                .parent()
                .expect("preserved placeholder parent"),
        )
        .expect("create preserved parent");
        fs::write(&preserved_path, "keep me\n").expect("seed existing closure artifact");

        prepare_closure_artifact_placeholders(&execution_root, &wave)
            .expect("seed closure placeholders");

        assert!(execution_root.join(".wave/design/wave-15.md").exists());
        assert!(execution_root.join(".wave/security/wave-15.md").exists());
        assert!(execution_root.join(".wave/integration/wave-15.md").exists());
        assert_eq!(
            fs::read_to_string(&preserved_path).expect("read preserved placeholder"),
            "keep me\n"
        );
        assert_eq!(
            fs::read_to_string(execution_root.join("reports/wave-15-summary.json"))
                .expect("read json placeholder"),
            "{}\n"
        );

        let _ = fs::remove_dir_all(&execution_root);
    }

    #[test]
    fn create_workspace_snapshot_commit_excludes_ignored_runtime_state_paths() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-snapshot-ignore-state-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        seed_lint_context(&root);
        init_git_repo(&root);

        fs::create_dir_all(root.join(".wave/state/build/specs")).expect("create state dir");
        fs::create_dir_all(root.join(".wave/codex")).expect("create codex dir");
        fs::create_dir_all(root.join(".wave/traces")).expect("create trace dir");
        fs::write(
            root.join(".wave/state/build/specs/ignored.txt"),
            "ignored runtime state\n",
        )
        .expect("write ignored state");
        fs::write(root.join(".wave/codex/session.db"), "ignored codex state\n")
            .expect("write ignored codex state");
        fs::write(root.join(".wave/traces/run.json"), "{}\n").expect("write ignored trace");
        fs::write(root.join("LIVE-WAVE-15-RUN.md"), "real run candidate\n")
            .expect("write untracked file");

        let snapshot_ref =
            create_workspace_snapshot_commit(&root, &root, &config, "wave-15-live", "base")
                .expect("create snapshot commit");
        let tree = git_output(
            &root,
            &["ls-tree", "--name-only", "-r", snapshot_ref.as_str()],
        )
        .expect("list snapshot tree");

        assert!(tree.lines().any(|line| line == "LIVE-WAVE-15-RUN.md"));
        assert!(!tree.contains(".wave/state/"));
        assert!(!tree.contains(".wave/codex/"));
        assert!(!tree.contains(".wave/traces/"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn codex_runtime_availability_accepts_logged_in_message_from_stderr() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-codex-stderr-availability-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        let bin_dir = root.join(".wave/test-bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let script_path = bin_dir.join("codex");
        fs::write(
            &script_path,
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "--version" ]]; then
  echo "codex-test"
  exit 0
fi
if [[ "${1:-}" == "login" && "${2:-}" == "status" ]]; then
  echo "Logged in using ChatGPT" >&2
  exit 0
fi
exit 1
"#,
        )
        .expect("write fake codex probe");
        let mut permissions = fs::metadata(&script_path)
            .expect("fake codex probe metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("chmod fake codex probe");

        let availability = CodexRuntimeAdapter {
            binary: script_path.to_string_lossy().into_owned(),
        }
        .availability();
        assert!(availability.available);
        assert_eq!(availability.runtime, RuntimeId::Codex);
        assert_eq!(availability.detail, "available");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn runtime_check_times_out_blocking_probe() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-probe-timeout-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        let bin_dir = root.join(".wave/test-bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        let script_path = bin_dir.join("slow-probe");
        fs::write(
            &script_path,
            r#"#!/usr/bin/env bash
set -euo pipefail
sleep 1
echo '{"loggedIn":true}'
"#,
        )
        .expect("write slow probe");
        let mut permissions = fs::metadata(&script_path)
            .expect("slow probe metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("chmod slow probe");

        let (ok, detail) = runtime_check_with_timeout(
            script_path.to_string_lossy().as_ref(),
            &["auth", "status", "--json"],
            Duration::from_millis(50),
            |status, stdout, _| status && stdout.contains("\"loggedIn\":true"),
        );
        assert!(!ok);
        assert!(detail.contains("timed out after 50ms"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_runtime_plan_uses_execution_root_for_skill_projection() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-execution-root-skill-projection-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        let execution_root = root.join(".wave/state/worktrees/wave-15");
        let bundle_dir = root.join(".wave/state/build/specs/wave-15-runtime-plan");
        let agent_dir = bundle_dir.join("agents/A1");
        fs::create_dir_all(&agent_dir).expect("create agent dir");
        fs::create_dir_all(execution_root.join("skills")).expect("create execution skills");
        fs::create_dir_all(root.join("skills")).expect("create root skills");
        write_skill_bundle(&root, "repo-only", &["codex"]);
        write_skill_bundle(&execution_root, "worktree-only", &["codex"]);
        write_skill_bundle(&execution_root, "runtime-codex", &["codex"]);

        let mut wave = launchable_test_wave(15);
        wave.agents[0].skills = vec!["repo-only".to_string(), "worktree-only".to_string()];
        let agent = wave.agents[0].clone();
        let run = scheduler_test_run(&root, &wave, "wave-15-runtime-plan", 1);
        let base_record = AgentRunRecord {
            id: agent.id.clone(),
            title: agent.title.clone(),
            status: WaveRunStatus::Planned,
            prompt_path: agent_dir.join("prompt.md"),
            last_message_path: agent_dir.join("last-message.txt"),
            events_path: agent_dir.join("events.jsonl"),
            stderr_path: agent_dir.join("stderr.txt"),
            result_envelope_path: None,
            runtime_detail_path: None,
            expected_markers: agent
                .expected_final_markers()
                .iter()
                .map(|marker| marker.to_string())
                .collect(),
            observed_markers: Vec::new(),
            exit_code: None,
            error: None,
            runtime: None,
        };

        with_fake_codex(&root, "ok", || {
            let registry = RuntimeAdapterRegistry::new();
            let plan = resolve_runtime_plan(
                &root,
                &execution_root,
                &run,
                &agent,
                &base_record,
                "# base prompt",
                &registry,
            )?;

            assert_eq!(plan.runtime.selected_runtime, RuntimeId::Codex);
            assert_eq!(plan.launch.execution_root, execution_root);
            assert_eq!(
                plan.launch.projected_skill_dirs,
                vec![
                    execution_root.join("skills/worktree-only"),
                    execution_root.join("skills/runtime-codex"),
                ]
            );
            assert_eq!(
                plan.runtime.skill_projection.declared_skills,
                vec!["repo-only".to_string(), "worktree-only".to_string()]
            );
            assert_eq!(
                plan.runtime.skill_projection.projected_skills,
                vec!["worktree-only".to_string(), "runtime-codex".to_string()]
            );
            assert_eq!(
                plan.runtime.skill_projection.dropped_skills,
                vec!["repo-only".to_string()]
            );
            assert_eq!(
                plan.runtime.skill_projection.auto_attached_skills,
                vec!["runtime-codex".to_string()]
            );

            let overlay_path = PathBuf::from(
                plan.runtime
                    .execution_identity
                    .artifact_paths
                    .get("skill_overlay")
                    .expect("skill overlay artifact"),
            );
            let overlay = fs::read_to_string(&overlay_path).expect("read skill overlay");
            assert!(overlay.contains(&format!("- execution root: {}", execution_root.display())));
            assert!(
                overlay.contains(
                    &execution_root
                        .join("skills/worktree-only/SKILL.md")
                        .display()
                        .to_string()
                )
            );
            assert!(
                !overlay.contains(&root.join("skills/repo-only/SKILL.md").display().to_string())
            );

            Ok(())
        })
        .expect("resolve runtime plan");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_runtime_plan_separates_runtime_launch_spec_from_codex_options() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-codex-launch-boundary-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        let execution_root = root.join(".wave/state/worktrees/wave-15");
        let bundle_dir = root.join(".wave/state/build/specs/wave-15-runtime-boundary");
        let agent_dir = bundle_dir.join("agents/A1");
        fs::create_dir_all(&agent_dir).expect("create agent dir");
        fs::create_dir_all(execution_root.join("skills")).expect("create execution skills");
        write_skill_bundle(&execution_root, "runtime-codex", &["codex"]);

        let mut wave = launchable_test_wave(15);
        wave.agents[0].executor = BTreeMap::from([
            ("id".to_string(), "codex".to_string()),
            ("model".to_string(), "gpt-5.4-mini".to_string()),
            (
                "codex.config".to_string(),
                "model_reasoning_effort=low,model_verbosity=low".to_string(),
            ),
        ]);
        let agent = wave.agents[0].clone();
        let run = scheduler_test_run(&root, &wave, "wave-15-runtime-boundary", 1);
        let base_record = AgentRunRecord {
            id: agent.id.clone(),
            title: agent.title.clone(),
            status: WaveRunStatus::Planned,
            prompt_path: agent_dir.join("prompt.md"),
            last_message_path: agent_dir.join("last-message.txt"),
            events_path: agent_dir.join("events.jsonl"),
            stderr_path: agent_dir.join("stderr.txt"),
            result_envelope_path: None,
            runtime_detail_path: None,
            expected_markers: agent
                .expected_final_markers()
                .iter()
                .map(|marker| marker.to_string())
                .collect(),
            observed_markers: Vec::new(),
            exit_code: None,
            error: None,
            runtime: None,
        };

        with_fake_codex(&root, "ok", || {
            let registry = RuntimeAdapterRegistry::new();
            let plan = resolve_runtime_plan(
                &root,
                &execution_root,
                &run,
                &agent,
                &base_record,
                "# runtime boundary prompt",
                &registry,
            )?;

            assert_eq!(plan.runtime.execution_identity.runtime, RuntimeId::Codex);
            assert_eq!(
                plan.runtime.execution_identity.adapter,
                "wave-runtime/codex"
            );
            assert_eq!(plan.launch.agent_id, "A1");
            assert_eq!(plan.launch.execution_root, execution_root);
            assert!(plan.launch.prompt.starts_with("# runtime boundary prompt"));
            assert!(plan.launch.prompt.contains("## Runtime selection"));
            assert_eq!(
                plan.launch.projected_skill_dirs,
                vec![execution_root.join("skills/runtime-codex")]
            );

            match &plan.adapter_config {
                RuntimeAdapterConfig::Codex(config) => {
                    assert_eq!(config.model.as_deref(), Some("gpt-5.4-mini"));
                    assert_eq!(
                        config.config_entries,
                        vec![
                            "model_reasoning_effort=low".to_string(),
                            "model_verbosity=low".to_string(),
                        ]
                    );
                }
                RuntimeAdapterConfig::Claude(_) => panic!("expected codex adapter config"),
            }

            Ok(())
        })
        .expect("resolve codex runtime boundary");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_runtime_plan_records_fallback_metadata_when_requested_runtime_is_unavailable() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-fallback-plan-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        let execution_root = root.join(".wave/state/worktrees/wave-15");
        let bundle_dir = root.join(".wave/state/build/specs/wave-15-fallback");
        let agent_dir = bundle_dir.join("agents/A1");
        fs::create_dir_all(&agent_dir).expect("create agent dir");
        fs::create_dir_all(execution_root.join("skills")).expect("create execution skills");
        write_skill_bundle(&execution_root, "runtime-claude", &["claude"]);

        let mut wave = launchable_test_wave(15);
        wave.agents[0].executor = BTreeMap::from([
            ("id".to_string(), "codex".to_string()),
            ("fallbacks".to_string(), "claude".to_string()),
        ]);
        wave.agents[0].skills.clear();
        let agent = wave.agents[0].clone();
        let run = scheduler_test_run(&root, &wave, "wave-15-fallback", 1);
        let base_record = AgentRunRecord {
            id: agent.id.clone(),
            title: agent.title.clone(),
            status: WaveRunStatus::Planned,
            prompt_path: agent_dir.join("prompt.md"),
            last_message_path: agent_dir.join("last-message.txt"),
            events_path: agent_dir.join("events.jsonl"),
            stderr_path: agent_dir.join("stderr.txt"),
            result_envelope_path: None,
            runtime_detail_path: None,
            expected_markers: agent
                .expected_final_markers()
                .iter()
                .map(|marker| marker.to_string())
                .collect(),
            observed_markers: Vec::new(),
            exit_code: None,
            error: None,
            runtime: None,
        };

        with_fake_codex_and_claude(&root, "unavailable", "ok", || {
            let registry = RuntimeAdapterRegistry::new();
            let plan = resolve_runtime_plan(
                &root,
                &execution_root,
                &run,
                &agent,
                &base_record,
                "# base prompt",
                &registry,
            )?;

            assert_eq!(plan.runtime.selected_runtime, RuntimeId::Claude);
            let fallback = plan.runtime.fallback.expect("fallback record");
            assert_eq!(fallback.requested_runtime, RuntimeId::Codex);
            assert_eq!(fallback.selected_runtime, RuntimeId::Claude);
            assert!(fallback.reason.contains("reported unavailable"));
            assert!(plan.runtime.selection_reason.contains("after fallback"));
            assert_eq!(
                plan.runtime.policy.allowed_runtimes,
                vec![RuntimeId::Codex, RuntimeId::Claude]
            );
            assert_eq!(
                plan.runtime.execution_identity.provider,
                "anthropic-claude-code"
            );
            assert_eq!(
                plan.runtime.skill_projection.auto_attached_skills,
                vec!["runtime-claude".to_string()]
            );

            Ok(())
        })
        .expect("resolve fallback plan");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn launch_wave_persists_codex_runtime_identity_through_runtime_neutral_boundary() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-codex-boundary-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::create_dir_all(root.join("waves")).expect("create waves dir");
        seed_lint_context(&root);
        write_skill_bundle(&root, "runtime-codex", &["codex"]);
        fs::write(root.join("src/wave15.rs"), "fn wave15() {}\n").expect("write source");
        fs::write(root.join("waves/15.md"), "# Wave 15\n").expect("write wave file");
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let mut wave = parallel_launchable_test_wave(15, "src/wave15.rs");
        wave.agents[0].skills = vec!["wave-core".to_string()];
        let waves = vec![wave];
        let status = build_planning_status_with_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
        );

        with_fake_codex(&root, "parallel", || {
            launch_wave(
                &root,
                &config,
                &waves,
                &status,
                LaunchOptions {
                    wave_id: Some(15),
                    dry_run: false,
                },
            )
        })
        .expect("launch codex wave");

        let latest_runs = load_latest_runs(&root, &config).expect("latest runs");
        let run = latest_runs.get(&15).expect("wave 15 run");
        let worktree = run.worktree.as_ref().expect("worktree");
        let implementation = run
            .agents
            .iter()
            .find(|agent| agent.id == "A1")
            .expect("implementation agent");
        let runtime = implementation.runtime.as_ref().expect("runtime record");
        assert_eq!(runtime.selected_runtime, RuntimeId::Codex);
        assert_eq!(runtime.execution_identity.adapter, "wave-runtime/codex");
        assert_eq!(
            runtime.skill_projection.auto_attached_skills,
            vec!["runtime-codex".to_string()]
        );
        assert_eq!(read_agent_worktree_marker(run, "A1").trim(), worktree.path);

        let runtime_detail_path = implementation
            .runtime_detail_path
            .as_ref()
            .expect("runtime detail path");
        let snapshot = serde_json::from_str::<RuntimeDetailSnapshot>(
            &fs::read_to_string(runtime_detail_path).expect("read runtime detail"),
        )
        .expect("parse runtime detail");
        assert_eq!(snapshot.runtime.selected_runtime, RuntimeId::Codex);
        assert_eq!(
            snapshot
                .runtime
                .execution_identity
                .artifact_paths
                .get("runtime_detail"),
            Some(&runtime_detail_path.to_string_lossy().into_owned())
        );

        let overlay_path = PathBuf::from(
            snapshot
                .runtime
                .execution_identity
                .artifact_paths
                .get("skill_overlay")
                .expect("skill overlay path"),
        );
        let overlay = fs::read_to_string(overlay_path).expect("read codex overlay");
        assert!(overlay.contains(&format!("- execution root: {}", worktree.path)));
        assert!(overlay.contains(&format!("{}/skills/runtime-codex/SKILL.md", worktree.path)));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn launch_wave_persists_claude_runtime_identity_and_transport_artifacts() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-claude-boundary-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::create_dir_all(root.join("waves")).expect("create waves dir");
        fs::create_dir_all(root.join("config")).expect("create config dir");
        seed_lint_context(&root);
        write_skill_bundle(&root, "runtime-claude", &["claude"]);
        fs::write(root.join("src/wave16.rs"), "fn wave16() {}\n").expect("write source");
        fs::write(root.join("waves/16.md"), "# Wave 16\n").expect("write wave file");
        fs::write(
            root.join("config/claude-base.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "permissions": {"allow": ["Read"]},
                "hooks": {"Start": [{"command": "echo start"}]},
            }))
            .expect("serialize base settings"),
        )
        .expect("write base settings");
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let mut wave = parallel_launchable_test_wave(16, "src/wave16.rs");
        for agent in &mut wave.agents {
            agent.executor = BTreeMap::from([
                ("id".to_string(), "claude".to_string()),
                ("model".to_string(), "claude-sonnet-test".to_string()),
            ]);
            agent.skills = vec!["wave-core".to_string()];
        }
        wave.agents[0].executor.insert(
            "claude.settings".to_string(),
            "config/claude-base.json".to_string(),
        );
        wave.agents[0].executor.insert(
            "claude.settings_json".to_string(),
            r#"{"permissions":{"allow":["Read","Edit"]}}"#.to_string(),
        );
        wave.agents[0].executor.insert(
            "claude.allowed_http_hook_urls".to_string(),
            "https://example.com/hooks".to_string(),
        );
        let waves = vec![wave];
        let status = build_planning_status_with_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
        );

        with_fake_claude(&root, "ok", || {
            launch_wave(
                &root,
                &config,
                &waves,
                &status,
                LaunchOptions {
                    wave_id: Some(16),
                    dry_run: false,
                },
            )
        })
        .expect("launch claude wave");

        let latest_runs = load_latest_runs(&root, &config).expect("latest runs");
        let run = latest_runs.get(&16).expect("wave 16 run");
        let worktree = run.worktree.as_ref().expect("worktree");
        let implementation = run
            .agents
            .iter()
            .find(|agent| agent.id == "A1")
            .expect("implementation agent");
        let runtime = implementation.runtime.as_ref().expect("runtime record");
        assert_eq!(runtime.selected_runtime, RuntimeId::Claude);
        assert_eq!(runtime.execution_identity.adapter, "wave-runtime/claude");
        assert_eq!(
            runtime.skill_projection.auto_attached_skills,
            vec!["runtime-claude".to_string()]
        );

        let runtime_detail_path = implementation
            .runtime_detail_path
            .as_ref()
            .expect("runtime detail path");
        let snapshot = serde_json::from_str::<RuntimeDetailSnapshot>(
            &fs::read_to_string(runtime_detail_path).expect("read runtime detail"),
        )
        .expect("parse runtime detail");
        let system_prompt_path = PathBuf::from(
            snapshot
                .runtime
                .execution_identity
                .artifact_paths
                .get("system_prompt")
                .expect("system prompt path"),
        );
        let settings_path = PathBuf::from(
            snapshot
                .runtime
                .execution_identity
                .artifact_paths
                .get("settings")
                .expect("settings path"),
        );
        assert!(system_prompt_path.exists());
        assert!(settings_path.exists());

        let used_system_prompt = fs::read_to_string(
            implementation
                .last_message_path
                .parent()
                .expect("agent dir")
                .join("claude-system-prompt-used.txt"),
        )
        .expect("read used system prompt");
        assert!(used_system_prompt.contains(&format!("Execution root: {}.", worktree.path)));
        assert!(
            used_system_prompt
                .contains(&format!("{}/skills/runtime-claude/SKILL.md", worktree.path))
        );

        let used_settings_path = fs::read_to_string(
            implementation
                .last_message_path
                .parent()
                .expect("agent dir")
                .join("claude-settings-path-used.txt"),
        )
        .expect("read used settings path");
        assert_eq!(used_settings_path.trim(), settings_path.to_string_lossy());
        assert_eq!(
            fs::read_to_string(
                implementation
                    .last_message_path
                    .parent()
                    .expect("agent dir")
                    .join("worktree.txt"),
            )
            .expect("read claude worktree marker")
            .trim(),
            worktree.path
        );

        let settings = serde_json::from_str::<JsonValue>(
            &fs::read_to_string(settings_path).expect("read settings overlay"),
        )
        .expect("parse settings overlay");
        assert_eq!(
            settings.pointer("/permissions/allow"),
            Some(&serde_json::json!(["Read", "Edit"]))
        );
        assert_eq!(
            settings.get("allowedHttpHookUrls"),
            Some(&serde_json::json!(["https://example.com/hooks"]))
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn default_scheduler_budget_is_emitted_once() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-budget-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };

        ensure_default_scheduler_budget(&root, &config).expect("first budget");
        ensure_default_scheduler_budget(&root, &config).expect("second budget");
        let events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, SchedulerEventKind::SchedulerBudgetUpdated);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn concurrent_claimers_only_allow_one_live_claim() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-atomic-claim-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        seed_lint_context(&root);
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave = launchable_test_wave(4);
        let waves = vec![wave.clone()];
        let findings = wave_dark_factory::lint_project(&root, &waves);
        assert!(
            findings.is_empty(),
            "unexpected lint findings: {findings:?}"
        );
        let barrier = Arc::new(Barrier::new(2));
        let mut handles = Vec::new();

        for run_suffix in ["a", "b"] {
            let root = root.clone();
            let config = config.clone();
            let wave = wave.clone();
            let waves = waves.clone();
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                claim_wave_for_launch(
                    &root,
                    &config,
                    &waves,
                    &wave,
                    &format!("wave-04-run-{run_suffix}"),
                    now_epoch_ms().expect("timestamp"),
                )
                .map(|claim| claim.claim_id.as_str().to_string())
                .map_err(|error| error.to_string())
            }));
        }

        let results = handles
            .into_iter()
            .map(|handle| handle.join().expect("join claim thread"))
            .collect::<Vec<_>>();
        assert_eq!(
            results.iter().filter(|result| result.is_ok()).count(),
            1,
            "claim results: {results:?}"
        );
        assert_eq!(
            results.iter().filter(|result| result.is_err()).count(),
            1,
            "claim results: {results:?}"
        );
        assert!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .any(|error| error.contains("not claimable"))
        );

        let events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == SchedulerEventKind::WaveClaimAcquired)
                .count(),
            1,
            "exactly one claim acquisition event should exist"
        );
        assert!(
            load_latest_runs(&root, &config)
                .expect("latest runs")
                .is_empty()
        );
        assert!(!state_runs_dir(&root, &config).exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn third_wave_claim_is_budget_blocked_until_a_parallel_claim_releases() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-budget-block-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        seed_lint_context(&root);
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave_a = launchable_test_wave(5);
        let wave_b = launchable_test_wave(6);
        let wave_c = launchable_test_wave(7);
        let waves = vec![wave_a.clone(), wave_b.clone(), wave_c.clone()];

        let claim_a = claim_wave_for_launch(&root, &config, &waves, &wave_a, "wave-05-run", 1)
            .expect("claim a");
        let claim_b = claim_wave_for_launch(&root, &config, &waves, &wave_b, "wave-06-run", 2)
            .expect("claim b");
        let error = claim_wave_for_launch(&root, &config, &waves, &wave_c, "wave-07-run", 3)
            .expect_err("budget should block third claim");
        assert!(error.to_string().contains("budget"));

        release_wave_claim(&root, &config, &claim_a, "wave complete").expect("release claim a");
        let claim_c = claim_wave_for_launch(&root, &config, &waves, &wave_c, "wave-07-run", 4)
            .expect("claim c");
        assert_eq!(claim_b.wave_id, 6);
        assert_eq!(claim_c.wave_id, 7);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn wave_scoped_worktree_allocation_is_distinct_per_wave() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-worktree-allocation-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        seed_lint_context(&root);
        fs::write(root.join("src/wave5.rs"), "fn wave5() {}\n").expect("write wave5");
        fs::write(root.join("src/wave6.rs"), "fn wave6() {}\n").expect("write wave6");
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave_a = parallel_launchable_test_wave(5, "src/wave5.rs");
        let wave_b = parallel_launchable_test_wave(6, "src/wave6.rs");
        let worktree_a = allocate_wave_worktree(&root, &config, &wave_a, "wave-05-proof", 1)
            .expect("worktree a");
        let worktree_b = allocate_wave_worktree(&root, &config, &wave_b, "wave-06-proof", 2)
            .expect("worktree b");

        assert_ne!(worktree_a.path, worktree_b.path);
        assert_eq!(worktree_a.shared_scope, WaveWorktreeScope::Wave);
        assert_eq!(worktree_b.shared_scope, WaveWorktreeScope::Wave);
        assert!(Path::new(&worktree_a.path).is_dir());
        assert!(Path::new(&worktree_b.path).is_dir());

        let events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events");
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == SchedulerEventKind::WaveWorktreeUpdated)
                .count(),
            2
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn released_worktree_is_removed_from_git_and_filesystem() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-worktree-release-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        seed_lint_context(&root);
        fs::write(root.join("src/wave5.rs"), "fn wave5() {}\n").expect("write wave5");
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave = parallel_launchable_test_wave(5, "src/wave5.rs");
        let worktree =
            allocate_wave_worktree(&root, &config, &wave, "wave-05-proof", 1).expect("worktree");
        assert!(Path::new(&worktree.path).exists());
        assert!(git_worktree_registered(&root, Path::new(&worktree.path)).expect("worktree list"));

        let released =
            release_wave_worktree(&root, &config, &worktree, "wave-05-proof", "release proof")
                .expect("release worktree");
        assert_eq!(released.state, WaveWorktreeState::Released);
        assert!(!Path::new(&released.path).exists());
        assert!(
            !git_worktree_registered(&root, Path::new(&released.path)).expect("worktree removed")
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn promotion_conflict_is_explicit_before_closure() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-promotion-conflict-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create root");
        seed_lint_context(&root);
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");
        let wave = launchable_test_wave(12);
        let worktree =
            allocate_wave_worktree(&root, &config, &wave, "wave-12-proof", 1).expect("worktree");
        let initial = publish_promotion_record(
            &root,
            &config,
            initial_promotion_record(&root, &wave, &worktree).expect("initial promotion"),
            "wave-12-proof",
        )
        .expect("publish initial promotion");

        fs::write(root.join("README.md"), "# root changed\n").expect("change root readme");
        fs::write(
            Path::new(&worktree.path).join("README.md"),
            "# worktree changed\n",
        )
        .expect("change worktree readme");

        let evaluated =
            evaluate_wave_promotion(&root, &config, &worktree, &initial, "wave-12-proof")
                .expect("evaluate promotion");
        assert_eq!(evaluated.state, WavePromotionState::Conflicted);
        assert_eq!(evaluated.conflict_paths, vec!["README.md".to_string()]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn promotion_conflict_is_explicit_with_relative_worktree_path() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-promotion-conflict-relative-worktree-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create root");
        seed_lint_context(&root);
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");
        let wave = launchable_test_wave(12);
        let worktree =
            allocate_wave_worktree(&root, &config, &wave, "wave-12-proof", 1).expect("worktree");
        let relative_worktree_path = format!(
            "./{}",
            Path::new(worktree.path.as_str())
                .strip_prefix(&root)
                .expect("worktree under root")
                .display()
        );
        let worktree = WaveWorktreeRecord {
            path: relative_worktree_path,
            ..worktree
        };
        let initial = publish_promotion_record(
            &root,
            &config,
            initial_promotion_record(&root, &wave, &worktree).expect("initial promotion"),
            "wave-12-proof",
        )
        .expect("publish initial promotion");

        fs::write(root.join("README.md"), "# root changed\n").expect("change root readme");
        fs::write(
            root.join(Path::new(worktree.path.as_str()))
                .join("README.md"),
            "# worktree changed\n",
        )
        .expect("change worktree readme");

        let evaluated =
            evaluate_wave_promotion(&root, &config, &worktree, &initial, "wave-12-proof")
                .expect("evaluate promotion");
        assert_eq!(evaluated.state, WavePromotionState::Conflicted);
        assert_eq!(evaluated.conflict_paths, vec!["README.md".to_string()]);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parallel_admission_respects_reserved_closure_capacity_and_fairness() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-reserved-closure-admission-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::create_dir_all(root.join("waves")).expect("create waves dir");
        seed_lint_context(&root);
        for (wave_id, filename) in [
            (5, "wave5.rs"),
            (6, "wave6.rs"),
            (40, "wave40.rs"),
            (41, "wave41.rs"),
        ] {
            fs::write(
                root.join("src").join(filename),
                format!("fn wave{wave_id}() {{}}\n"),
            )
            .expect("write source file");
            fs::write(
                root.join("waves").join(format!("{wave_id:02}.md")),
                format!("# Wave {wave_id}\n"),
            )
            .expect("write wave file");
        }
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        append_scheduler_event(
            &root,
            &config,
            SchedulerEvent::new(
                "sched-budget-reserved-proof",
                SchedulerEventKind::SchedulerBudgetUpdated,
            )
            .with_created_at_ms(1)
            .with_correlation_id("reserved-capacity-budget")
            .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated {
                budget: SchedulerBudgetRecord {
                    budget_id: SchedulerBudgetId::new("budget-reserved-proof"),
                    budget: SchedulerBudget {
                        max_active_wave_claims: Some(4),
                        max_active_task_leases: Some(2),
                        reserved_closure_task_leases: Some(1),
                        preemption_enabled: true,
                    },
                    owner: runtime_scheduler_owner("reserved-capacity-budget"),
                    updated_at_ms: 1,
                    detail: Some("reserved closure capacity proof".to_string()),
                },
            }),
        )
        .expect("budget event");

        let waves = vec![
            parallel_launchable_test_wave(5, "src/wave5.rs"),
            parallel_launchable_test_wave(6, "src/wave6.rs"),
            parallel_launchable_test_wave(40, "src/wave40.rs"),
            parallel_launchable_test_wave(41, "src/wave41.rs"),
        ];
        let active_impl_wave = waves
            .iter()
            .find(|wave| wave.metadata.id == 40)
            .expect("wave 40")
            .clone();
        let waiting_closure_wave = waves
            .iter()
            .find(|wave| wave.metadata.id == 41)
            .expect("wave 41")
            .clone();
        let active_claim =
            claim_wave_for_launch(&root, &config, &waves, &active_impl_wave, "wave-40-run", 10)
                .expect("claim active impl wave");
        let active_run = scheduler_test_run(&root, &active_impl_wave, "wave-40-run", 10);
        grant_task_lease(
            &root,
            &config,
            &active_run,
            &active_impl_wave.agents[0],
            &active_claim,
            LeaseTiming::default(),
        )
        .expect("active implementation lease");
        let _waiting_claim = claim_wave_for_launch(
            &root,
            &config,
            &waves,
            &waiting_closure_wave,
            "wave-41-run",
            11,
        )
        .expect("claim waiting closure wave");
        publish_scheduling_record(
            &root,
            &config,
            WaveSchedulingRecord {
                wave_id: 41,
                phase: WaveExecutionPhase::Closure,
                priority: WaveSchedulerPriority::Closure,
                state: WaveSchedulingState::Protected,
                fairness_rank: 1,
                waiting_since_ms: Some(11),
                protected_closure_capacity: true,
                preemptible: false,
                last_decision: Some(
                    "closure lane protected while waiting for reserved capacity".to_string(),
                ),
                updated_at_ms: 11,
            },
            "wave-41-run",
        )
        .expect("protected closure scheduling");

        let status = refresh_planning_status(&root, &config, &waves).expect("refresh status");
        let batch =
            next_parallel_wave_batch(&root, &config, &waves, &status, 2).expect("parallel batch");
        assert!(
            batch.is_empty(),
            "reserved closure capacity should block new implementation admission"
        );

        let scheduling_events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events")
            .into_iter()
            .filter_map(|event| match event.payload {
                SchedulerEventPayload::WaveSchedulingUpdated { scheduling }
                    if scheduling.wave_id == 5 || scheduling.wave_id == 6 =>
                {
                    Some(scheduling)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(scheduling_events.iter().any(|record| {
            record.wave_id == 5
                && record.state == WaveSchedulingState::Waiting
                && record.fairness_rank == 1
        }));
        assert!(scheduling_events.iter().any(|record| {
            record.wave_id == 6
                && record.state == WaveSchedulingState::Waiting
                && record.fairness_rank == 2
        }));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn parallel_admission_prefers_oldest_waiting_claimable_wave() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-fairness-admission-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::create_dir_all(root.join("waves")).expect("create waves dir");
        seed_lint_context(&root);
        for (wave_id, filename) in [(5, "wave5.rs"), (6, "wave6.rs")] {
            fs::write(
                root.join("src").join(filename),
                format!("fn wave{wave_id}() {{}}\n"),
            )
            .expect("write source file");
            fs::write(
                root.join("waves").join(format!("{wave_id:02}.md")),
                format!("# Wave {wave_id}\n"),
            )
            .expect("write wave file");
        }
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let waves = vec![
            parallel_launchable_test_wave(5, "src/wave5.rs"),
            parallel_launchable_test_wave(6, "src/wave6.rs"),
        ];
        publish_scheduling_record(
            &root,
            &config,
            WaveSchedulingRecord {
                wave_id: 6,
                phase: WaveExecutionPhase::Implementation,
                priority: WaveSchedulerPriority::Implementation,
                state: WaveSchedulingState::Waiting,
                fairness_rank: 1,
                waiting_since_ms: Some(1),
                protected_closure_capacity: false,
                preemptible: true,
                last_decision: Some("older waiting wave retained its place".to_string()),
                updated_at_ms: 1,
            },
            "wave-06-waiting",
        )
        .expect("waiting scheduling");

        let status = refresh_planning_status(&root, &config, &waves).expect("refresh status");
        let batch =
            next_parallel_wave_batch(&root, &config, &waves, &status, 1).expect("parallel batch");
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].wave_id, 6);

        let scheduling_events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events")
            .into_iter()
            .filter_map(|event| match event.payload {
                SchedulerEventPayload::WaveSchedulingUpdated { scheduling }
                    if scheduling.wave_id == 5 =>
                {
                    Some(scheduling)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(scheduling_events.iter().any(|record| {
            record.wave_id == 5
                && record.state == WaveSchedulingState::Waiting
                && record.fairness_rank == 2
                && record.last_decision.as_deref()
                    == Some("waiting for fairness turn behind older claimable waves")
        }));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn closure_lease_preempts_running_implementation_when_capacity_is_saturated() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-closure-preemption-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::create_dir_all(root.join("waves")).expect("create waves dir");
        seed_lint_context(&root);
        for (wave_id, filename) in [(20, "wave20.rs"), (21, "wave21.rs"), (22, "wave22.rs")] {
            fs::write(
                root.join("src").join(filename),
                format!("fn wave{wave_id}() {{}}\n"),
            )
            .expect("write source file");
            fs::write(
                root.join("waves").join(format!("{wave_id:02}.md")),
                format!("# Wave {wave_id}\n"),
            )
            .expect("write wave file");
        }
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");
        append_scheduler_event(
            &root,
            &config,
            SchedulerEvent::new(
                "sched-budget-preemption-proof",
                SchedulerEventKind::SchedulerBudgetUpdated,
            )
            .with_created_at_ms(1)
            .with_correlation_id("preemption-budget")
            .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated {
                budget: SchedulerBudgetRecord {
                    budget_id: SchedulerBudgetId::new("budget-preemption-proof"),
                    budget: SchedulerBudget {
                        max_active_wave_claims: Some(3),
                        max_active_task_leases: Some(2),
                        reserved_closure_task_leases: Some(1),
                        preemption_enabled: true,
                    },
                    owner: runtime_scheduler_owner("preemption-budget"),
                    updated_at_ms: 1,
                    detail: Some("preemption proof budget".to_string()),
                },
            }),
        )
        .expect("budget event");

        let waves = vec![
            parallel_launchable_test_wave(20, "src/wave20.rs"),
            parallel_launchable_test_wave(21, "src/wave21.rs"),
            parallel_launchable_test_wave(22, "src/wave22.rs"),
        ];
        let wave_20 = waves
            .iter()
            .find(|wave| wave.metadata.id == 20)
            .expect("wave 20")
            .clone();
        let wave_21 = waves
            .iter()
            .find(|wave| wave.metadata.id == 21)
            .expect("wave 21")
            .clone();
        let wave_22 = waves
            .iter()
            .find(|wave| wave.metadata.id == 22)
            .expect("wave 22")
            .clone();

        let claim_20 = claim_wave_for_launch(&root, &config, &waves, &wave_20, "wave-20-run", 12)
            .expect("claim 20");
        let claim_21 = claim_wave_for_launch(&root, &config, &waves, &wave_21, "wave-21-run", 10)
            .expect("claim 21");
        let run_21 = scheduler_test_run(&root, &wave_21, "wave-21-run", 10);
        let _lease_21 = grant_task_lease(
            &root,
            &config,
            &run_21,
            &wave_21.agents[0],
            &claim_21,
            LeaseTiming {
                heartbeat_interval_ms: 250,
                ttl_ms: 5_000,
                poll_interval_ms: 10,
            },
        )
        .expect("lease 21");
        std::thread::sleep(Duration::from_millis(5));

        let claim_22 = claim_wave_for_launch(&root, &config, &waves, &wave_22, "wave-22-run", 11)
            .expect("claim 22");
        let run_22 = scheduler_test_run(&root, &wave_22, "wave-22-run", 11);
        let lease_22 = grant_task_lease(
            &root,
            &config,
            &run_22,
            &wave_22.agents[0],
            &claim_22,
            LeaseTiming {
                heartbeat_interval_ms: 250,
                ttl_ms: 5_000,
                poll_interval_ms: 10,
            },
        )
        .expect("lease 22");

        let wait_root = root.clone();
        let wait_config = config.clone();
        let wait_handle = std::thread::spawn(move || {
            let mut child = Command::new("sh")
                .arg("-c")
                .arg("sleep 1")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn helper");
            wait_for_agent_exit_with_lease(
                &wait_root,
                &wait_config,
                "A1",
                &mut child,
                &lease_22,
                LeaseTiming {
                    heartbeat_interval_ms: 250,
                    ttl_ms: 5_000,
                    poll_interval_ms: 10,
                },
            )
            .map(|_| ())
            .map_err(|error| error.to_string())
        });
        std::thread::sleep(Duration::from_millis(50));

        let run_20 = scheduler_test_run(&root, &wave_20, "wave-20-run", 12);
        let closure_lease = grant_task_lease(
            &root,
            &config,
            &run_20,
            &wave_20.agents[1],
            &claim_20,
            LeaseTiming::default(),
        )
        .expect("closure lease");
        assert!(task_id_is_closure(&closure_lease.task_id));

        let wait_result = wait_handle.join().expect("join wait thread");
        wait_result.expect_err("preempted implementation should fail closed");

        let scheduler_events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events");
        assert!(scheduler_events.iter().any(|event| {
            event.kind == SchedulerEventKind::TaskLeaseRevoked
                && matches!(
                    &event.payload,
                    SchedulerEventPayload::TaskLeaseUpdated { lease }
                        if lease.wave_id == 22 && lease.state == TaskLeaseState::Revoked
                )
        }));
        assert!(scheduler_events.iter().any(|event| {
            matches!(
                &event.payload,
                SchedulerEventPayload::WaveSchedulingUpdated { scheduling }
                    if scheduling.wave_id == 22
                        && scheduling.state == WaveSchedulingState::Preempted
            )
        }));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn autonomous_launch_runs_two_non_conflicting_waves_in_parallel_with_distinct_worktrees() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-parallel-autonomous-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::create_dir_all(root.join("waves")).expect("create waves dir");
        seed_lint_context(&root);
        fs::write(root.join("src/wave5.rs"), "fn wave5() {}\n").expect("write wave5");
        fs::write(root.join("src/wave6.rs"), "fn wave6() {}\n").expect("write wave6");
        fs::write(root.join("waves/05.md"), "# Wave 5\n").expect("write wave 05");
        fs::write(root.join("waves/06.md"), "# Wave 6\n").expect("write wave 06");
        init_git_repo(&root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let waves = vec![
            parallel_launchable_test_wave(5, "src/wave5.rs"),
            parallel_launchable_test_wave(6, "src/wave6.rs"),
        ];
        let status = build_planning_status_with_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
        );
        let lint_messages = wave_dark_factory::lint_project(&root, &waves)
            .into_iter()
            .map(|finding| (finding.wave_id, finding.rule, finding.message))
            .collect::<Vec<_>>();
        let refreshed = refresh_planning_status(&root, &config, &waves).expect("refresh status");
        assert!(
            refreshed.queue.claimable_wave_ids.len() >= 2,
            "refreshed status blocked waves: {:?}; lint={:?}",
            refreshed
                .waves
                .iter()
                .map(|wave| (wave.id, wave.blocked_by.clone()))
                .collect::<Vec<_>>(),
            lint_messages
        );
        let reports = with_fake_codex(&root, "parallel", || {
            autonomous_launch(
                &root,
                &config,
                &waves,
                status.clone(),
                AutonomousOptions {
                    limit: Some(2),
                    dry_run: false,
                },
            )
        })
        .expect("parallel autonomous launch");
        assert_eq!(reports.len(), 2);
        assert!(
            reports
                .iter()
                .all(|report| report.status == WaveRunStatus::Succeeded)
        );

        let latest_runs = load_latest_runs(&root, &config).expect("latest runs");
        let run_a = latest_runs.get(&5).expect("run a");
        let run_b = latest_runs.get(&6).expect("run b");
        let worktree_a = run_a.worktree.as_ref().expect("run a worktree");
        let worktree_b = run_b.worktree.as_ref().expect("run b worktree");
        assert_ne!(worktree_a.path, worktree_b.path);
        assert_eq!(
            run_a.promotion.as_ref().map(|promotion| promotion.state),
            Some(WavePromotionState::Ready)
        );
        assert_eq!(
            run_b.promotion.as_ref().map(|promotion| promotion.state),
            Some(WavePromotionState::Ready)
        );
        assert!(
            run_a
                .scheduling
                .as_ref()
                .map(|record| record.protected_closure_capacity)
                .unwrap_or(false)
        );
        assert!(
            run_b
                .scheduling
                .as_ref()
                .map(|record| record.protected_closure_capacity)
                .unwrap_or(false)
        );
        assert!(!Path::new(&worktree_a.path).exists());
        assert!(!Path::new(&worktree_b.path).exists());

        for (worktree, run) in [(worktree_a, run_a), (worktree_b, run_b)] {
            for agent in ["A1", "A8", "A9", "A0"] {
                let seen = read_agent_worktree_marker(run, agent);
                assert_eq!(seen.trim(), worktree.path);
            }
            assert_eq!(
                events_for_wave_worktree_allocations(&root, &config, run.wave_id),
                1,
                "each wave should allocate exactly one shared worktree"
            );
        }

        let timing_a = read_agent_timing_for_run(run_a, "A1");
        let timing_b = read_agent_timing_for_run(run_b, "A1");
        assert!(timing_a.0 < timing_b.1 && timing_b.0 < timing_a.1);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn heartbeat_renewal_updates_live_lease_state() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-heartbeat-renewal-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        seed_lint_context(&root);
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave = launchable_test_wave(7);
        let run = WaveRunRecord {
            run_id: "wave-07-run".to_string(),
            wave_id: 7,
            slug: wave.metadata.slug.clone(),
            title: wave.metadata.title.clone(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-07-run"),
            trace_path: root.join(".wave/traces/runs/wave-07-run.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(1),
            launcher_pid: Some(std::process::id()),
            launcher_started_at_ms: current_process_started_at_ms(),
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        };
        let timing = LeaseTiming {
            heartbeat_interval_ms: 25,
            ttl_ms: 200,
            poll_interval_ms: 10,
        };

        let claim = claim_wave_for_launch(&root, &config, &[wave.clone()], &wave, &run.run_id, 1)
            .expect("claim");
        let lease =
            grant_task_lease(&root, &config, &run, &wave.agents[0], &claim, timing).expect("lease");
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 0.12")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn helper");

        let (status, renewed_lease) = wait_for_agent_exit_with_lease(
            &root,
            &config,
            wave.agents[0].id.as_str(),
            &mut child,
            &lease,
            timing,
        )
        .expect("wait with heartbeat");
        assert!(status.success());
        assert!(
            renewed_lease.heartbeat_at_ms.expect("heartbeat")
                > lease.heartbeat_at_ms.expect("initial heartbeat")
        );
        close_task_lease(
            &root,
            &config,
            &renewed_lease,
            TaskLeaseState::Released,
            "agent completed",
        )
        .expect("release lease");

        let events = SchedulerEventLog::new(
            config
                .resolved_paths(&root)
                .authority
                .state_events_scheduler_dir,
        )
        .load_all()
        .expect("scheduler events");
        assert!(
            events
                .iter()
                .any(|event| event.kind == SchedulerEventKind::TaskLeaseRenewed)
        );
        assert!(
            events
                .iter()
                .any(|event| event.kind == SchedulerEventKind::TaskLeaseReleased)
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn overdue_live_lease_expires_and_fails_closed() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-heartbeat-expiry-test-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        seed_lint_context(&root);
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");

        let wave = launchable_test_wave(8);
        let run = WaveRunRecord {
            run_id: "wave-08-run".to_string(),
            wave_id: 8,
            slug: wave.metadata.slug.clone(),
            title: wave.metadata.title.clone(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-08-run"),
            trace_path: root.join(".wave/traces/runs/wave-08-run.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(1),
            launcher_pid: Some(std::process::id()),
            launcher_started_at_ms: current_process_started_at_ms(),
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        };
        let timing = LeaseTiming {
            heartbeat_interval_ms: 120,
            ttl_ms: 50,
            poll_interval_ms: 10,
        };

        let claim = claim_wave_for_launch(&root, &config, &[wave.clone()], &wave, &run.run_id, 1)
            .expect("claim");
        let lease =
            grant_task_lease(&root, &config, &run, &wave.agents[0], &claim, timing).expect("lease");
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn helper");

        let error = wait_for_agent_exit_with_lease(
            &root,
            &config,
            wave.agents[0].id.as_str(),
            &mut child,
            &lease,
            timing,
        )
        .expect_err("lease should expire");
        assert!(error.to_string().contains("lost its lease"));

        let events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events");
        assert!(
            events
                .iter()
                .any(|event| event.kind == SchedulerEventKind::TaskLeaseExpired)
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn orphan_cleanup_revokes_active_lease_and_releases_claim() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-scheduler-cleanup-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        seed_lint_context(&root);
        bootstrap_authority_roots(&root, &config).expect("bootstrap authority");
        ensure_default_scheduler_budget(&root, &config).expect("budget");

        let wave = launchable_test_wave(3);
        let run = WaveRunRecord {
            run_id: "wave-03-run".to_string(),
            wave_id: 3,
            slug: wave.metadata.slug.clone(),
            title: wave.metadata.title.clone(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs/wave-03-run"),
            trace_path: root.join(".wave/traces/runs/wave-03-run.json"),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(1),
            launcher_pid: Some(std::process::id()),
            launcher_started_at_ms: current_process_started_at_ms(),
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        };

        let claim = claim_wave_for_launch(&root, &config, &[wave.clone()], &wave, &run.run_id, 1)
            .expect("claim");
        let lease = grant_task_lease(
            &root,
            &config,
            &run,
            &wave.agents[0],
            &claim,
            LeaseTiming::default(),
        )
        .expect("lease");
        assert_eq!(lease.state, TaskLeaseState::Granted);

        cleanup_scheduler_ownership_for_run(&root, &config, &run, "repair").expect("cleanup");
        let events = scheduler_event_log(&root, &config)
            .load_all()
            .expect("scheduler events");

        assert!(
            events
                .iter()
                .any(|event| event.kind == SchedulerEventKind::TaskLeaseRevoked)
        );
        assert!(
            events
                .iter()
                .any(|event| event.kind == SchedulerEventKind::WaveClaimReleased)
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "writes the Wave 14 live-proof bundle and local trace seed"]
    fn generate_phase_2_parallel_wave_execution_live_proof_bundle() {
        #[derive(Debug, Serialize)]
        struct TimingWindow {
            start_ms: u128,
            end_ms: u128,
        }

        #[derive(Debug, Serialize)]
        struct ParallelWaveProof {
            wave_id: u32,
            run_id: String,
            worktree: WaveWorktreeRecord,
            promotion: WavePromotionRecord,
            scheduling: WaveSchedulingRecord,
            agent_worktree_markers: BTreeMap<String, String>,
            timing_window: TimingWindow,
        }

        #[derive(Debug, Serialize)]
        struct ParallelRuntimeProofBundle {
            generated_at_ms: u128,
            fixture_root: String,
            overlap_observed: bool,
            distinct_worktrees: bool,
            per_agent_worktrees_used: bool,
            waves: Vec<ParallelWaveProof>,
        }

        #[derive(Debug, Serialize)]
        struct ProjectionSnapshotBundle {
            planning: wave_control_plane::PlanningStatus,
            control_status: wave_control_plane::ControlStatusReadModel,
        }

        #[derive(Debug, Serialize)]
        struct PromotionConflictBundle {
            wave_id: u32,
            worktree: WaveWorktreeRecord,
            initial_promotion: WavePromotionRecord,
            evaluated_promotion: WavePromotionRecord,
        }

        #[derive(Debug, Serialize)]
        struct ReservedClosureCapacityBundle {
            generated_at_ms: u128,
            claimable_wave_ids: Vec<u32>,
            selected_wave_ids: Vec<u32>,
            closure_capacity_reserved: bool,
            waiting_wave_scheduling: Vec<WaveSchedulingRecord>,
        }

        #[derive(Debug, Serialize)]
        struct FairnessAdmissionBundle {
            generated_at_ms: u128,
            claimable_wave_ids: Vec<u32>,
            fairness_ordered_wave_ids: Vec<u32>,
            selected_wave_ids: Vec<u32>,
            waiting_wave_scheduling: Vec<WaveSchedulingRecord>,
        }

        #[derive(Debug, Serialize)]
        struct PreemptionProofBundle {
            generated_at_ms: u128,
            closure_wave_id: u32,
            preempted_wave_id: u32,
            revoked_lease_id: String,
            closure_lease_id: String,
            wait_outcome: PreemptionWaitOutcome,
            scheduling_events: Vec<WaveSchedulingRecord>,
        }

        #[derive(Debug, Serialize)]
        struct PreemptionWaitOutcome {
            kind: String,
            wave_id: u32,
            lease_id: String,
            detail: String,
        }

        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let proof_dir =
            workspace_root.join("docs/implementation/live-proofs/phase-2-parallel-wave-execution");
        fs::create_dir_all(&proof_dir).expect("create proof dir");

        let fixture_root =
            workspace_root.join(".wave/state/live-proofs/phase-2-parallel-wave-execution-fixture");
        if fixture_root.exists() {
            fs::remove_dir_all(&fixture_root).expect("clear prior fixture");
        }
        fs::create_dir_all(fixture_root.join("src")).expect("create fixture src dir");
        fs::create_dir_all(fixture_root.join("waves")).expect("create fixture waves dir");
        seed_lint_context(&fixture_root);
        fs::write(fixture_root.join("src/wave5.rs"), "fn wave5() {}\n").expect("write wave5");
        fs::write(fixture_root.join("src/wave6.rs"), "fn wave6() {}\n").expect("write wave6");
        fs::write(fixture_root.join("waves/05.md"), "# Wave 5\n").expect("write wave 05");
        fs::write(fixture_root.join("waves/06.md"), "# Wave 6\n").expect("write wave 06");
        init_git_repo(&fixture_root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&fixture_root, &config).expect("bootstrap fixture authority");

        let waves = vec![
            parallel_launchable_test_wave(5, "src/wave5.rs"),
            parallel_launchable_test_wave(6, "src/wave6.rs"),
        ];
        let planning = build_planning_status_with_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
        );
        with_fake_codex(&fixture_root, "parallel", || {
            autonomous_launch(
                &fixture_root,
                &config,
                &waves,
                planning,
                AutonomousOptions {
                    limit: Some(2),
                    dry_run: false,
                },
            )
        })
        .expect("run parallel proof fixture");

        let latest_runs = load_latest_runs(&fixture_root, &config).expect("fixture latest runs");
        let run_a = latest_runs.get(&5).expect("fixture run a").clone();
        let run_b = latest_runs.get(&6).expect("fixture run b").clone();
        let worktree_a = run_a.worktree.clone().expect("run a worktree");
        let worktree_b = run_b.worktree.clone().expect("run b worktree");
        let promotion_a = run_a.promotion.clone().expect("run a promotion");
        let promotion_b = run_b.promotion.clone().expect("run b promotion");
        let scheduling_a = run_a.scheduling.clone().expect("run a scheduling");
        let scheduling_b = run_b.scheduling.clone().expect("run b scheduling");
        let timing_a = read_agent_timing_for_run(&run_a, "A1");
        let timing_b = read_agent_timing_for_run(&run_b, "A1");

        let parallel_bundle = ParallelRuntimeProofBundle {
            generated_at_ms: now_epoch_ms().expect("proof timestamp"),
            fixture_root: fixture_root.display().to_string(),
            overlap_observed: timing_a.0 < timing_b.1 && timing_b.0 < timing_a.1,
            distinct_worktrees: worktree_a.path != worktree_b.path,
            per_agent_worktrees_used: false,
            waves: vec![
                ParallelWaveProof {
                    wave_id: run_a.wave_id,
                    run_id: run_a.run_id.clone(),
                    worktree: worktree_a.clone(),
                    promotion: promotion_a.clone(),
                    scheduling: scheduling_a.clone(),
                    agent_worktree_markers: ["A1", "A8", "A9", "A0"]
                        .into_iter()
                        .map(|agent| {
                            (
                                agent.to_string(),
                                read_agent_worktree_marker(&run_a, agent).trim().to_string(),
                            )
                        })
                        .collect(),
                    timing_window: TimingWindow {
                        start_ms: timing_a.0,
                        end_ms: timing_a.1,
                    },
                },
                ParallelWaveProof {
                    wave_id: run_b.wave_id,
                    run_id: run_b.run_id.clone(),
                    worktree: worktree_b.clone(),
                    promotion: promotion_b.clone(),
                    scheduling: scheduling_b.clone(),
                    agent_worktree_markers: ["A1", "A8", "A9", "A0"]
                        .into_iter()
                        .map(|agent| {
                            (
                                agent.to_string(),
                                read_agent_worktree_marker(&run_b, agent).trim().to_string(),
                            )
                        })
                        .collect(),
                    timing_window: TimingWindow {
                        start_ms: timing_b.0,
                        end_ms: timing_b.1,
                    },
                },
            ],
        };
        fs::write(
            proof_dir.join("parallel-runtime-proof.json"),
            serde_json::to_string_pretty(&parallel_bundle).expect("serialize parallel proof"),
        )
        .expect("write parallel proof");

        let findings = wave_dark_factory::lint_project(&fixture_root, &waves);
        let skill_catalog_issues = wave_dark_factory::validate_skill_catalog(&fixture_root);
        let spine = wave_control_plane::build_projection_spine_from_authority(
            &fixture_root,
            &config,
            &waves,
            &findings,
            &skill_catalog_issues,
            &latest_runs,
            &HashSet::new(),
            &HashSet::new(),
            true,
        )
        .expect("build proof projection spine");
        let projection_bundle = ProjectionSnapshotBundle {
            planning: spine.planning.status.clone(),
            control_status: wave_control_plane::build_control_status_read_model_from_spine(&spine),
        };
        fs::write(
            proof_dir.join("projection-snapshot.json"),
            serde_json::to_string_pretty(&projection_bundle)
                .expect("serialize projection snapshot"),
        )
        .expect("write projection snapshot");

        let scheduler_events = scheduler_event_log(&fixture_root, &config)
            .load_all()
            .expect("load fixture scheduler events");
        fs::write(
            proof_dir.join("scheduler-events.jsonl"),
            scheduler_events
                .iter()
                .map(|event| serde_json::to_string(event).expect("serialize event"))
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .expect("write scheduler events");

        let fairness_root =
            workspace_root.join(".wave/state/live-proofs/phase-2-parallel-wave-fairness");
        if fairness_root.exists() {
            fs::remove_dir_all(&fairness_root).expect("clear prior fairness fixture");
        }
        fs::create_dir_all(fairness_root.join("src")).expect("create fairness fixture src dir");
        fs::create_dir_all(fairness_root.join("waves")).expect("create fairness fixture waves dir");
        seed_lint_context(&fairness_root);
        for (wave_id, filename) in [(5, "wave5.rs"), (6, "wave6.rs")] {
            fs::write(
                fairness_root.join("src").join(filename),
                format!("fn wave{wave_id}() {{}}\n"),
            )
            .expect("write fairness fixture source");
            fs::write(
                fairness_root.join("waves").join(format!("{wave_id:02}.md")),
                format!("# Wave {wave_id}\n"),
            )
            .expect("write fairness fixture wave");
        }
        init_git_repo(&fairness_root);
        bootstrap_authority_roots(&fairness_root, &config).expect("bootstrap fairness fixture");
        let fairness_waves = vec![
            parallel_launchable_test_wave(5, "src/wave5.rs"),
            parallel_launchable_test_wave(6, "src/wave6.rs"),
        ];
        publish_scheduling_record(
            &fairness_root,
            &config,
            WaveSchedulingRecord {
                wave_id: 6,
                phase: WaveExecutionPhase::Implementation,
                priority: WaveSchedulerPriority::Implementation,
                state: WaveSchedulingState::Waiting,
                fairness_rank: 1,
                waiting_since_ms: Some(1),
                protected_closure_capacity: false,
                preemptible: true,
                last_decision: Some("older waiting wave retained its place".to_string()),
                updated_at_ms: 1,
            },
            "wave-06-waiting",
        )
        .expect("fairness waiting scheduling");
        let fairness_status = refresh_planning_status(&fairness_root, &config, &fairness_waves)
            .expect("fairness status");
        let fairness_order = fifo_ordered_claimable_implementation_waves(
            &fairness_status,
            now_epoch_ms().expect("fairness timestamp"),
        )
        .into_iter()
        .map(|candidate| candidate.selection.wave_id)
        .collect::<Vec<_>>();
        let fairness_batch = next_parallel_wave_batch(
            &fairness_root,
            &config,
            &fairness_waves,
            &fairness_status,
            1,
        )
        .expect("fairness batch");
        let fairness_waiting_scheduling = scheduler_event_log(&fairness_root, &config)
            .load_all()
            .expect("fairness scheduler events")
            .into_iter()
            .filter_map(|event| match event.payload {
                SchedulerEventPayload::WaveSchedulingUpdated { scheduling }
                    if scheduling.wave_id == 5 =>
                {
                    Some(scheduling)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        fs::write(
            proof_dir.join("fairness-admission-order.json"),
            serde_json::to_string_pretty(&FairnessAdmissionBundle {
                generated_at_ms: now_epoch_ms().expect("fairness proof timestamp"),
                claimable_wave_ids: fairness_status.queue.claimable_wave_ids.clone(),
                fairness_ordered_wave_ids: fairness_order,
                selected_wave_ids: fairness_batch.iter().map(|wave| wave.wave_id).collect(),
                waiting_wave_scheduling: fairness_waiting_scheduling,
            })
            .expect("serialize fairness proof"),
        )
        .expect("write fairness proof");

        let reserved_root =
            workspace_root.join(".wave/state/live-proofs/phase-2-parallel-wave-reserved-capacity");
        if reserved_root.exists() {
            fs::remove_dir_all(&reserved_root).expect("clear prior reserved-capacity fixture");
        }
        fs::create_dir_all(reserved_root.join("src")).expect("create reserved fixture src dir");
        fs::create_dir_all(reserved_root.join("waves")).expect("create reserved fixture waves dir");
        seed_lint_context(&reserved_root);
        for (wave_id, filename) in [
            (5, "wave5.rs"),
            (6, "wave6.rs"),
            (40, "wave40.rs"),
            (41, "wave41.rs"),
        ] {
            fs::write(
                reserved_root.join("src").join(filename),
                format!("fn wave{wave_id}() {{}}\n"),
            )
            .expect("write reserved fixture source");
            fs::write(
                reserved_root.join("waves").join(format!("{wave_id:02}.md")),
                format!("# Wave {wave_id}\n"),
            )
            .expect("write reserved fixture wave");
        }
        init_git_repo(&reserved_root);
        bootstrap_authority_roots(&reserved_root, &config).expect("bootstrap reserved fixture");
        append_scheduler_event(
            &reserved_root,
            &config,
            SchedulerEvent::new(
                "sched-budget-reserved-proof",
                SchedulerEventKind::SchedulerBudgetUpdated,
            )
            .with_created_at_ms(1)
            .with_correlation_id("reserved-capacity-budget")
            .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated {
                budget: SchedulerBudgetRecord {
                    budget_id: SchedulerBudgetId::new("budget-reserved-proof"),
                    budget: SchedulerBudget {
                        max_active_wave_claims: Some(4),
                        max_active_task_leases: Some(2),
                        reserved_closure_task_leases: Some(1),
                        preemption_enabled: true,
                    },
                    owner: runtime_scheduler_owner("reserved-capacity-budget"),
                    updated_at_ms: 1,
                    detail: Some("reserved closure capacity proof".to_string()),
                },
            }),
        )
        .expect("reserved proof budget event");
        let reserved_waves = vec![
            parallel_launchable_test_wave(5, "src/wave5.rs"),
            parallel_launchable_test_wave(6, "src/wave6.rs"),
            parallel_launchable_test_wave(40, "src/wave40.rs"),
            parallel_launchable_test_wave(41, "src/wave41.rs"),
        ];
        let reserved_active_wave = reserved_waves
            .iter()
            .find(|wave| wave.metadata.id == 40)
            .expect("reserved active wave")
            .clone();
        let reserved_closure_wave = reserved_waves
            .iter()
            .find(|wave| wave.metadata.id == 41)
            .expect("reserved closure wave")
            .clone();
        let reserved_claim = claim_wave_for_launch(
            &reserved_root,
            &config,
            &reserved_waves,
            &reserved_active_wave,
            "wave-40-run",
            10,
        )
        .expect("reserved active claim");
        let reserved_run =
            scheduler_test_run(&reserved_root, &reserved_active_wave, "wave-40-run", 10);
        grant_task_lease(
            &reserved_root,
            &config,
            &reserved_run,
            &reserved_active_wave.agents[0],
            &reserved_claim,
            LeaseTiming::default(),
        )
        .expect("reserved active lease");
        let _reserved_waiting_claim = claim_wave_for_launch(
            &reserved_root,
            &config,
            &reserved_waves,
            &reserved_closure_wave,
            "wave-41-run",
            11,
        )
        .expect("reserved waiting claim");
        publish_scheduling_record(
            &reserved_root,
            &config,
            WaveSchedulingRecord {
                wave_id: 41,
                phase: WaveExecutionPhase::Closure,
                priority: WaveSchedulerPriority::Closure,
                state: WaveSchedulingState::Protected,
                fairness_rank: 1,
                waiting_since_ms: Some(11),
                protected_closure_capacity: true,
                preemptible: false,
                last_decision: Some(
                    "closure lane protected while waiting for reserved capacity".to_string(),
                ),
                updated_at_ms: 11,
            },
            "wave-41-run",
        )
        .expect("reserved protected scheduling");
        let reserved_status = refresh_planning_status(&reserved_root, &config, &reserved_waves)
            .expect("reserved status");
        let reserved_batch = next_parallel_wave_batch(
            &reserved_root,
            &config,
            &reserved_waves,
            &reserved_status,
            2,
        )
        .expect("reserved batch");
        let reserved_waiting_scheduling = scheduler_event_log(&reserved_root, &config)
            .load_all()
            .expect("reserved scheduler events")
            .into_iter()
            .filter_map(|event| match event.payload {
                SchedulerEventPayload::WaveSchedulingUpdated { scheduling }
                    if scheduling.wave_id == 5 || scheduling.wave_id == 6 =>
                {
                    Some(scheduling)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        fs::write(
            proof_dir.join("reserved-closure-capacity.json"),
            serde_json::to_string_pretty(&ReservedClosureCapacityBundle {
                generated_at_ms: now_epoch_ms().expect("reserved proof timestamp"),
                claimable_wave_ids: reserved_status.queue.claimable_wave_ids.clone(),
                selected_wave_ids: reserved_batch.iter().map(|wave| wave.wave_id).collect(),
                closure_capacity_reserved: status_closure_capacity_reserved(&reserved_status),
                waiting_wave_scheduling: reserved_waiting_scheduling,
            })
            .expect("serialize reserved proof"),
        )
        .expect("write reserved proof");

        let preemption_root =
            workspace_root.join(".wave/state/live-proofs/phase-2-parallel-wave-preemption");
        if preemption_root.exists() {
            fs::remove_dir_all(&preemption_root).expect("clear prior preemption fixture");
        }
        fs::create_dir_all(preemption_root.join("src")).expect("create preemption fixture src dir");
        fs::create_dir_all(preemption_root.join("waves"))
            .expect("create preemption fixture waves dir");
        seed_lint_context(&preemption_root);
        for (wave_id, filename) in [(20, "wave20.rs"), (21, "wave21.rs"), (22, "wave22.rs")] {
            fs::write(
                preemption_root.join("src").join(filename),
                format!("fn wave{wave_id}() {{}}\n"),
            )
            .expect("write preemption fixture source");
            fs::write(
                preemption_root
                    .join("waves")
                    .join(format!("{wave_id:02}.md")),
                format!("# Wave {wave_id}\n"),
            )
            .expect("write preemption fixture wave");
        }
        init_git_repo(&preemption_root);
        bootstrap_authority_roots(&preemption_root, &config).expect("bootstrap preemption fixture");
        append_scheduler_event(
            &preemption_root,
            &config,
            SchedulerEvent::new(
                "sched-budget-preemption-proof",
                SchedulerEventKind::SchedulerBudgetUpdated,
            )
            .with_created_at_ms(1)
            .with_correlation_id("preemption-budget")
            .with_payload(SchedulerEventPayload::SchedulerBudgetUpdated {
                budget: SchedulerBudgetRecord {
                    budget_id: SchedulerBudgetId::new("budget-preemption-proof"),
                    budget: SchedulerBudget {
                        max_active_wave_claims: Some(3),
                        max_active_task_leases: Some(2),
                        reserved_closure_task_leases: Some(1),
                        preemption_enabled: true,
                    },
                    owner: runtime_scheduler_owner("preemption-budget"),
                    updated_at_ms: 1,
                    detail: Some("preemption proof budget".to_string()),
                },
            }),
        )
        .expect("preemption proof budget event");
        let preemption_waves = vec![
            parallel_launchable_test_wave(20, "src/wave20.rs"),
            parallel_launchable_test_wave(21, "src/wave21.rs"),
            parallel_launchable_test_wave(22, "src/wave22.rs"),
        ];
        let wave_20 = preemption_waves
            .iter()
            .find(|wave| wave.metadata.id == 20)
            .expect("preemption wave 20")
            .clone();
        let wave_21 = preemption_waves
            .iter()
            .find(|wave| wave.metadata.id == 21)
            .expect("preemption wave 21")
            .clone();
        let wave_22 = preemption_waves
            .iter()
            .find(|wave| wave.metadata.id == 22)
            .expect("preemption wave 22")
            .clone();
        let claim_20 = claim_wave_for_launch(
            &preemption_root,
            &config,
            &preemption_waves,
            &wave_20,
            "wave-20-run",
            12,
        )
        .expect("preemption claim 20");
        let claim_21 = claim_wave_for_launch(
            &preemption_root,
            &config,
            &preemption_waves,
            &wave_21,
            "wave-21-run",
            10,
        )
        .expect("preemption claim 21");
        let run_21 = scheduler_test_run(&preemption_root, &wave_21, "wave-21-run", 10);
        let _lease_21 = grant_task_lease(
            &preemption_root,
            &config,
            &run_21,
            &wave_21.agents[0],
            &claim_21,
            LeaseTiming {
                heartbeat_interval_ms: 250,
                ttl_ms: 5_000,
                poll_interval_ms: 10,
            },
        )
        .expect("preemption lease 21");
        std::thread::sleep(Duration::from_millis(5));
        let claim_22 = claim_wave_for_launch(
            &preemption_root,
            &config,
            &preemption_waves,
            &wave_22,
            "wave-22-run",
            11,
        )
        .expect("preemption claim 22");
        let run_22 = scheduler_test_run(&preemption_root, &wave_22, "wave-22-run", 11);
        let lease_22 = grant_task_lease(
            &preemption_root,
            &config,
            &run_22,
            &wave_22.agents[0],
            &claim_22,
            LeaseTiming {
                heartbeat_interval_ms: 250,
                ttl_ms: 5_000,
                poll_interval_ms: 10,
            },
        )
        .expect("preemption lease 22");
        let wait_root = preemption_root.clone();
        let wait_config = config.clone();
        let wait_handle = std::thread::spawn(move || {
            let mut child = Command::new("sh")
                .arg("-c")
                .arg("sleep 1")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn preemption helper");
            wait_for_agent_exit_with_lease(
                &wait_root,
                &wait_config,
                "A1",
                &mut child,
                &lease_22,
                LeaseTiming {
                    heartbeat_interval_ms: 250,
                    ttl_ms: 5_000,
                    poll_interval_ms: 10,
                },
            )
        });
        std::thread::sleep(Duration::from_millis(50));
        let run_20 = scheduler_test_run(&preemption_root, &wave_20, "wave-20-run", 12);
        let closure_lease = grant_task_lease(
            &preemption_root,
            &config,
            &run_20,
            &wave_20.agents[1],
            &claim_20,
            LeaseTiming::default(),
        )
        .expect("preemption closure lease");
        let wait_error = wait_handle
            .join()
            .expect("join preemption wait thread")
            .expect_err("preemption should fail closed");
        let revoked = lease_revoked_error(&wait_error)
            .expect("preemption wait should end with an explicit lease revocation");
        let preemption_scheduling = scheduler_event_log(&preemption_root, &config)
            .load_all()
            .expect("preemption scheduler events")
            .into_iter()
            .filter_map(|event| match event.payload {
                SchedulerEventPayload::WaveSchedulingUpdated { scheduling }
                    if scheduling.wave_id == 22 =>
                {
                    Some(scheduling)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        fs::write(
            proof_dir.join("preemption-proof.json"),
            serde_json::to_string_pretty(&PreemptionProofBundle {
                generated_at_ms: now_epoch_ms().expect("preemption proof timestamp"),
                closure_wave_id: 20,
                preempted_wave_id: 22,
                revoked_lease_id: "lease-wave-22-a1".to_string(),
                closure_lease_id: closure_lease.lease_id.as_str().to_string(),
                wait_outcome: PreemptionWaitOutcome {
                    kind: "lease_revoked".to_string(),
                    wave_id: revoked.wave_id,
                    lease_id: revoked.lease_id.clone(),
                    detail: revoked.detail.clone(),
                },
                scheduling_events: preemption_scheduling,
            })
            .expect("serialize preemption proof"),
        )
        .expect("write preemption proof");

        let conflict_root =
            workspace_root.join(".wave/state/live-proofs/phase-2-parallel-wave-execution-conflict");
        if conflict_root.exists() {
            fs::remove_dir_all(&conflict_root).expect("clear prior conflict fixture");
        }
        fs::create_dir_all(&conflict_root).expect("create conflict fixture");
        seed_lint_context(&conflict_root);
        init_git_repo(&conflict_root);
        bootstrap_authority_roots(&conflict_root, &config).expect("bootstrap conflict fixture");
        let conflict_wave = launchable_test_wave(12);
        let conflict_worktree =
            allocate_wave_worktree(&conflict_root, &config, &conflict_wave, "wave-12-proof", 1)
                .expect("conflict worktree");
        let initial_promotion = publish_promotion_record(
            &conflict_root,
            &config,
            initial_promotion_record(&conflict_root, &conflict_wave, &conflict_worktree)
                .expect("initial promotion"),
            "wave-12-proof",
        )
        .expect("publish initial conflict promotion");
        fs::write(conflict_root.join("README.md"), "# root changed\n").expect("change root readme");
        fs::write(
            Path::new(&conflict_worktree.path).join("README.md"),
            "# worktree changed\n",
        )
        .expect("change worktree readme");
        let evaluated_promotion = evaluate_wave_promotion(
            &conflict_root,
            &config,
            &conflict_worktree,
            &initial_promotion,
            "wave-12-proof",
        )
        .expect("evaluate conflict promotion");
        fs::write(
            proof_dir.join("promotion-conflict.json"),
            serde_json::to_string_pretty(&PromotionConflictBundle {
                wave_id: conflict_wave.metadata.id,
                worktree: conflict_worktree,
                initial_promotion,
                evaluated_promotion: evaluated_promotion.clone(),
            })
            .expect("serialize conflict proof"),
        )
        .expect("write conflict proof");

        let workspace_config =
            ProjectConfig::load_from_repo_root(&workspace_root).expect("load workspace config");
        bootstrap_authority_roots(&workspace_root, &workspace_config)
            .expect("bootstrap workspace authority");
        let trace_seed_run_id = "wave-14-live-proof".to_string();
        let state_path = state_runs_dir(&workspace_root, &workspace_config)
            .join(format!("{trace_seed_run_id}.json"));
        let trace_path = trace_runs_dir(&workspace_root, &workspace_config)
            .join(format!("{trace_seed_run_id}.json"));
        let mut trace_seed = run_a.clone();
        trace_seed.run_id = trace_seed_run_id;
        trace_seed.wave_id = 14;
        trace_seed.slug = "parallel-wave-execution-and-merge-discipline-live-proof".to_string();
        trace_seed.title = "Wave 14 live-proof fixture".to_string();
        trace_seed.status = WaveRunStatus::DryRun;
        trace_seed.dry_run = true;
        trace_seed.error = Some(
            "local Wave 14 live-proof trace seed; not an authored-wave completion record"
                .to_string(),
        );
        trace_seed.trace_path = trace_path.clone();
        trace_seed.created_at_ms = now_epoch_ms().expect("trace seed timestamp");
        trace_seed.started_at_ms = Some(trace_seed.created_at_ms);
        trace_seed.completed_at_ms = Some(trace_seed.created_at_ms + 1);
        let trace_seed_worktree_id = WaveWorktreeId::new("worktree-wave-14-live-proof".to_string());
        if let Some(worktree) = trace_seed.worktree.as_mut() {
            worktree.wave_id = 14;
            worktree.worktree_id = trace_seed_worktree_id.clone();
        }
        if let Some(promotion) = trace_seed.promotion.as_mut() {
            promotion.wave_id = 14;
            promotion.promotion_id =
                WavePromotionId::new("promotion-wave-14-live-proof".to_string());
            promotion.worktree_id = Some(trace_seed_worktree_id.clone());
        }
        if let Some(scheduling) = trace_seed.scheduling.as_mut() {
            scheduling.wave_id = 14;
        }
        write_run_record(&state_path, &trace_seed).expect("write wave 14 proof run");
        write_trace_bundle(&trace_path, &trace_seed).expect("write wave 14 proof trace");

        fs::write(
            proof_dir.join("trace-latest-wave-14.json"),
            serde_json::to_string_pretty(&dogfood_evidence_report(&trace_seed))
                .expect("serialize latest trace"),
        )
        .expect("write latest trace proof");
        fs::write(
            proof_dir.join("trace-replay-wave-14.json"),
            serde_json::to_string_pretty(&trace_inspection_report(&trace_seed).replay)
                .expect("serialize replay trace"),
        )
        .expect("write replay trace proof");
    }

    #[test]
    #[ignore = "writes the Wave 15 runtime-policy and multi-runtime proof bundle"]
    fn generate_phase_3_runtime_policy_and_multi_runtime_proof_bundle() {
        #[derive(Debug, Serialize)]
        struct RuntimeAdapterProof {
            proof_classification: String,
            run: WaveRunRecord,
            runtime_detail: RuntimeDetailSnapshot,
            used_worktree_path: Option<String>,
            used_system_prompt: Option<String>,
            used_settings_path: Option<String>,
        }

        #[derive(Debug, Serialize)]
        struct WorktreeSkillProjectionProof {
            repo_root: String,
            execution_root: String,
            selected_runtime: String,
            declared_skills: Vec<String>,
            projected_skills: Vec<String>,
            dropped_skills: Vec<String>,
            auto_attached_skills: Vec<String>,
            overlay_preview: String,
        }

        #[derive(Debug, Serialize)]
        struct RuntimePolicyProofBundle {
            generated_at_ms: u128,
            current_environment: RuntimeBoundaryStatus,
            codex_fixture: RuntimeAdapterProof,
            claude_fixture: RuntimeAdapterProof,
            fallback_fixture: RuntimeExecutionRecord,
            worktree_skill_projection: WorktreeSkillProjectionProof,
        }

        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root");
        let proof_dir = workspace_root
            .join("docs/implementation/live-proofs/phase-3-runtime-policy-and-multi-runtime");
        fs::create_dir_all(&proof_dir).expect("create proof dir");

        let fixture_root = workspace_root
            .join(".wave/state/live-proofs/phase-3-runtime-policy-and-multi-runtime-fixture");
        if fixture_root.exists() {
            fs::remove_dir_all(&fixture_root).expect("clear prior proof fixture");
        }
        fs::create_dir_all(fixture_root.join("src")).expect("create fixture src dir");
        fs::create_dir_all(fixture_root.join("waves")).expect("create fixture waves dir");
        fs::create_dir_all(fixture_root.join("config")).expect("create fixture config dir");
        seed_lint_context(&fixture_root);
        write_skill_bundle(&fixture_root, "runtime-codex", &["codex"]);
        write_skill_bundle(&fixture_root, "runtime-claude", &["claude"]);
        fs::write(fixture_root.join("src/wave15.rs"), "fn wave15() {}\n").expect("write wave15");
        fs::write(fixture_root.join("src/wave16.rs"), "fn wave16() {}\n").expect("write wave16");
        fs::write(fixture_root.join("waves/15.md"), "# Wave 15\n").expect("write wave 15");
        fs::write(fixture_root.join("waves/16.md"), "# Wave 16\n").expect("write wave 16");
        fs::write(
            fixture_root.join("config/claude-base.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "permissions": {"allow": ["Read"]},
            }))
            .expect("serialize proof base settings"),
        )
        .expect("write proof base settings");
        init_git_repo(&fixture_root);

        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };
        bootstrap_authority_roots(&fixture_root, &config).expect("bootstrap proof authority");

        let codex_wave = parallel_launchable_test_wave(15, "src/wave15.rs");
        let mut claude_wave = parallel_launchable_test_wave(16, "src/wave16.rs");
        for agent in &mut claude_wave.agents {
            agent.executor = BTreeMap::from([
                ("id".to_string(), "claude".to_string()),
                ("model".to_string(), "claude-sonnet-test".to_string()),
            ]);
            agent.skills = vec!["wave-core".to_string()];
        }
        claude_wave.agents[0].executor.insert(
            "claude.settings".to_string(),
            "config/claude-base.json".to_string(),
        );
        claude_wave.agents[0].executor.insert(
            "claude.settings_json".to_string(),
            r#"{"permissions":{"allow":["Read","Edit"]}}"#.to_string(),
        );
        claude_wave.agents[0].executor.insert(
            "claude.allowed_http_hook_urls".to_string(),
            "https://example.com/hooks".to_string(),
        );
        let waves = vec![codex_wave.clone(), claude_wave.clone()];
        let status = build_planning_status_with_state(
            &config,
            &waves,
            &[],
            &[],
            &HashMap::new(),
            &HashSet::new(),
            &HashSet::new(),
        );

        with_fake_codex_and_claude(&fixture_root, "parallel", "ok", || {
            launch_wave(
                &fixture_root,
                &config,
                &waves,
                &status,
                LaunchOptions {
                    wave_id: Some(15),
                    dry_run: false,
                },
            )?;
            launch_wave(
                &fixture_root,
                &config,
                &waves,
                &status,
                LaunchOptions {
                    wave_id: Some(16),
                    dry_run: false,
                },
            )?;
            Ok(())
        })
        .expect("launch runtime proof fixtures");

        let latest_runs = load_latest_runs(&fixture_root, &config).expect("load proof runs");
        let codex_run = latest_runs.get(&15).expect("codex proof run");
        let claude_run = latest_runs.get(&16).expect("claude proof run");
        let codex_agent = codex_run
            .agents
            .iter()
            .find(|agent| agent.id == "A1")
            .expect("codex agent");
        let claude_agent = claude_run
            .agents
            .iter()
            .find(|agent| agent.id == "A1")
            .expect("claude agent");

        let codex_runtime_detail = serde_json::from_str::<RuntimeDetailSnapshot>(
            &fs::read_to_string(
                codex_agent
                    .runtime_detail_path
                    .as_ref()
                    .expect("codex runtime detail path"),
            )
            .expect("read codex runtime detail"),
        )
        .expect("parse codex runtime detail");
        let claude_runtime_detail = serde_json::from_str::<RuntimeDetailSnapshot>(
            &fs::read_to_string(
                claude_agent
                    .runtime_detail_path
                    .as_ref()
                    .expect("claude runtime detail path"),
            )
            .expect("read claude runtime detail"),
        )
        .expect("parse claude runtime detail");

        let fallback_execution_root = fixture_root.join(".wave/state/worktrees/wave-15-fallback");
        fs::create_dir_all(fallback_execution_root.join("skills")).expect("create fallback skills");
        write_skill_bundle(&fallback_execution_root, "runtime-claude", &["claude"]);
        let mut fallback_wave = launchable_test_wave(17);
        fallback_wave.agents[0].executor = BTreeMap::from([
            ("id".to_string(), "codex".to_string()),
            ("fallbacks".to_string(), "claude".to_string()),
        ]);
        let fallback_agent = fallback_wave.agents[0].clone();
        let fallback_run = scheduler_test_run(&fixture_root, &fallback_wave, "wave-17-fallback", 3);
        let fallback_agent_dir =
            fixture_root.join(".wave/state/build/specs/wave-17-fallback/agents/A1");
        fs::create_dir_all(&fallback_agent_dir).expect("create fallback agent dir");
        let fallback_base_record = AgentRunRecord {
            id: fallback_agent.id.clone(),
            title: fallback_agent.title.clone(),
            status: WaveRunStatus::Planned,
            prompt_path: fallback_agent_dir.join("prompt.md"),
            last_message_path: fallback_agent_dir.join("last-message.txt"),
            events_path: fallback_agent_dir.join("events.jsonl"),
            stderr_path: fallback_agent_dir.join("stderr.txt"),
            result_envelope_path: None,
            runtime_detail_path: None,
            expected_markers: fallback_agent
                .expected_final_markers()
                .iter()
                .map(|marker| marker.to_string())
                .collect(),
            observed_markers: Vec::new(),
            exit_code: None,
            error: None,
            runtime: None,
        };
        let fallback_runtime =
            with_fake_codex_and_claude(&fixture_root, "unavailable", "ok", || {
                let registry = RuntimeAdapterRegistry::new();
                Ok(resolve_runtime_plan(
                    &fixture_root,
                    &fallback_execution_root,
                    &fallback_run,
                    &fallback_agent,
                    &fallback_base_record,
                    "# fallback proof prompt",
                    &registry,
                )?
                .runtime)
            })
            .expect("resolve fallback proof");

        let worktree_repo_root = fixture_root.join(".wave/state/live-proofs/runtime-policy-root");
        let worktree_execution_root =
            fixture_root.join(".wave/state/live-proofs/runtime-policy-worktree");
        fs::create_dir_all(worktree_repo_root.join("skills")).expect("create worktree repo skills");
        fs::create_dir_all(worktree_execution_root.join("skills"))
            .expect("create worktree execution skills");
        write_skill_bundle(&worktree_repo_root, "repo-only", &["codex"]);
        write_skill_bundle(&worktree_execution_root, "worktree-only", &["codex"]);
        write_skill_bundle(&worktree_execution_root, "runtime-codex", &["codex"]);
        let mut projection_wave = launchable_test_wave(18);
        projection_wave.agents[0].skills =
            vec!["repo-only".to_string(), "worktree-only".to_string()];
        let projection_agent = projection_wave.agents[0].clone();
        let projection_run =
            scheduler_test_run(&fixture_root, &projection_wave, "wave-18-projection", 4);
        let projection_agent_dir =
            fixture_root.join(".wave/state/build/specs/wave-18-projection/agents/A1");
        fs::create_dir_all(&projection_agent_dir).expect("create projection agent dir");
        let projection_base_record = AgentRunRecord {
            id: projection_agent.id.clone(),
            title: projection_agent.title.clone(),
            status: WaveRunStatus::Planned,
            prompt_path: projection_agent_dir.join("prompt.md"),
            last_message_path: projection_agent_dir.join("last-message.txt"),
            events_path: projection_agent_dir.join("events.jsonl"),
            stderr_path: projection_agent_dir.join("stderr.txt"),
            result_envelope_path: None,
            runtime_detail_path: None,
            expected_markers: projection_agent
                .expected_final_markers()
                .iter()
                .map(|marker| marker.to_string())
                .collect(),
            observed_markers: Vec::new(),
            exit_code: None,
            error: None,
            runtime: None,
        };
        let projection_plan = with_fake_codex(&fixture_root, "ok", || {
            let registry = RuntimeAdapterRegistry::new();
            resolve_runtime_plan(
                &worktree_repo_root,
                &worktree_execution_root,
                &projection_run,
                &projection_agent,
                &projection_base_record,
                "# projection proof prompt",
                &registry,
            )
        })
        .expect("resolve worktree projection proof");
        let overlay_preview = fs::read_to_string(
            projection_plan
                .runtime
                .execution_identity
                .artifact_paths
                .get("skill_overlay")
                .expect("projection overlay path"),
        )
        .expect("read projection overlay");

        fs::write(
            proof_dir.join("runtime-boundary-proof.json"),
            serde_json::to_string_pretty(&RuntimePolicyProofBundle {
                generated_at_ms: now_epoch_ms().expect("proof timestamp"),
                current_environment: runtime_boundary_status(),
                codex_fixture: RuntimeAdapterProof {
                    proof_classification: "fixture-backed".to_string(),
                    run: codex_run.clone(),
                    runtime_detail: codex_runtime_detail,
                    used_worktree_path: Some(
                        read_agent_worktree_marker(codex_run, "A1")
                            .trim()
                            .to_string(),
                    ),
                    used_system_prompt: None,
                    used_settings_path: None,
                },
                claude_fixture: RuntimeAdapterProof {
                    proof_classification: "fixture-backed".to_string(),
                    run: claude_run.clone(),
                    runtime_detail: claude_runtime_detail,
                    used_worktree_path: Some(
                        fs::read_to_string(
                            claude_agent
                                .last_message_path
                                .parent()
                                .expect("claude agent dir")
                                .join("worktree.txt"),
                        )
                        .expect("read claude worktree path")
                        .trim()
                        .to_string(),
                    ),
                    used_system_prompt: Some(
                        fs::read_to_string(
                            claude_agent
                                .last_message_path
                                .parent()
                                .expect("claude agent dir")
                                .join("claude-system-prompt-used.txt"),
                        )
                        .expect("read claude used system prompt"),
                    ),
                    used_settings_path: Some(
                        fs::read_to_string(
                            claude_agent
                                .last_message_path
                                .parent()
                                .expect("claude agent dir")
                                .join("claude-settings-path-used.txt"),
                        )
                        .expect("read claude used settings path")
                        .trim()
                        .to_string(),
                    ),
                },
                fallback_fixture: fallback_runtime,
                worktree_skill_projection: WorktreeSkillProjectionProof {
                    repo_root: worktree_repo_root.display().to_string(),
                    execution_root: worktree_execution_root.display().to_string(),
                    selected_runtime: projection_plan.runtime.selected_runtime.to_string(),
                    declared_skills: projection_plan.runtime.skill_projection.declared_skills,
                    projected_skills: projection_plan.runtime.skill_projection.projected_skills,
                    dropped_skills: projection_plan.runtime.skill_projection.dropped_skills,
                    auto_attached_skills: projection_plan
                        .runtime
                        .skill_projection
                        .auto_attached_skills,
                    overlay_preview,
                },
            })
            .expect("serialize runtime proof bundle"),
        )
        .expect("write runtime proof bundle");
    }

    fn test_agent(id: &str) -> WaveAgent {
        match id {
            "A0" | "A6" | "A7" | "A8" | "A9" | "E0" => closure_test_agent(id),
            _ => WaveAgent {
                id: id.to_string(),
                title: format!("Implementation {id}"),
                role_prompts: Vec::new(),
                executor: BTreeMap::from([("profile".to_string(), "codex".to_string())]),
                context7: Some(Context7Defaults {
                    bundle: "rust-control-plane".to_string(),
                    query: Some(
                        "runtime fixture for scheduler claims leases and queue behavior"
                            .to_string(),
                    ),
                }),
                skills: vec!["wave-core".to_string()],
                components: vec!["runtime".to_string()],
                capabilities: vec!["testing".to_string()],
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Contract,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Unit,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec![format!("src/{id}.rs")],
                file_ownership: vec![format!("src/{id}.rs")],
                final_markers: vec![
                    "[wave-proof]".to_string(),
                    "[wave-doc-delta]".to_string(),
                    "[wave-component]".to_string(),
                ],
                prompt: format!(
                    "Primary goal:\n- implement fixture work\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output.\n\nFile ownership (only touch these paths):\n- src/{id}.rs"
                ),
            },
        }
    }

    fn closure_test_agent(id: &str) -> WaveAgent {
        let (role_prompt, owned_path, final_marker) = match id {
            "E0" => (
                "docs/agents/wave-cont-eval-role.md",
                ".wave/eval/test-wave.md",
                "[wave-eval]",
            ),
            "A6" => (
                "docs/agents/wave-design-role.md",
                ".wave/design/test-wave.md",
                "[wave-design]",
            ),
            "A7" => (
                "docs/agents/wave-security-role.md",
                ".wave/security/test-wave.md",
                "[wave-security]",
            ),
            "A0" => (
                "docs/agents/wave-cont-qa-role.md",
                ".wave/reviews/test-cont-qa.md",
                "[wave-gate]",
            ),
            "A8" => (
                "docs/agents/wave-integration-role.md",
                ".wave/integration/test-wave.md",
                "[wave-integration]",
            ),
            "A9" => (
                "docs/agents/wave-documentation-role.md",
                ".wave/docs/test-wave.md",
                "[wave-doc-closure]",
            ),
            other => panic!("unexpected closure agent {other}"),
        };
        WaveAgent {
            id: id.to_string(),
            title: format!("Closure {id}"),
            role_prompts: vec![role_prompt.to_string()],
            executor: BTreeMap::from([("profile".to_string(), "review-codex".to_string())]),
            context7: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some(
                    "closure fixture for integration documentation and qa review".to_string(),
                ),
            }),
            skills: vec!["wave-core".to_string()],
            components: Vec::new(),
            capabilities: Vec::new(),
            exit_contract: None,
            deliverables: Vec::new(),
            file_ownership: vec![owned_path.to_string()],
            final_markers: vec![final_marker.to_string()],
            prompt: format!(
                "Primary goal:\n- close the fixture wave\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final {final_marker} marker as a plain last line.\n\nFile ownership (only touch these paths):\n- {owned_path}"
            ),
        }
    }

    fn launchable_test_wave(id: u32) -> WaveDocument {
        let implementation_agent = WaveAgent {
            id: "A1".to_string(),
            title: "Implementation".to_string(),
            role_prompts: Vec::new(),
            executor: BTreeMap::from([("profile".to_string(), "codex".to_string())]),
            context7: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some(
                    "runtime fixture for scheduler claims leases and queue behavior".to_string(),
                ),
            }),
            skills: vec!["wave-core".to_string()],
            components: vec!["runtime".to_string()],
            capabilities: vec!["testing".to_string()],
            exit_contract: Some(ExitContract {
                completion: CompletionLevel::Contract,
                durability: DurabilityLevel::Durable,
                proof: ProofLevel::Unit,
                doc_impact: DocImpact::Owned,
            }),
            deliverables: vec!["README.md".to_string()],
            file_ownership: vec!["README.md".to_string()],
            final_markers: vec![
                "[wave-proof]".to_string(),
                "[wave-doc-delta]".to_string(),
                "[wave-component]".to_string(),
            ],
            prompt: "Primary goal:\n- land the runtime fixture\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output.\n\nFile ownership (only touch these paths):\n- README.md".to_string(),
        };

        WaveDocument {
            path: PathBuf::from(format!("waves/{id:02}.md")),
            metadata: WaveMetadata {
                id,
                slug: format!("wave-{id}"),
                title: format!("Wave {id}"),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["README.md".to_string()],
            },
            heading_title: Some(format!("Wave {id}")),
            commit_message: Some(format!("Feat: wave {id}")),
            component_promotions: vec![ComponentPromotion {
                component: "runtime-fixture".to_string(),
                target: "baseline-proved".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "Local validation".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some(
                    "runtime fixture for scheduler claims leases and queue behavior".to_string(),
                ),
            }),
            agents: vec![
                implementation_agent,
                closure_test_agent("A8"),
                closure_test_agent("A9"),
                closure_test_agent("A0"),
            ],
        }
    }

    fn seed_lint_context(root: &Path) {
        fs::write(root.join("README.md"), "# fixture\n").expect("write readme");

        write_skill_bundle(root, "wave-core", &[]);

        let context7_dir = root.join("docs/context7");
        fs::create_dir_all(&context7_dir).expect("create context7 dir");
        fs::write(
            context7_dir.join("bundles.json"),
            r#"{"version":1,"defaultBundle":"rust-control-plane","laneDefaults":{},"bundles":{"rust-control-plane":{"description":"Fixture bundle","libraries":[{"libraryName":"fixture-lib","queryHint":"scheduler claims leases queue projection fixture"}]}}}"#,
        )
        .expect("write context7 bundle catalog");

        let agent_dir = root.join("docs/agents");
        fs::create_dir_all(&agent_dir).expect("create role prompt dir");
        for path in [
            "wave-cont-qa-role.md",
            "wave-integration-role.md",
            "wave-documentation-role.md",
        ] {
            fs::write(agent_dir.join(path), "# role prompt\n").expect("write role prompt");
        }
    }

    fn init_git_repo(root: &Path) {
        run_git(root, &["init", "-b", "main"]).expect("git init");
        run_git(root, &["config", "user.email", "wave-tests@example.com"]).expect("git email");
        run_git(root, &["config", "user.name", "Wave Tests"]).expect("git name");
        run_git(root, &["add", "-A"]).expect("git add");
        run_git(root, &["commit", "-m", "initial fixture"]).expect("git commit");
    }

    fn write_skill_bundle(root: &Path, skill_id: &str, runtimes: &[&str]) {
        let skills_dir = root.join("skills").join(skill_id);
        fs::create_dir_all(&skills_dir).expect("create skills dir");
        fs::write(
            skills_dir.join("skill.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "id": skill_id,
                "title": format!("Fixture {skill_id}"),
                "description": format!("Fixture skill for {skill_id}"),
                "activation": {
                    "when": "Always",
                    "runtimes": runtimes,
                },
            }))
            .expect("serialize skill manifest"),
        )
        .expect("write skill manifest");
        fs::write(skills_dir.join("SKILL.md"), format!("# {skill_id}\n"))
            .expect("write skill body");
    }

    fn fake_runtime_env_lock() -> &'static Mutex<()> {
        FAKE_RUNTIME_ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    fn install_fake_codex(root: &Path) -> PathBuf {
        let bin_dir = root.join(".wave/test-bin");
        fs::create_dir_all(&bin_dir).expect("create fake codex bin dir");
        let script_path = bin_dir.join("codex");
        fs::write(
            &script_path,
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "--version" ]]; then
  echo "codex-test"
  exit 0
fi
if [[ "${1:-}" == "login" && "${2:-}" == "status" ]]; then
  if [[ "${WAVE_FAKE_CODEX_SCENARIO:-}" == "unavailable" ]]; then
    echo "Not logged in using test fixture"
    exit 0
  fi
  echo "Logged in using test fixture"
  exit 0
fi
workdir=""
output=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -C)
      workdir="$2"
      shift 2
      ;;
    -o)
      output="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
agent="$(basename "$(dirname "$output")")"
agent_dir="$(dirname "$output")"
wave_tag="$(basename "$workdir" | cut -d- -f1-2)"
mkdir -p "$(dirname "$output")"
mkdir -p "$workdir"
if [[ "${WAVE_FAKE_CODEX_SCENARIO:-}" == "parallel" && "$agent" == "A1" ]]; then
  printf 'start=%s\n' "$(date +%s%3N)" > "$workdir/.wave-${agent}-timing.txt"
  sleep 0.5
  printf 'end=%s\n' "$(date +%s%3N)" >> "$workdir/.wave-${agent}-timing.txt"
  cp "$workdir/.wave-${agent}-timing.txt" "$agent_dir/timing.txt"
fi
echo "$workdir" > "$workdir/.wave-${agent}-worktree.txt"
printf '%s\n' "$workdir" > "$agent_dir/worktree.txt"
case "$agent" in
  A8)
    mkdir -p "$workdir/.wave/integration"
    printf '%s\n' '[wave-integration] state=ready-for-doc-closure claims=1 conflicts=0 blockers=0 detail=ok' > "$workdir/.wave/integration/${wave_tag}.md"
    printf '%s\n' '[wave-integration] state=ready-for-doc-closure claims=1 conflicts=0 blockers=0 detail=ok' > "$output"
    ;;
  A9)
    mkdir -p "$workdir/.wave/docs"
    printf '%s\n' '[wave-doc-closure] state=closed paths=docs/implementation/live.md detail=ok' > "$workdir/.wave/docs/${wave_tag}.md"
    printf '%s\n' '[wave-doc-closure] state=closed paths=docs/implementation/live.md detail=ok' > "$output"
    ;;
  A0)
    mkdir -p "$workdir/.wave/reviews"
    cat > "$workdir/.wave/reviews/${wave_tag}.md" <<'EOF'
[wave-gate] architecture=pass integration=pass durability=pass live=pass docs=pass detail=ok
Verdict: PASS
EOF
    cat > "$output" <<'EOF'
[wave-gate] architecture=pass integration=pass durability=pass live=pass docs=pass detail=ok
Verdict: PASS
EOF
    ;;
  *)
    printf 'touched by %s\n' "$agent" >> "$workdir/README.md"
    cat > "$output" <<'EOF'
[wave-proof]
[wave-doc-delta]
[wave-component]
EOF
    ;;
esac
printf '{"event":"ok","agent":"%s"}\n' "$agent"
"#,
        )
        .expect("write fake codex script");
        let mut permissions = fs::metadata(&script_path)
            .expect("fake codex metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("chmod fake codex");
        script_path
    }

    fn install_fake_claude(root: &Path) -> PathBuf {
        let bin_dir = root.join(".wave/test-bin");
        fs::create_dir_all(&bin_dir).expect("create fake claude bin dir");
        let script_path = bin_dir.join("claude");
        fs::write(
            &script_path,
            r#"#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "--version" ]]; then
  echo "claude-test"
  exit 0
fi
if [[ "${1:-}" == "auth" && "${2:-}" == "status" && "${3:-}" == "--json" ]]; then
  if [[ "${WAVE_FAKE_CLAUDE_SCENARIO:-}" == "unavailable" ]]; then
    echo '{"loggedIn":false}'
  else
    echo '{"loggedIn":true}'
  fi
  exit 0
fi
workdir="$PWD"
prompt="${@: -1}"
system_prompt_file=""
settings_file=""
agent_dir=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --append-system-prompt-file|--system-prompt-file)
      system_prompt_file="$2"
      shift 2
      ;;
    --settings)
      settings_file="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
agent="$(printf '%s\n' "$prompt" | awk '/^- id: / {print $3; exit}')"
agent="${agent:-A1}"
wave_tag="$(basename "$workdir" | cut -d- -f1-2)"
mkdir -p "$workdir"
if [[ -n "$system_prompt_file" ]]; then
  agent_dir="$(dirname "$system_prompt_file")"
  cp "$system_prompt_file" "$workdir/.wave-${agent}-claude-system-prompt.txt"
  cp "$system_prompt_file" "$agent_dir/claude-system-prompt-used.txt"
fi
if [[ -n "$settings_file" ]]; then
  if [[ -n "$agent_dir" ]]; then
    printf '%s\n' "$settings_file" > "$agent_dir/claude-settings-path-used.txt"
  fi
  printf '%s\n' "$settings_file" > "$workdir/.wave-${agent}-claude-settings-path.txt"
fi
if [[ -n "$agent_dir" ]]; then
  printf '%s\n' "$workdir" > "$agent_dir/worktree.txt"
fi
case "$agent" in
  A8)
    mkdir -p "$workdir/.wave/integration"
    printf '%s\n' '[wave-integration] state=ready-for-doc-closure claims=1 conflicts=0 blockers=0 detail=ok' > "$workdir/.wave/integration/${wave_tag}.md"
    printf '%s\n' '[wave-integration] state=ready-for-doc-closure claims=1 conflicts=0 blockers=0 detail=ok'
    ;;
  A9)
    mkdir -p "$workdir/.wave/docs"
    printf '%s\n' '[wave-doc-closure] state=closed paths=docs/implementation/live.md detail=ok' > "$workdir/.wave/docs/${wave_tag}.md"
    printf '%s\n' '[wave-doc-closure] state=closed paths=docs/implementation/live.md detail=ok'
    ;;
  A0)
    mkdir -p "$workdir/.wave/reviews"
    cat > "$workdir/.wave/reviews/${wave_tag}.md" <<'EOF'
[wave-gate] architecture=pass integration=pass durability=pass live=pass docs=pass detail=ok
Verdict: PASS
EOF
    cat <<'EOF'
[wave-gate] architecture=pass integration=pass durability=pass live=pass docs=pass detail=ok
Verdict: PASS
EOF
    ;;
  *)
    printf 'touched by %s\n' "$agent" >> "$workdir/README.md"
    cat <<'EOF'
[wave-proof]
[wave-doc-delta]
[wave-component]
EOF
    ;;
esac
"#,
        )
        .expect("write fake claude script");
        let mut permissions = fs::metadata(&script_path)
            .expect("fake claude metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("chmod fake claude");
        script_path
    }

    fn with_fake_runtime_binaries<T>(
        root: &Path,
        codex_scenario: Option<&str>,
        claude_scenario: Option<&str>,
        f: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        let _guard = fake_runtime_env_lock()
            .lock()
            .expect("fake runtime env lock");
        let previous_binary = env::var("WAVE_CODEX_BIN").ok();
        let previous_scenario = env::var("WAVE_FAKE_CODEX_SCENARIO").ok();
        let previous_claude_binary = env::var("WAVE_CLAUDE_BIN").ok();
        let previous_claude_scenario = env::var("WAVE_FAKE_CLAUDE_SCENARIO").ok();
        if let Some(scenario) = codex_scenario {
            let binary = install_fake_codex(root);
            unsafe {
                env::set_var("WAVE_CODEX_BIN", &binary);
                env::set_var("WAVE_FAKE_CODEX_SCENARIO", scenario);
            }
        }
        if let Some(scenario) = claude_scenario {
            let binary = install_fake_claude(root);
            unsafe {
                env::set_var("WAVE_CLAUDE_BIN", &binary);
                env::set_var("WAVE_FAKE_CLAUDE_SCENARIO", scenario);
            }
        }
        let result = f();
        match previous_binary {
            Some(value) => unsafe { env::set_var("WAVE_CODEX_BIN", value) },
            None => unsafe { env::remove_var("WAVE_CODEX_BIN") },
        }
        match previous_scenario {
            Some(value) => unsafe { env::set_var("WAVE_FAKE_CODEX_SCENARIO", value) },
            None => unsafe { env::remove_var("WAVE_FAKE_CODEX_SCENARIO") },
        }
        match previous_claude_binary {
            Some(value) => unsafe { env::set_var("WAVE_CLAUDE_BIN", value) },
            None => unsafe { env::remove_var("WAVE_CLAUDE_BIN") },
        }
        match previous_claude_scenario {
            Some(value) => unsafe { env::set_var("WAVE_FAKE_CLAUDE_SCENARIO", value) },
            None => unsafe { env::remove_var("WAVE_FAKE_CLAUDE_SCENARIO") },
        }
        result
    }

    fn with_fake_codex<T>(root: &Path, scenario: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
        with_fake_runtime_binaries(root, Some(scenario), None, f)
    }

    fn with_fake_claude<T>(
        root: &Path,
        scenario: &str,
        f: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        with_fake_runtime_binaries(root, None, Some(scenario), f)
    }

    fn with_fake_codex_and_claude<T>(
        root: &Path,
        codex_scenario: &str,
        claude_scenario: &str,
        f: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        with_fake_runtime_binaries(root, Some(codex_scenario), Some(claude_scenario), f)
    }

    fn parallel_launchable_test_wave(id: u32, owned_path: &str) -> WaveDocument {
        let mut wave = launchable_test_wave(id);
        wave.agents[0].deliverables = vec![owned_path.to_string()];
        wave.agents[0].file_ownership = vec![owned_path.to_string()];
        wave.agents[0].prompt = format!(
            "Primary goal:\n- land the runtime fixture\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-proof], [wave-doc-delta], and [wave-component] markers as plain lines by themselves at the end of the output.\n\nFile ownership (only touch these paths):\n- {owned_path}"
        );
        wave.agents[1].file_ownership = vec![format!(".wave/integration/wave-{id:02}.md")];
        wave.agents[1].prompt = format!(
            "Primary goal:\n- integration close the fixture wave\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-integration] marker as a plain last line.\n\nFile ownership (only touch these paths):\n- .wave/integration/wave-{id:02}.md"
        );
        wave.agents[2].file_ownership = vec![format!(".wave/docs/wave-{id:02}.md")];
        wave.agents[2].prompt = format!(
            "Primary goal:\n- documentation close the fixture wave\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-doc-closure] marker as a plain last line.\n\nFile ownership (only touch these paths):\n- .wave/docs/wave-{id:02}.md"
        );
        wave.agents[3].file_ownership = vec![format!(".wave/reviews/wave-{id:02}.md")];
        wave.agents[3].prompt = format!(
            "Primary goal:\n- qa close the fixture wave\n\nRequired context before coding:\n- Read README.md.\n\nSpecific expectations:\n- Emit the final [wave-gate] marker as a plain last line before Verdict: PASS.\n\nFile ownership (only touch these paths):\n- .wave/reviews/wave-{id:02}.md"
        );
        wave.metadata.proof = vec![owned_path.to_string()];
        wave
    }

    fn events_for_wave_worktree_allocations(
        root: &Path,
        config: &ProjectConfig,
        wave_id: u32,
    ) -> usize {
        scheduler_event_log(root, config)
            .load_all()
            .expect("scheduler events")
            .into_iter()
            .filter(|event| match &event.payload {
                SchedulerEventPayload::WaveWorktreeUpdated { worktree } => {
                    worktree.wave_id == wave_id
                        && event.kind == SchedulerEventKind::WaveWorktreeUpdated
                        && worktree.state == WaveWorktreeState::Allocated
                }
                _ => false,
            })
            .count()
    }

    fn read_agent_timing(path: PathBuf) -> (u128, u128) {
        let raw = fs::read_to_string(path).expect("read timing");
        let mut start = None;
        let mut end = None;
        for line in raw.lines() {
            if let Some(value) = line.strip_prefix("start=") {
                start = value.parse::<u128>().ok();
            }
            if let Some(value) = line.strip_prefix("end=") {
                end = value.parse::<u128>().ok();
            }
        }
        (start.expect("start"), end.expect("end"))
    }

    fn read_agent_worktree_marker(run: &WaveRunRecord, agent_id: &str) -> String {
        let agent = run
            .agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .expect("agent record");
        let path = agent
            .last_message_path
            .parent()
            .expect("agent artifact dir")
            .join("worktree.txt");
        fs::read_to_string(path).expect("agent worktree marker")
    }

    fn read_agent_timing_for_run(run: &WaveRunRecord, agent_id: &str) -> (u128, u128) {
        let agent = run
            .agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .expect("agent record");
        read_agent_timing(
            agent
                .last_message_path
                .parent()
                .expect("agent artifact dir")
                .join("timing.txt"),
        )
    }

    fn scheduler_test_run(
        root: &Path,
        wave: &WaveDocument,
        run_id: &str,
        created_at_ms: u128,
    ) -> WaveRunRecord {
        WaveRunRecord {
            run_id: run_id.to_string(),
            wave_id: wave.metadata.id,
            slug: wave.metadata.slug.clone(),
            title: wave.metadata.title.clone(),
            status: WaveRunStatus::Running,
            dry_run: false,
            bundle_dir: root.join(".wave/state/build/specs").join(run_id),
            trace_path: root
                .join(".wave/traces/runs")
                .join(format!("{run_id}.json")),
            codex_home: root.join(".wave/codex"),
            created_at_ms,
            started_at_ms: Some(created_at_ms),
            launcher_pid: Some(std::process::id()),
            launcher_started_at_ms: current_process_started_at_ms(),
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: None,
            agents: Vec::new(),
            error: None,
        }
    }

    fn write_run_record_fixture(root: &Path, config: &ProjectConfig, run: &WaveRunRecord) {
        let path = state_runs_dir(root, config).join(format!("{}.json", run.run_id));
        fs::create_dir_all(path.parent().expect("run record parent")).expect("create run dir");
        fs::write(&path, serde_json::to_string_pretty(run).expect("serialize run"))
            .expect("write run record");
    }

    #[test]
    fn build_specs_are_rooted_in_project_state_dir() {
        let config = ProjectConfig {
            version: 1,
            project_name: "Codex Wave Mode".to_string(),
            default_lane: "main".to_string(),
            default_mode: ExecutionMode::DarkFactory,
            waves_dir: PathBuf::from("waves"),
            authority: AuthorityConfig {
                project_codex_home: PathBuf::from(".wave/codex"),
                state_dir: PathBuf::from(".wave/state"),
                state_build_specs_dir: PathBuf::from(".wave/state/build/specs"),
                state_runs_dir: PathBuf::from(".wave/state/runs"),
                state_control_dir: PathBuf::from(".wave/state/control"),
                trace_dir: PathBuf::from(".wave/traces"),
                trace_runs_dir: PathBuf::from(".wave/traces/runs"),
                ..AuthorityConfig::default()
            },
            codex_vendor_dir: PathBuf::from("third_party/codex-rs"),
            reference_wave_repo_dir: PathBuf::from("third_party/agent-wave-orchestrator"),
            dark_factory: Default::default(),
            lanes: BTreeMap::new(),
            ..ProjectConfig::default()
        };

        assert_eq!(
            build_specs_dir(Path::new("/repo"), &config),
            PathBuf::from("/repo/.wave/state/build/specs")
        );
    }

    #[test]
    fn bootstrap_authority_roots_materializes_canonical_state_dirs() {
        let root = std::env::temp_dir().join(format!(
            "wave-runtime-authority-roots-{}-{}",
            std::process::id(),
            now_epoch_ms().expect("timestamp")
        ));
        fs::create_dir_all(&root).expect("create temp root");
        let config = ProjectConfig {
            authority: AuthorityConfig::default(),
            ..ProjectConfig::default()
        };

        bootstrap_authority_roots(&root, &config).expect("bootstrap authority roots");
        let authority = config.resolved_paths(&root).authority;

        for path in [
            authority.state_dir,
            authority.state_build_specs_dir,
            authority.state_events_dir,
            authority.state_events_control_dir,
            authority.state_events_coordination_dir,
            authority.state_results_dir,
            authority.state_derived_dir,
            authority.state_projections_dir,
            authority.state_traces_dir,
        ] {
            assert!(path.is_dir(), "{} should exist", path.display());
        }

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    #[ignore = "materializes repo-local authority roots for verification"]
    fn repo_local_bootstrap_materializes_authority_roots() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("canonical repo root");
        let config = ProjectConfig::load_from_repo_root(&root).expect("load repo config");

        bootstrap_authority_roots(&root, &config).expect("bootstrap repo authority roots");
        let authority = config.resolved_paths(&root).authority;

        for path in [
            authority.state_dir,
            authority.state_build_specs_dir,
            authority.state_events_dir,
            authority.state_events_control_dir,
            authority.state_events_coordination_dir,
            authority.state_results_dir,
            authority.state_derived_dir,
            authority.state_projections_dir,
            authority.state_traces_dir,
        ] {
            assert!(path.is_dir(), "{} should exist", path.display());
        }
    }
}
