use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use serde::Serialize;
use std::collections::HashMap;
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
use wave_domain::RerunScope;
use wave_domain::WaveClosureOverrideRecord;
use wave_runtime::AutonomousOptions;
use wave_runtime::DogfoodEvidenceReport;
use wave_runtime::LaunchOptions;
use wave_runtime::LaunchPreflightError;
use wave_runtime::LaunchPreflightReport;
use wave_runtime::RerunIntentRecord;
use wave_runtime::active_closure_override_wave_ids;
use wave_runtime::apply_closure_override;
use wave_runtime::autonomous_launch;
use wave_runtime::clear_rerun;
use wave_runtime::clear_closure_override;
use wave_runtime::dogfood_evidence_report;
use wave_runtime::draft_wave;
use wave_runtime::launch_wave;
use wave_runtime::list_closure_overrides;
use wave_runtime::load_latest_runs;
use wave_runtime::pending_rerun_wave_ids;
use wave_runtime::repair_orphaned_runs;
use wave_runtime::request_rerun;
use wave_runtime::runtime_boundary_status;
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
    Adhoc,
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
    control_status: ControlStatusReadModel,
}

#[derive(Debug, Serialize)]
struct ControlShowReport {
    wave: WaveStatusReadModel,
    latest_run: Option<ActiveRunDetail>,
    rerun_intent: Option<RerunIntentRecord>,
    closure_override: Option<WaveClosureOverrideRecord>,
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
    replay: Option<ReplayReport>,
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
    let root = config_root(&cli.config);
    let config = ProjectConfig::load(&cli.config)?;
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
            &cli.config,
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
        Some(Command::Adhoc) => render_not_ready(
            "adhoc",
            "ad hoc execution is still pending; use draft or launch with a concrete wave",
        ),
    }
}

fn config_root(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
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
    for line in &control_status.queue_decision.lines {
        println!("{line}");
    }
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
        for line in &control_status.queue_decision.lines {
            println!("{line}");
        }
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
        Ok(())
    }
}

fn build_control_status_report(spine: &ProjectionSpine) -> ControlStatusReport {
    ControlStatusReport {
        status: spine.planning.status.clone(),
        projection: spine.planning.projection.clone(),
        operator: spine.operator.clone(),
        control_status: build_control_status_read_model_from_spine(spine),
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
    let report = ControlShowReport {
        wave,
        latest_run,
        rerun_intent,
        closure_override,
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
        Ok(())
    }
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
                "- {} {} | state={} | proof={} | deliverables={}",
                agent.id,
                agent.title,
                agent.status,
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
        &waves,
        &snapshot.latest_run_details,
        &relevant_runs,
        wave_id,
    );

    if json {
        print_json(&report)
    } else if let Some(proof) = report.proof {
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
                    "- {} {} | state={} | source={} | proof={}",
                    agent.id,
                    agent.title,
                    agent.status,
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
        println!("wave {} has no recorded proof snapshot", wave_id);
        Ok(())
    }
}

fn proof_report_for_wave(
    root: &Path,
    waves: &[WaveDocument],
    latest_run_details: &[ActiveRunDetail],
    latest_runs: &HashMap<u32, WaveRunRecord>,
    wave_id: u32,
) -> ProofReport {
    let run_detail = latest_run_details
        .iter()
        .find(|run| run.wave_id == wave_id)
        .cloned()
        .or_else(|| latest_relevant_run_detail(root, waves, latest_runs, wave_id));

    run_detail
        .map(|run| ProofReport {
            wave_id,
            run_id: Some(run.run_id.clone()),
            proof: Some(run.proof.clone()),
            replay: Some(run.replay.clone()),
            run: Some(run),
        })
        .unwrap_or(ProofReport {
            wave_id,
            run_id: None,
            run: None,
            proof: None,
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
                total_agents: 3,
                implementation_agents: 1,
                closure_agents: 2,
                waves_with_complete_closure: 1,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
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
                lint_errors: 0,
                ready: true,
                ownership: empty_ownership(),
                execution: empty_execution(),
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
            }],
            has_errors: false,
        };
        let projection = build_planning_status_projection(&status);
        let planning = PlanningProjectionBundle {
            status: status.clone(),
            projection: projection.clone(),
        };
        let operator = build_operator_snapshot_inputs(&planning, &HashMap::new(), true);
        let spine = ProjectionSpine { planning, operator };

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
    fn proof_report_falls_back_to_latest_completed_run() {
        let root = std::env::temp_dir().join(format!(
            "wave-cli-proof-test-{}-{}",
            std::process::id(),
            wave_trace::now_epoch_ms().expect("timestamp")
        ));
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

        let report = proof_report_for_wave(&root, &[wave], &[], &latest_runs, 12);

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
                total_agents: 3,
                implementation_agents: 1,
                closure_agents: 2,
                waves_with_complete_closure: 1,
                waves_missing_closure: 0,
                total_missing_closure_agents: 0,
                lint_error_waves: 0,
                skill_catalog_issue_count: 0,
            },
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
                lint_errors: 0,
                ready: true,
                ownership: empty_ownership(),
                execution: empty_execution(),
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
            }],
            has_errors: false,
        };
        let projection = build_planning_status_projection(&status);
        let planning = PlanningProjectionBundle {
            status: status.clone(),
            projection,
        };
        let operator = build_operator_snapshot_inputs(&planning, &HashMap::new(), true);
        let spine = ProjectionSpine { planning, operator };

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
                final_markers: vec!["[wave-proof]".to_string()],
                prompt: "Primary goal:\n- noop\n\nRequired context before coding:\n- Read README.md.\n\nFile ownership (only touch these paths):\n- README.md".to_string(),
            }],
        }
    }
}
