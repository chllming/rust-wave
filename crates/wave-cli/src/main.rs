use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::io;
use std::io::IsTerminal;
use std::path::Path;
use std::path::PathBuf;
use wave_app_server::ActiveRunDetail;
use wave_app_server::AgentPanelItem;
use wave_app_server::OperatorSnapshot;
use wave_app_server::ProofSnapshot;
use wave_app_server::latest_relevant_run_detail;
use wave_app_server::load_operator_snapshot;
use wave_app_server::load_relevant_run_records;
use wave_config::DEFAULT_CONFIG_PATH;
use wave_config::ProjectConfig;
use wave_control_plane::ControlStatusReadModel;
use wave_control_plane::DeliveryReadModel;
use wave_control_plane::OperatorSnapshotInputs;
use wave_control_plane::PlanningProjectionReadModel;
use wave_control_plane::PlanningStatusReadModel;
use wave_control_plane::ProjectionSpine;
use wave_control_plane::WaveStatusReadModel;
use wave_control_plane::build_control_status_read_model_from_spine;
use wave_control_plane::build_projection_spine_from_authority;
use wave_dark_factory::LintFinding;
use wave_dark_factory::has_errors;
use wave_dark_factory::lint_project;
use wave_dark_factory::validate_context7_bundle_catalog;
use wave_dark_factory::validate_skill_catalog;
use wave_domain::DirectiveOrigin;
use wave_domain::OrchestratorMode;
use wave_domain::RerunScope;
use wave_domain::WaveClosureOverrideRecord;
use wave_runtime::AdhocPlanReport;
use wave_runtime::AdhocPromotionReport;
use wave_runtime::AdhocRunRecord;
use wave_runtime::AdhocRunReport;
use wave_runtime::AutonomousOptions;
use wave_runtime::DogfoodEvidenceReport;
use wave_runtime::LaunchOptions;
use wave_runtime::LaunchPreflightError;
use wave_runtime::LaunchPreflightReport;
use wave_runtime::RerunIntentRecord;
use wave_runtime::active_closure_override_wave_ids;
use wave_runtime::apply_closure_override;
use wave_runtime::approve_agent_merge;
use wave_runtime::autonomous_launch;
use wave_runtime::clear_closure_override;
use wave_runtime::clear_rerun;
use wave_runtime::dogfood_evidence_report;
use wave_runtime::draft_wave;
use wave_runtime::latest_orchestrator_session;
use wave_runtime::launch_wave;
use wave_runtime::list_adhoc_runs;
use wave_runtime::list_closure_overrides;
use wave_runtime::list_control_directives;
use wave_runtime::load_latest_runs;
use wave_runtime::pause_agent;
use wave_runtime::pending_rerun_wave_ids;
use wave_runtime::plan_adhoc;
use wave_runtime::promote_adhoc;
use wave_runtime::rebase_agent_sandbox;
use wave_runtime::reject_agent_merge;
use wave_runtime::repair_orphaned_runs;
use wave_runtime::request_agent_reconciliation;
use wave_runtime::request_rerun;
use wave_runtime::rerun_agent;
use wave_runtime::resume_agent;
use wave_runtime::run_adhoc;
use wave_runtime::runtime_boundary_status;
use wave_runtime::seed_design_authority_live_proof;
use wave_runtime::set_orchestrator_mode;
use wave_runtime::show_adhoc_run;
use wave_runtime::steer_agent;
use wave_runtime::steer_wave;
use wave_runtime::trace_inspection_report;
use wave_spec::WaveDocument;
use wave_spec::load_wave_documents;
use wave_trace::ReplayReport;
use wave_trace::WaveRunRecord;
use wave_trace::load_trace_bundle;

