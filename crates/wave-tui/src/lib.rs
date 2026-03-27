//! Bootstrap interactive operator shell for the Wave workspace.
//!
//! This crate keeps the TUI thin: it reads operator snapshots, renders the
//! current state, and forwards basic rerun actions into the local runtime
//! surface. Queue and control truth stay owned by reducer-backed projection
//! helpers and arrive through the app-server snapshot rather than terminal-
//! local readiness logic.

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use crossterm::event;
use crossterm::event::Event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::execute;
use crossterm::terminal;
use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
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
use ratatui::Terminal;
use std::fmt;
use std::io;
use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;
use wave_app_server::load_operator_snapshot;
use wave_app_server::ActiveRunDetail;
use wave_app_server::OperatorSnapshot;
use wave_config::ProjectConfig;
use wave_domain::RerunScope;
use wave_runtime::acknowledge_escalation;
use wave_runtime::apply_closure_override;
use wave_runtime::approve_human_input_request;
use wave_runtime::clear_closure_override;
use wave_runtime::clear_rerun;
use wave_runtime::dismiss_escalation;
use wave_runtime::preview_closure_override;
use wave_runtime::reject_human_input_request;
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
    Portfolio,
    Run,
    Agents,
    Queue,
    Blockers,
    Proof,
    Control,
    Delivery,
}

impl PanelTab {
    fn all() -> [Self; 8] {
        [
            Self::Portfolio,
            Self::Run,
            Self::Agents,
            Self::Queue,
            Self::Blockers,
            Self::Proof,
            Self::Control,
            Self::Delivery,
        ]
    }

    fn title(self) -> &'static str {
        match self {
            Self::Portfolio => "Portfolio",
            Self::Run => "Run",
            Self::Agents => "Agents",
            Self::Queue => "Queue",
            Self::Blockers => "Blockers",
            Self::Proof => "Proof",
            Self::Control => "Control",
            Self::Delivery => "Delivery",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Portfolio => Self::Run,
            Self::Run => Self::Agents,
            Self::Agents => Self::Queue,
            Self::Queue => Self::Blockers,
            Self::Blockers => Self::Proof,
            Self::Proof => Self::Control,
            Self::Control => Self::Delivery,
            Self::Delivery => Self::Portfolio,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Portfolio => Self::Delivery,
            Self::Run => Self::Portfolio,
            Self::Agents => Self::Run,
            Self::Queue => Self::Agents,
            Self::Blockers => Self::Queue,
            Self::Proof => Self::Blockers,
            Self::Control => Self::Proof,
            Self::Delivery => Self::Control,
        }
    }
}

#[derive(Debug, Default)]
struct AppState {
    selected_wave_index: usize,
    selected_operator_action_index: usize,
    flash_message: Option<FlashMessage>,
    pending_control_action: Option<PendingControlAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlashMessage {
    text: String,
    kind: FlashMessageKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlashMessageKind {
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingControlAction {
    ApplyManualClose(ManualCloseConfirmation),
    ClearManualClose(ClearManualCloseConfirmation),
    ApproveOperatorAction(OperatorActionConfirmation),
    RejectOperatorAction(OperatorActionConfirmation),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManualCloseConfirmation {
    wave_id: u32,
    wave_title: String,
    source_run_id: String,
    evidence_paths: Vec<String>,
    reason: String,
    detail: String,
    summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClearManualCloseConfirmation {
    wave_id: u32,
    wave_title: String,
    source_run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OperatorActionConfirmation {
    wave_id: u32,
    wave_title: String,
    record_id: String,
    kind: wave_app_server::OperatorActionableKind,
    summary: String,
    waiting_on: Option<String>,
    next_action: Option<String>,
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
        clamp_selected_operator_action(&mut app.state, &snapshot);

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

        if app.state.pending_control_action.is_some() {
            match key.code {
                KeyCode::Enter => handle_confirm_pending_control_action(app)?,
                KeyCode::Esc => {
                    app.state.pending_control_action = None;
                    set_info_message(&mut app.state, "cancelled control action");
                }
                _ => {}
            }
            continue;
        }

        match key.code {
            KeyCode::Char('q') => return Ok(()),
            KeyCode::Tab => app.tab = app.tab.next(),
            KeyCode::BackTab => app.tab = app.tab.previous(),
            KeyCode::Char('j') | KeyCode::Down => select_next_wave(&mut app.state, &snapshot),
            KeyCode::Char('k') | KeyCode::Up => select_previous_wave(&mut app.state),
            KeyCode::Char('[') => select_previous_operator_action(&mut app.state, &snapshot),
            KeyCode::Char(']') => select_next_operator_action(&mut app.state, &snapshot),
            KeyCode::Char('r') => handle_request_rerun(app)?,
            KeyCode::Char('c') => handle_clear_rerun(app)?,
            KeyCode::Char('m') => handle_prepare_manual_close(app)?,
            KeyCode::Char('M') => handle_prepare_clear_manual_close(app)?,
            KeyCode::Char('u') => handle_prepare_operator_action(app, true)?,
            KeyCode::Char('x') => handle_prepare_operator_action(app, false)?,
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
            RerunScope::Full,
        )?;
        set_info_message(
            &mut app.state,
            format!("requested rerun for wave {wave_id}"),
        );
    } else {
        set_error_message(&mut app.state, "no wave selected");
    }
    Ok(())
}

fn handle_clear_rerun(app: &mut App) -> Result<()> {
    let snapshot = load_operator_snapshot(&app.root, &app.config)?;
    if let Some(wave_id) = selected_wave_id(&app.state, &snapshot) {
        let result = clear_rerun(&app.root, &app.config, wave_id)?;
        match result {
            Some(_) => {
                set_info_message(&mut app.state, format!("cleared rerun for wave {wave_id}"))
            }
            None => set_info_message(
                &mut app.state,
                format!("no rerun intent for wave {wave_id}"),
            ),
        }
    } else {
        set_error_message(&mut app.state, "no wave selected");
    }
    Ok(())
}

fn handle_prepare_manual_close(app: &mut App) -> Result<()> {
    let snapshot = load_operator_snapshot(&app.root, &app.config)?;
    let Some(wave) = selected_wave(&app.state, &snapshot) else {
        set_error_message(&mut app.state, "no wave selected");
        return Ok(());
    };
    if snapshot
        .closure_overrides
        .iter()
        .any(|record| record.wave_id == wave.id && record.is_active())
    {
        set_error_message(
            &mut app.state,
            format!(
                "wave {} already has an active manual close override",
                wave.id
            ),
        );
        return Ok(());
    }

    match preview_closure_override(&app.root, &app.config, wave.id, None, Vec::new()) {
        Ok(preview) => {
            let summary = manual_close_summary_from_preview(&preview);
            let detail = preview
                .source_run_error
                .clone()
                .or(preview.source_promotion_detail.clone())
                .unwrap_or_else(|| {
                    "Manual close requested from the Wave operator TUI.".to_string()
                });
            app.state.pending_control_action = Some(PendingControlAction::ApplyManualClose(
                ManualCloseConfirmation {
                    wave_id: wave.id,
                    wave_title: wave.title.clone(),
                    source_run_id: preview.source_run_id,
                    evidence_paths: preview.evidence_paths,
                    reason:
                        "Applied from the Wave operator TUI after operator review of the latest terminal run"
                            .to_string(),
                    detail,
                    summary,
                },
            ));
            set_info_message(
                &mut app.state,
                format!("review manual close confirmation for wave {}", wave.id),
            );
        }
        Err(error) => {
            set_error_message(&mut app.state, error.to_string());
        }
    }
    Ok(())
}

fn handle_prepare_clear_manual_close(app: &mut App) -> Result<()> {
    let snapshot = load_operator_snapshot(&app.root, &app.config)?;
    let Some(wave) = selected_wave(&app.state, &snapshot) else {
        set_error_message(&mut app.state, "no wave selected");
        return Ok(());
    };
    let Some(record) = snapshot
        .closure_overrides
        .iter()
        .find(|record| record.wave_id == wave.id && record.is_active())
    else {
        set_error_message(
            &mut app.state,
            format!("wave {} has no active manual close override", wave.id),
        );
        return Ok(());
    };

    app.state.pending_control_action = Some(PendingControlAction::ClearManualClose(
        ClearManualCloseConfirmation {
            wave_id: wave.id,
            wave_title: wave.title.clone(),
            source_run_id: record.source_run_id.clone(),
        },
    ));
    set_info_message(
        &mut app.state,
        format!(
            "review manual close clear confirmation for wave {}",
            wave.id
        ),
    );
    Ok(())
}

fn handle_prepare_operator_action(app: &mut App, approve: bool) -> Result<()> {
    let snapshot = load_operator_snapshot(&app.root, &app.config)?;
    let Some(wave) = selected_wave(&app.state, &snapshot) else {
        set_error_message(&mut app.state, "no wave selected");
        return Ok(());
    };
    let Some(item) = selected_actionable_operator_item(&app.state, &snapshot, wave.id) else {
        set_error_message(
            &mut app.state,
            format!(
                "wave {} has no actionable approval or escalation item",
                wave.id
            ),
        );
        return Ok(());
    };

    let confirmation = OperatorActionConfirmation {
        wave_id: wave.id,
        wave_title: wave.title.clone(),
        record_id: item.record_id.clone(),
        kind: item.kind,
        summary: item.summary.clone(),
        waiting_on: item.waiting_on.clone(),
        next_action: item.next_action.clone(),
    };
    app.state.pending_control_action = Some(if approve {
        PendingControlAction::ApproveOperatorAction(confirmation)
    } else {
        PendingControlAction::RejectOperatorAction(confirmation)
    });
    set_info_message(
        &mut app.state,
        format!(
            "review {} confirmation for wave {}",
            if approve {
                "operator action"
            } else {
                "operator rejection"
            },
            wave.id
        ),
    );
    Ok(())
}

fn handle_confirm_pending_control_action(app: &mut App) -> Result<()> {
    let Some(pending_action) = app.state.pending_control_action.take() else {
        return Ok(());
    };
    match pending_action {
        PendingControlAction::ApplyManualClose(confirmation) => {
            match apply_closure_override(
                &app.root,
                &app.config,
                confirmation.wave_id,
                confirmation.reason,
                Some(&confirmation.source_run_id),
                confirmation.evidence_paths,
                Some(confirmation.detail),
            ) {
                Ok(record) => set_info_message(
                    &mut app.state,
                    format!(
                        "applied manual close for wave {} from {}",
                        record.wave_id, record.source_run_id
                    ),
                ),
                Err(error) => set_error_message(&mut app.state, error.to_string()),
            }
        }
        PendingControlAction::ApproveOperatorAction(confirmation) => {
            let result = match confirmation.kind {
                wave_app_server::OperatorActionableKind::Approval => {
                    approve_human_input_request(&app.root, &app.config, &confirmation.record_id)
                        .map(|request| {
                            format!(
                                "approved human input {} for wave {}",
                                request.request_id, request.wave_id
                            )
                        })
                }
                wave_app_server::OperatorActionableKind::Escalation => {
                    acknowledge_escalation(&app.root, &app.config, &confirmation.record_id).map(
                        |record| {
                            format!(
                                "acknowledged escalation {} for wave {}",
                                confirmation.record_id, record.wave_id
                            )
                        },
                    )
                }
                wave_app_server::OperatorActionableKind::Override => {
                    bail!("manual close overrides use M to clear")
                }
            };
            match result {
                Ok(message) => set_info_message(&mut app.state, message),
                Err(error) => set_error_message(&mut app.state, error.to_string()),
            }
        }
        PendingControlAction::RejectOperatorAction(confirmation) => {
            let result = match confirmation.kind {
                wave_app_server::OperatorActionableKind::Approval => {
                    reject_human_input_request(&app.root, &app.config, &confirmation.record_id).map(
                        |request| {
                            format!(
                                "rejected human input {} for wave {}",
                                request.request_id, request.wave_id
                            )
                        },
                    )
                }
                wave_app_server::OperatorActionableKind::Escalation => {
                    dismiss_escalation(&app.root, &app.config, &confirmation.record_id).map(
                        |record| {
                            format!(
                                "dismissed escalation {} for wave {}",
                                confirmation.record_id, record.wave_id
                            )
                        },
                    )
                }
                wave_app_server::OperatorActionableKind::Override => {
                    bail!("manual close overrides use M to clear")
                }
            };
            match result {
                Ok(message) => set_info_message(&mut app.state, message),
                Err(error) => set_error_message(&mut app.state, error.to_string()),
            }
        }
        PendingControlAction::ClearManualClose(confirmation) => {
            match clear_closure_override(&app.root, &app.config, confirmation.wave_id) {
                Ok(Some(record)) if record.is_active() => set_info_message(
                    &mut app.state,
                    format!(
                        "wave {} manual close override is still active",
                        confirmation.wave_id
                    ),
                ),
                Ok(Some(_)) => set_info_message(
                    &mut app.state,
                    format!("cleared manual close for wave {}", confirmation.wave_id),
                ),
                Ok(None) => set_info_message(
                    &mut app.state,
                    format!("no manual close override for wave {}", confirmation.wave_id),
                ),
                Err(error) => set_error_message(&mut app.state, error.to_string()),
            }
        }
    }
    Ok(())
}

fn set_info_message(state: &mut AppState, text: impl Into<String>) {
    state.flash_message = Some(FlashMessage {
        text: text.into(),
        kind: FlashMessageKind::Info,
    });
}

fn set_error_message(state: &mut AppState, text: impl Into<String>) {
    state.flash_message = Some(FlashMessage {
        text: text.into(),
        kind: FlashMessageKind::Error,
    });
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

fn clamp_selected_operator_action(state: &mut AppState, snapshot: &OperatorSnapshot) {
    let Some(wave_id) = selected_wave_id(state, snapshot) else {
        state.selected_operator_action_index = 0;
        return;
    };
    let actionable_count = actionable_operator_items(snapshot, wave_id).len();
    if actionable_count == 0 {
        state.selected_operator_action_index = 0;
        return;
    }
    state.selected_operator_action_index = state
        .selected_operator_action_index
        .min(actionable_count.saturating_sub(1));
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

fn select_next_operator_action(state: &mut AppState, snapshot: &OperatorSnapshot) {
    let Some(wave_id) = selected_wave_id(state, snapshot) else {
        state.selected_operator_action_index = 0;
        return;
    };
    let actionable_count = actionable_operator_items(snapshot, wave_id).len();
    if actionable_count == 0 {
        state.selected_operator_action_index = 0;
        return;
    }
    state.selected_operator_action_index =
        (state.selected_operator_action_index + 1).min(actionable_count - 1);
}

fn select_previous_operator_action(state: &mut AppState, snapshot: &OperatorSnapshot) {
    if selected_wave_id(state, snapshot).is_none() {
        state.selected_operator_action_index = 0;
        return;
    }
    state.selected_operator_action_index = state.selected_operator_action_index.saturating_sub(1);
}

fn selected_wave_id(state: &AppState, snapshot: &OperatorSnapshot) -> Option<u32> {
    selected_wave(state, snapshot).map(|wave| wave.id)
}

fn selected_wave<'a>(
    state: &AppState,
    snapshot: &'a OperatorSnapshot,
) -> Option<&'a wave_control_plane::WaveStatusReadModel> {
    snapshot.planning.waves.get(state.selected_wave_index)
}

fn actionable_operator_items<'a>(
    snapshot: &'a OperatorSnapshot,
    wave_id: u32,
) -> Vec<&'a wave_app_server::OperatorActionableItem> {
    snapshot
        .operator_objects
        .iter()
        .filter(|item| {
            item.wave_id == wave_id
                && matches!(
                    item.kind,
                    wave_app_server::OperatorActionableKind::Approval
                        | wave_app_server::OperatorActionableKind::Escalation
                )
        })
        .collect()
}

fn selected_actionable_operator_item<'a>(
    state: &AppState,
    snapshot: &'a OperatorSnapshot,
    wave_id: u32,
) -> Option<&'a wave_app_server::OperatorActionableItem> {
    actionable_operator_items(snapshot, wave_id)
        .get(state.selected_operator_action_index)
        .copied()
}

fn selected_actionable_operator_context<'a>(
    state: &AppState,
    snapshot: &'a OperatorSnapshot,
    wave_id: u32,
) -> Option<(usize, usize, &'a wave_app_server::OperatorActionableItem)> {
    let items = actionable_operator_items(snapshot, wave_id);
    let total = items.len();
    items
        .get(state.selected_operator_action_index)
        .map(|item| (state.selected_operator_action_index, total, *item))
}

