//! Bootstrap interactive operator shell for the Wave workspace.
//!
//! This crate keeps the TUI thin: it reads operator snapshots, renders the
//! current state, and forwards basic rerun actions into the local runtime
//! surface. Queue and control truth stay owned by reducer-backed projection
//! helpers and arrive through the app-server snapshot rather than terminal-
//! local readiness logic.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use crossterm::event;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::execute;
use crossterm::terminal;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Cell;
use ratatui::widgets::List;
use ratatui::widgets::ListItem;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Row;
use ratatui::widgets::Table;
use ratatui::widgets::Tabs;
use std::fmt;
use std::io;
use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;
use wave_app_server::ActiveRunDetail;
use wave_app_server::OperatorSnapshot;
use wave_app_server::load_operator_snapshot;
use wave_config::ProjectConfig;
use wave_runtime::clear_rerun;
use wave_runtime::request_rerun;
use wave_trace::WaveRunStatus;

/// Stable label for the terminal-shell landing zone.
pub const TUI_LANDING_ZONE: &str = "interactive-operator-shell-bootstrap";
const NARROW_LAYOUT_THRESHOLD: u16 = 100;
const WIDE_MAIN_PERCENT: u16 = 58;
const WIDE_PANEL_PERCENT: u16 = 42;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellLayoutMode {
    Wide,
    Narrow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelTab {
    Run,
    Agents,
    Queue,
    Control,
}

impl PanelTab {
    fn all() -> [Self; 4] {
        [Self::Run, Self::Agents, Self::Queue, Self::Control]
    }

    fn title(self) -> &'static str {
        match self {
            Self::Run => "Run",
            Self::Agents => "Agents",
            Self::Queue => "Queue",
            Self::Control => "Control",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Run => Self::Agents,
            Self::Agents => Self::Queue,
            Self::Queue => Self::Control,
            Self::Control => Self::Run,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Run => Self::Control,
            Self::Agents => Self::Run,
            Self::Queue => Self::Agents,
            Self::Control => Self::Queue,
        }
    }
}

#[derive(Debug, Default)]
struct AppState {
    selected_wave_index: usize,
    flash_message: Option<String>,
}

pub fn run(root: &Path, config: &ProjectConfig) -> Result<()> {
    if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
        bail!("the Wave TUI requires an interactive terminal");
    }

    let mut stdout = io::stdout();
    terminal::enable_raw_mode().context("failed to enable raw mode")?;
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to create terminal backend")?;
    let mut app = App {
        root: root.to_path_buf(),
        config: config.clone(),
        state: AppState::default(),
        tab: PanelTab::Run,
    };

    let result = run_loop(&mut terminal, &mut app);

    terminal::disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    result
}

struct App {
    root: std::path::PathBuf,
    config: ProjectConfig,
    state: AppState,
    tab: PanelTab,
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        let snapshot = load_operator_snapshot(&app.root, &app.config)?;
        clamp_selected_wave(&mut app.state, &snapshot);

        terminal.draw(|frame| draw_ui(frame, app, &snapshot))?;

        if !event::poll(Duration::from_millis(250)).context("failed to poll terminal events")? {
            continue;
        }

        let Event::Key(key) = event::read().context("failed to read terminal event")? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Char('q') => return Ok(()),
            KeyCode::Tab => app.tab = app.tab.next(),
            KeyCode::BackTab => app.tab = app.tab.previous(),
            KeyCode::Char('j') | KeyCode::Down => select_next_wave(&mut app.state, &snapshot),
            KeyCode::Char('k') | KeyCode::Up => select_previous_wave(&mut app.state),
            KeyCode::Char('r') => handle_request_rerun(app)?,
            KeyCode::Char('c') => handle_clear_rerun(app)?,
            _ => {}
        }
    }
}

fn handle_request_rerun(app: &mut App) -> Result<()> {
    let snapshot = load_operator_snapshot(&app.root, &app.config)?;
    if let Some(wave_id) = selected_wave_id(&app.state, &snapshot) {
        request_rerun(
            &app.root,
            &app.config,
            wave_id,
            "Requested from the Wave operator TUI",
        )?;
        app.state.flash_message = Some(format!("requested rerun for wave {wave_id}"));
    }
    Ok(())
}

fn handle_clear_rerun(app: &mut App) -> Result<()> {
    let snapshot = load_operator_snapshot(&app.root, &app.config)?;
    if let Some(wave_id) = selected_wave_id(&app.state, &snapshot) {
        let result = clear_rerun(&app.root, &app.config, wave_id)?;
        app.state.flash_message = Some(match result {
            Some(_) => format!("cleared rerun for wave {wave_id}"),
            None => format!("no rerun intent for wave {wave_id}"),
        });
    }
    Ok(())
}

fn clamp_selected_wave(state: &mut AppState, snapshot: &OperatorSnapshot) {
    if snapshot.planning.waves.is_empty() {
        state.selected_wave_index = 0;
        return;
    }
    state.selected_wave_index = state
        .selected_wave_index
        .min(snapshot.planning.waves.len().saturating_sub(1));
}

fn select_next_wave(state: &mut AppState, snapshot: &OperatorSnapshot) {
    if snapshot.planning.waves.is_empty() {
        return;
    }
    state.selected_wave_index =
        (state.selected_wave_index + 1).min(snapshot.planning.waves.len() - 1);
}

fn select_previous_wave(state: &mut AppState) {
    state.selected_wave_index = state.selected_wave_index.saturating_sub(1);
}

fn selected_wave_id(state: &AppState, snapshot: &OperatorSnapshot) -> Option<u32> {
    snapshot
        .planning
        .waves
        .get(state.selected_wave_index)
        .map(|wave| wave.id)
}

fn draw_ui(frame: &mut ratatui::Frame<'_>, app: &App, snapshot: &OperatorSnapshot) {
    let area = frame.area();
    match shell_layout_mode(area.width) {
        ShellLayoutMode::Wide => draw_wide_shell(frame, area, snapshot, app),
        // Narrow terminals collapse into a single summary so the operator still
        // gets a readable, honest view instead of a broken two-column layout.
        ShellLayoutMode::Narrow => draw_narrow_shell_fallback(frame, area, snapshot, &app.state),
    }
}

