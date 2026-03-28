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
use crossterm::event::KeyModifiers;
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
use ratatui::widgets::Clear;
use ratatui::widgets::List;
use ratatui::widgets::ListItem;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Row;
use ratatui::widgets::Table;
use ratatui::widgets::TableState;
use ratatui::widgets::Tabs;
use ratatui::widgets::Wrap;
use std::fmt;
use std::io;
use std::io::IsTerminal;
use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;
use std::time::Instant;
use wave_app_server::ActiveRunDetail;
use wave_app_server::OperatorSnapshot;
use wave_app_server::load_operator_snapshot;
use wave_config::ProjectConfig;
use wave_domain::DirectiveOrigin;
use wave_domain::OrchestratorMode;
use wave_domain::RerunScope;
use wave_runtime::LaunchOptions;
use wave_runtime::acknowledge_escalation;
use wave_runtime::apply_closure_override;
use wave_runtime::apply_head_proposal;
use wave_runtime::approve_agent_merge;
use wave_runtime::approve_human_input_request;
use wave_runtime::clear_closure_override;
use wave_runtime::clear_rerun;
use wave_runtime::dismiss_escalation;
use wave_runtime::dismiss_head_proposal;
use wave_runtime::latest_operator_shell_session;
use wave_runtime::launch_wave;
use wave_runtime::pause_agent;
use wave_runtime::preview_closure_override;
use wave_runtime::rebase_agent_sandbox;
use wave_runtime::record_operator_shell_guidance_turn;
use wave_runtime::reject_agent_merge;
use wave_runtime::reject_human_input_request;
use wave_runtime::request_agent_reconciliation;
use wave_runtime::request_rerun;
use wave_runtime::rerun_agent;
use wave_runtime::resume_agent;
use wave_runtime::set_orchestrator_mode;
use wave_runtime::start_operator_shell_session;
use wave_runtime::steer_agent;
use wave_runtime::steer_wave;
use wave_runtime::submit_operator_shell_head_turn;
use wave_runtime::upsert_operator_shell_session;
use wave_spec::load_wave_documents;
use wave_trace::WaveRunStatus;

