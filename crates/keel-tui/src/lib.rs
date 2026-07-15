//! The keel cockpit: a k9s-style, keyboard-first ratatui dashboard.
//!
//! Views: stacks -> fleet grid -> repo detail, changesets -> changeset grid,
//! tree, help overlay. `/` filters the grid, `:` opens a command bar whose
//! verbs mirror the CLI. Actions run on a worker thread so the UI never
//! freezes; a spinner shows progress.
//!
//! All domain work goes through the [`Controller`] trait — this crate renders
//! and dispatches, nothing more.

use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use keel_core::workspace::RepoStatus;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

/// One repo of a changeset, with its rendered PR/CI cells.
#[derive(Debug, Clone)]
pub struct ChangeRepoRow {
    pub name: String,
    pub branch: String,
    pub on_branch: bool,
    pub dirty: bool,
    pub head: Option<String>,
    /// Rendered PR/MR cell (`#128 ● open`), `—` before `change request`.
    pub pr: String,
    /// Rendered CI cell (`✓ passed`, `⏳ running`, `—`).
    pub ci: String,
}

/// One changeset and its repos.
#[derive(Debug, Clone)]
pub struct ChangesetSummary {
    pub id: String,
    pub repos: Vec<ChangeRepoRow>,
}

/// Full data refresh for the cockpit.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub root_label: String,
    pub stacks: Vec<String>,
    pub current_stack: Option<String>,
    /// stack -> repo statuses.
    pub fleet: Vec<(String, Vec<RepoStatus>)>,
    pub changesets: Vec<ChangesetSummary>,
    pub lock_present: bool,
    /// repo name -> absolute checkout path (for goto).
    pub paths: Vec<(String, PathBuf)>,
    /// Rendered `keel tree` output for the tree view.
    pub tree: String,
}

/// Everything the cockpit can ask the application to do. Implementations run
/// on a worker thread, so they must be `Send`.
pub trait Controller: Send {
    fn snapshot(&mut self) -> io::Result<Snapshot>;
    /// PR/CI cells for one changeset (network; fetched on drill-in).
    fn changeset_prs(&mut self, id: &str) -> io::Result<ChangesetSummary>;
    fn sync_stack(&mut self, stack: &str) -> io::Result<String>;
    fn sync_repo(&mut self, repo: &str) -> io::Result<String>;
    fn switch(&mut self, stack: &str) -> io::Result<String>;
    fn pin(&mut self) -> io::Result<String>;
    fn lock(&mut self) -> io::Result<String>;
    fn run_cmd(&mut self, cmd: &str) -> io::Result<String>;
    fn change_start(&mut self, id: &str) -> io::Result<String>;
    fn change_request(&mut self, id: &str, only: Option<Vec<String>>) -> io::Result<String>;
    fn change_land(&mut self, id: &str) -> io::Result<String>;
}

const SPINNER: [&str; 4] = ["◐", "◓", "◑", "◒"];

#[derive(Debug, Clone, Copy, PartialEq)]
enum View {
    Stacks,
    Fleet,
    Changesets,
    Changeset,
    Tree,
}

#[derive(Debug, Clone, PartialEq)]
enum InputMode {
    None,
    Filter(String),
    Command(String),
    NewChangeset(String),
}