fn draw_wide_shell(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    app: &App,
) {
    let (main_area, panel_area) = split_wide_shell_layout(area);
    draw_main_pane(frame, main_area, snapshot, &app.state);
    draw_right_panel(
        frame,
        panel_area,
        snapshot,
        selected_wave_id(&app.state, snapshot),
        app.tab,
        ShellLayoutMode::Wide,
    );
}

fn split_wide_shell_layout(area: Rect) -> (Rect, Rect) {
    let (main_percent, panel_percent) = wide_layout_percentages(area.width);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(main_percent),
            Constraint::Percentage(panel_percent),
        ])
        .split(area);
    (chunks[0], chunks[1])
}

fn shell_layout_mode(width: u16) -> ShellLayoutMode {
    if wide_layout_percentages(width).1 == 0 {
        ShellLayoutMode::Narrow
    } else {
        ShellLayoutMode::Wide
    }
}

fn wide_layout_percentages(width: u16) -> (u16, u16) {
    if width < NARROW_LAYOUT_THRESHOLD {
        return (0, 0);
    }
    (WIDE_MAIN_PERCENT, WIDE_PANEL_PERCENT)
}

fn draw_main_pane(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    state: &AppState,
) {
    let selected_wave = snapshot.planning.waves.get(state.selected_wave_index);
    let active_run = selected_wave.and_then(|wave| {
        snapshot
            .active_run_details
            .iter()
            .find(|run| run.wave_id == wave.id)
    });
    let mut lines = vec![
        Line::styled(
            "Conversation / logs",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::from(vec![
            Span::styled(
                snapshot.dashboard.project_name.as_str(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("ready {}", snapshot.planning.next_ready_wave_ids.len()),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  "),
            Span::styled(
                format!("completed {}", snapshot.dashboard.completed_waves),
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::raw(""),
    ];

    if let Some(wave) = selected_wave {
        lines.push(Line::from(vec![
            Span::styled("Selected wave ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} {}", wave.id, wave.title),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::raw(format!("slug: {}", wave.slug)));
        lines.push(Line::raw(format!(
            "state: {}",
            if wave.completed {
                "completed"
            } else if wave.ready {
                "ready"
            } else if wave.blocked_by.is_empty() {
                "pending"
            } else {
                "blocked"
            }
        )));
        if !wave.blocked_by.is_empty() {
            lines.push(Line::raw(format!(
                "blockers: {}",
                wave.blocked_by.join(", ")
            )));
        }
        lines.push(Line::raw(""));
        lines.extend(wave_execution_lines(wave));
    }

    lines.push(Line::raw(""));
    lines.push(Line::styled(
        "Live activity",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ));
    if let Some(active_run) = active_run {
        lines.extend(run_summary_lines(active_run));
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Current excerpt",
            Style::default().fg(Color::Gray),
        ));
        for line in active_run.activity_excerpt.lines() {
            lines.push(Line::raw(line.to_string()));
        }
    } else {
        lines.push(Line::raw("No active run for the selected wave."));
    }

    if let Some(message) = state.flash_message.as_deref() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            message.to_string(),
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Conversation / logs"),
    );
    frame.render_widget(paragraph, area);
}

fn draw_narrow_shell_fallback(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    state: &AppState,
) {
    let paragraph = Paragraph::new(narrow_summary_lines(snapshot, state)).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Operator summary"),
    );
    frame.render_widget(paragraph, area);
}

fn narrow_summary_lines<'a>(snapshot: &'a OperatorSnapshot, state: &'a AppState) -> Vec<Line<'a>> {
    let selected_wave = snapshot.planning.waves.get(state.selected_wave_index);
    let mut lines = vec![
        Line::styled(
            "Narrow terminal fallback",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::raw("Main pane: conversation and logs."),
        Line::raw("Right pane: orchestration state."),
        Line::raw(format!(
            "Layout switches to a single-pane operator summary below {} columns.",
            NARROW_LAYOUT_THRESHOLD
        )),
        Line::raw("The wide layout always keeps the right-side operator panel visible."),
        Line::raw("The summary below preserves Run, Agents, Queue, and Control truth."),
        Line::raw(""),
        Line::styled(
            "Selected wave",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if let Some(wave) = selected_wave {
        lines.push(Line::raw(format!("{} {}", wave.id, wave.title)));
        lines.push(Line::raw(format!(
            "state: {}",
            describe_wave_state(wave.completed, wave.ready, &wave.blocked_by)
        )));
        lines.push(Line::raw(format!("slug: {}", wave.slug)));
        if !wave.blocked_by.is_empty() {
            lines.push(Line::raw(format!(
                "blockers: {}",
                wave.blocked_by.join(", ")
            )));
        }
        lines.extend(wave_execution_lines(wave));
    } else {
        lines.push(Line::raw("No wave selected."));
    }

    lines.push(Line::raw(""));
    push_summary_heading(&mut lines, "Run");
    lines.push(Line::raw(format!(
        "active runs: {}  completed runs: {}",
        snapshot.panels.run.active_run_count, snapshot.panels.run.completed_run_count
    )));
    if let Some(active_run) = selected_wave
        .and_then(|wave| {
            snapshot
                .active_run_details
                .iter()
                .find(|run| run.wave_id == wave.id)
        })
        .or_else(|| snapshot.active_run_details.first())
    {
        lines.extend(run_summary_lines(active_run));
    } else {
        lines.push(Line::raw("No active runs."));
    }

    lines.push(Line::raw(""));
    push_summary_heading(&mut lines, "Agents");
    lines.push(Line::raw(format!(
        "agents: total {}  implementation {}  closure {}",
        snapshot.panels.agents.total_agents,
        snapshot.panels.agents.implementation_agents,
        snapshot.panels.agents.closure_agents
    )));
    lines.push(Line::raw(format!(
        "closure coverage: present {}  missing {}",
        snapshot.panels.agents.present_closure_agents.len(),
        format_string_list(&snapshot.panels.agents.missing_closure_agents)
    )));
    if let Some(active_run) = selected_wave
        .and_then(|wave| {
            snapshot
                .active_run_details
                .iter()
                .find(|run| run.wave_id == wave.id)
        })
        .or_else(|| snapshot.active_run_details.first())
    {
        for agent in &active_run.agents {
            lines.push(Line::raw(format!(
                "{} {} | {} | proof {}",
                agent.id,
                agent.title,
                agent.status,
                if agent.proof_complete {
                    "complete".to_string()
                } else if agent.missing_markers.is_empty() {
                    "pending".to_string()
                } else {
                    format!("missing {}", agent.missing_markers.join(", "))
                }
            )));
        }
    } else {
        lines.push(Line::raw("No live agent rows."));
    }

    lines.push(Line::raw(""));
    push_summary_heading(&mut lines, "Queue");
    lines.extend(queue_decision_lines(snapshot));
    lines.push(Line::raw(format!(
        "ready: {}  active: {}  blocked: {}",
        format_u32_list(&snapshot.panels.queue.ready_wave_ids),
        format_u32_list(&snapshot.panels.queue.active_wave_ids),
        format_u32_list(&snapshot.panels.queue.blocked_wave_ids)
    )));
    lines.extend(closure_attention_lines(snapshot));
    lines.extend(skill_issue_lines(snapshot));

    lines.push(Line::raw(""));
    push_summary_heading(&mut lines, "Control");
    lines.push(Line::raw(format!(
        "actions: rerun={} clear-rerun={} launch={} autonomous={}",
        yes_no(snapshot.panels.control.rerun_supported),
        yes_no(snapshot.panels.control.clear_rerun_supported),
        yes_no(snapshot.panels.control.launch_supported),
        yes_no(snapshot.panels.control.autonomous_supported)
    )));
    lines.push(Line::raw(format!(
        "rerun intents: {}",
        if snapshot.rerun_intents.is_empty() {
            "none".to_string()
        } else {
            snapshot
                .rerun_intents
                .iter()
                .map(|intent| format!("wave {} {}", intent.wave_id, intent.reason))
                .collect::<Vec<_>>()
                .join("; ")
        }
    )));
    if !snapshot.panels.control.unavailable_reasons.is_empty() {
        lines.push(Line::raw(format!(
            "unavailable: {}",
            snapshot.panels.control.unavailable_reasons.join("; ")
        )));
    }
    lines.push(Line::raw("keys: Tab Shift+Tab j k r c q"));

    if let Some(message) = state.flash_message.as_deref() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            message,
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ));
    }
    lines
}

fn describe_wave_state(completed: bool, ready: bool, blocked_by: &[String]) -> &'static str {
    if completed {
        "completed"
    } else if ready {
        "ready"
    } else if blocked_by.is_empty() {
        "pending"
    } else {
        "blocked"
    }
}

fn push_summary_heading<'a>(lines: &mut Vec<Line<'a>>, title: &'static str) {
    lines.push(Line::styled(
        title,
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    ));
}

fn format_u32_list(values: &[u32]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .map(|value| value.to_string())
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

fn wave_execution_lines(wave: &wave_control_plane::WaveStatusReadModel) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        "Execution",
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(worktree) = &wave.execution.worktree {
        lines.push(Line::raw(format!(
            "worktree: {} ({})",
            worktree.path,
            debug_label(worktree.state)
        )));
    } else {
        lines.push(Line::raw("worktree: none"));
    }
    if let Some(promotion) = &wave.execution.promotion {
        lines.push(Line::raw(format!(
            "promotion: {}",
            debug_label(promotion.state)
        )));
        if !promotion.conflict_paths.is_empty() {
            lines.push(Line::raw(format!(
                "promotion conflicts: {}",
                promotion.conflict_paths.join(", ")
            )));
        }
        if let Some(detail) = promotion.detail.as_deref() {
            lines.push(Line::raw(format!("promotion detail: {detail}")));
        }
    } else {
        lines.push(Line::raw("promotion: none"));
    }
    if let Some(scheduling) = &wave.execution.scheduling {
        lines.push(Line::raw(format!(
            "scheduler: {}/{} fairness={} protected={} preemptible={}",
            debug_label(scheduling.phase),
            debug_label(scheduling.state),
            scheduling.fairness_rank,
            yes_no(scheduling.protected_closure_capacity),
            yes_no(scheduling.preemptible)
        )));
        if let Some(decision) = scheduling.last_decision.as_deref() {
            lines.push(Line::raw(format!("decision: {decision}")));
        }
    } else {
        lines.push(Line::raw("scheduler: none"));
    }
    lines.push(Line::raw(format!(
        "merge blocked: {}  closure blocked: {}",
        yes_no(wave.execution.merge_blocked),
        yes_no(wave.execution.closure_blocked_by_promotion)
    )));
    lines.push(Line::raw(format!(
        "budget: reserved_closure={} reserved_now={} preemption={}",
        wave.ownership
            .budget
            .reserved_closure_task_leases
            .map(|count| count.to_string())
            .unwrap_or_else(|| "none".to_string()),
        yes_no(wave.ownership.budget.closure_capacity_reserved),
        yes_no(wave.ownership.budget.preemption_enabled)
    )));
    lines
}

fn queue_decision_lines(snapshot: &OperatorSnapshot) -> Vec<Line<'static>> {
    snapshot
        .control_status
        .queue_decision
        .lines
        .iter()
        .cloned()
        .map(Line::raw)
        .collect()
}

fn closure_attention_lines(snapshot: &OperatorSnapshot) -> Vec<Line<'static>> {
    snapshot
        .control_status
        .closure_attention_lines
        .iter()
        .cloned()
        .map(Line::raw)
        .collect()
}

