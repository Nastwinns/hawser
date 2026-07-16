//! haw.dev — the fleet cockpit, rendered in the browser with Ratzilla.
//!
//! A standalone showcase: real ratatui widgets, real Ratzilla DOM backend, and
//! real keyboard interaction. A visitor DRIVES the cockpit — moving a cursor
//! through the fleet, switching between the fleet / PR / CI views — over a set
//! of sample data (a real git backend can't run inside a wasm sandbox). Colors
//! mirror `haw-tui`'s theme.
//!
//! Interactivity is wired via Ratzilla 0.3's `WebRenderer::on_key_event`
//! (`terminal.on_key_event(cb)`), which the `DomBackend` implements. The
//! callback receives `ratzilla::event::KeyEvent { code: KeyCode, .. }`.
//! Shared mutable state lives in `Rc<RefCell<Cockpit>>`: the key callback
//! mutates it, the `draw_web` render closure reads it each frame. A small
//! free-running spinner keeps a subtle ambient animation without driving state.

use std::cell::RefCell;
use std::rc::Rc;

use ratzilla::event::{KeyCode, KeyEvent};
use ratzilla::ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratzilla::ratatui::style::{Modifier, Style};
use ratzilla::ratatui::text::{Line, Span, Text};
use ratzilla::ratatui::widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table};
use ratzilla::ratatui::Terminal;
use ratzilla::{DomBackend, WebRenderer};

mod theme {
    use ratzilla::ratatui::style::Color;

    pub const ACCENT: Color = Color::Rgb(137, 180, 250);
    pub const MAUVE: Color = Color::Rgb(203, 166, 247);
    pub const GREEN: Color = Color::Rgb(166, 227, 161);
    pub const YELLOW: Color = Color::Rgb(249, 226, 175);
    pub const RED: Color = Color::Rgb(243, 139, 168);
    pub const TEAL: Color = Color::Rgb(148, 226, 213);
    pub const TEXT: Color = Color::Rgb(205, 214, 244);
    pub const DIM: Color = Color::Rgb(127, 132, 156);
    pub const SURFACE: Color = Color::Rgb(69, 71, 90);
    pub const SURFACE0: Color = Color::Rgb(49, 50, 68);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RepoState {
    Clean,
    Dirty,
    Drift,
    Missing,
}

struct Repo {
    name: &'static str,
    branch: &'static str,
    head: &'static str,
    state: RepoState,
}

/// The fleet — fixed sample data. The visitor's cursor moves over these rows.
const FLEET: &[(&str, &str, &str, RepoState)] = &[
    ("kernel", "v6.1.2", "a1b2c3d4", RepoState::Clean),
    ("hal", "main", "9f8e7d6c", RepoState::Dirty),
    ("app-mqtt", "release/2.x", "4d5e6f7a", RepoState::Drift),
    ("sensor-fw", "main", "eeff0011", RepoState::Clean),
    ("bootloader", "main", "77aa2200", RepoState::Missing),
];

/// Open cross-repo pull/merge requests.
const PRS: &[(&str, &str, &str, &str)] = &[
    ("#128", "kernel", "FEAT-42: dma ring resize", "approved · CI green"),
    ("!47", "hal", "FEAT-42: gpio mux table", "1 approval · CI green"),
    ("#131", "app-mqtt", "fix: reconnect backoff", "review requested"),
    ("!49", "sensor-fw", "chore: bump toolchain", "CI running"),
];

/// CI pipelines across the fleet.
const CI: &[(&str, &str, &str, &str)] = &[
    ("kernel", "build+test", "passed", "1m42s"),
    ("hal", "build+test", "passed", "0m58s"),
    ("app-mqtt", "build+test", "running", "0m31s"),
    ("sensor-fw", "lint+build", "queued", "—"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Fleet,
    Prs,
    Ci,
}

impl View {
    fn next(self) -> Self {
        match self {
            View::Fleet => View::Prs,
            View::Prs => View::Ci,
            View::Ci => View::Fleet,
        }
    }

    fn title(self) -> &'static str {
        match self {
            View::Fleet => "fleet",
            View::Prs => "pull / merge requests",
            View::Ci => "ci pipelines",
        }
    }

    /// Number of selectable rows in this view.
    fn len(self) -> usize {
        match self {
            View::Fleet => FLEET.len(),
            View::Prs => PRS.len(),
            View::Ci => CI.len(),
        }
    }
}

/// Shared, key-driven state. Mutated by the key callback, read by the render
/// closure. Only `spinner` advances on its own (ambient animation).
struct Cockpit {
    view: View,
    cursor: usize,
    spinner: u32,
}

impl Cockpit {
    fn new() -> Self {
        Cockpit {
            view: View::Fleet,
            cursor: 0,
            spinner: 0,
        }
    }