/// Stable label for the terminal-shell landing zone.
pub const TUI_LANDING_ZONE: &str = "interactive-operator-shell-bootstrap";
const NARROW_LAYOUT_THRESHOLD: u16 = 100;
const WIDE_MAIN_PERCENT: u16 = 58;
const WIDE_PANEL_PERCENT: u16 = 42;
const SNAPSHOT_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AltScreenMode {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunOptions {
    pub alt_screen: AltScreenMode,
    pub fresh_session: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellLayoutMode {
    Wide,
    Narrow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PanelTab {
    Overview,
    Agents,
    Queue,
    Proof,
    Control,
}

impl PanelTab {
    fn all() -> [Self; 5] {
        [
            Self::Overview,
            Self::Agents,
            Self::Queue,
            Self::Proof,
            Self::Control,
        ]
    }

    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Agents => "Agents",
            Self::Queue => "Queue",
            Self::Proof => "Proof",
            Self::Control => "Control",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Overview => Self::Agents,
            Self::Agents => Self::Queue,
            Self::Queue => Self::Proof,
            Self::Proof => Self::Control,
            Self::Control => Self::Overview,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Overview => Self::Control,
            Self::Agents => Self::Overview,
            Self::Queue => Self::Agents,
            Self::Proof => Self::Queue,
            Self::Control => Self::Proof,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusLane {
    Transcript,
    Composer,
    Dashboard,
}

impl FocusLane {
    fn next(self) -> Self {
        match self {
            Self::Transcript => Self::Composer,
            Self::Composer => Self::Dashboard,
            Self::Dashboard => Self::Transcript,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Transcript => Self::Dashboard,
            Self::Composer => Self::Transcript,
            Self::Dashboard => Self::Composer,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FollowMode {
    Run,
    Agent,
    Off,
}

impl FollowMode {
    fn label(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Agent => "agent",
            Self::Off => "off",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellScope {
    Head,
    Wave,
    Agent,
}

impl ShellScope {
    fn label(self) -> &'static str {
        match self {
            Self::Head => "head",
            Self::Wave => "wave",
            Self::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShellTargetState {
    scope: ShellScope,
    wave_id: Option<u32>,
    agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CompareMode {
    Wave { wave_id: u32 },
    Agent { agent_id: String },
}

#[derive(Debug)]
struct AppState {
    selected_wave_index: usize,
    selected_operator_action_index: usize,
    selected_orchestrator_agent_index: usize,
    flash_message: Option<FlashMessage>,
    pending_control_action: Option<PendingControlAction>,
    composer_input: String,
    transcript_scroll: u16,
    focus: FocusLane,
    follow_mode: FollowMode,
    shell_target: Option<ShellTargetState>,
    help_visible: bool,
    transcript_search: Option<String>,
    compare_mode: Option<CompareMode>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            selected_wave_index: 0,
            selected_operator_action_index: 0,
            selected_orchestrator_agent_index: 0,
            flash_message: None,
            pending_control_action: None,
            composer_input: String::new(),
            transcript_scroll: 0,
            focus: FocusLane::Dashboard,
            follow_mode: FollowMode::Run,
            shell_target: None,
            help_visible: false,
            transcript_search: None,
            compare_mode: None,
        }
    }
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
    run_with_options(root, config, RunOptions::default())
}

pub fn run_with_options(root: &Path, config: &ProjectConfig, options: RunOptions) -> Result<()> {
    if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
        bail!("the Wave TUI requires an interactive terminal");
    }

    initialize_shell_session(root, config, options)?;

    let mut stdout = io::stdout();
    terminal::enable_raw_mode().context("failed to enable raw mode")?;
    let use_alt_screen = should_use_alt_screen(options.alt_screen);
    if use_alt_screen {
        execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to create terminal backend")?;
    let mut app = App {
        root: root.to_path_buf(),
        config: config.clone(),
        state: AppState::default(),
        tab: PanelTab::Overview,
        snapshot: None,
        snapshot_error: None,
        snapshot_receiver: None,
        refresh_in_flight: false,
        last_refresh_started_at: None,
    };

    let result = run_loop(&mut terminal, &mut app);

    terminal::disable_raw_mode().ok();
    if use_alt_screen {
        execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    }
    terminal.show_cursor().ok();
    result
}

struct App {
    root: std::path::PathBuf,
    config: ProjectConfig,
    state: AppState,
    tab: PanelTab,
    snapshot: Option<OperatorSnapshot>,
    snapshot_error: Option<String>,
    snapshot_receiver: Option<Receiver<std::result::Result<OperatorSnapshot, String>>>,
    refresh_in_flight: bool,
    last_refresh_started_at: Option<Instant>,
}

fn should_use_alt_screen(mode: AltScreenMode) -> bool {
    match mode {
        AltScreenMode::Always => true,
        AltScreenMode::Never => false,
        AltScreenMode::Auto => std::env::var_os("ZELLIJ").is_none(),
    }
}

fn initialize_shell_session(
    root: &Path,
    config: &ProjectConfig,
    options: RunOptions,
) -> Result<()> {
    if options.fresh_session || latest_operator_shell_session(root, config)?.is_none() {
        start_operator_shell_session(
            root,
            config,
            wave_domain::OperatorShellScope::Head,
            None,
            None,
            "overview",
            FollowMode::Run.label(),
            OrchestratorMode::Operator,
            "wave-tui",
        )?;
    }
    Ok(())
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    request_snapshot_refresh(app);
    loop {
        poll_snapshot_refresh(app);
        maybe_schedule_snapshot_refresh(app);

        terminal.draw(|frame| draw_ui(frame, app))?;

        if !event::poll(Duration::from_millis(250)).context("failed to poll terminal events")? {
            continue;
        }

        let Event::Key(key) = event::read().context("failed to read terminal event")? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            if app.state.pending_control_action.is_some() {
                app.state.pending_control_action = None;
                set_info_message(&mut app.state, "cancelled control action");
                continue;
            }
            return Ok(());
        }

        if app.state.help_visible {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => {
                    app.state.help_visible = false;
                }
                KeyCode::Tab => app.state.focus = app.state.focus.next(),
                KeyCode::BackTab => app.state.focus = app.state.focus.previous(),
                _ => {}
            }
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

        if matches!(app.state.focus, FocusLane::Composer) {
            handle_composer_key(app, key)?;
            continue;
        }

        match key.code {
            KeyCode::Char('q') => return Ok(()),
            KeyCode::Tab => app.state.focus = app.state.focus.next(),
            KeyCode::BackTab => app.state.focus = app.state.focus.previous(),
            KeyCode::Char('?') => app.state.help_visible = true,
            KeyCode::Char('[') => {
                app.tab = app.tab.previous();
                let _ = persist_shell_session(app);
            }
            KeyCode::Char(']') => {
                app.tab = app.tab.next();
                let _ = persist_shell_session(app);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                handle_focus_next(app);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                handle_focus_previous(app);
            }
            KeyCode::Char('r') => handle_request_rerun(app)?,
            KeyCode::Char('c') => handle_clear_rerun(app)?,
            KeyCode::Char('m') => handle_prepare_manual_close(app)?,
            KeyCode::Char('M') => handle_prepare_clear_manual_close(app)?,
            KeyCode::Char('u') => handle_prepare_operator_action(app, true)?,
            KeyCode::Char('x') => handle_prepare_operator_action(app, false)?,
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                app.state.focus = FocusLane::Composer;
                app.state.composer_input.push(ch);
            }
            _ => {}
        }
    }
}

fn request_snapshot_refresh(app: &mut App) {
    if app.refresh_in_flight {
        return;
    }

    let (sender, receiver) = mpsc::channel();
    let root = app.root.clone();
    let config = app.config.clone();
    app.refresh_in_flight = true;
    app.last_refresh_started_at = Some(Instant::now());
    app.snapshot_receiver = Some(receiver);
    std::thread::spawn(move || {
        let result = load_operator_snapshot(&root, &config).map_err(|error| error.to_string());
        let _ = sender.send(result);
    });
}

fn maybe_schedule_snapshot_refresh(app: &mut App) {
    if app.refresh_in_flight {
        return;
    }
    let should_refresh = app
        .last_refresh_started_at
        .map(|started| started.elapsed() >= SNAPSHOT_REFRESH_INTERVAL)
        .unwrap_or(true)
        || app.snapshot.is_none();
    if should_refresh {
        request_snapshot_refresh(app);
    }
}

fn poll_snapshot_refresh(app: &mut App) {
    let Some(receiver) = app.snapshot_receiver.as_ref() else {
        return;
    };
    let result = receiver.try_recv();
    match result {
        Ok(Ok(snapshot)) => {
            clamp_selected_wave(&mut app.state, &snapshot);
            clamp_selected_operator_action(&mut app.state, &snapshot);
            clamp_selected_orchestrator_agent(&mut app.state, &snapshot);
            sync_shell_state_with_snapshot(app, &snapshot);
            app.snapshot = Some(snapshot);
            app.snapshot_error = None;
            app.refresh_in_flight = false;
            app.snapshot_receiver = None;
        }
        Ok(Err(error)) => {
            app.snapshot_error = Some(error);
            app.refresh_in_flight = false;
            app.snapshot_receiver = None;
        }
        Err(TryRecvError::Empty) => {}
        Err(TryRecvError::Disconnected) => {
            app.snapshot_error = Some("snapshot refresh channel disconnected".to_string());
            app.refresh_in_flight = false;
            app.snapshot_receiver = None;
        }
    }
}

fn sync_shell_state_with_snapshot(app: &mut App, snapshot: &OperatorSnapshot) {
    if let Some(session) = snapshot.shell.session.as_ref() {
        app.tab = panel_tab_from_session(session.tab.as_str()).unwrap_or(app.tab);
        app.state.follow_mode =
            follow_mode_from_session(session.follow_mode.as_str()).unwrap_or(app.state.follow_mode);
    }
    sync_shell_target_with_snapshot(&mut app.state, snapshot);
    apply_follow_mode(&mut app.state, snapshot);
}

fn sync_shell_target_with_snapshot(state: &mut AppState, snapshot: &OperatorSnapshot) {
    let fallback = shell_target_from_snapshot(snapshot);
    let mut target = state
        .shell_target
        .clone()
        .unwrap_or_else(|| fallback.clone());

    if let Some(wave_id) = target.wave_id {
        if selected_wave_by_id(snapshot, wave_id).is_none() {
            target = fallback.clone();
        }
    }

    if matches!(target.scope, ShellScope::Agent) {
        let Some(wave_id) = target.wave_id else {
            target = fallback.clone();
            state.shell_target = Some(target);
            return;
        };
        let Some(agent_id) = target.agent_id.as_deref() else {
            target = fallback.clone();
            state.shell_target = Some(target);
            return;
        };
        let valid = snapshot
            .panels
            .orchestrator
            .waves
            .iter()
            .find(|wave| wave.wave_id == wave_id)
            .is_some_and(|wave| wave.agents.iter().any(|agent| agent.id == agent_id));
        if !valid {
            target.scope = ShellScope::Wave;
            target.agent_id = None;
        }
    }

    if let Some(wave_id) = target.wave_id {
        select_wave_by_id(state, snapshot, wave_id);
    }

    if matches!(target.scope, ShellScope::Agent) {
        if let (Some(wave_id), Some(agent_id)) = (target.wave_id, target.agent_id.as_deref()) {
            if let Some(index) = orchestrator_agent_index(snapshot, wave_id, agent_id) {
                state.selected_orchestrator_agent_index = index;
            }
        }
    }

    state.shell_target = Some(target);
}

fn shell_target_from_snapshot(snapshot: &OperatorSnapshot) -> ShellTargetState {
    let scope = match snapshot
        .shell
        .session
        .as_ref()
        .map(|session| session.scope.as_str())
        .unwrap_or(snapshot.shell.default_target.scope.as_str())
    {
        "wave" => ShellScope::Wave,
        "agent" => ShellScope::Agent,
        _ => ShellScope::Head,
    };
    ShellTargetState {
        scope,
        wave_id: snapshot
            .shell
            .session
            .as_ref()
            .and_then(|session| session.wave_id)
            .or(snapshot.shell.default_target.wave_id),
        agent_id: snapshot
            .shell
            .session
            .as_ref()
            .and_then(|session| session.agent_id.clone())
            .or_else(|| snapshot.shell.default_target.agent_id.clone()),
    }
}

fn panel_tab_from_session(value: &str) -> Option<PanelTab> {
    match value {
        "overview" | "Overview" => Some(PanelTab::Overview),
        "agents" | "Agents" => Some(PanelTab::Agents),
        "queue" | "Queue" => Some(PanelTab::Queue),
        "proof" | "Proof" => Some(PanelTab::Proof),
        "control" | "Control" => Some(PanelTab::Control),
        _ => None,
    }
}

fn follow_mode_from_session(value: &str) -> Option<FollowMode> {
    match value {
        "run" => Some(FollowMode::Run),
        "agent" => Some(FollowMode::Agent),
        "off" => Some(FollowMode::Off),
        _ => None,
    }
}

fn selected_wave_by_id(
    snapshot: &OperatorSnapshot,
    wave_id: u32,
) -> Option<&wave_control_plane::WaveStatusReadModel> {
    snapshot
        .planning
        .waves
        .iter()
        .find(|wave| wave.id == wave_id)
}

fn select_wave_by_id(state: &mut AppState, snapshot: &OperatorSnapshot, wave_id: u32) -> bool {
    let Some(index) = snapshot
        .planning
        .waves
        .iter()
        .position(|wave| wave.id == wave_id)
    else {
        return false;
    };
    state.selected_wave_index = index;
    state.selected_orchestrator_agent_index = 0;
    state.selected_operator_action_index = 0;
    sync_shell_target_to_selected_wave(state, snapshot);
    true
}

fn sync_shell_target_to_selected_wave(state: &mut AppState, snapshot: &OperatorSnapshot) {
    let wave_id = selected_wave_id(state, snapshot);
    let agent_id = selected_orchestrator_agent(snapshot, state).map(|agent| agent.id.clone());
    let Some(target) = state.shell_target.as_mut() else {
        return;
    };
    match target.scope {
        ShellScope::Head => {}
        ShellScope::Wave => {
            target.wave_id = wave_id;
            target.agent_id = None;
        }
        ShellScope::Agent => {
            target.wave_id = wave_id;
            if let Some(agent_id) = agent_id {
                target.agent_id = Some(agent_id);
            } else {
                target.scope = ShellScope::Wave;
                target.agent_id = None;
            }
        }
    }
}

fn sync_shell_target_to_selected_agent(state: &mut AppState, snapshot: &OperatorSnapshot) {
    let wave_id = selected_wave_id(state, snapshot);
    let agent_id = selected_orchestrator_agent(snapshot, state).map(|agent| agent.id.clone());
    let Some(target) = state.shell_target.as_mut() else {
        return;
    };
    if !matches!(target.scope, ShellScope::Agent) {
        return;
    }
    target.wave_id = wave_id;
    if let Some(agent_id) = agent_id {
        target.agent_id = Some(agent_id);
    } else {
        target.scope = ShellScope::Wave;
        target.agent_id = None;
    }
}

fn orchestrator_agent_index(
    snapshot: &OperatorSnapshot,
    wave_id: u32,
    agent_id: &str,
) -> Option<usize> {
    snapshot
        .panels
        .orchestrator
        .waves
        .iter()
        .find(|wave| wave.wave_id == wave_id)
        .and_then(|wave| wave.agents.iter().position(|agent| agent.id == agent_id))
}

fn handle_focus_next(app: &mut App) {
    let Some(snapshot) = app.snapshot.as_ref() else {
        return;
    };
    match app.state.focus {
        FocusLane::Transcript => {
            app.state.transcript_scroll = app.state.transcript_scroll.saturating_add(1);
        }
        FocusLane::Composer => {}
        FocusLane::Dashboard => match app.tab {
            PanelTab::Agents => select_next_orchestrator_agent(&mut app.state, snapshot),
            PanelTab::Control => select_next_operator_action(&mut app.state, snapshot),
            _ => select_next_wave(&mut app.state, snapshot),
        },
    }
}

fn handle_focus_previous(app: &mut App) {
    let Some(snapshot) = app.snapshot.as_ref() else {
        return;
    };
    match app.state.focus {
        FocusLane::Transcript => {
            app.state.transcript_scroll = app.state.transcript_scroll.saturating_sub(1);
        }
        FocusLane::Composer => {}
        FocusLane::Dashboard => match app.tab {
            PanelTab::Agents => select_previous_orchestrator_agent(&mut app.state, snapshot),
            PanelTab::Control => select_previous_operator_action(&mut app.state, snapshot),
            _ => select_previous_wave(&mut app.state, snapshot),
        },
    }
}

fn handle_composer_key(app: &mut App, key: crossterm::event::KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Tab => app.state.focus = app.state.focus.next(),
        KeyCode::BackTab => app.state.focus = app.state.focus.previous(),
        KeyCode::Esc => app.state.focus = FocusLane::Dashboard,
        KeyCode::Enter => handle_submit_composer(app)?,
        KeyCode::Backspace => {
            app.state.composer_input.pop();
        }
        KeyCode::Char(ch)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            app.state.composer_input.push(ch);
        }
        _ => {}
    }
    Ok(())
}

fn current_snapshot(app: &App) -> Result<&OperatorSnapshot> {
    app.snapshot
        .as_ref()
        .context("operator snapshot is still loading")
}

fn selected_action_wave_id(state: &AppState, snapshot: &OperatorSnapshot) -> Option<u32> {
    selected_wave_id(state, snapshot)
}

fn current_shell_target(state: &AppState, snapshot: &OperatorSnapshot) -> ShellTargetState {
    state
        .shell_target
        .clone()
        .unwrap_or_else(|| shell_target_from_snapshot(snapshot))
}

fn apply_follow_mode(state: &mut AppState, snapshot: &OperatorSnapshot) {
    match state.follow_mode {
        FollowMode::Off => {}
        FollowMode::Run => {
            let followed_run = selected_action_wave_id(state, snapshot)
                .and_then(|wave_id| {
                    snapshot
                        .active_run_details
                        .iter()
                        .find(|run| run.wave_id == wave_id)
                })
                .or_else(|| {
                    state.shell_target.as_ref().and_then(|target| {
                        target.wave_id.and_then(|wave_id| {
                            snapshot
                                .active_run_details
                                .iter()
                                .find(|run| run.wave_id == wave_id)
                        })
                    })
                })
                .or_else(|| snapshot.active_run_details.first());
            let Some(run) = followed_run else {
                return;
            };
            let _ = select_wave_by_id(state, snapshot, run.wave_id);
            if let Some(agent_id) = run.current_agent_id.as_deref() {
                if let Some(index) = orchestrator_agent_index(snapshot, run.wave_id, agent_id) {
                    state.selected_orchestrator_agent_index = index;
                    sync_shell_target_to_selected_agent(state, snapshot);
                }
            }
            if !matches!(state.focus, FocusLane::Transcript) {
                state.transcript_scroll = 0;
            }
        }
        FollowMode::Agent => {
            let followed_agent = state
                .shell_target
                .as_ref()
                .filter(|target| matches!(target.scope, ShellScope::Agent))
                .and_then(|target| {
                    Some((target.wave_id?, target.agent_id.as_ref()?.clone())).filter(
                        |(wave_id, agent_id)| {
                            orchestrator_agent_index(snapshot, *wave_id, agent_id).is_some()
                        },
                    )
                })
                .or_else(|| {
                    let wave_id = selected_action_wave_id(state, snapshot)?;
                    let agent_id = selected_orchestrator_agent(snapshot, state)?.id.clone();
                    Some((wave_id, agent_id))
                });
            let Some((wave_id, agent_id)) = followed_agent else {
                return;
            };
            let _ = select_wave_by_id(state, snapshot, wave_id);
            if let Some(index) = orchestrator_agent_index(snapshot, wave_id, &agent_id) {
                state.selected_orchestrator_agent_index = index;
                state.shell_target = Some(ShellTargetState {
                    scope: ShellScope::Agent,
                    wave_id: Some(wave_id),
                    agent_id: Some(agent_id),
                });
            }
            if !matches!(state.focus, FocusLane::Transcript) {
                state.transcript_scroll = 0;
            }
        }
    }
}

fn shell_scope_for_runtime(scope: ShellScope) -> wave_domain::OperatorShellScope {
    match scope {
        ShellScope::Head => wave_domain::OperatorShellScope::Head,
        ShellScope::Wave => wave_domain::OperatorShellScope::Wave,
        ShellScope::Agent => wave_domain::OperatorShellScope::Agent,
    }
}

fn current_shell_mode(snapshot: &OperatorSnapshot) -> OrchestratorMode {
    snapshot
        .shell
        .session
        .as_ref()
        .map(|session| session.mode.as_str())
        .or(match snapshot.panels.orchestrator.mode.as_str() {
            "autonomous" => Some("autonomous"),
            "operator" => Some("operator"),
            _ => None,
        })
        .map(|mode| match mode {
            "autonomous" => OrchestratorMode::Autonomous,
            _ => OrchestratorMode::Operator,
        })
        .unwrap_or(OrchestratorMode::Operator)
}

fn persist_shell_session(app: &mut App) -> Result<()> {
    let snapshot = current_snapshot(app)?;
    let target = current_shell_target(&app.state, snapshot);
    upsert_operator_shell_session(
        &app.root,
        &app.config,
        shell_scope_for_runtime(target.scope),
        target.wave_id,
        target.agent_id.as_deref(),
        &app.tab.title().to_ascii_lowercase(),
        app.state.follow_mode.label(),
        current_shell_mode(snapshot),
        "wave-tui",
    )?;
    Ok(())
}

fn refresh_snapshot_after_action(app: &mut App) {
    request_snapshot_refresh(app);
}

fn active_wave_ids(snapshot: &OperatorSnapshot) -> Vec<u32> {
    let mut wave_ids = snapshot
        .active_run_details
        .iter()
        .map(|run| run.wave_id)
        .collect::<Vec<_>>();
    wave_ids.sort_unstable();
    wave_ids.dedup();
    wave_ids
}

fn handle_submit_composer(app: &mut App) -> Result<()> {
    let input = app.state.composer_input.trim().to_string();
    app.state.composer_input.clear();
    if input.is_empty() {
        return Ok(());
    }

    let result = if input.starts_with('/') {
        handle_shell_command(app, &input)
    } else {
        handle_shell_guidance(app, &input)
    };

    if result.is_ok() {
        refresh_snapshot_after_action(app);
    }
    result
}

fn handle_shell_guidance(app: &mut App, message: &str) -> Result<()> {
    let (target, target_label) = {
        let snapshot = current_snapshot(app)?;
        let target = current_shell_target(&app.state, snapshot);
        let label = shell_target_label(&target, snapshot);
        (target, label)
    };

    match target.scope {
        ShellScope::Head => {
            let snapshot = current_snapshot(app)?;
            let autonomous = matches!(current_shell_mode(snapshot), OrchestratorMode::Autonomous);
            let outcome = submit_operator_shell_head_turn(
                &app.root,
                &app.config,
                shell_scope_for_runtime(target.scope),
                target.wave_id,
                target.agent_id.as_deref(),
                message,
                &app.tab.title().to_ascii_lowercase(),
                app.state.follow_mode.label(),
                current_shell_mode(snapshot),
                "wave-tui",
            )?;
            set_info_message(
                &mut app.state,
                if autonomous {
                    format!(
                        "head applied {} autonomous action{} for {}",
                        outcome.applied_proposals,
                        if outcome.applied_proposals == 1 {
                            ""
                        } else {
                            "s"
                        },
                        target_label
                    )
                } else {
                    format!(
                        "head prepared {} proposal{} for {}",
                        outcome.proposals.len(),
                        if outcome.proposals.len() == 1 {
                            ""
                        } else {
                            "s"
                        },
                        target_label
                    )
                },
            );
        }
        ShellScope::Agent => {
            let Some(wave_id) = target.wave_id else {
                set_error_message(&mut app.state, "shell target has no wave context");
                return Ok(());
            };
            let Some(agent_id) = target.agent_id.as_deref() else {
                set_error_message(&mut app.state, "agent scope has no selected agent");
                return Ok(());
            };
            let snapshot = current_snapshot(app)?;
            steer_agent(
                &app.root,
                &app.config,
                wave_id,
                agent_id,
                message,
                DirectiveOrigin::Operator,
                "wave-tui",
            )?;
            let _ = record_operator_shell_guidance_turn(
                &app.root,
                &app.config,
                shell_scope_for_runtime(target.scope),
                Some(wave_id),
                Some(agent_id),
                message,
                &format!("sent guidance to {target_label}"),
                &app.tab.title().to_ascii_lowercase(),
                app.state.follow_mode.label(),
                current_shell_mode(snapshot),
                "wave-tui",
            );
            set_info_message(&mut app.state, format!("sent guidance to {target_label}"));
        }
        ShellScope::Wave => {
            let Some(wave_id) = target.wave_id else {
                set_error_message(&mut app.state, "shell target has no wave context");
                return Ok(());
            };
            let snapshot = current_snapshot(app)?;
            steer_wave(
                &app.root,
                &app.config,
                wave_id,
                message,
                DirectiveOrigin::Operator,
                "wave-tui",
            )?;
            let _ = record_operator_shell_guidance_turn(
                &app.root,
                &app.config,
                shell_scope_for_runtime(target.scope),
                Some(wave_id),
                None,
                message,
                &format!("sent guidance to {target_label}"),
                &app.tab.title().to_ascii_lowercase(),
                app.state.follow_mode.label(),
                current_shell_mode(snapshot),
                "wave-tui",
            );
            set_info_message(&mut app.state, format!("sent guidance to {target_label}"));
        }
    }
    Ok(())
}

fn handle_shell_command(app: &mut App, input: &str) -> Result<()> {
    let mut parts = input.split_whitespace();
    let command = parts.next().unwrap_or_default();

    match command {
        "/wave" => {
            let Some(raw_wave_id) = parts.next() else {
                set_error_message(&mut app.state, "usage: /wave <id>");
                return Ok(());
            };
            let wave_id = raw_wave_id.parse::<u32>().context("invalid wave id")?;
            let (index, title) = {
                let snapshot = current_snapshot(app)?;
                let Some(index) = snapshot
                    .planning
                    .waves
                    .iter()
                    .position(|wave| wave.id == wave_id)
                else {
                    set_error_message(&mut app.state, format!("wave {wave_id} not found"));
                    return Ok(());
                };
                (index, snapshot.planning.waves[index].title.clone())
            };
            app.state.selected_wave_index = index;
            app.state.selected_operator_action_index = 0;
            app.state.selected_orchestrator_agent_index = 0;
            app.state.shell_target = Some(ShellTargetState {
                scope: ShellScope::Wave,
                wave_id: Some(wave_id),
                agent_id: None,
            });
            set_info_message(
                &mut app.state,
                format!("shell retargeted to wave {wave_id} {title}"),
            );
        }
        "/agent" => {
            let Some(agent_id) = parts.next() else {
                set_error_message(&mut app.state, "usage: /agent <id>");
                return Ok(());
            };
            let agent_id = agent_id.to_string();
            let (wave_id, wave_index, index, title) = {
                let snapshot = current_snapshot(app)?;
                let Some(wave_id) = selected_action_wave_id(&app.state, snapshot) else {
                    set_error_message(&mut app.state, "no wave selected");
                    return Ok(());
                };
                let Some(index) = orchestrator_agent_index(snapshot, wave_id, &agent_id) else {
                    set_error_message(
                        &mut app.state,
                        format!("agent {agent_id} not found on wave {wave_id}"),
                    );
                    return Ok(());
                };
                let title = snapshot
                    .panels
                    .orchestrator
                    .waves
                    .iter()
                    .find(|wave| wave.wave_id == wave_id)
                    .and_then(|wave| wave.agents.get(index))
                    .map(|agent| agent.title.clone())
                    .unwrap_or_else(|| agent_id.clone());
                let wave_index = snapshot
                    .planning
                    .waves
                    .iter()
                    .position(|wave| wave.id == wave_id)
                    .unwrap_or(app.state.selected_wave_index);
                (wave_id, wave_index, index, title)
            };
            app.state.selected_wave_index = wave_index;
            app.state.selected_orchestrator_agent_index = index;
            app.state.selected_operator_action_index = 0;
            app.state.shell_target = Some(ShellTargetState {
                scope: ShellScope::Agent,
                wave_id: Some(wave_id),
                agent_id: Some(agent_id.clone()),
            });
            set_info_message(
                &mut app.state,
                format!("shell retargeted to agent {agent_id} {title} on wave {wave_id}"),
            );
        }
        "/scope" => {
            let Some(scope) = parts.next() else {
                set_error_message(&mut app.state, "usage: /scope head|wave|agent");
                return Ok(());
            };
            let (shell_target, label) = {
                let snapshot = current_snapshot(app)?;
                let selected_wave_id = selected_action_wave_id(&app.state, snapshot);
                let shell_target = match scope {
                    "head" => ShellTargetState {
                        scope: ShellScope::Head,
                        wave_id: None,
                        agent_id: None,
                    },
                    "wave" => ShellTargetState {
                        scope: ShellScope::Wave,
                        wave_id: selected_wave_id,
                        agent_id: None,
                    },
                    "agent" => {
                        let Some(wave_id) = selected_wave_id else {
                            set_error_message(&mut app.state, "no wave selected");
                            return Ok(());
                        };
                        let Some(agent) = selected_orchestrator_agent(snapshot, &app.state) else {
                            set_error_message(&mut app.state, "no orchestrator agent selected");
                            return Ok(());
                        };
                        ShellTargetState {
                            scope: ShellScope::Agent,
                            wave_id: Some(wave_id),
                            agent_id: Some(agent.id.clone()),
                        }
                    }
                    _ => {
                        set_error_message(&mut app.state, "usage: /scope head|wave|agent");
                        return Ok(());
                    }
                };
                let label = shell_target_label(&shell_target, snapshot);
                (shell_target, label)
            };
            app.state.shell_target = Some(shell_target.clone());
            set_info_message(&mut app.state, format!("shell scope set to {}", label));
        }
        "/mode" => {
            let Some(mode) = parts.next() else {
                set_error_message(&mut app.state, "usage: /mode operator|autonomous");
                return Ok(());
            };
            let mode = match mode {
                "operator" => OrchestratorMode::Operator,
                "autonomous" => OrchestratorMode::Autonomous,
                _ => {
                    set_error_message(&mut app.state, "usage: /mode operator|autonomous");
                    return Ok(());
                }
            };
            let target = {
                let snapshot = current_snapshot(app)?;
                current_shell_target(&app.state, snapshot)
            };
            let wave_ids = {
                let snapshot = current_snapshot(app)?;
                if matches!(target.scope, ShellScope::Head) && target.wave_id.is_none() {
                    let wave_ids = active_wave_ids(snapshot);
                    if wave_ids.is_empty() {
                        set_error_message(&mut app.state, "no active waves in head workspace");
                        return Ok(());
                    }
                    wave_ids
                } else {
                    let Some(wave_id) = target
                        .wave_id
                        .or_else(|| selected_action_wave_id(&app.state, snapshot))
                    else {
                        set_error_message(&mut app.state, "no wave selected");
                        return Ok(());
                    };
                    vec![wave_id]
                }
            };
            for wave_id in &wave_ids {
                set_orchestrator_mode(&app.root, &app.config, *wave_id, mode, "wave-tui")?;
            }
            let scope_wave_id =
                if matches!(target.scope, ShellScope::Head) && target.wave_id.is_none() {
                    None
                } else {
                    target.wave_id.or_else(|| wave_ids.first().copied())
                };
            upsert_operator_shell_session(
                &app.root,
                &app.config,
                shell_scope_for_runtime(target.scope),
                scope_wave_id,
                target.agent_id.as_deref(),
                &app.tab.title().to_ascii_lowercase(),
                app.state.follow_mode.label(),
                mode,
                "wave-tui",
            )?;
            set_info_message(
                &mut app.state,
                format!(
                    "orchestrator mode set to {} for {}",
                    if matches!(mode, OrchestratorMode::Autonomous) {
                        "autonomous"
                    } else {
                        "operator"
                    },
                    if wave_ids.len() == 1 {
                        format!("wave {}", wave_ids[0])
                    } else {
                        format!(
                            "active waves {}",
                            wave_ids
                                .iter()
                                .map(u32::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                ),
            );
        }
        "/launch" => {
            let explicit_wave_id = match parts.next() {
                Some(raw_wave_id) => Some(raw_wave_id.parse::<u32>().context("invalid wave id")?),
                None => None,
            };
            let planning = current_snapshot(app)?.planning.clone();
            let waves = load_wave_documents(&app.config, &app.root)?;
            let report = launch_wave(
                &app.root,
                &app.config,
                &waves,
                &planning,
                LaunchOptions {
                    wave_id: explicit_wave_id.or_else(|| {
                        app.snapshot
                            .as_ref()
                            .and_then(|snapshot| selected_action_wave_id(&app.state, snapshot))
                    }),
                    dry_run: false,
                },
            )?;
            if let Some(snapshot) = app.snapshot.as_ref() {
                let _ = select_wave_by_id(&mut app.state, snapshot, report.wave_id);
            }
            app.state.shell_target = Some(ShellTargetState {
                scope: ShellScope::Wave,
                wave_id: Some(report.wave_id),
                agent_id: None,
            });
            set_info_message(
                &mut app.state,
                format!("launched wave {} as run {}", report.wave_id, report.run_id),
            );
        }
        "/rerun" => {
            let scope = match parts.next() {
                Some("full") | None => RerunScope::Full,
                Some("closure-only") => RerunScope::ClosureOnly,
                Some("promotion-only") => RerunScope::PromotionOnly,
                Some("from-first-incomplete") => RerunScope::FromFirstIncomplete,
                Some(_) => {
                    set_error_message(
                        &mut app.state,
                        "usage: /rerun [full|from-first-incomplete|closure-only|promotion-only]",
                    );
                    return Ok(());
                }
            };
            handle_request_rerun_with_scope(app, scope)?;
        }
        "/clear-rerun" => handle_clear_rerun(app)?,
        "/pause" => {
            let (wave_id, agent_id) = require_selected_shell_agent(app)?;
            pause_agent(
                &app.root,
                &app.config,
                wave_id,
                &agent_id,
                DirectiveOrigin::Operator,
                "wave-tui",
            )?;
            set_info_message(&mut app.state, format!("paused agent {agent_id}"));
        }
        "/resume" => {
            let (wave_id, agent_id) = require_selected_shell_agent(app)?;
            resume_agent(
                &app.root,
                &app.config,
                wave_id,
                &agent_id,
                DirectiveOrigin::Operator,
                "wave-tui",
            )?;
            set_info_message(&mut app.state, format!("resumed agent {agent_id}"));
        }
        "/rerun-agent" => {
            let (wave_id, agent_id) = require_selected_shell_agent(app)?;
            rerun_agent(
                &app.root,
                &app.config,
                wave_id,
                &agent_id,
                DirectiveOrigin::Operator,
                "wave-tui",
            )?;
            set_info_message(&mut app.state, format!("scheduled rerun for {agent_id}"));
        }
        "/rebase" => {
            let (wave_id, agent_id) = require_selected_shell_agent(app)?;
            rebase_agent_sandbox(
                &app.root,
                &app.config,
                wave_id,
                &agent_id,
                DirectiveOrigin::Operator,
                "wave-tui",
            )?;
            set_info_message(&mut app.state, format!("scheduled rebase for {agent_id}"));
        }
        "/reconcile" => {
            let (wave_id, agent_id) = require_selected_shell_agent(app)?;
            request_agent_reconciliation(
                &app.root,
                &app.config,
                wave_id,
                &agent_id,
                DirectiveOrigin::Operator,
                "wave-tui",
            )?;
            set_info_message(
                &mut app.state,
                format!("requested reconciliation for {agent_id}"),
            );
        }
        "/approve-merge" => {
            let (wave_id, agent_id) = require_selected_shell_agent(app)?;
            approve_agent_merge(
                &app.root,
                &app.config,
                wave_id,
                &agent_id,
                DirectiveOrigin::Operator,
                "wave-tui",
            )?;
            set_info_message(&mut app.state, format!("approved merge for {agent_id}"));
        }
        "/reject-merge" => {
            let (wave_id, agent_id) = require_selected_shell_agent(app)?;
            reject_agent_merge(
                &app.root,
                &app.config,
                wave_id,
                &agent_id,
                DirectiveOrigin::Operator,
                "wave-tui",
            )?;
            set_info_message(&mut app.state, format!("rejected merge for {agent_id}"));
        }
        "/approve" => handle_prepare_operator_action(app, true)?,
        "/reject" => handle_prepare_operator_action(app, false)?,
        "/close" => handle_prepare_manual_close(app)?,
        "/open" => {
            let Some(tab) = parts.next() else {
                set_error_message(
                    &mut app.state,
                    "usage: /open overview|agents|queue|proof|control",
                );
                return Ok(());
            };
            app.tab = match tab {
                "overview" => PanelTab::Overview,
                "agents" => PanelTab::Agents,
                "queue" => PanelTab::Queue,
                "proof" => PanelTab::Proof,
                "control" => PanelTab::Control,
                _ => {
                    set_error_message(
                        &mut app.state,
                        "usage: /open overview|agents|queue|proof|control",
                    );
                    return Ok(());
                }
            };
            set_info_message(&mut app.state, format!("opened {} panel", app.tab.title()));
        }
        "/follow" => {
            let Some(mode) = parts.next() else {
                set_error_message(&mut app.state, "usage: /follow run|agent|off");
                return Ok(());
            };
            let follow_mode = match mode {
                "run" => FollowMode::Run,
                "agent" => FollowMode::Agent,
                "off" => FollowMode::Off,
                _ => {
                    set_error_message(&mut app.state, "usage: /follow run|agent|off");
                    return Ok(());
                }
            };
            app.state.follow_mode = follow_mode;
            if matches!(follow_mode, FollowMode::Agent) {
                let snapshot = current_snapshot(app)?;
                let Some(wave_id) = selected_action_wave_id(&app.state, snapshot) else {
                    set_error_message(&mut app.state, "no wave selected");
                    return Ok(());
                };
                let Some(agent) = selected_orchestrator_agent(snapshot, &app.state) else {
                    set_error_message(&mut app.state, "no orchestrator agent selected");
                    return Ok(());
                };
                app.state.shell_target = Some(ShellTargetState {
                    scope: ShellScope::Agent,
                    wave_id: Some(wave_id),
                    agent_id: Some(agent.id.clone()),
                });
            }
            if let Ok(snapshot) = current_snapshot(app).cloned() {
                apply_follow_mode(&mut app.state, &snapshot);
            }
            set_info_message(
                &mut app.state,
                format!("follow mode set to {}", follow_mode.label()),
            );
        }
        "/search" => {
            let query = input.trim_start_matches("/search").trim();
            if query.is_empty() {
                set_error_message(&mut app.state, "usage: /search <text>");
                return Ok(());
            }
            app.state.transcript_search = Some(query.to_string());
            app.state.transcript_scroll = 0;
            set_info_message(
                &mut app.state,
                format!("transcript search set to \"{query}\""),
            );
        }
        "/clear-search" => {
            app.state.transcript_search = None;
            app.state.transcript_scroll = 0;
            set_info_message(&mut app.state, "cleared transcript search");
        }
        "/compare" => {
            let Some(kind) = parts.next() else {
                set_error_message(
                    &mut app.state,
                    "usage: /compare wave <id> | /compare agent <id>",
                );
                return Ok(());
            };
            match kind {
                "wave" => {
                    let Some(raw_wave_id) = parts.next() else {
                        set_error_message(&mut app.state, "usage: /compare wave <id>");
                        return Ok(());
                    };
                    let wave_id = raw_wave_id.parse::<u32>().context("invalid wave id")?;
                    let snapshot = current_snapshot(app)?;
                    if selected_wave_by_id(snapshot, wave_id).is_none() {
                        set_error_message(&mut app.state, format!("wave {wave_id} not found"));
                        return Ok(());
                    }
                    app.state.compare_mode = Some(CompareMode::Wave { wave_id });
                    app.state.transcript_scroll = 0;
                    set_info_message(
                        &mut app.state,
                        format!("comparing selected wave to wave {wave_id}"),
                    );
                }
                "agent" => {
                    let Some(agent_id) = parts.next() else {
                        set_error_message(&mut app.state, "usage: /compare agent <id>");
                        return Ok(());
                    };
                    let snapshot = current_snapshot(app)?;
                    let Some(wave_id) = selected_wave_id(&app.state, snapshot) else {
                        set_error_message(&mut app.state, "no wave selected");
                        return Ok(());
                    };
                    let Some(wave) = selected_orchestrator_wave(snapshot, &app.state) else {
                        set_error_message(
                            &mut app.state,
                            format!("wave {wave_id} has no MAS agent workspace"),
                        );
                        return Ok(());
                    };
                    if !wave.agents.iter().any(|agent| agent.id == agent_id) {
                        set_error_message(
                            &mut app.state,
                            format!("agent {agent_id} not found on wave {wave_id}"),
                        );
                        return Ok(());
                    }
                    app.state.compare_mode = Some(CompareMode::Agent {
                        agent_id: agent_id.to_string(),
                    });
                    app.state.transcript_scroll = 0;
                    set_info_message(
                        &mut app.state,
                        format!("comparing selected agent to {agent_id}"),
                    );
                }
                _ => {
                    set_error_message(
                        &mut app.state,
                        "usage: /compare wave <id> | /compare agent <id>",
                    );
                    return Ok(());
                }
            }
        }
        "/clear-compare" => {
            app.state.compare_mode = None;
            app.state.transcript_scroll = 0;
            set_info_message(&mut app.state, "cleared compare mode");
        }
        "/help" => {
            app.state.help_visible = true;
            set_info_message(&mut app.state, "operator shell help opened");
        }
        _ => {
            set_error_message(
                &mut app.state,
                format!("unknown command: {command}. Press ? for help."),
            );
        }
    }

    let _ = persist_shell_session(app);
    Ok(())
}

fn require_selected_shell_agent(app: &mut App) -> Result<(u32, String)> {
    let snapshot = current_snapshot(app)?;
    let Some(wave_id) = selected_action_wave_id(&app.state, snapshot) else {
        bail!("no wave selected");
    };
    let Some(agent) = selected_orchestrator_agent(snapshot, &app.state) else {
        bail!("no orchestrator agent selected");
    };
    Ok((wave_id, agent.id.clone()))
}

fn handle_request_rerun(app: &mut App) -> Result<()> {
    handle_request_rerun_with_scope(app, RerunScope::Full)
}

fn handle_request_rerun_with_scope(app: &mut App, scope: RerunScope) -> Result<()> {
    let wave_id = {
        let snapshot = current_snapshot(app)?;
        let Some(wave_id) = selected_action_wave_id(&app.state, snapshot) else {
            set_error_message(&mut app.state, "no wave selected");
            return Ok(());
        };
        wave_id
    };
    request_rerun(
        &app.root,
        &app.config,
        wave_id,
        "Requested from the Wave operator shell",
        scope,
    )?;
    set_info_message(
        &mut app.state,
        format!(
            "requested {} rerun for wave {}",
            debug_label(scope),
            wave_id
        ),
    );
    Ok(())
}

fn handle_clear_rerun(app: &mut App) -> Result<()> {
    let wave_id = {
        let snapshot = current_snapshot(app)?;
        let Some(wave_id) = selected_action_wave_id(&app.state, snapshot) else {
            set_error_message(&mut app.state, "no wave selected");
            return Ok(());
        };
        wave_id
    };
    let result = clear_rerun(&app.root, &app.config, wave_id)?;
    match result {
        Some(_) => set_info_message(&mut app.state, format!("cleared rerun for wave {wave_id}")),
        None => set_info_message(
            &mut app.state,
            format!("no rerun intent for wave {wave_id}"),
        ),
    }
    Ok(())
}

fn handle_prepare_manual_close(app: &mut App) -> Result<()> {
    let (wave_id, wave_title, already_active) = {
        let snapshot = current_snapshot(app)?;
        let Some(wave_id) = selected_action_wave_id(&app.state, snapshot) else {
            set_error_message(&mut app.state, "no wave selected");
            return Ok(());
        };
        let Some(wave) = selected_wave_by_id(snapshot, wave_id) else {
            set_error_message(&mut app.state, format!("wave {wave_id} not found"));
            return Ok(());
        };
        let already_active = snapshot
            .closure_overrides
            .iter()
            .any(|record| record.wave_id == wave.id && record.is_active());
        (wave.id, wave.title.clone(), already_active)
    };
    if already_active {
        set_error_message(
            &mut app.state,
            format!(
                "wave {} already has an active manual close override",
                wave_id
            ),
        );
        return Ok(());
    }

    match preview_closure_override(&app.root, &app.config, wave_id, None, Vec::new()) {
        Ok(preview) => {
            let summary = manual_close_summary_from_preview(&preview);
            let detail = preview
                .source_run_error
                .clone()
                .or(preview.source_promotion_detail.clone())
                .unwrap_or_else(|| {
                    "Manual close requested from the Wave operator shell.".to_string()
                });
            app.state.pending_control_action = Some(PendingControlAction::ApplyManualClose(
                ManualCloseConfirmation {
                    wave_id,
                    wave_title,
                    source_run_id: preview.source_run_id,
                    evidence_paths: preview.evidence_paths,
                    reason: "Applied from the Wave operator shell after operator review of the latest terminal run".to_string(),
                    detail,
                    summary,
                },
            ));
            set_info_message(
                &mut app.state,
                format!("review manual close confirmation for wave {}", wave_id),
            );
        }
        Err(error) => set_error_message(&mut app.state, error.to_string()),
    }
    Ok(())
}

fn handle_prepare_clear_manual_close(app: &mut App) -> Result<()> {
    let (wave_id, wave_title, source_run_id) = {
        let snapshot = current_snapshot(app)?;
        let Some(wave_id) = selected_action_wave_id(&app.state, snapshot) else {
            set_error_message(&mut app.state, "no wave selected");
            return Ok(());
        };
        let Some(wave) = selected_wave_by_id(snapshot, wave_id) else {
            set_error_message(&mut app.state, format!("wave {wave_id} not found"));
            return Ok(());
        };
        let active_wave_id = wave.id;
        let Some(record) = snapshot
            .closure_overrides
            .iter()
            .find(|record| record.wave_id == active_wave_id && record.is_active())
        else {
            set_error_message(
                &mut app.state,
                format!(
                    "wave {} has no active manual close override",
                    active_wave_id
                ),
            );
            return Ok(());
        };
        (
            active_wave_id,
            wave.title.clone(),
            record.source_run_id.clone(),
        )
    };

    app.state.pending_control_action = Some(PendingControlAction::ClearManualClose(
        ClearManualCloseConfirmation {
            wave_id,
            wave_title,
            source_run_id,
        },
    ));
    set_info_message(
        &mut app.state,
        format!(
            "review manual close clear confirmation for wave {}",
            wave_id
        ),
    );
    Ok(())
}

fn handle_prepare_operator_action(app: &mut App, approve: bool) -> Result<()> {
    let (wave_id, wave_title, item) = {
        let snapshot = current_snapshot(app)?;
        let Some(item) = selected_visible_actionable_operator_item(&app.state, snapshot) else {
            set_error_message(
                &mut app.state,
                "no actionable approval, proposal, or escalation item is selected",
            );
            return Ok(());
        };
        let wave_id = item.wave_id;
        let Some(wave) = selected_wave_by_id(snapshot, wave_id) else {
            set_error_message(&mut app.state, format!("wave {wave_id} not found"));
            return Ok(());
        };
        (wave_id, wave.title.clone(), item.clone())
    };

    let confirmation = OperatorActionConfirmation {
        wave_id,
        wave_title,
        record_id: item.record_id,
        kind: item.kind,
        summary: item.summary,
        waiting_on: item.waiting_on,
        next_action: item.next_action,
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
            wave_id
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
                wave_app_server::OperatorActionableKind::Proposal => {
                    apply_head_proposal(&app.root, &app.config, &confirmation.record_id, "wave-tui")
                        .map(|proposal| {
                            format!(
                                "applied head proposal {} for wave {}",
                                proposal.proposal_id, proposal.wave_id
                            )
                        })
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
                wave_app_server::OperatorActionableKind::Proposal => dismiss_head_proposal(
                    &app.root,
                    &app.config,
                    &confirmation.record_id,
                    "wave-tui",
                )
                .map(|proposal| {
                    format!(
                        "dismissed head proposal {} for wave {}",
                        proposal.proposal_id, proposal.wave_id
                    )
                }),
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
    let actionable_count =
        visible_actionable_operator_items(snapshot, control_review_wave_filter(state, snapshot))
            .len();
    if actionable_count == 0 {
        state.selected_operator_action_index = 0;
        return;
    }
    state.selected_operator_action_index = state
        .selected_operator_action_index
        .min(actionable_count.saturating_sub(1));
}

fn clamp_selected_orchestrator_agent(state: &mut AppState, snapshot: &OperatorSnapshot) {
    let Some(wave) = selected_orchestrator_wave(snapshot, state) else {
        state.selected_orchestrator_agent_index = 0;
        return;
    };
    if wave.agents.is_empty() {
        state.selected_orchestrator_agent_index = 0;
        return;
    }
    state.selected_orchestrator_agent_index = state
        .selected_orchestrator_agent_index
        .min(wave.agents.len().saturating_sub(1));
}

fn select_next_wave(state: &mut AppState, snapshot: &OperatorSnapshot) {
    if snapshot.planning.waves.is_empty() {
        return;
    }
    state.selected_wave_index =
        (state.selected_wave_index + 1).min(snapshot.planning.waves.len() - 1);
    state.selected_orchestrator_agent_index = 0;
    state.selected_operator_action_index = 0;
    sync_shell_target_to_selected_wave(state, snapshot);
}

fn select_previous_wave(state: &mut AppState, snapshot: &OperatorSnapshot) {
    state.selected_wave_index = state.selected_wave_index.saturating_sub(1);
    state.selected_orchestrator_agent_index = 0;
    state.selected_operator_action_index = 0;
    sync_shell_target_to_selected_wave(state, snapshot);
}

fn select_next_orchestrator_agent(state: &mut AppState, snapshot: &OperatorSnapshot) {
    let Some(wave) = selected_orchestrator_wave(snapshot, state) else {
        state.selected_orchestrator_agent_index = 0;
        return;
    };
    if wave.agents.is_empty() {
        state.selected_orchestrator_agent_index = 0;
        return;
    }
    state.selected_orchestrator_agent_index =
        (state.selected_orchestrator_agent_index + 1).min(wave.agents.len() - 1);
    sync_shell_target_to_selected_agent(state, snapshot);
}

fn select_previous_orchestrator_agent(state: &mut AppState, snapshot: &OperatorSnapshot) {
    state.selected_orchestrator_agent_index =
        state.selected_orchestrator_agent_index.saturating_sub(1);
    sync_shell_target_to_selected_agent(state, snapshot);
}

fn select_next_operator_action(state: &mut AppState, snapshot: &OperatorSnapshot) {
    let actionable_count =
        visible_actionable_operator_items(snapshot, control_review_wave_filter(state, snapshot))
            .len();
    if actionable_count == 0 {
        state.selected_operator_action_index = 0;
        return;
    }
    state.selected_operator_action_index =
        (state.selected_operator_action_index + 1).min(actionable_count - 1);
}

fn select_previous_operator_action(state: &mut AppState, snapshot: &OperatorSnapshot) {
    if visible_actionable_operator_items(snapshot, control_review_wave_filter(state, snapshot))
        .is_empty()
    {
        state.selected_operator_action_index = 0;
        return;
    }
    state.selected_operator_action_index = state.selected_operator_action_index.saturating_sub(1);
}

fn selected_wave_id(state: &AppState, snapshot: &OperatorSnapshot) -> Option<u32> {
    selected_wave(state, snapshot).map(|wave| wave.id)
}

fn selected_orchestrator_wave<'a>(
    snapshot: &'a OperatorSnapshot,
    state: &AppState,
) -> Option<&'a wave_app_server::WaveOrchestratorSnapshot> {
    let wave_id = selected_wave_id(state, snapshot)?;
    snapshot
        .panels
        .orchestrator
        .waves
        .iter()
        .find(|wave| wave.wave_id == wave_id)
}

fn selected_orchestrator_agent<'a>(
    snapshot: &'a OperatorSnapshot,
    state: &AppState,
) -> Option<&'a wave_app_server::MasAgentSnapshot> {
    selected_orchestrator_wave(snapshot, state)?
        .agents
        .get(state.selected_orchestrator_agent_index)
}

fn selected_wave<'a>(
    state: &AppState,
    snapshot: &'a OperatorSnapshot,
) -> Option<&'a wave_control_plane::WaveStatusReadModel> {
    snapshot.planning.waves.get(state.selected_wave_index)
}

fn selected_queue_wave_index(snapshot: &OperatorSnapshot, state: &AppState) -> Option<usize> {
    let wave_id = selected_wave_id(state, snapshot)?;
    snapshot
        .panels
        .queue
        .waves
        .iter()
        .position(|wave| wave.id == wave_id)
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
                        | wave_app_server::OperatorActionableKind::Proposal
                        | wave_app_server::OperatorActionableKind::Escalation
                )
        })
        .collect()
}

fn control_review_wave_filter(state: &AppState, snapshot: &OperatorSnapshot) -> Option<u32> {
    let shell_target = current_shell_target(state, snapshot);
    if matches!(shell_target.scope, ShellScope::Head) && shell_target.wave_id.is_none() {
        None
    } else {
        selected_wave_id(state, snapshot)
    }
}

fn control_context_wave_id(state: &AppState, snapshot: &OperatorSnapshot) -> Option<u32> {
    let review_wave_filter = control_review_wave_filter(state, snapshot);
    if review_wave_filter.is_none() {
        selected_visible_actionable_operator_item(state, snapshot)
            .map(|item| item.wave_id)
            .or_else(|| selected_wave_id(state, snapshot))
    } else {
        selected_wave_id(state, snapshot)
    }
}

fn visible_actionable_operator_items<'a>(
    snapshot: &'a OperatorSnapshot,
    review_wave_filter: Option<u32>,
) -> Vec<&'a wave_app_server::OperatorActionableItem> {
    snapshot
        .operator_objects
        .iter()
        .filter(|item| {
            review_wave_filter
                .map(|wave_id| item.wave_id == wave_id)
                .unwrap_or(true)
                && matches!(
                    item.kind,
                    wave_app_server::OperatorActionableKind::Approval
                        | wave_app_server::OperatorActionableKind::Proposal
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

fn selected_visible_actionable_operator_item<'a>(
    state: &AppState,
    snapshot: &'a OperatorSnapshot,
) -> Option<&'a wave_app_server::OperatorActionableItem> {
    visible_actionable_operator_items(snapshot, control_review_wave_filter(state, snapshot))
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

fn selected_visible_actionable_operator_context<'a>(
    state: &AppState,
    snapshot: &'a OperatorSnapshot,
) -> Option<(usize, usize, &'a wave_app_server::OperatorActionableItem)> {
    let items =
        visible_actionable_operator_items(snapshot, control_review_wave_filter(state, snapshot));
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

fn draw_ui(frame: &mut ratatui::Frame<'_>, app: &App) {
    let area = frame.area();
    if let Some(snapshot) = app.snapshot.as_ref() {
        match shell_layout_mode(area.width) {
            ShellLayoutMode::Wide => draw_wide_shell(frame, area, snapshot, app),
            ShellLayoutMode::Narrow => draw_narrow_shell(frame, area, snapshot, app),
        }
    } else {
        draw_loading_shell(frame, area, app);
    }

    if app.state.help_visible {
        draw_help_overlay(frame, area, app);
    }
}

fn draw_loading_shell(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let mut lines = vec![
        Line::styled(
            "Wave Operator Shell",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::raw("Loading reducer-backed operator snapshot..."),
        Line::raw("The shell keeps rendering while the first snapshot loads."),
    ];
    if let Some(error) = app.snapshot_error.as_deref() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("Last refresh error: {error}"),
            Style::default().fg(Color::Red),
        ));
    }
    if let Some(started_at) = app.last_refresh_started_at {
        lines.push(Line::raw(format!(
            "Refresh in flight: {}",
            HumanDuration(started_at.elapsed().as_millis())
        )));
    }
    lines.push(Line::raw(""));
    lines.push(Line::raw("Keys: ? help  Ctrl+C quit"));

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Operator shell"),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
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

fn draw_narrow_shell(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    app: &App,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);
    draw_main_pane(frame, chunks[0], snapshot, &app.state);
    draw_right_panel(frame, chunks[1], snapshot, app, ShellLayoutMode::Narrow);
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
    let target = current_shell_target(state, snapshot);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(8),
            Constraint::Length(if state.pending_control_action.is_some() {
                6
            } else {
                5
            }),
        ])
        .split(area);

    let header_lines = shell_header_lines(snapshot, state, &target);
    frame.render_widget(
        Paragraph::new(header_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Operator shell"),
        ),
        chunks[0],
    );

    let transcript_block = Block::default()
        .borders(Borders::ALL)
        .border_style(focus_border_style(matches!(
            state.focus,
            FocusLane::Transcript
        )))
        .title(main_pane_title(state));
    let transcript = Paragraph::new(main_pane_lines(snapshot, state))
        .block(transcript_block)
        .scroll((state.transcript_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(transcript, chunks[1]);

    let composer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(focus_border_style(matches!(
            state.focus,
            FocusLane::Composer
        )))
        .title(composer_title(state, &target, snapshot));
    let composer = Paragraph::new(composer_lines(snapshot, state, &target))
        .block(composer_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(composer, chunks[2]);

    if matches!(state.focus, FocusLane::Composer) {
        let x = chunks[2]
            .x
            .saturating_add(2 + state.composer_input.len() as u16);
        let y = chunks[2].y.saturating_add(1);
        frame.set_cursor_position((x, y));
    }
}

fn shell_header_lines<'a>(
    snapshot: &'a OperatorSnapshot,
    state: &'a AppState,
    target: &'a ShellTargetState,
) -> Vec<Line<'a>> {
    let freshness = snapshot_age_label(snapshot.generated_at_ms);
    let selected_wave = selected_wave(state, snapshot);
    let status = if snapshot.active_run_details.is_empty() {
        "idle".to_string()
    } else {
        format!("{} active", snapshot.active_run_details.len())
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                snapshot.dashboard.project_name.as_str(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                format!("target {}", shell_target_label(target, snapshot)),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw("  "),
            Span::styled(
                format!("follow {}", state.follow_mode.label()),
                Style::default().fg(Color::Gray),
            ),
            Span::raw("  "),
            Span::styled(
                format!("snapshot {freshness}"),
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(vec![
            Span::raw("mode "),
            Span::styled(
                snapshot.panels.orchestrator.mode.as_str(),
                Style::default()
                    .fg(if snapshot.panels.orchestrator.active {
                        Color::Yellow
                    } else {
                        Color::White
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::raw(format!(
                "queue ready {}  completed {}  status {}",
                snapshot.planning.next_ready_wave_ids.len(),
                snapshot.dashboard.completed_waves,
                status
            )),
        ]),
    ];
    if let Some(wave) = selected_wave {
        lines.push(Line::raw(format!(
            "selected wave {} {}  state={}  soft={}  blockers={}",
            wave.id,
            wave.title,
            describe_wave_state(wave.completed, wave.ready, &wave.blocked_by),
            wave.soft_state.label(),
            if wave.blocked_by.is_empty() {
                "none".to_string()
            } else {
                wave.blocked_by.join(" | ")
            }
        )));
    } else {
        lines.push(Line::raw("selected wave none"));
    }
    if let Some(compare_mode) = state.compare_mode.as_ref() {
        lines.push(Line::raw(format!(
            "view: {}",
            compare_mode_label(compare_mode, snapshot, state)
        )));
    } else if let Some(query) = state.transcript_search.as_deref() {
        lines.push(Line::raw(format!("view: transcript search=\"{query}\"")));
    }
    lines
}

fn main_pane_title(state: &AppState) -> String {
    if state.compare_mode.is_some() {
        "Compare".to_string()
    } else if let Some(query) = state.transcript_search.as_deref() {
        format!("Transcript / search: {query}")
    } else {
        "Transcript".to_string()
    }
}

fn main_pane_lines(snapshot: &OperatorSnapshot, state: &AppState) -> Vec<Line<'static>> {
    if let Some(compare_mode) = state.compare_mode.as_ref() {
        return compare_mode_lines(snapshot, state, compare_mode);
    }
    shell_transcript_lines(snapshot, state.transcript_search.as_deref())
}

fn shell_transcript_lines(snapshot: &OperatorSnapshot, search: Option<&str>) -> Vec<Line<'static>> {
    if snapshot.shell.transcript.is_empty() {
        return vec![Line::raw("No transcript items yet.")];
    }

    let normalized_search = search.map(|value| value.to_ascii_lowercase());
    let mut lines = Vec::new();
    if let Some(query) = search {
        lines.push(Line::raw(format!("Search filter: {query}")));
        lines.push(Line::raw(""));
    }
    for item in snapshot.shell.transcript.iter().rev() {
        if let Some(query) = normalized_search.as_deref() {
            let haystack = format!(
                "{}\n{}\n{}\n{}\n{}",
                item.kind,
                item.title,
                item.detail,
                item.origin.as_deref().unwrap_or_default(),
                item.status.as_deref().unwrap_or_default()
            )
            .to_ascii_lowercase();
            if !haystack.contains(query) {
                continue;
            }
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!(
                    "[{}{}]",
                    item.kind,
                    item.origin
                        .as_deref()
                        .map(|origin| format!(":{origin}"))
                        .unwrap_or_default()
                ),
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" "),
            Span::styled(
                item.title.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                item.status
                    .clone()
                    .unwrap_or_else(|| "observed".to_string()),
                Style::default().fg(Color::Gray),
            ),
        ]));
        lines.push(Line::raw(format!("  {}", item.detail)));
        if let Some(wave_id) = item.wave_id {
            lines.push(Line::raw(format!(
                "  wave={} agent={} at={}",
                wave_id,
                item.agent_id.clone().unwrap_or_else(|| "head".to_string()),
                item.created_at_ms
            )));
        }
        lines.push(Line::raw(""));
    }
    if lines.is_empty() || (lines.len() == 2 && search.is_some()) {
        return vec![Line::raw("No transcript items match the current search.")];
    }
    lines
}