fn skill_issue_lines(snapshot: &OperatorSnapshot) -> Vec<Line<'static>> {
    snapshot
        .control_status
        .skill_issue_lines
        .iter()
        .cloned()
        .map(Line::raw)
        .collect()
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn run_summary_lines(run: &ActiveRunDetail) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::raw(format!("run: {}", run.run_id)),
        Line::raw(format!("wave: {} {}", run.wave_id, run.wave_title)),
        Line::from(vec![
            Span::raw("status: "),
            status_span(run.status),
            Span::raw("  "),
            Span::styled(
                format!(
                    "proof {}/{}",
                    run.proof.completed_agents, run.proof.total_agents
                ),
                Style::default().fg(if run.proof.complete {
                    Color::Green
                } else {
                    Color::Yellow
                }),
            ),
        ]),
    ];
    if let Some(agent_id) = run.current_agent_id.as_deref() {
        lines.push(Line::raw(format!(
            "current agent: {} {}",
            agent_id,
            run.current_agent_title.as_deref().unwrap_or("")
        )));
    }
    if let Some(elapsed_ms) = run.elapsed_ms {
        lines.push(Line::raw(format!("elapsed: {}", HumanDuration(elapsed_ms))));
    }
    if !run.replay.ok {
        lines.push(Line::styled(
            format!("replay issues: {}", run.replay.issues.len()),
            Style::default().fg(Color::Red),
        ));
    }
    lines
}