fn manual_close_summary_from_preview(preview: &wave_runtime::ClosureOverridePreview) -> String {
    if let Some(error) = preview.source_run_error.as_deref() {
        return format!("latest run {} failed: {error}", preview.source_run_id);
    }
    if let Some(detail) = preview.source_promotion_detail.as_deref() {
        return format!(
            "latest run {} promotion detail: {detail}",
            preview.source_run_id
        );
    }
    format!(
        "latest run {} status={}",
        preview.source_run_id, preview.source_run_status
    )
}

fn draw_ui(frame: &mut ratatui::Frame<'_>, app: &App, snapshot: &OperatorSnapshot) {
    let area = frame.area();
    match shell_layout_mode(area.width) {
        ShellLayoutMode::Wide => draw_wide_shell(frame, area, snapshot, app),
        // Narrow terminals collapse into a single summary so the operator still
        // gets a readable, honest view instead of a broken two-column layout.
        ShellLayoutMode::Narrow => draw_narrow_shell_fallback(frame, area, snapshot, app),
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
    draw_right_panel(frame, panel_area, snapshot, app, ShellLayoutMode::Wide);
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
        lines.push(Line::raw(format!("soft state: {}", wave.soft_state.label())));
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

    if let Some(message) = state.flash_message.as_ref() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            message.text.clone(),
            flash_message_style(message.kind),
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
    app: &App,
) {
    let paragraph = Paragraph::new(narrow_summary_lines(snapshot, app)).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Operator summary"),
    );
    frame.render_widget(paragraph, area);
}

fn narrow_summary_lines<'a>(snapshot: &'a OperatorSnapshot, app: &'a App) -> Vec<Line<'a>> {
    let state = &app.state;
    let selected_wave = selected_wave(state, snapshot);
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
        Line::raw(
            "The summary below preserves Portfolio, Run, Agents, Proof, Queue, and Control truth.",
        ),
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
        lines.push(Line::raw(""));
        push_summary_heading(&mut lines, "Portfolio");
        lines.extend(portfolio_focus_lines(snapshot, Some(wave.id)));
        if let Some(package) = acceptance_package_for_wave(snapshot, wave.id) {
            lines.push(Line::raw(""));
            push_summary_heading(&mut lines, "Proof");
            lines.extend(acceptance_package_summary_lines(package));
        }
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
        "actions: rerun={} clear-rerun={} manual-close={} clear-manual-close={} approve={} reject={} launch={} autonomous={}",
        yes_no(snapshot.panels.control.rerun_supported),
        yes_no(snapshot.panels.control.clear_rerun_supported),
        yes_no(snapshot.panels.control.apply_closure_override_supported),
        yes_no(snapshot.panels.control.clear_closure_override_supported),
        yes_no(snapshot.panels.control.approve_operator_action_supported),
        yes_no(snapshot.panels.control.reject_operator_action_supported),
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
    for item in manual_close_status_items(
        &app.root,
        &app.config,
        snapshot,
        selected_wave_id(state, snapshot),
    ) {
        lines.push(Line::raw(item));
    }
    if let Some(pending_action) = state.pending_control_action.as_ref() {
        lines.push(Line::raw(""));
        for item in pending_control_action_lines(pending_action) {
            lines.push(Line::raw(item));
        }
    }
    lines.push(Line::raw("keys: Tab Shift+Tab j k r c m M u x Enter Esc q"));

    if let Some(message) = state.flash_message.as_ref() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            message.text.clone(),
            flash_message_style(message.kind),
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

fn execution_lines(
    execution: &wave_control_plane::WaveExecutionState,
    budget: Option<&wave_control_plane::SchedulerBudgetState>,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        "Execution",
        Style::default()
            .fg(Color::Gray)
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(worktree) = &execution.worktree {
        lines.push(Line::raw(format!(
            "worktree: {} -> {} ({})",
            worktree.worktree_id.as_str(),
            worktree.path,
            debug_label(worktree.state)
        )));
    } else {
        lines.push(Line::raw("worktree: none"));
    }
    if let Some(promotion) = &execution.promotion {
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
    if let Some(scheduling) = &execution.scheduling {
        lines.push(Line::raw(format!(
            "scheduler: {}/{} fairness={} protected={} preemptible={}",
            debug_label(scheduling.phase),
            debug_label(scheduling.state),
            scheduling.fairness_rank,
            yes_no(scheduling.protected_closure_capacity),
            yes_no(scheduling.preemptible)
        )));
        if let Some(decision) = scheduling.last_decision.as_deref() {
            let label = match scheduling.state {
                wave_domain::WaveSchedulingState::Waiting => "wait reason",
                wave_domain::WaveSchedulingState::Preempted => "preemption",
                _ if scheduling.protected_closure_capacity => "closure protection",
                _ => "scheduler detail",
            };
            lines.push(Line::raw(format!("{label}: {decision}")));
        }
    } else {
        lines.push(Line::raw("scheduler: none"));
    }
    lines.push(Line::raw(format!(
        "merge blocked: {}",
        yes_no(execution.merge_blocked)
    )));
    lines.push(Line::raw(format!(
        "closure blocked: {}",
        yes_no(execution.closure_blocked_by_promotion)
    )));
    if let Some(budget) = budget {
        lines.push(Line::raw(format!(
            "closure capacity: reserved_slots={} reservation_active={} preemption={}",
            budget
                .reserved_closure_task_leases
                .map(|count| count.to_string())
                .unwrap_or_else(|| "none".to_string()),
            yes_no(budget.closure_capacity_reserved),
            yes_no(budget.preemption_enabled)
        )));
        if budget.closure_capacity_reserved {
            lines.push(Line::raw(
                "closure reservation: waiting closure work is holding protected capacity",
            ));
        }
    }
    lines
}

fn wave_execution_lines(wave: &wave_control_plane::WaveStatusReadModel) -> Vec<Line<'static>> {
    execution_lines(&wave.execution, Some(&wave.ownership.budget))
}

fn selected_execution_for_run_tab(
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) -> Option<(
    wave_control_plane::WaveExecutionState,
    Option<wave_control_plane::SchedulerBudgetState>,
)> {
    let selected_wave = selected_wave_id.and_then(|wave_id| {
        snapshot
            .planning
            .waves
            .iter()
            .find(|wave| wave.id == wave_id)
    });

    if let Some(run) = selected_active_run(snapshot, selected_wave_id) {
        let budget = selected_wave
            .filter(|wave| wave.id == run.wave_id)
            .map(|wave| wave.ownership.budget.clone());
        return Some((run.execution.clone(), budget));
    }

    selected_wave.map(|wave| (wave.execution.clone(), Some(wave.ownership.budget.clone())))
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
    if value {
        "yes"
    } else {
        "no"
    }
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
    if let Some(source) = run.activity_source.as_deref() {
        lines.push(Line::raw(format!(
            "last activity: {} via {}",
            run.last_activity_at_ms
                .map(|timestamp| timestamp.to_string())
                .unwrap_or_else(|| "none".to_string()),
            source
        )));
    }
    if run.stalled {
        lines.push(Line::raw(format!(
            "stall: {}",
            run.stall_reason
                .as_deref()
                .unwrap_or("agent appears stalled")
        )));
    } else if let Some(reason) = run.stall_reason.as_deref() {
        lines.push(Line::raw(format!("activity: {}", reason)));
    }
    if run_has_mixed_runtime_selection(run) {
        lines.push(Line::raw(format!(
            "selected runtimes: {}",
            format_string_list(&run.runtime_summary.selected_runtimes)
        )));
        if !run.runtime_summary.requested_runtimes.is_empty() {
            lines.push(Line::raw(format!(
                "requested runtimes: {}",
                format_string_list(&run.runtime_summary.requested_runtimes)
            )));
        }
        if let Some(runtime) = current_agent_runtime_detail(run) {
            lines.push(Line::raw(format!(
                "current agent runtime: {}",
                runtime_decision_summary(runtime)
            )));
            if let Some(fallback) = runtime.fallback.as_ref() {
                lines.push(Line::raw(format!(
                    "current agent fallback: {}",
                    fallback.reason
                )));
            }
            lines.push(Line::raw(format!(
                "current agent adapter: {} provider={}",
                runtime.execution_identity.adapter, runtime.execution_identity.provider
            )));
        }
    } else if let Some(runtime) = representative_runtime_detail(run) {
        lines.push(Line::raw(format!(
            "runtime decision: {}",
            runtime_decision_summary(runtime)
        )));
        if let Some(fallback) = runtime.fallback.as_ref() {
            lines.push(Line::raw(format!("fallback reason: {}", fallback.reason)));
        }
        lines.push(Line::raw(format!(
            "adapter: {} provider={}",
            runtime.execution_identity.adapter, runtime.execution_identity.provider
        )));
    } else if !run.runtime_summary.selected_runtimes.is_empty() {
        lines.push(Line::raw(format!(
            "selected runtimes: {}",
            run.runtime_summary.selected_runtimes.join(", ")
        )));
    }
    if !run.runtime_summary.selection_sources.is_empty() {
        lines.push(Line::raw(format!(
            "selection sources: {}",
            run.runtime_summary.selection_sources.join(", ")
        )));
    }
    if run.runtime_summary.fallback_count > 0 || !run.runtime_summary.fallback_targets.is_empty() {
        lines.push(Line::raw(format!(
            "fallbacks: {} target={}",
            run.runtime_summary.fallback_count,
            format_string_list(&run.runtime_summary.fallback_targets)
        )));
    }
    if !run.replay.ok {
        lines.push(Line::styled(
            format!("replay issues: {}", run.replay.issues.len()),
            Style::default().fg(Color::Red),
        ));
    }
    lines
}

fn acceptance_package_for_wave(
    snapshot: &OperatorSnapshot,
    wave_id: u32,
) -> Option<&wave_app_server::AcceptancePackageSnapshot> {
    snapshot
        .acceptance_packages
        .iter()
        .find(|package| package.wave_id == wave_id)
}

fn portfolio_focus_lines(snapshot: &OperatorSnapshot, wave_id: Option<u32>) -> Vec<Line<'static>> {
    let Some(wave_id) = wave_id else {
        return vec![Line::raw("No wave selected.")];
    };
    let portfolio = &snapshot.planning.portfolio;
    let acceptance_package = acceptance_package_for_wave(snapshot, wave_id);
    if portfolio.initiatives.is_empty()
        && portfolio.milestones.is_empty()
        && portfolio.release_trains.is_empty()
        && portfolio.outcome_contracts.is_empty()
        && acceptance_package.is_none()
    {
        return vec![Line::raw("No portfolio delivery model is active.")];
    }

    let mut lines = Vec::new();
    if let Some(package) = acceptance_package {
        lines.extend(portfolio_delivery_summary_lines(package));
    }
    if !(portfolio.initiatives.is_empty()
        && portfolio.milestones.is_empty()
        && portfolio.release_trains.is_empty()
        && portfolio.outcome_contracts.is_empty())
    {
        lines.push(Line::raw(format!(
            "summary: initiatives={} milestones={} release_trains={} outcome_contracts={} mapped_waves={}",
            portfolio.summary.initiative_count,
            portfolio.summary.milestone_count,
            portfolio.summary.release_train_count,
            portfolio.summary.outcome_contract_count,
            portfolio.summary.mapped_wave_count
        )));
    }
    for initiative in portfolio
        .initiatives
        .iter()
        .filter(|initiative| initiative.delivery.wave_ids.contains(&wave_id))
    {
        lines.extend(portfolio_entry_acceptance_lines(
            "initiative",
            &initiative.title,
            &initiative.delivery.wave_ids,
            &initiative.delivery.blocking_reasons,
            &snapshot.acceptance_packages,
        ));
    }
    for milestone in portfolio
        .milestones
        .iter()
        .filter(|milestone| milestone.delivery.wave_ids.contains(&wave_id))
    {
        lines.extend(portfolio_entry_acceptance_lines(
            "milestone",
            &milestone.title,
            &milestone.delivery.wave_ids,
            &milestone.delivery.blocking_reasons,
            &snapshot.acceptance_packages,
        ));
    }
    for train in portfolio
        .release_trains
        .iter()
        .filter(|train| train.delivery.wave_ids.contains(&wave_id))
    {
        lines.extend(portfolio_entry_acceptance_lines(
            "release train",
            &train.title,
            &train.delivery.wave_ids,
            &train.delivery.blocking_reasons,
            &snapshot.acceptance_packages,
        ));
    }
    for contract in portfolio
        .outcome_contracts
        .iter()
        .filter(|contract| contract.delivery.wave_ids.contains(&wave_id))
    {
        lines.extend(portfolio_entry_acceptance_lines(
            "outcome contract",
            &contract.title,
            &contract.delivery.wave_ids,
            &contract.delivery.blocking_reasons,
            &snapshot.acceptance_packages,
        ));
    }
    lines
}