    fn move_down(&mut self) {
        let len = self.view.len();
        if len > 0 {
            self.cursor = (self.cursor + 1) % len;
        }
    }

    fn move_up(&mut self) {
        let len = self.view.len();
        if len > 0 {
            self.cursor = (self.cursor + len - 1) % len;
        }
    }

    fn cycle_view(&mut self) {
        self.view = self.view.next();
        // Keep the cursor in range for the new view.
        let len = self.view.len();
        if len > 0 && self.cursor >= len {
            self.cursor = len - 1;
        }
    }
}

fn fleet_repos() -> Vec<Repo> {
    FLEET
        .iter()
        .map(|(name, branch, head, state)| Repo {
            name,
            branch,
            head,
            state: *state,
        })
        .collect()
}

fn state_dot(state: RepoState) -> Span<'static> {
    match state {
        RepoState::Clean => Span::styled("●", Style::default().fg(theme::GREEN)),
        RepoState::Dirty => Span::styled("●", Style::default().fg(theme::YELLOW)),
        RepoState::Drift => Span::styled("●", Style::default().fg(theme::RED)),
        RepoState::Missing => Span::styled("○", Style::default().fg(theme::DIM)),
    }
}

fn state_cells(state: RepoState) -> (Span<'static>, Span<'static>) {
    let (dirty, drift) = match state {
        RepoState::Clean => ("·", "·"),
        RepoState::Dirty => ("yes", "·"),
        RepoState::Drift => ("·", "DRIFT"),
        RepoState::Missing => ("—", "—"),
    };
    let dirty_color = if dirty == "yes" {
        theme::YELLOW
    } else {
        theme::DIM
    };
    let drift_color = if drift == "DRIFT" {
        theme::RED
    } else {
        theme::DIM
    };
    (
        Span::styled(dirty, Style::default().fg(dirty_color)),
        Span::styled(drift, Style::default().fg(drift_color)),
    )
}

fn panel(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::SURFACE))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ))
}

fn header_cell(label: &str) -> Cell<'static> {
    Cell::from(Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
}

/// Style for the currently-selected row's leading label.
fn cursor_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD)
            .bg(theme::SURFACE0)
    } else {
        Style::default().fg(theme::TEXT)
    }
}

fn draw(frame: &mut ratzilla::ratatui::Frame, cockpit: &Cockpit) {
    let zones = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .split(frame.area());

    draw_header(frame, zones[0], cockpit);
    match cockpit.view {
        View::Fleet => draw_fleet(frame, zones[1], cockpit),
        View::Prs => draw_prs(frame, zones[1], cockpit),
        View::Ci => draw_ci(frame, zones[1], cockpit),
    }
    draw_status(frame, zones[2], cockpit);
    draw_footer(frame, zones[3]);
}