fn debug_label(value: impl fmt::Debug) -> String {
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

fn draw_right_panel(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
    tab: PanelTab,
    mode: ShellLayoutMode,
) {
    let panel_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let titles = PanelTab::all()
        .into_iter()
        .map(|candidate| Line::from(candidate.title()))
        .collect::<Vec<_>>();
    let selected_index = PanelTab::all()
        .iter()
        .position(|candidate| *candidate == tab)
        .unwrap_or_default();
    let tabs = Tabs::new(titles)
        .select(selected_index)
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL).title(match mode {
            ShellLayoutMode::Narrow => "Orchestration stack",
            ShellLayoutMode::Wide => "Operator panel",
        }));
    frame.render_widget(tabs, panel_chunks[0]);

    match tab {
        PanelTab::Run => draw_run_tab(frame, panel_chunks[1], snapshot, selected_wave_id),
        PanelTab::Agents => draw_agents_tab(frame, panel_chunks[1], snapshot, selected_wave_id),
        PanelTab::Queue => draw_queue_tab(frame, panel_chunks[1], snapshot),
        PanelTab::Control => draw_control_tab(frame, panel_chunks[1], snapshot, selected_wave_id),
    }
}

fn draw_run_tab(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) {
    let selected_wave = selected_wave_id.and_then(|wave_id| {
        snapshot
            .planning
            .waves
            .iter()
            .find(|wave| wave.id == wave_id)
    });
    let mut lines = Vec::new();
    if let Some(run) = selected_active_run(snapshot, selected_wave_id) {
        lines.extend(run_summary_lines(run));
        lines.push(Line::raw(""));
        lines.push(Line::raw(format!(
            "declared proof artifacts: {}",
            run.proof.declared_artifacts.len()
        )));
        for artifact in &run.proof.declared_artifacts {
            lines.push(Line::from(vec![
                Span::raw("- "),
                Span::raw(&artifact.path),
                Span::raw(" "),
                Span::styled(
                    if artifact.exists {
                        "present"
                    } else {
                        "missing"
                    },
                    Style::default().fg(if artifact.exists {
                        Color::Green
                    } else {
                        Color::Red
                    }),
                ),
            ]));
        }
    } else {
        lines.push(Line::raw("No active runs."));
    }
    if let Some(wave) = selected_wave {
        lines.push(Line::raw(""));
        lines.extend(wave_execution_lines(wave));
    }

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Run")),
        area,
    );
}

fn draw_agents_tab(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) {
    let Some(run) = selected_active_run(snapshot, selected_wave_id) else {
        frame.render_widget(
            Paragraph::new("No active runs.")
                .block(Block::default().borders(Borders::ALL).title("Agents")),
            area,
        );
        return;
    };

    let rows = run.agents.iter().map(|agent| {
        Row::new(vec![
            Cell::from(agent.id.clone()),
            Cell::from(agent.title.clone()),
            Cell::from(agent.status.to_string()),
            Cell::from(if agent.proof_complete {
                "complete".to_string()
            } else if agent.missing_markers.is_empty() {
                "pending".to_string()
            } else {
                format!("missing {}", agent.missing_markers.join(", "))
            }),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Percentage(35),
            Constraint::Length(10),
            Constraint::Percentage(45),
        ],
    )
    .header(
        Row::new(vec!["Id", "Title", "State", "Proof"]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(Block::default().borders(Borders::ALL).title("Agents"));
    frame.render_widget(table, area);
}

fn draw_queue_tab(frame: &mut ratatui::Frame<'_>, area: Rect, snapshot: &OperatorSnapshot) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(0)])
        .split(area);

    let summary = Paragraph::new(queue_decision_lines(snapshot)).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Queue decision"),
    );
    frame.render_widget(summary, chunks[0]);

    let rows = queue_table_rows(snapshot)
        .into_iter()
        .map(|(id, title, queue_state)| {
            Row::new(vec![
                Cell::from(id),
                Cell::from(title),
                Cell::from(queue_state),
            ])
        });

    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Percentage(42),
            Constraint::Percentage(54),
        ],
    )
    .header(
        Row::new(vec!["Id", "Wave", "Queue state"]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(Block::default().borders(Borders::ALL).title("Queue"));
    frame.render_widget(table, chunks[1]);
}

fn draw_control_tab(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(5),
            Constraint::Length(8),
        ])
        .split(area);

    let rerun_items = if snapshot.rerun_intents.is_empty() {
        vec![ListItem::new("No rerun intents.")]
    } else {
        snapshot
            .rerun_intents
            .iter()
            .map(|intent| {
                ListItem::new(format!(
                    "wave {} {}: {}",
                    intent.wave_id,
                    match intent.status {
                        wave_runtime::RerunIntentStatus::Requested => "requested",
                        wave_runtime::RerunIntentStatus::Cleared => "cleared",
                    },
                    intent.reason
                ))
            })
            .collect()
    };
    frame.render_widget(
        List::new(rerun_items).block(Block::default().borders(Borders::ALL).title("Reruns")),
        chunks[0],
    );

    let status_items = control_status_items(snapshot, selected_wave_id)
        .into_iter()
        .map(ListItem::new)
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(status_items).block(Block::default().borders(Borders::ALL).title("Status")),
        chunks[1],
    );

    let action_items = snapshot
        .panels
        .control
        .actions
        .iter()
        .map(|action| {
            ListItem::new(format!(
                "{}  {}  {}",
                action.key, action.label, action.description
            ))
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(action_items).block(Block::default().borders(Borders::ALL).title("Actions")),
        chunks[2],
    );
}

fn selected_active_run(
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) -> Option<&ActiveRunDetail> {
    selected_wave_id
        .and_then(|wave_id| {
            snapshot
                .active_run_details
                .iter()
                .find(|run| run.wave_id == wave_id)
        })
        .or_else(|| snapshot.active_run_details.first())
}

fn queue_table_rows(snapshot: &OperatorSnapshot) -> Vec<(String, String, String)> {
    snapshot
        .panels
        .queue
        .waves
        .iter()
        .map(|wave| {
            (
                wave.id.to_string(),
                wave.title.clone(),
                wave.queue_state.clone(),
            )
        })
        .collect()
}

fn control_status_items(snapshot: &OperatorSnapshot, selected_wave_id: Option<u32>) -> Vec<String> {
    let mut items = selected_active_run(snapshot, selected_wave_id)
        .map(|run| {
            if run.replay.ok {
                vec![format!(
                    "Replay OK for wave {} run {}",
                    run.wave_id, run.run_id
                )]
            } else {
                run.replay
                    .issues
                    .iter()
                    .map(|issue| format!("{}: {}", issue.kind, issue.detail))
                    .collect::<Vec<_>>()
            }
        })
        .unwrap_or_else(|| vec!["No active replay state.".to_string()]);
    items.extend(
        snapshot
            .control_status
            .closure_attention_lines
            .iter()
            .cloned(),
    );
    items.extend(snapshot.control_status.skill_issue_lines.iter().cloned());
    items
}

#[cfg(test)]
fn right_panel_ratio(width: u16) -> (u16, u16) {
    wide_layout_percentages(width)
}

fn status_span(status: WaveRunStatus) -> Span<'static> {
    let style = match status {
        WaveRunStatus::Planned => Style::default().fg(Color::Gray),
        WaveRunStatus::Running => Style::default().fg(Color::Cyan),
        WaveRunStatus::Succeeded => Style::default().fg(Color::Green),
        WaveRunStatus::Failed => Style::default().fg(Color::Red),
        WaveRunStatus::DryRun => Style::default().fg(Color::Yellow),
    };
    Span::styled(status.to_string(), style.add_modifier(Modifier::BOLD))
}