enum Job {
    Refresh,
    ChangesetPrs(String),
    Action(&'static str, ActionKind),
}

enum ActionKind {
    SyncStack(String),
    SyncRepo(String),
    Switch(String),
    Pin,
    Lock,
    Run(String),
    ChangeStart(String),
    ChangeRequest(String, Option<Vec<String>>),
    ChangeLand(String),
}

enum Outcome {
    Snapshot(Box<io::Result<Snapshot>>),
    ChangesetPrs(Box<io::Result<ChangesetSummary>>),
    Action(&'static str, io::Result<String>),
}

struct App {
    view: View,
    back: Vec<View>,
    snapshot: Snapshot,
    stack: Option<String>,
    changeset: Option<String>,
    selected_repos: Vec<String>,
    cursor: ListState,
    input: InputMode,
    filter: String,
    message: String,
    busy: Option<&'static str>,
    spinner: usize,
    help: bool,
    goto: Option<PathBuf>,
}

impl App {
    fn fleet_rows(&self) -> Vec<&RepoStatus> {
        let stack = self.stack.as_deref().unwrap_or_default();
        self.snapshot
            .fleet
            .iter()
            .find(|(name, _)| name == stack)
            .map(|(_, repos)| {
                repos
                    .iter()
                    .filter(|r| self.filter.is_empty() || r.name.contains(&self.filter))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn stack_rows(&self) -> Vec<&str> {
        self.snapshot
            .stacks
            .iter()
            .map(String::as_str)
            .filter(|s| self.filter.is_empty() || s.contains(&self.filter))
            .collect()
    }

    fn changeset_rows(&self) -> Vec<&ChangesetSummary> {
        self.snapshot
            .changesets
            .iter()
            .filter(|c| self.filter.is_empty() || c.id.contains(&self.filter))
            .collect()
    }

    fn current_changeset(&self) -> Option<&ChangesetSummary> {
        let id = self.changeset.as_deref()?;
        self.snapshot.changesets.iter().find(|c| c.id == id)
    }

    fn change_repo_rows(&self) -> Vec<&ChangeRepoRow> {
        self.current_changeset()
            .map(|c| {
                c.repos
                    .iter()
                    .filter(|r| self.filter.is_empty() || r.name.contains(&self.filter))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn rows_len(&self) -> usize {
        match self.view {
            View::Stacks => self.stack_rows().len(),
            View::Fleet => self.fleet_rows().len(),
            View::Changesets => self.changeset_rows().len(),
            View::Changeset => self.change_repo_rows().len(),
            View::Tree => 0,
        }
    }

    fn cursor_repo(&self) -> Option<String> {
        let index = self.cursor.selected()?;
        match self.view {
            View::Fleet => self.fleet_rows().get(index).map(|r| r.name.clone()),
            View::Changeset => self.change_repo_rows().get(index).map(|r| r.name.clone()),
            _ => None,
        }
    }

    fn repo_path(&self, repo: &str) -> Option<PathBuf> {
        self.snapshot
            .paths
            .iter()
            .find(|(name, _)| name == repo)
            .map(|(_, path)| path.clone())
    }

    fn clamp_cursor(&mut self) {
        let last = self.rows_len().saturating_sub(1);
        self.cursor
            .select(Some(self.cursor.selected().unwrap_or(0).min(last)));
    }

    fn goto_view(&mut self, view: View) {
        if self.view != view {
            self.back.push(self.view);
            self.view = view;
            self.cursor.select(Some(0));
            self.filter.clear();
        }
    }

    fn go_back(&mut self) {
        if let Some(previous) = self.back.pop() {
            self.view = previous;
            self.filter.clear();
            self.clamp_cursor();
        }
    }
}

/// Run the cockpit until quit. Returns a path when the user asked to `goto`
/// a repo, so the caller can print it (`cd "$(keel dash)"`).
pub fn run(controller: Box<dyn Controller>) -> io::Result<Option<PathBuf>> {
    let (job_tx, job_rx) = channel::<Job>();
    let (out_tx, out_rx) = channel::<Outcome>();
    spawn_worker(controller, job_rx, out_tx);

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &job_tx, &out_rx);
    ratatui::restore();
    result
}

fn spawn_worker(controller: Box<dyn Controller>, jobs: Receiver<Job>, outcomes: Sender<Outcome>) {
    std::thread::spawn(move || {
        let mut controller = controller;
        while let Ok(job) = jobs.recv() {
            let outcome = match job {
                Job::Refresh => Outcome::Snapshot(Box::new(controller.snapshot())),
                Job::ChangesetPrs(id) => {
                    Outcome::ChangesetPrs(Box::new(controller.changeset_prs(&id)))
                }
                Job::Action(label, kind) => {
                    let result = match kind {
                        ActionKind::SyncStack(stack) => controller.sync_stack(&stack),
                        ActionKind::SyncRepo(repo) => controller.sync_repo(&repo),
                        ActionKind::Switch(stack) => controller.switch(&stack),
                        ActionKind::Pin => controller.pin(),
                        ActionKind::Lock => controller.lock(),
                        ActionKind::Run(cmd) => controller.run_cmd(&cmd),
                        ActionKind::ChangeStart(id) => controller.change_start(&id),
                        ActionKind::ChangeRequest(id, only) => controller.change_request(&id, only),
                        ActionKind::ChangeLand(id) => controller.change_land(&id),
                    };
                    Outcome::Action(label, result)
                }
            };
            if outcomes.send(outcome).is_err() {
                return;
            }
        }
    });
}

fn dispatch(app: &mut App, jobs: &Sender<Job>, label: &'static str, kind: ActionKind) {
    if app.busy.is_some() {
        app.message = "busy — wait for the current operation".to_string();
        return;
    }
    app.busy = Some(label);
    let _ = jobs.send(Job::Action(label, kind));
}

fn request_refresh(app: &mut App, jobs: &Sender<Job>) {
    if app.busy.is_none() {
        app.busy = Some("refresh");
        let _ = jobs.send(Job::Refresh);
    }
}

fn run_command_bar(app: &mut App, jobs: &Sender<Job>, line: &str) {
    let (verb, rest) = line
        .trim()
        .split_once(' ')
        .map_or((line.trim(), ""), |(v, r)| (v, r.trim()));
    match (verb, rest) {
        ("sync", "") => {
            if let Some(stack) = app.stack.clone() {
                app.message = format!("→ keel sync --stack {stack}");
                dispatch(app, jobs, "sync", ActionKind::SyncStack(stack));
            }
        }
        ("stack" | "switch", name) if !name.is_empty() => {
            app.message = format!("→ keel switch {name}");
            dispatch(app, jobs, "switch", ActionKind::Switch(name.to_string()));
        }
        ("run", cmd) if !cmd.is_empty() => {
            app.message = format!("→ keel run '{cmd}'");
            dispatch(app, jobs, "run", ActionKind::Run(cmd.to_string()));
        }
        ("change", id) if !id.is_empty() => {
            app.message = format!("→ keel change start {id}");
            dispatch(
                app,
                jobs,
                "change start",
                ActionKind::ChangeStart(id.to_string()),
            );
        }
        ("pin", "") => {
            app.message = "→ keel pin".to_string();
            dispatch(app, jobs, "pin", ActionKind::Pin);
        }
        ("lock", "") => {
            app.message = "→ keel lock".to_string();
            dispatch(app, jobs, "lock", ActionKind::Lock);
        }
        ("tree", "") => app.goto_view(View::Tree),
        ("q" | "quit", _) => app.message = "use q outside the command bar".to_string(),
        _ => app.message = format!("unknown command `{line}`"),
    }
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    jobs: &Sender<Job>,
    outcomes: &Receiver<Outcome>,
) -> io::Result<Option<PathBuf>> {
    let mut app = App {
        view: View::Fleet,
        back: Vec::new(),
        snapshot: Snapshot::default(),
        stack: None,
        changeset: None,
        selected_repos: Vec::new(),
        cursor: ListState::default(),
        input: InputMode::None,
        filter: String::new(),
        message: "loading…".to_string(),
        busy: None,
        spinner: 0,
        help: false,
        goto: None,
    };
    app.cursor.select(Some(0));
    request_refresh(&mut app, jobs);

    loop {
        while let Ok(outcome) = outcomes.try_recv() {
            match outcome {
                Outcome::Snapshot(result) => {
                    app.busy = None;
                    match *result {
                        Ok(snapshot) => {
                            if app.stack.is_none() {
                                app.stack = snapshot
                                    .current_stack
                                    .clone()
                                    .or_else(|| snapshot.stacks.first().cloned());
                            }
                            app.snapshot = snapshot;
                            app.clamp_cursor();
                            if app.message == "loading…" {
                                app.message =
                                    "[s]ync [S]witch [p]in [l]ock [t]ree [c]hange [?]help"
                                        .to_string();
                            }
                        }
                        Err(err) => app.message = format!("refresh failed: {err}"),
                    }
                }
                Outcome::ChangesetPrs(result) => {
                    app.busy = None;
                    match *result {
                        Ok(summary) => {
                            if let Some(slot) = app
                                .snapshot
                                .changesets
                                .iter_mut()
                                .find(|c| c.id == summary.id)
                            {
                                *slot = summary;
                            }
                            app.message = "PR/MR status refreshed".to_string();
                        }
                        Err(err) => app.message = format!("PR status failed: {err}"),
                    }
                }
                Outcome::Action(label, result) => {
                    app.busy = None;
                    match result {
                        Ok(message) => app.message = message,
                        Err(err) => app.message = format!("{label} failed: {err}"),
                    }
                    request_refresh(&mut app, jobs);
                }
            }
        }

        if app.busy.is_some() {
            app.spinner = (app.spinner + 1) % SPINNER.len();
        }
        terminal.draw(|frame| draw(frame, &mut app))?;

        if !event::poll(Duration::from_millis(120))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(app.goto);
        }

        if app.help {
            app.help = false;
            continue;
        }

        match &mut app.input {
            InputMode::Filter(buffer)
            | InputMode::Command(buffer)
            | InputMode::NewChangeset(buffer) => {
                match key.code {
                    KeyCode::Esc => app.input = InputMode::None,
                    KeyCode::Backspace => {
                        buffer.pop();
                        if let InputMode::Filter(b) = &app.input {
                            app.filter = b.clone();
                            app.clamp_cursor();
                        }
                    }
                    KeyCode::Char(c) => {
                        buffer.push(c);
                        if let InputMode::Filter(b) = &app.input {
                            app.filter = b.clone();
                            app.clamp_cursor();
                        }
                    }
                    KeyCode::Enter => {
                        let mode = std::mem::replace(&mut app.input, InputMode::None);
                        match mode {
                            InputMode::Filter(_) => {}
                            InputMode::Command(line) => run_command_bar(&mut app, jobs, &line),
                            InputMode::NewChangeset(id) => {
                                let id = id.trim().to_string();
                                if !id.is_empty() {
                                    app.message = format!("→ keel change start {id}");
                                    dispatch(
                                        &mut app,
                                        jobs,
                                        "change start",
                                        ActionKind::ChangeStart(id),
                                    );
                                }
                            }
                            InputMode::None => {}
                        }
                    }
                    _ => {}
                }
                continue;
            }
            InputMode::None => {}
        }

        let selected = app.cursor.selected().unwrap_or(0);
        match key.code {
            KeyCode::Char('q') => return Ok(app.goto),
            KeyCode::Char('?') => app.help = true,
            KeyCode::Char('/') => app.input = InputMode::Filter(String::new()),
            KeyCode::Char(':') => app.input = InputMode::Command(String::new()),
            KeyCode::Esc | KeyCode::Char('b') => {
                if !app.filter.is_empty() {
                    app.filter.clear();
                } else {
                    app.go_back();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.cursor
                    .select(Some((selected + 1).min(app.rows_len().saturating_sub(1))));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.cursor.select(Some(selected.saturating_sub(1)));
            }
            KeyCode::Char('t') => app.goto_view(View::Tree),
            KeyCode::Char('c') => app.goto_view(View::Changesets),
            KeyCode::Char('g') => {
                if let Some(repo) = app.cursor_repo()
                    && let Some(path) = app.repo_path(&repo)
                {
                    app.goto = Some(path);
                    return Ok(app.goto);
                }
                app.message = "goto: put the cursor on a repo row".to_string();
            }
            KeyCode::Enter => match app.view {
                View::Stacks => {
                    if let Some(stack) = app.stack_rows().get(selected).map(|s| s.to_string()) {
                        app.stack = Some(stack);
                        app.goto_view(View::Fleet);
                    }
                }
                View::Changesets => {
                    if let Some(id) = app.changeset_rows().get(selected).map(|c| c.id.clone()) {
                        app.changeset = Some(id.clone());
                        app.selected_repos.clear();
                        app.goto_view(View::Changeset);
                        if app.busy.is_none() {
                            app.busy = Some("PR status");
                            let _ = jobs.send(Job::ChangesetPrs(id));
                        }
                    }
                }
                _ => {}
            },
            KeyCode::Char('s') if app.view == View::Fleet => {
                if let Some(repo) = app.cursor_repo() {
                    app.message = format!("→ keel sync ({repo})");
                    dispatch(&mut app, jobs, "sync", ActionKind::SyncRepo(repo));
                } else if let Some(stack) = app.stack.clone() {
                    app.message = format!("→ keel sync --stack {stack}");
                    dispatch(&mut app, jobs, "sync", ActionKind::SyncStack(stack));
                }
            }
            KeyCode::Char('s') if app.view == View::Stacks => {
                if let Some(stack) = app.stack_rows().get(selected).map(|s| s.to_string()) {
                    app.message = format!("→ keel sync --stack {stack}");
                    dispatch(&mut app, jobs, "sync", ActionKind::SyncStack(stack));
                }
            }
            KeyCode::Char('S') => {
                let target = match app.view {
                    View::Stacks => app.stack_rows().get(selected).map(|s| s.to_string()),
                    _ => None,
                };
                match target {
                    Some(stack) => {
                        app.message = format!("→ keel switch {stack}");
                        app.stack = Some(stack.clone());
                        dispatch(&mut app, jobs, "switch", ActionKind::Switch(stack));
                    }
                    None => app.goto_view(View::Stacks),
                }
            }
            KeyCode::Char('p') if app.view == View::Fleet || app.view == View::Stacks => {
                app.message = "→ keel pin".to_string();
                dispatch(&mut app, jobs, "pin", ActionKind::Pin);
            }
            KeyCode::Char('l') if app.view == View::Fleet || app.view == View::Stacks => {
                app.message = "→ keel lock".to_string();
                dispatch(&mut app, jobs, "lock", ActionKind::Lock);
            }
            KeyCode::Char('r') => {
                app.input = InputMode::Command("run ".to_string());
            }
            KeyCode::Char('n') if app.view == View::Changesets || app.view == View::Changeset => {
                app.input = InputMode::NewChangeset(String::new());
            }
            KeyCode::Char(' ') if app.view == View::Changeset => {
                if let Some(repo) = app.cursor_repo() {
                    if let Some(found) = app.selected_repos.iter().position(|r| r == &repo) {
                        app.selected_repos.remove(found);
                    } else {
                        app.selected_repos.push(repo);
                    }
                }
            }
            KeyCode::Char('R') if app.view == View::Changeset => {
                if let Some(id) = app.changeset.clone() {
                    let only = if app.selected_repos.is_empty() {
                        None
                    } else {
                        Some(app.selected_repos.clone())
                    };
                    app.message = format!("→ keel change request {id}");
                    dispatch(
                        &mut app,
                        jobs,
                        "change request",
                        ActionKind::ChangeRequest(id, only),
                    );
                }
            }
            KeyCode::Char('L') if app.view == View::Changeset => {
                if let Some(id) = app.changeset.clone() {
                    app.message = format!("→ keel change land {id}");
                    dispatch(&mut app, jobs, "change land", ActionKind::ChangeLand(id));
                }
            }
            _ => {}
        }
    }
}

fn header_line(app: &App) -> String {
    let context = match app.view {
        View::Stacks => "stacks".to_string(),
        View::Fleet => format!(
            "stack: {}   lock: {}   repos: {}",
            app.stack.as_deref().unwrap_or("—"),
            if app.snapshot.lock_present {
                "✓"
            } else {
                "✗"
            },
            app.fleet_rows().len()
        ),
        View::Changesets => format!("changesets: {}", app.changeset_rows().len()),
        View::Changeset => format!(
            "change {}   {} repos",
            app.changeset.as_deref().unwrap_or("—"),
            app.change_repo_rows().len()
        ),
        View::Tree => "tree".to_string(),
    };
    format!(" keel ▸ {} ── {}", app.snapshot.root_label, context)
}

fn action_bar(app: &App) -> String {
    let keys = match app.view {
        View::Stacks => "[enter]open [s]ync [S]witch [p]in [l]ock [t]ree [c]hange",
        View::Fleet => "[s]ync [S]tacks [p]in [l]ock [t]ree [c]hange [r]un [g]oto",
        View::Changesets => "[enter]open [n]ew [b]ack",
        View::Changeset => "[n]ew [␣]select [R]equest-PR [L]and [g]oto [b]ack",
        View::Tree => "[b]ack",
    };
    format!("{keys}  [/]filter [:]cmd [?]help [q]uit")
}

fn draw(frame: &mut Frame, app: &mut App) {
    let zones = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(frame.area());

    frame.render_widget(
        Paragraph::new(Line::styled(
            header_line(app),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        zones[0],
    );

    match app.view {
        View::Stacks => draw_stacks(frame, app, zones[1]),
        View::Fleet => draw_fleet(frame, app, zones[1]),
        View::Changesets => draw_changesets(frame, app, zones[1]),
        View::Changeset => draw_changeset(frame, app, zones[1]),
        View::Tree => draw_tree(frame, app, zones[1]),
    }

    let status = match (&app.input, app.busy) {
        (InputMode::Filter(buffer), _) => format!("/{buffer}▏"),
        (InputMode::Command(buffer), _) => format!(":{buffer}▏"),
        (InputMode::NewChangeset(buffer), _) => format!("new changeset id: {buffer}▏"),
        (InputMode::None, Some(label)) => {
            format!("{} {label}…", SPINNER[app.spinner])
        }
        (InputMode::None, None) => app.message.clone(),
    };
    frame.render_widget(
        Paragraph::new(Line::styled(status, Style::default().fg(Color::Cyan))),
        zones[2],
    );
    frame.render_widget(
        Paragraph::new(Line::styled(
            action_bar(app),
            Style::default().fg(Color::DarkGray),
        )),
        zones[3],
    );

    if app.help {
        draw_help(frame);
    }
}

fn glyph(repo: &RepoStatus) -> (&'static str, Color) {
    if repo.missing {
        ("✗", Color::Red)
    } else if repo.dirty {
        ("!", Color::Yellow)
    } else if repo.drift {
        ("●", Color::Magenta)
    } else {
        ("✓", Color::Green)
    }
}

fn short(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

fn draw_stacks(frame: &mut Frame, app: &mut App, area: Rect) {
    let current = app.stack.clone();
    let items: Vec<ListItem> = app
        .stack_rows()
        .iter()
        .map(|name| {
            let marker = if current.as_deref() == Some(name) {
                "▸"
            } else {
                " "
            };
            ListItem::new(format!("{marker} {name}"))
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" stacks "))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut app.cursor);
}

fn draw_fleet(frame: &mut Frame, app: &mut App, area: Rect) {
    let zones = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let rows = app.fleet_rows();
    let width = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);
    let header = format!(
        " {:<width$}  {:<24} {:<10} {:<6} {:<6} AHEAD/BEHIND",
        "REPO", "BRANCH", "HEAD", "DIRTY", "DRIFT"
    );
    let mut items: Vec<ListItem> = vec![ListItem::new(Line::styled(
        header,
        Style::default().add_modifier(Modifier::UNDERLINED),
    ))];
    items.extend(rows.iter().map(|repo| {
        let (mark, color) = glyph(repo);
        let ahead_behind = repo
            .ahead_behind
            .map_or("—".to_string(), |(a, b)| format!("{a} / {b}"));
        ListItem::new(Line::styled(
            format!(
                "{mark}{:<width$}  {:<24} {:<10} {:<6} {:<6} {}",
                repo.name,
                repo.branch.as_deref().unwrap_or("(detached)"),
                repo.head.as_deref().map_or("—", short),
                if repo.dirty { "yes" } else { "·" },
                if repo.drift { "DRIFT" } else { "·" },
                ahead_behind,
            ),
            Style::default().fg(color),
        ))
    }));

    let mut state = ListState::default();
    state.select(app.cursor.selected().map(|i| i + 1));
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" fleet "))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, zones[0], &mut state);

    let detail = match app
        .cursor
        .selected()
        .and_then(|i| app.fleet_rows().get(i).copied().cloned())
    {
        Some(repo) => format!(
            " {}  ›  path {}   branch {}   dirty {}   locked {}   {}",
            repo.name,
            repo.path.display(),
            repo.branch.as_deref().unwrap_or("(detached)"),
            if repo.dirty { "yes" } else { "no" },
            repo.locked_rev.as_deref().map_or("—", short),
            if repo.missing {
                "NOT CLONED — press s"
            } else if repo.drift {
                "DRIFT (head != lock)"
            } else {
                "in sync"
            },
        ),
        None => " no repos — check keel.toml".to_string(),
    };
    frame.render_widget(
        Paragraph::new(Line::styled(detail, Style::default().fg(Color::Gray))),
        zones[1],
    );
}

fn draw_changesets(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .changeset_rows()
        .iter()
        .map(|c| ListItem::new(format!(" {}  ({} repos)", c.id, c.repos.len())))
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" changesets "))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut app.cursor);
}

fn draw_changeset(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = app.change_repo_rows();
    let width = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);
    let header = format!(
        "  {:<width$}  {:<16} {:<6} {:<6} {:<10} {:<14} CI",
        "REPO", "BRANCH", "ON IT", "DIRTY", "HEAD", "PR / MR"
    );
    let mut items: Vec<ListItem> = vec![ListItem::new(Line::styled(
        header,
        Style::default().add_modifier(Modifier::UNDERLINED),
    ))];
    items.extend(rows.iter().map(|repo| {
        let selected = app.selected_repos.contains(&repo.name);
        ListItem::new(format!(
            "{} {:<width$}  {:<16} {:<6} {:<6} {:<10} {:<14} {}",
            if selected { "◉" } else { "·" },
            repo.name,
            repo.branch,
            if repo.on_branch { "yes" } else { "NO" },
            if repo.dirty { "yes" } else { "·" },
            repo.head.as_deref().map_or("—", short),
            repo.pr,
            repo.ci,
        ))
    }));

    let mut state = ListState::default();
    state.select(app.cursor.selected().map(|i| i + 1));
    let title = format!(" change {} ", app.changeset.as_deref().unwrap_or_default());
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_tree(frame: &mut Frame, app: &mut App, area: Rect) {
    frame.render_widget(
        Paragraph::new(app.snapshot.tree.as_str())
            .block(Block::default().borders(Borders::ALL).title(" tree ")),
        area,
    );
}

fn draw_help(frame: &mut Frame) {
    let area = frame.area();
    let width = area.width.min(64);
    let height = area.height.min(20);
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    let text = [
        "global   j/k move · enter drill in · esc/b back · q quit",
        "         / filter · : command bar · ? this help",
        "",
        "fleet    s sync repo/stack · S stacks · p pin · l lock",
        "         t tree · c changesets · r run · g goto",
        "",
        "change   n new · space select · R request PR/MR",
        "         L land (topological, stops on failure) · g goto",
        "",
        "cmd bar  :sync · :stack NAME · :run CMD · :change ID",
        "         :pin · :lock · :tree",
        "",
        "press any key to close",
    ]
    .map(Line::from)
    .to_vec();
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(" help ")),
        popup,
    );
}