fn portfolio_overview_lines(
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) -> Vec<Line<'static>> {
    let portfolio = &snapshot.planning.portfolio;
    let mut lines = vec![Line::raw(format!(
        "overview: initiatives={} milestones={} release_trains={} outcome_contracts={} mapped_waves={}",
        portfolio.summary.initiative_count,
        portfolio.summary.milestone_count,
        portfolio.summary.release_train_count,
        portfolio.summary.outcome_contract_count,
        portfolio.summary.mapped_wave_count
    ))];

    let mut packages = snapshot
        .acceptance_packages
        .iter()
        .filter(|package| {
            portfolio.mapped_wave_ids.is_empty()
                || portfolio.mapped_wave_ids.contains(&package.wave_id)
        })
        .collect::<Vec<_>>();
    if packages.is_empty() {
        if let Some(package) =
            selected_wave_id.and_then(|wave_id| acceptance_package_for_wave(snapshot, wave_id))
        {
            packages.push(package);
        }
    }
    packages.sort_by_key(|package| package.wave_id);

    for package in packages {
        let prefix = if selected_wave_id == Some(package.wave_id) {
            "> "
        } else {
            ""
        };
        lines.push(Line::raw(format!(
            "{prefix}wave {} {} ship={} release={} signoff={} proof={}/{} risks={} debt={}",
            package.wave_id,
            package.wave_title,
            debug_label(package.ship_state),
            debug_label(package.release_state),
            debug_label(package.signoff.state),
            package.implementation.completed_agents,
            package.implementation.total_agents,
            package.known_risks.len(),
            package.outstanding_debt.len()
        )));
        lines.push(Line::raw(format!(
            "{prefix}delivery summary: {}",
            package.summary
        )));
        if !package.blocking_reasons.is_empty() {
            lines.push(Line::raw(format!(
                "{prefix}delivery blockers: {}",
                package.blocking_reasons.join(" | ")
            )));
        }
    }

    for initiative in &portfolio.initiatives {
        lines.extend(portfolio_entry_acceptance_lines(
            "initiative",
            &initiative.title,
            &initiative.delivery.wave_ids,
            &initiative.delivery.blocking_reasons,
            &snapshot.acceptance_packages,
        ));
    }
    for milestone in &portfolio.milestones {
        lines.extend(portfolio_entry_acceptance_lines(
            "milestone",
            &milestone.title,
            &milestone.delivery.wave_ids,
            &milestone.delivery.blocking_reasons,
            &snapshot.acceptance_packages,
        ));
    }
    for train in &portfolio.release_trains {
        lines.extend(portfolio_entry_acceptance_lines(
            "release train",
            &train.title,
            &train.delivery.wave_ids,
            &train.delivery.blocking_reasons,
            &snapshot.acceptance_packages,
        ));
    }
    for contract in &portfolio.outcome_contracts {
        lines.extend(portfolio_entry_acceptance_lines(
            "outcome contract",
            &contract.title,
            &contract.delivery.wave_ids,
            &contract.delivery.blocking_reasons,
            &snapshot.acceptance_packages,
        ));
    }

    lines
}

fn portfolio_delivery_summary_lines(
    package: &wave_app_server::AcceptancePackageSnapshot,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::raw(format!(
            "delivery: ship={} release={} signoff={} proof={}/{} complete={} source={} risks={} debt={}",
            debug_label(package.ship_state),
            debug_label(package.release_state),
            debug_label(package.signoff.state),
            package.implementation.completed_agents,
            package.implementation.total_agents,
            yes_no(package.implementation.proof_complete),
            package
                .implementation
                .proof_source
                .clone()
                .unwrap_or_else(|| "none".to_string()),
            package.known_risks.len(),
            package.outstanding_debt.len()
        )),
        Line::raw(format!("delivery summary: {}", package.summary)),
    ];
    if !package.blocking_reasons.is_empty() {
        lines.push(Line::raw(format!(
            "delivery blockers: {}",
            package.blocking_reasons.join(" | ")
        )));
    }
    lines
}

fn portfolio_entry_acceptance_lines(
    prefix: &str,
    title: &str,
    wave_ids: &[u32],
    portfolio_blocking_reasons: &[String],
    acceptance_packages: &[wave_app_server::AcceptancePackageSnapshot],
) -> Vec<Line<'static>> {
    let mut blocking_reasons = portfolio_blocking_reasons.to_vec();
    let mut wave_states = Vec::new();
    let mut ship_ready = 0;
    let mut accepted = 0;
    let mut signed_off = 0;

    for wave_id in wave_ids {
        if let Some(package) = acceptance_packages
            .iter()
            .find(|package| package.wave_id == *wave_id)
        {
            let ship_state = debug_label(package.ship_state);
            let release_state = debug_label(package.release_state);
            let signoff_state = debug_label(package.signoff.state);
            if ship_state == "ship" {
                ship_ready += 1;
            }
            if release_state == "accepted" {
                accepted += 1;
            }
            if signoff_state == "signed_off" {
                signed_off += 1;
            }
            wave_states.push(format!(
                "{}:{}/{}/{}",
                wave_id, ship_state, release_state, signoff_state
            ));
            for reason in &package.blocking_reasons {
                let reason = format!("wave {} {}", wave_id, reason);
                if !blocking_reasons.iter().any(|existing| existing == &reason) {
                    blocking_reasons.push(reason);
                }
            }
        } else {
            wave_states.push(format!("{wave_id}:missing/missing/missing"));
            let reason = format!("wave {} acceptance package missing", wave_id);
            if !blocking_reasons.iter().any(|existing| existing == &reason) {
                blocking_reasons.push(reason);
            }
        }
    }

    let mut lines = vec![Line::raw(format!(
        "{prefix}: {title} ship={}/{} release={}/{} signoff={}/{} waves={}",
        ship_ready,
        wave_ids.len(),
        accepted,
        wave_ids.len(),
        signed_off,
        wave_ids.len(),
        wave_states.join(", ")
    ))];
    if !blocking_reasons.is_empty() {
        lines.push(Line::raw(format!(
            "{prefix} blockers: {}",
            blocking_reasons.join(" | ")
        )));
    }
    lines
}

fn acceptance_package_summary_lines(
    package: &wave_app_server::AcceptancePackageSnapshot,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::raw(format!(
            "ship: {}  release: {}  signoff: {}",
            debug_label(package.ship_state),
            debug_label(package.release_state),
            debug_label(package.signoff.state)
        )),
        Line::raw(format!("acceptance: {}", package.summary)),
        Line::raw(format!(
            "design: {:?} blockers={} contradictions={} questions={} assumptions={} human_input={} ambiguous_dependencies={}",
            package.design_intent.completeness,
            package.design_intent.blocker_count,
            package.design_intent.contradiction_count,
            package.design_intent.unresolved_question_count,
            package.design_intent.unresolved_assumption_count,
            package.design_intent.pending_human_input_count,
            package.design_intent.ambiguous_dependency_count
        )),
        Line::raw(format!(
            "implementation: proof_complete={} proof={}/{} replay_ok={} source={}",
            yes_no(package.implementation.proof_complete),
            package.implementation.completed_agents,
            package.implementation.total_agents,
            package
                .implementation
                .replay_ok
                .map(yes_no)
                .unwrap_or("no-run"),
            package
                .implementation
                .proof_source
                .clone()
                .unwrap_or_else(|| "none".to_string())
        )),
        Line::raw(format!(
            "signoff: complete={} manual_close={} completed={} pending={} operator_actions={}",
            yes_no(package.signoff.complete),
            yes_no(package.signoff.manual_close_applied),
            format_string_list(&package.signoff.completed_closure_agents),
            format_string_list(&package.signoff.pending_closure_agents),
            format_string_list(&package.signoff.pending_operator_actions)
        )),
        Line::raw(format!(
            "closure gates: {}",
            closure_gate_status_summary(&package.signoff.closure_agents)
        )),
        Line::raw(format!(
            "release: promotion={} merge_blocked={} closure_blocked={}",
            package
                .release
                .promotion_state
                .map(debug_label)
                .unwrap_or_else(|| "none".to_string()),
            yes_no(package.release.merge_blocked),
            yes_no(package.release.closure_blocked)
        )),
    ];
    if let Some(decision) = package.release.last_decision.as_deref() {
        lines.push(Line::raw(format!("release detail: {decision}")));
    }
    if !package.blocking_reasons.is_empty() {
        lines.push(Line::raw(format!(
            "ship blockers: {}",
            package.blocking_reasons.join(" | ")
        )));
    }
    if !package.known_risks.is_empty() {
        lines.push(Line::raw(format!(
            "known risks: {}",
            package.known_risks.len()
        )));
        for item in &package.known_risks {
            lines.push(Line::raw(format!("risk {}: {}", item.code, item.summary)));
            if let Some(detail) = item.detail.as_deref() {
                lines.push(Line::raw(format!("risk detail: {}", detail)));
            }
        }
    }
    if !package.outstanding_debt.is_empty() {
        lines.push(Line::raw(format!(
            "outstanding debt: {}",
            package.outstanding_debt.len()
        )));
        for item in &package.outstanding_debt {
            lines.push(Line::raw(format!("debt {}: {}", item.code, item.summary)));
            if let Some(detail) = item.detail.as_deref() {
                lines.push(Line::raw(format!("debt detail: {}", detail)));
            }
        }
    }
    for agent in package
        .signoff
        .closure_agents
        .iter()
        .filter(|agent| agent.error.is_some())
    {
        lines.push(Line::raw(format!(
            "closure error: {} {}",
            agent.agent_id,
            agent.error.as_deref().unwrap_or_default()
        )));
    }
    lines
}

fn acceptance_package_status_items(
    package: &wave_app_server::AcceptancePackageSnapshot,
) -> Vec<String> {
    let mut items = vec![
        format!("Ship state: {}", debug_label(package.ship_state)),
        format!("Release state: {}", debug_label(package.release_state)),
        format!("Signoff state: {}", debug_label(package.signoff.state)),
        format!("Acceptance summary: {}", package.summary),
        format!(
            "Acceptance signoff: complete={} manual_close={} pending={} operator_actions={}",
            yes_no(package.signoff.complete),
            yes_no(package.signoff.manual_close_applied),
            format_string_list(&package.signoff.pending_closure_agents),
            format_string_list(&package.signoff.pending_operator_actions)
        ),
        format!(
            "Acceptance closure gates: {}",
            closure_gate_status_summary(&package.signoff.closure_agents)
        ),
        format!(
            "Acceptance proof: complete={} source={} replay_ok={}",
            yes_no(package.implementation.proof_complete),
            package
                .implementation
                .proof_source
                .clone()
                .unwrap_or_else(|| "none".to_string()),
            package
                .implementation
                .replay_ok
                .map(yes_no)
                .unwrap_or("no-run")
        ),
    ];
    if !package.blocking_reasons.is_empty() {
        items.push(format!(
            "Ship blockers: {}",
            package.blocking_reasons.join(" | ")
        ));
    }
    for risk in &package.known_risks {
        items.push(format!("Risk {}: {}", risk.code, risk.summary));
    }
    for debt in &package.outstanding_debt {
        items.push(format!("Debt {}: {}", debt.code, debt.summary));
    }
    items
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
                    .map(debug_label)
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

fn agent_runtime_label(runtime: &wave_app_server::RuntimeDetail) -> String {
    if let Some(fallback) = runtime.fallback.as_ref() {
        format!(
            "{} <- {} [{}]",
            runtime.selected_runtime,
            fallback.requested_runtime,
            fallback_reason_tag(&fallback.reason)
        )
    } else {
        let requested = runtime
            .policy
            .requested_runtime
            .as_deref()
            .unwrap_or(runtime.selected_runtime.as_str());
        format!("{} via {}", runtime.selected_runtime, requested)
    }
}

fn fallback_reason_tag(reason: &str) -> &'static str {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("auth") || lower.contains("login") {
        "auth"
    } else if lower.contains("unavailable") || lower.contains("blocked") {
        "unavailable"
    } else if lower.contains("missing") {
        "missing"
    } else {
        "fallback"
    }
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
    app: &App,
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
        .position(|candidate| *candidate == app.tab)
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

    match app.tab {
        PanelTab::Portfolio => draw_portfolio_tab(
            frame,
            panel_chunks[1],
            snapshot,
            selected_wave_id(&app.state, snapshot),
        ),
        PanelTab::Run => draw_run_tab(
            frame,
            panel_chunks[1],
            snapshot,
            selected_wave_id(&app.state, snapshot),
        ),
        PanelTab::Agents => draw_agents_tab(
            frame,
            panel_chunks[1],
            snapshot,
            selected_wave_id(&app.state, snapshot),
        ),
        PanelTab::Queue => draw_queue_tab(frame, panel_chunks[1], snapshot),
        PanelTab::Blockers => draw_blockers_tab(
            frame,
            panel_chunks[1],
            snapshot,
            &app.state,
            selected_wave_id(&app.state, snapshot),
        ),
        PanelTab::Proof => draw_proof_tab(
            frame,
            panel_chunks[1],
            snapshot,
            selected_wave_id(&app.state, snapshot),
        ),
        PanelTab::Control => draw_control_tab(frame, panel_chunks[1], snapshot, app),
        PanelTab::Delivery => draw_delivery_tab(frame, panel_chunks[1], snapshot),
    }
}

