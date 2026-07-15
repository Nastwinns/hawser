//! Read-only ratatui fleet dashboard: product -> brick tree on the left,
//! per-brick detail on the right. Actions arrive in Phase 4.

use std::io;

use keel_core::workspace::BrickStatus;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

/// One product and the observed state of its bricks.
#[derive(Debug, Clone)]
pub struct FleetView {
    pub product: String,
    pub bricks: Vec<BrickStatus>,
}

enum Row {
    Product(String),
    Brick(BrickStatus),
}

fn flatten(views: &[FleetView]) -> Vec<Row> {
    let mut rows = Vec::new();
    for view in views {
        rows.push(Row::Product(view.product.clone()));
        for brick in &view.bricks {
            rows.push(Row::Brick(brick.clone()));
        }
    }
    rows
}

fn glyph(brick: &BrickStatus) -> (&'static str, Color) {
    if brick.missing {
        ("✗", Color::Red)
    } else if brick.dirty {
        ("!", Color::Yellow)
    } else if brick.drift {
        ("●", Color::Magenta)
    } else {
        ("✓", Color::Green)
    }
}

fn short(sha: &str) -> &str {
    sha.get(..8).unwrap_or(sha)
}

/// Run the dashboard until `q`/`Esc`. `refresh` recomputes the fleet on `r`.
pub fn run<F>(mut refresh: F) -> io::Result<()>
where
    F: FnMut() -> io::Result<Vec<FleetView>>,
{
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut refresh);
    ratatui::restore();
    result
}

fn event_loop<F>(terminal: &mut ratatui::DefaultTerminal, refresh: &mut F) -> io::Result<()>
where
    F: FnMut() -> io::Result<Vec<FleetView>>,
{
    let mut views = refresh()?;
    let mut rows = flatten(&views);
    let mut state = ListState::default();
    state.select(Some(0));

    loop {
        terminal.draw(|frame| draw(frame, &rows, &mut state))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        let selected = state.selected().unwrap_or(0);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
            KeyCode::Char('r') => {
                views = refresh()?;
                rows = flatten(&views);
                state.select(Some(selected.min(rows.len().saturating_sub(1))));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.select(Some((selected + 1).min(rows.len().saturating_sub(1))));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.select(Some(selected.saturating_sub(1)));
            }
            _ => {}
        }
    }
}

fn draw(frame: &mut Frame, rows: &[Row], state: &mut ListState) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(frame.area());

    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| match row {
            Row::Product(name) => ListItem::new(Line::styled(
                name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Row::Brick(brick) => {
                let (mark, color) = glyph(brick);
                ListItem::new(Line::styled(
                    format!("  {mark} {}", brick.name),
                    Style::default().fg(color),
                ))
            }
        })
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" fleet "))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, panes[0], state);

    let detail: Vec<Line> = match state.selected().and_then(|i| rows.get(i)) {
        Some(Row::Brick(brick)) => vec![
            Line::from(format!("repo    {}", brick.name)),
            Line::from(format!("path    {}", brick.path.display())),
            Line::from(format!(
                "branch  {}",
                brick.branch.as_deref().unwrap_or("(detached)")
            )),
            Line::from(format!(
                "head    {}",
                brick.head.as_deref().map_or("—", short)
            )),
            Line::from(format!(
                "lock    {}",
                brick.locked_rev.as_deref().map_or("—", short)
            )),
            Line::from(format!(
                "dirty   {}",
                if brick.dirty { "yes" } else { "no" }
            )),
            Line::from(format!(
                "drift   {}",
                if brick.drift {
                    "YES (head != lock)"
                } else {
                    "no"
                }
            )),
            Line::from(if brick.missing {
                "state   NOT CLONED — run `keel sync`"
            } else {
                "state   present"
            }),
        ],
        Some(Row::Product(name)) => vec![
            Line::from(format!("stack   {name}")),
            Line::from("select a repo for details"),
        ],
        None => vec![Line::from("no repos — check keel.toml")],
    };
    let paragraph = Paragraph::new(detail).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" detail — q quit · r refresh · j/k move "),
    );
    frame.render_widget(paragraph, panes[1]);
}