fn composer_title(
    state: &AppState,
    target: &ShellTargetState,
    snapshot: &OperatorSnapshot,
) -> String {
    let suffix = if state.pending_control_action.is_some() {
        "confirm with Enter or cancel with Esc"
    } else if matches!(target.scope, ShellScope::Head) {
        "plain text asks the head for proposals; /help lists commands"
    } else {
        "plain text steers current target; /help lists commands"
    };
    format!(
        "Composer [{}] {suffix}",
        shell_target_label(target, snapshot)
    )
}

fn composer_lines<'a>(
    snapshot: &'a OperatorSnapshot,
    state: &'a AppState,
    target: &'a ShellTargetState,
) -> Vec<Line<'a>> {
    let mut lines = vec![
        Line::raw(format!("> {}", state.composer_input)),
        Line::raw(format!(
            "scope={}  focus={}  follow={}  last-event={}",
            target.scope.label(),
            focus_lane_label(state.focus),
            state.follow_mode.label(),
            snapshot
                .shell
                .last_event_at_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string())
        )),
    ];
    if let Some(compare_mode) = state.compare_mode.as_ref() {
        lines.push(Line::raw(format!(
            "compare={}  search={}",
            compare_mode_label(compare_mode, snapshot, state),
            state.transcript_search.as_deref().unwrap_or("off")
        )));
    } else if let Some(query) = state.transcript_search.as_deref() {
        lines.push(Line::raw(format!("transcript search=\"{query}\"")));
    }
    if let Some(pending_action) = state.pending_control_action.as_ref() {
        for item in pending_control_action_lines(pending_action) {
            lines.push(Line::raw(item));
        }
    } else if let Some(message) = state.flash_message.as_ref() {
        lines.push(Line::styled(
            message.text.clone(),
            flash_message_style(message.kind),
        ));
    } else {
        lines.push(Line::raw(
            "Tab cycles lanes. [ ] switch dashboard tabs. ? opens help.",
        ));
    }
    lines
}