fn draw_portfolio_tab(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) {
    frame.render_widget(
        Paragraph::new(portfolio_overview_lines(snapshot, selected_wave_id))
            .block(Block::default().borders(Borders::ALL).title("Portfolio")),
        area,
    );
}

fn draw_run_tab(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) {
    let mut lines = Vec::new();
    let acceptance_package =
        selected_wave_id.and_then(|wave_id| acceptance_package_for_wave(snapshot, wave_id));
    if let Some(run) = selected_active_run(snapshot, selected_wave_id) {
        lines.extend(run_summary_lines(run));
        if let Some(package) = acceptance_package {
            lines.push(Line::raw(""));
            lines.extend(acceptance_package_summary_lines(package));
        }
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
        if let Some(package) = acceptance_package {
            lines.push(Line::raw(""));
            lines.extend(acceptance_package_summary_lines(package));
        }
    }
    if let Some((execution, budget)) = selected_execution_for_run_tab(snapshot, selected_wave_id) {
        lines.push(Line::raw(""));
        lines.extend(execution_lines(&execution, budget.as_ref()));
    }

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Run")),
        area,
    );
}

fn draw_proof_tab(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) {
    let mut lines = Vec::new();
    if let Some(package) =
        selected_wave_id.and_then(|wave_id| acceptance_package_for_wave(snapshot, wave_id))
    {
        lines.extend(acceptance_package_summary_lines(package));
    } else {
        lines.push(Line::raw("No acceptance package for the selected wave."));
    }

    if let Some(run) = selected_latest_run(snapshot, selected_wave_id) {
        lines.push(Line::raw(""));
        lines.push(Line::raw(format!(
            "proof artifacts: {} complete={}",
            run.proof.declared_artifacts.len(),
            yes_no(run.proof.complete)
        )));
        lines.push(Line::raw(format!(
            "proof source: {}  replay_ok={}",
            run.proof.proof_source,
            yes_no(run.replay.ok)
        )));
        lines.push(Line::raw(format!(
            "result authority: structured={} compatibility={}",
            run.proof.envelope_backed_agents, run.proof.compatibility_backed_agents
        )));
        for artifact in &run.proof.declared_artifacts {
            lines.push(Line::raw(format!(
                "- {} {}",
                artifact.path,
                if artifact.exists {
                    "present"
                } else {
                    "missing"
                }
            )));
        }
        for issue in &run.replay.issues {
            lines.push(Line::raw(format!(
                "replay issue: {} ({})",
                issue.kind, issue.detail
            )));
        }
    }

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Proof")),
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
        let runtime = agent
            .runtime
            .as_ref()
            .map(agent_runtime_label)
            .unwrap_or_else(|| "n/a".to_string());
        Row::new(vec![
            Cell::from(agent.id.clone()),
            Cell::from(agent.title.clone()),
            Cell::from(runtime),
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
            Constraint::Percentage(28),
            Constraint::Length(28),
            Constraint::Length(10),
            Constraint::Percentage(32),
        ],
    )
    .header(
        Row::new(vec!["Id", "Title", "Runtime", "State", "Proof"]).style(
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

fn draw_blockers_tab(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    state: &AppState,
    selected_wave_id: Option<u32>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(0)])
        .split(area);

    let summary = Paragraph::new(blocker_summary_lines(snapshot, selected_wave_id)).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Blocker summary"),
    );
    frame.render_widget(summary, chunks[0]);

    let items = blocker_item_lines(snapshot, state, selected_wave_id)
        .into_iter()
        .map(ListItem::new)
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title("Triage items")),
        chunks[1],
    );
}