struct HumanDuration(u128);

impl fmt::Display for HumanDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total_seconds = self.0 / 1000;
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        if minutes > 0 {
            write!(f, "{}m{}s", minutes, seconds)
        } else {
            write!(f, "{}s", seconds)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wave_control_plane::ControlStatusReadModel;
    use wave_control_plane::PlanningStatusReadModel;
    use wave_control_plane::PlanningStatusSummary;
    use wave_control_plane::QueueBlockerSummary;
    use wave_control_plane::QueueDecisionReadModel;
    use wave_control_plane::QueueReadinessReadModel;
    use wave_control_plane::QueueReadinessStateReadModel;
    use wave_control_plane::SkillCatalogHealth;
    use wave_control_plane::WaveReadinessReadModel;
    use wave_control_plane::WaveStatusReadModel;

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
    fn wide_layout_keeps_right_side_panel() {
        assert_eq!(shell_layout_mode(140), ShellLayoutMode::Wide);
        assert_eq!(wide_layout_percentages(140), (58, 42));
        let (main, panel) = split_wide_shell_layout(Rect::new(0, 0, 140, 40));
        assert!(panel.x > main.x);
        assert_eq!(main.height, panel.height);
        assert_eq!(main.width + panel.width, 140);
        assert!(panel.width >= 58);
        assert_eq!(
            right_panel_ratio(140),
            (WIDE_MAIN_PERCENT, WIDE_PANEL_PERCENT)
        );
    }

    #[test]
    fn narrow_layout_switches_to_single_pane_summary() {
        assert_eq!(shell_layout_mode(80), ShellLayoutMode::Narrow);
        assert_eq!(right_panel_ratio(80), (0, 0));
    }

    #[test]
    fn narrow_summary_preserves_operator_truth_sections() {
        let snapshot = test_snapshot();
        let state = AppState::default();
        let lines = narrow_summary_lines(&snapshot, &state);
        let rendered = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("Run"));
        assert!(rendered.contains("Agents"));
        assert!(rendered.contains("Queue"));
        assert!(rendered.contains("Control"));
        assert!(rendered.contains("ready: 6"));
        assert!(rendered.contains("A1 TUI Shell And Layout Scaffold"));
        assert!(rendered.contains("worktree: .wave/state/worktrees/wave-05-test (allocated)"));
        assert!(rendered.contains("promotion: ready"));
        assert!(rendered.contains("budget: reserved_closure=1 reserved_now=yes preemption=yes"));
        assert!(rendered.contains("actions: rerun=yes clear-rerun=yes launch=yes autonomous=yes"));
    }

    #[test]
    fn selected_run_prefers_the_selected_wave_over_the_first_active_run() {
        let mut snapshot = test_snapshot();
        let second_run = ActiveRunDetail {
            wave_id: 6,
            wave_slug: "dark-factory-enforcement".to_string(),
            wave_title: "Make dark-factory an enforced execution profile".to_string(),
            run_id: "wave-06-test".to_string(),
            status: WaveRunStatus::Running,
            created_at_ms: 3,
            started_at_ms: Some(4),
            elapsed_ms: Some(5_000),
            current_agent_id: Some("A2".to_string()),
            current_agent_title: Some("Status Bindings And Control Subscriptions".to_string()),
            activity_excerpt: "No live agent output yet.".to_string(),
            proof: wave_app_server::ProofSnapshot {
                declared_artifacts: vec![wave_app_server::ProofArtifactStatus {
                    path: "crates/wave-runtime/src/lib.rs".to_string(),
                    exists: true,
                }],
                complete: false,
                proof_source: "compatibility-adapter".to_string(),
                completed_agents: 0,
                envelope_backed_agents: 0,
                compatibility_backed_agents: 6,
                total_agents: 6,
            },
            replay: wave_trace::ReplayReport {
                run_id: "wave-06-test".to_string(),
                wave_id: 6,
                ok: true,
                issues: Vec::new(),
            },
            agents: Vec::new(),
        };
        snapshot
            .planning
            .waves
            .push(wave_control_plane::WaveStatusReadModel {
                id: 6,
                slug: "dark-factory-enforcement".to_string(),
                title: "Make dark-factory an enforced execution profile".to_string(),
                depends_on: vec![2, 3, 4],
                blocked_by: Vec::new(),
                blocker_state: Vec::new(),
                lint_errors: 0,
                ready: true,
                ownership: empty_ownership(),
                execution: empty_execution(),
                agent_count: 6,
                implementation_agent_count: 3,
                closure_agent_count: 3,
                closure_complete: true,
                required_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                present_closure_agents: vec!["A0".to_string(), "A8".to_string(), "A9".to_string()],
                missing_closure_agents: Vec::new(),
                rerun_requested: false,
                completed: false,
                last_run_status: Some(WaveRunStatus::Running),
                readiness: WaveReadinessReadModel {
                    state: QueueReadinessStateReadModel::Active,
                    planning_ready: false,
                    claimable: false,
                    reasons: vec![wave_control_plane::QueueBlockerReadModel {
                        kind: wave_control_plane::QueueBlockerKindReadModel::ActiveRun,
                        raw: "active-run:running".to_string(),
                        detail: Some("wave is already active".to_string()),
                    }],
                    primary_reason: Some(wave_control_plane::QueueBlockerReadModel {
                        kind: wave_control_plane::QueueBlockerKindReadModel::ActiveRun,
                        raw: "active-run:running".to_string(),
                        detail: Some("wave is already active".to_string()),
                    }),
                },
            });
        snapshot.active_run_details.push(second_run.clone());
        snapshot.panels.run.active_runs.push(second_run);
        snapshot.panels.run.active_wave_ids.push(6);
        snapshot
            .panels
            .run
            .active_run_ids
            .push("wave-06-test".to_string());
        snapshot.panels.run.active_run_count = 2;

        assert_eq!(
            selected_active_run(&snapshot, Some(6)).map(|run| run.wave_id),
            Some(6)
        );
        assert_eq!(
            selected_active_run(&snapshot, Some(99)).map(|run| run.wave_id),
            Some(5)
        );
    }

    #[test]
    fn queue_story_comes_from_snapshot_control_status() {
        let mut snapshot = test_snapshot();
        snapshot.control_status.queue_decision.lines = vec![
            "queue decision: next claimable wave=custom".to_string(),
            "queue decision: claimable waves=custom".to_string(),
        ];

        let rendered = queue_decision_lines(&snapshot)
            .into_iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            rendered,
            vec![
                "queue decision: next claimable wave=custom".to_string(),
                "queue decision: claimable waves=custom".to_string(),
            ]
        );
    }

    #[test]
    fn queue_rows_come_from_snapshot_queue_panel() {
        let mut snapshot = test_snapshot();
        snapshot.panels.queue.waves = vec![wave_app_server::QueuePanelWaveSnapshot {
            id: 42,
            slug: "queue-row".to_string(),
            title: "Queue Row".to_string(),
            queue_state: "blocked".to_string(),
            blocked: true,
        }];

        assert_eq!(
            queue_table_rows(&snapshot),
            vec![(
                "42".to_string(),
                "Queue Row".to_string(),
                "blocked".to_string()
            )]
        );
    }

    #[test]
    fn queue_rows_preserve_projection_owned_active_label() {
        let snapshot = test_snapshot();

        assert_eq!(
            queue_table_rows(&snapshot),
            vec![(
                "5".to_string(),
                "Build the right-side operator panel in the TUI".to_string(),
                "active".to_string()
            )]
        );
    }

    #[test]
    fn control_items_come_from_snapshot_control_payload() {
        let mut snapshot = test_snapshot();
        snapshot.control_status.closure_attention_lines = vec!["closure gap: custom".to_string()];
        snapshot.control_status.skill_issue_lines = vec!["skill issue: custom".to_string()];

        let rendered = control_status_items(&snapshot, Some(5));

        assert!(
            rendered
                .iter()
                .any(|line| line == "Replay OK for wave 5 run wave-05-test")
        );
        assert!(rendered.iter().any(|line| line == "closure gap: custom"));
        assert!(rendered.iter().any(|line| line == "skill issue: custom"));
    }

    #[test]
    fn layout_mode_switches_at_threshold() {
        assert_eq!(
            shell_layout_mode(NARROW_LAYOUT_THRESHOLD - 1),
            ShellLayoutMode::Narrow
        );
        assert_eq!(
            shell_layout_mode(NARROW_LAYOUT_THRESHOLD),
            ShellLayoutMode::Wide
        );
        assert_eq!(wide_layout_percentages(NARROW_LAYOUT_THRESHOLD - 1), (0, 0));
    }

    fn test_snapshot() -> OperatorSnapshot {
        use wave_app_server::AgentPanelItem;
        use wave_app_server::AgentsPanelSnapshot;
        use wave_app_server::ControlAction;
        use wave_app_server::ControlPanelSnapshot;
        use wave_app_server::DashboardSnapshot;
        use wave_app_server::LauncherStatus;
        use wave_app_server::OperatorPanelsSnapshot;
        use wave_app_server::ProofArtifactStatus;
        use wave_app_server::ProofSnapshot;
        use wave_app_server::QueuePanelSnapshot;
        use wave_app_server::QueuePanelWaveSnapshot;
        use wave_app_server::RunPanelSnapshot;
        use wave_runtime::RerunIntentRecord;
        use wave_runtime::RerunIntentStatus;

        let active_run = ActiveRunDetail {
            wave_id: 5,
            wave_slug: "tui-right-panel".to_string(),
            wave_title: "Build the right-side operator panel in the TUI".to_string(),
            run_id: "wave-05-test".to_string(),
            status: WaveRunStatus::Running,
            created_at_ms: 1,
            started_at_ms: Some(2),
            elapsed_ms: Some(45_000),
            current_agent_id: Some("A1".to_string()),
            current_agent_title: Some("TUI Shell And Layout Scaffold".to_string()),
            activity_excerpt: "No live agent output yet.".to_string(),
            proof: ProofSnapshot {
                declared_artifacts: vec![ProofArtifactStatus {
                    path: "crates/wave-tui/src/lib.rs".to_string(),
                    exists: true,
                }],
                complete: false,
                proof_source: "compatibility-adapter".to_string(),
                completed_agents: 1,
                envelope_backed_agents: 0,
                compatibility_backed_agents: 6,
                total_agents: 6,
            },
            replay: wave_trace::ReplayReport {
                run_id: "wave-05-test".to_string(),
                wave_id: 5,
                ok: true,
                issues: Vec::new(),
            },
            agents: vec![
                AgentPanelItem {
                    id: "A1".to_string(),
                    title: "TUI Shell And Layout Scaffold".to_string(),
                    status: WaveRunStatus::Running,
                    current_task: "TUI Shell And Layout Scaffold".to_string(),
                    proof_complete: false,
                    proof_source: "compatibility-adapter".to_string(),
                    expected_markers: vec!["[wave-proof]".to_string()],
                    observed_markers: Vec::new(),
                    missing_markers: vec!["[wave-proof]".to_string()],
                    deliverables: vec!["crates/wave-tui/src/lib.rs".to_string()],
                    error: None,
                },
                AgentPanelItem {
                    id: "A8".to_string(),
                    title: "Integration Steward".to_string(),
                    status: WaveRunStatus::Planned,
                    current_task: "Integration Steward".to_string(),
                    proof_complete: false,
                    proof_source: "compatibility-adapter".to_string(),
                    expected_markers: vec!["[wave-integration]".to_string()],
                    observed_markers: Vec::new(),
                    missing_markers: vec!["[wave-integration]".to_string()],
                    deliverables: Vec::new(),
                    error: None,
                },
            ],
        };

        OperatorSnapshot {
            generated_at_ms: 1,
            dashboard: DashboardSnapshot {
                project_name: "Codex Wave Mode".to_string(),
                next_ready_wave_ids: vec![6],
                active_runs: Vec::new(),
                total_waves: 10,
                completed_waves: 5,
            },
            latest_run_details: vec![active_run.clone()],
            planning: PlanningStatusReadModel {
                project_name: "Codex Wave Mode".to_string(),
                default_mode: wave_config::ExecutionMode::DarkFactory,
                summary: PlanningStatusSummary {
                    total_waves: 10,
                    ready_waves: 1,
                    blocked_waves: 4,
                    active_waves: 1,
                    completed_waves: 5,
                    total_agents: 60,
                    implementation_agents: 30,
                    closure_agents: 30,
                    waves_with_complete_closure: 10,
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
                next_ready_wave_ids: vec![6],
                queue: QueueReadinessReadModel {
                    next_ready_wave_ids: vec![6],
                    next_ready_wave_id: Some(6),
                    claimable_wave_ids: vec![6],
                    claimed_wave_ids: Vec::new(),
                    ready_wave_count: 1,
                    claimed_wave_count: 0,
                    blocked_wave_count: 3,
                    active_wave_count: 1,
                    completed_wave_count: 5,
                    queue_ready: true,
                    queue_ready_reason: "ready waves are available to claim".to_string(),
                },
                waves: vec![WaveStatusReadModel {
                    id: 5,
                    slug: "tui-right-panel".to_string(),
                    title: "Build the right-side operator panel in the TUI".to_string(),
                    depends_on: vec![3, 4],
                    blocked_by: vec!["active-run:running".to_string()],
                    blocker_state: Vec::new(),
                    lint_errors: 0,
                    ready: false,
                    ownership: wave_control_plane::WaveOwnershipState {
                        budget: wave_control_plane::SchedulerBudgetState {
                            reserved_closure_task_leases: Some(1),
                            active_implementation_task_leases: 1,
                            active_closure_task_leases: 0,
                            closure_capacity_reserved: true,
                            preemption_enabled: true,
                            ..empty_ownership().budget
                        },
                        ..empty_ownership()
                    },
                    execution: wave_control_plane::WaveExecutionState {
                        worktree: Some(wave_domain::WaveWorktreeRecord {
                            worktree_id: wave_domain::WaveWorktreeId::new(
                                "worktree-wave-05-test".to_string(),
                            ),
                            wave_id: 5,
                            path: ".wave/state/worktrees/wave-05-test".to_string(),
                            base_ref: "HEAD".to_string(),
                            snapshot_ref: "snapshot-wave-05".to_string(),
                            branch_ref: Some("wave/05/test".to_string()),
                            shared_scope: wave_domain::WaveWorktreeScope::Wave,
                            state: wave_domain::WaveWorktreeState::Allocated,
                            allocated_at_ms: 1,
                            released_at_ms: None,
                            detail: Some("shared wave worktree".to_string()),
                        }),
                        promotion: Some(wave_domain::WavePromotionRecord {
                            promotion_id: wave_domain::WavePromotionId::new(
                                "promotion-wave-05-test".to_string(),
                            ),
                            wave_id: 5,
                            worktree_id: Some(wave_domain::WaveWorktreeId::new(
                                "worktree-wave-05-test".to_string(),
                            )),
                            state: wave_domain::WavePromotionState::Ready,
                            target_ref: "HEAD".to_string(),
                            snapshot_ref: "snapshot-wave-05".to_string(),
                            candidate_ref: Some("refs/wave/05/test".to_string()),
                            candidate_tree: Some("abc123".to_string()),
                            conflict_paths: Vec::new(),
                            detail: Some("promotion candidate recorded".to_string()),
                            checked_at_ms: 2,
                            completed_at_ms: Some(3),
                        }),
                        scheduling: Some(wave_domain::WaveSchedulingRecord {
                            wave_id: 5,
                            phase: wave_domain::WaveExecutionPhase::Closure,
                            priority: wave_domain::WaveSchedulerPriority::Closure,
                            state: wave_domain::WaveSchedulingState::Protected,
                            fairness_rank: 1,
                            waiting_since_ms: Some(2),
                            protected_closure_capacity: true,
                            preemptible: false,
                            last_decision: Some(
                                "closure capacity reserved before A8".to_string(),
                            ),
                            updated_at_ms: 3,
                        }),
                        merge_blocked: false,
                        closure_blocked_by_promotion: false,
                    },
                    agent_count: 6,
                    implementation_agent_count: 3,
                    closure_agent_count: 3,
                    closure_complete: true,
                    required_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    present_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    missing_closure_agents: Vec::new(),
                    rerun_requested: false,
                    completed: false,
                    last_run_status: Some(WaveRunStatus::Running),
                    readiness: WaveReadinessReadModel {
                        state: QueueReadinessStateReadModel::Active,
                        planning_ready: false,
                        claimable: false,
                        reasons: vec![wave_control_plane::QueueBlockerReadModel {
                            kind: wave_control_plane::QueueBlockerKindReadModel::ActiveRun,
                            raw: "active-run:running".to_string(),
                            detail: Some("wave is already active".to_string()),
                        }],
                        primary_reason: Some(wave_control_plane::QueueBlockerReadModel {
                            kind: wave_control_plane::QueueBlockerKindReadModel::ActiveRun,
                            raw: "active-run:running".to_string(),
                            detail: Some("wave is already active".to_string()),
                        }),
                    },
                }],
                has_errors: false,
            },
            control_status: ControlStatusReadModel {
                queue_decision: QueueDecisionReadModel {
                    next_claimable_wave_id: Some(6),
                    claimable_wave_ids: vec![6],
                    claimed_wave_ids: Vec::new(),
                    queue_ready_reason: "ready waves are available to claim".to_string(),
                    blocker_summary: QueueBlockerSummary {
                        dependency: 3,
                        lint: 0,
                        closure: 0,
                        ownership: 0,
                        lease_expired: 0,
                        budget: 0,
                        active_run: 1,
                        already_completed: 5,
                        other: 0,
                    },
                    closure_blocked: Vec::new(),
                    lines: vec![
                        "queue decision: next claimable wave=6".to_string(),
                        "queue decision: claimable waves=6".to_string(),
                        "queue decision: claimed waves=none".to_string(),
                        "queue decision: queue ready reason=ready waves are available to claim"
                            .to_string(),
                        "queue decision: blocker story dependency=3 lint=0 closure=0 ownership=0 lease_expired=0 budget=0 active_run=1"
                            .to_string(),
                        "queue decision: closure-blocked=none".to_string(),
                    ],
                },
                closure_attention_lines: Vec::new(),
                skill_issue_paths: Vec::new(),
                skill_issue_lines: Vec::new(),
            },
            panels: OperatorPanelsSnapshot {
                run: RunPanelSnapshot {
                    active_wave_ids: vec![5],
                    active_run_ids: vec!["wave-05-test".to_string()],
                    active_run_count: 1,
                    completed_run_count: 5,
                    active_runs: vec![active_run.clone()],
                    proof_complete_run_count: 0,
                },
                agents: AgentsPanelSnapshot {
                    total_agents: 60,
                    implementation_agents: 30,
                    closure_agents: 30,
                    required_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    present_closure_agents: vec![
                        "A0".to_string(),
                        "A8".to_string(),
                        "A9".to_string(),
                    ],
                    missing_closure_agents: Vec::new(),
                    agent_details: active_run.agents.clone(),
                },
                queue: QueuePanelSnapshot {
                    ready_wave_count: 1,
                    claimed_wave_count: 0,
                    blocked_wave_count: 3,
                    active_wave_count: 1,
                    completed_wave_count: 5,
                    ready_wave_ids: vec![6],
                    claimed_wave_ids: Vec::new(),
                    blocked_wave_ids: vec![7, 8, 9],
                    active_wave_ids: vec![5],
                    blocker_summary: QueueBlockerSummary {
                        dependency: 3,
                        lint: 0,
                        closure: 0,
                        ownership: 0,
                        lease_expired: 0,
                        budget: 0,
                        active_run: 1,
                        already_completed: 5,
                        other: 0,
                    },
                    next_ready_wave_ids: vec![6],
                    claimable_wave_ids: vec![6],
                    queue_ready: true,
                    queue_ready_reason: "ready waves are available to claim".to_string(),
                    waves: vec![QueuePanelWaveSnapshot {
                        id: 5,
                        slug: "tui-right-panel".to_string(),
                        title: "Build the right-side operator panel in the TUI".to_string(),
                        queue_state: "active".to_string(),
                        blocked: false,
                    }],
                },
                control: ControlPanelSnapshot {
                    rerun_supported: true,
                    clear_rerun_supported: true,
                    launch_supported: true,
                    autonomous_supported: true,
                    launcher_required: true,
                    launcher_ready: false,
                    actions: vec![
                        ControlAction {
                            key: "r".to_string(),
                            label: "Request rerun".to_string(),
                            description: "Request rerun".to_string(),
                            implemented: true,
                        },
                        ControlAction {
                            key: "launch".to_string(),
                            label: "Launch wave".to_string(),
                            description:
                                "Launch is unavailable because the Codex binary is missing."
                                    .to_string(),
                            implemented: false,
                        },
                    ],
                    implemented_actions: vec![ControlAction {
                        key: "r".to_string(),
                        label: "Request rerun".to_string(),
                        description: "Request rerun".to_string(),
                        implemented: true,
                    }],
                    unavailable_actions: vec![ControlAction {
                        key: "launch".to_string(),
                        label: "Launch wave".to_string(),
                        description: "Launch is unavailable because the Codex binary is missing."
                            .to_string(),
                        implemented: false,
                    }],
                    unavailable_reasons: Vec::new(),
                },
            },
            launcher: LauncherStatus {
                codex_binary_available: false,
                ready: false,
            },
            active_run_details: vec![active_run],
            rerun_intents: vec![RerunIntentRecord {
                request_id: Some("rerun-wave-05-1".to_string()),
                wave_id: 5,
                reason: "Requested from the Wave operator TUI".to_string(),
                requested_by: "operator".to_string(),
                status: RerunIntentStatus::Requested,
                requested_at_ms: 1,
                cleared_at_ms: None,
            }],
            control_actions: vec![
                ControlAction {
                    key: "r".to_string(),
                    label: "Request rerun".to_string(),
                    description: "Request rerun".to_string(),
                    implemented: true,
                },
                ControlAction {
                    key: "launch".to_string(),
                    label: "Launch wave".to_string(),
                    description: "Launch is unavailable because the Codex binary is missing."
                        .to_string(),
                    implemented: false,
                },
            ],
        }
    }
}