fn draw_help_overlay(frame: &mut ratatui::Frame<'_>, area: Rect, app: &App) {
    let popup = centered_rect(area, 78, 70);
    let mut lines = vec![
        Line::styled(
            "Operator shell help",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::raw("Focus lanes: Tab / Shift+Tab"),
        Line::raw("Startup focus: Dashboard. Move to Composer explicitly to type guidance."),
        Line::raw("Transcript scroll: j k or arrows when Transcript is focused"),
        Line::raw("Dashboard navigation: j k or arrows when Dashboard is focused"),
        Line::raw("Composer: Enter submits, Esc returns to Dashboard"),
        Line::raw("Global: ? help, Ctrl+C cancel or quit, q quit"),
        Line::raw("Tabs: [ previous, ] next"),
        Line::raw(
            "Hotkeys: r rerun, c clear-rerun, m manual-close, M clear-close, u approve, x reject",
        ),
        Line::raw(""),
        Line::styled(
            "Commands",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some(snapshot) = app.snapshot.as_ref() {
        for command in &snapshot.shell.commands {
            lines.push(Line::raw(format!("{}  {}", command.usage, command.summary)));
        }
    } else {
        lines.push(Line::raw("/help  show shell commands"));
    }
    lines.push(Line::raw(""));
    lines.push(Line::raw("Esc or ? closes this overlay."));

    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Help"))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn centered_rect(area: Rect, width_percent: u16, height_percent: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}

fn focus_border_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn focus_lane_label(focus: FocusLane) -> &'static str {
    match focus {
        FocusLane::Transcript => "transcript",
        FocusLane::Composer => "composer",
        FocusLane::Dashboard => "dashboard",
    }
}

fn shell_target_label(target: &ShellTargetState, snapshot: &OperatorSnapshot) -> String {
    match target.scope {
        ShellScope::Head => match target.wave_id {
            Some(wave_id) => format!("head / wave {wave_id}"),
            None if !snapshot.active_run_details.is_empty() => format!(
                "head / active waves {}",
                snapshot
                    .active_run_details
                    .iter()
                    .map(|run| run.wave_id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => "head".to_string(),
        },
        ShellScope::Wave => target
            .wave_id
            .and_then(|wave_id| selected_wave_by_id(snapshot, wave_id))
            .map(|wave| format!("wave {} {}", wave.id, wave.title))
            .unwrap_or_else(|| "wave".to_string()),
        ShellScope::Agent => match (target.wave_id, target.agent_id.as_deref()) {
            (Some(wave_id), Some(agent_id)) => format!("wave {wave_id} / agent {agent_id}"),
            _ => "agent".to_string(),
        },
    }
}

fn compare_mode_label(
    compare_mode: &CompareMode,
    snapshot: &OperatorSnapshot,
    state: &AppState,
) -> String {
    match compare_mode {
        CompareMode::Wave { wave_id } => format!(
            "wave {} vs wave {}",
            selected_wave_id(state, snapshot)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            wave_id
        ),
        CompareMode::Agent { agent_id } => format!(
            "agent {} vs agent {}",
            selected_orchestrator_agent(snapshot, state)
                .map(|agent| agent.id.clone())
                .unwrap_or_else(|| "none".to_string()),
            agent_id
        ),
    }
}

fn compare_mode_lines(
    snapshot: &OperatorSnapshot,
    state: &AppState,
    compare_mode: &CompareMode,
) -> Vec<Line<'static>> {
    match compare_mode {
        CompareMode::Wave { wave_id } => compare_wave_lines(snapshot, state, *wave_id),
        CompareMode::Agent { agent_id } => compare_agent_lines(snapshot, state, agent_id),
    }
}

fn snapshot_age_label(generated_at_ms: u128) -> String {
    match wave_trace::now_epoch_ms() {
        Ok(now_ms) if now_ms >= generated_at_ms => {
            format!("{} ago", HumanDuration(now_ms - generated_at_ms))
        }
        Ok(_) => "just now".to_string(),
        Err(_) => generated_at_ms.to_string(),
    }
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

#[allow(dead_code)]
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

fn compare_wave_lines(
    snapshot: &OperatorSnapshot,
    state: &AppState,
    target_wave_id: u32,
) -> Vec<Line<'static>> {
    let Some(base_wave_id) = selected_wave_id(state, snapshot) else {
        return vec![Line::raw("Select a wave before comparing waves.")];
    };
    let Some(base_wave) = selected_wave_by_id(snapshot, base_wave_id) else {
        return vec![Line::raw("The selected base wave is no longer available.")];
    };
    let Some(target_wave) = selected_wave_by_id(snapshot, target_wave_id) else {
        return vec![Line::raw(format!("Wave {target_wave_id} was not found."))];
    };

    let base_orchestrator = snapshot
        .panels
        .orchestrator
        .waves
        .iter()
        .find(|wave| wave.wave_id == base_wave_id);
    let target_orchestrator = snapshot
        .panels
        .orchestrator
        .waves
        .iter()
        .find(|wave| wave.wave_id == target_wave_id);
    let base_package = acceptance_package_for_wave(snapshot, base_wave_id);
    let target_package = acceptance_package_for_wave(snapshot, target_wave_id);
    let base_run = selected_latest_run(snapshot, Some(base_wave_id));
    let target_run = selected_latest_run(snapshot, Some(target_wave_id));

    let mut lines = vec![
        Line::raw(format!(
            "Wave compare: {} {} vs {} {}",
            base_wave.id, base_wave.title, target_wave.id, target_wave.title
        )),
        Line::raw(""),
    ];
    lines.extend(compare_wave_side_lines(
        "base",
        base_wave,
        base_orchestrator,
        base_package,
        base_run,
    ));
    lines.push(Line::raw(""));
    lines.extend(compare_wave_side_lines(
        "target",
        target_wave,
        target_orchestrator,
        target_package,
        target_run,
    ));
    lines
}

fn compare_wave_side_lines(
    label: &str,
    wave: &wave_control_plane::WaveStatusReadModel,
    orchestrator: Option<&wave_app_server::WaveOrchestratorSnapshot>,
    acceptance_package: Option<&wave_app_server::AcceptancePackageSnapshot>,
    run: Option<&ActiveRunDetail>,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::styled(
            format!("{label}: wave {} {}", wave.id, wave.title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(format!(
            "queue={} soft={} claimable={} completed={} recovery_required={}",
            describe_wave_state(wave.completed, wave.ready, &wave.blocked_by),
            wave.soft_state.label(),
            yes_no(wave.readiness.claimable),
            yes_no(wave.completed),
            yes_no(wave.recovery.required)
        )),
    ];
    if !wave.blocked_by.is_empty() {
        lines.push(Line::raw(format!(
            "blockers: {}",
            wave.blocked_by.join(" | ")
        )));
    }
    if let Some(orchestrator) = orchestrator {
        lines.push(Line::raw(format!(
            "orchestrator mode={} active_run={} pending_proposals={} auto_actions={} last_head={}",
            orchestrator.mode,
            orchestrator.active_run_id.as_deref().unwrap_or("none"),
            orchestrator.pending_proposal_count,
            orchestrator.autonomous_action_count,
            orchestrator.last_head_summary.as_deref().unwrap_or("none")
        )));
        if let Some(failure) = orchestrator.last_autonomous_failure.as_deref() {
            lines.push(Line::raw(format!("last autonomous failure: {failure}")));
        }
    }
    if let Some(package) = acceptance_package {
        lines.push(Line::raw(format!(
            "delivery ship={} release={} signoff={} proof={}/{} risks={} debt={}",
            debug_label(package.ship_state),
            debug_label(package.release_state),
            debug_label(package.signoff.state),
            package.implementation.completed_agents,
            package.implementation.total_agents,
            package.known_risks.len(),
            package.outstanding_debt.len()
        )));
        if !package.blocking_reasons.is_empty() {
            lines.push(Line::raw(format!(
                "delivery blockers: {}",
                package.blocking_reasons.join(" | ")
            )));
        }
    }
    if let Some(run) = run {
        lines.push(Line::raw(format!(
            "run {} status={} proof={}/{} recovery={}",
            run.run_id,
            debug_label(run.status),
            run.proof.completed_agents,
            run.proof.total_agents,
            run.mas
                .as_ref()
                .and_then(|mas| mas.recovery.as_ref())
                .map(|recovery| recovery.status.clone())
                .unwrap_or_else(|| "none".to_string())
        )));
    }
    lines
}

fn compare_agent_lines(
    snapshot: &OperatorSnapshot,
    state: &AppState,
    target_agent_id: &str,
) -> Vec<Line<'static>> {
    let Some(wave) = selected_orchestrator_wave(snapshot, state) else {
        return vec![Line::raw("Select a MAS wave before comparing agents.")];
    };
    let Some(base_agent) = selected_orchestrator_agent(snapshot, state) else {
        return vec![Line::raw(
            "Select a base MAS agent before comparing agents.",
        )];
    };
    let Some(target_agent) = wave.agents.iter().find(|agent| agent.id == target_agent_id) else {
        return vec![Line::raw(format!(
            "Agent {target_agent_id} was not found on wave {}.",
            wave.wave_id
        ))];
    };

    let mut lines = vec![
        Line::raw(format!(
            "Agent compare: wave {} {} vs {}",
            wave.wave_id, base_agent.id, target_agent.id
        )),
        Line::raw(""),
    ];
    lines.extend(compare_agent_side_lines(
        "base",
        wave.wave_id,
        base_agent,
        latest_run_agent_item(snapshot, wave.wave_id, &base_agent.id),
        latest_directive_state_for_agent(snapshot, wave.wave_id, &base_agent.id),
    ));
    lines.push(Line::raw(""));
    lines.extend(compare_agent_side_lines(
        "target",
        wave.wave_id,
        target_agent,
        latest_run_agent_item(snapshot, wave.wave_id, &target_agent.id),
        latest_directive_state_for_agent(snapshot, wave.wave_id, &target_agent.id),
    ));
    lines
}

fn compare_agent_side_lines(
    label: &str,
    wave_id: u32,
    agent: &wave_app_server::MasAgentSnapshot,
    latest_agent_item: Option<&wave_app_server::AgentPanelItem>,
    latest_directive_state: Option<String>,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::styled(
            format!("{label}: agent {} {}", agent.id, agent.title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(format!(
            "state={} merge={} sandbox={} directives={} recovery={} lease_age={}",
            agent.status,
            agent.merge_state.as_deref().unwrap_or("none"),
            agent.sandbox_id.as_deref().unwrap_or("none"),
            agent.pending_directive_count,
            agent.recovery_state.as_deref().unwrap_or("none"),
            agent
                .heartbeat_age_ms
                .map(HumanDuration)
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string())
        )),
        Line::raw(format!(
            "last head={} last directive={} wave={}",
            agent.last_head_action.as_deref().unwrap_or("none"),
            latest_directive_state.as_deref().unwrap_or("none"),
            wave_id
        )),
    ];
    if let Some(item) = latest_agent_item {
        lines.push(Line::raw(format!(
            "proof={} source={} error={}",
            yes_no(item.proof_complete),
            item.proof_source,
            item.error.as_deref().unwrap_or("none")
        )));
    }
    if !agent.barrier_reasons.is_empty() {
        lines.push(Line::raw(format!(
            "barriers: {}",
            agent.barrier_reasons.join(" | ")
        )));
    }
    lines
}

fn latest_run_agent_item<'a>(
    snapshot: &'a OperatorSnapshot,
    wave_id: u32,
    agent_id: &str,
) -> Option<&'a wave_app_server::AgentPanelItem> {
    selected_latest_run(snapshot, Some(wave_id))?
        .agents
        .iter()
        .find(|agent| agent.id == agent_id)
}

fn latest_directive_state_for_agent(
    snapshot: &OperatorSnapshot,
    wave_id: u32,
    agent_id: &str,
) -> Option<String> {
    snapshot
        .panels
        .orchestrator
        .directives
        .iter()
        .rev()
        .find(|directive| {
            directive.wave_id == wave_id && directive.agent_id.as_deref() == Some(agent_id)
        })
        .map(|directive| {
            directive
                .delivery_state
                .clone()
                .unwrap_or_else(|| "pending".to_string())
        })
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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
        PanelTab::Overview => draw_overview_tab(frame, panel_chunks[1], snapshot, app),
        PanelTab::Agents => draw_agents_tab(frame, panel_chunks[1], snapshot, app),
        PanelTab::Queue => draw_queue_tab(frame, panel_chunks[1], snapshot, app),
        PanelTab::Proof => draw_proof_tab(
            frame,
            panel_chunks[1],
            snapshot,
            selected_wave_id(&app.state, snapshot),
        ),
        PanelTab::Control => draw_control_tab(frame, panel_chunks[1], snapshot, app),
    }
}

fn draw_overview_tab(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    app: &App,
) {
    let selected_wave_id = selected_wave_id(&app.state, snapshot);
    let mut lines = Vec::new();
    let target = current_shell_target(&app.state, snapshot);
    lines.push(Line::raw(format!(
        "target: {}",
        shell_target_label(&target, snapshot)
    )));
    lines.push(Line::raw(format!(
        "queue: ready={} active={} blocked={} next={}",
        snapshot.panels.queue.ready_wave_count,
        snapshot.panels.queue.active_wave_count,
        snapshot.panels.queue.blocked_wave_count,
        format_u32_list(&snapshot.panels.queue.next_ready_wave_ids)
    )));
    lines.push(Line::raw(format!(
        "head workspace: mode={} active={} autonomous_waves={} pending_proposals={} auto_actions={} head_failures={} recovery={} launcher_ready={}",
        snapshot.panels.orchestrator.mode,
        yes_no(snapshot.panels.orchestrator.active),
        format_u32_list(&snapshot.panels.orchestrator.autonomous_wave_ids),
        snapshot.panels.orchestrator.pending_proposal_count,
        snapshot.panels.orchestrator.autonomous_action_count,
        snapshot.panels.orchestrator.failed_head_turn_count,
        snapshot.panels.orchestrator.unresolved_recovery_count,
        yes_no(snapshot.panels.control.launcher_ready)
    )));
    if !snapshot
        .panels
        .orchestrator
        .recent_autonomous_actions
        .is_empty()
    {
        lines.push(Line::raw("recent autonomous actions:"));
        for action in snapshot
            .panels
            .orchestrator
            .recent_autonomous_actions
            .iter()
            .rev()
            .take(4)
        {
            lines.push(Line::raw(format!(
                "- wave {} {} {}",
                action.wave_id,
                action
                    .agent_id
                    .as_deref()
                    .map(|agent_id| format!("agent {}", agent_id))
                    .unwrap_or_else(|| "wave".to_string()),
                action.summary
            )));
        }
    }
    let active_waves = snapshot
        .panels
        .orchestrator
        .waves
        .iter()
        .filter(|wave| wave.active_run_id.is_some())
        .collect::<Vec<_>>();
    if !active_waves.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "Active wave workspace",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ));
        for wave in active_waves {
            let current_agent = snapshot
                .active_run_details
                .iter()
                .find(|run| run.wave_id == wave.wave_id)
                .and_then(|run| run.current_agent_id.as_deref())
                .unwrap_or("none");
            lines.push(Line::raw(format!(
                "wave {} mode={} current={} pending_proposals={} auto_actions={} recovery={}",
                wave.wave_id,
                wave.mode,
                current_agent,
                wave.pending_proposal_count,
                wave.autonomous_action_count,
                yes_no(wave.recovery_required)
            )));
            if let Some(summary) = wave.last_head_summary.as_deref() {
                lines.push(Line::raw(format!("  last head: {}", summary)));
            }
            if let Some(failure) = wave.last_autonomous_failure.as_deref() {
                lines.push(Line::raw(format!("  last failure: {}", failure)));
            }
        }
    }
    if let Some(wave) = selected_wave(&app.state, snapshot) {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!("Wave {} {}", wave.id, wave.title),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::raw(format!(
            "state={} soft={} claimable={} completed={}",
            describe_wave_state(wave.completed, wave.ready, &wave.blocked_by),
            wave.soft_state.label(),
            yes_no(wave.readiness.claimable),
            yes_no(wave.completed)
        )));
        if let Some(orchestrator_wave) = snapshot
            .panels
            .orchestrator
            .waves
            .iter()
            .find(|candidate| candidate.wave_id == wave.id)
        {
            lines.push(Line::raw(format!(
                "orchestrator mode={} pending_proposals={} auto_actions={} recovery_required={}",
                orchestrator_wave.mode,
                orchestrator_wave.pending_proposal_count,
                orchestrator_wave.autonomous_action_count,
                yes_no(orchestrator_wave.recovery_required)
            )));
            if let Some(summary) = orchestrator_wave.last_head_summary.as_deref() {
                lines.push(Line::raw(format!("last head summary: {}", summary)));
            }
            if let Some(failure) = orchestrator_wave.last_autonomous_failure.as_deref() {
                lines.push(Line::raw(format!("last autonomous failure: {}", failure)));
            }
        }
        if !wave.blocked_by.is_empty() {
            lines.push(Line::raw(format!(
                "blockers: {}",
                wave.blocked_by.join(" | ")
            )));
        }
        lines.push(Line::raw(""));
        lines.extend(portfolio_focus_lines(snapshot, Some(wave.id)));
        if let Some(run) = selected_active_run(snapshot, Some(wave.id)) {
            lines.push(Line::raw(""));
            lines.extend(run_summary_lines(run));
        }
        let recovery_lines = selected_wave_recovery_lines(snapshot, wave.id);
        if !recovery_lines.is_empty() {
            lines.push(Line::raw(""));
            lines.extend(recovery_lines);
        }
        let blocker_lines = blocker_item_lines(snapshot, &app.state, Some(wave.id));
        if !blocker_lines.is_empty() {
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "Top blockers",
                Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::BOLD),
            ));
            for item in blocker_lines.into_iter().take(8) {
                lines.push(Line::raw(item));
            }
        }
    } else if selected_wave_id.is_none() {
        lines.push(Line::raw(""));
        lines.push(Line::raw("No wave selected."));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Overview"))
            .wrap(Wrap { trim: false }),
        area,
    );
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
fn draw_orchestrator_tab(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    app: &App,
) {
    let mut lines = vec![
        Line::raw(format!(
            "mode={} active={} multi-agent-waves={}",
            snapshot.panels.orchestrator.mode,
            yes_no(snapshot.panels.orchestrator.active),
            snapshot.panels.orchestrator.multi_agent_wave_count
        )),
        Line::raw("controls: m=toggle mode s=steer selected agent j/k=select agent".to_string()),
        Line::raw(""),
    ];
    if let Some(wave) = selected_orchestrator_wave(snapshot, &app.state) {
        lines.push(Line::raw(format!("wave {} {}", wave.wave_id, wave.title)));
        lines.push(Line::raw(format!(
            "execution model={} active_run={}",
            wave.execution_model,
            wave.active_run_id
                .clone()
                .unwrap_or_else(|| "none".to_string())
        )));
        lines.push(Line::raw(""));
        for (index, agent) in wave.agents.iter().enumerate() {
            let prefix = if index == app.state.selected_orchestrator_agent_index {
                ">"
            } else {
                " "
            };
            lines.push(Line::raw(format!(
                "{prefix} {} {} [{}] deps={} resources={}",
                agent.id,
                agent.title,
                agent.status,
                if agent.depends_on_agents.is_empty() {
                    "none".to_string()
                } else {
                    agent.depends_on_agents.join(",")
                },
                if agent.exclusive_resources.is_empty() {
                    "none".to_string()
                } else {
                    agent.exclusive_resources.join(",")
                }
            )));
        }
        if let Some(agent) = selected_orchestrator_agent(snapshot, &app.state) {
            lines.push(Line::raw(""));
            lines.push(Line::raw(format!(
                "selected {} merge={} sandbox={}",
                agent.id,
                agent
                    .merge_state
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
                agent
                    .sandbox_id
                    .clone()
                    .unwrap_or_else(|| "none".to_string())
            )));
            if agent.barrier_reasons.is_empty() {
                lines.push(Line::raw("barriers: clear"));
            } else {
                lines.push(Line::raw(format!(
                    "barriers: {}",
                    agent.barrier_reasons.join(" | ")
                )));
            }
        }
    } else {
        lines.push(Line::raw("No multi-agent wave selected."));
    }
    lines.push(Line::raw(""));
    lines.push(Line::raw("directives:"));
    if snapshot.panels.orchestrator.directives.is_empty() {
        lines.push(Line::raw("  none"));
    } else {
        for directive in snapshot.panels.orchestrator.directives.iter().rev().take(8) {
            lines.push(Line::raw(format!(
                "  {} {} {} {}",
                directive.directive_id,
                directive.kind,
                directive
                    .agent_id
                    .clone()
                    .unwrap_or_else(|| format!("wave-{}", directive.wave_id)),
                directive
                    .delivery_state
                    .clone()
                    .unwrap_or_else(|| "pending".to_string())
            )));
            if let Some(message) = directive.message.as_ref() {
                lines.push(Line::raw(format!("    {}", message)));
            }
        }
    }
    if !app.state.composer_input.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::raw(format!("composer> {}", app.state.composer_input)));
    }
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("Orchestrator")),
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
    if let Some(wave_id) = selected_wave_id {
        let recovery_lines = selected_wave_recovery_lines(snapshot, wave_id);
        if !recovery_lines.is_empty() {
            lines.push(Line::raw(""));
            lines.extend(recovery_lines);
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
    app: &App,
) {
    let Some(wave) = selected_orchestrator_wave(snapshot, &app.state) else {
        frame.render_widget(
            Paragraph::new("No multi-agent wave selected.")
                .block(Block::default().borders(Borders::ALL).title("Agents")),
            area,
        );
        return;
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(8),
            Constraint::Length(8),
        ])
        .split(area);
    let summary = vec![
        Line::raw(format!(
            "wave {} mode={} active_run={} pending_proposals={} auto_actions={} recovery_required={}",
            wave.wave_id,
            wave.mode,
            wave.active_run_id.as_deref().unwrap_or("none"),
            wave.pending_proposal_count,
            wave.autonomous_action_count,
            yes_no(wave.recovery_required)
        )),
        Line::raw(format!(
            "selected agent={}  last_head={}  last_failure={}",
            selected_orchestrator_agent(snapshot, &app.state)
                .map(|agent| agent.id.as_str())
                .unwrap_or("none"),
            wave.last_head_summary.as_deref().unwrap_or("none"),
            wave.last_autonomous_failure.as_deref().unwrap_or("none")
        )),
    ];
    frame.render_widget(
        Paragraph::new(summary).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Agent workspace"),
        ),
        chunks[0],
    );

    let rows = wave.agents.iter().map(|agent| {
        Row::new(vec![
            Cell::from(agent.id.clone()),
            Cell::from(agent.status.clone()),
            Cell::from(
                agent
                    .merge_state
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
            ),
            Cell::from(
                agent
                    .sandbox_id
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
            ),
            Cell::from(
                agent
                    .recovery_state
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
            ),
            Cell::from(agent.pending_directive_count.to_string()),
            Cell::from(
                agent
                    .heartbeat_age_ms
                    .map(HumanDuration)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
            ),
            Cell::from(
                agent
                    .last_head_action
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
            ),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(18),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Min(24),
        ],
    )
    .header(
        Row::new(vec![
            "Id",
            "State",
            "Merge",
            "Sandbox",
            "Recovery",
            "Directives",
            "Lease",
            "Last head",
        ])
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .row_highlight_style(
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(">> ")
    .block(Block::default().borders(Borders::ALL).title("MAS agents"));
    let mut table_state = TableState::default();
    if !wave.agents.is_empty() {
        table_state.select(Some(
            app.state
                .selected_orchestrator_agent_index
                .min(wave.agents.len().saturating_sub(1)),
        ));
    }
    frame.render_stateful_widget(table, chunks[1], &mut table_state);

    let detail_lines = if let Some(agent) = selected_orchestrator_agent(snapshot, &app.state) {
        let latest_item = latest_run_agent_item(snapshot, wave.wave_id, &agent.id);
        vec![
            Line::raw(format!(
                "agent {} recovery={} lease_age={} last_directive={}",
                agent.id,
                agent.recovery_state.as_deref().unwrap_or("none"),
                agent
                    .heartbeat_age_ms
                    .map(HumanDuration)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                latest_directive_state_for_agent(snapshot, wave.wave_id, &agent.id)
                    .unwrap_or_else(|| "none".to_string())
            )),
            Line::raw(format!(
                "merge={} sandbox={} barriers={}",
                agent.merge_state.as_deref().unwrap_or("none"),
                agent.sandbox_id.as_deref().unwrap_or("none"),
                if agent.barrier_reasons.is_empty() {
                    "none".to_string()
                } else {
                    agent.barrier_reasons.join(" | ")
                }
            )),
            Line::raw(format!(
                "last head={} proof={} runtime_error={}",
                agent.last_head_action.as_deref().unwrap_or("none"),
                latest_item
                    .map(|item| yes_no(item.proof_complete))
                    .unwrap_or("none"),
                latest_item
                    .and_then(|item| item.error.as_deref())
                    .unwrap_or("none")
            )),
        ]
    } else {
        vec![Line::raw("No MAS agent selected.")]
    };
    frame.render_widget(
        Paragraph::new(detail_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Selected agent"),
        ),
        chunks[2],
    );
}

fn draw_queue_tab(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &OperatorSnapshot,
    app: &App,
) {
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
    .row_highlight_style(
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(">> ")
    .block(Block::default().borders(Borders::ALL).title("Queue"));
    let mut table_state = TableState::default();
    table_state.select(selected_queue_wave_index(snapshot, &app.state));
    frame.render_stateful_widget(table, chunks[1], &mut table_state);
}

#[allow(dead_code)]
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
    let control_context_wave_id = control_context_wave_id(&app.state, snapshot);
    let shell_target = current_shell_target(&app.state, snapshot);
    let review_wave_filter = control_review_wave_filter(&app.state, snapshot);
    let selected_review_item = selected_visible_actionable_operator_item(&app.state, snapshot);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(8),
            Constraint::Length(8),
            Constraint::Min(8),
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

    let mut review_items = vec![
        format!(
            "Shell target: {}",
            shell_target_label(&shell_target, snapshot)
        ),
        format!("Follow mode: {}", app.state.follow_mode.label()),
        format!("Focused lane: {}", focus_lane_label(app.state.focus)),
    ];
    review_items.extend(control_status_items(
        snapshot,
        &app.state,
        control_context_wave_id,
    ));
    review_items.extend(manual_close_status_items(
        &app.root,
        &app.config,
        snapshot,
        control_context_wave_id,
    ));
    if let Some((selected_index, actionable_count, item)) =
        selected_visible_actionable_operator_context(&app.state, snapshot)
    {
        review_items.push(format!(
            "Selected review item: {}/{} wave {} {}",
            selected_index + 1,
            actionable_count,
            item.wave_id,
            item.record_id
        ));
    }
    if let Some(pending_action) = app.state.pending_control_action.as_ref() {
        review_items.push(String::new());
        review_items.extend(pending_control_action_lines(pending_action));
    }
    let request_queue = snapshot
        .operator_objects
        .iter()
        .filter(|item| {
            review_wave_filter
                .map(|wave_id| item.wave_id == wave_id)
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();
    if request_queue.is_empty() {
        review_items.push("Request queue: empty".to_string());
    } else {
        review_items.push(format!("Request queue: {} items", request_queue.len()));
        for item in request_queue {
            let is_selected = selected_review_item.is_some_and(|selected| {
                selected.record_id == item.record_id && selected.kind == item.kind
            });
            review_items.extend(operator_object_lines(
                item,
                review_wave_filter.is_none(),
                is_selected,
            ));
        }
    }
    let review_items = review_items
        .into_iter()
        .map(ListItem::new)
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(review_items).block(Block::default().borders(Borders::ALL).title("Review")),
        chunks[1],
    );

    let recovery_items = recovery_status_items(snapshot, review_wave_filter)
        .into_iter()
        .map(ListItem::new)
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(recovery_items).block(Block::default().borders(Borders::ALL).title("Recovery")),
        chunks[2],
    );

    let mut autonomous_items = vec![
        format!(
            "Autonomous waves: {}",
            format_u32_list(&snapshot.panels.orchestrator.autonomous_wave_ids)
        ),
        format!(
            "Pending proposals: {}",
            snapshot.panels.orchestrator.pending_proposal_count
        ),
        format!(
            "Autonomous actions: {}",
            snapshot.panels.orchestrator.autonomous_action_count
        ),
        format!(
            "Head failures: {}",
            snapshot.panels.orchestrator.failed_head_turn_count
        ),
    ];
    if !snapshot
        .panels
        .orchestrator
        .recent_autonomous_actions
        .is_empty()
    {
        autonomous_items.push("Recent autonomous actions:".to_string());
        autonomous_items.extend(
            snapshot
                .panels
                .orchestrator
                .recent_autonomous_actions
                .iter()
                .rev()
                .take(4)
                .map(|action| {
                    format!(
                        "wave {} {} {}",
                        action.wave_id,
                        action
                            .agent_id
                            .as_deref()
                            .map(|agent_id| format!("agent {}", agent_id))
                            .unwrap_or_else(|| "wave".to_string()),
                        action.summary
                    )
                }),
        );
    }
    if !snapshot
        .panels
        .orchestrator
        .recent_autonomous_failures
        .is_empty()
    {
        autonomous_items.push("Recent head failures:".to_string());
        autonomous_items.extend(
            snapshot
                .panels
                .orchestrator
                .recent_autonomous_failures
                .iter()
                .rev()
                .take(4)
                .map(|failure| format!("wave {} {}", failure.wave_id, failure.summary)),
        );
    }
    frame.render_widget(
        List::new(
            autonomous_items
                .into_iter()
                .map(ListItem::new)
                .collect::<Vec<_>>(),
        )
        .block(Block::default().borders(Borders::ALL).title("Autonomy")),
        chunks[3],
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
        chunks[4],
    );
}

#[allow(dead_code)]
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
        Paragraph::new(summary_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Delivery summary"),
        ),
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

fn selected_wave_recovery_lines(snapshot: &OperatorSnapshot, wave_id: u32) -> Vec<Line<'static>> {
    let Some(run) = selected_latest_run(snapshot, Some(wave_id)) else {
        return Vec::new();
    };
    let Some(recovery) = run.mas.as_ref().and_then(|mas| mas.recovery.as_ref()) else {
        return Vec::new();
    };
    let mut lines = vec![
        Line::styled(
            "Recovery",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::BOLD),
        ),
        Line::raw(format!(
            "plan={} status={} causes={}",
            recovery.recovery_plan_id,
            recovery.status,
            if recovery.causes.is_empty() {
                "none".to_string()
            } else {
                recovery.causes.join(", ")
            }
        )),
        Line::raw(format!(
            "affected={} preserved={} required_actions={}",
            if recovery.affected_agent_ids.is_empty() {
                "none".to_string()
            } else {
                recovery.affected_agent_ids.join(", ")
            },
            if recovery.preserved_accepted_agent_ids.is_empty() {
                "none".to_string()
            } else {
                recovery.preserved_accepted_agent_ids.join(", ")
            },
            if recovery.required_actions.is_empty() {
                "none".to_string()
            } else {
                recovery.required_actions.join(", ")
            }
        )),
    ];
    if let Some(detail) = recovery.detail.as_deref() {
        lines.push(Line::raw(format!("detail: {detail}")));
    }
    for action in recovery.recent_actions.iter().rev().take(3) {
        lines.push(Line::raw(format!(
            "recent action: {} {}",
            action.action_kind,
            action.detail.as_deref().unwrap_or("no detail")
        )));
    }
    lines
}

fn recovery_status_items(
    snapshot: &OperatorSnapshot,
    selected_wave_id: Option<u32>,
) -> Vec<String> {
    let mut items = vec![format!(
        "Unresolved recovery plans: {}",
        snapshot.panels.orchestrator.unresolved_recovery_count
    )];
    let wave_ids = if let Some(wave_id) = selected_wave_id {
        vec![wave_id]
    } else {
        snapshot
            .panels
            .orchestrator
            .waves
            .iter()
            .filter(|wave| wave.recovery_required)
            .map(|wave| wave.wave_id)
            .collect::<Vec<_>>()
    };
    if wave_ids.is_empty() {
        items.push("Recovery queue: clear".to_string());
        return items;
    }
    for wave_id in wave_ids {
        let Some(run) = selected_latest_run(snapshot, Some(wave_id)) else {
            continue;
        };
        let Some(recovery) = run.mas.as_ref().and_then(|mas| mas.recovery.as_ref()) else {
            continue;
        };
        items.push(format!(
            "wave {} recovery {} causes={} actions={}",
            wave_id,
            recovery.status,
            if recovery.causes.is_empty() {
                "none".to_string()
            } else {
                recovery.causes.join(", ")
            },
            if recovery.required_actions.is_empty() {
                "none".to_string()
            } else {
                recovery.required_actions.join(", ")
            }
        ));
        if let Some(detail) = recovery.detail.as_deref() {
            items.push(format!("  {detail}"));
        }
    }
    items
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
        if let Some(orchestrator_wave) = snapshot
            .panels
            .orchestrator
            .waves
            .iter()
            .find(|wave| wave.wave_id == wave_id)
        {
            items.push(format!(
                "Head mode: {}  pending proposals={}  auto actions={}",
                orchestrator_wave.mode,
                orchestrator_wave.pending_proposal_count,
                orchestrator_wave.autonomous_action_count
            ));
            if let Some(summary) = orchestrator_wave.last_head_summary.as_deref() {
                items.push(format!("Last head summary: {}", summary));
            }
            if let Some(failure) = orchestrator_wave.last_autonomous_failure.as_deref() {
                items.push(format!("Last head failure: {}", failure));
            }
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
    let selected_action_context = if control_review_wave_filter(state, snapshot).is_none() {
        selected_visible_actionable_operator_context(state, snapshot)
    } else {
        selected_wave_id
            .and_then(|wave_id| selected_actionable_operator_context(state, snapshot, wave_id))
    };
    if let Some((selected_index, actionable_count, item)) = selected_action_context {
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

#[allow(dead_code)]
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
        wave_app_server::OperatorActionableKind::Proposal => "head-proposal",
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
    use ratatui::backend::TestBackend;
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

    fn empty_recovery() -> wave_control_plane::WaveRecoveryState {
        wave_control_plane::WaveRecoveryState::default()
    }

    fn unavailable_runtime(
        runtime: wave_domain::RuntimeId,
        binary: &str,
        detail: &str,
    ) -> wave_runtime::RuntimeAvailability {
        wave_runtime::RuntimeAvailability {
            runtime,
            binary: binary.to_string(),
            available: false,
            detail: detail.to_string(),
            directive_capabilities: wave_runtime::RuntimeDirectiveCapabilities {
                live_injection: false,
                checkpoint_overlay: false,
                ack_support: false,
            },
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

    fn render_buffer(buffer: &ratatui::buffer::Buffer) -> String {
        buffer
            .content
            .chunks(buffer.area.width as usize)
            .map(|row| {
                row.iter()
                    .map(|cell| cell.symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_app(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| draw_ui(frame, app))
            .expect("render operator shell");
        render_buffer(terminal.backend().buffer())
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
    fn narrow_layout_switches_to_single_column_shell() {
        assert_eq!(shell_layout_mode(80), ShellLayoutMode::Narrow);
        assert_eq!(right_panel_ratio(80), (0, 0));
    }

    #[test]
    fn narrow_layout_keeps_transcript_composer_and_dashboard_visible() {
        let snapshot = test_snapshot();
        let mut app = test_app(AppState::default());
        app.snapshot = Some(snapshot);
        let rendered = render_app(&app, 80, 28);

        assert!(rendered.contains("Operator shell"));
        assert!(rendered.contains("Orchestration stack"));
        assert!(rendered.contains("Transcript"));
        assert!(rendered.contains("Composer"));
        assert!(rendered.contains("> "));
        assert!(rendered.contains("Overview"));
        assert!(rendered.contains("Agents"));
        assert!(rendered.contains("Queue"));
        assert!(rendered.contains("Proof"));
        assert!(rendered.contains("Control"));
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
        assert!(
            waiting_rendered.contains(
                "closure reservation: waiting closure work is holding protected capacity"
            )
        );

        let preempted_rendered = execution_lines(&preempted, None)
            .into_iter()
            .flat_map(|line| line.spans.into_iter().map(|span| span.content.into_owned()))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            preempted_rendered
                .contains("preemption: preempted to free closure capacity for wave 20 agent A8")
        );
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
            mas: None,
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
                recovery: empty_recovery(),
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
    fn visible_selection_drives_wave_actions_even_when_shell_target_differs() {
        let mut snapshot = test_snapshot();
        let mut second_wave = snapshot.planning.waves[0].clone();
        second_wave.id = 6;
        second_wave.slug = "wave-06".to_string();
        second_wave.title = "Wave 6".to_string();
        snapshot.planning.waves.push(second_wave);
        snapshot
            .panels
            .queue
            .waves
            .push(wave_app_server::QueuePanelWaveSnapshot {
                id: 6,
                slug: "wave-06".to_string(),
                title: "Wave 6".to_string(),
                queue_state: "ready".to_string(),
                blocked: false,
            });

        let state = AppState {
            selected_wave_index: 1,
            shell_target: Some(ShellTargetState {
                scope: ShellScope::Wave,
                wave_id: Some(5),
                agent_id: None,
            }),
            ..AppState::default()
        };

        assert_eq!(selected_action_wave_id(&state, &snapshot), Some(6));
        assert_eq!(selected_queue_wave_index(&snapshot, &state), Some(1));
    }

    #[test]
    fn follow_mode_tracks_run_and_agent_honestly() {
        let mut snapshot = test_snapshot();
        snapshot.panels.orchestrator.waves = vec![wave_app_server::WaveOrchestratorSnapshot {
            wave_id: 5,
            title: "Wave 5".to_string(),
            execution_model: "multi-agent".to_string(),
            mode: "operator".to_string(),
            active_run_id: Some("wave-05-test".to_string()),
            pending_proposal_count: 0,
            autonomous_action_count: 0,
            recovery_required: false,
            last_head_turn_at_ms: None,
            last_head_summary: None,
            last_autonomous_failure: None,
            agents: vec![
                wave_app_server::MasAgentSnapshot {
                    id: "A1".to_string(),
                    title: "TUI Shell And Layout Scaffold".to_string(),
                    barrier_class: "independent".to_string(),
                    depends_on_agents: Vec::new(),
                    writes_artifacts: Vec::new(),
                    exclusive_resources: Vec::new(),
                    status: "running".to_string(),
                    merge_state: None,
                    sandbox_id: Some("sandbox-a1".to_string()),
                    heartbeat_age_ms: Some(1_000),
                    pending_directive_count: 0,
                    last_head_action: None,
                    recovery_state: None,
                    barrier_reasons: Vec::new(),
                },
                wave_app_server::MasAgentSnapshot {
                    id: "A8".to_string(),
                    title: "Integration Steward".to_string(),
                    barrier_class: "integration-barrier".to_string(),
                    depends_on_agents: vec!["A1".to_string()],
                    writes_artifacts: Vec::new(),
                    exclusive_resources: Vec::new(),
                    status: "planned".to_string(),
                    merge_state: None,
                    sandbox_id: Some("sandbox-a8".to_string()),
                    heartbeat_age_ms: None,
                    pending_directive_count: 0,
                    last_head_action: None,
                    recovery_state: None,
                    barrier_reasons: vec!["awaiting implementation frontier A1".to_string()],
                },
            ],
        }];

        let mut run_state = AppState {
            follow_mode: FollowMode::Run,
            focus: FocusLane::Dashboard,
            selected_orchestrator_agent_index: 1,
            transcript_scroll: 9,
            ..AppState::default()
        };
        apply_follow_mode(&mut run_state, &snapshot);
        assert_eq!(selected_wave_id(&run_state, &snapshot), Some(5));
        assert_eq!(
            selected_orchestrator_agent(&snapshot, &run_state).map(|agent| agent.id.as_str()),
            Some("A1")
        );
        assert_eq!(run_state.transcript_scroll, 0);

        let mut agent_state = AppState {
            follow_mode: FollowMode::Agent,
            focus: FocusLane::Dashboard,
            selected_orchestrator_agent_index: 1,
            transcript_scroll: 7,
            ..AppState::default()
        };
        apply_follow_mode(&mut agent_state, &snapshot);
        assert_eq!(
            agent_state.shell_target,
            Some(ShellTargetState {
                scope: ShellScope::Agent,
                wave_id: Some(5),
                agent_id: Some("A8".to_string()),
            })
        );
        assert_eq!(agent_state.transcript_scroll, 0);
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

        assert!(
            rendered
                .iter()
                .any(|line| line == "Replay OK for wave 5 run wave-05-test")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "Open questions: question-api-shape")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "Open assumptions: assumption-cache-valid")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "Invalidated facts: fact-api")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "Superseded decisions: decision-api-v1")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "Ambiguous dependencies: 4")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "Selected operator action: 1/2")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "Waiting on: operator dependency approval")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "Next operator action: press u to approve or x to reject")
        );
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

        assert!(
            rendered
                .iter()
                .any(|line| line == "selected runtimes: claude, codex")
        );
        assert!(rendered.iter().any(|line| line
            == "current agent runtime: requested codex -> selected claude via executor.id"));
        assert!(
            rendered
                .iter()
                .all(|line| !line.starts_with("runtime decision:"))
        );
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

        assert!(
            rendered
                .iter()
                .any(|line| line == "Run runtimes: claude, codex")
        );
        assert!(rendered.iter().any(|line| line
            == "Current agent runtime: requested codex -> selected claude via executor.id"));
        assert!(
            rendered
                .iter()
                .all(|line| !line.starts_with("Run runtime:"))
        );
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
        assert!(
            rendered
                .iter()
                .any(|line| line == "promotion promotion-blocked:merge-conflict")
        );
        assert!(rendered
            .iter()
            .any(|line| line == "promotion merge-blocked  promotion blocked by merge conflicts"));
        assert!(
            rendered
                .iter()
                .any(|line| line == "promotion conflict-paths=crates/wave-tui/src/lib.rs")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "promotion closure-blocked  waiting for promotion to clear")
        );
        assert!(
            rendered
                .iter()
                .any(|line| { line == "acceptance  implementation proof is only 2/6 complete" })
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "risk  agent-error  agent A6 failed")
        );
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

        assert!(
            rendered
                .iter()
                .any(|line| line == "question  wave 5  question-api-shape")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "assumption  wave 5  assumption-cache-valid")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "invalidated-fact  wave 5  fact-api")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "invalidated-decision  wave 5  decision-api-shape")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "superseded-decision  wave 5  decision-api-v1")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "dependency-ambiguity  wave 5  wave-4")
        );
        assert!(rendered.iter().any(|line| {
            line == "manual-close-override  wave 15  manual close accepted  state=applied"
        }));
        assert!(
            rendered
                .iter()
                .any(|line| line
                    == "  source_run=wave-15-failed  evidence=1  waiting_on=manual close override is active  next_action=press M to clear")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "  detail=promotion conflict reviewed")
        );
        assert!(rendered.iter().any(|line| {
            line == "approval-request  wave 5  Need dependency confirmation  state=pending"
        }));
        assert!(
            rendered
                .iter()
                .any(|line| line
                    == "  route=dependency:wave-04  task=wave-05:agent-a1  waiting_on=operator dependency approval  next_action=press u to approve or x to reject")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "escalation  wave 5  Need operator review  state=open")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line
                    == "  route=dependency:wave-04  task=wave-05:agent-a6  evidence=1  waiting_on=operator escalation review  next_action=press u to acknowledge or x to dismiss")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "  detail=escalated from design review")
        );
        assert!(rendered.iter().any(|line| {
            line
                == "contradiction  wave 5  contradiction-5  API shape contradicts dependency result  state=detected"
        }));
        assert!(
            rendered
                .iter()
                .any(|line| { line == "  invalidates=fact:fact-api, decision:decision-api-shape" })
        );
        assert!(rendered.iter().any(|line| {
            line
                == "invalidation  wave 5  contradiction contradiction-5 invalidates fact fact-api -> decision decision-api-shape"
        }));
    }

    #[test]
    fn control_actions_include_manual_close_shortcuts() {
        let snapshot = test_snapshot();

        assert!(
            snapshot
                .panels
                .control
                .actions
                .iter()
                .any(|action| action.key == "m" && action.label == "Apply manual close")
        );
        assert!(
            snapshot
                .panels
                .control
                .actions
                .iter()
                .any(|action| action.key == "M" && action.label == "Clear manual close")
        );
        assert!(
            snapshot
                .panels
                .control
                .actions
                .iter()
                .any(|action| action.key == "[ / ]" && action.label == "Select action")
        );
        assert!(
            snapshot
                .panels
                .control
                .actions
                .iter()
                .any(|action| action.key == "u" && action.label == "Approve action")
        );
        assert!(
            snapshot
                .panels
                .control
                .actions
                .iter()
                .any(|action| action.key == "x" && action.label == "Reject or dismiss")
        );
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

        assert!(
            rendered
                .iter()
                .any(|line| line == "Source run: wave-15-failed")
        );
        assert!(rendered.iter().any(|line| line == "Evidence files: 1"));
        assert!(
            rendered
                .iter()
                .any(|line| line == "Enter apply  Esc cancel")
        );
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

        assert!(
            rendered
                .iter()
                .any(|line| line == "Approve approval-request human-5")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "Waiting on: operator dependency approval")
        );
        assert!(
            rendered
                .iter()
                .any(|line| line == "Enter approve  Esc cancel")
        );
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
    fn app_state_defaults_to_dashboard_focus() {
        assert_eq!(AppState::default().focus, FocusLane::Dashboard);
    }

    #[test]
    fn repo_head_review_queue_actions_follow_visible_selected_row() {
        let mut snapshot = test_snapshot();
        let mut wave_15 = snapshot.planning.waves[0].clone();
        wave_15.id = 15;
        wave_15.slug = "manual-close".to_string();
        wave_15.title = "Close earlier waves honestly with manual close override".to_string();
        wave_15.last_run_status = Some(WaveRunStatus::Failed);
        wave_15.completed = true;
        snapshot.planning.waves.push(wave_15);
        snapshot
            .operator_objects
            .push(wave_app_server::OperatorActionableItem {
                kind: wave_app_server::OperatorActionableKind::Proposal,
                wave_id: 15,
                record_id: "proposal-15".to_string(),
                state: "pending".to_string(),
                summary: "Propose recovery".to_string(),
                detail: Some("repair the failed promotion".to_string()),
                waiting_on: Some("operator proposal review".to_string()),
                next_action: Some("press u to apply or x to dismiss".to_string()),
                route: None,
                task_id: None,
                source_run_id: Some("wave-15-failed".to_string()),
                evidence_count: 1,
                created_at_ms: Some(4),
            });

        let mut app = test_app(AppState {
            shell_target: Some(ShellTargetState {
                scope: ShellScope::Head,
                wave_id: None,
                agent_id: None,
            }),
            selected_operator_action_index: 2,
            ..AppState::default()
        });
        app.snapshot = Some(snapshot);

        handle_prepare_operator_action(&mut app, true).expect("prepare operator action");

        assert_eq!(
            app.state.pending_control_action,
            Some(PendingControlAction::ApproveOperatorAction(
                OperatorActionConfirmation {
                    wave_id: 15,
                    wave_title: "Close earlier waves honestly with manual close override"
                        .to_string(),
                    record_id: "proposal-15".to_string(),
                    kind: wave_app_server::OperatorActionableKind::Proposal,
                    summary: "Propose recovery".to_string(),
                    waiting_on: Some("operator proposal review".to_string()),
                    next_action: Some("press u to apply or x to dismiss".to_string()),
                }
            ))
        );
    }

    #[test]
    fn repo_head_control_context_follows_visible_selected_review_row_wave() {
        let mut snapshot = test_snapshot();
        let mut wave_15 = snapshot.planning.waves[0].clone();
        wave_15.id = 15;
        wave_15.slug = "manual-close".to_string();
        wave_15.title = "Close earlier waves honestly with manual close override".to_string();
        wave_15.last_run_status = Some(WaveRunStatus::Failed);
        wave_15.completed = true;
        snapshot.planning.waves.push(wave_15);
        snapshot
            .operator_objects
            .push(wave_app_server::OperatorActionableItem {
                kind: wave_app_server::OperatorActionableKind::Proposal,
                wave_id: 15,
                record_id: "proposal-15".to_string(),
                state: "pending".to_string(),
                summary: "Propose recovery".to_string(),
                detail: Some("repair the failed promotion".to_string()),
                waiting_on: Some("operator proposal review".to_string()),
                next_action: Some("press u to apply or x to dismiss".to_string()),
                route: None,
                task_id: None,
                source_run_id: Some("wave-15-failed".to_string()),
                evidence_count: 1,
                created_at_ms: Some(4),
            });

        let state = AppState {
            shell_target: Some(ShellTargetState {
                scope: ShellScope::Head,
                wave_id: None,
                agent_id: None,
            }),
            selected_operator_action_index: 2,
            ..AppState::default()
        };

        assert_eq!(control_context_wave_id(&state, &snapshot), Some(15));
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
            snapshot: None,
            snapshot_error: None,
            snapshot_receiver: None,
            refresh_in_flight: false,
            last_refresh_started_at: None,
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
        use wave_app_server::OperatorShellCommand;
        use wave_app_server::OperatorShellSnapshot;
        use wave_app_server::OperatorShellTargetSnapshot;
        use wave_app_server::OperatorShellTranscriptItem;
        use wave_app_server::OrchestratorPanelSnapshot;
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
            mas: None,
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
                    recovery: empty_recovery(),
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
                        recovery: 0,
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
                        recovery: 0,
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
                orchestrator: OrchestratorPanelSnapshot {
                    mode: "operator".to_string(),
                    active: true,
                    multi_agent_wave_count: 0,
                    selected_wave_id: None,
                    autonomous_wave_ids: Vec::new(),
                    pending_proposal_count: 0,
                    autonomous_action_count: 0,
                    failed_head_turn_count: 0,
                    unresolved_recovery_count: 0,
                    recent_autonomous_actions: Vec::new(),
                    recent_autonomous_failures: Vec::new(),
                    directives: Vec::new(),
                    waves: Vec::new(),
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
                    unavailable_runtime(
                        wave_domain::RuntimeId::Codex,
                        "codex",
                        "codex login status reported unavailable",
                    ),
                    unavailable_runtime(
                        wave_domain::RuntimeId::Claude,
                        "claude",
                        "claude auth status --json reported unavailable",
                    ),
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
            shell: OperatorShellSnapshot {
                default_target: OperatorShellTargetSnapshot {
                    scope: "head".to_string(),
                    wave_id: Some(5),
                    agent_id: None,
                    label: "Head / Wave 5".to_string(),
                    summary: "default operator shell target for wave 5".to_string(),
                },
                session: None,
                transcript: vec![
                    OperatorShellTranscriptItem {
                        item_id: "run-wave-05-test".to_string(),
                        kind: "run".to_string(),
                        title: "Wave 5 running".to_string(),
                        detail: "agent A1 TUI Shell And Layout Scaffold | proof 1/6 complete=false | No live agent output yet.".to_string(),
                        origin: Some("system".to_string()),
                        wave_id: Some(5),
                        agent_id: Some("A1".to_string()),
                        session_id: None,
                        turn_id: None,
                        proposal_id: None,
                        created_at_ms: 2,
                        status: Some("running".to_string()),
                    },
                    OperatorShellTranscriptItem {
                        item_id: "human-5".to_string(),
                        kind: "approval".to_string(),
                        title: "Need dependency confirmation".to_string(),
                        detail: "operator dependency approval | press u to approve or x to reject".to_string(),
                        origin: Some("system".to_string()),
                        wave_id: Some(5),
                        agent_id: None,
                        session_id: None,
                        turn_id: None,
                        proposal_id: None,
                        created_at_ms: 3,
                        status: Some("pending".to_string()),
                    },
                ],
                proposals: Vec::new(),
                command_availability: std::collections::BTreeMap::new(),
                commands: vec![
                    OperatorShellCommand {
                        name: "/wave".to_string(),
                        usage: "/wave <id>".to_string(),
                        summary: "retarget the shell to a wave".to_string(),
                    },
                    OperatorShellCommand {
                        name: "/help".to_string(),
                        usage: "/help".to_string(),
                        summary: "show shell commands".to_string(),
                    },
                ],
                last_event_at_ms: Some(3),
            },
        }
    }
}