fn draw_control_tab(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    app: &App,
) {
    let selected_wave_id = selected_wave_id(&app.state, snapshot);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(6),
            Constraint::Length(10),
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

    let mut status_items = control_status_items(snapshot, &app.state, selected_wave_id);
    status_items.extend(manual_close_status_items(
        &app.root,
        &app.config,
        snapshot,
        selected_wave_id,
    ));
    if let Some(pending_action) = app.state.pending_control_action.as_ref() {
        status_items.push(String::new());
        status_items.extend(pending_control_action_lines(pending_action));
    }
    let status_items = status_items
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

fn draw_delivery_tab(frame: &mut ratatui::Frame<'_>, area: Rect, snapshot: &OperatorSnapshot) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(6)])
        .split(area);

    let summary_lines = vec![
        Line::raw(format!(
            "initiatives={} releases={} acceptance={}",
            snapshot.delivery.summary.initiative_count,
            snapshot.delivery.summary.release_count,
            snapshot.delivery.summary.acceptance_package_count
        )),
        Line::raw(format!(
            "blocking risks={} debts={}",
            snapshot.delivery.summary.blocking_risk_count,
            snapshot.delivery.summary.blocking_debt_count
        )),
        Line::raw(format!(
            "signal: queue={} soft={} exit_code={}",
            snapshot.delivery.signal.queue_state,
            snapshot.delivery.signal.delivery_soft_state.label(),
            snapshot.delivery.signal.exit_code
        )),
    ];
    frame.render_widget(
        Paragraph::new(summary_lines)
            .block(Block::default().borders(Borders::ALL).title("Delivery summary")),
        chunks[0],
    );

    let items = if snapshot.delivery.attention_lines.is_empty() {
        vec![ListItem::new("No delivery attention lines.")]
    } else {
        snapshot
            .delivery
            .attention_lines
            .iter()
            .map(|line| ListItem::new(line.clone()))
            .collect::<Vec<_>>()
    };
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title("Delivery")),
        chunks[1],
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

fn selected_latest_run(
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) -> Option<&ActiveRunDetail> {
    selected_wave_id
        .and_then(|wave_id| {
            snapshot
                .latest_run_details
                .iter()
                .find(|run| run.wave_id == wave_id)
        })
        .or_else(|| snapshot.latest_run_details.first())
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

fn control_status_items(
    snapshot: &OperatorSnapshot,
    state: &AppState,
    selected_wave_id: Option<u32>,
) -> Vec<String> {
    let mut items = selected_active_run(snapshot, selected_wave_id)
        .map(|run| {
            let mut lines = if run.replay.ok {
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
            };
            if run_has_mixed_runtime_selection(run) {
                lines.push(format!(
                    "Run runtimes: {}",
                    format_string_list(&run.runtime_summary.selected_runtimes)
                ));
                if !run.runtime_summary.requested_runtimes.is_empty() {
                    lines.push(format!(
                        "Requested runtimes: {}",
                        format_string_list(&run.runtime_summary.requested_runtimes)
                    ));
                }
                if let Some(runtime) = current_agent_runtime_detail(run) {
                    lines.push(format!(
                        "Current agent runtime: {}",
                        runtime_decision_summary(runtime)
                    ));
                    lines.push(format!(
                        "Current agent adapter: {} ({})",
                        runtime.execution_identity.adapter, runtime.execution_identity.provider
                    ));
                    if let Some(fallback) = runtime.fallback.as_ref() {
                        lines.push(format!(
                            "Current agent fallback: {} -> {} ({})",
                            fallback.requested_runtime, fallback.selected_runtime, fallback.reason
                        ));
                    }
                }
            } else if let Some(runtime) = representative_runtime_detail(run) {
                lines.push(format!(
                    "Run runtime: {}",
                    runtime_decision_summary(runtime)
                ));
                lines.push(format!(
                    "Run adapter: {} ({})",
                    runtime.execution_identity.adapter, runtime.execution_identity.provider
                ));
                if let Some(fallback) = runtime.fallback.as_ref() {
                    lines.push(format!(
                        "Run fallback: {} -> {} ({})",
                        fallback.requested_runtime, fallback.selected_runtime, fallback.reason
                    ));
                }
            }
            if let Some(source) = run.activity_source.as_deref() {
                lines.push(format!(
                    "Last activity: {} via {}",
                    run.last_activity_at_ms
                        .map(|timestamp| timestamp.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    source
                ));
            }
            lines.push(format!("Stalled: {}", yes_no(run.stalled)));
            if let Some(reason) = run.stall_reason.as_deref() {
                lines.push(format!("Stall detail: {}", reason));
            }
            lines.push(format!("Proof reuse: {}", proof_reuse_summary(run)));
            lines
        })
        .unwrap_or_else(|| vec!["No active replay state.".to_string()]);
    if let Some(wave_id) = selected_wave_id {
        if let Some(package) = acceptance_package_for_wave(snapshot, wave_id) {
            items.extend(acceptance_package_status_items(package));
        }
        if let Some(detail) = design_detail_for_wave(snapshot, wave_id) {
            items.push(format!("Design: {:?}", detail.completeness));
            if !detail.active_contradictions.is_empty() {
                items.push(format!(
                    "Contradictions: {}",
                    detail
                        .active_contradictions
                        .iter()
                        .map(|contradiction| format!(
                            "{}:{}",
                            contradiction.contradiction_id, contradiction.state
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !detail.unresolved_question_ids.is_empty() {
                items.push(format!(
                    "Open questions: {}",
                    detail.unresolved_question_ids.join(", ")
                ));
            }
            if !detail.unresolved_assumption_ids.is_empty() {
                items.push(format!(
                    "Open assumptions: {}",
                    detail.unresolved_assumption_ids.join(", ")
                ));
            }
            if !detail.pending_human_inputs.is_empty() {
                items.push(format!(
                    "Pending human input: {}",
                    detail
                        .pending_human_inputs
                        .iter()
                        .map(|request| format!("{} via {}", request.request_id, request.route))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if !detail.dependency_handshake_routes.is_empty() {
                items.push(format!(
                    "Dependency handshakes: {}",
                    detail.dependency_handshake_routes.join(", ")
                ));
            }
            if !detail.invalidated_fact_ids.is_empty() {
                items.push(format!(
                    "Invalidated facts: {}",
                    detail.invalidated_fact_ids.join(", ")
                ));
            }
            if !detail.invalidated_decision_ids.is_empty() {
                items.push(format!(
                    "Invalidated decisions: {}",
                    detail.invalidated_decision_ids.join(", ")
                ));
            }
            if !detail.invalidation_routes.is_empty() {
                items.extend(
                    detail
                        .invalidation_routes
                        .iter()
                        .map(|route| format!("Invalidation route: {}", route)),
                );
            }
            if !detail.superseded_decision_ids.is_empty() {
                items.push(format!(
                    "Superseded decisions: {}",
                    detail.superseded_decision_ids.join(", ")
                ));
            }
            if !detail.selectively_invalidated_task_ids.is_empty() {
                items.push(format!(
                    "Selective rerun: {}",
                    detail.selectively_invalidated_task_ids.join(", ")
                ));
            }
            if !detail.ambiguous_dependency_wave_ids.is_empty() {
                items.push(format!(
                    "Ambiguous dependencies: {}",
                    detail
                        .ambiguous_dependency_wave_ids
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }
    let request_queue = snapshot
        .operator_objects
        .iter()
        .filter(|item| {
            selected_wave_id
                .map(|wave_id| item.wave_id == wave_id)
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    if let Some(wave_id) = selected_wave_id {
        if let Some((selected_index, actionable_count, item)) =
            selected_actionable_operator_context(state, snapshot, wave_id)
        {
            items.push(format!(
                "Selected operator action: {}/{}",
                selected_index + 1,
                actionable_count
            ));
            if let Some(waiting_on) = item.waiting_on.as_deref() {
                items.push(format!("Waiting on: {}", waiting_on));
            }
            if let Some(next_action) = item.next_action.as_deref() {
                items.push(format!("Next operator action: {}", next_action));
            }
        }
    }
    if request_queue.is_empty() {
        items.push("Request queue: empty".to_string());
    } else {
        items.push(format!("Request queue: {} items", request_queue.len()));
        for item in request_queue {
            let is_selected = selected_wave_id
                .filter(|wave_id| *wave_id == item.wave_id)
                .and_then(|wave_id| selected_actionable_operator_item(state, snapshot, wave_id))
                .is_some_and(|selected| {
                    selected.record_id == item.record_id && selected.kind == item.kind
                });
            items.extend(operator_object_lines(
                item,
                selected_wave_id.is_none(),
                is_selected,
            ));
        }
    }
    items.push(format!(
        "Launcher boundary: {}",
        snapshot.launcher.executor_boundary
    ));
    items.push(format!(
        "Launcher selection policy: {}",
        snapshot.launcher.selection_policy
    ));
    items.push(format!(
        "Launcher fallback policy: {}",
        snapshot.launcher.fallback_policy
    ));
    items.push(format!(
        "Launcher available: {}",
        format_string_list(&snapshot.launcher.available_runtimes)
    ));
    items.push(format!(
        "Launcher unavailable: {}",
        format_string_list(&snapshot.launcher.unavailable_runtimes)
    ));
    items.extend(snapshot.launcher.runtimes.iter().map(|runtime| {
        format!(
            "availability {}: {} ({})",
            runtime.runtime,
            if runtime.available {
                "ready"
            } else {
                "blocked"
            },
            runtime.detail
        )
    }));
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

fn design_detail_for_wave(
    snapshot: &OperatorSnapshot,
    wave_id: u32,
) -> Option<&wave_app_server::WaveDesignDetail> {
    snapshot
        .design_details
        .iter()
        .find(|detail| detail.wave_id == wave_id)
}

fn wave_status_for_wave(
    snapshot: &OperatorSnapshot,
    wave_id: u32,
) -> Option<&wave_control_plane::WaveStatusReadModel> {
    snapshot
        .planning
        .waves
        .iter()
        .find(|wave| wave.id == wave_id)
}

fn relevant_run_detail_for_wave(
    snapshot: &OperatorSnapshot,
    wave_id: u32,
) -> Option<&ActiveRunDetail> {
    snapshot
        .latest_run_details
        .iter()
        .find(|run| run.wave_id == wave_id)
        .or_else(|| {
            snapshot
                .active_run_details
                .iter()
                .find(|run| run.wave_id == wave_id)
        })
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

fn blocker_summary_lines<'a>(
    snapshot: &'a OperatorSnapshot,
    selected_wave_id: Option<u32>,
) -> Vec<Line<'a>> {
    let mut lines = vec![
        Line::raw(format!(
            "queue blockers: dependency={} design={} closure={} ownership={} lease_expired={} budget={} active_run={}",
            snapshot.panels.queue.blocker_summary.dependency,
            snapshot.panels.queue.blocker_summary.design,
            snapshot.panels.queue.blocker_summary.closure,
            snapshot.panels.queue.blocker_summary.ownership,
            snapshot.panels.queue.blocker_summary.lease_expired,
            snapshot.panels.queue.blocker_summary.budget,
            snapshot.panels.queue.blocker_summary.active_run
        )),
        Line::raw(format!(
            "operator objects: approvals={} overrides={} escalations={}",
            snapshot
                .operator_objects
                .iter()
                .filter(|item| matches!(
                    item.kind,
                    wave_app_server::OperatorActionableKind::Approval
                ))
                .count(),
            snapshot
                .operator_objects
                .iter()
                .filter(|item| matches!(
                    item.kind,
                    wave_app_server::OperatorActionableKind::Override
                ))
                .count(),
            snapshot
                .operator_objects
                .iter()
                .filter(|item| matches!(
                    item.kind,
                    wave_app_server::OperatorActionableKind::Escalation
                ))
                .count(),
        )),
    ];
    if let Some(wave_id) = selected_wave_id {
        if let Some(detail) = design_detail_for_wave(snapshot, wave_id) {
            lines.push(Line::raw(format!(
                "wave {} design={:?} blockers={}",
                wave_id,
                detail.completeness,
                detail.blocker_reasons.len()
            )));
            lines.push(Line::raw(format!(
                "lineage: questions={} assumptions={} invalidated_facts={} invalidated_decisions={} superseded={} contradictions={}",
                detail.unresolved_question_ids.len(),
                detail.unresolved_assumption_ids.len(),
                detail.invalidated_fact_ids.len(),
                detail.invalidated_decision_ids.len(),
                detail.superseded_decision_ids.len(),
                detail.active_contradictions.len()
            )));
            if !detail.ambiguous_dependency_wave_ids.is_empty() {
                lines.push(Line::raw(format!(
                    "ambiguous dependencies: {}",
                    detail
                        .ambiguous_dependency_wave_ids
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }
        if let Some(run) = relevant_run_detail_for_wave(snapshot, wave_id) {
            lines.push(Line::raw(format!(
                "wave {} promotion: merge_blocked={} closure_blocked={}",
                wave_id,
                yes_no(run.execution.merge_blocked),
                yes_no(run.execution.closure_blocked_by_promotion)
            )));
        }
    }
    lines
}

fn blocker_item_lines(
    snapshot: &OperatorSnapshot,
    state: &AppState,
    selected_wave_id: Option<u32>,
) -> Vec<String> {
    let mut items = Vec::new();
    if let Some(wave_id) = selected_wave_id {
        if let Some(wave) = wave_status_for_wave(snapshot, wave_id) {
            items.extend(wave_blocker_lines(wave, false, true));
        }
        if let Some(run) = relevant_run_detail_for_wave(snapshot, wave_id) {
            items.extend(run_blocker_lines(run, false));
        }
        if let Some(package) = acceptance_package_for_wave(snapshot, wave_id) {
            items.extend(acceptance_blocker_lines(package, false));
        }
        if let Some(detail) = design_detail_for_wave(snapshot, wave_id) {
            for contradiction in &detail.active_contradictions {
                items.push(format!(
                    "contradiction  {}  {}  state={}",
                    contradiction.contradiction_id, contradiction.summary, contradiction.state
                ));
                if !contradiction.invalidated_refs.is_empty() {
                    items.push(format!(
                        "  invalidates={}",
                        contradiction.invalidated_refs.join(", ")
                    ));
                }
                if let Some(detail) = contradiction.detail.as_deref() {
                    items.push(format!("  detail={detail}"));
                }
            }
            for question_id in &detail.unresolved_question_ids {
                items.push(format!("question  {question_id}"));
            }
            for assumption_id in &detail.unresolved_assumption_ids {
                items.push(format!("assumption  {assumption_id}"));
            }
            for blocker in &detail.blocker_reasons {
                if !is_known_design_blocker(blocker) {
                    items.push(format!("blocker  {blocker}"));
                }
            }
            for fact_id in &detail.invalidated_fact_ids {
                items.push(format!("invalidated-fact  {fact_id}"));
            }
            for decision_id in &detail.invalidated_decision_ids {
                items.push(format!("invalidated-decision  {decision_id}"));
            }
            for route in &detail.invalidation_routes {
                items.push(format!("invalidation  {route}"));
            }
            for decision_id in &detail.superseded_decision_ids {
                items.push(format!("superseded-decision  {decision_id}"));
            }
            for dependency_wave_id in &detail.ambiguous_dependency_wave_ids {
                items.push(format!("dependency-ambiguity  wave-{dependency_wave_id}"));
            }
            for request in &detail.pending_human_inputs {
                items.push(format!(
                    "{}  {}  state={}  route={}  task={}",
                    operator_object_kind_label(wave_app_server::OperatorActionableKind::Approval),
                    request.request_id,
                    debug_label(request.state),
                    request.route,
                    request
                        .task_id
                        .clone()
                        .unwrap_or_else(|| "none".to_string())
                ));
            }
        }
        if let Some((selected_index, actionable_count, item)) =
            selected_actionable_operator_context(state, snapshot, wave_id)
        {
            items.push(format!(
                "selected-operator-action  {}/{}  {}",
                selected_index + 1,
                actionable_count,
                item.record_id
            ));
        }
        for item in snapshot
            .operator_objects
            .iter()
            .filter(|item| item.wave_id == wave_id)
        {
            let is_selected = selected_actionable_operator_item(state, snapshot, wave_id)
                .is_some_and(|selected| {
                    selected.record_id == item.record_id && selected.kind == item.kind
                });
            items.extend(operator_object_lines(item, false, is_selected));
        }
    } else {
        for wave in snapshot.planning.waves.iter().filter(|wave| {
            !wave.ready && (!wave.blocker_state.is_empty() || !wave.blocked_by.is_empty())
        }) {
            items.extend(wave_blocker_lines(wave, true, true));
        }
        for detail in &snapshot.design_details {
            for contradiction in &detail.active_contradictions {
                items.push(format!(
                    "contradiction  wave {}  {}  {}  state={}",
                    detail.wave_id,
                    contradiction.contradiction_id,
                    contradiction.summary,
                    contradiction.state
                ));
                if !contradiction.invalidated_refs.is_empty() {
                    items.push(format!(
                        "  invalidates={}",
                        contradiction.invalidated_refs.join(", ")
                    ));
                }
                if let Some(detail) = contradiction.detail.as_deref() {
                    items.push(format!("  detail={detail}"));
                }
            }
            for question_id in &detail.unresolved_question_ids {
                items.push(format!(
                    "question  wave {}  {}",
                    detail.wave_id, question_id
                ));
            }
            for assumption_id in &detail.unresolved_assumption_ids {
                items.push(format!(
                    "assumption  wave {}  {}",
                    detail.wave_id, assumption_id
                ));
            }
            for fact_id in &detail.invalidated_fact_ids {
                items.push(format!(
                    "invalidated-fact  wave {}  {}",
                    detail.wave_id, fact_id
                ));
            }
            for decision_id in &detail.invalidated_decision_ids {
                items.push(format!(
                    "invalidated-decision  wave {}  {}",
                    detail.wave_id, decision_id
                ));
            }
            for route in &detail.invalidation_routes {
                items.push(format!("invalidation  wave {}  {}", detail.wave_id, route));
            }
            for decision_id in &detail.superseded_decision_ids {
                items.push(format!(
                    "superseded-decision  wave {}  {}",
                    detail.wave_id, decision_id
                ));
            }
            for dependency_wave_id in &detail.ambiguous_dependency_wave_ids {
                items.push(format!(
                    "dependency-ambiguity  wave {}  wave-{}",
                    detail.wave_id, dependency_wave_id
                ));
            }
        }
        for run in snapshot
            .latest_run_details
            .iter()
            .filter(|run| run.execution.merge_blocked || run.execution.closure_blocked_by_promotion)
        {
            items.extend(run_blocker_lines(run, true));
        }
        for package in &snapshot.acceptance_packages {
            items.extend(acceptance_blocker_lines(package, true));
        }
        for item in &snapshot.operator_objects {
            items.extend(operator_object_lines(item, true, false));
        }
    }
    if items.is_empty() {
        items.push("No blocker triage items.".to_string());
    }
    items
}

fn acceptance_blocker_lines(
    package: &wave_app_server::AcceptancePackageSnapshot,
    include_wave: bool,
) -> Vec<String> {
    let mut items = Vec::new();
    let prefix = if include_wave {
        format!("wave {}  ", package.wave_id)
    } else {
        String::new()
    };

    for blocker in &package.blocking_reasons {
        items.push(format!("acceptance  {prefix}{blocker}"));
    }
    for risk in &package.known_risks {
        items.push(format!("risk  {prefix}{}  {}", risk.code, risk.summary));
        if let Some(detail) = risk.detail.as_deref() {
            items.push(format!("  detail={detail}"));
        }
    }
    for debt in &package.outstanding_debt {
        items.push(format!("debt  {prefix}{}  {}", debt.code, debt.summary));
        if let Some(detail) = debt.detail.as_deref() {
            items.push(format!("  detail={detail}"));
        }
    }

    items
}

fn wave_blocker_lines(
    wave: &wave_control_plane::WaveStatusReadModel,
    include_wave: bool,
    suppress_known_design: bool,
) -> Vec<String> {
    if !wave.blocker_state.is_empty() {
        wave.blocker_state
            .iter()
            .filter(|blocker| {
                !(suppress_known_design
                    && blocker.raw.starts_with("design:")
                    && is_known_design_blocker(&blocker.raw))
            })
            .map(|blocker| {
                format_blocker_line(
                    include_wave,
                    wave.id,
                    &blocker.raw,
                    blocker.detail.as_deref(),
                )
            })
            .collect()
    } else {
        wave.blocked_by
            .iter()
            .filter(|blocker| {
                !(suppress_known_design
                    && blocker.starts_with("design:")
                    && is_known_design_blocker(blocker))
            })
            .map(|blocker| format_blocker_line(include_wave, wave.id, blocker, None))
            .collect()
    }
}

fn run_blocker_lines(run: &ActiveRunDetail, include_wave: bool) -> Vec<String> {
    let mut items = Vec::new();
    if run.execution.merge_blocked {
        let prefix = blocker_prefix("promotion", include_wave, run.wave_id);
        let detail = run
            .execution
            .promotion
            .as_ref()
            .and_then(|promotion| promotion.detail.as_deref())
            .unwrap_or("merge blocked by promotion conflict");
        items.push(format!("{prefix} merge-blocked  {detail}"));
        if let Some(promotion) = run.execution.promotion.as_ref() {
            if !promotion.conflict_paths.is_empty() {
                items.push(format!(
                    "{prefix} conflict-paths={}",
                    promotion.conflict_paths.join(", ")
                ));
            }
        }
    }
    if run.execution.closure_blocked_by_promotion {
        let prefix = blocker_prefix("promotion", include_wave, run.wave_id);
        items.push(format!(
            "{prefix} closure-blocked  waiting for promotion to clear"
        ));
    }
    items
}

fn operator_object_lines(
    item: &wave_app_server::OperatorActionableItem,
    include_wave: bool,
    selected: bool,
) -> Vec<String> {
    let prefix = if selected { "> " } else { "" };
    let mut lines = vec![if include_wave {
        format!(
            "{prefix}{}  wave {}  {}  state={}",
            operator_object_kind_label(item.kind),
            item.wave_id,
            item.summary,
            item.state
        )
    } else {
        format!(
            "{prefix}{}  {}  {}  state={}",
            operator_object_kind_label(item.kind),
            item.record_id,
            item.summary,
            item.state
        )
    }];
    let mut context = Vec::new();
    if let Some(route) = item.route.as_deref() {
        context.push(format!("route={route}"));
    }
    if let Some(task_id) = item.task_id.as_deref() {
        context.push(format!("task={task_id}"));
    }
    if let Some(source_run_id) = item.source_run_id.as_deref() {
        context.push(format!("source_run={source_run_id}"));
    }
    if item.evidence_count > 0 {
        context.push(format!("evidence={}", item.evidence_count));
    }
    if let Some(waiting_on) = item.waiting_on.as_deref() {
        context.push(format!("waiting_on={waiting_on}"));
    }
    if let Some(next_action) = item.next_action.as_deref() {
        context.push(format!("next_action={next_action}"));
    }
    if !context.is_empty() {
        lines.push(format!("  {}", context.join("  ")));
    }
    if let Some(detail) = item.detail.as_deref() {
        lines.push(format!("  detail={detail}"));
    }
    lines
}

fn format_blocker_line(
    include_wave: bool,
    wave_id: u32,
    raw: &str,
    detail: Option<&str>,
) -> String {
    if let Some(line) = format_design_blocker_line(include_wave, wave_id, raw) {
        return line;
    }
    let (kind, fallback_detail) = if let Some(value) = raw.strip_prefix("wave:") {
        ("dependency", value)
    } else if raw == "lint:error" {
        ("lint", "error")
    } else if let Some(value) = raw.strip_prefix("closure:") {
        if value.starts_with("promotion-blocked:") {
            ("promotion", value)
        } else {
            ("closure", value)
        }
    } else if let Some(value) = raw.strip_prefix("ownership:") {
        ("ownership", value)
    } else if let Some(value) = raw.strip_prefix("lease-expired:") {
        ("lease", value)
    } else if let Some(value) = raw.strip_prefix("budget:") {
        ("budget", value)
    } else if let Some(value) = raw.strip_prefix("active-run:") {
        ("active-run", value)
    } else if raw == "already-completed" {
        ("completed", "already completed")
    } else {
        ("other", raw)
    };
    let prefix = blocker_prefix(kind, include_wave, wave_id);
    format!("{} {}", prefix, detail.unwrap_or(fallback_detail))
}

fn is_known_design_blocker(raw: &str) -> bool {
    matches!(
        raw.strip_prefix("design:"),
        Some(value)
            if value.starts_with("open-question:")
                || value.starts_with("open-assumption:")
                || value.starts_with("human-input:")
                || value.starts_with("invalidated-fact:")
                || value.starts_with("invalidated-decision:")
                || value.starts_with("downstream-task-invalidated:")
                || value.starts_with("dependency-ambiguity:")
    )
}

fn format_design_blocker_line(include_wave: bool, wave_id: u32, raw: &str) -> Option<String> {
    let value = raw.strip_prefix("design:")?;
    let (label, detail) = if let Some(value) = value.strip_prefix("open-question:") {
        ("question", value)
    } else if let Some(value) = value.strip_prefix("open-assumption:") {
        ("assumption", value)
    } else if let Some(value) = value.strip_prefix("human-input:") {
        ("approval-request", value)
    } else if let Some(value) = value.strip_prefix("invalidated-fact:") {
        ("invalidated-fact", value)
    } else if let Some(value) = value.strip_prefix("invalidated-decision:") {
        ("invalidated-decision", value)
    } else if let Some(value) = value.strip_prefix("downstream-task-invalidated:") {
        ("selective-rerun", value)
    } else if let Some(value) = value.strip_prefix("dependency-ambiguity:") {
        ("dependency-ambiguity", value)
    } else {
        return None;
    };
    Some(if include_wave {
        format!("{label}  wave {wave_id}  {detail}")
    } else {
        format!("{label}  {detail}")
    })
}

fn blocker_prefix(kind: &str, include_wave: bool, wave_id: u32) -> String {
    if include_wave {
        format!("{kind}  wave {wave_id}")
    } else {
        kind.to_string()
    }
}

fn operator_object_kind_label(kind: wave_app_server::OperatorActionableKind) -> &'static str {
    match kind {
        wave_app_server::OperatorActionableKind::Approval => "approval-request",
        wave_app_server::OperatorActionableKind::Override => "manual-close-override",
        wave_app_server::OperatorActionableKind::Escalation => "escalation",
    }
}

fn manual_close_status_items(
    root: &Path,
    config: &ProjectConfig,
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) -> Vec<String> {
    let Some(wave_id) = selected_wave_id else {
        return Vec::new();
    };
    let mut items = Vec::new();
    if let Some(record) = snapshot
        .closure_overrides
        .iter()
        .find(|record| record.wave_id == wave_id)
    {
        items.push(format!(
            "Manual close: {} ({})",
            if record.is_active() {
                "applied"
            } else {
                "cleared"
            },
            record.reason
        ));
        items.push(format!("Manual close source run: {}", record.source_run_id));
        items.push(format!(
            "Manual close evidence files: {}",
            record.evidence_paths.len()
        ));
        if record.is_active() {
            return items;
        }
    }

    match preview_closure_override(root, config, wave_id, None, Vec::new()) {
        Ok(preview) => {
            items.push("Manual close: available".to_string());
            items.push(format!(
                "Manual close source run: {}",
                preview.source_run_id
            ));
            items.push(format!(
                "Manual close evidence files: {}",
                preview.evidence_paths.len()
            ));
        }
        Err(error) => {
            items.push(format!("Manual close: blocked ({error})"));
        }
    }
    items
}

fn pending_control_action_lines(pending_action: &PendingControlAction) -> Vec<String> {
    match pending_action {
        PendingControlAction::ApplyManualClose(confirmation) => vec![
            format!(
                "Confirm manual close: wave {} {}",
                confirmation.wave_id, confirmation.wave_title
            ),
            format!("Source run: {}", confirmation.source_run_id),
            format!("Evidence files: {}", confirmation.evidence_paths.len()),
            format!("Summary: {}", confirmation.summary),
            "Enter apply  Esc cancel".to_string(),
        ],
        PendingControlAction::ClearManualClose(confirmation) => vec![
            format!(
                "Confirm manual close clear: wave {} {}",
                confirmation.wave_id, confirmation.wave_title
            ),
            format!("Source run: {}", confirmation.source_run_id),
            "Enter clear  Esc cancel".to_string(),
        ],
        PendingControlAction::ApproveOperatorAction(confirmation) => {
            let mut lines = vec![
                format!(
                    "Confirm operator action: wave {} {}",
                    confirmation.wave_id, confirmation.wave_title
                ),
                format!(
                    "Approve {} {}",
                    operator_object_kind_label(confirmation.kind),
                    confirmation.record_id
                ),
                format!("Summary: {}", confirmation.summary),
            ];
            if let Some(waiting_on) = confirmation.waiting_on.as_deref() {
                lines.push(format!("Waiting on: {}", waiting_on));
            }
            if let Some(next_action) = confirmation.next_action.as_deref() {
                lines.push(format!("Next action: {}", next_action));
            }
            lines.push("Enter approve  Esc cancel".to_string());
            lines
        }
        PendingControlAction::RejectOperatorAction(confirmation) => {
            let mut lines = vec![
                format!(
                    "Confirm operator rejection: wave {} {}",
                    confirmation.wave_id, confirmation.wave_title
                ),
                format!(
                    "Reject {} {}",
                    operator_object_kind_label(confirmation.kind),
                    confirmation.record_id
                ),
                format!("Summary: {}", confirmation.summary),
            ];
            if let Some(waiting_on) = confirmation.waiting_on.as_deref() {
                lines.push(format!("Waiting on: {}", waiting_on));
            }
            if let Some(next_action) = confirmation.next_action.as_deref() {
                lines.push(format!("Next action: {}", next_action));
            }
            lines.push("Enter reject  Esc cancel".to_string());
            lines
        }
    }
}

fn flash_message_style(kind: FlashMessageKind) -> Style {
    let color = match kind {
        FlashMessageKind::Info => Color::LightGreen,
        FlashMessageKind::Error => Color::LightRed,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
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
    use wave_control_plane::QueueBlockerKindReadModel;
    use wave_control_plane::QueueBlockerReadModel;
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

    fn default_delivery() -> wave_control_plane::DeliveryReadModel {
        wave_control_plane::DeliveryReadModel::default()
    }

    fn render_test_lines(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn sample_acceptance_package(wave_id: u32) -> wave_app_server::AcceptancePackageSnapshot {
        wave_app_server::AcceptancePackageSnapshot {
            package_id: format!("acceptance-package-wave-{wave_id}"),
            wave_id,
            wave_slug: format!("wave-{wave_id}"),
            wave_title: format!("Wave {wave_id}"),
            run_id: Some(format!("wave-{wave_id}-test")),
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
                        status: Some(WaveRunStatus::Failed),
                        proof_complete: false,
                        satisfied: false,
                        error: Some("design review blocked".to_string()),
                    },
                    wave_app_server::AcceptanceClosureAgentSnapshot {
                        agent_id: "A8".to_string(),
                        title: Some("Integration Steward".to_string()),
                        status: Some(WaveRunStatus::Planned),
                        proof_complete: false,
                        satisfied: false,
                        error: None,
                    },
                    wave_app_server::AcceptanceClosureAgentSnapshot {
                        agent_id: "A9".to_string(),
                        title: Some("Wave Documentation Steward".to_string()),
                        status: Some(WaveRunStatus::Succeeded),
                        proof_complete: true,
                        satisfied: true,
                        error: None,
                    },
                    wave_app_server::AcceptanceClosureAgentSnapshot {
                        agent_id: "A0".to_string(),
                        title: Some("Running cont-QA".to_string()),
                        status: Some(WaveRunStatus::Succeeded),
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
        let app = test_app(AppState::default());
        let lines = narrow_summary_lines(&snapshot, &app);
        let rendered = render_test_lines(&lines);

        assert!(rendered.contains("Run"));
        assert!(rendered.contains("Agents"));
        assert!(rendered.contains("Queue"));
        assert!(rendered.contains("Control"));
        assert!(rendered.contains("ready: 6"));
        assert!(rendered.contains("A1 TUI Shell And Layout Scaffold"));
        assert!(rendered.contains(
            "worktree: worktree-wave-05-test -> .wave/state/worktrees/wave-05-test (allocated)"
        ));
        assert!(rendered.contains("promotion: ready"));
        assert!(rendered.contains("closure protection: closure capacity reserved before A8"));
        assert!(rendered.contains("merge blocked: no"));
        assert!(rendered.contains("closure blocked: no"));
        assert!(rendered
            .contains("closure capacity: reserved_slots=1 reservation_active=yes preemption=yes"));
        assert!(rendered.contains(
            "actions: rerun=yes clear-rerun=yes manual-close=yes clear-manual-close=yes approve=yes reject=yes launch=yes autonomous=yes"
        ));
        assert!(rendered.contains("keys: Tab Shift+Tab j k r c m M u x Enter Esc q"));
    }

    #[test]
    fn portfolio_focus_lines_show_delivery_summary_without_portfolio_model() {
        let mut snapshot = test_snapshot();
        snapshot.acceptance_packages = vec![sample_acceptance_package(5)];

        let rendered = render_test_lines(&portfolio_focus_lines(&snapshot, Some(5)));

        assert!(rendered.contains(
            "delivery: ship=no_ship release=building_evidence signoff=pending_evidence proof=2/6 complete=no source=mixed-envelope-and-compatibility risks=1 debt=1"
        ));
        assert!(rendered.contains(
            "delivery blockers: implementation proof is only 2/6 complete | signoff cannot begin until proof and release evidence are complete"
        ));
    }

    #[test]
    fn portfolio_overview_lines_show_multiple_delivery_packets() {
        let mut snapshot = test_snapshot();
        snapshot.acceptance_packages =
            vec![sample_acceptance_package(5), sample_acceptance_package(6)];

        let rendered = render_test_lines(&portfolio_overview_lines(&snapshot, Some(5)));

        assert!(rendered.contains("overview: initiatives=0 milestones=0 release_trains=0 outcome_contracts=0 mapped_waves=0"));
        assert!(rendered.contains("> wave 5 Wave 5 ship=no_ship release=building_evidence signoff=pending_evidence proof=2/6 risks=1 debt=1"));
        assert!(rendered.contains("wave 6 Wave 6 ship=no_ship release=building_evidence signoff=pending_evidence proof=2/6 risks=1 debt=1"));
    }

    #[test]
    fn proof_lines_itemize_risks_and_debt() {
        let rendered = render_test_lines(&acceptance_package_summary_lines(
            &sample_acceptance_package(5),
        ));

        assert!(rendered.contains("risk agent-error: agent A6 failed"));
        assert!(rendered.contains("risk detail: design review blocked"));
        assert!(
            rendered.contains("debt proof-incomplete: implementation proof is incomplete (2/6)")
        );
        assert!(rendered.contains("debt detail: proof source mixed-envelope-and-compatibility"));
    }

    #[test]
    fn execution_lines_label_waiting_and_preemption_with_operator_language() {
        let waiting = wave_control_plane::WaveExecutionState {
            worktree: None,
            promotion: None,
            scheduling: Some(wave_domain::WaveSchedulingRecord {
                wave_id: 14,
                phase: wave_domain::WaveExecutionPhase::Implementation,
                priority: wave_domain::WaveSchedulerPriority::Implementation,
                state: wave_domain::WaveSchedulingState::Waiting,
                fairness_rank: 2,
                waiting_since_ms: Some(9),
                protected_closure_capacity: false,
                preemptible: true,
                last_decision: Some(
                    "waiting because closure capacity is reserved ahead of new implementation work"
                        .to_string(),
                ),
                updated_at_ms: 10,
            }),
            merge_blocked: false,
            closure_blocked_by_promotion: false,
        };
        let preempted = wave_control_plane::WaveExecutionState {
            worktree: None,
            promotion: None,
            scheduling: Some(wave_domain::WaveSchedulingRecord {
                wave_id: 22,
                phase: wave_domain::WaveExecutionPhase::Implementation,
                priority: wave_domain::WaveSchedulerPriority::Implementation,
                state: wave_domain::WaveSchedulingState::Preempted,
                fairness_rank: 1,
                waiting_since_ms: Some(11),
                protected_closure_capacity: false,
                preemptible: true,
                last_decision: Some(
                    "preempted to free closure capacity for wave 20 agent A8".to_string(),
                ),
                updated_at_ms: 12,
            }),
            merge_blocked: false,
            closure_blocked_by_promotion: false,
        };

        let waiting_rendered = execution_lines(
            &waiting,
            Some(&wave_control_plane::SchedulerBudgetState {
                max_active_wave_claims: Some(2),
                max_active_task_leases: Some(2),
                reserved_closure_task_leases: Some(1),
                active_wave_claims: 1,
                active_task_leases: 1,
                active_implementation_task_leases: 1,
                active_closure_task_leases: 0,
                closure_capacity_reserved: true,
                preemption_enabled: true,
                budget_blocked: false,
            }),
        )
        .into_iter()
        .flat_map(|line| line.spans.into_iter().map(|span| span.content.into_owned()))
        .collect::<Vec<_>>()
        .join("\n");
        assert!(waiting_rendered.contains(
            "wait reason: waiting because closure capacity is reserved ahead of new implementation work"
        ));
        assert!(waiting_rendered
            .contains("closure reservation: waiting closure work is holding protected capacity"));

        let preempted_rendered = execution_lines(&preempted, None)
            .into_iter()
            .flat_map(|line| line.spans.into_iter().map(|span| span.content.into_owned()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(preempted_rendered
            .contains("preemption: preempted to free closure capacity for wave 20 agent A8"));
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
            last_activity_at_ms: Some(4),
            activity_source: Some("events".to_string()),
            stalled: false,
            stall_reason: None,
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
            runtime_summary: wave_app_server::RuntimeSummary {
                selected_runtimes: vec!["codex".to_string()],
                requested_runtimes: vec!["codex".to_string()],
                selection_sources: vec!["executor.id".to_string()],
                fallback_targets: Vec::new(),
                fallback_count: 0,
                agents_with_runtime: 1,
            },
            execution: empty_execution(),
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
                design_completeness: wave_domain::DesignCompletenessState::ImplementationReady,
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
                closure_override_applied: false,
                completed: false,
                last_run_status: Some(WaveRunStatus::Running),
                soft_state: wave_domain::SoftState::Clear,
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
    fn run_tab_prefers_active_run_execution_transport_over_planning_execution() {
        let mut snapshot = test_snapshot();
        let planning_wave = snapshot
            .planning
            .waves
            .iter_mut()
            .find(|wave| wave.id == 5)
            .expect("wave 5 in planning snapshot");
        planning_wave.execution = empty_execution();

        let (execution, budget) =
            selected_execution_for_run_tab(&snapshot, Some(5)).expect("selected execution");

        assert_eq!(
            execution
                .worktree
                .as_ref()
                .map(|worktree| worktree.path.as_str()),
            Some(".wave/state/worktrees/wave-05-test")
        );
        assert_eq!(
            execution
                .promotion
                .as_ref()
                .map(|promotion| promotion.state),
            Some(wave_domain::WavePromotionState::Ready)
        );
        assert!(budget.is_some());
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
        let state = AppState::default();
        snapshot.control_status.closure_attention_lines = vec!["closure gap: custom".to_string()];
        snapshot.control_status.skill_issue_lines = vec!["skill issue: custom".to_string()];

        let rendered = control_status_items(&snapshot, &state, Some(5));

        assert!(rendered
            .iter()
            .any(|line| line == "Replay OK for wave 5 run wave-05-test"));
        assert!(rendered
            .iter()
            .any(|line| line == "Open questions: question-api-shape"));
        assert!(rendered
            .iter()
            .any(|line| line == "Open assumptions: assumption-cache-valid"));
        assert!(rendered
            .iter()
            .any(|line| line == "Invalidated facts: fact-api"));
        assert!(rendered
            .iter()
            .any(|line| line == "Superseded decisions: decision-api-v1"));
        assert!(rendered
            .iter()
            .any(|line| line == "Ambiguous dependencies: 4"));
        assert!(rendered
            .iter()
            .any(|line| line == "Selected operator action: 1/2"));
        assert!(rendered
            .iter()
            .any(|line| line == "Waiting on: operator dependency approval"));
        assert!(rendered
            .iter()
            .any(|line| line == "Next operator action: press u to approve or x to reject"));
        assert!(rendered.iter().any(|line| line == "Request queue: 2 items"));
        assert!(rendered.iter().any(|line| {
            line == "> approval-request  human-5  Need dependency confirmation  state=pending"
        }));
        assert!(rendered.iter().any(|line| {
            line
                == "  route=dependency:wave-04  task=wave-05:agent-a1  waiting_on=operator dependency approval  next_action=press u to approve or x to reject"
        }));
        assert!(rendered
            .iter()
            .any(|line| line == "escalation  esc-5  Need operator review  state=open"));
        assert!(rendered.iter().any(|line| line == "closure gap: custom"));
        assert!(rendered.iter().any(|line| line == "skill issue: custom"));
    }

    #[test]
    fn run_summary_lines_keep_mixed_runtime_runs_explicit() {
        let mut snapshot = test_snapshot();
        let run = snapshot
            .active_run_details
            .first_mut()
            .expect("active run detail");
        run.runtime_summary.selected_runtimes = vec!["claude".to_string(), "codex".to_string()];
        run.runtime_summary.requested_runtimes = vec!["claude".to_string(), "codex".to_string()];
        run.runtime_summary.agents_with_runtime = 2;
        let mut codex_agent = run.agents[0].clone();
        codex_agent.id = "A2".to_string();
        codex_agent.title = "Codex Adapter".to_string();
        codex_agent.runtime = Some(wave_app_server::RuntimeDetail {
            selected_runtime: "codex".to_string(),
            selection_reason: "selected codex".to_string(),
            policy: wave_app_server::RuntimePolicyDetail {
                requested_runtime: Some("codex".to_string()),
                allowed_runtimes: vec!["codex".to_string(), "claude".to_string()],
                fallback_runtimes: vec!["claude".to_string()],
                selection_source: Some("executor.id".to_string()),
                uses_fallback: false,
            },
            fallback: None,
            execution_identity: wave_app_server::RuntimeExecutionIdentityDetail {
                adapter: "wave-runtime/codex".to_string(),
                binary: "codex".to_string(),
                provider: "openai-codex-cli".to_string(),
                artifact_paths: std::collections::BTreeMap::new(),
            },
            skill_projection: wave_app_server::RuntimeSkillProjectionDetail {
                declared_skills: vec!["wave-core".to_string()],
                projected_skills: vec!["wave-core".to_string()],
                dropped_skills: Vec::new(),
                auto_attached_skills: vec!["runtime-codex".to_string()],
            },
        });
        run.agents.push(codex_agent);

        let rendered = run_summary_lines(run)
            .into_iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert!(rendered
            .iter()
            .any(|line| line == "selected runtimes: claude, codex"));
        assert!(rendered.iter().any(|line| line
            == "current agent runtime: requested codex -> selected claude via executor.id"));
        assert!(rendered
            .iter()
            .all(|line| !line.starts_with("runtime decision:")));
    }

    #[test]
    fn control_items_keep_mixed_runtime_runs_explicit() {
        let mut snapshot = test_snapshot();
        let state = AppState::default();
        let run = snapshot
            .active_run_details
            .first_mut()
            .expect("active run detail");
        run.runtime_summary.selected_runtimes = vec!["claude".to_string(), "codex".to_string()];
        run.runtime_summary.requested_runtimes = vec!["claude".to_string(), "codex".to_string()];

        let rendered = control_status_items(&snapshot, &state, Some(5));

        assert!(rendered
            .iter()
            .any(|line| line == "Run runtimes: claude, codex"));
        assert!(rendered.iter().any(|line| line
            == "Current agent runtime: requested codex -> selected claude via executor.id"));
        assert!(rendered
            .iter()
            .all(|line| !line.starts_with("Run runtime:")));
    }

    #[test]
    fn blocker_items_surface_lease_and_promotion_blockers() {
        let mut snapshot = test_snapshot();
        let state = AppState::default();
        snapshot.acceptance_packages = vec![sample_acceptance_package(5)];
        snapshot.planning.waves[0].blocked_by = vec![
            "lease-expired:lease-wave-05-a6".to_string(),
            "closure:promotion-blocked:merge-conflict".to_string(),
        ];
        snapshot.planning.waves[0].blocker_state = vec![
            QueueBlockerReadModel {
                kind: QueueBlockerKindReadModel::LeaseExpired,
                raw: "lease-expired:lease-wave-05-a6".to_string(),
                detail: Some("lease-wave-05-a6".to_string()),
            },
            QueueBlockerReadModel {
                kind: QueueBlockerKindReadModel::Closure,
                raw: "closure:promotion-blocked:merge-conflict".to_string(),
                detail: Some("promotion-blocked:merge-conflict".to_string()),
            },
        ];
        snapshot.latest_run_details[0].execution.merge_blocked = true;
        snapshot.latest_run_details[0]
            .execution
            .closure_blocked_by_promotion = true;
        if let Some(promotion) = snapshot.latest_run_details[0].execution.promotion.as_mut() {
            promotion.detail = Some("promotion blocked by merge conflicts".to_string());
            promotion.conflict_paths = vec!["crates/wave-tui/src/lib.rs".to_string()];
        }

        let rendered = blocker_item_lines(&snapshot, &state, Some(5));

        assert!(rendered.iter().any(|line| line == "lease lease-wave-05-a6"));
        assert!(rendered
            .iter()
            .any(|line| line == "promotion promotion-blocked:merge-conflict"));
        assert!(rendered
            .iter()
            .any(|line| line == "promotion merge-blocked  promotion blocked by merge conflicts"));
        assert!(rendered
            .iter()
            .any(|line| line == "promotion conflict-paths=crates/wave-tui/src/lib.rs"));
        assert!(rendered
            .iter()
            .any(|line| line == "promotion closure-blocked  waiting for promotion to clear"));
        assert!(rendered
            .iter()
            .any(|line| { line == "acceptance  implementation proof is only 2/6 complete" }));
        assert!(rendered
            .iter()
            .any(|line| line == "risk  agent-error  agent A6 failed"));
        assert!(
            rendered
                .iter()
                .any(|line| line
                    == "debt  proof-incomplete  implementation proof is incomplete (2/6)")
        );
    }

    #[test]
    fn blocker_items_keep_operator_object_context() {
        let rendered = blocker_item_lines(&test_snapshot(), &AppState::default(), None);

        assert!(rendered
            .iter()
            .any(|line| line == "question  wave 5  question-api-shape"));
        assert!(rendered
            .iter()
            .any(|line| line == "assumption  wave 5  assumption-cache-valid"));
        assert!(rendered
            .iter()
            .any(|line| line == "invalidated-fact  wave 5  fact-api"));
        assert!(rendered
            .iter()
            .any(|line| line == "invalidated-decision  wave 5  decision-api-shape"));
        assert!(rendered
            .iter()
            .any(|line| line == "superseded-decision  wave 5  decision-api-v1"));
        assert!(rendered
            .iter()
            .any(|line| line == "dependency-ambiguity  wave 5  wave-4"));
        assert!(rendered.iter().any(|line| {
            line == "manual-close-override  wave 15  manual close accepted  state=applied"
        }));
        assert!(
            rendered
                .iter()
                .any(|line| line
                    == "  source_run=wave-15-failed  evidence=1  waiting_on=manual close override is active  next_action=press M to clear")
        );
        assert!(rendered
            .iter()
            .any(|line| line == "  detail=promotion conflict reviewed"));
        assert!(rendered.iter().any(|line| {
            line == "approval-request  wave 5  Need dependency confirmation  state=pending"
        }));
        assert!(
            rendered
                .iter()
                .any(|line| line
                    == "  route=dependency:wave-04  task=wave-05:agent-a1  waiting_on=operator dependency approval  next_action=press u to approve or x to reject")
        );
        assert!(rendered
            .iter()
            .any(|line| line == "escalation  wave 5  Need operator review  state=open"));
        assert!(
            rendered
                .iter()
                .any(|line| line
                    == "  route=dependency:wave-04  task=wave-05:agent-a6  evidence=1  waiting_on=operator escalation review  next_action=press u to acknowledge or x to dismiss")
        );
        assert!(rendered
            .iter()
            .any(|line| line == "  detail=escalated from design review"));
        assert!(rendered.iter().any(|line| {
            line
                == "contradiction  wave 5  contradiction-5  API shape contradicts dependency result  state=detected"
        }));
        assert!(rendered
            .iter()
            .any(|line| { line == "  invalidates=fact:fact-api, decision:decision-api-shape" }));
        assert!(rendered.iter().any(|line| {
            line
                == "invalidation  wave 5  contradiction contradiction-5 invalidates fact fact-api -> decision decision-api-shape"
        }));
    }

    #[test]
    fn control_actions_include_manual_close_shortcuts() {
        let snapshot = test_snapshot();

        assert!(snapshot
            .panels
            .control
            .actions
            .iter()
            .any(|action| action.key == "m" && action.label == "Apply manual close"));
        assert!(snapshot
            .panels
            .control
            .actions
            .iter()
            .any(|action| action.key == "M" && action.label == "Clear manual close"));
        assert!(snapshot
            .panels
            .control
            .actions
            .iter()
            .any(|action| action.key == "[ / ]" && action.label == "Select action"));
        assert!(snapshot
            .panels
            .control
            .actions
            .iter()
            .any(|action| action.key == "u" && action.label == "Approve action"));
        assert!(snapshot
            .panels
            .control
            .actions
            .iter()
            .any(|action| action.key == "x" && action.label == "Reject or dismiss"));
    }

    #[test]
    fn pending_manual_close_confirmation_lines_include_confirmation_keys() {
        let rendered = pending_control_action_lines(&PendingControlAction::ApplyManualClose(
            ManualCloseConfirmation {
                wave_id: 15,
                wave_title: "Runtime Policy".to_string(),
                source_run_id: "wave-15-failed".to_string(),
                evidence_paths: vec!["docs/evidence.md".to_string()],
                reason: "reason".to_string(),
                detail: "detail".to_string(),
                summary: "latest run wave-15-failed failed".to_string(),
            },
        ));

        assert!(rendered
            .iter()
            .any(|line| line == "Source run: wave-15-failed"));
        assert!(rendered.iter().any(|line| line == "Evidence files: 1"));
        assert!(rendered
            .iter()
            .any(|line| line == "Enter apply  Esc cancel"));
    }

    #[test]
    fn pending_operator_action_lines_include_waiting_context() {
        let rendered = pending_control_action_lines(&PendingControlAction::ApproveOperatorAction(
            OperatorActionConfirmation {
                wave_id: 5,
                wave_title: "Build the right-side operator panel in the TUI".to_string(),
                record_id: "human-5".to_string(),
                kind: wave_app_server::OperatorActionableKind::Approval,
                summary: "Need dependency confirmation".to_string(),
                waiting_on: Some("operator dependency approval".to_string()),
                next_action: Some("press u to approve or x to reject".to_string()),
            },
        ));

        assert!(rendered
            .iter()
            .any(|line| line == "Approve approval-request human-5"));
        assert!(rendered
            .iter()
            .any(|line| line == "Waiting on: operator dependency approval"));
        assert!(rendered
            .iter()
            .any(|line| line == "Enter approve  Esc cancel"));
    }

    #[test]
    fn operator_action_selection_moves_between_actionable_items() {
        let snapshot = test_snapshot();
        let mut state = AppState::default();
        clamp_selected_wave(&mut state, &snapshot);
        clamp_selected_operator_action(&mut state, &snapshot);

        let first = selected_actionable_operator_item(&state, &snapshot, 5)
            .expect("first selected actionable item");
        assert_eq!(first.record_id, "human-5");

        select_next_operator_action(&mut state, &snapshot);
        let second = selected_actionable_operator_item(&state, &snapshot, 5)
            .expect("second selected actionable item");
        assert_eq!(second.record_id, "esc-5");

        select_previous_operator_action(&mut state, &snapshot);
        let selected = selected_actionable_operator_item(&state, &snapshot, 5)
            .expect("selected actionable item after moving back");
        assert_eq!(selected.record_id, "human-5");
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

    fn test_app(state: AppState) -> App {
        App {
            root: std::env::temp_dir().join(format!(
                "wave-tui-test-app-{}-{}",
                std::process::id(),
                wave_trace::now_epoch_ms().expect("timestamp")
            )),
            config: ProjectConfig::default(),
            state,
            tab: PanelTab::Control,
        }
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
        use wave_app_server::RuntimeDetail;
        use wave_app_server::RuntimeExecutionIdentityDetail;
        use wave_app_server::RuntimeFallbackDetail;
        use wave_app_server::RuntimePolicyDetail;
        use wave_app_server::RuntimeSkillProjectionDetail;
        use wave_app_server::RuntimeSummary;
        use wave_runtime::RerunIntentRecord;
        use wave_runtime::RerunIntentStatus;

        let active_execution = wave_control_plane::WaveExecutionState {
            worktree: Some(wave_domain::WaveWorktreeRecord {
                worktree_id: wave_domain::WaveWorktreeId::new("worktree-wave-05-test".to_string()),
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
                last_decision: Some("closure capacity reserved before A8".to_string()),
                updated_at_ms: 3,
            }),
            merge_blocked: false,
            closure_blocked_by_promotion: false,
        };

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
            last_activity_at_ms: Some(2),
            activity_source: Some("last-message".to_string()),
            stalled: false,
            stall_reason: None,
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
            execution: active_execution.clone(),
            runtime_summary: RuntimeSummary {
                selected_runtimes: vec!["claude".to_string()],
                requested_runtimes: vec!["codex".to_string()],
                selection_sources: vec!["executor.id".to_string()],
                fallback_targets: vec!["claude".to_string()],
                fallback_count: 1,
                agents_with_runtime: 1,
            },
            agents: vec![
                AgentPanelItem {
                    id: "A1".to_string(),
                    title: "TUI Shell And Layout Scaffold".to_string(),
                    status: WaveRunStatus::Running,
                    current_task: "TUI Shell And Layout Scaffold".to_string(),
                    reused_from_prior_run: false,
                    proof_complete: false,
                    proof_source: "compatibility-adapter".to_string(),
                    expected_markers: vec!["[wave-proof]".to_string()],
                    observed_markers: Vec::new(),
                    missing_markers: vec!["[wave-proof]".to_string()],
                    deliverables: vec!["crates/wave-tui/src/lib.rs".to_string()],
                    error: None,
                    runtime: Some(RuntimeDetail {
                        selected_runtime: "claude".to_string(),
                        selection_reason: "selected claude after fallback".to_string(),
                        policy: RuntimePolicyDetail {
                            requested_runtime: Some("codex".to_string()),
                            allowed_runtimes: vec!["codex".to_string(), "claude".to_string()],
                            fallback_runtimes: vec!["claude".to_string()],
                            selection_source: Some("executor.id".to_string()),
                            uses_fallback: true,
                        },
                        fallback: Some(RuntimeFallbackDetail {
                            requested_runtime: "codex".to_string(),
                            selected_runtime: "claude".to_string(),
                            reason: "codex login status reported unavailable".to_string(),
                        }),
                        execution_identity: RuntimeExecutionIdentityDetail {
                            adapter: "wave-runtime/claude".to_string(),
                            binary: "claude".to_string(),
                            provider: "anthropic-claude-code".to_string(),
                            artifact_paths: std::collections::BTreeMap::new(),
                        },
                        skill_projection: RuntimeSkillProjectionDetail {
                            declared_skills: vec!["wave-core".to_string()],
                            projected_skills: vec![
                                "wave-core".to_string(),
                                "runtime-claude".to_string(),
                            ],
                            dropped_skills: Vec::new(),
                            auto_attached_skills: vec!["runtime-claude".to_string()],
                        },
                    }),
                },
                AgentPanelItem {
                    id: "A8".to_string(),
                    title: "Integration Steward".to_string(),
                    status: WaveRunStatus::Planned,
                    current_task: "Integration Steward".to_string(),
                    reused_from_prior_run: false,
                    proof_complete: false,
                    proof_source: "compatibility-adapter".to_string(),
                    expected_markers: vec!["[wave-integration]".to_string()],
                    observed_markers: Vec::new(),
                    missing_markers: vec!["[wave-integration]".to_string()],
                    deliverables: Vec::new(),
                    error: None,
                    runtime: None,
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
            design_details: vec![wave_app_server::WaveDesignDetail {
                wave_id: 5,
                completeness: wave_domain::DesignCompletenessState::Underspecified,
                blocker_reasons: vec![
                    "design:human-input:human-5".to_string(),
                    "design:downstream-task-invalidated:wave-05:agent-a1".to_string(),
                ],
                active_contradictions: vec![wave_app_server::ContradictionDetail {
                    contradiction_id: "contradiction-5".to_string(),
                    state: "detected".to_string(),
                    summary: "API shape contradicts dependency result".to_string(),
                    detail: Some("wave 5 still depends on invalidated decision-api-shape".to_string()),
                    invalidated_refs: vec![
                        "fact:fact-api".to_string(),
                        "decision:decision-api-shape".to_string(),
                    ],
                }],
                unresolved_question_ids: vec!["question-api-shape".to_string()],
                unresolved_assumption_ids: vec!["assumption-cache-valid".to_string()],
                pending_human_inputs: vec![wave_app_server::PendingHumanInputDetail {
                    request_id: "human-5".to_string(),
                    task_id: Some("wave-05:agent-a1".to_string()),
                    state: wave_domain::HumanInputState::Pending,
                    workflow_kind: wave_domain::HumanInputWorkflowKind::DependencyHandshake,
                    route: "dependency:wave-04".to_string(),
                    prompt: "Need dependency confirmation".to_string(),
                    requested_by: "A2".to_string(),
                    answer: None,
                }],
                dependency_handshake_routes: vec!["dependency:wave-04".to_string()],
                invalidated_fact_ids: vec!["fact-api".to_string()],
                invalidated_decision_ids: vec!["decision-api-shape".to_string()],
                invalidation_routes: vec![
                    "contradiction contradiction-5 invalidates fact fact-api -> decision decision-api-shape".to_string(),
                    "decision decision-api-shape invalidates task wave-05:agent-a1".to_string(),
                ],
                selectively_invalidated_task_ids: vec!["wave-05:agent-a1".to_string()],
                superseded_decision_ids: vec!["decision-api-v1".to_string()],
                ambiguous_dependency_wave_ids: vec![4],
            }],
            operator_objects: vec![
                wave_app_server::OperatorActionableItem {
                    kind: wave_app_server::OperatorActionableKind::Approval,
                    wave_id: 5,
                    record_id: "human-5".to_string(),
                    state: "pending".to_string(),
                    summary: "Need dependency confirmation".to_string(),
                    detail: Some("requested by A2 via dependency:wave-04".to_string()),
                    waiting_on: Some("operator dependency approval".to_string()),
                    next_action: Some("press u to approve or x to reject".to_string()),
                    route: Some("dependency:wave-04".to_string()),
                    task_id: Some("wave-05:agent-a1".to_string()),
                    source_run_id: None,
                    evidence_count: 0,
                    created_at_ms: Some(1),
                },
                wave_app_server::OperatorActionableItem {
                    kind: wave_app_server::OperatorActionableKind::Override,
                    wave_id: 15,
                    record_id: "closure-override-wave-15".to_string(),
                    state: "applied".to_string(),
                    summary: "manual close accepted".to_string(),
                    detail: Some("promotion conflict reviewed".to_string()),
                    waiting_on: Some("manual close override is active".to_string()),
                    next_action: Some("press M to clear".to_string()),
                    route: None,
                    task_id: None,
                    source_run_id: Some("wave-15-failed".to_string()),
                    evidence_count: 1,
                    created_at_ms: Some(2),
                },
                wave_app_server::OperatorActionableItem {
                    kind: wave_app_server::OperatorActionableKind::Escalation,
                    wave_id: 5,
                    record_id: "esc-5".to_string(),
                    state: "open".to_string(),
                    summary: "Need operator review".to_string(),
                    detail: Some("escalated from design review".to_string()),
                    waiting_on: Some("operator escalation review".to_string()),
                    next_action: Some("press u to acknowledge or x to dismiss".to_string()),
                    route: Some("dependency:wave-04".to_string()),
                    task_id: Some("wave-05:agent-a6".to_string()),
                    source_run_id: None,
                    evidence_count: 1,
                    created_at_ms: Some(3),
                },
            ],
            acceptance_packages: Vec::new(),
            planning: PlanningStatusReadModel {
                project_name: "Codex Wave Mode".to_string(),
                default_mode: wave_config::ExecutionMode::DarkFactory,
                summary: PlanningStatusSummary {
                    total_waves: 10,
                    ready_waves: 1,
                    blocked_waves: 4,
                    active_waves: 1,
                    completed_waves: 5,
                    design_incomplete_waves: 1,
                    total_agents: 60,
                    implementation_agents: 30,
                    closure_agents: 30,
                    waves_with_complete_closure: 10,
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
                    design_completeness: wave_domain::DesignCompletenessState::Underspecified,
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
                    execution: active_execution,
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
                    closure_override_applied: false,
                    completed: false,
                    last_run_status: Some(WaveRunStatus::Running),
                    soft_state: wave_domain::SoftState::Clear,
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
                        design: 1,
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
                delivery_attention_lines: Vec::new(),
                skill_issue_paths: Vec::new(),
                skill_issue_lines: Vec::new(),
                signal: wave_control_plane::DeliverySignalReadModel::default(),
            },
            delivery: default_delivery(),
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
                        design: 1,
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
                    apply_closure_override_supported: true,
                    clear_closure_override_supported: true,
                    approve_operator_action_supported: true,
                    reject_operator_action_supported: true,
                    launch_supported: true,
                    autonomous_supported: true,
                    launcher_required: true,
                    launcher_ready: false,
                    actions: vec![
                        ControlAction {
                            key: "[ / ]".to_string(),
                            label: "Select action".to_string(),
                            description: "Select action".to_string(),
                            implemented: true,
                        },
                        ControlAction {
                            key: "r".to_string(),
                            label: "Request rerun".to_string(),
                            description: "Request rerun".to_string(),
                            implemented: true,
                        },
                        ControlAction {
                            key: "m".to_string(),
                            label: "Apply manual close".to_string(),
                            description: "Apply manual close".to_string(),
                            implemented: true,
                        },
                        ControlAction {
                            key: "M".to_string(),
                            label: "Clear manual close".to_string(),
                            description: "Clear manual close".to_string(),
                            implemented: true,
                        },
                        ControlAction {
                            key: "u".to_string(),
                            label: "Approve action".to_string(),
                            description: "Approve action".to_string(),
                            implemented: true,
                        },
                        ControlAction {
                            key: "x".to_string(),
                            label: "Reject or dismiss".to_string(),
                            description: "Reject or dismiss".to_string(),
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
                    implemented_actions: vec![
                        ControlAction {
                            key: "[ / ]".to_string(),
                            label: "Select action".to_string(),
                            description: "Select action".to_string(),
                            implemented: true,
                        },
                        ControlAction {
                            key: "r".to_string(),
                            label: "Request rerun".to_string(),
                            description: "Request rerun".to_string(),
                            implemented: true,
                        },
                        ControlAction {
                            key: "m".to_string(),
                            label: "Apply manual close".to_string(),
                            description: "Apply manual close".to_string(),
                            implemented: true,
                        },
                        ControlAction {
                            key: "M".to_string(),
                            label: "Clear manual close".to_string(),
                            description: "Clear manual close".to_string(),
                            implemented: true,
                        },
                        ControlAction {
                            key: "u".to_string(),
                            label: "Approve action".to_string(),
                            description: "Approve action".to_string(),
                            implemented: true,
                        },
                        ControlAction {
                            key: "x".to_string(),
                            label: "Reject or dismiss".to_string(),
                            description: "Reject or dismiss".to_string(),
                            implemented: true,
                        },
                    ],
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
                executor_boundary: "runtime-neutral adapter registry in wave-runtime".to_string(),
                selection_policy:
                    "explicit executor runtime selection with default codex and authored fallback order"
                        .to_string(),
                fallback_policy:
                    "fallback only when the selected runtime is unavailable before meaningful work starts"
                        .to_string(),
                available_runtimes: Vec::new(),
                unavailable_runtimes: vec!["codex".to_string(), "claude".to_string()],
                runtimes: vec![
                    wave_runtime::RuntimeAvailability {
                        runtime: wave_domain::RuntimeId::Codex,
                        binary: "codex".to_string(),
                        available: false,
                        detail: "codex login status reported unavailable".to_string(),
                    },
                    wave_runtime::RuntimeAvailability {
                        runtime: wave_domain::RuntimeId::Claude,
                        binary: "claude".to_string(),
                        available: false,
                        detail: "claude auth status --json reported unavailable".to_string(),
                    },
                ],
                ready: false,
            },
            active_run_details: vec![active_run],
            rerun_intents: vec![RerunIntentRecord {
                request_id: Some("rerun-wave-05-1".to_string()),
                wave_id: 5,
                reason: "Requested from the Wave operator TUI".to_string(),
                requested_by: "operator".to_string(),
                scope: RerunScope::Full,
                status: RerunIntentStatus::Requested,
                requested_at_ms: 1,
                cleared_at_ms: None,
            }],
            closure_overrides: Vec::new(),
            control_actions: vec![
                ControlAction {
                    key: "[ / ]".to_string(),
                    label: "Select action".to_string(),
                    description: "Select action".to_string(),
                    implemented: true,
                },
                ControlAction {
                    key: "r".to_string(),
                    label: "Request rerun".to_string(),
                    description: "Request rerun".to_string(),
                    implemented: true,
                },
                ControlAction {
                    key: "m".to_string(),
                    label: "Apply manual close".to_string(),
                    description: "Apply manual close".to_string(),
                    implemented: true,
                },
                ControlAction {
                    key: "M".to_string(),
                    label: "Clear manual close".to_string(),
                    description: "Clear manual close".to_string(),
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
