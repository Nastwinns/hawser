//! Terminal-render (rasterized) snapshot tests for the fleet dashboard.
//!
//! The rest of the suite tests `App` state in isolation and never renders to a
//! terminal, so layout/wrapping/overflow regressions slip through. Here we
//! drive the *real* draw path via ratatui's [`TestBackend`] (no process spawn,
//! fully deterministic) through the `haw_tui::render_snapshot` test seam, and
//! assert on the resulting cell grid.
//!
//! Two sizes are checked:
//!   * normal (40x160): the fleet header row + every repo row is visible;
//!   * small  (10x40): the collapsible header shrinks to one line so data rows
//!     stay on screen (audit fix #3).
//!
//! The `⚓ haw v<version>` footer must appear in its own right-aligned cell.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use haw_core::workspace::RepoStatus;
use haw_tui::Snapshot;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

fn repo(name: &str, groups: &[&str]) -> RepoStatus {
    RepoStatus {
        name: name.to_string(),
        path: PathBuf::from(format!("/w/{name}")),
        missing: false,
        branch: Some("main".to_string()),
        head: Some("a".repeat(40)),
        dirty: false,
        locked_rev: Some("a".repeat(40)),
        drift: false,
        ahead_behind: Some((0, 0)),
        groups: groups.iter().map(|g| g.to_string()).collect(),
    }
}

fn fleet_snapshot() -> Snapshot {
    Snapshot {
        root_label: "acme".to_string(),
        stacks: vec!["gw".to_string()],
        current_stack: Some("gw".to_string()),
        fleet: vec![(
            "gw".to_string(),
            vec![
                repo("kernel", &["firmware", "ci"]),
                repo("hal", &["firmware"]),
                repo("app-mqtt", &[]),
            ],
        )],
        ..Default::default()
    }
}

/// Flatten a rendered [`Buffer`] into one `String` per row (symbols joined,
/// styling dropped) so tests can assert on visible text.
fn rows_text(buf: &Buffer) -> Vec<String> {
    let area = buf.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn render(width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    haw_tui::render_snapshot(&mut terminal, fleet_snapshot()).unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn fleet_grid_renders_header_and_all_repo_rows_at_normal_size() {
    let buf = render(160, 40);
    let rows = rows_text(&buf);
    let screen = rows.join("\n");

    // Header row: the fleet table column headers are all present.
    for col in [
        "REPO", "GROUPS", "BRANCH", "HEAD", "DIRTY", "DRIFT", "MERGE",
    ] {
        assert!(
            screen.contains(col),
            "fleet header column {col:?} missing from render:\n{screen}"
        );
    }

    // Every repo row is rendered.
    for name in ["kernel", "hal", "app-mqtt"] {
        assert!(
            screen.contains(name),
            "repo row {name:?} missing from render:\n{screen}"
        );
    }

    // Group labels show up too (data cells, not just headers).
    assert!(
        screen.contains("firmware"),
        "group label missing:\n{screen}"
    );
}

#[test]
fn footer_shows_anchored_version_cell() {
    let buf = render(160, 40);
    let rows = rows_text(&buf);
    // Footer is the last rendered row; the version tag is right-aligned into its
    // own cell so the breadcrumb trail can never overwrite it.
    let footer = rows.last().expect("at least one row");
    // The anchor glyph is double-width, so the terminal pads it; assert on the
    // stable text portion plus the anchor's presence.
    let expected = format!("haw v{}", env!("CARGO_PKG_VERSION"));
    assert!(
        footer.contains(&expected) && footer.contains('⚓'),
        "footer {footer:?} missing version tag {expected:?}"
    );
    // Right-aligned: the tag sits at the far right of the line.
    assert!(
        footer.trim_end().ends_with(env!("CARGO_PKG_VERSION")),
        "version tag is not right-aligned in footer {footer:?}"
    );
}

#[test]
fn small_terminal_collapses_header_and_keeps_data_rows_visible() {
    // 10 rows is below COMPACT_HEADER_HEIGHT (16): the ~6-row header must
    // collapse to a single compact line, leaving data rows on screen.
    let buf = render(40, 10);
    let rows = rows_text(&buf);
    let screen = rows.join("\n");

    // The compact header banner (⚓ + root label) is present...
    assert!(
        screen.contains("⚓") && screen.contains("acme"),
        "compact header banner missing on small terminal:\n{screen}"
    );

    // ...and — the whole point of the collapse — at least one repo data row is
    // still visible despite the cramped height.
    let visible_repos = ["kernel", "hal", "app-mqtt"]
        .iter()
        .filter(|name| screen.contains(**name))
        .count();
    assert!(
        visible_repos >= 1,
        "no repo data rows visible after header collapse:\n{screen}"
    );

    // The header did collapse: with only 10 rows, the full multi-row header
    // would leave no room for the fleet panel title. Assert the panel rendered.
    assert!(
        screen.contains("fleet"),
        "fleet panel title missing on small terminal:\n{screen}"
    );
}
