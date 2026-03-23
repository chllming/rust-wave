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
use wave_app_server::load_operator_snapshot;
use wave_config::DEFAULT_CONFIG_PATH;
use wave_config::ProjectConfig;
use wave_control_plane::PlanningStatus;
use wave_control_plane::PlanningStatusProjection;
use wave_control_plane::WaveQueueEntry;
use wave_control_plane::WaveRef;
use wave_control_plane::build_planning_status_projection;
use wave_control_plane::build_planning_status_with_state;
use wave_dark_factory::LintFinding;
use wave_dark_factory::has_errors;
use wave_dark_factory::lint_project;
use wave_dark_factory::validate_context7_bundle_catalog;
use wave_dark_factory::validate_skill_catalog;
use wave_runtime::AutonomousOptions;
use wave_runtime::DogfoodEvidenceReport;
use wave_runtime::LaunchOptions;
use wave_runtime::LaunchPreflightError;
use wave_runtime::LaunchPreflightReport;
use wave_runtime::RerunIntentRecord;
use wave_runtime::autonomous_launch;
use wave_runtime::clear_rerun;
use wave_runtime::codex_binary_available;
use wave_runtime::dogfood_evidence_report;
use wave_runtime::draft_wave;
use wave_runtime::launch_wave;
use wave_runtime::load_latest_runs;
use wave_runtime::pending_rerun_wave_ids;
use wave_runtime::request_rerun;
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
    checks: Vec<DoctorCheck>,
    projection: PlanningStatusProjection,
    status: PlanningStatus,
}

#[derive(Debug, Serialize)]
struct ControlStatusReport {
    projection: PlanningStatusProjection,
    status: PlanningStatus,
}

#[derive(Debug, Serialize)]
struct ControlShowReport {
    wave: WaveQueueEntry,
    active_run: Option<ActiveRunDetail>,
    rerun_intent: Option<RerunIntentRecord>,
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
    proof: Option<ProofSnapshot>,
    replay: Option<ReplayReport>,
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
    let status = build_planning_status_with_state(
        &config,
        &waves,
        &findings,
        &skill_catalog_issues,
        &latest_runs,
        &rerun_wave_ids,
    );

    match cli.command {
        None => {
            if io::stdout().is_terminal() && io::stdin().is_terminal() {
                wave_tui::run(&root, &config)
            } else {
                render_summary(&config, &status, &latest_runs)
            }
        }
        Some(Command::Project {
            command: ProjectCommand::Show { json },
        }) => render_project(&config, json),
        Some(Command::Doctor { json }) => render_doctor(
            &cli.config,
            &config,
            &root,
            &waves,
            &findings,
            &latest_runs,
            &status,
            json,
        ),
        Some(Command::Lint { json }) => render_lint(&findings, json),
        Some(Command::Draft { wave, json }) => render_draft(&root, &waves, &status, wave, json),
        Some(Command::Control {
            command: ControlCommand::Status { json },
        }) => render_status(&status, json),
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
                    command: RerunCommand::Request { wave, reason, json },
                },
        }) => render_rerun_request(&root, &config, wave, &reason, json),
        Some(Command::Control {
            command:
                ControlCommand::Rerun {
                    command: RerunCommand::Clear { wave, json },
                },
        }) => render_rerun_clear(&root, &config, wave, json),
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
            &status,
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
            status,
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