#[derive(Debug, Parser)]
#[command(name = "wave", about = "Rust/Codex Wave operator CLI")]
struct Cli {
    #[arg(long, global = true, default_value = DEFAULT_CONFIG_PATH)]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Doctor {
        #[arg(long)]
        json: bool,
    },
    Lint {
        #[arg(long)]
        json: bool,
    },
    Draft {
        #[arg(long)]
        wave: Option<u32>,
        #[arg(long)]
        json: bool,
    },
    Control {
        #[command(subcommand)]
        command: ControlCommand,
    },
    Delivery {
        #[command(subcommand)]
        command: DeliveryCommand,
    },
    Launch {
        #[arg(long)]
        wave: Option<u32>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    Autonomous {
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        json: bool,
    },
    Dep,
    Trace {
        #[command(subcommand)]
        command: TraceCommand,
    },
    Adhoc {
        #[command(subcommand)]
        command: AdhocCommand,
    },
    Tui {
        #[arg(long, value_enum, default_value_t = TuiAltScreenMode::Auto)]
        alt_screen: TuiAltScreenMode,
        #[arg(long)]
        fresh_session: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum TuiAltScreenMode {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    Show {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ControlCommand {
    Status {
        #[arg(long)]
        json: bool,
    },
    Show {
        #[arg(long)]
        wave: Option<u32>,
        #[arg(long)]
        json: bool,
    },
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    Rerun {
        #[command(subcommand)]
        command: RerunCommand,
    },
    Close {
        #[command(subcommand)]
        command: CloseCommand,
    },
    Repair {
        #[arg(long)]
        json: bool,
    },
    Proof {
        #[command(subcommand)]
        command: ProofCommand,
    },
    Orchestrator {
        #[command(subcommand)]
        command: OrchestratorCommand,
    },
}

#[derive(Debug, Subcommand)]
enum DeliveryCommand {
    Status {
        #[arg(long)]
        json: bool,
    },
    Initiative {
        #[command(subcommand)]
        command: DeliveryEntityCommand,
    },
    Release {
        #[command(subcommand)]
        command: DeliveryEntityCommand,
    },
    Acceptance {
        #[command(subcommand)]
        command: DeliveryEntityCommand,
    },
}

#[derive(Debug, Subcommand)]
enum DeliveryEntityCommand {
    Show {
        #[arg(long = "id")]
        id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum TaskCommand {
    List {
        #[arg(long)]
        wave: Option<u32>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum AgentCommand {
    Pause {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        agent: String,
        #[arg(long)]
        json: bool,
    },
    Resume {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        agent: String,
        #[arg(long)]
        json: bool,
    },
    Rerun {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        agent: String,
        #[arg(long)]
        json: bool,
    },
    Rebase {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        agent: String,
        #[arg(long)]
        json: bool,
    },
    Reconcile {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        agent: String,
        #[arg(long)]
        json: bool,
    },
    ApproveMerge {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        agent: String,
        #[arg(long)]
        json: bool,
    },
    RejectMerge {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        agent: String,
        #[arg(long)]
        json: bool,
    },
    Steer {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        agent: String,
        #[arg(long)]
        message: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum RerunCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Request {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        reason: String,
        #[arg(long, default_value = "full")]
        scope: String,
        #[arg(long)]
        json: bool,
    },
    Clear {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum CloseCommand {
    Apply {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        source_run: Option<String>,
        #[arg(long = "evidence-path")]
        evidence_paths: Vec<String>,
        #[arg(long)]
        detail: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Clear {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ProofCommand {
    Show {
        #[arg(long)]
        wave: Option<u32>,
        #[arg(long)]
        json: bool,
    },
    SeedDesignAuthority {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum OrchestratorCommand {
    Show {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        json: bool,
    },
    Steer {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        message: String,
        #[arg(long)]
        json: bool,
    },
    Mode {
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        mode: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum TraceCommand {
    Latest {
        #[arg(long)]
        wave: Option<u32>,
        #[arg(long)]
        json: bool,
    },
    Replay {
        #[arg(long)]
        wave: Option<u32>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum AdhocCommand {
    Plan {
        #[arg(long)]
        title: String,
        #[arg(long)]
        request: String,
        #[arg(long)]
        owner: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Run {
        #[arg(long = "id")]
        id: String,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        json: bool,
    },
    Show {
        #[arg(long = "id")]
        id: String,
        #[arg(long)]
        json: bool,
    },
    Promote {
        #[arg(long = "id")]
        id: String,
        #[arg(long = "wave-id")]
        wave_id: Option<u32>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Serialize)]
struct DoctorCheck {
    name: &'static str,
    ok: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    ok: bool,
    status: PlanningStatusReadModel,
    projection: PlanningProjectionReadModel,
    operator: OperatorSnapshotInputs,
    control_status: ControlStatusReadModel,
    checks: Vec<DoctorCheck>,
    role_prompts: RolePromptSurface,
    authority: AuthoritySurface,
    runtime_boundary: wave_runtime::RuntimeBoundaryStatus,
    closure_overrides: Vec<WaveClosureOverrideRecord>,
}

#[derive(Debug, Serialize)]
struct ControlStatusReport {
    status: PlanningStatusReadModel,
    projection: PlanningProjectionReadModel,
    operator: OperatorSnapshotInputs,
    delivery: DeliveryReadModel,
    control_status: ControlStatusReadModel,
}

#[derive(Debug, Serialize)]
struct DeliveryStatusReport {
    delivery: DeliveryReadModel,
}

#[derive(Debug, Serialize)]
struct AdhocListReport {
    runs: Vec<AdhocRunRecord>,
}

#[derive(Debug, Serialize)]
struct ControlShowReport {
    wave: WaveStatusReadModel,
    portfolio_focus: Option<PortfolioFocusReport>,
    design_detail: Option<wave_app_server::WaveDesignDetail>,
    latest_run: Option<ActiveRunDetail>,
    acceptance_package: Option<wave_app_server::AcceptancePackageSnapshot>,
    operator_objects: Vec<wave_app_server::OperatorActionableItem>,
    rerun_intent: Option<RerunIntentRecord>,
    closure_override: Option<WaveClosureOverrideRecord>,
    orchestrator_mode: Option<String>,
    directives: Vec<wave_domain::ControlDirectiveRecord>,
}

#[derive(Debug, Serialize)]
struct AgentSteerReport {
    directive: wave_domain::ControlDirectiveRecord,
}

#[derive(Debug, Serialize)]
struct AgentControlReport {
    directive: wave_domain::ControlDirectiveRecord,
}

#[derive(Debug, Serialize)]
struct OrchestratorModeReport {
    session: Option<wave_domain::OrchestratorSessionRecord>,
}

#[derive(Debug, Serialize)]
struct OrchestratorShowReport {
    session: Option<wave_domain::OrchestratorSessionRecord>,
    wave: Option<wave_app_server::WaveOrchestratorSnapshot>,
    directives: Vec<wave_app_server::DirectiveSnapshot>,
}

#[derive(Debug, Serialize)]
struct OrchestratorSteerReport {
    directive: wave_domain::ControlDirectiveRecord,
}

#[derive(Debug, Serialize)]
struct TaskListReport {
    wave_id: u32,
    run_id: Option<String>,
    agents: Vec<AgentPanelItem>,
}

#[derive(Debug, Serialize)]
struct ProofReport {
    wave_id: u32,
    run_id: Option<String>,
    run: Option<ActiveRunDetail>,
    proof: Option<ProofSnapshot>,
    portfolio_focus: Option<PortfolioFocusReport>,
    acceptance_package: Option<wave_app_server::AcceptancePackageSnapshot>,
    replay: Option<ReplayReport>,
}

#[derive(Debug, Clone, Serialize)]
struct PortfolioFocusReport {
    delivery: Option<PortfolioDeliverySummaryReport>,
    initiatives: Vec<PortfolioEntryReport>,
    milestones: Vec<PortfolioEntryReport>,
    release_trains: Vec<PortfolioEntryReport>,
    outcome_contracts: Vec<PortfolioEntryReport>,
}

#[derive(Debug, Clone, Serialize)]
struct PortfolioDeliverySummaryReport {
    ship_state: String,
    release_state: String,
    signoff_state: String,
    summary: String,
    proof_complete: bool,
    completed_agents: usize,
    total_agents: usize,
    proof_source: String,
    known_risk_count: usize,
    outstanding_debt_count: usize,
    blocking_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PortfolioEntryReport {
    id: String,
    title: String,
    wave_ids: Vec<u32>,
    ship_ready_waves: usize,
    accepted_waves: usize,
    signed_off_waves: usize,
    wave_delivery: Vec<PortfolioWaveDeliveryReport>,
    blocking_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PortfolioWaveDeliveryReport {
    wave_id: u32,
    ship_state: String,
    release_state: String,
    signoff_state: String,
}

#[derive(Debug, Serialize)]
struct ControlRepairReport {
    repaired_runs: Vec<RepairRunSurface>,
}

#[derive(Debug, Serialize)]
struct RepairRunSurface {
    wave_id: u32,
    run_id: String,
    status: wave_trace::WaveRunStatus,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct ProjectShowReport {
    project_name: String,
    default_lane: String,
    default_mode: String,
    waves_dir: PathBuf,
    docs_dir: PathBuf,
    skills_dir: PathBuf,
    codex_vendor_dir: PathBuf,
    role_prompts: RolePromptSurface,
    authority: AuthoritySurface,
    runtime_boundary: wave_runtime::RuntimeBoundaryStatus,
    closure_overrides: Vec<WaveClosureOverrideRecord>,
}

#[derive(Debug, Serialize)]
struct RolePromptSurface {
    dir: PathBuf,
    cont_qa: PathBuf,
    cont_eval: PathBuf,
    integration: PathBuf,
    documentation: PathBuf,
    design: PathBuf,
    security: PathBuf,
}

#[derive(Debug, Serialize)]
struct AuthoritySurface {
    project_codex_home: PathBuf,
    state_dir: PathBuf,
    configured_canonical: ConfiguredCanonicalAuthoritySurface,
    materialized_canonical: MaterializedCanonicalAuthoritySurface,
    compatibility: CompatibilityAuthoritySurface,
    projection_source: &'static str,
}

#[derive(Debug, Serialize)]
struct ConfiguredCanonicalAuthoritySurface {
    build_specs: PathBuf,
    events: PathBuf,
    control_events: PathBuf,
    coordination: PathBuf,
    scheduler_events: PathBuf,
    results: PathBuf,
    derived: PathBuf,
    projections: PathBuf,
    state_traces: PathBuf,
}

#[derive(Debug, Serialize)]
struct MaterializedCanonicalAuthoritySurface {
    build_specs: MaterializedPathSurface,
    events: MaterializedPathSurface,
    control_events: MaterializedPathSurface,
    coordination: MaterializedPathSurface,
    scheduler_events: MaterializedPathSurface,
    results: MaterializedPathSurface,
    derived: MaterializedPathSurface,
    projections: MaterializedPathSurface,
    state_traces: MaterializedPathSurface,
}

#[derive(Debug, Serialize)]
struct MaterializedPathSurface {
    path: PathBuf,
    exists: bool,
}

#[derive(Debug, Serialize)]
struct CompatibilityAuthoritySurface {
    state_control: PathBuf,
    state_runs: PathBuf,
    trace_root: PathBuf,
    trace_runs: PathBuf,
}

impl MaterializedCanonicalAuthoritySurface {
    fn entries(&self) -> [&MaterializedPathSurface; 9] {
        [
            &self.build_specs,
            &self.events,
            &self.control_events,
            &self.coordination,
            &self.scheduler_events,
            &self.results,
            &self.derived,
            &self.projections,
            &self.state_traces,
        ]
    }

    fn present_count(&self) -> usize {
        self.entries().iter().filter(|entry| entry.exists).count()
    }

    fn all_exist(&self) -> bool {
        self.present_count() == self.entries().len()
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let (root, config_path) = resolve_cli_root_and_config_path(&cli.config)?;
    let config = ProjectConfig::load(&config_path)?;
    let waves = load_wave_documents(&config, &root)?;
    let findings = lint_project(&root, &waves);
    let skill_catalog_issues = validate_skill_catalog(&root);
    let latest_runs = load_latest_runs(&root, &config)?;
    let rerun_wave_ids = pending_rerun_wave_ids(&root, &config)?;
    let closure_override_wave_ids = active_closure_override_wave_ids(&root, &config)?;
    let runtime_boundary = runtime_boundary_status();
    let spine = build_projection_spine_from_authority(
        &root,
        &config,
        &waves,
        &findings,
        &skill_catalog_issues,
        &latest_runs,
        &rerun_wave_ids,
        &closure_override_wave_ids,
        runtime_boundary
            .runtimes
            .iter()
            .any(|runtime| runtime.available),
    )?;

    match cli.command {
        None => {
            if io::stdout().is_terminal() && io::stdin().is_terminal() {
                wave_tui::run(&root, &config)
            } else {
                render_summary(&config, &spine)
            }
        }
        Some(Command::Project {
            command: ProjectCommand::Show { json },
        }) => render_project(&config, &root, json),
        Some(Command::Doctor { json }) => render_doctor(
            &config_path,
            &config,
            &root,
            &waves,
            &findings,
            &latest_runs,
            &spine,
            json,
        ),
        Some(Command::Lint { json }) => render_lint(&findings, json),
        Some(Command::Draft { wave, json }) => {
            render_draft(&root, &waves, &spine.planning.status, wave, json)
        }
        Some(Command::Control {
            command: ControlCommand::Status { json },
        }) => render_status(&spine, json),
        Some(Command::Delivery {
            command: DeliveryCommand::Status { json },
        }) => render_delivery_status(&spine.delivery, json),
        Some(Command::Delivery {
            command:
                DeliveryCommand::Initiative {
                    command: DeliveryEntityCommand::Show { id, json },
                },
        }) => render_delivery_initiative_show(&spine.delivery, &id, json),
        Some(Command::Delivery {
            command:
                DeliveryCommand::Release {
                    command: DeliveryEntityCommand::Show { id, json },
                },
        }) => render_delivery_release_show(&spine.delivery, &id, json),
        Some(Command::Delivery {
            command:
                DeliveryCommand::Acceptance {
                    command: DeliveryEntityCommand::Show { id, json },
                },
        }) => render_delivery_acceptance_show(&spine.delivery, &id, json),
        Some(Command::Control {
            command: ControlCommand::Show { wave, json },
        }) => render_control_show(&root, &config, wave, json),
        Some(Command::Control {
            command:
                ControlCommand::Task {
                    command: TaskCommand::List { wave, json },
                },
        }) => render_task_list(&root, &config, wave, json),
        Some(Command::Control {
            command:
                ControlCommand::Agent {
                    command: AgentCommand::Pause { wave, agent, json },
                },
        }) => render_agent_control(
            &root,
            &config,
            wave,
            &agent,
            json,
            "paused",
            |root, config, wave, agent| {
                pause_agent(
                    root,
                    config,
                    wave,
                    agent,
                    DirectiveOrigin::Operator,
                    "wave-cli",
                )
            },
        ),
        Some(Command::Control {
            command:
                ControlCommand::Agent {
                    command: AgentCommand::Resume { wave, agent, json },
                },
        }) => render_agent_control(
            &root,
            &config,
            wave,
            &agent,
            json,
            "resumed",
            |root, config, wave, agent| {
                resume_agent(
                    root,
                    config,
                    wave,
                    agent,
                    DirectiveOrigin::Operator,
                    "wave-cli",
                )
            },
        ),
        Some(Command::Control {
            command:
                ControlCommand::Agent {
                    command: AgentCommand::Rerun { wave, agent, json },
                },
        }) => render_agent_control(
            &root,
            &config,
            wave,
            &agent,
            json,
            "scheduled for rerun",
            |root, config, wave, agent| {
                rerun_agent(
                    root,
                    config,
                    wave,
                    agent,
                    DirectiveOrigin::Operator,
                    "wave-cli",
                )
            },
        ),
        Some(Command::Control {
            command:
                ControlCommand::Agent {
                    command: AgentCommand::Rebase { wave, agent, json },
                },
        }) => render_agent_control(
            &root,
            &config,
            wave,
            &agent,
            json,
            "scheduled for rebase",
            |root, config, wave, agent| {
                rebase_agent_sandbox(
                    root,
                    config,
                    wave,
                    agent,
                    DirectiveOrigin::Operator,
                    "wave-cli",
                )
            },
        ),
        Some(Command::Control {
            command:
                ControlCommand::Agent {
                    command: AgentCommand::Reconcile { wave, agent, json },
                },
        }) => render_agent_control(
            &root,
            &config,
            wave,
            &agent,
            json,
            "queued for reconciliation",
            |root, config, wave, agent| {
                request_agent_reconciliation(
                    root,
                    config,
                    wave,
                    agent,
                    DirectiveOrigin::Operator,
                    "wave-cli",
                )
            },
        ),
        Some(Command::Control {
            command:
                ControlCommand::Agent {
                    command: AgentCommand::ApproveMerge { wave, agent, json },
                },
        }) => render_agent_control(
            &root,
            &config,
            wave,
            &agent,
            json,
            "merge approved",
            |root, config, wave, agent| {
                approve_agent_merge(
                    root,
                    config,
                    wave,
                    agent,
                    DirectiveOrigin::Operator,
                    "wave-cli",
                )
            },
        ),
        Some(Command::Control {
            command:
                ControlCommand::Agent {
                    command: AgentCommand::RejectMerge { wave, agent, json },
                },
        }) => render_agent_control(
            &root,
            &config,
            wave,
            &agent,
            json,
            "merge rejected",
            |root, config, wave, agent| {
                reject_agent_merge(
                    root,
                    config,
                    wave,
                    agent,
                    DirectiveOrigin::Operator,
                    "wave-cli",
                )
            },
        ),
        Some(Command::Control {
            command:
                ControlCommand::Agent {
                    command:
                        AgentCommand::Steer {
                            wave,
                            agent,
                            message,
                            json,
                        },
                },
        }) => render_agent_steer(&root, &config, wave, &agent, &message, json),
        Some(Command::Control {
            command:
                ControlCommand::Rerun {
                    command: RerunCommand::List { json },
                },
        }) => render_rerun_list(&root, &config, json),
        Some(Command::Control {
            command:
                ControlCommand::Rerun {
                    command:
                        RerunCommand::Request {
                            wave,
                            reason,
                            scope,
                            json,
                        },
                },
        }) => render_rerun_request(&root, &config, wave, &reason, &scope, json),
        Some(Command::Control {
            command:
                ControlCommand::Rerun {
                    command: RerunCommand::Clear { wave, json },
                },
        }) => render_rerun_clear(&root, &config, wave, json),
        Some(Command::Control {
            command:
                ControlCommand::Close {
                    command:
                        CloseCommand::Apply {
                            wave,
                            reason,
                            source_run,
                            evidence_paths,
                            detail,
                            json,
                        },
                },
        }) => render_close_apply(
            &root,
            &config,
            wave,
            &reason,
            source_run.as_deref(),
            evidence_paths,
            detail,
            json,
        ),
        Some(Command::Control {
            command:
                ControlCommand::Close {
                    command: CloseCommand::Clear { wave, json },
                },
        }) => render_close_clear(&root, &config, wave, json),
        Some(Command::Control {
            command: ControlCommand::Repair { json },
        }) => render_control_repair(&root, &config, json),
        Some(Command::Control {
            command:
                ControlCommand::Proof {
                    command: ProofCommand::Show { wave, json },
                },
        }) => render_proof_show(&root, &config, wave, json),
        Some(Command::Control {
            command:
                ControlCommand::Proof {
                    command: ProofCommand::SeedDesignAuthority { wave, json },
                },
        }) => render_proof_seed_design_authority(&root, &config, wave, json),
        Some(Command::Control {
            command:
                ControlCommand::Orchestrator {
                    command: OrchestratorCommand::Show { wave, json },
                },
        }) => render_orchestrator_show(&root, &config, wave, json),
        Some(Command::Control {
            command:
                ControlCommand::Orchestrator {
                    command:
                        OrchestratorCommand::Steer {
                            wave,
                            message,
                            json,
                        },
                },
        }) => render_orchestrator_steer(&root, &config, wave, &message, json),
        Some(Command::Control {
            command:
                ControlCommand::Orchestrator {
                    command: OrchestratorCommand::Mode { wave, mode, json },
                },
        }) => render_orchestrator_mode(&root, &config, wave, &mode, json),
        Some(Command::Launch {
            wave,
            dry_run,
            json,
        }) => render_launch(
            &root,
            &config,
            &waves,
            &spine.planning.status,
            LaunchOptions {
                wave_id: wave,
                dry_run,
            },
            json,
        ),
        Some(Command::Autonomous {
            limit,
            dry_run,
            json,
        }) => render_autonomous(
            &root,
            &config,
            &waves,
            spine.planning.status.clone(),
            AutonomousOptions { limit, dry_run },
            json,
        ),
        Some(Command::Dep) => render_not_ready(
            "dep",
            "dependency control is still pending; use control status and rerun/proof actions for now",
        ),
        Some(Command::Trace {
            command: TraceCommand::Latest { wave, json },
        }) => render_trace_latest(&latest_runs, wave, json),
        Some(Command::Trace {
            command: TraceCommand::Replay { wave, json },
        }) => render_trace_replay(&latest_runs, wave, json),
        Some(Command::Adhoc {
            command:
                AdhocCommand::Plan {
                    title,
                    request,
                    owner,
                    json,
                },
        }) => render_adhoc_plan(&root, &config, &title, &request, owner.as_deref(), json),
        Some(Command::Adhoc {
            command: AdhocCommand::Run { id, json },
        }) => render_adhoc_run(&root, &config, &id, json),
        Some(Command::Adhoc {
            command: AdhocCommand::List { json },
        }) => render_adhoc_list(&root, &config, json),
        Some(Command::Adhoc {
            command: AdhocCommand::Show { id, json },
        }) => render_adhoc_show(&root, &config, &id, json),
        Some(Command::Adhoc {
            command: AdhocCommand::Promote { id, wave_id, json },
        }) => render_adhoc_promote(&root, &config, &id, wave_id, json),
        Some(Command::Tui {
            alt_screen,
            fresh_session,
        }) => wave_tui::run_with_options(
            &root,
            &config,
            wave_tui::RunOptions {
                alt_screen: tui_alt_screen_mode(alt_screen),
                fresh_session,
            },
        ),
    }
}

fn tui_alt_screen_mode(mode: TuiAltScreenMode) -> wave_tui::AltScreenMode {
    match mode {
        TuiAltScreenMode::Auto => wave_tui::AltScreenMode::Auto,
        TuiAltScreenMode::Always => wave_tui::AltScreenMode::Always,
        TuiAltScreenMode::Never => wave_tui::AltScreenMode::Never,
    }
}

fn config_root(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn resolve_cli_root_and_config_path(config_path: &Path) -> Result<(PathBuf, PathBuf)> {
    resolve_cli_root_and_config_path_from_cwd(config_path, &env::current_dir()?)
}

fn resolve_cli_root_and_config_path_from_cwd(
    config_path: &Path,
    cwd: &Path,
) -> Result<(PathBuf, PathBuf)> {
    if config_path.is_absolute() {
        return Ok((config_root(config_path), config_path.to_path_buf()));
    }

    if config_path == Path::new(DEFAULT_CONFIG_PATH) {
        if let Some(shared_root) = shared_repo_root_from_worktree(cwd) {
            return Ok((shared_root.clone(), shared_root.join(DEFAULT_CONFIG_PATH)));
        }
    }

    let resolved_config_path = cwd.join(config_path);
    Ok((config_root(&resolved_config_path), resolved_config_path))
}

fn shared_repo_root_from_worktree(path: &Path) -> Option<PathBuf> {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    for ancestor in resolved.ancestors() {
        if ancestor.file_name().is_some_and(|name| name == "worktrees") {
            let state_dir = ancestor.parent()?;
            let wave_dir = state_dir.parent()?;
            if state_dir.file_name().is_some_and(|name| name == "state")
                && wave_dir.file_name().is_some_and(|name| name == ".wave")
            {
                return wave_dir.parent().map(Path::to_path_buf);
            }
        }
    }
    None
}

fn render_summary(config: &ProjectConfig, spine: &ProjectionSpine) -> Result<()> {
    let report = build_control_status_report(spine);
    let status = &report.status;
    let projection = &report.projection;
    let control_status = &report.control_status;
    let operator = &report.operator;

    println!("Wave operator shell");
    println!("project: {}", config.project_name);
    println!("mode: {}", config.default_mode);
    println!("waves dir: {}", config.waves_dir.display());
    println!(
        "queue: total={} ready={} blocked={} active={} completed={}",
        status.summary.total_waves,
        status.summary.ready_waves,
        status.summary.blocked_waves,
        status.summary.active_waves,
        status.summary.completed_waves
    );
    println!(
        "agents: total={} impl={} closure={}",
        status.summary.total_agents,
        status.summary.implementation_agents,
        status.summary.closure_agents
    );
    println!(
        "coverage: complete={} missing={} missing_agents={}",
        status.summary.waves_with_complete_closure,
        status.summary.waves_missing_closure,
        status.summary.total_missing_closure_agents
    );
    println!(
        "skill catalog: {} ({} issues)",
        if projection.skill_catalog.ok {
            "ok"
        } else {
            "error"
        },
        projection.skill_catalog.issue_count
    );
    println!(
        "delivery: initiatives={} releases={} acceptance={} blocking_risks={} blocking_debts={}",
        status.delivery.initiative_count,
        status.delivery.release_count,
        status.delivery.acceptance_package_count,
        status.delivery.blocking_risk_count,
        status.delivery.blocking_debt_count
    );
    for line in &control_status.queue_decision.lines {
        println!("{line}");
    }
    println!(
        "signal: queue={} soft={} exit_code={}",
        control_status.signal.queue_state,
        control_status.signal.delivery_soft_state.label(),
        control_status.signal.exit_code
    );
    println!(
        "skill issue paths: {}",
        format_string_list(&control_status.skill_issue_paths)
    );
    println!(
        "launcher: ready={} runtimes={}",
        operator.control.launcher_ready && !status.next_ready_wave_ids.is_empty(),
        runtime_boundary_status()
            .runtimes
            .iter()
            .map(|runtime| format!(
                "{}={}",
                runtime.runtime,
                if runtime.available {
                    "ready"
                } else {
                    "blocked"
                }
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("active runs: {}", operator.dashboard.active_runs.len());
    Ok(())
}

fn role_prompt_surface(config: &ProjectConfig, root: &Path) -> RolePromptSurface {
    let resolved = config.resolved_paths(root);
    RolePromptSurface {
        dir: resolved.role_prompts.dir,
        cont_qa: resolved.role_prompts.cont_qa,
        cont_eval: resolved.role_prompts.cont_eval,
        integration: resolved.role_prompts.integration,
        documentation: resolved.role_prompts.documentation,
        design: resolved.role_prompts.design,
        security: resolved.role_prompts.security,
    }
}

fn authority_surface(config: &ProjectConfig, root: &Path) -> AuthoritySurface {
    let resolved = config.resolved_paths(root);
    let configured_canonical = ConfiguredCanonicalAuthoritySurface {
        build_specs: resolved.authority.state_build_specs_dir.clone(),
        events: resolved.authority.state_events_dir.clone(),
        control_events: resolved.authority.state_events_control_dir.clone(),
        coordination: resolved.authority.state_events_coordination_dir.clone(),
        scheduler_events: resolved.authority.state_events_scheduler_dir.clone(),
        results: resolved.authority.state_results_dir.clone(),
        derived: resolved.authority.state_derived_dir.clone(),
        projections: resolved.authority.state_projections_dir.clone(),
        state_traces: resolved.authority.state_traces_dir.clone(),
    };
    AuthoritySurface {
        project_codex_home: resolved.authority.project_codex_home,
        state_dir: resolved.authority.state_dir,
        configured_canonical,
        materialized_canonical: MaterializedCanonicalAuthoritySurface {
            build_specs: materialized_path_surface(
                resolved.authority.state_build_specs_dir.clone(),
            ),
            events: materialized_path_surface(resolved.authority.state_events_dir.clone()),
            control_events: materialized_path_surface(
                resolved.authority.state_events_control_dir.clone(),
            ),
            coordination: materialized_path_surface(
                resolved.authority.state_events_coordination_dir.clone(),
            ),
            scheduler_events: materialized_path_surface(
                resolved.authority.state_events_scheduler_dir.clone(),
            ),
            results: materialized_path_surface(resolved.authority.state_results_dir.clone()),
            derived: materialized_path_surface(resolved.authority.state_derived_dir.clone()),
            projections: materialized_path_surface(
                resolved.authority.state_projections_dir.clone(),
            ),
            state_traces: materialized_path_surface(resolved.authority.state_traces_dir.clone()),
        },
        compatibility: CompatibilityAuthoritySurface {
            state_control: resolved.authority.state_control_dir,
            state_runs: resolved.authority.state_runs_dir,
            trace_root: resolved.authority.trace_dir,
            trace_runs: resolved.authority.trace_runs_dir,
        },
        projection_source: "planning status, queue/control JSON, and operator-facing status surfaces are reducer-backed projections over canonical scheduler authority plus compatibility run records; proof and closure surfaces are envelope-first, and replay remains compatibility-backed",
    }
}

fn ensure_authority_roots_materialized(config: &ProjectConfig, root: &Path) -> Result<()> {
    config
        .resolved_paths(root)
        .authority
        .materialize_canonical_state_tree()
}

fn render_project(config: &ProjectConfig, root: &Path, json: bool) -> Result<()> {
    ensure_authority_roots_materialized(config, root)?;
    let resolved = config.resolved_paths(root);
    let mut closure_overrides = list_closure_overrides(root, config)?
        .into_values()
        .collect::<Vec<_>>();
    closure_overrides.sort_by_key(|record| record.wave_id);
    let report = ProjectShowReport {
        project_name: config.project_name.clone(),
        default_lane: config.default_lane.clone(),
        default_mode: config.default_mode.to_string(),
        waves_dir: resolved.waves_dir,
        docs_dir: resolved.docs_dir,
        skills_dir: resolved.skills_dir,
        codex_vendor_dir: resolved.codex_vendor_dir,
        role_prompts: role_prompt_surface(config, root),
        authority: authority_surface(config, root),
        runtime_boundary: runtime_boundary_status(),
        closure_overrides,
    };
    if json {
        print_json(&report)
    } else {
        println!("project: {}", report.project_name);
        println!("default lane: {}", report.default_lane);
        println!("default mode: {}", report.default_mode);
        println!("waves dir: {}", report.waves_dir.display());
        println!("docs dir: {}", report.docs_dir.display());
        println!("skills dir: {}", report.skills_dir.display());
        println!("codex vendor dir: {}", report.codex_vendor_dir.display());
        println!(
            "runtime boundary: {} | {} | {}",
            report.runtime_boundary.executor_boundary,
            report.runtime_boundary.selection_policy,
            report.runtime_boundary.fallback_policy
        );
        println!(
            "runtime availability: {}",
            report
                .runtime_boundary
                .runtimes
                .iter()
                .map(|runtime| format!(
                    "{}={} ({})",
                    runtime.runtime,
                    if runtime.available {
                        "ready"
                    } else {
                        "blocked"
                    },
                    runtime.detail
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!(
            "role prompts: dir={} | cont_qa={} cont_eval={} integration={} documentation={} design={} security={}",
            report.role_prompts.dir.display(),
            report.role_prompts.cont_qa.display(),
            report.role_prompts.cont_eval.display(),
            report.role_prompts.integration.display(),
            report.role_prompts.documentation.display(),
            report.role_prompts.design.display(),
            report.role_prompts.security.display()
        );
        println!(
            "authority roots: project_codex_home={} state_root={}",
            report.authority.project_codex_home.display(),
            report.authority.state_dir.display()
        );
        println!(
            "configured canonical roots: build_specs={} events={} control_events={} coordination={} scheduler_events={} results={} derived={} projections={} state_traces={}",
            report.authority.configured_canonical.build_specs.display(),
            report.authority.configured_canonical.events.display(),
            report
                .authority
                .configured_canonical
                .control_events
                .display(),
            report.authority.configured_canonical.coordination.display(),
            report
                .authority
                .configured_canonical
                .scheduler_events
                .display(),
            report.authority.configured_canonical.results.display(),
            report.authority.configured_canonical.derived.display(),
            report.authority.configured_canonical.projections.display(),
            report.authority.configured_canonical.state_traces.display()
        );
        println!(
            "materialized canonical roots: build_specs={} events={} control_events={} coordination={} scheduler_events={} results={} derived={} projections={} state_traces={}",
            format_materialized_path(&report.authority.materialized_canonical.build_specs),
            format_materialized_path(&report.authority.materialized_canonical.events),
            format_materialized_path(&report.authority.materialized_canonical.control_events),
            format_materialized_path(&report.authority.materialized_canonical.coordination),
            format_materialized_path(&report.authority.materialized_canonical.scheduler_events),
            format_materialized_path(&report.authority.materialized_canonical.results),
            format_materialized_path(&report.authority.materialized_canonical.derived),
            format_materialized_path(&report.authority.materialized_canonical.projections),
            format_materialized_path(&report.authority.materialized_canonical.state_traces)
        );
        println!(
            "compatibility roots: state_control={} state_runs={} trace_root={} trace_runs={}",
            report.authority.compatibility.state_control.display(),
            report.authority.compatibility.state_runs.display(),
            report.authority.compatibility.trace_root.display(),
            report.authority.compatibility.trace_runs.display()
        );
        println!("projection source: {}", report.authority.projection_source);
        println!(
            "closure overrides: {}",
            if report.closure_overrides.is_empty() {
                "none".to_string()
            } else {
                report
                    .closure_overrides
                    .iter()
                    .map(|record| {
                        format!(
                            "wave {}={} ({})",
                            record.wave_id,
                            closure_override_status_label(record),
                            record.reason
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        Ok(())
    }
}

fn render_doctor(
    config_path: &Path,
    config: &ProjectConfig,
    root: &Path,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    latest_runs: &HashMap<u32, WaveRunRecord>,
    spine: &ProjectionSpine,
    json: bool,
) -> Result<()> {
    let report = build_doctor_report(
        config_path,
        config,
        root,
        waves,
        findings,
        latest_runs,
        spine,
    )?;
    if json {
        return print_json(&report);
    }
    let status = &report.status;
    let projection = &report.projection;
    let control_status = &report.control_status;
    println!("doctor: {}", if report.ok { "ok" } else { "error" });
    println!(
        "authored waves: {} (agents={} implementation={} closure={})",
        status.summary.total_waves,
        status.summary.total_agents,
        status.summary.implementation_agents,
        status.summary.closure_agents
    );
    println!(
        "queue: ready={} blocked={} active={} completed={}",
        status.summary.ready_waves,
        status.summary.blocked_waves,
        status.summary.active_waves,
        status.summary.completed_waves
    );
    println!(
        "closure coverage: complete={} missing={} missing_agents={}",
        status.summary.waves_with_complete_closure,
        status.summary.waves_missing_closure,
        status.summary.total_missing_closure_agents
    );
    println!(
        "skill catalog: {} ({} issues)",
        if projection.skill_catalog.ok {
            "ok"
        } else {
            "error"
        },
        projection.skill_catalog.issue_count
    );
    for line in &control_status.queue_decision.lines {
        println!("{line}");
    }
    println!(
        "skill issue paths: {}",
        format_string_list(&control_status.skill_issue_paths)
    );
    println!(
        "typed role prompts: dir={} | cont_qa={} cont_eval={} integration={} documentation={} design={} security={}",
        report.role_prompts.dir.display(),
        report.role_prompts.cont_qa.display(),
        report.role_prompts.cont_eval.display(),
        report.role_prompts.integration.display(),
        report.role_prompts.documentation.display(),
        report.role_prompts.design.display(),
        report.role_prompts.security.display()
    );
    println!(
        "typed authority roots: project_codex_home={} state_root={}",
        report.authority.project_codex_home.display(),
        report.authority.state_dir.display()
    );
    println!(
        "runtime boundary: {}",
        report.runtime_boundary.executor_boundary
    );
    println!(
        "runtime availability: {}",
        report
            .runtime_boundary
            .runtimes
            .iter()
            .map(|runtime| format!(
                "{}={} ({})",
                runtime.runtime,
                if runtime.available {
                    "ready"
                } else {
                    "blocked"
                },
                runtime.detail
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "closure overrides: {}",
        if report.closure_overrides.is_empty() {
            "none".to_string()
        } else {
            report
                .closure_overrides
                .iter()
                .map(|record| {
                    format!(
                        "wave {}={} source_run={}",
                        record.wave_id,
                        closure_override_status_label(record),
                        record.source_run_id
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    println!(
        "configured canonical roots: build_specs={} events={} control_events={} coordination={} scheduler_events={} results={} derived={} projections={} state_traces={}",
        report.authority.configured_canonical.build_specs.display(),
        report.authority.configured_canonical.events.display(),
        report
            .authority
            .configured_canonical
            .control_events
            .display(),
        report.authority.configured_canonical.coordination.display(),
        report
            .authority
            .configured_canonical
            .scheduler_events
            .display(),
        report.authority.configured_canonical.results.display(),
        report.authority.configured_canonical.derived.display(),
        report.authority.configured_canonical.projections.display(),
        report.authority.configured_canonical.state_traces.display()
    );
    println!(
        "materialized canonical roots: build_specs={} events={} control_events={} coordination={} scheduler_events={} results={} derived={} projections={} state_traces={}",
        format_materialized_path(&report.authority.materialized_canonical.build_specs),
        format_materialized_path(&report.authority.materialized_canonical.events),
        format_materialized_path(&report.authority.materialized_canonical.control_events),
        format_materialized_path(&report.authority.materialized_canonical.coordination),
        format_materialized_path(&report.authority.materialized_canonical.scheduler_events),
        format_materialized_path(&report.authority.materialized_canonical.results),
        format_materialized_path(&report.authority.materialized_canonical.derived),
        format_materialized_path(&report.authority.materialized_canonical.projections),
        format_materialized_path(&report.authority.materialized_canonical.state_traces)
    );
    println!(
        "compatibility truth: state_control={} state_runs={} trace_root={} trace_runs={}",
        report.authority.compatibility.state_control.display(),
        report.authority.compatibility.state_runs.display(),
        report.authority.compatibility.trace_root.display(),
        report.authority.compatibility.trace_runs.display()
    );
    println!("projection source: {}", report.authority.projection_source);
    for check in &report.checks {
        println!(
            "- {}: {} ({})",
            check.name,
            if check.ok { "ok" } else { "error" },
            check.detail
        );
    }
    for line in &control_status.closure_attention_lines {
        println!("{line}");
    }
    for line in &control_status.skill_issue_lines {
        println!("{line}");
    }
    Ok(())
}

fn build_doctor_report(
    config_path: &Path,
    config: &ProjectConfig,
    root: &Path,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    latest_runs: &HashMap<u32, WaveRunRecord>,
    spine: &ProjectionSpine,
) -> Result<DoctorReport> {
    ensure_authority_roots_materialized(config, root)?;
    let status = &spine.planning.status;
    let projection = &spine.planning.projection;
    let control_status = build_control_status_read_model_from_spine(spine);
    let context7_catalog_issues = validate_context7_bundle_catalog(root);
    let resolved_paths = config.resolved_paths(root);
    let role_prompts = role_prompt_surface(config, root);
    let authority = authority_surface(config, root);
    let runtime_boundary = runtime_boundary_status();
    let mut closure_overrides = list_closure_overrides(root, config)?
        .into_values()
        .collect::<Vec<_>>();
    closure_overrides.sort_by_key(|record| record.wave_id);
    let role_prompt_checks = resolved_paths
        .role_prompts
        .all_files()
        .iter()
        .map(|path| path.exists())
        .collect::<Vec<_>>();
    let authority_roots_ok = resolved_paths.authority.canonical_roots_within_state_dir();
    let materialized_root_count = authority.materialized_canonical.present_count();
    let materialized_root_total = authority.materialized_canonical.entries().len();
    let authority_materialization_ok =
        materialized_root_count == 0 || authority.materialized_canonical.all_exist();
    let authority_materialization_detail = if materialized_root_count == 0 {
        "runtime bootstrap has not materialized canonical roots yet".to_string()
    } else {
        format!(
            "{} of {} canonical roots materialized | build_specs={} events={} control_events={} coordination={} scheduler_events={} results={} derived={} projections={} state_traces={}",
            materialized_root_count,
            materialized_root_total,
            format_materialized_path(&authority.materialized_canonical.build_specs),
            format_materialized_path(&authority.materialized_canonical.events),
            format_materialized_path(&authority.materialized_canonical.control_events),
            format_materialized_path(&authority.materialized_canonical.coordination),
            format_materialized_path(&authority.materialized_canonical.scheduler_events),
            format_materialized_path(&authority.materialized_canonical.results),
            format_materialized_path(&authority.materialized_canonical.derived),
            format_materialized_path(&authority.materialized_canonical.projections),
            format_materialized_path(&authority.materialized_canonical.state_traces),
        )
    };
    let checks = vec![
        DoctorCheck {
            name: "config",
            ok: true,
            detail: format!("loaded {}", config_path.display()),
        },
        DoctorCheck {
            name: "authored-waves",
            ok: !waves.is_empty(),
            detail: format!(
                "{} waves, {} agents ({} implementation, {} closure)",
                status.summary.total_waves,
                status.summary.total_agents,
                status.summary.implementation_agents,
                status.summary.closure_agents
            ),
        },
        DoctorCheck {
            name: "codex-upstream",
            ok: root
                .join(&config.codex_vendor_dir)
                .join("UPSTREAM.toml")
                .exists(),
            detail: format!(
                "checked {}",
                root.join(&config.codex_vendor_dir)
                    .join("UPSTREAM.toml")
                    .display()
            ),
        },
        DoctorCheck {
            name: "wave-upstream",
            ok: root
                .join(&config.reference_wave_repo_dir)
                .join("UPSTREAM.toml")
                .exists(),
            detail: format!(
                "checked {}",
                root.join(&config.reference_wave_repo_dir)
                    .join("UPSTREAM.toml")
                    .display()
            ),
        },
        DoctorCheck {
            name: "lint",
            ok: !has_errors(findings),
            detail: format!(
                "{} findings, {} waves with lint errors",
                findings.len(),
                status.summary.lint_error_waves
            ),
        },
        DoctorCheck {
            name: "closure-coverage",
            ok: projection.closure_coverage.waves.is_empty(),
            detail: format!(
                "{} complete, {} missing, required agents present {}/{}, {} absent",
                projection.closure_coverage.complete_wave_ids.len(),
                projection.closure_coverage.missing_wave_ids.len(),
                projection.closure_coverage.present_agents,
                projection.closure_coverage.required_agents,
                projection.closure_coverage.missing_required_agents
            ),
        },
        DoctorCheck {
            name: "skill-catalog",
            ok: projection.skill_catalog.ok,
            detail: format!(
                "{} issues ({})",
                projection.skill_catalog.issue_count,
                format_string_list(&projection.skill_catalog.issue_paths)
            ),
        },
        DoctorCheck {
            name: "context7-catalog",
            ok: context7_catalog_issues.is_empty(),
            detail: format!(
                "{} issues ({})",
                context7_catalog_issues.len(),
                format_string_list(
                    &context7_catalog_issues
                        .iter()
                        .map(|issue| issue.path.clone())
                        .collect::<Vec<_>>()
                )
            ),
        },
        DoctorCheck {
            name: "typed-role-prompts",
            ok: role_prompt_checks.iter().all(|ok| *ok),
            detail: format!(
                "dir={} | cont_qa={} cont_eval={} integration={} documentation={} design={} security={}",
                role_prompts.dir.display(),
                role_prompts.cont_qa.display(),
                role_prompts.cont_eval.display(),
                role_prompts.integration.display(),
                role_prompts.documentation.display(),
                role_prompts.design.display(),
                role_prompts.security.display()
            ),
        },
        DoctorCheck {
            name: "typed-authority-roots",
            ok: authority_roots_ok,
            detail: format!(
                "state_root={} | build_specs={} control_events={} coordination={} scheduler_events={} results={} derived={} projections={} state_traces={} | compatibility truth remains state_runs={} trace_runs={}",
                authority.state_dir.display(),
                authority.configured_canonical.build_specs.display(),
                authority.configured_canonical.control_events.display(),
                authority.configured_canonical.coordination.display(),
                authority.configured_canonical.scheduler_events.display(),
                authority.configured_canonical.results.display(),
                authority.configured_canonical.derived.display(),
                authority.configured_canonical.projections.display(),
                authority.configured_canonical.state_traces.display(),
                authority.compatibility.state_runs.display(),
                authority.compatibility.trace_runs.display()
            ),
        },
        DoctorCheck {
            name: "materialized-authority-roots",
            ok: authority_materialization_ok,
            detail: authority_materialization_detail,
        },
        DoctorCheck {
            name: "runtime-boundary",
            ok: runtime_boundary
                .runtimes
                .iter()
                .any(|runtime| runtime.available),
            detail: runtime_boundary
                .runtimes
                .iter()
                .map(|runtime| {
                    format!(
                        "{}={} ({})",
                        runtime.runtime,
                        if runtime.available {
                            "ready"
                        } else {
                            "blocked"
                        },
                        runtime.detail
                    )
                })
                .collect::<Vec<_>>()
                .join(", "),
        },
        DoctorCheck {
            name: "run-state",
            ok: true,
            detail: format!(
                "loaded {} recorded runs, {} active waves",
                latest_runs.len(),
                status.summary.active_waves
            ),
        },
        DoctorCheck {
            name: "planning-queue",
            ok: true,
            detail: format!(
                "ready={}, blocked={}, active={}, completed={} | blockers dependency={} ({}) lint={} ({}) closure={} ({}) active_run={} ({})",
                projection.queue.ready.len(),
                projection.queue.blocked.len(),
                projection.queue.active.len(),
                projection.queue.completed.len(),
                projection.queue.blocker_summary.dependency,
                projection.queue.blocker_waves.dependency.len(),
                projection.queue.blocker_summary.lint,
                projection.queue.blocker_waves.lint.len(),
                projection.queue.blocker_summary.closure,
                projection.queue.blocker_waves.closure.len(),
                projection.queue.blocker_summary.active_run,
                projection.queue.blocker_waves.active_run.len()
            ),
        },
    ];
    Ok(DoctorReport {
        ok: checks.iter().all(|check| check.ok),
        status: status.clone(),
        projection: projection.clone(),
        operator: spine.operator.clone(),
        control_status: control_status.clone(),
        checks,
        role_prompts,
        authority,
        runtime_boundary,
        closure_overrides,
    })
}

fn render_lint(findings: &[LintFinding], json: bool) -> Result<()> {
    if json {
        print_json(&findings)?;
    } else if findings.is_empty() {
        println!("lint: ok");
    } else {
        for finding in findings {
            println!(
                "wave {} [{}] {}: {}",
                finding.wave_id,
                format!("{:?}", finding.severity).to_lowercase(),
                finding.rule,
                finding.message
            );
        }
    }

    if findings.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("lint failed with {} finding(s)", findings.len())
    }
}

fn render_draft(
    root: &Path,
    waves: &[WaveDocument],
    status: &PlanningStatusReadModel,
    wave_id: Option<u32>,
    json: bool,
) -> Result<()> {
    let bundle = draft_wave(root, waves, status, wave_id)?;
    if json {
        print_json(&bundle)
    } else {
        println!("drafted wave {}", bundle.wave_id);
        println!("run id: {}", bundle.run_id);
        println!("bundle dir: {}", bundle.bundle_dir.display());
        for agent in &bundle.agents {
            println!(
                "- {} {} => {}",
                agent.id,
                agent.title,
                agent.prompt_path.display()
            );
        }
        Ok(())
    }
}

fn render_status(spine: &ProjectionSpine, json: bool) -> Result<()> {
    let report = build_control_status_report(spine);
    if json {
        print_json(&report)
    } else {
        let status = &report.status;
        let projection = &report.projection;
        let control_status = &report.control_status;
        println!("project: {}", status.project_name);
        println!("mode: {}", status.default_mode);
        println!(
            "queue: total={} ready={} blocked={} active={} completed={}",
            status.summary.total_waves,
            status.summary.ready_waves,
            status.summary.blocked_waves,
            status.summary.active_waves,
            status.summary.completed_waves
        );
        println!(
            "agents: total={} impl={} closure={}",
            status.summary.total_agents,
            status.summary.implementation_agents,
            status.summary.closure_agents
        );
        println!(
            "coverage: complete={} missing={} missing_agents={}",
            status.summary.waves_with_complete_closure,
            status.summary.waves_missing_closure,
            status.summary.total_missing_closure_agents
        );
        println!(
            "skill catalog: {} ({} issues)",
            if projection.skill_catalog.ok {
                "ok"
            } else {
                "error"
            },
            projection.skill_catalog.issue_count
        );
        println!(
            "delivery: initiatives={} releases={} acceptance={} blocking_risks={} blocking_debts={}",
            status.delivery.initiative_count,
            status.delivery.release_count,
            status.delivery.acceptance_package_count,
            status.delivery.blocking_risk_count,
            status.delivery.blocking_debt_count
        );
        for line in &control_status.queue_decision.lines {
            println!("{line}");
        }
        println!(
            "signal: queue={} soft={} exit_code={}",
            control_status.signal.queue_state,
            control_status.signal.delivery_soft_state.label(),
            control_status.signal.exit_code
        );
        println!(
            "skill issue paths: {}",
            format_string_list(&control_status.skill_issue_paths)
        );
        if !control_status.closure_attention_lines.is_empty() {
            for line in &control_status.closure_attention_lines {
                println!("{line}");
            }
        }
        if !control_status.skill_issue_lines.is_empty() {
            for line in &control_status.skill_issue_lines {
                println!("{line}");
            }
        }
        if !control_status.delivery_attention_lines.is_empty() {
            for line in &control_status.delivery_attention_lines {
                println!("{line}");
            }
        }
        Ok(())
    }
}

fn build_control_status_report(spine: &ProjectionSpine) -> ControlStatusReport {
    ControlStatusReport {
        status: spine.planning.status.clone(),
        projection: spine.planning.projection.clone(),
        operator: spine.operator.clone(),
        delivery: spine.delivery.clone(),
        control_status: build_control_status_read_model_from_spine(spine),
    }
}

fn render_delivery_status(delivery: &DeliveryReadModel, json: bool) -> Result<()> {
    let report = DeliveryStatusReport {
        delivery: delivery.clone(),
    };
    if json {
        print_json(&report)
    } else {
        println!(
            "delivery: initiatives={} releases={} acceptance={} blocking_risks={} blocking_debts={}",
            delivery.summary.initiative_count,
            delivery.summary.release_count,
            delivery.summary.acceptance_package_count,
            delivery.summary.blocking_risk_count,
            delivery.summary.blocking_debt_count
        );
        println!(
            "signal: queue={} soft={} exit_code={}",
            delivery.signal.queue_state,
            delivery.signal.delivery_soft_state.label(),
            delivery.signal.exit_code
        );
        if !delivery.attention_lines.is_empty() {
            for line in &delivery.attention_lines {
                println!("{line}");
            }
        }
        Ok(())
    }
}

fn render_delivery_initiative_show(
    delivery: &DeliveryReadModel,
    id: &str,
    json: bool,
) -> Result<()> {
    let initiative = delivery
        .initiatives
        .iter()
        .find(|initiative| initiative.id == id)
        .cloned()
        .with_context(|| format!("unknown initiative id {}", id))?;
    if json {
        print_json(&initiative)
    } else {
        println!("initiative {} {}", initiative.id, initiative.title);
        println!(
            "state={} soft={} owners={} waves={} releases={}",
            initiative
                .state
                .map(|state| format!("{state:?}").to_ascii_lowercase())
                .unwrap_or_else(|| "unspecified".to_string()),
            initiative.soft_state.label(),
            format_string_list(&initiative.owners),
            format_u32_list(&initiative.wave_ids),
            format_string_list(&initiative.release_ids)
        );
        if let Some(outcome) = initiative.outcome {
            println!("outcome: {outcome}");
        }
        println!("summary: {}", initiative.summary);
        Ok(())
    }
}

fn render_delivery_release_show(delivery: &DeliveryReadModel, id: &str, json: bool) -> Result<()> {
    let release = delivery
        .releases
        .iter()
        .find(|release| release.id == id)
        .cloned()
        .with_context(|| format!("unknown release id {}", id))?;
    if json {
        print_json(&release)
    } else {
        println!("release {} {}", release.id, release.title);
        println!(
            "state={} soft={} ready={} initiative={} waves={}",
            release
                .state
                .map(|state| format!("{state:?}").to_ascii_lowercase())
                .unwrap_or_else(|| "unspecified".to_string()),
            release.soft_state.label(),
            yes_no(release.ready),
            release.initiative_id.unwrap_or_else(|| "none".to_string()),
            format_u32_list(&release.wave_ids)
        );
        println!(
            "acceptance={} blockers={}",
            format_string_list(&release.acceptance_package_ids),
            format_string_list(&release.blocked_reasons)
        );
        println!("summary: {}", release.summary);
        Ok(())
    }
}

fn render_delivery_acceptance_show(
    delivery: &DeliveryReadModel,
    id: &str,
    json: bool,
) -> Result<()> {
    let acceptance = delivery
        .acceptance_packages
        .iter()
        .find(|acceptance| acceptance.id == id)
        .cloned()
        .with_context(|| format!("unknown acceptance package id {}", id))?;
    if json {
        print_json(&acceptance)
    } else {
        println!("acceptance {} {}", acceptance.id, acceptance.title);
        println!(
            "state={} soft={} ship_ready={} release={} waves={}",
            acceptance
                .state
                .map(|state| format!("{state:?}").to_ascii_lowercase())
                .unwrap_or_else(|| "unspecified".to_string()),
            acceptance.soft_state.label(),
            yes_no(acceptance.ship_ready),
            acceptance.release_id.unwrap_or_else(|| "none".to_string()),
            format_u32_list(&acceptance.wave_ids)
        );
        println!(
            "signoffs={} blockers={}",
            format_string_list(&acceptance.signoffs),
            format_string_list(&acceptance.blocked_reasons)
        );
        println!("summary: {}", acceptance.summary);
        Ok(())
    }
}

fn render_adhoc_plan(
    root: &Path,
    config: &ProjectConfig,
    title: &str,
    request: &str,
    owner: Option<&str>,
    json: bool,
) -> Result<()> {
    let report: AdhocPlanReport = plan_adhoc(root, config, title, request, owner)?;
    if json {
        print_json(&report)
    } else {
        println!("planned adhoc run {}", report.run_id);
        println!("run dir: {}", report.run_dir.display());
        println!("runtime dir: {}", report.runtime_dir.display());
        println!("wave doc: {}", report.wave_path.display());
        Ok(())
    }
}

fn render_adhoc_run(root: &Path, config: &ProjectConfig, id: &str, json: bool) -> Result<()> {
    let report: AdhocRunReport = run_adhoc(root, config, id)?;
    if json {
        print_json(&report)
    } else {
        println!("launched adhoc run {}", report.record.run_id);
        println!("bundle dir: {}", report.launch.bundle_dir.display());
        println!("state path: {}", report.launch.state_path.display());
        println!("trace path: {}", report.launch.trace_path.display());
        Ok(())
    }
}

fn render_adhoc_list(root: &Path, config: &ProjectConfig, json: bool) -> Result<()> {
    let report = AdhocListReport {
        runs: list_adhoc_runs(root, config)?,
    };
    if json {
        print_json(&report)
    } else if report.runs.is_empty() {
        println!("no adhoc runs recorded");
        Ok(())
    } else {
        for run in report.runs {
            println!(
                "{} {} {}",
                run.run_id,
                format!("{:?}", run.result.status).to_ascii_lowercase(),
                run.request.title
            );
        }
        Ok(())
    }
}

fn render_adhoc_show(root: &Path, config: &ProjectConfig, id: &str, json: bool) -> Result<()> {
    let record = show_adhoc_run(root, config, id)?;
    if json {
        print_json(&record)
    } else {
        println!("adhoc {}", record.run_id);
        println!("title: {}", record.request.title);
        println!(
            "status: {}",
            format!("{:?}", record.result.status).to_ascii_lowercase()
        );
        println!("wave doc: {}", record.wave_path);
        println!("runtime dir: {}", record.runtime_dir);
        if let Some(detail) = record.result.detail {
            println!("detail: {detail}");
        }
        Ok(())
    }
}

fn render_adhoc_promote(
    root: &Path,
    config: &ProjectConfig,
    id: &str,
    wave_id: Option<u32>,
    json: bool,
) -> Result<()> {
    let report: AdhocPromotionReport = promote_adhoc(root, config, id, wave_id)?;
    if json {
        print_json(&report)
    } else {
        println!(
            "promoted adhoc {} to wave {}",
            report.record.run_id, report.promoted_wave_id
        );
        println!("path: {}", report.promoted_path.display());
        Ok(())
    }
}

fn render_control_show(
    root: &Path,
    config: &ProjectConfig,
    wave_id: Option<u32>,
    json: bool,
) -> Result<()> {
    let snapshot = load_operator_snapshot(root, config)?;
    let wave_id = select_wave_id(&snapshot, wave_id)?;
    let Some(wave) = snapshot
        .planning
        .waves
        .iter()
        .find(|entry| entry.id == wave_id)
        .cloned()
    else {
        println!("wave {} was not found", wave_id);
        return Ok(());
    };
    let latest_run = snapshot
        .latest_run_details
        .iter()
        .find(|run| run.wave_id == wave_id)
        .cloned();
    let design_detail = design_detail_for_wave(&snapshot, wave_id).cloned();
    let acceptance_package = snapshot
        .acceptance_packages
        .iter()
        .find(|package| package.wave_id == wave_id)
        .cloned();
    let portfolio_focus =
        portfolio_focus_report(&snapshot.planning, wave_id, &snapshot.acceptance_packages);
    let operator_objects = snapshot
        .operator_objects
        .iter()
        .filter(|item| item.wave_id == wave_id)
        .cloned()
        .collect::<Vec<_>>();
    let rerun_intent = snapshot
        .rerun_intents
        .iter()
        .find(|intent| intent.wave_id == wave_id)
        .cloned();
    let closure_override = snapshot
        .closure_overrides
        .iter()
        .find(|record| record.wave_id == wave_id)
        .cloned();
    let directives = list_control_directives(root, config, Some(wave_id))?;
    let orchestrator_mode =
        latest_orchestrator_session(root, config, wave_id)?.map(|session| match session.mode {
            OrchestratorMode::Operator => "operator".to_string(),
            OrchestratorMode::Autonomous => "autonomous".to_string(),
        });
    let report = ControlShowReport {
        wave,
        portfolio_focus,
        design_detail,
        latest_run,
        acceptance_package,
        operator_objects,
        rerun_intent,
        closure_override,
        orchestrator_mode,
        directives,
    };
    if json {
        print_json(&report)
    } else {
        println!("wave {} {}", report.wave.id, report.wave.title);
        println!("ready: {}", report.wave.ready);
        println!("rerun requested: {}", report.wave.rerun_requested);
        println!(
            "manual close: {}",
            if report.wave.closure_override_applied {
                "applied"
            } else {
                "none"
            }
        );
        println!("completed: {}", report.wave.completed);
        println!(
            "last run: {}",
            report
                .wave
                .last_run_status
                .map(|status| status.to_string())
                .unwrap_or_else(|| "none".to_string())
        );
        println!(
            "blocked by: {}",
            if report.wave.blocked_by.is_empty() {
                "none".to_string()
            } else {
                report.wave.blocked_by.join(", ")
            }
        );
        println!(
            "missing closure: {}",
            format_string_list(&report.wave.missing_closure_agents)
        );
        if let Some(portfolio_focus) = report.portfolio_focus.as_ref() {
            for line in portfolio_focus_lines(portfolio_focus) {
                println!("{line}");
            }
        }
        if let Some(package) = report.acceptance_package.as_ref() {
            for line in acceptance_package_lines(package) {
                println!("{line}");
            }
        }
        if let Some(design) = report.design_detail {
            for line in control_show_design_lines(&design) {
                println!("{line}");
            }
        }
        if let Some(run) = report.latest_run {
            println!("latest run: {}", run.run_id);
            println!("run status: {}", run.status);
            println!(
                "current agent: {}",
                run.current_agent_id
                    .clone()
                    .zip(run.current_agent_title.clone())
                    .map(|(id, title)| format!("{id} {title}"))
                    .unwrap_or_else(|| "none".to_string())
            );
            println!(
                "proof: {}/{} complete={}",
                run.proof.completed_agents, run.proof.total_agents, run.proof.complete
            );
            println!("proof source: {}", run.proof.proof_source);
            println!("replay ok: {}", run.replay.ok);
            println!(
                "last activity: {}",
                run.last_activity_at_ms
                    .map(|timestamp| timestamp.to_string())
                    .unwrap_or_else(|| "none".to_string())
            );
            println!(
                "activity source: {}",
                run.activity_source
                    .clone()
                    .unwrap_or_else(|| "none".to_string())
            );
            println!("stalled: {}", run.stalled);
            if let Some(reason) = run.stall_reason.clone() {
                println!("stall reason: {}", reason);
            }
            println!("proof reuse: {}", proof_reuse_summary(&run));
            for line in control_runtime_lines(&run) {
                println!("{line}");
            }
            println!(
                "launcher available: {}",
                format_string_list(&snapshot.launcher.available_runtimes)
            );
            println!(
                "launcher unavailable: {}",
                format_string_list(&snapshot.launcher.unavailable_runtimes)
            );
        }
        if !report.operator_objects.is_empty() {
            for item in &report.operator_objects {
                println!("{}", operator_object_line(item));
                for detail_line in operator_object_detail_lines(item) {
                    println!("{}", detail_line);
                }
            }
        }
        if let Some(intent) = report.rerun_intent {
            println!(
                "rerun intent: {} ({}, scope={})",
                intent.reason,
                intent.requested_by,
                rerun_scope_label(intent.scope)
            );
        }
        if let Some(record) = report.closure_override {
            println!(
                "closure override: {} ({})",
                closure_override_status_label(&record),
                record.reason
            );
            println!("closure override source run: {}", record.source_run_id);
            println!(
                "closure override evidence: {}",
                format_string_list(&record.evidence_paths)
            );
            if let Some(detail) = record.detail {
                println!("closure override detail: {}", detail);
            }
        }
        if let Some(mode) = report.orchestrator_mode {
            println!("orchestrator mode: {mode}");
        }
        if !report.directives.is_empty() {
            for directive in &report.directives {
                println!(
                    "directive {} {} {}",
                    directive.directive_id,
                    format!("{:?}", directive.kind).to_ascii_lowercase(),
                    directive
                        .agent_id
                        .clone()
                        .unwrap_or_else(|| format!("wave-{}", directive.wave_id))
                );
            }
        }
        Ok(())
    }
}

fn render_agent_steer(
    root: &Path,
    config: &ProjectConfig,
    wave: u32,
    agent: &str,
    message: &str,
    json: bool,
) -> Result<()> {
    let report = AgentSteerReport {
        directive: steer_agent(
            root,
            config,
            wave,
            agent,
            message,
            DirectiveOrigin::Operator,
            "wave-cli",
        )?,
    };
    if json {
        print_json(&report)
    } else {
        println!("steered wave {wave} agent {agent}");
        println!("message: {}", message.trim());
        Ok(())
    }
}

fn render_agent_control<F>(
    root: &Path,
    config: &ProjectConfig,
    wave: u32,
    agent: &str,
    json: bool,
    action_label: &str,
    action: F,
) -> Result<()>
where
    F: FnOnce(&Path, &ProjectConfig, u32, &str) -> Result<wave_domain::ControlDirectiveRecord>,
{
    let report = AgentControlReport {
        directive: action(root, config, wave, agent)?,
    };
    if json {
        print_json(&report)
    } else {
        println!("wave {wave} agent {agent} {action_label}");
        Ok(())
    }
}

fn render_orchestrator_show(
    root: &Path,
    config: &ProjectConfig,
    wave: u32,
    json: bool,
) -> Result<()> {
    let snapshot = load_operator_snapshot(root, config)?;
    let report = OrchestratorShowReport {
        session: latest_orchestrator_session(root, config, wave)?,
        wave: snapshot
            .panels
            .orchestrator
            .waves
            .into_iter()
            .find(|candidate| candidate.wave_id == wave),
        directives: snapshot
            .panels
            .orchestrator
            .directives
            .into_iter()
            .filter(|directive| directive.wave_id == wave)
            .collect(),
    };
    if json {
        print_json(&report)
    } else {
        let mode = report
            .session
            .as_ref()
            .map(|session| match session.mode {
                OrchestratorMode::Operator => "operator",
                OrchestratorMode::Autonomous => "autonomous",
            })
            .unwrap_or("operator");
        println!("wave {wave} orchestrator mode: {mode}");
        if let Some(wave) = report.wave.as_ref() {
            println!(
                "execution model: {} | active run: {}",
                wave.execution_model,
                wave.active_run_id.as_deref().unwrap_or("none")
            );
            for agent in &wave.agents {
                println!(
                    "agent {} {} status={} merge={} sandbox={} barrier={} deps={}",
                    agent.id,
                    agent.title,
                    agent.status,
                    agent.merge_state.as_deref().unwrap_or("none"),
                    agent.sandbox_id.as_deref().unwrap_or("none"),
                    agent.barrier_class,
                    if agent.depends_on_agents.is_empty() {
                        "none".to_string()
                    } else {
                        agent.depends_on_agents.join(",")
                    }
                );
                if !agent.barrier_reasons.is_empty() {
                    println!("  barrier reasons: {}", agent.barrier_reasons.join(" | "));
                }
            }
        }
        if !report.directives.is_empty() {
            for directive in &report.directives {
                println!(
                    "directive {} {} {} state={} detail={}",
                    directive.directive_id,
                    directive.kind,
                    directive
                        .agent_id
                        .clone()
                        .unwrap_or_else(|| format!("wave-{}", directive.wave_id)),
                    directive.delivery_state.as_deref().unwrap_or("unknown"),
                    directive.delivery_detail.as_deref().unwrap_or("none"),
                );
            }
        }
        Ok(())
    }
}

fn render_orchestrator_steer(
    root: &Path,
    config: &ProjectConfig,
    wave: u32,
    message: &str,
    json: bool,
) -> Result<()> {
    let report = OrchestratorSteerReport {
        directive: steer_wave(
            root,
            config,
            wave,
            message,
            DirectiveOrigin::Operator,
            "wave-cli",
        )?,
    };
    if json {
        print_json(&report)
    } else {
        println!("steered wave {wave} orchestrator");
        println!("message: {}", message.trim());
        Ok(())
    }
}

fn render_orchestrator_mode(
    root: &Path,
    config: &ProjectConfig,
    wave: u32,
    mode: &str,
    json: bool,
) -> Result<()> {
    let parsed = match mode.trim() {
        "operator" => OrchestratorMode::Operator,
        "autonomous" => OrchestratorMode::Autonomous,
        other => anyhow::bail!("unsupported orchestrator mode `{other}`"),
    };
    let report = OrchestratorModeReport {
        session: Some(set_orchestrator_mode(
            root, config, wave, parsed, "wave-cli",
        )?),
    };
    if json {
        print_json(&report)
    } else {
        println!("wave {wave} orchestrator mode updated");
        Ok(())
    }
}

fn control_show_design_lines(design: &wave_app_server::WaveDesignDetail) -> Vec<String> {
    let mut lines = vec![
        format!("design completeness: {:?}", design.completeness),
        format!(
            "design blockers: {}",
            if design.blocker_reasons.is_empty() {
                "none".to_string()
            } else {
                design.blocker_reasons.join(", ")
            }
        ),
    ];
    if !design.active_contradictions.is_empty() {
        lines.push(format!(
            "active contradictions: {}",
            design
                .active_contradictions
                .iter()
                .map(|contradiction| {
                    format!("{}:{}", contradiction.contradiction_id, contradiction.state)
                })
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for contradiction in &design.active_contradictions {
            lines.push(format!(
                "contradiction {} state={} summary={}",
                contradiction.contradiction_id, contradiction.state, contradiction.summary
            ));
            if !contradiction.invalidated_refs.is_empty() {
                lines.push(format!(
                    "contradiction refs: {}",
                    contradiction.invalidated_refs.join(", ")
                ));
            }
            if let Some(detail) = contradiction.detail.as_deref() {
                lines.push(format!("contradiction detail: {}", detail));
            }
        }
    }
    if !design.unresolved_question_ids.is_empty() {
        lines.push(format!(
            "open questions: {}",
            design.unresolved_question_ids.join(", ")
        ));
    }
    if !design.unresolved_assumption_ids.is_empty() {
        lines.push(format!(
            "open assumptions: {}",
            design.unresolved_assumption_ids.join(", ")
        ));
    }
    if !design.pending_human_inputs.is_empty() {
        lines.push(format!(
            "pending human input: {}",
            design
                .pending_human_inputs
                .iter()
                .map(|request| format!("{} via {}", request.request_id, request.route))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !design.dependency_handshake_routes.is_empty() {
        lines.push(format!(
            "dependency handshakes: {}",
            design.dependency_handshake_routes.join(", ")
        ));
    }
    if !design.invalidated_fact_ids.is_empty() {
        lines.push(format!(
            "invalidated facts: {}",
            design.invalidated_fact_ids.join(", ")
        ));
    }
    if !design.invalidated_decision_ids.is_empty() {
        lines.push(format!(
            "invalidated decisions: {}",
            design.invalidated_decision_ids.join(", ")
        ));
    }
    if !design.invalidation_routes.is_empty() {
        for route in &design.invalidation_routes {
            lines.push(format!("invalidation route: {}", route));
        }
    }
    if !design.selectively_invalidated_task_ids.is_empty() {
        lines.push(format!(
            "selective rerun tasks: {}",
            design.selectively_invalidated_task_ids.join(", ")
        ));
    }
    if !design.superseded_decision_ids.is_empty() {
        lines.push(format!(
            "superseded decisions: {}",
            design.superseded_decision_ids.join(", ")
        ));
    }
    if !design.ambiguous_dependency_wave_ids.is_empty() {
        lines.push(format!(
            "ambiguous dependency waves: {}",
            design
                .ambiguous_dependency_wave_ids
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    lines
}

fn portfolio_focus_report(
    status: &PlanningStatusReadModel,
    wave_id: u32,
    acceptance_packages: &[wave_app_server::AcceptancePackageSnapshot],
) -> Option<PortfolioFocusReport> {
    let delivery = acceptance_packages
        .iter()
        .find(|package| package.wave_id == wave_id)
        .map(portfolio_delivery_summary_report);
    let packages_by_wave = acceptance_packages
        .iter()
        .map(|package| (package.wave_id, package))
        .collect::<HashMap<_, _>>();
    let initiatives = status
        .portfolio
        .initiatives
        .iter()
        .filter(|initiative| initiative.delivery.wave_ids.contains(&wave_id))
        .map(|initiative| {
            portfolio_entry_report(
                initiative.initiative_id.clone(),
                initiative.title.clone(),
                initiative.delivery.wave_ids.clone(),
                initiative.delivery.blocking_reasons.clone(),
                &packages_by_wave,
            )
        })
        .collect::<Vec<_>>();
    let milestones = status
        .portfolio
        .milestones
        .iter()
        .filter(|milestone| milestone.delivery.wave_ids.contains(&wave_id))
        .map(|milestone| {
            portfolio_entry_report(
                milestone.milestone_id.clone(),
                milestone.title.clone(),
                milestone.delivery.wave_ids.clone(),
                milestone.delivery.blocking_reasons.clone(),
                &packages_by_wave,
            )
        })
        .collect::<Vec<_>>();
    let release_trains = status
        .portfolio
        .release_trains
        .iter()
        .filter(|train| train.delivery.wave_ids.contains(&wave_id))
        .map(|train| {
            portfolio_entry_report(
                train.release_train_id.clone(),
                train.title.clone(),
                train.delivery.wave_ids.clone(),
                train.delivery.blocking_reasons.clone(),
                &packages_by_wave,
            )
        })
        .collect::<Vec<_>>();
    let outcome_contracts = status
        .portfolio
        .outcome_contracts
        .iter()
        .filter(|contract| contract.delivery.wave_ids.contains(&wave_id))
        .map(|contract| {
            portfolio_entry_report(
                contract.outcome_contract_id.clone(),
                contract.title.clone(),
                contract.delivery.wave_ids.clone(),
                contract.delivery.blocking_reasons.clone(),
                &packages_by_wave,
            )
        })
        .collect::<Vec<_>>();

    if initiatives.is_empty()
        && milestones.is_empty()
        && release_trains.is_empty()
        && outcome_contracts.is_empty()
        && delivery.is_none()
    {
        None
    } else {
        Some(PortfolioFocusReport {
            delivery,
            initiatives,
            milestones,
            release_trains,
            outcome_contracts,
        })
    }
}

fn portfolio_entry_report(
    id: String,
    title: String,
    wave_ids: Vec<u32>,
    portfolio_blocking_reasons: Vec<String>,
    packages_by_wave: &HashMap<u32, &wave_app_server::AcceptancePackageSnapshot>,
) -> PortfolioEntryReport {
    let mut blocking_reasons = portfolio_blocking_reasons;
    let mut wave_delivery = Vec::new();

    for wave_id in &wave_ids {
        if let Some(package) = packages_by_wave.get(wave_id) {
            wave_delivery.push(PortfolioWaveDeliveryReport {
                wave_id: *wave_id,
                ship_state: acceptance_state_label(package.ship_state),
                release_state: acceptance_state_label(package.release_state),
                signoff_state: acceptance_state_label(package.signoff.state),
            });
            for reason in &package.blocking_reasons {
                let reason = format!("wave {} {}", wave_id, reason);
                if !blocking_reasons.iter().any(|existing| existing == &reason) {
                    blocking_reasons.push(reason);
                }
            }
        } else {
            wave_delivery.push(PortfolioWaveDeliveryReport {
                wave_id: *wave_id,
                ship_state: "missing".to_string(),
                release_state: "missing".to_string(),
                signoff_state: "missing".to_string(),
            });
            let reason = format!("wave {} acceptance package missing", wave_id);
            if !blocking_reasons.iter().any(|existing| existing == &reason) {
                blocking_reasons.push(reason);
            }
        }
    }

    let ship_ready_waves = wave_delivery
        .iter()
        .filter(|wave| wave.ship_state == "ship")
        .count();
    let accepted_waves = wave_delivery
        .iter()
        .filter(|wave| wave.release_state == "accepted")
        .count();
    let signed_off_waves = wave_delivery
        .iter()
        .filter(|wave| wave.signoff_state == "signed_off")
        .count();

    PortfolioEntryReport {
        id,
        title,
        wave_ids,
        ship_ready_waves,
        accepted_waves,
        signed_off_waves,
        wave_delivery,
        blocking_reasons,
    }
}

fn portfolio_delivery_summary_report(
    package: &wave_app_server::AcceptancePackageSnapshot,
) -> PortfolioDeliverySummaryReport {
    PortfolioDeliverySummaryReport {
        ship_state: acceptance_state_label(package.ship_state),
        release_state: acceptance_state_label(package.release_state),
        signoff_state: acceptance_state_label(package.signoff.state),
        summary: package.summary.clone(),
        proof_complete: package.implementation.proof_complete,
        completed_agents: package.implementation.completed_agents,
        total_agents: package.implementation.total_agents,
        proof_source: package
            .implementation
            .proof_source
            .clone()
            .unwrap_or_else(|| "none".to_string()),
        known_risk_count: package.known_risks.len(),
        outstanding_debt_count: package.outstanding_debt.len(),
        blocking_reasons: package.blocking_reasons.clone(),
    }
}

fn portfolio_focus_lines(focus: &PortfolioFocusReport) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(delivery) = focus.delivery.as_ref() {
        lines.push(format!(
            "portfolio delivery: ship={} release={} signoff={} proof={}/{} complete={} source={} risks={} debt={}",
            delivery.ship_state,
            delivery.release_state,
            delivery.signoff_state,
            delivery.completed_agents,
            delivery.total_agents,
            delivery.proof_complete,
            delivery.proof_source,
            delivery.known_risk_count,
            delivery.outstanding_debt_count
        ));
        lines.push(format!("portfolio delivery summary: {}", delivery.summary));
        if !delivery.blocking_reasons.is_empty() {
            lines.push(format!(
                "portfolio delivery blockers: {}",
                delivery.blocking_reasons.join(" | ")
            ));
        }
    }
    for entry in &focus.initiatives {
        lines.push(portfolio_entry_line("portfolio initiative", entry));
        lines.extend(portfolio_entry_detail_lines(entry));
    }
    for entry in &focus.milestones {
        lines.push(portfolio_entry_line("portfolio milestone", entry));
        lines.extend(portfolio_entry_detail_lines(entry));
    }
    for entry in &focus.release_trains {
        lines.push(portfolio_entry_line("portfolio release train", entry));
        lines.extend(portfolio_entry_detail_lines(entry));
    }
    for entry in &focus.outcome_contracts {
        lines.push(portfolio_entry_line("portfolio outcome contract", entry));
        lines.extend(portfolio_entry_detail_lines(entry));
    }
    lines
}

fn portfolio_entry_line(prefix: &str, entry: &PortfolioEntryReport) -> String {
    format!(
        "{prefix}: {} ship={}/{} release={}/{} signoff={}/{} waves={}",
        entry.title,
        entry.ship_ready_waves,
        entry.wave_ids.len(),
        entry.accepted_waves,
        entry.wave_ids.len(),
        entry.signed_off_waves,
        entry.wave_ids.len(),
        entry
            .wave_delivery
            .iter()
            .map(|wave| format!(
                "{}:{}/{}/{}",
                wave.wave_id, wave.ship_state, wave.release_state, wave.signoff_state
            ))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn portfolio_entry_detail_lines(entry: &PortfolioEntryReport) -> Vec<String> {
    if entry.blocking_reasons.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "portfolio blockers: {}",
            entry.blocking_reasons.join(" | ")
        )]
    }
}

fn acceptance_package_lines(package: &wave_app_server::AcceptancePackageSnapshot) -> Vec<String> {
    let mut lines = vec![
        format!("ship state: {}", acceptance_state_label(package.ship_state)),
        format!(
            "release state: {}",
            acceptance_state_label(package.release_state)
        ),
        format!(
            "signoff state: {}",
            acceptance_state_label(package.signoff.state)
        ),
        format!("acceptance summary: {}", package.summary),
        format!(
            "acceptance design: completeness={:?} blockers={} contradictions={} questions={} assumptions={} human_input={} ambiguous_dependencies={}",
            package.design_intent.completeness,
            package.design_intent.blocker_count,
            package.design_intent.contradiction_count,
            package.design_intent.unresolved_question_count,
            package.design_intent.unresolved_assumption_count,
            package.design_intent.pending_human_input_count,
            package.design_intent.ambiguous_dependency_count
        ),
        format!(
            "acceptance implementation: proof_complete={} proof={}/{} replay_ok={} source={}",
            package.implementation.proof_complete,
            package.implementation.completed_agents,
            package.implementation.total_agents,
            package
                .implementation
                .replay_ok
                .map(|ok| ok.to_string())
                .unwrap_or_else(|| "none".to_string()),
            package
                .implementation
                .proof_source
                .clone()
                .unwrap_or_else(|| "none".to_string())
        ),
        format!(
            "acceptance signoff: complete={} manual_close={} completed={} pending={} operator_actions={}",
            package.signoff.complete,
            package.signoff.manual_close_applied,
            format_string_list(&package.signoff.completed_closure_agents),
            format_string_list(&package.signoff.pending_closure_agents),
            format_string_list(&package.signoff.pending_operator_actions)
        ),
        format!(
            "acceptance closure gates: {}",
            closure_gate_status_summary(&package.signoff.closure_agents)
        ),
    ];
    lines.push(format!(
        "acceptance release: promotion={} merge_blocked={} closure_blocked={}",
        package
            .release
            .promotion_state
            .map(acceptance_state_label)
            .unwrap_or_else(|| "none".to_string()),
        package.release.merge_blocked,
        package.release.closure_blocked
    ));
    if let Some(decision) = package.release.last_decision.as_deref() {
        lines.push(format!("acceptance release detail: {}", decision));
    }
    if !package.blocking_reasons.is_empty() {
        lines.push(format!(
            "ship blockers: {}",
            package.blocking_reasons.join(" | ")
        ));
    }
    if !package.known_risks.is_empty() {
        lines.push(format!("known risks: {}", package.known_risks.len()));
        for item in &package.known_risks {
            lines.push(format!("risk {}: {}", item.code, item.summary));
            if let Some(detail) = item.detail.as_deref() {
                lines.push(format!("risk detail: {}", detail));
            }
        }
    }
    if !package.outstanding_debt.is_empty() {
        lines.push(format!(
            "outstanding debt: {}",
            package.outstanding_debt.len()
        ));
        for item in &package.outstanding_debt {
            lines.push(format!("debt {}: {}", item.code, item.summary));
            if let Some(detail) = item.detail.as_deref() {
                lines.push(format!("debt detail: {}", detail));
            }
        }
    }
    for agent in package
        .signoff
        .closure_agents
        .iter()
        .filter(|agent| agent.error.is_some())
    {
        lines.push(format!(
            "acceptance closure error: {} {}",
            agent.agent_id,
            agent.error.as_deref().unwrap_or_default()
        ));
    }
    lines
}

fn closure_gate_status_summary(
    agents: &[wave_app_server::AcceptanceClosureAgentSnapshot],
) -> String {
    if agents.is_empty() {
        "none".to_string()
    } else {
        agents
            .iter()
            .map(|agent| {
                let status = agent
                    .status
                    .map(acceptance_state_label)
                    .unwrap_or_else(|| "not_started".to_string());
                let proof = if agent.proof_complete {
                    "proof"
                } else {
                    "no-proof"
                };
                format!("{}={}/{proof}", agent.agent_id, status)
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn acceptance_state_label(value: impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    let mut label = String::new();
    for (index, ch) in debug.chars().enumerate() {
        if ch.is_uppercase() && index > 0 {
            label.push('_');
        }
        for lower in ch.to_lowercase() {
            label.push(lower);
        }
    }
    label
}

fn operator_object_label(kind: wave_app_server::OperatorActionableKind) -> &'static str {
    match kind {
        wave_app_server::OperatorActionableKind::Approval => "approval-request",
        wave_app_server::OperatorActionableKind::Proposal => "head-proposal",
        wave_app_server::OperatorActionableKind::Override => "manual-close-override",
        wave_app_server::OperatorActionableKind::Escalation => "escalation",
    }
}

fn operator_object_line(item: &wave_app_server::OperatorActionableItem) -> String {
    format!(
        "{} {} state={} summary={}",
        operator_object_label(item.kind),
        item.record_id,
        item.state,
        item.summary
    )
}

fn operator_object_detail_lines(item: &wave_app_server::OperatorActionableItem) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(route) = item.route.as_deref() {
        lines.push(format!(
            "{} route={route}",
            operator_object_label(item.kind)
        ));
    }
    if let Some(task_id) = item.task_id.as_deref() {
        lines.push(format!(
            "{} task={task_id}",
            operator_object_label(item.kind)
        ));
    }
    if let Some(source_run_id) = item.source_run_id.as_deref() {
        lines.push(format!(
            "{} source_run={source_run_id}",
            operator_object_label(item.kind)
        ));
    }
    if item.evidence_count > 0 {
        lines.push(format!(
            "{} evidence={}",
            operator_object_label(item.kind),
            item.evidence_count
        ));
    }
    if let Some(waiting_on) = item.waiting_on.as_deref() {
        lines.push(format!(
            "{} waiting_on={waiting_on}",
            operator_object_label(item.kind)
        ));
    }
    if let Some(next_action) = item.next_action.as_deref() {
        lines.push(format!(
            "{} next_action={next_action}",
            operator_object_label(item.kind)
        ));
    }
    if let Some(detail) = item.detail.as_deref() {
        lines.push(format!(
            "{} detail={detail}",
            operator_object_label(item.kind)
        ));
    }
    lines
}

fn control_runtime_lines(run: &ActiveRunDetail) -> Vec<String> {
    if run_has_mixed_runtime_selection(run) {
        let mut lines = vec![format!(
            "run runtimes: {}",
            format_string_list(&run.runtime_summary.selected_runtimes)
        )];
        if !run.runtime_summary.requested_runtimes.is_empty() {
            lines.push(format!(
                "requested runtimes: {}",
                format_string_list(&run.runtime_summary.requested_runtimes)
            ));
        }
        if !run.runtime_summary.selection_sources.is_empty() {
            lines.push(format!(
                "selection sources: {}",
                format_string_list(&run.runtime_summary.selection_sources)
            ));
        }
        if run.runtime_summary.fallback_count > 0
            || !run.runtime_summary.fallback_targets.is_empty()
        {
            lines.push(format!(
                "fallbacks: {} target={}",
                run.runtime_summary.fallback_count,
                format_string_list(&run.runtime_summary.fallback_targets)
            ));
        }
        if let Some(runtime) = current_agent_runtime_detail(run) {
            lines.push(format!(
                "current agent runtime: {}",
                runtime_decision_summary(runtime)
            ));
            if let Some(fallback) = runtime.fallback.as_ref() {
                lines.push(format!(
                    "current agent fallback reason: {}",
                    fallback.reason
                ));
            }
            lines.push(format!(
                "current agent adapter: {} ({})",
                runtime.execution_identity.adapter, runtime.execution_identity.provider
            ));
        }
        lines
    } else {
        representative_runtime_detail(run)
            .map(|runtime| {
                let mut lines = vec![format!(
                    "runtime decision: {}",
                    runtime_decision_summary(runtime)
                )];
                if let Some(fallback) = runtime.fallback.as_ref() {
                    lines.push(format!("fallback reason: {}", fallback.reason));
                }
                lines.push(format!(
                    "adapter: {} ({})",
                    runtime.execution_identity.adapter, runtime.execution_identity.provider
                ));
                lines
            })
            .unwrap_or_default()
    }
}

fn run_has_mixed_runtime_selection(run: &ActiveRunDetail) -> bool {
    run.runtime_summary.selected_runtimes.len() > 1
}

fn current_agent_runtime_detail(run: &ActiveRunDetail) -> Option<&wave_app_server::RuntimeDetail> {
    run.current_agent_id.as_deref().and_then(|agent_id| {
        run.agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .and_then(|agent| agent.runtime.as_ref())
    })
}

fn representative_runtime_detail(run: &ActiveRunDetail) -> Option<&wave_app_server::RuntimeDetail> {
    current_agent_runtime_detail(run)
        .or_else(|| {
            run.agents
                .iter()
                .filter_map(|agent| agent.runtime.as_ref())
                .find(|runtime| runtime.fallback.is_some())
        })
        .or_else(|| {
            run.agents
                .iter()
                .filter_map(|agent| agent.runtime.as_ref())
                .next()
        })
}

fn runtime_decision_summary(runtime: &wave_app_server::RuntimeDetail) -> String {
    let requested = runtime
        .policy
        .requested_runtime
        .as_deref()
        .unwrap_or("unspecified");
    let source = runtime
        .policy
        .selection_source
        .as_deref()
        .unwrap_or("runtime policy");
    format!(
        "requested {} -> selected {} via {}",
        requested, runtime.selected_runtime, source
    )
}

fn design_detail_for_wave<'a>(
    snapshot: &'a OperatorSnapshot,
    wave_id: u32,
) -> Option<&'a wave_app_server::WaveDesignDetail> {
    snapshot
        .design_details
        .iter()
        .find(|detail| detail.wave_id == wave_id)
}

fn proof_reuse_summary(run: &ActiveRunDetail) -> String {
    let reused = run
        .agents
        .iter()
        .filter(|agent| agent.reused_from_prior_run)
        .count();
    let fresh = run.agents.len().saturating_sub(reused);
    format!("{reused} reused, {fresh} rerun")
}

fn render_task_list(
    root: &Path,
    config: &ProjectConfig,
    wave_id: Option<u32>,
    json: bool,
) -> Result<()> {
    let snapshot = load_operator_snapshot(root, config)?;
    let wave_id = select_wave_id(&snapshot, wave_id)?;
    let report = snapshot
        .active_run_details
        .iter()
        .find(|run| run.wave_id == wave_id)
        .map(|run| TaskListReport {
            wave_id,
            run_id: Some(run.run_id.clone()),
            agents: run.agents.clone(),
        })
        .unwrap_or(TaskListReport {
            wave_id,
            run_id: None,
            agents: Vec::new(),
        });

    if json {
        print_json(&report)
    } else if report.agents.is_empty() {
        println!("wave {} has no active tasks", report.wave_id);
        Ok(())
    } else {
        println!(
            "wave {} tasks (run {})",
            report.wave_id,
            report.run_id.unwrap_or_else(|| "none".to_string())
        );
        for agent in report.agents {
            println!(
                "- {} {} | state={} | reuse={} | proof={} | deliverables={}",
                agent.id,
                agent.title,
                agent.status,
                if agent.reused_from_prior_run {
                    "reused"
                } else {
                    "fresh"
                },
                if agent.proof_complete {
                    "complete".to_string()
                } else if agent.missing_markers.is_empty() {
                    "pending".to_string()
                } else {
                    format!("missing {}", agent.missing_markers.join(", "))
                },
                if agent.deliverables.is_empty() {
                    "none".to_string()
                } else {
                    agent.deliverables.join(", ")
                }
            );
        }
        Ok(())
    }
}

fn render_rerun_list(root: &Path, config: &ProjectConfig, json: bool) -> Result<()> {
    let snapshot = load_operator_snapshot(root, config)?;
    if json {
        print_json(&snapshot.rerun_intents)
    } else if snapshot.rerun_intents.is_empty() {
        println!("reruns: none");
        Ok(())
    } else {
        for intent in snapshot.rerun_intents {
            println!(
                "- wave {} | status={:?} | scope={} | requested_by={} | reason={}",
                intent.wave_id,
                intent.status,
                rerun_scope_label(intent.scope),
                intent.requested_by,
                intent.reason
            );
        }
        Ok(())
    }
}

fn render_rerun_request(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
    reason: &str,
    scope: &str,
    json: bool,
) -> Result<()> {
    let record = request_rerun(root, config, wave_id, reason, parse_rerun_scope(scope)?)?;
    if json {
        print_json(&record)
    } else {
        println!("requested rerun for wave {}", wave_id);
        println!("reason: {}", record.reason);
        println!("scope: {}", rerun_scope_label(record.scope));
        Ok(())
    }
}

fn render_rerun_clear(root: &Path, config: &ProjectConfig, wave_id: u32, json: bool) -> Result<()> {
    let record = clear_rerun(root, config, wave_id)?;
    if json {
        print_json(&record)
    } else {
        match record {
            Some(record) => {
                println!("cleared rerun for wave {}", record.wave_id);
                Ok(())
            }
            None => {
                println!("no rerun intent recorded for wave {}", wave_id);
                Ok(())
            }
        }
    }
}

fn render_control_repair(root: &Path, config: &ProjectConfig, json: bool) -> Result<()> {
    let repaired = repair_orphaned_runs(root, config)?;
    let report = ControlRepairReport {
        repaired_runs: repaired
            .into_iter()
            .map(|run| RepairRunSurface {
                wave_id: run.wave_id,
                run_id: run.run_id,
                status: run.status,
                error: run.error,
            })
            .collect(),
    };
    if json {
        print_json(&report)
    } else if report.repaired_runs.is_empty() {
        println!("control repair: no orphaned runs found");
        Ok(())
    } else {
        println!(
            "control repair: reconciled {} orphaned run(s)",
            report.repaired_runs.len()
        );
        for run in report.repaired_runs {
            println!(
                "- wave {} | run id={} | status={} | error={}",
                run.wave_id,
                run.run_id,
                run.status,
                run.error.unwrap_or_else(|| "none".to_string())
            );
        }
        Ok(())
    }
}

fn render_close_apply(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
    reason: &str,
    source_run: Option<&str>,
    evidence_paths: Vec<String>,
    detail: Option<String>,
    json: bool,
) -> Result<()> {
    let record = apply_closure_override(
        root,
        config,
        wave_id,
        reason,
        source_run,
        evidence_paths,
        detail,
    )?;
    if json {
        print_json(&record)
    } else {
        println!("applied closure override for wave {}", record.wave_id);
        println!("status: {}", closure_override_status_label(&record));
        println!("reason: {}", record.reason);
        println!("source run: {}", record.source_run_id);
        println!("evidence: {}", format_string_list(&record.evidence_paths));
        Ok(())
    }
}

fn render_close_clear(root: &Path, config: &ProjectConfig, wave_id: u32, json: bool) -> Result<()> {
    let record = clear_closure_override(root, config, wave_id)?;
    if json {
        print_json(&record)
    } else {
        match record {
            Some(record) => {
                println!("cleared closure override for wave {}", record.wave_id);
                Ok(())
            }
            None => {
                println!("no closure override recorded for wave {}", wave_id);
                Ok(())
            }
        }
    }
}

fn render_proof_show(
    root: &Path,
    config: &ProjectConfig,
    wave_id: Option<u32>,
    json: bool,
) -> Result<()> {
    let snapshot = load_operator_snapshot(root, config)?;
    let wave_id = select_wave_id(&snapshot, wave_id)?;
    let waves = load_wave_documents(config, root)?;
    let relevant_runs = load_relevant_run_records(root, config)?;
    let report = proof_report_for_wave(
        root,
        config,
        &waves,
        &snapshot.planning,
        &snapshot.acceptance_packages,
        &snapshot.latest_run_details,
        &relevant_runs,
        wave_id,
    );

    if json {
        print_json(&report)
    } else if let Some(proof) = report.proof {
        if let Some(portfolio_focus) = report.portfolio_focus.as_ref() {
            for line in portfolio_focus_lines(portfolio_focus) {
                println!("{line}");
            }
        }
        if let Some(package) = report.acceptance_package.as_ref() {
            for line in acceptance_package_lines(package) {
                println!("{line}");
            }
        }
        println!(
            "wave {} proof {}/{} complete={}",
            wave_id, proof.completed_agents, proof.total_agents, proof.complete
        );
        println!("proof source: {}", proof.proof_source);
        println!(
            "result authority: structured={} compatibility={}",
            proof.envelope_backed_agents, proof.compatibility_backed_agents
        );
        for artifact in proof.declared_artifacts {
            println!(
                "- {} {}",
                artifact.path,
                if artifact.exists {
                    "present"
                } else {
                    "missing"
                }
            );
        }
        if let Some(replay) = report.replay {
            println!("replay ok: {}", replay.ok);
            for issue in replay.issues {
                println!("replay issue: {} ({})", issue.kind, issue.detail);
            }
        }
        if let Some(run) = report.run {
            for agent in run.agents {
                println!(
                    "- {} {} | state={} | reuse={} | source={} | proof={}",
                    agent.id,
                    agent.title,
                    agent.status,
                    if agent.reused_from_prior_run {
                        "reused"
                    } else {
                        "fresh"
                    },
                    agent.proof_source,
                    if agent.proof_complete {
                        "complete".to_string()
                    } else if agent.missing_markers.is_empty() {
                        "pending".to_string()
                    } else {
                        format!("missing {}", agent.missing_markers.join(", "))
                    }
                );
            }
        }
        Ok(())
    } else {
        if let Some(portfolio_focus) = report.portfolio_focus.as_ref() {
            for line in portfolio_focus_lines(portfolio_focus) {
                println!("{line}");
            }
        }
        if let Some(package) = report.acceptance_package.as_ref() {
            for line in acceptance_package_lines(package) {
                println!("{line}");
            }
        }
        println!("wave {} has no recorded proof snapshot", wave_id);
        Ok(())
    }
}

fn render_proof_seed_design_authority(
    root: &Path,
    config: &ProjectConfig,
    wave_id: u32,
    json: bool,
) -> Result<()> {
    let report = seed_design_authority_live_proof(root, config, wave_id)?;
    if json {
        print_json(&report)
    } else {
        if report.already_present {
            println!(
                "design-authority proof already present for wave {}",
                report.wave_id
            );
        } else {
            println!("seeded design-authority proof for wave {}", report.wave_id);
        }
        println!("event log: {}", report.event_log_path.display());
        println!("correlation id: {}", report.correlation_id);
        println!("events: {}", report.event_ids.join(", "));
        Ok(())
    }
}

fn proof_report_for_wave(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    planning: &PlanningStatusReadModel,
    acceptance_packages: &[wave_app_server::AcceptancePackageSnapshot],
    latest_run_details: &[ActiveRunDetail],
    latest_runs: &HashMap<u32, WaveRunRecord>,
    wave_id: u32,
) -> ProofReport {
    let run_detail = latest_run_details
        .iter()
        .find(|run| run.wave_id == wave_id)
        .cloned()
        .or_else(|| latest_relevant_run_detail(root, config, waves, latest_runs, wave_id));
    let acceptance_package = acceptance_packages
        .iter()
        .find(|package| package.wave_id == wave_id)
        .cloned();
    let portfolio_focus = portfolio_focus_report(planning, wave_id, acceptance_packages);

    run_detail
        .map(|run| ProofReport {
            wave_id,
            run_id: Some(run.run_id.clone()),
            proof: Some(run.proof.clone()),
            portfolio_focus: portfolio_focus.clone(),
            acceptance_package: acceptance_package.clone(),
            replay: Some(run.replay.clone()),
            run: Some(run),
        })
        .unwrap_or(ProofReport {
            wave_id,
            run_id: None,
            run: None,
            proof: None,
            portfolio_focus,
            acceptance_package,
            replay: None,
        })
}

fn render_launch(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    status: &PlanningStatusReadModel,
    options: LaunchOptions,
    json: bool,
) -> Result<()> {
    match launch_wave(root, config, waves, status, options) {
        Ok(report) => {
            if json {
                print_json(&report)
            } else {
                println!("launched wave {}", report.wave_id);
                println!("run id: {}", report.run_id);
                println!("status: {}", report.status);
                println!("state: {}", report.state_path.display());
                println!("trace: {}", report.trace_path.display());
                println!("bundle: {}", report.bundle_dir.display());
                println!("preflight: {}", report.preflight_path.display());
                Ok(())
            }
        }
        Err(error) => {
            if let Some(preflight) = launch_preflight_report(&error) {
                return render_launch_preflight_failure(preflight, json);
            }
            Err(error)
        }
    }
}

fn render_autonomous(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    status: PlanningStatusReadModel,
    options: AutonomousOptions,
    json: bool,
) -> Result<()> {
    let reports = autonomous_launch(root, config, waves, status, options)?;
    if json {
        print_json(&reports)
    } else if reports.is_empty() {
        println!("autonomous: no waves launched");
        Ok(())
    } else {
        println!("autonomous launched {} wave(s)", reports.len());
        for report in reports {
            println!(
                "- wave {} | run id={} | status={} | state={}",
                report.wave_id,
                report.run_id,
                report.status,
                report.state_path.display()
            );
        }
        Ok(())
    }
}

fn launch_preflight_report(error: &anyhow::Error) -> Option<&LaunchPreflightReport> {
    error.chain().find_map(|cause| {
        cause
            .downcast_ref::<LaunchPreflightError>()
            .map(|error| error.report())
    })
}

fn render_launch_preflight_failure(report: &LaunchPreflightReport, json: bool) -> Result<()> {
    if json {
        print_json(report)
    } else {
        println!(
            "launch refused for wave {} ({})",
            report.wave_id, report.wave_slug
        );
        for diagnostic in &report.diagnostics {
            if diagnostic.ok {
                continue;
            }
            println!("- {}: {}", diagnostic.contract, diagnostic.detail);
        }
        if let Some(refusal) = &report.refusal {
            println!("{}", refusal.detail);
        }
        Ok(())
    }
}

fn render_trace_latest(
    latest_runs: &HashMap<u32, WaveRunRecord>,
    wave_id: Option<u32>,
    json: bool,
) -> Result<()> {
    if let Some(wave_id) = wave_id {
        let Some(record) = latest_runs.get(&wave_id) else {
            println!("no trace found for wave {}", wave_id);
            return Ok(());
        };
        let report = dogfood_evidence_report(record);
        let evidence_source = load_trace_bundle(&report.trace_path)
            .ok()
            .flatten()
            .map(|bundle| {
                if bundle.self_host_evidence.is_some() {
                    "stored trace evidence"
                } else {
                    "stored trace bundle"
                }
            })
            .unwrap_or("live run record");
        if json {
            return print_json(&report);
        }
        println!("wave {} latest trace", wave_id);
        println!("run id: {}", report.run_id);
        println!("trace path: {}", report.trace_path.display());
        println!("evidence source: {}", evidence_source);
        println!("recorded: {}", report.recorded);
        println!("replay ok: {}", report.replay.ok);
        println!("operator help required: {}", report.operator_help_required);
        if let Some(worktree) = &report.worktree {
            println!("worktree: {}", worktree.path);
            println!("worktree state: {:?}", worktree.state);
        }
        if let Some(promotion) = &report.promotion {
            println!("promotion state: {:?}", promotion.state);
            if !promotion.conflict_paths.is_empty() {
                println!(
                    "promotion conflicts: {}",
                    promotion.conflict_paths.join(", ")
                );
            }
        }
        if let Some(scheduling) = &report.scheduling {
            println!("scheduler phase: {:?}", scheduling.phase);
            println!("scheduler state: {:?}", scheduling.state);
        }
        for item in report.help_items {
            println!(
                "- {}: {} ({})",
                item.name,
                if item.ok { "ok" } else { "help-needed" },
                item.detail
            );
        }
        println!("status: {}", record.status);
        println!("agent count: {}", record.agents.len());
        return Ok(());
    }

    if json {
        return print_json(&latest_trace_reports_from_runs(latest_runs));
    }

    if latest_runs.is_empty() {
        println!("trace: no runs recorded");
        return Ok(());
    }

    let mut records = latest_runs.values().collect::<Vec<_>>();
    records.sort_by_key(|record| record.wave_id);
    for record in records {
        let report = dogfood_evidence_report(record);
        println!(
            "- wave {} | run id={} | recorded={} | replay={} | help_required={} | trace={}",
            report.wave_id,
            report.run_id,
            report.recorded,
            report.replay.ok,
            report.operator_help_required,
            report.trace_path.display()
        );
    }
    Ok(())
}

fn render_trace_replay(
    latest_runs: &HashMap<u32, WaveRunRecord>,
    wave_id: Option<u32>,
    json: bool,
) -> Result<()> {
    if let Some(wave_id) = wave_id {
        let Some(record) = latest_runs.get(&wave_id) else {
            println!("no trace found for wave {}", wave_id);
            return Ok(());
        };
        let report = trace_inspection_report(record).replay;
        if json {
            return print_json(&report);
        }
        println!("wave {} replay ok={}", wave_id, report.ok);
        for issue in report.issues {
            println!("- {} ({})", issue.kind, issue.detail);
        }
        return Ok(());
    }

    let mut reports = latest_runs
        .values()
        .map(trace_inspection_report)
        .collect::<Vec<_>>();
    reports.sort_by_key(|report| report.wave_id);
    if json {
        return print_json(
            &reports
                .into_iter()
                .map(|report| report.replay)
                .collect::<Vec<_>>(),
        );
    }
    if reports.is_empty() {
        println!("trace replay: no runs recorded");
        return Ok(());
    }
    for report in reports {
        println!(
            "- wave {} | run id={} | ok={} | issues={} | trace={}",
            report.wave_id,
            report.run_id,
            report.replay.ok,
            report.replay.issues.len(),
            report.trace_path.display()
        );
    }
    Ok(())
}

fn latest_trace_reports_from_runs(
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> HashMap<u32, DogfoodEvidenceReport> {
    latest_runs
        .values()
        .map(dogfood_evidence_report)
        .map(|report| (report.wave_id, report))
        .collect()
}

fn render_not_ready(command: &str, note: &str) -> Result<()> {
    println!("{command}: not implemented");
    println!("{note}");
    Ok(())
}

fn parse_rerun_scope(raw: &str) -> Result<RerunScope> {
    match raw {
        "full" => Ok(RerunScope::Full),
        "from-first-incomplete" => Ok(RerunScope::FromFirstIncomplete),
        "closure-only" => Ok(RerunScope::ClosureOnly),
        "promotion-only" => Ok(RerunScope::PromotionOnly),
        _ => anyhow::bail!(
            "unknown rerun scope {raw}; expected full, from-first-incomplete, closure-only, or promotion-only"
        ),
    }
}

fn rerun_scope_label(scope: RerunScope) -> &'static str {
    match scope {
        RerunScope::Full => "full",
        RerunScope::FromFirstIncomplete => "from-first-incomplete",
        RerunScope::ClosureOnly => "closure-only",
        RerunScope::PromotionOnly => "promotion-only",
    }
}

fn closure_override_status_label(record: &WaveClosureOverrideRecord) -> &'static str {
    if record.is_active() {
        "applied"
    } else {
        "cleared"
    }
}

fn select_wave_id(snapshot: &OperatorSnapshot, requested: Option<u32>) -> Result<u32> {
    if let Some(wave_id) = requested {
        return Ok(wave_id);
    }
    if let Some(run) = snapshot.active_run_details.first() {
        return Ok(run.wave_id);
    }
    if let Some(run) = snapshot.latest_run_details.first() {
        return Ok(run.wave_id);
    }
    if let Some(wave_id) = snapshot.dashboard.next_ready_wave_ids.first().copied() {
        return Ok(wave_id);
    }
    if let Some(wave) = snapshot.planning.waves.first() {
        return Ok(wave.id);
    }
    anyhow::bail!("no waves are available")
}

fn format_string_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn format_u32_list(values: &[u32]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn materialized_path_surface(path: PathBuf) -> MaterializedPathSurface {
    let exists = path.exists();
    MaterializedPathSurface { path, exists }
}

fn format_materialized_path(path: &MaterializedPathSurface) -> String {
    format!(
        "{} [{}]",
        path.path.display(),
        if path.exists { "present" } else { "missing" }
    )
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let rendered = serde_json::to_string_pretty(value)?;
    println!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::collections::HashMap;
    use wave_control_plane::PlanningProjectionBundle;
    use wave_control_plane::PlanningStatusReadModel;
    use wave_control_plane::PlanningStatusSummary;
    use wave_control_plane::QueueReadinessReadModel;
    use wave_control_plane::QueueReadinessStateReadModel;
    use wave_control_plane::SkillCatalogHealth;
    use wave_control_plane::WaveReadinessReadModel;
    use wave_control_plane::WaveStatusReadModel;
    use wave_control_plane::build_operator_snapshot_inputs;
    use wave_control_plane::build_planning_status_projection;
    use wave_spec::CompletionLevel;
    use wave_spec::Context7Defaults;
    use wave_spec::DocImpact;
    use wave_spec::DurabilityLevel;
    use wave_spec::ExitContract;
    use wave_spec::ProofLevel;
    use wave_spec::WaveAgent;
    use wave_spec::WaveDocument;
    use wave_spec::WaveMetadata;

    fn empty_ownership() -> wave_control_plane::WaveOwnershipState {
        wave_control_plane::WaveOwnershipState {
            claim: None,
            active_leases: Vec::new(),
            stale_leases: Vec::new(),
            contention_reasons: Vec::new(),
            blocked_by_owner: None,
            budget: wave_control_plane::SchedulerBudgetState {
                max_active_wave_claims: None,
                max_active_task_leases: None,
                reserved_closure_task_leases: None,
                active_wave_claims: 0,
                active_task_leases: 0,
                active_implementation_task_leases: 0,
                active_closure_task_leases: 0,
                closure_capacity_reserved: false,
                preemption_enabled: false,
                budget_blocked: false,
            },
        }
    }

    fn empty_execution() -> wave_control_plane::WaveExecutionState {
        wave_control_plane::WaveExecutionState {
            worktree: None,
            promotion: None,
            scheduling: None,
            merge_blocked: false,
            closure_blocked_by_promotion: false,
        }
    }

    fn empty_recovery() -> wave_control_plane::WaveRecoveryState {
        wave_control_plane::WaveRecoveryState::default()
    }

    fn default_delivery() -> wave_control_plane::DeliveryReadModel {
        wave_control_plane::DeliveryReadModel::default()
    }

    fn default_wave_metadata() -> WaveMetadata {
        WaveMetadata {
            id: 0,
            slug: String::new(),
            title: String::new(),
            mode: wave_config::ExecutionMode::DarkFactory,
            execution_model: wave_spec::WaveExecutionModel::Serial,
            concurrency_budget: wave_spec::WaveConcurrencyBudget::default(),
            owners: Vec::new(),
            depends_on: Vec::new(),
            validation: Vec::new(),
            rollback: Vec::new(),
            proof: Vec::new(),
            wave_class: wave_spec::WaveClass::Implementation,
            intent: None,
            delivery: None,
            design_gate: None,
        }
    }

    fn empty_planning_status() -> PlanningStatusReadModel {
        PlanningStatusReadModel {
            project_name: "Test".to_string(),
            default_mode: wave_config::ExecutionMode::DarkFactory,
            summary: PlanningStatusSummary {
                total_waves: 0,
                ready_waves: 0,
                blocked_waves: 0,
                active_waves: 0,
                completed_waves: 0,
                design_incomplete_waves: 0,
                total_agents: 0,
                implementation_agents: 0,
                closure_agents: 0,
                waves_with_complete_closure: 0,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
            delivery: default_delivery().summary.clone(),
            portfolio: Default::default(),
            skill_catalog: SkillCatalogHealth {
                ok: true,
                issue_count: 0,
                issues: Vec::new(),
            },
            queue: QueueReadinessReadModel {
                next_ready_wave_ids: Vec::new(),
                next_ready_wave_id: None,
                claimable_wave_ids: Vec::new(),
                claimed_wave_ids: Vec::new(),
                ready_wave_count: 0,
                claimed_wave_count: 0,
                blocked_wave_count: 0,
                active_wave_count: 0,
                completed_wave_count: 0,
                queue_ready: false,
                queue_ready_reason: "no waves are ready to claim".to_string(),
            },
            next_ready_wave_ids: Vec::new(),
            waves: Vec::new(),
            has_errors: false,
        }
    }

    fn sample_acceptance_package() -> wave_app_server::AcceptancePackageSnapshot {
        wave_app_server::AcceptancePackageSnapshot {
            package_id: "acceptance-package-wave-17".to_string(),
            wave_id: 17,
            wave_slug: "portfolio-release-and-acceptance-packages".to_string(),
            wave_title: "Wave 17".to_string(),
            run_id: Some("wave-17-test".to_string()),
            ship_state: wave_app_server::ShipReadinessState::NoShip,
            release_state: wave_app_server::ReleaseReadinessState::BuildingEvidence,
            summary: "no ship: implementation proof is only 2/6 complete".to_string(),
            blocking_reasons: vec![
                "implementation proof is only 2/6 complete".to_string(),
                "signoff cannot begin until proof and release evidence are complete".to_string(),
            ],
            design_intent: wave_app_server::AcceptanceDesignIntentSnapshot {
                completeness: wave_domain::DesignCompletenessState::StructurallyComplete,
                blocker_count: 0,
                contradiction_count: 0,
                unresolved_question_count: 0,
                unresolved_assumption_count: 0,
                pending_human_input_count: 0,
                ambiguous_dependency_count: 0,
            },
            implementation: wave_app_server::AcceptanceImplementationSnapshot {
                proof_complete: false,
                proof_source: Some("mixed-envelope-and-compatibility".to_string()),
                replay_ok: Some(true),
                completed_agents: 2,
                total_agents: 6,
            },
            release: wave_app_server::AcceptanceReleaseSnapshot {
                promotion_state: None,
                merge_blocked: false,
                closure_blocked: true,
                scheduler_phase: None,
                scheduler_state: None,
                last_decision: Some("A6 failed; run released".to_string()),
            },
            signoff: wave_app_server::AcceptanceSignoffSnapshot {
                state: wave_app_server::AcceptanceSignoffState::PendingEvidence,
                complete: false,
                manual_close_applied: false,
                required_closure_agents: vec![
                    "A6".to_string(),
                    "A8".to_string(),
                    "A9".to_string(),
                    "A0".to_string(),
                ],
                completed_closure_agents: vec!["A9".to_string(), "A0".to_string()],
                pending_closure_agents: vec!["A6".to_string(), "A8".to_string()],
                pending_operator_actions: Vec::new(),
                closure_agents: vec![
                    wave_app_server::AcceptanceClosureAgentSnapshot {
                        agent_id: "A6".to_string(),
                        title: Some("Design Review Steward".to_string()),
                        status: Some(wave_trace::WaveRunStatus::Failed),
                        proof_complete: false,
                        satisfied: false,
                        error: Some("design review blocked".to_string()),
                    },
                    wave_app_server::AcceptanceClosureAgentSnapshot {
                        agent_id: "A8".to_string(),
                        title: Some("Integration Steward".to_string()),
                        status: Some(wave_trace::WaveRunStatus::Planned),
                        proof_complete: false,
                        satisfied: false,
                        error: None,
                    },
                    wave_app_server::AcceptanceClosureAgentSnapshot {
                        agent_id: "A9".to_string(),
                        title: Some("Wave Documentation Steward".to_string()),
                        status: Some(wave_trace::WaveRunStatus::Succeeded),
                        proof_complete: true,
                        satisfied: true,
                        error: None,
                    },
                    wave_app_server::AcceptanceClosureAgentSnapshot {
                        agent_id: "A0".to_string(),
                        title: Some("Running cont-QA".to_string()),
                        status: Some(wave_trace::WaveRunStatus::Succeeded),
                        proof_complete: true,
                        satisfied: true,
                        error: None,
                    },
                ],
            },
            known_risks: vec![wave_app_server::DeliveryStateItem {
                code: "agent-error".to_string(),
                summary: "agent A6 failed".to_string(),
                detail: Some("design review blocked".to_string()),
            }],
            outstanding_debt: vec![wave_app_server::DeliveryStateItem {
                code: "proof-incomplete".to_string(),
                summary: "implementation proof is incomplete (2/6)".to_string(),
                detail: Some("proof source mixed-envelope-and-compatibility".to_string()),
            }],
        }
    }

    #[test]
    fn portfolio_focus_report_carries_delivery_summary_without_portfolio_entries() {
        let package = sample_acceptance_package();

        let report =
            portfolio_focus_report(&empty_planning_status(), 17, std::slice::from_ref(&package))
                .expect("portfolio focus with delivery summary");

        let delivery = report.delivery.expect("delivery summary");
        assert_eq!(delivery.ship_state, "no_ship");
        assert_eq!(delivery.release_state, "building_evidence");
        assert_eq!(delivery.signoff_state, "pending_evidence");
        assert_eq!(delivery.completed_agents, 2);
        assert_eq!(delivery.total_agents, 6);
        assert_eq!(delivery.known_risk_count, 1);
        assert_eq!(delivery.outstanding_debt_count, 1);
        assert!(report.initiatives.is_empty());
    }

    #[test]
    fn acceptance_package_lines_expose_closure_gate_statuses() {
        let package = sample_acceptance_package();

        let lines = acceptance_package_lines(&package);

        assert!(lines.iter().any(|line| {
            line == "acceptance closure gates: A6=failed/no-proof | A8=planned/no-proof | A9=succeeded/proof | A0=succeeded/proof"
        }));
        assert!(
            lines
                .iter()
                .any(|line| { line == "acceptance closure error: A6 design review blocked" })
        );
    }
    #[test]
    fn config_root_uses_parent() {
        let root = config_root(Path::new("/tmp/example/wave.toml"));
        assert_eq!(root, PathBuf::from("/tmp/example"));
    }

    #[test]
    fn config_root_defaults_to_current_directory() {
        let root = config_root(Path::new("wave.toml"));
        assert_eq!(root, PathBuf::from("."));
    }

    #[test]
    fn shared_repo_root_from_worktree_detects_repo_root() {
        let worktree = Path::new("/repo/.wave/state/worktrees/wave-16-worktree");
        assert_eq!(
            shared_repo_root_from_worktree(worktree),
            Some(PathBuf::from("/repo"))
        );
    }

    #[test]
    fn resolve_cli_root_uses_shared_repo_root_inside_worktree() {
        let cwd = Path::new("/repo/.wave/state/worktrees/wave-16-worktree");
        let (root, config_path) =
            resolve_cli_root_and_config_path_from_cwd(Path::new("wave.toml"), cwd)
                .expect("resolve cli root");
        assert_eq!(root, PathBuf::from("/repo"));
        assert_eq!(config_path, PathBuf::from("/repo/wave.toml"));
    }

    #[test]
    fn materialized_authority_surface_counts_presence() {
        let surface = MaterializedCanonicalAuthoritySurface {
            build_specs: MaterializedPathSurface {
                path: PathBuf::from("/repo/.wave/state/build/specs"),
                exists: true,
            },
            events: MaterializedPathSurface {
                path: PathBuf::from("/repo/.wave/state/events"),
                exists: true,
            },
            control_events: MaterializedPathSurface {
                path: PathBuf::from("/repo/.wave/state/events/control"),
                exists: true,
            },
            coordination: MaterializedPathSurface {
                path: PathBuf::from("/repo/.wave/state/events/coordination"),
                exists: true,
            },
            scheduler_events: MaterializedPathSurface {
                path: PathBuf::from("/repo/.wave/state/events/scheduler"),
                exists: true,
            },
            results: MaterializedPathSurface {
                path: PathBuf::from("/repo/.wave/state/results"),
                exists: true,
            },
            derived: MaterializedPathSurface {
                path: PathBuf::from("/repo/.wave/state/derived"),
                exists: true,
            },
            projections: MaterializedPathSurface {
                path: PathBuf::from("/repo/.wave/state/projections"),
                exists: true,
            },
            state_traces: MaterializedPathSurface {
                path: PathBuf::from("/repo/.wave/state/traces"),
                exists: false,
            },
        };

        assert_eq!(surface.present_count(), 8);
        assert!(!surface.all_exist());
    }

    #[test]
    fn control_status_report_preserves_projection_spine_truth() {
        let status = PlanningStatusReadModel {
            project_name: "Test".to_string(),
            default_mode: wave_config::ExecutionMode::DarkFactory,
            summary: PlanningStatusSummary {
                total_waves: 1,
                ready_waves: 1,
                blocked_waves: 0,
                active_waves: 0,
                completed_waves: 0,
                design_incomplete_waves: 0,
                total_agents: 3,
                implementation_agents: 1,
                closure_agents: 2,
                waves_with_complete_closure: 1,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
            delivery: default_delivery().summary.clone(),
            portfolio: Default::default(),
            skill_catalog: SkillCatalogHealth {
                ok: true,
                issue_count: 0,
                issues: Vec::new(),
            },
            queue: QueueReadinessReadModel {
                next_ready_wave_ids: vec![11],
                next_ready_wave_id: Some(11),
                claimable_wave_ids: vec![11],
                claimed_wave_ids: Vec::new(),
                ready_wave_count: 1,
                claimed_wave_count: 0,
                blocked_wave_count: 0,
                active_wave_count: 0,
                completed_wave_count: 0,
                queue_ready: true,
                queue_ready_reason: "ready waves are available to claim".to_string(),
            },
            next_ready_wave_ids: vec![11],
            waves: vec![WaveStatusReadModel {
                id: 11,
                slug: "projection-spine".to_string(),
                title: "Projection Spine".to_string(),
                depends_on: Vec::new(),
                blocked_by: Vec::new(),
                blocker_state: Vec::new(),
                design_completeness: wave_domain::DesignCompletenessState::ImplementationReady,
                lint_errors: 0,
                ready: true,
                ownership: empty_ownership(),
                execution: empty_execution(),
                recovery: empty_recovery(),
                agent_count: 3,
                implementation_agent_count: 1,
                closure_agent_count: 2,
                closure_complete: true,
                required_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                present_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                missing_closure_agents: Vec::new(),
                readiness: WaveReadinessReadModel {
                    state: QueueReadinessStateReadModel::Ready,
                    planning_ready: true,
                    claimable: true,
                    reasons: Vec::new(),
                    primary_reason: None,
                },
                rerun_requested: false,
                closure_override_applied: false,
                completed: false,
                last_run_status: None,
                soft_state: wave_domain::SoftState::Clear,
            }],
            has_errors: false,
        };
        let projection = build_planning_status_projection(&status);
        let planning = PlanningProjectionBundle {
            status: status.clone(),
            projection: projection.clone(),
        };
        let delivery = default_delivery();
        let operator = build_operator_snapshot_inputs(&planning, &delivery, &HashMap::new(), true);
        let spine = ProjectionSpine {
            planning,
            operator,
            delivery,
        };

        let report = build_control_status_report(&spine);

        assert_eq!(report.status.queue, report.projection.queue.readiness);
        assert_eq!(
            report.operator.dashboard.next_ready_wave_ids,
            report.status.next_ready_wave_ids
        );
        assert_eq!(
            report.operator.queue.waves.len(),
            report.projection.waves.len()
        );
        assert_eq!(report.operator.queue.waves[0].queue_state, "ready");
        assert_eq!(
            report.control_status.queue_decision.claimable_wave_ids,
            report.operator.queue.claimable_wave_ids
        );
        assert_eq!(
            report.control_status.queue_decision.blocker_summary,
            report.operator.queue.blocker_summary
        );
        assert_eq!(
            report.control_status.queue_decision.lines[0],
            "queue decision: next claimable wave=11"
        );
    }

    #[test]
    fn control_show_design_lines_surface_lineage_details() {
        let design = wave_app_server::WaveDesignDetail {
            wave_id: 16,
            completeness: wave_domain::DesignCompletenessState::Underspecified,
            blocker_reasons: vec![
                "design:open-question:question-api-shape".to_string(),
                "design:invalidated-decision:decision-api-shape".to_string(),
            ],
            active_contradictions: vec![wave_app_server::ContradictionDetail {
                contradiction_id: "contradiction-16".to_string(),
                state: "detected".to_string(),
                summary: "API shape contradicts dependency result".to_string(),
                detail: Some("wave 16 still depends on an invalidated API fact".to_string()),
                invalidated_refs: vec![
                    "fact:fact-api".to_string(),
                    "decision:decision-api-shape".to_string(),
                ],
            }],
            unresolved_question_ids: vec!["question-api-shape".to_string()],
            unresolved_assumption_ids: vec!["assumption-cache-valid".to_string()],
            pending_human_inputs: vec![wave_app_server::PendingHumanInputDetail {
                request_id: "human-16".to_string(),
                task_id: Some("wave-16:agent-a2".to_string()),
                state: wave_domain::HumanInputState::Pending,
                workflow_kind: wave_domain::HumanInputWorkflowKind::DependencyHandshake,
                route: "dependency:wave-15".to_string(),
                prompt: "Need dependency confirmation".to_string(),
                requested_by: "A2".to_string(),
                answer: None,
            }],
            dependency_handshake_routes: vec!["dependency:wave-15".to_string()],
            invalidated_fact_ids: vec!["fact-api".to_string()],
            invalidated_decision_ids: vec!["decision-api-shape".to_string()],
            invalidation_routes: vec![
                "contradiction contradiction-16 invalidates fact fact-api -> decision decision-api-shape"
                    .to_string(),
            ],
            selectively_invalidated_task_ids: vec!["wave-16:agent-a2".to_string()],
            superseded_decision_ids: vec!["decision-api-v1".to_string()],
            ambiguous_dependency_wave_ids: vec![15],
        };

        let lines = control_show_design_lines(&design);

        assert!(
            lines
                .iter()
                .any(|line| line == "open questions: question-api-shape")
        );
        assert!(
            lines
                .iter()
                .any(|line| line == "open assumptions: assumption-cache-valid")
        );
        assert!(
            lines
                .iter()
                .any(|line| line == "invalidated facts: fact-api")
        );
        assert!(
            lines
                .iter()
                .any(|line| line == "superseded decisions: decision-api-v1")
        );
        assert!(
            lines
                .iter()
                .any(|line| line == "ambiguous dependency waves: 15")
        );
    }

    #[test]
    fn proof_report_falls_back_to_latest_completed_run() {
        let root = std::env::temp_dir().join(format!(
            "wave-cli-proof-test-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        let config = ProjectConfig::default();
        std::fs::create_dir_all(&root).expect("create temp root");
        let bundle_dir = root.join(".wave/state/build/specs/wave-12-1");
        let agent_dir = bundle_dir.join("agents/A1");
        let trace_path = root.join(".wave/traces/runs/wave-12-1.json");
        let envelope_path =
            root.join(".wave/state/results/wave-12/attempt-a1/agent_result_envelope.json");
        std::fs::create_dir_all(&agent_dir).expect("create agent dir");
        std::fs::create_dir_all(trace_path.parent().expect("trace dir")).expect("create trace dir");
        std::fs::create_dir_all(envelope_path.parent().expect("envelope dir"))
            .expect("create envelope dir");
        std::fs::create_dir_all(root.join(".wave/codex")).expect("create codex dir");
        std::fs::write(root.join("README.md"), "proof\n").expect("write proof artifact");
        std::fs::write(agent_dir.join("prompt.md"), "# prompt\n").expect("write prompt");
        std::fs::write(agent_dir.join("last-message.txt"), "[wave-proof]\n")
            .expect("write message");
        std::fs::write(agent_dir.join("events.jsonl"), "{}\n").expect("write events");
        std::fs::write(agent_dir.join("stderr.txt"), "").expect("write stderr");
        wave_trace::write_result_envelope(
            &envelope_path,
            &wave_trace::ResultEnvelopeRecord {
                result_envelope_id: "result:wave-12-1:a1".to_string(),
                wave_id: 12,
                task_id: "wave-12:agent-a1".to_string(),
                attempt_id: "attempt-a1".to_string(),
                agent_id: "A1".to_string(),
                task_role: "implementation".to_string(),
                closure_role: None,
                source: wave_trace::ResultEnvelopeSource::Structured,
                attempt_state: wave_trace::AttemptState::Succeeded,
                disposition: wave_trace::ResultDisposition::Completed,
                summary: Some("structured".to_string()),
                output_text: Some("[wave-proof]".to_string()),
                final_markers: wave_trace::FinalMarkerEnvelope::from_contract(
                    vec!["[wave-proof]".to_string()],
                    vec!["[wave-proof]".to_string()],
                ),
                proof_bundle_ids: Vec::new(),
                fact_ids: Vec::new(),
                contradiction_ids: Vec::new(),
                artifacts: Vec::new(),
                doc_delta: wave_trace::DocDeltaEnvelope::default(),
                marker_evidence: Vec::new(),
                closure: wave_trace::ClosureState {
                    disposition: wave_trace::ClosureDisposition::Ready,
                    required_final_markers: vec!["[wave-proof]".to_string()],
                    observed_final_markers: vec!["[wave-proof]".to_string()],
                    blocking_reasons: Vec::new(),
                    satisfied_fact_ids: Vec::new(),
                    contradiction_ids: Vec::new(),
                    verdict: wave_trace::ClosureVerdictPayload::None,
                },
                runtime: None,
                created_at_ms: 3,
            },
        )
        .expect("write envelope");

        let wave = proof_test_wave();
        let run = WaveRunRecord {
            run_id: "wave-12-1".to_string(),
            wave_id: 12,
            slug: "result-envelope".to_string(),
            title: "Result Envelope".to_string(),
            status: wave_trace::WaveRunStatus::Succeeded,
            dry_run: false,
            bundle_dir: bundle_dir.clone(),
            trace_path: trace_path.clone(),
            codex_home: root.join(".wave/codex"),
            created_at_ms: 1,
            started_at_ms: Some(2),
            launcher_pid: None,
            launcher_started_at_ms: None,
            worktree: None,
            promotion: None,
            scheduling: None,
            completed_at_ms: Some(3),
            agents: vec![wave_trace::AgentRunRecord {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                status: wave_trace::WaveRunStatus::Succeeded,
                prompt_path: agent_dir.join("prompt.md"),
                last_message_path: agent_dir.join("last-message.txt"),
                events_path: agent_dir.join("events.jsonl"),
                stderr_path: agent_dir.join("stderr.txt"),
                result_envelope_path: Some(envelope_path),
                runtime_detail_path: None,
                expected_markers: vec!["[wave-proof]".to_string()],
                observed_markers: Vec::new(),
                exit_code: Some(0),
                error: None,
                runtime: None,
            }],
            error: None,
        };
        wave_trace::write_trace_bundle(&trace_path, &run).expect("write trace bundle");
        let latest_runs = HashMap::from([(12, run)]);

        let report = proof_report_for_wave(
            &root,
            &config,
            &[wave],
            &empty_planning_status(),
            &[],
            &[],
            &latest_runs,
            12,
        );

        assert_eq!(report.run_id.as_deref(), Some("wave-12-1"));
        assert!(report.proof.as_ref().expect("proof").complete);
        assert_eq!(
            report.proof.as_ref().expect("proof").proof_source,
            "structured-envelope"
        );
        assert_eq!(
            report.run.as_ref().expect("run detail").agents[0].proof_source,
            "structured-envelope"
        );
        assert!(report.replay.is_some());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn doctor_report_materializes_missing_scheduler_root() {
        let root = std::env::temp_dir().join(format!(
            "wave-cli-doctor-roots-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
        std::fs::create_dir_all(&root).expect("create temp root");

        let config = ProjectConfig::default();
        let resolved = config.resolved_paths(&root);
        for path in [
            resolved.authority.state_dir.clone(),
            resolved.authority.state_build_specs_dir.clone(),
            resolved.authority.state_events_dir.clone(),
            resolved.authority.state_events_control_dir.clone(),
            resolved.authority.state_events_coordination_dir.clone(),
            resolved.authority.state_results_dir.clone(),
            resolved.authority.state_derived_dir.clone(),
            resolved.authority.state_projections_dir.clone(),
            resolved.authority.state_traces_dir.clone(),
        ] {
            std::fs::create_dir_all(path).expect("create authority path");
        }

        let status = PlanningStatusReadModel {
            project_name: "Test".to_string(),
            default_mode: wave_config::ExecutionMode::DarkFactory,
            summary: PlanningStatusSummary {
                total_waves: 1,
                ready_waves: 1,
                blocked_waves: 0,
                active_waves: 0,
                completed_waves: 0,
                design_incomplete_waves: 0,
                total_agents: 3,
                implementation_agents: 1,
                closure_agents: 2,
                waves_with_complete_closure: 1,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
            delivery: default_delivery().summary.clone(),
            portfolio: Default::default(),
            skill_catalog: SkillCatalogHealth {
                ok: true,
                issue_count: 0,
                issues: Vec::new(),
            },
            queue: QueueReadinessReadModel {
                next_ready_wave_ids: vec![12],
                next_ready_wave_id: Some(12),
                claimable_wave_ids: vec![12],
                claimed_wave_ids: Vec::new(),
                ready_wave_count: 1,
                claimed_wave_count: 0,
                blocked_wave_count: 0,
                active_wave_count: 0,
                completed_wave_count: 0,
                queue_ready: true,
                queue_ready_reason: "ready waves are available to claim".to_string(),
            },
            next_ready_wave_ids: vec![12],
            waves: vec![WaveStatusReadModel {
                id: 12,
                slug: "result-envelope".to_string(),
                title: "Result Envelope".to_string(),
                depends_on: Vec::new(),
                blocked_by: Vec::new(),
                blocker_state: Vec::new(),
                design_completeness: wave_domain::DesignCompletenessState::ImplementationReady,
                lint_errors: 0,
                ready: true,
                ownership: empty_ownership(),
                execution: empty_execution(),
                recovery: empty_recovery(),
                agent_count: 3,
                implementation_agent_count: 1,
                closure_agent_count: 2,
                closure_complete: true,
                required_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                present_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                missing_closure_agents: Vec::new(),
                readiness: WaveReadinessReadModel {
                    state: QueueReadinessStateReadModel::Ready,
                    planning_ready: true,
                    claimable: true,
                    reasons: Vec::new(),
                    primary_reason: None,
                },
                rerun_requested: false,
                closure_override_applied: false,
                completed: false,
                last_run_status: None,
                soft_state: wave_domain::SoftState::Clear,
            }],
            has_errors: false,
        };
        let projection = build_planning_status_projection(&status);
        let planning = PlanningProjectionBundle {
            status: status.clone(),
            projection,
        };
        let delivery = default_delivery();
        let operator = build_operator_snapshot_inputs(&planning, &delivery, &HashMap::new(), true);
        let spine = ProjectionSpine {
            planning,
            operator,
            delivery,
        };

        let report = build_doctor_report(
            &root.join("wave.toml"),
            &config,
            &root,
            &[proof_test_wave()],
            &[],
            &HashMap::new(),
            &spine,
        )
        .expect("build doctor report");

        let materialized_check = report
            .checks
            .iter()
            .find(|check| check.name == "materialized-authority-roots")
            .expect("materialized check");
        assert!(materialized_check.ok, "{}", materialized_check.detail);
        assert!(
            report
                .authority
                .materialized_canonical
                .scheduler_events
                .exists
        );
        assert!(resolved.authority.state_events_scheduler_dir.exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn control_runtime_lines_keep_mixed_runtime_runs_explicit() {
        let run = ActiveRunDetail {
            wave_id: 15,
            wave_slug: "runtime-policy".to_string(),
            wave_title: "Runtime Policy".to_string(),
            run_id: "wave-15-test".to_string(),
            status: wave_trace::WaveRunStatus::Running,
            created_at_ms: 1,
            started_at_ms: Some(2),
            elapsed_ms: Some(3_000),
            current_agent_id: Some("A2".to_string()),
            current_agent_title: Some("Claude Adapter".to_string()),
            activity_excerpt: "testing runtime boundary".to_string(),
            last_activity_at_ms: Some(2),
            activity_source: Some("events".to_string()),
            stalled: false,
            stall_reason: None,
            proof: wave_app_server::ProofSnapshot {
                declared_artifacts: Vec::new(),
                complete: false,
                proof_source: "structured-envelope".to_string(),
                completed_agents: 2,
                envelope_backed_agents: 2,
                compatibility_backed_agents: 0,
                total_agents: 6,
            },
            replay: wave_trace::ReplayReport {
                run_id: "wave-15-test".to_string(),
                wave_id: 15,
                ok: true,
                issues: Vec::new(),
            },
            runtime_summary: wave_app_server::RuntimeSummary {
                selected_runtimes: vec!["claude".to_string(), "codex".to_string()],
                requested_runtimes: vec!["claude".to_string(), "codex".to_string()],
                selection_sources: vec!["executor.id".to_string()],
                fallback_targets: vec!["claude".to_string()],
                fallback_count: 1,
                agents_with_runtime: 2,
            },
            execution: empty_execution(),
            agents: vec![
                wave_app_server::AgentPanelItem {
                    id: "A1".to_string(),
                    title: "Codex Adapter".to_string(),
                    status: wave_trace::WaveRunStatus::Succeeded,
                    current_task: "done".to_string(),
                    reused_from_prior_run: false,
                    proof_complete: true,
                    proof_source: "structured-envelope".to_string(),
                    expected_markers: vec!["[wave-proof]".to_string()],
                    observed_markers: vec!["[wave-proof]".to_string()],
                    missing_markers: Vec::new(),
                    deliverables: Vec::new(),
                    error: None,
                    runtime: Some(test_runtime_detail(
                        "codex",
                        "wave-runtime/codex",
                        "openai-codex-cli",
                        false,
                    )),
                },
                wave_app_server::AgentPanelItem {
                    id: "A2".to_string(),
                    title: "Claude Adapter".to_string(),
                    status: wave_trace::WaveRunStatus::Running,
                    current_task: "running".to_string(),
                    reused_from_prior_run: true,
                    proof_complete: false,
                    proof_source: "structured-envelope".to_string(),
                    expected_markers: vec!["[wave-proof]".to_string()],
                    observed_markers: Vec::new(),
                    missing_markers: vec!["[wave-proof]".to_string()],
                    deliverables: Vec::new(),
                    error: None,
                    runtime: Some(test_runtime_detail(
                        "claude",
                        "wave-runtime/claude",
                        "anthropic-claude-code",
                        true,
                    )),
                },
            ],
            mas: None,
        };

        let lines = control_runtime_lines(&run);

        assert!(
            lines
                .iter()
                .any(|line| line == "run runtimes: claude, codex")
        );
        assert!(lines.iter().any(|line| line
            == "current agent runtime: requested codex -> selected claude via executor.id"));
        assert!(
            lines
                .iter()
                .all(|line| !line.starts_with("runtime decision:"))
        );
    }

    fn test_runtime_detail(
        selected_runtime: &str,
        adapter: &str,
        provider: &str,
        uses_fallback: bool,
    ) -> wave_app_server::RuntimeDetail {
        wave_app_server::RuntimeDetail {
            selected_runtime: selected_runtime.to_string(),
            selection_reason: format!("selected {selected_runtime}"),
            policy: wave_app_server::RuntimePolicyDetail {
                requested_runtime: Some("codex".to_string()),
                allowed_runtimes: vec!["codex".to_string(), "claude".to_string()],
                fallback_runtimes: vec!["claude".to_string()],
                selection_source: Some("executor.id".to_string()),
                uses_fallback,
            },
            fallback: uses_fallback.then(|| wave_app_server::RuntimeFallbackDetail {
                requested_runtime: "codex".to_string(),
                selected_runtime: selected_runtime.to_string(),
                reason: "codex unavailable".to_string(),
            }),
            execution_identity: wave_app_server::RuntimeExecutionIdentityDetail {
                adapter: adapter.to_string(),
                binary: selected_runtime.to_string(),
                provider: provider.to_string(),
                artifact_paths: BTreeMap::new(),
            },
            skill_projection: wave_app_server::RuntimeSkillProjectionDetail {
                declared_skills: vec!["wave-core".to_string()],
                projected_skills: vec!["wave-core".to_string()],
                dropped_skills: Vec::new(),
                auto_attached_skills: vec![format!("runtime-{selected_runtime}")],
            },
        }
    }

    fn proof_test_wave() -> WaveDocument {
        WaveDocument {
            path: PathBuf::from("waves/12.md"),
            metadata: WaveMetadata {
                id: 12,
                slug: "result-envelope".to_string(),
                title: "Result Envelope".to_string(),
                mode: wave_config::ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on: Vec::new(),
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["README.md".to_string()],
                ..default_wave_metadata()
            },
            heading_title: Some("Wave 12".to_string()),
            commit_message: Some("Feat: result envelope".to_string()),
            component_promotions: Vec::new(),
            deploy_environments: Vec::new(),
            context7_defaults: None,
            agents: vec![WaveAgent {
                id: "A1".to_string(),
                title: "Implementation".to_string(),
                role_prompts: Vec::new(),
                executor: BTreeMap::from([("model".to_string(), "gpt-5.4".to_string())]),
                context7: Some(Context7Defaults {
                    bundle: "none".to_string(),
                    query: Some("noop".to_string()),
                }),
                skills: Vec::new(),
                components: Vec::new(),
                capabilities: Vec::new(),
                exit_contract: Some(ExitContract {
                    completion: CompletionLevel::Contract,
                    durability: DurabilityLevel::Durable,
                    proof: ProofLevel::Unit,
                    doc_impact: DocImpact::Owned,
                }),
                deliverables: vec!["README.md".to_string()],
                file_ownership: vec!["README.md".to_string()],
                depends_on_agents: Vec::new(),
                reads_artifacts_from: Vec::new(),
                writes_artifacts: Vec::new(),
                barrier_class: wave_spec::BarrierClass::Independent,
                parallel_safety: wave_spec::ParallelSafetyClass::Serialized,
                exclusive_resources: Vec::new(),
                parallel_with: Vec::new(),
                final_markers: vec!["[wave-proof]".to_string()],
                prompt: "Primary goal:\n- noop\n\nRequired context before coding:\n- Read README.md.\n\nFile ownership (only touch these paths):\n- README.md".to_string(),
            }],
        }
    }
}
