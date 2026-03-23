use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use wave_config::ExecutionMode;
use wave_config::ProjectConfig;
use wave_dark_factory::FindingSeverity;
use wave_dark_factory::LintFinding;
use wave_dark_factory::SkillCatalogIssue;
use wave_dark_factory::has_errors;
use wave_spec::WaveDocument;
use wave_trace::WaveRunRecord;
use wave_trace::WaveRunStatus;

const REQUIRED_CLOSURE_AGENT_IDS: [&str; 3] = ["A0", "A8", "A9"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveBlockerKind {
    Dependency,
    Lint,
    Closure,
    ActiveRun,
    AlreadyCompleted,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueReadinessState {
    Ready,
    Blocked,
    Active,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveBlockerState {
    pub kind: WaveBlockerKind,
    pub raw: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveReadinessState {
    pub state: QueueReadinessState,
    pub claimable: bool,
    pub reasons: Vec<WaveBlockerState>,
    pub primary_reason: Option<WaveBlockerState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueReadinessProjection {
    pub next_ready_wave_ids: Vec<u32>,
    pub next_ready_wave_id: Option<u32>,
    pub claimable_wave_ids: Vec<u32>,
    pub ready_wave_count: usize,
    pub blocked_wave_count: usize,
    pub active_wave_count: usize,
    pub completed_wave_count: usize,
    pub queue_ready: bool,
    pub queue_ready_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveQueueEntry {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub depends_on: Vec<u32>,
    pub blocked_by: Vec<String>,
    pub blocker_state: Vec<WaveBlockerState>,
    pub lint_errors: usize,
    pub ready: bool,
    pub agent_count: usize,
    pub implementation_agent_count: usize,
    pub closure_agent_count: usize,
    pub closure_complete: bool,
    pub required_closure_agents: Vec<String>,
    pub present_closure_agents: Vec<String>,
    pub missing_closure_agents: Vec<String>,
    pub readiness: WaveReadinessState,
    pub rerun_requested: bool,
    pub completed: bool,
    pub last_run_status: Option<WaveRunStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningStatusSummary {
    pub total_waves: usize,
    pub ready_waves: usize,
    pub blocked_waves: usize,
    pub active_waves: usize,
    pub completed_waves: usize,
    pub total_agents: usize,
    pub implementation_agents: usize,
    pub closure_agents: usize,
    pub waves_with_complete_closure: usize,
    pub waves_missing_closure: usize,
    pub total_missing_closure_agents: usize,
    pub lint_error_waves: usize,
    pub skill_catalog_issue_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillCatalogHealth {
    pub ok: bool,
    pub issue_count: usize,
    pub issues: Vec<SkillCatalogIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningStatus {
    pub project_name: String,
    pub default_mode: ExecutionMode,
    pub summary: PlanningStatusSummary,
    pub skill_catalog: SkillCatalogHealth,
    pub queue: QueueReadinessProjection,
    pub next_ready_wave_ids: Vec<u32>,
    pub waves: Vec<WaveQueueEntry>,
    pub has_errors: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveRef {
    pub id: u32,
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct WaveBlockerFlags {
    pub dependency: bool,
    pub lint: bool,
    pub closure: bool,
    pub active_run: bool,
    pub already_completed: bool,
    pub other: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveAgentCounts {
    pub total: usize,
    pub implementation: usize,
    pub closure: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveClosureStatus {
    pub complete: bool,
    pub required_agents: Vec<String>,
    pub present_agents: Vec<String>,
    pub missing_agents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WavePlanningProjection {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub depends_on: Vec<u32>,
    pub blocked_by: Vec<String>,
    pub blocker_state: Vec<WaveBlockerState>,
    pub lint_errors: usize,
    pub ready: bool,
    pub rerun_requested: bool,
    pub completed: bool,
    pub last_run_status: Option<WaveRunStatus>,
    pub agents: WaveAgentCounts,
    pub closure: WaveClosureStatus,
    pub blockers: WaveBlockerFlags,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentCountProjection {
    pub total: usize,
    pub implementation: usize,
    pub closure: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaveClosureGap {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub present_closure_agents: Vec<String>,
    pub missing_closure_agents: Vec<String>,
    pub implementation_agent_count: usize,
    pub closure_agent_count: usize,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClosureCoverageProjection {
    pub complete_wave_ids: Vec<u32>,
    pub missing_wave_ids: Vec<u32>,
    pub required_agents: usize,
    pub present_agents: usize,
    pub missing_required_agents: usize,
    pub waves: Vec<WaveClosureGap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct QueueBlockerSummary {
    pub dependency: usize,
    pub lint: usize,
    pub closure: usize,
    pub active_run: usize,
    pub already_completed: usize,
    pub other: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockedWaveProjection {
    pub id: u32,
    pub slug: String,
    pub title: String,
    pub depends_on: Vec<u32>,
    pub blocked_by: Vec<String>,
    pub blocker_state: Vec<WaveBlockerState>,
    pub lint_errors: usize,
    pub missing_closure_agents: Vec<String>,
    pub rerun_requested: bool,
    pub last_run_status: Option<WaveRunStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueProjection {
    pub ready: Vec<WaveRef>,
    pub active: Vec<WaveRef>,
    pub completed: Vec<WaveRef>,
    pub blocked: Vec<BlockedWaveProjection>,
    pub blocker_summary: QueueBlockerSummary,
    pub blocker_waves: QueueBlockerWaves,
    pub readiness: QueueReadinessProjection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct QueueBlockerWaves {
    pub dependency: Vec<WaveRef>,
    pub lint: Vec<WaveRef>,
    pub closure: Vec<WaveRef>,
    pub active_run: Vec<WaveRef>,
    pub already_completed: Vec<WaveRef>,
    pub other: Vec<WaveRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillCatalogProjection {
    pub ok: bool,
    pub issue_count: usize,
    pub issue_paths: Vec<String>,
    pub issues: Vec<SkillCatalogIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningStatusProjection {
    pub agent_counts: AgentCountProjection,
    pub closure_coverage: ClosureCoverageProjection,
    pub queue: QueueProjection,
    pub skill_catalog: SkillCatalogProjection,
    pub run: RunProjection,
    pub agents: AgentsProjection,
    pub control: ControlProjection,
    pub waves: Vec<WavePlanningProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunProjection {
    pub active_wave_ids: Vec<u32>,
    pub active_run_ids: Vec<String>,
    pub active_run_count: usize,
    pub completed_run_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentsProjection {
    pub total_agents: usize,
    pub implementation_agents: usize,
    pub closure_agents: usize,
    pub required_closure_agents: Vec<String>,
    pub present_closure_agents: Vec<String>,
    pub missing_closure_agents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ControlProjection {
    pub rerun_supported: bool,
    pub clear_rerun_supported: bool,
    pub launch_supported: bool,
    pub autonomous_supported: bool,
    pub launcher_required: bool,
    pub launcher_ready: bool,
    pub unavailable_actions: Vec<String>,
    pub unavailable_reasons: Vec<String>,
}

pub fn build_planning_status_projection(status: &PlanningStatus) -> PlanningStatusProjection {
    let mut ready = Vec::new();
    let mut active = Vec::new();
    let mut completed = Vec::new();
    let mut blocked = Vec::new();
    let mut blocker_summary = QueueBlockerSummary::default();
    let mut blocker_waves = QueueBlockerWaves::default();
    let mut active_wave_ids = Vec::new();
    let mut complete_wave_ids = Vec::new();
    let mut missing_wave_ids = Vec::new();
    let mut closure_gaps = Vec::new();
    let mut required_agents = 0;
    let mut present_agents = 0;
    let mut waves = Vec::new();

    for wave in &status.waves {
        let wave_ref = WaveRef {
            id: wave.id,
            slug: wave.slug.clone(),
            title: wave.title.clone(),
        };
        let blocker_state = classify_blockers(&wave.blocked_by);
        let blocker_flags = classify_blocker_flags(&blocker_state);

        if wave.ready {
            ready.push(wave_ref.clone());
        }

        if matches!(
            wave.last_run_status,
            Some(WaveRunStatus::Running | WaveRunStatus::Planned)
        ) {
            active.push(wave_ref.clone());
            active_wave_ids.push(wave.id);
        }

        if wave.completed {
            completed.push(wave_ref.clone());
        }

        required_agents += wave.required_closure_agents.len();
        present_agents += wave.present_closure_agents.len();

        if wave.closure_complete {
            complete_wave_ids.push(wave.id);
        } else {
            missing_wave_ids.push(wave.id);
            closure_gaps.push(WaveClosureGap {
                id: wave.id,
                slug: wave.slug.clone(),
                title: wave.title.clone(),
                present_closure_agents: wave.present_closure_agents.clone(),
                missing_closure_agents: wave.missing_closure_agents.clone(),
                implementation_agent_count: wave.implementation_agent_count,
                closure_agent_count: wave.closure_agent_count,
                blocked_by: wave.blocked_by.clone(),
            });
        }

        if !wave.ready {
            accumulate_blockers(&mut blocker_summary, &wave.blocked_by);
            accumulate_blocker_waves(&mut blocker_waves, &wave_ref, &blocker_flags);
        }

        if !wave.ready
            && !wave.completed
            && !matches!(
                wave.last_run_status,
                Some(WaveRunStatus::Running | WaveRunStatus::Planned)
            )
        {
            blocked.push(BlockedWaveProjection {
                id: wave.id,
                slug: wave.slug.clone(),
                title: wave.title.clone(),
                depends_on: wave.depends_on.clone(),
                blocked_by: wave.blocked_by.clone(),
                blocker_state: blocker_state.clone(),
                lint_errors: wave.lint_errors,
                missing_closure_agents: wave.missing_closure_agents.clone(),
                rerun_requested: wave.rerun_requested,
                last_run_status: wave.last_run_status,
            });
        }

        waves.push(WavePlanningProjection {
            id: wave.id,
            slug: wave.slug.clone(),
            title: wave.title.clone(),
            depends_on: wave.depends_on.clone(),
            blocked_by: wave.blocked_by.clone(),
            blocker_state: blocker_state.clone(),
            lint_errors: wave.lint_errors,
            ready: wave.ready,
            rerun_requested: wave.rerun_requested,
            completed: wave.completed,
            last_run_status: wave.last_run_status,
            agents: WaveAgentCounts {
                total: wave.agent_count,
                implementation: wave.implementation_agent_count,
                closure: wave.closure_agent_count,
            },
            closure: WaveClosureStatus {
                complete: wave.closure_complete,
                required_agents: wave.required_closure_agents.clone(),
                present_agents: wave.present_closure_agents.clone(),
                missing_agents: wave.missing_closure_agents.clone(),
            },
            blockers: blocker_flags,
        });
    }

    let active_run_count = active.len();
    let completed_run_count = completed.len();

    PlanningStatusProjection {
        agent_counts: AgentCountProjection {
            total: status.summary.total_agents,
            implementation: status.summary.implementation_agents,
            closure: status.summary.closure_agents,
        },
        closure_coverage: ClosureCoverageProjection {
            complete_wave_ids,
            missing_wave_ids,
            required_agents,
            present_agents,
            missing_required_agents: status.summary.total_missing_closure_agents,
            waves: closure_gaps,
        },
        queue: QueueProjection {
            ready,
            active,
            completed,
            blocked,
            blocker_summary,
            blocker_waves,
            readiness: status.queue.clone(),
        },
        skill_catalog: SkillCatalogProjection {
            ok: status.skill_catalog.ok,
            issue_count: status.skill_catalog.issue_count,
            issue_paths: status
                .skill_catalog
                .issues
                .iter()
                .map(|issue| issue.path.clone())
                .collect(),
            issues: status.skill_catalog.issues.clone(),
        },
        run: RunProjection {
            active_wave_ids: active_wave_ids.clone(),
            active_run_ids: active_wave_ids
                .iter()
                .map(|wave_id| wave_id.to_string())
                .collect(),
            active_run_count,
            completed_run_count,
        },
        agents: AgentsProjection {
            total_agents: status.summary.total_agents,
            implementation_agents: status.summary.implementation_agents,
            closure_agents: status.summary.closure_agents,
            required_closure_agents: REQUIRED_CLOSURE_AGENT_IDS
                .iter()
                .map(|agent_id| (*agent_id).to_string())
                .collect(),
            present_closure_agents: status
                .waves
                .iter()
                .flat_map(|wave| wave.present_closure_agents.clone())
                .collect(),
            missing_closure_agents: status
                .waves
                .iter()
                .flat_map(|wave| wave.missing_closure_agents.clone())
                .collect(),
        },
        control: ControlProjection {
            rerun_supported: true,
            clear_rerun_supported: true,
            launch_supported: true,
            autonomous_supported: true,
            launcher_required: true,
            launcher_ready: true,
            unavailable_actions: Vec::new(),
            unavailable_reasons: Vec::new(),
        },
        waves,
    }
}

fn accumulate_blockers(summary: &mut QueueBlockerSummary, blocked_by: &[String]) {
    for blocker in blocked_by {
        if blocker.starts_with("wave:") {
            summary.dependency += 1;
        } else if blocker == "lint:error" {
            summary.lint += 1;
        } else if blocker.starts_with("closure:") {
            summary.closure += 1;
        } else if blocker.starts_with("active-run:") {
            summary.active_run += 1;
        } else if blocker == "already-completed" {
            summary.already_completed += 1;
        } else {
            summary.other += 1;
        }
    }
}

fn classify_blockers(blocked_by: &[String]) -> Vec<WaveBlockerState> {
    blocked_by
        .iter()
        .map(|blocker| {
            if let Some(detail) = blocker.strip_prefix("wave:") {
                WaveBlockerState {
                    kind: WaveBlockerKind::Dependency,
                    raw: blocker.clone(),
                    detail: Some(detail.to_string()),
                }
            } else if blocker == "lint:error" {
                WaveBlockerState {
                    kind: WaveBlockerKind::Lint,
                    raw: blocker.clone(),
                    detail: None,
                }
            } else if let Some(detail) = blocker.strip_prefix("closure:") {
                WaveBlockerState {
                    kind: WaveBlockerKind::Closure,
                    raw: blocker.clone(),
                    detail: Some(detail.to_string()),
                }
            } else if let Some(detail) = blocker.strip_prefix("active-run:") {
                WaveBlockerState {
                    kind: WaveBlockerKind::ActiveRun,
                    raw: blocker.clone(),
                    detail: Some(detail.to_string()),
                }
            } else if blocker == "already-completed" {
                WaveBlockerState {
                    kind: WaveBlockerKind::AlreadyCompleted,
                    raw: blocker.clone(),
                    detail: None,
                }
            } else {
                WaveBlockerState {
                    kind: WaveBlockerKind::Other,
                    raw: blocker.clone(),
                    detail: Some(blocker.clone()),
                }
            }
        })
        .collect()
}

fn classify_blocker_flags(blocker_state: &[WaveBlockerState]) -> WaveBlockerFlags {
    let mut flags = WaveBlockerFlags::default();
    for blocker in blocker_state {
        match blocker.kind {
            WaveBlockerKind::Dependency => flags.dependency = true,
            WaveBlockerKind::Lint => flags.lint = true,
            WaveBlockerKind::Closure => flags.closure = true,
            WaveBlockerKind::ActiveRun => flags.active_run = true,
            WaveBlockerKind::AlreadyCompleted => flags.already_completed = true,
            WaveBlockerKind::Other => flags.other = true,
        }
    }
    flags
}

fn accumulate_blocker_waves(
    summary: &mut QueueBlockerWaves,
    wave: &WaveRef,
    flags: &WaveBlockerFlags,
) {
    if flags.dependency {
        summary.dependency.push(wave.clone());
    }
    if flags.lint {
        summary.lint.push(wave.clone());
    }
    if flags.closure {
        summary.closure.push(wave.clone());
    }
    if flags.active_run {
        summary.active_run.push(wave.clone());
    }
    if flags.already_completed {
        summary.already_completed.push(wave.clone());
    }
    if flags.other {
        summary.other.push(wave.clone());
    }
}

pub fn build_planning_status(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> PlanningStatus {
    build_planning_status_with_state(config, waves, findings, &[], latest_runs, &HashSet::new())
}

pub fn build_planning_status_with_skill_catalog(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, WaveRunRecord>,
) -> PlanningStatus {
    build_planning_status_with_state(
        config,
        waves,
        findings,
        skill_catalog_issues,
        latest_runs,
        &HashSet::new(),
    )
}

pub fn build_planning_status_with_state(
    config: &ProjectConfig,
    waves: &[WaveDocument],
    findings: &[LintFinding],
    skill_catalog_issues: &[SkillCatalogIssue],
    latest_runs: &HashMap<u32, WaveRunRecord>,
    rerun_wave_ids: &HashSet<u32>,
) -> PlanningStatus {
    let mut findings_by_wave: HashMap<u32, usize> = HashMap::new();
    for finding in findings {
        if matches!(finding.severity, FindingSeverity::Error) {
            *findings_by_wave.entry(finding.wave_id).or_default() += 1;
        }
    }

    let mut entries = Vec::new();
    for wave in waves {
        let lint_errors = findings_by_wave
            .get(&wave.metadata.id)
            .copied()
            .unwrap_or_default();
        let last_run_status = latest_runs.get(&wave.metadata.id).map(|run| run.status);
        let rerun_requested = rerun_wave_ids.contains(&wave.metadata.id);
        let completed = latest_runs
            .get(&wave.metadata.id)
            .map(WaveRunRecord::completed_successfully)
            .unwrap_or(false)
            && !rerun_requested;
        let actively_running = matches!(
            last_run_status,
            Some(WaveRunStatus::Running | WaveRunStatus::Planned)
        );
        let required_closure_agents = REQUIRED_CLOSURE_AGENT_IDS
            .iter()
            .map(|agent_id| (*agent_id).to_string())
            .collect::<Vec<_>>();
        let present_closure_agents = REQUIRED_CLOSURE_AGENT_IDS
            .iter()
            .filter(|agent_id| wave.agents.iter().any(|agent| agent.id == **agent_id))
            .map(|agent_id| (*agent_id).to_string())
            .collect::<Vec<_>>();
        let missing_closure_agents = REQUIRED_CLOSURE_AGENT_IDS
            .iter()
            .filter(|agent_id| wave.agents.iter().all(|agent| agent.id != **agent_id))
            .map(|agent_id| (*agent_id).to_string())
            .collect::<Vec<_>>();
        let closure_complete = missing_closure_agents.is_empty();

        let mut blocked_by = Vec::new();
        for dependency in &wave.metadata.depends_on {
            let Some(dep_run) = latest_runs.get(dependency) else {
                blocked_by.push(format!("wave:{dependency}:pending"));
                continue;
            };
            if !dep_run.completed_successfully() {
                blocked_by.push(format!("wave:{dependency}:{}", dep_run.status));
            }
        }
        if lint_errors > 0 {
            blocked_by.push("lint:error".to_string());
        }
        for missing_agent in &missing_closure_agents {
            blocked_by.push(format!("closure:{missing_agent}:missing"));
        }
        if completed {
            blocked_by.push("already-completed".to_string());
        }
        if actively_running {
            blocked_by.push(format!(
                "active-run:{}",
                last_run_status.unwrap_or(WaveRunStatus::Running)
            ));
        }
        let blocker_state = classify_blockers(&blocked_by);
        let readiness = classify_wave_readiness(
            lint_errors,
            closure_complete,
            completed,
            actively_running,
            &blocker_state,
        );

        let agent_count = wave.agents.len();
        let implementation_agent_count = wave.implementation_agents().count();
        let closure_agent_count = wave.closure_agents().count();
        entries.push(WaveQueueEntry {
            id: wave.metadata.id,
            slug: wave.metadata.slug.clone(),
            title: wave.metadata.title.clone(),
            depends_on: wave.metadata.depends_on.clone(),
            blocked_by,
            blocker_state: blocker_state.clone(),
            lint_errors,
            ready: readiness.claimable,
            agent_count,
            implementation_agent_count,
            closure_agent_count,
            closure_complete,
            required_closure_agents,
            present_closure_agents,
            missing_closure_agents,
            readiness,
            rerun_requested,
            completed,
            last_run_status,
        });
    }

    let next_ready_wave_ids = entries
        .iter()
        .filter(|entry| matches!(entry.readiness.state, QueueReadinessState::Ready))
        .map(|entry| entry.id)
        .collect::<Vec<_>>();
    let next_ready_wave_id = next_ready_wave_ids.first().copied();
    let claimable_wave_ids = next_ready_wave_ids.clone();

    let active_waves = entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.last_run_status,
                Some(WaveRunStatus::Running | WaveRunStatus::Planned)
            )
        })
        .count();
    let ready_waves = entries
        .iter()
        .filter(|entry| matches!(entry.readiness.state, QueueReadinessState::Ready))
        .count();
    let completed_waves = entries.iter().filter(|entry| entry.completed).count();
    let waves_missing_closure = entries
        .iter()
        .filter(|entry| !entry.closure_complete)
        .count();
    let total_missing_closure_agents = entries
        .iter()
        .map(|entry| entry.missing_closure_agents.len())
        .sum();
    let blocked_waves = entries
        .iter()
        .filter(|entry| matches!(entry.readiness.state, QueueReadinessState::Blocked))
        .count();

    let queue_ready_reason = if !next_ready_wave_ids.is_empty() {
        "ready waves are available to claim".to_string()
    } else if active_waves > 0 {
        "active waves are still running".to_string()
    } else if blocked_waves > 0 {
        "all remaining waves are blocked".to_string()
    } else {
        "queue is empty".to_string()
    };

    PlanningStatus {
        project_name: config.project_name.clone(),
        default_mode: config.default_mode,
        summary: PlanningStatusSummary {
            total_waves: entries.len(),
            ready_waves,
            blocked_waves,
            active_waves,
            completed_waves,
            total_agents: entries.iter().map(|entry| entry.agent_count).sum(),
            implementation_agents: entries
                .iter()
                .map(|entry| entry.implementation_agent_count)
                .sum(),
            closure_agents: entries.iter().map(|entry| entry.closure_agent_count).sum(),
            waves_with_complete_closure: entries.len().saturating_sub(waves_missing_closure),
            waves_missing_closure,
            total_missing_closure_agents,
            lint_error_waves: entries.iter().filter(|entry| entry.lint_errors > 0).count(),
            skill_catalog_issue_count: skill_catalog_issues.len(),
        },
        skill_catalog: SkillCatalogHealth {
            ok: skill_catalog_issues.is_empty(),
            issue_count: skill_catalog_issues.len(),
            issues: skill_catalog_issues.to_vec(),
        },
        queue: QueueReadinessProjection {
            next_ready_wave_ids: next_ready_wave_ids.clone(),
            next_ready_wave_id,
            claimable_wave_ids,
            ready_wave_count: ready_waves,
            blocked_wave_count: blocked_waves,
            active_wave_count: active_waves,
            completed_wave_count: completed_waves,
            queue_ready: next_ready_wave_id.is_some() || active_waves > 0,
            queue_ready_reason,
        },
        next_ready_wave_ids,
        waves: entries,
        has_errors: has_errors(findings) || !skill_catalog_issues.is_empty(),
    }
}

fn classify_wave_readiness(
    lint_errors: usize,
    closure_complete: bool,
    completed: bool,
    actively_running: bool,
    blocker_state: &[WaveBlockerState],
) -> WaveReadinessState {
    let claimable = lint_errors == 0
        && closure_complete
        && !completed
        && !actively_running
        && blocker_state.is_empty();
    let state = if completed {
        QueueReadinessState::Completed
    } else if actively_running {
        QueueReadinessState::Active
    } else if claimable {
        QueueReadinessState::Ready
    } else {
        QueueReadinessState::Blocked
    };
    let reasons = blocker_state.to_vec();
    let primary_reason = reasons.first().cloned();
    WaveReadinessState {
        state,
        claimable,
        reasons,
        primary_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use wave_config::DarkFactoryPolicy;
    use wave_config::LaneConfig;
    use wave_dark_factory::FindingSeverity;
    use wave_spec::CompletionLevel;
    use wave_spec::ComponentPromotion;
    use wave_spec::Context7Defaults;
    use wave_spec::DeployEnvironment;
    use wave_spec::DocImpact;
    use wave_spec::DurabilityLevel;
    use wave_spec::ExitContract;
    use wave_spec::ProofLevel;
    use wave_spec::WaveAgent;
    use wave_spec::WaveMetadata;

    #[test]
    fn dependent_wave_becomes_ready_after_successful_dependency() {
        let config = test_config();
        let waves = vec![test_wave(0, Vec::new()), test_wave(1, vec![0])];
        let findings = vec![LintFinding {
            wave_id: 0,
            severity: FindingSeverity::Warning,
            rule: "note",
            message: "noop".to_string(),
        }];

        let status_before = build_planning_status_with_skill_catalog(
            &config,
            &waves,
            &findings,
            &[],
            &HashMap::new(),
        );
        assert!(status_before.waves[0].ready);
        assert_eq!(
            status_before.waves[0].readiness.state,
            QueueReadinessState::Ready
        );
        assert!(status_before.waves[0].readiness.claimable);
        assert_eq!(status_before.queue.next_ready_wave_ids, vec![0]);
        assert_eq!(status_before.queue.next_ready_wave_id, Some(0));
        assert!(!status_before.waves[1].ready);
        assert_eq!(
            status_before.waves[1].readiness.state,
            QueueReadinessState::Blocked
        );
        assert!(!status_before.waves[1].readiness.claimable);
        assert_eq!(status_before.summary.ready_waves, 1);
        assert_eq!(status_before.summary.blocked_waves, 1);
        assert_eq!(status_before.queue.ready_wave_count, 1);
        assert_eq!(status_before.queue.blocked_wave_count, 1);
        assert_eq!(
            status_before.queue.queue_ready_reason,
            "ready waves are available to claim"
        );

        let latest_runs = HashMap::from([(
            0,
            WaveRunRecord {
                run_id: "wave-0-1".to_string(),
                wave_id: 0,
                slug: "bootstrap".to_string(),
                title: "Bootstrap".to_string(),
                status: WaveRunStatus::Succeeded,
                dry_run: false,
                bundle_dir: PathBuf::from(".wave/state/build/specs/wave-0"),
                trace_path: PathBuf::from(".wave/traces/wave-0.json"),
                codex_home: PathBuf::from(".wave/codex"),
                created_at_ms: 1,
                started_at_ms: Some(1),
                launcher_pid: None,
                completed_at_ms: Some(2),
                agents: Vec::new(),
                error: None,
            },
        )]);

        let status_after =
            build_planning_status_with_skill_catalog(&config, &waves, &[], &[], &latest_runs);
        assert!(status_after.waves[0].completed);
        assert!(status_after.waves[1].ready);
        assert_eq!(
            status_after.waves[1].readiness.state,
            QueueReadinessState::Ready
        );
        assert_eq!(status_after.summary.completed_waves, 1);
        assert_eq!(status_after.summary.ready_waves, 1);
    }

    #[test]
    fn running_wave_is_not_ready() {
        let config = test_config();
        let waves = vec![test_wave(0, Vec::new())];
        let latest_runs = HashMap::from([(
            0,
            WaveRunRecord {
                run_id: "wave-0-running".to_string(),
                wave_id: 0,
                slug: "bootstrap".to_string(),
                title: "Bootstrap".to_string(),
                status: WaveRunStatus::Running,
                dry_run: false,
                bundle_dir: PathBuf::from(".wave/state/build/specs/wave-0"),
                trace_path: PathBuf::from(".wave/traces/wave-0.json"),
                codex_home: PathBuf::from(".wave/codex"),
                created_at_ms: 1,
                started_at_ms: Some(1),
                launcher_pid: None,
                completed_at_ms: None,
                agents: Vec::new(),
                error: None,
            },
        )]);

        let status =
            build_planning_status_with_skill_catalog(&config, &waves, &[], &[], &latest_runs);
        assert!(!status.waves[0].ready);
        assert_eq!(status.waves[0].readiness.state, QueueReadinessState::Active);
        assert!(!status.waves[0].readiness.claimable);
        assert_eq!(
            status.waves[0].blocked_by,
            vec!["active-run:running".to_string()]
        );
        assert_eq!(status.summary.active_waves, 1);
        assert_eq!(status.summary.blocked_waves, 0);
        assert_eq!(status.queue.active_wave_count, 1);
        assert!(status.queue.queue_ready);
        assert_eq!(
            status.queue.queue_ready_reason,
            "active waves are still running"
        );
        assert_eq!(status.queue.next_ready_wave_id, None);
    }

    #[test]
    fn closure_gaps_and_skill_catalog_issues_are_surfaced() {
        let config = test_config();
        let mut wave = test_wave(0, Vec::new());
        wave.agents.retain(|agent| agent.id != "A9");

        let skill_catalog_issues = vec![SkillCatalogIssue {
            path: "skills/missing/skill.json".to_string(),
            message: "missing manifest".to_string(),
        }];

        let status = build_planning_status_with_skill_catalog(
            &config,
            &[wave],
            &[],
            &skill_catalog_issues,
            &HashMap::new(),
        );

        assert!(!status.waves[0].ready);
        assert_eq!(
            status.waves[0].readiness.state,
            QueueReadinessState::Blocked
        );
        assert!(!status.waves[0].readiness.claimable);
        assert_eq!(
            status.waves[0].present_closure_agents,
            vec!["A0".to_string(), "A8".to_string()]
        );
        assert_eq!(
            status.waves[0].missing_closure_agents,
            vec!["A9".to_string()]
        );
        assert_eq!(
            status.waves[0].blocked_by,
            vec!["closure:A9:missing".to_string()]
        );
        assert_eq!(status.summary.waves_missing_closure, 1);
        assert_eq!(status.summary.total_missing_closure_agents, 1);
        assert_eq!(status.summary.skill_catalog_issue_count, 1);
        assert_eq!(status.skill_catalog.issue_count, 1);
        assert!(status.has_errors);
        assert_eq!(status.queue.next_ready_wave_id, None);
    }

    #[test]
    fn planning_projection_surfaces_queue_and_closure_details() {
        let config = test_config();
        let running_wave = test_wave(0, Vec::new());
        let mut blocked_wave = test_wave(1, vec![0]);
        blocked_wave.agents.retain(|agent| agent.id != "A9");
        let completed_wave = test_wave(2, Vec::new());
        let findings = vec![LintFinding {
            wave_id: 1,
            severity: FindingSeverity::Error,
            rule: "lint",
            message: "broken prompt".to_string(),
        }];
        let latest_runs = HashMap::from([
            (
                0,
                WaveRunRecord {
                    run_id: "wave-0-running".to_string(),
                    wave_id: 0,
                    slug: "wave-0".to_string(),
                    title: "Wave 0".to_string(),
                    status: WaveRunStatus::Running,
                    dry_run: false,
                    bundle_dir: PathBuf::from(".wave/state/build/specs/wave-0"),
                    trace_path: PathBuf::from(".wave/traces/wave-0.json"),
                    codex_home: PathBuf::from(".wave/codex"),
                    created_at_ms: 1,
                    started_at_ms: Some(1),
                    launcher_pid: None,
                    completed_at_ms: None,
                    agents: Vec::new(),
                    error: None,
                },
            ),
            (
                2,
                WaveRunRecord {
                    run_id: "wave-2-succeeded".to_string(),
                    wave_id: 2,
                    slug: "wave-2".to_string(),
                    title: "Wave 2".to_string(),
                    status: WaveRunStatus::Succeeded,
                    dry_run: false,
                    bundle_dir: PathBuf::from(".wave/state/build/specs/wave-2"),
                    trace_path: PathBuf::from(".wave/traces/wave-2.json"),
                    codex_home: PathBuf::from(".wave/codex"),
                    created_at_ms: 1,
                    started_at_ms: Some(1),
                    launcher_pid: None,
                    completed_at_ms: Some(2),
                    agents: Vec::new(),
                    error: None,
                },
            ),
        ]);

        let status = build_planning_status_with_skill_catalog(
            &config,
            &[running_wave, blocked_wave, completed_wave],
            &findings,
            &[],
            &latest_runs,
        );
        let projection = build_planning_status_projection(&status);

        assert_eq!(projection.queue.ready.len(), 0);
        assert_eq!(projection.queue.active.len(), 1);
        assert_eq!(projection.queue.active[0].id, 0);
        assert_eq!(projection.queue.completed.len(), 1);
        assert_eq!(projection.queue.completed[0].id, 2);
        assert_eq!(projection.queue.blocked.len(), 1);
        assert_eq!(projection.queue.blocked[0].id, 1);
        assert_eq!(projection.queue.blocker_summary.dependency, 1);
        assert_eq!(projection.queue.blocker_summary.lint, 1);
        assert_eq!(projection.queue.blocker_summary.closure, 1);
        assert_eq!(projection.queue.blocker_summary.active_run, 1);
        assert_eq!(projection.queue.blocker_summary.already_completed, 1);
        assert_eq!(projection.queue.blocked[0].blocker_state.len(), 3);
        assert_eq!(projection.queue.blocker_waves.dependency.len(), 1);
        assert_eq!(projection.queue.blocker_waves.dependency[0].id, 1);
        assert_eq!(projection.queue.blocker_waves.lint.len(), 1);
        assert_eq!(projection.queue.blocker_waves.lint[0].id, 1);
        assert_eq!(projection.queue.blocker_waves.closure.len(), 1);
        assert_eq!(projection.queue.blocker_waves.closure[0].id, 1);
        assert_eq!(projection.queue.blocker_waves.active_run.len(), 1);
        assert_eq!(projection.queue.blocker_waves.active_run[0].id, 0);
        assert_eq!(projection.queue.blocker_waves.already_completed.len(), 1);
        assert_eq!(projection.queue.blocker_waves.already_completed[0].id, 2);
        assert_eq!(projection.queue.readiness.ready_wave_count, 0);
        assert_eq!(projection.queue.readiness.next_ready_wave_id, None);
        assert_eq!(projection.queue.readiness.blocked_wave_count, 1);
        assert_eq!(projection.queue.readiness.active_wave_count, 1);
        assert_eq!(projection.queue.readiness.completed_wave_count, 1);
        assert_eq!(projection.closure_coverage.complete_wave_ids, vec![0, 2]);
        assert_eq!(projection.closure_coverage.missing_wave_ids, vec![1]);
        assert_eq!(projection.closure_coverage.required_agents, 9);
        assert_eq!(projection.closure_coverage.present_agents, 8);
        assert_eq!(projection.closure_coverage.missing_required_agents, 1);
        assert_eq!(projection.closure_coverage.waves.len(), 1);
        assert_eq!(projection.closure_coverage.waves[0].id, 1);
        assert_eq!(
            projection.closure_coverage.waves[0].missing_closure_agents,
            vec!["A9".to_string()]
        );
        assert_eq!(projection.skill_catalog.issue_paths, Vec::<String>::new());
        assert_eq!(projection.waves.len(), 3);
        assert_eq!(projection.waves[0].id, 0);
        assert_eq!(projection.waves[0].agents.total, 4);
        assert!(projection.waves[0].blockers.active_run);
        assert_eq!(projection.waves[1].id, 1);
        assert_eq!(projection.waves[1].agents.implementation, 1);
        assert_eq!(projection.waves[1].agents.closure, 2);
        assert!(!projection.waves[1].closure.complete);
        assert_eq!(projection.waves[1].blocker_state.len(), 3);
        assert_eq!(
            projection.waves[1].closure.missing_agents,
            vec!["A9".to_string()]
        );
        assert!(projection.waves[1].blockers.dependency);
        assert!(projection.waves[1].blockers.lint);
        assert!(projection.waves[1].blockers.closure);
        assert_eq!(projection.waves[2].id, 2);
        assert!(projection.waves[2].blockers.already_completed);
    }

    fn test_config() -> ProjectConfig {
        ProjectConfig {
            version: 1,
            project_name: "Test".to_string(),
            default_lane: "main".to_string(),
            default_mode: ExecutionMode::DarkFactory,
            waves_dir: PathBuf::from("waves"),
            project_codex_home: PathBuf::from(".wave/codex"),
            state_dir: PathBuf::from(".wave/state"),
            state_runs_dir: PathBuf::from(".wave/state/runs"),
            state_control_dir: PathBuf::from(".wave/state/control"),
            trace_dir: PathBuf::from(".wave/traces"),
            trace_runs_dir: PathBuf::from(".wave/traces/runs"),
            codex_vendor_dir: PathBuf::from("third_party/codex-rs"),
            reference_wave_repo_dir: PathBuf::from("third_party/agent-wave-orchestrator"),
            dark_factory: DarkFactoryPolicy {
                require_validation: true,
                require_rollback: true,
                require_proof: true,
                require_closure: true,
            },
            lanes: BTreeMap::<String, LaneConfig>::new(),
        }
    }

    fn test_wave(id: u32, depends_on: Vec<u32>) -> WaveDocument {
        WaveDocument {
            path: PathBuf::from(format!("waves/{id:02}.md")),
            metadata: WaveMetadata {
                id,
                slug: format!("wave-{id}"),
                title: format!("Wave {id}"),
                mode: ExecutionMode::DarkFactory,
                owners: vec!["A0".to_string()],
                depends_on,
                validation: vec!["cargo test".to_string()],
                rollback: vec!["git revert".to_string()],
                proof: vec!["proof.json".to_string()],
            },
            heading_title: Some(format!("Wave {id}")),
            commit_message: Some("Feat: test".to_string()),
            component_promotions: vec![ComponentPromotion {
                component: "test".to_string(),
                target: "repo-landed".to_string(),
            }],
            deploy_environments: vec![DeployEnvironment {
                name: "repo-local".to_string(),
                detail: "custom default".to_string(),
            }],
            context7_defaults: Some(Context7Defaults {
                bundle: "rust-control-plane".to_string(),
                query: Some("Reducer state".to_string()),
            }),
            agents: vec![
                closure_agent("A0", "[wave-gate]"),
                closure_agent("A8", "[wave-integration]"),
                closure_agent("A9", "[wave-doc-closure]"),
                WaveAgent {
                    id: "A1".to_string(),
                    title: "Implementation".to_string(),
                    role_prompts: Vec::new(),
                    executor: BTreeMap::from([(
                        "profile".to_string(),
                        "implement-codex".to_string(),
                    )]),
                    context7: Some(Context7Defaults {
                        bundle: "rust-control-plane".to_string(),
                        query: Some("Reducer state and queue projections".to_string()),
                    }),
                    skills: vec!["wave-core".to_string()],
                    components: vec!["bootstrap".to_string()],
                    capabilities: vec!["queue".to_string()],
                    exit_contract: Some(ExitContract {
                        completion: CompletionLevel::Integrated,
                        durability: DurabilityLevel::Durable,
                        proof: ProofLevel::Integration,
                        doc_impact: DocImpact::Owned,
                    }),
                    deliverables: vec!["crates/wave-control-plane/src/lib.rs".to_string()],
                    file_ownership: vec!["crates/wave-control-plane/src/lib.rs".to_string()],
                    final_markers: vec![
                        "[wave-proof]".to_string(),
                        "[wave-doc-delta]".to_string(),
                        "[wave-component]".to_string(),
                    ],
                    prompt: [
                        "Primary goal:",
                        "- Land the reducer.",
                        "",
                        "Required context before coding:",
                        "- Read README.md.",
                        "",
                        "File ownership (only touch these paths):",
                        "- crates/wave-control-plane/src/lib.rs",
                    ]
                    .join("\n"),
                },
            ],
        }
    }

    fn closure_agent(id: &str, marker: &str) -> WaveAgent {
        WaveAgent {
            id: id.to_string(),
            title: "Closure".to_string(),
            role_prompts: vec![
                match id {
                    "A0" => "docs/agents/wave-cont-qa-role.md",
                    "A8" => "docs/agents/wave-integration-role.md",
                    "A9" => "docs/agents/wave-documentation-role.md",
                    _ => "docs/agents/wave-cont-eval-role.md",
                }
                .to_string(),
            ],
            executor: BTreeMap::from([("profile".to_string(), "review-codex".to_string())]),
            context7: Some(Context7Defaults {
                bundle: "none".to_string(),
                query: Some("Repository docs remain canonical".to_string()),
            }),
            skills: Vec::new(),
            components: Vec::new(),
            capabilities: Vec::new(),
            exit_contract: None,
            deliverables: Vec::new(),
            file_ownership: vec![format!(".wave/reviews/{id}.md")],
            final_markers: vec![marker.to_string()],
            prompt: [
                "Primary goal:",
                "- Close the wave honestly.",
                "",
                "Required context before coding:",
                "- Read README.md.",
                "",
                "File ownership (only touch these paths):",
                &format!("- .wave/reviews/{id}.md"),
            ]
            .join("\n"),
        }
    }
}