fn render_summary(
    config: &ProjectConfig,
    status: &PlanningStatus,
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> Result<()> {
    let projection = build_planning_status_projection(status);

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
    render_queue_decision_lines(status, &projection);
    println!(
        "skill issue paths: {}",
        format_string_list(&projection.skill_catalog.issue_paths)
    );
    println!(
        "launcher: codex={} ready={}",
        codex_binary_available(),
        !status.next_ready_wave_ids.is_empty()
    );
    println!(
        "active runs: {}",
        latest_runs
            .values()
            .filter(|run| !run.completed_successfully())
            .count()
    );
    Ok(())
}

fn render_project(config: &ProjectConfig, json: bool) -> Result<()> {
    if json {
        print_json(config)
    } else {
        println!("project: {}", config.project_name);
        println!("default lane: {}", config.default_lane);
        println!("default mode: {}", config.default_mode);
        println!("waves dir: {}", config.waves_dir.display());
        println!("codex vendor dir: {}", config.codex_vendor_dir.display());
        println!(
            "project codex home: {}",
            config.project_codex_home.display()
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
    status: &PlanningStatus,
    json: bool,
) -> Result<()> {
    let projection = build_planning_status_projection(status);
    let context7_catalog_issues = validate_context7_bundle_catalog(root);
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
            name: "codex-binary",
            ok: codex_binary_available(),
            detail: "checked `codex --version`".to_string(),
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
    let report = DoctorReport {
        ok: checks.iter().all(|check| check.ok),
        checks,
        projection: projection.clone(),
        status: status.clone(),
    };
    if json {
        print_json(&report)
    } else {
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
        render_queue_decision_lines(status, &projection);
        println!(
            "skill issue paths: {}",
            format_string_list(&projection.skill_catalog.issue_paths)
        );
        for check in &report.checks {
            println!(
                "- {}: {} ({})",
                check.name,
                if check.ok { "ok" } else { "error" },
                check.detail
            );
        }
        render_projection_attention_lines(&projection);
        for issue in &projection.skill_catalog.issues {
            println!("skill issue: {} ({})", issue.path, issue.message);
        }
        for issue in &context7_catalog_issues {
            println!("context7 issue: {} ({})", issue.path, issue.message);
        }
        Ok(())
    }
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
    status: &PlanningStatus,
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

fn render_status(status: &PlanningStatus, json: bool) -> Result<()> {
    let projection = build_planning_status_projection(status);

    if json {
        print_json(&ControlStatusReport {
            projection,
            status: status.clone(),
        })
    } else {
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
        render_queue_decision_lines(status, &projection);
        println!(
            "skill issue paths: {}",
            format_string_list(&projection.skill_catalog.issue_paths)
        );
        if projection.waves.iter().any(|wave| !wave.closure.complete) {
            render_projection_attention_lines(&projection);
        }
        if !projection.skill_catalog.issues.is_empty() {
            for issue in &projection.skill_catalog.issues {
                println!("skill issue: {} ({})", issue.path, issue.message);
            }
        }
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
    let active_run = snapshot
        .active_run_details
        .iter()
        .find(|run| run.wave_id == wave_id)
        .cloned();
    let rerun_intent = snapshot
        .rerun_intents
        .iter()
        .find(|intent| intent.wave_id == wave_id)
        .cloned();
    let report = ControlShowReport {
        wave,
        active_run,
        rerun_intent,
    };
    if json {
        print_json(&report)
    } else {
        println!("wave {} {}", report.wave.id, report.wave.title);
        println!("ready: {}", report.wave.ready);
        println!("rerun requested: {}", report.wave.rerun_requested);
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
        if let Some(run) = report.active_run {
            println!("active run: {}", run.run_id);
            println!(
                "current agent: {}",
                run.current_agent_id
                    .zip(run.current_agent_title)
                    .map(|(id, title)| format!("{id} {title}"))
                    .unwrap_or_else(|| "none".to_string())
            );
            println!(
                "proof: {}/{} complete={}",
                run.proof.completed_agents, run.proof.total_agents, run.proof.complete
            );
            println!("replay ok: {}", run.replay.ok);
        }
        if let Some(intent) = report.rerun_intent {
            println!("rerun intent: {} ({})", intent.reason, intent.requested_by);
        }
        Ok(())
    }
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
                "- wave {} | status={:?} | requested_by={} | reason={}",
                intent.wave_id, intent.status, intent.requested_by, intent.reason
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
    json: bool,
) -> Result<()> {
    let record = request_rerun(root, config, wave_id, reason)?;
    if json {
        print_json(&record)
    } else {
        println!("requested rerun for wave {}", wave_id);
        println!("reason: {}", record.reason);
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

fn render_proof_show(
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
        .map(|run| ProofReport {
            wave_id,
            run_id: Some(run.run_id.clone()),
            proof: Some(run.proof.clone()),
            replay: Some(run.replay.clone()),
        })
        .unwrap_or(ProofReport {
            wave_id,
            run_id: None,
            proof: None,
            replay: None,
        });

    if json {
        print_json(&report)
    } else if let Some(proof) = report.proof {
        println!(
            "wave {} proof {}/{} complete={}",
            wave_id, proof.completed_agents, proof.total_agents, proof.complete
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
        Ok(())
    } else {
        println!("wave {} has no active proof snapshot", wave_id);
        Ok(())
    }
}

fn render_launch(
    root: &Path,
    config: &ProjectConfig,
    waves: &[WaveDocument],
    status: &PlanningStatus,
    options: LaunchOptions,
    json: bool,
) -> Result<()> {
    ensure_codex_available(options.dry_run)?;
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
    status: PlanningStatus,
    options: AutonomousOptions,
    json: bool,
) -> Result<()> {
    ensure_codex_available(options.dry_run)?;
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

fn ensure_codex_available(dry_run: bool) -> Result<()> {
    if dry_run || codex_binary_available() {
        Ok(())
    } else {
        anyhow::bail!(
            "codex binary is required for non-dry-run launch actions; install codex or use --dry-run"
        )
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

fn select_wave_id(snapshot: &OperatorSnapshot, requested: Option<u32>) -> Result<u32> {
    if let Some(wave_id) = requested {
        return Ok(wave_id);
    }
    if let Some(run) = snapshot.active_run_details.first() {
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

fn format_wave_ids(wave_ids: &[u32]) -> String {
    if wave_ids.is_empty() {
        "none".to_string()
    } else {
        wave_ids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn format_string_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn format_wave_refs(values: &[WaveRef]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .map(|wave| format!("{}:{}", wave.id, wave.slug))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn render_queue_decision_lines(status: &PlanningStatus, projection: &PlanningStatusProjection) {
    let next_wave = status
        .queue
        .next_ready_wave_ids
        .first()
        .copied()
        .map(|wave_id| wave_id.to_string())
        .unwrap_or_else(|| "none".to_string());
    for line in queue_decision_story_lines(
        &next_wave,
        &status.queue.queue_ready_reason,
        &status.queue.claimable_wave_ids,
        projection.queue.blocker_summary.dependency,
        projection.queue.blocker_summary.lint,
        projection.queue.blocker_summary.closure,
        projection.queue.blocker_summary.active_run,
        &projection.queue.blocker_waves.closure,
    ) {
        println!("{line}");
    }
}

fn queue_decision_story_lines(
    next_wave: &str,
    queue_ready_reason: &str,
    claimable_wave_ids: &[u32],
    dependency_blockers: usize,
    lint_blockers: usize,
    closure_blockers: usize,
    active_run_blockers: usize,
    closure_blocked: &[WaveRef],
) -> Vec<String> {
    vec![
        format!("queue decision: next claimable wave={next_wave}"),
        format!(
            "queue decision: claimable waves={}",
            format_wave_ids(claimable_wave_ids)
        ),
        format!("queue decision: queue ready reason={queue_ready_reason}"),
        format!(
            "queue decision: blocker story dependency={} lint={} closure={} active_run={}",
            dependency_blockers, lint_blockers, closure_blockers, active_run_blockers
        ),
        format!(
            "queue decision: closure-blocked={}",
            format_wave_refs(closure_blocked)
        ),
    ]
}

fn render_projection_attention_lines(projection: &PlanningStatusProjection) {
    for wave in &projection.waves {
        if !wave.closure.complete {
            println!(
                "closure gap: wave {} {} missing {} | agents={} (impl={} closure={}) | blockers={}",
                wave.id,
                wave.slug,
                format_string_list(&wave.closure.missing_agents),
                wave.agents.total,
                wave.agents.implementation,
                wave.agents.closure,
                format_blockers(&wave.blocked_by)
            );
        }
    }
}

fn format_blockers(blocked_by: &[String]) -> String {
    if blocked_by.is_empty() {
        "none".to_string()
    } else {
        blocked_by.join(", ")
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let rendered = serde_json::to_string_pretty(value)?;
    println!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
