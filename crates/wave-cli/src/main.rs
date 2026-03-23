use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use serde::Serialize;
use std::path::Path;
use std::path::PathBuf;
use wave_config::DEFAULT_CONFIG_PATH;
use wave_config::ProjectConfig;
use wave_control_plane::PlanningStatus;
use wave_control_plane::build_planning_status;
use wave_dark_factory::LintFinding;
use wave_dark_factory::has_errors;
use wave_dark_factory::lint_project;
use wave_spec::WaveDocument;
use wave_spec::load_wave_documents;

#[derive(Debug, Parser)]
#[command(name = "wave", about = "Bootstrap CLI for the Rust/Codex Wave rewrite")]
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
    Control {
        #[command(subcommand)]
        command: ControlCommand,
    },
    Draft,
    Adhoc,
    Launch,
    Autonomous,
    Dep,
    Trace,
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = config_root(&cli.config);
    let config = ProjectConfig::load(&cli.config)?;
    let waves = load_wave_documents(&config, &root)?;
    let findings = lint_project(&waves);
    let status = build_planning_status(&config, &waves, &findings);

    match cli.command {
        None => render_summary(&config, &status),
        Some(Command::Project {
            command: ProjectCommand::Show { json },
        }) => render_project(&config, json),
        Some(Command::Doctor { json }) => {
            render_doctor(&cli.config, &config, &root, &waves, &findings, json)
        }
        Some(Command::Lint { json }) => render_lint(&findings, json),
        Some(Command::Control {
            command: ControlCommand::Status { json },
        }) => render_status(&status, json),
        Some(Command::Draft) => {
            render_not_ready("draft", "wave 2 defines the typed draft/compiler contract")
        }
        Some(Command::Adhoc) => render_not_ready(
            "adhoc",
            "wave 7 reintroduces autonomous and operator queue flows",
        ),
        Some(Command::Launch) => {
            render_not_ready("launch", "wave 4 implements the Codex-backed launcher")
        }
        Some(Command::Autonomous) => {
            render_not_ready("autonomous", "wave 7 implements multi-wave scheduling")
        }
        Some(Command::Dep) => render_not_ready("dep", "dependency control arrives with wave 7"),
        Some(Command::Trace) => {
            render_not_ready("trace", "trace capture and replay arrive with wave 8")
        }
    }
}

fn config_root(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn render_summary(config: &ProjectConfig, status: &PlanningStatus) -> Result<()> {
    println!("Wave bootstrap operator shell");
    println!("project: {}", config.project_name);
    println!("mode: {}", config.default_mode);
    println!("waves dir: {}", config.waves_dir.display());
    println!(
        "next ready waves: {}",
        if status.next_ready_wave_ids.is_empty() {
            "none".to_string()
        } else {
            status
                .next_ready_wave_ids
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    println!("wave count: {}", status.waves.len());
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
        Ok(())
    }
}

fn render_doctor(
    config_path: &Path,
    config: &ProjectConfig,
    root: &Path,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    json: bool,
) -> Result<()> {
    let checks = vec![
        DoctorCheck {
            name: "config",
            ok: true,
            detail: format!("loaded {}", config_path.display()),
        },
        DoctorCheck {
            name: "waves",
            ok: !waves.is_empty(),
            detail: format!("parsed {} wave files", waves.len()),
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
            detail: format!("{} findings", findings.len()),
        },
    ];
    let report = DoctorReport {
        ok: checks.iter().all(|check| check.ok),
        checks,
    };
    if json {
        print_json(&report)
    } else {
        println!("doctor: {}", if report.ok { "ok" } else { "error" });
        for check in report.checks {
            println!(
                "- {}: {} ({})",
                check.name,
                if check.ok { "ok" } else { "error" },
                check.detail
            );
        }
        Ok(())
    }
}

fn render_lint(findings: &[LintFinding], json: bool) -> Result<()> {
    if json {
        print_json(&findings)
    } else if findings.is_empty() {
        println!("lint: ok");
        Ok(())
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
        Ok(())
    }
}

fn render_status(status: &PlanningStatus, json: bool) -> Result<()> {
    if json {
        print_json(status)
    } else {
        println!("project: {}", status.project_name);
        println!("mode: {}", status.default_mode);
        println!("has errors: {}", status.has_errors);
        println!(
            "next ready waves: {}",
            if status.next_ready_wave_ids.is_empty() {
                "none".to_string()
            } else {
                status
                    .next_ready_wave_ids
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
        for wave in &status.waves {
            println!(
                "- wave {} {} | ready={} | blocked_by={}",
                wave.id,
                wave.slug,
                wave.ready,
                if wave.blocked_by.is_empty() {
                    "none".to_string()
                } else {
                    wave.blocked_by.join(", ")
                }
            );
        }
        Ok(())
    }
}

fn render_not_ready(command: &str, note: &str) -> Result<()> {
    println!("{command}: not implemented");
    println!("{note}");
    Ok(())
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