fn draw_header(frame: &mut ratzilla::ratatui::Frame, area: Rect, cockpit: &Cockpit) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(20)])
        .split(area);

    let repos = fleet_repos();
    let clean = repos
        .iter()
        .filter(|r| r.state == RepoState::Clean)
        .count();
    let info = vec![Line::from(vec![
        Span::styled(" context: ", Style::default().fg(theme::DIM)),
        Span::styled(
            "~/work/gateway",
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   stack: ", Style::default().fg(theme::DIM)),
        Span::styled(
            "gateway",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   view: ", Style::default().fg(theme::DIM)),
        Span::styled(
            cockpit.view.title(),
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   in sync: ", Style::default().fg(theme::DIM)),
        Span::styled(
            format!("{clean}/{}", repos.len()),
            Style::default().fg(theme::TEXT),
        ),
    ])];
    frame.render_widget(
        Paragraph::new(Text::from(info)).block(panel("haw ▸ fleet cockpit")),
        columns[0],
    );

    // Ambient spinner — the only self-advancing animation. State (cursor, view)
    // is driven purely by keys.
    const FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
    let spin = FRAMES[(cockpit.spinner / 6) as usize % FRAMES.len()];
    let logo = vec![Line::from(vec![
        Span::styled(
            "HAW ⚓ ",
            Style::default()
                .fg(theme::MAUVE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(spin, Style::default().fg(theme::TEAL)),
    ])];
    frame.render_widget(
        Paragraph::new(Text::from(logo))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(theme::SURFACE)),
            ),
        columns[1],
    );
}

fn draw_fleet(frame: &mut ratzilla::ratatui::Frame, area: Rect, cockpit: &Cockpit) {
    let repos = fleet_repos();

    let header = Row::new(vec![
        Cell::from(""),
        header_cell("REPO"),
        header_cell("BRANCH"),
        header_cell("HEAD"),
        header_cell("DIRTY"),
        header_cell("DRIFT"),
    ]);

    let rows: Vec<Row> = repos
        .iter()
        .enumerate()
        .map(|(i, repo)| {
            let (dirty, drift) = state_cells(repo.state);
            let selected = i == cockpit.cursor;
            Row::new(vec![
                Cell::from(state_dot(repo.state)),
                Cell::from(Span::styled(repo.name, cursor_style(selected))),
                Cell::from(Span::styled(
                    repo.branch,
                    Style::default().fg(theme::YELLOW),
                )),
                Cell::from(Span::styled(repo.head, Style::default().fg(theme::DIM))),
                Cell::from(dirty),
                Cell::from(drift),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Min(14),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(panel(&format!("fleet({})", repos.len())));

    frame.render_widget(table, area);
}

fn draw_prs(frame: &mut ratzilla::ratatui::Frame, area: Rect, cockpit: &Cockpit) {
    let header = Row::new(vec![
        header_cell("PR/MR"),
        header_cell("REPO"),
        header_cell("TITLE"),
        header_cell("STATUS"),
    ]);

    let rows: Vec<Row> = PRS
        .iter()
        .enumerate()
        .map(|(i, (id, repo, title, status))| {
            let selected = i == cockpit.cursor;
            let status_color = if status.contains("green") || status.contains("approv") {
                theme::GREEN
            } else if status.contains("running") {
                theme::YELLOW
            } else {
                theme::DIM
            };
            Row::new(vec![
                Cell::from(Span::styled(*id, cursor_style(selected))),
                Cell::from(Span::styled(*repo, Style::default().fg(theme::ACCENT))),
                Cell::from(Span::styled(*title, Style::default().fg(theme::TEXT))),
                Cell::from(Span::styled(*status, Style::default().fg(status_color))),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Min(20),
            Constraint::Length(22),
        ],
    )
    .header(header)
    .block(panel(&format!("pull / merge requests({})", PRS.len())));

    frame.render_widget(table, area);
}

fn draw_ci(frame: &mut ratzilla::ratatui::Frame, area: Rect, cockpit: &Cockpit) {
    let header = Row::new(vec![
        header_cell("REPO"),
        header_cell("PIPELINE"),
        header_cell("RESULT"),
        header_cell("DURATION"),
    ]);

    let rows: Vec<Row> = CI
        .iter()
        .enumerate()
        .map(|(i, (repo, pipeline, result, dur))| {
            let selected = i == cockpit.cursor;
            let (glyph, color) = match *result {
                "passed" => ("✓", theme::GREEN),
                "running" => ("»", theme::YELLOW),
                "queued" => ("·", theme::DIM),
                _ => ("✗", theme::RED),
            };
            Row::new(vec![
                Cell::from(Span::styled(*repo, cursor_style(selected))),
                Cell::from(Span::styled(*pipeline, Style::default().fg(theme::TEXT))),
                Cell::from(Span::styled(
                    format!("{glyph} {result}"),
                    Style::default().fg(color),
                )),
                Cell::from(Span::styled(*dur, Style::default().fg(theme::DIM))),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Min(12),
            Constraint::Length(14),
            Constraint::Length(10),
        ],
    )
    .header(header)
    .block(panel(&format!("ci pipelines({})", CI.len())));

    frame.render_widget(table, area);
}

fn draw_status(frame: &mut ratzilla::ratatui::Frame, area: Rect, cockpit: &Cockpit) {
    let message = match cockpit.view {
        View::Fleet => {
            let repo = &FLEET[cockpit.cursor.min(FLEET.len() - 1)];
            format!(
                " selected {} · {} @ {} — j/k move · Tab switch view",
                repo.0, repo.1, repo.2
            )
        }
        View::Prs => {
            let pr = &PRS[cockpit.cursor.min(PRS.len() - 1)];
            format!(
                " selected {} ({}) — {} · j/k move · Tab switch view",
                pr.0, pr.1, pr.3
            )
        }
        View::Ci => {
            let job = &CI[cockpit.cursor.min(CI.len() - 1)];
            format!(
                " selected {} · {} — {} · j/k move · Tab switch view",
                job.0, job.1, job.2
            )
        }
    };
    frame.render_widget(
        Paragraph::new(Line::styled(message, Style::default().fg(theme::TEAL))),
        area,
    );
}

fn draw_footer(frame: &mut ratzilla::ratatui::Frame, area: Rect) {
    let lines = vec![
        Line::from(vec![
            Span::styled("⌨ ", Style::default().fg(theme::TEAL)),
            Span::styled(
                "j/k · ↑/↓ move    Tab / m switch view",
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::styled(
            "interactive demo — the real cockpit: cargo install hawser",
            Style::default().fg(theme::SURFACE),
        ),
    ];
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

// wasm32-unknown-unknown runs single-threaded on the browser's main thread —
// there's no real preemption, so a critical section is a no-op. ratatui-core
// needs an impl registered; the browser event loop provides no other one.
struct NoPreemption;
critical_section::set_impl!(NoPreemption);
unsafe impl critical_section::Impl for NoPreemption {
    unsafe fn acquire() -> critical_section::RawRestoreState {}
    unsafe fn release(_token: critical_section::RawRestoreState) {}
}

fn main() {
    let cockpit = Rc::new(RefCell::new(Cockpit::new()));

    let backend = DomBackend::new().expect("DOM backend");
    let mut terminal = Terminal::new(backend).expect("terminal");

    // Wire real keyboard interaction via Ratzilla's `on_key_event` hook. The
    // callback mutates the shared `Cockpit`; the render closure reads it.
    {
        let cockpit = cockpit.clone();
        terminal
            .on_key_event(move |event: KeyEvent| {
                let mut c = cockpit.borrow_mut();
                match event.code {
                    KeyCode::Char('j') | KeyCode::Down => c.move_down(),
                    KeyCode::Char('k') | KeyCode::Up => c.move_up(),
                    KeyCode::Tab | KeyCode::Char('m') | KeyCode::Char('i') => c.cycle_view(),
                    _ => {}
                }
            })
            .expect("register key handler");
    }

    terminal.draw_web(move |frame| {
        {
            // Advance only the ambient spinner each frame; cursor/view are
            // key-driven and left untouched here.
            let mut c = cockpit.borrow_mut();
            c.spinner = c.spinner.wrapping_add(1);
        }
        let c = cockpit.borrow();
        draw(frame, &c);
    });
}
