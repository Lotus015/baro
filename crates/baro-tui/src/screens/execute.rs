use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Tabs},
    Frame,
};

use crate::app::{App, GlobalTab, StoryStatus};
use crate::theme;

use super::execute_completion::render_completion;
use super::execute_dashboard::render_dashboard;
use super::execute_dag::render_dag_full;
use super::execute_stats::render_stats_full;

pub fn render(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header + tabs
            Constraint::Min(8),   // Main content (tab-dependent)
            Constraint::Length(3), // Progress bar
            Constraint::Length(1), // Footer
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);

    match app.global_tab {
        GlobalTab::Dashboard => render_dashboard(f, app, chunks[1]),
        GlobalTab::Dag => render_dag_full(f, app, chunks[1]),
        GlobalTab::Stats => render_stats_full(f, app, chunks[1]),
    }

    render_progress(f, app, chunks[2]);
    render_footer(f, app, chunks[3]);

    if app.done {
        render_completion(f, app);
    }
}

// --- Header with Tabs ---

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(60),
            Constraint::Length(40),
        ])
        .split(area);

    let elapsed = app.elapsed_secs();
    let active_count = app.active_stories.len();

    let info_line = Line::from(vec![
        Span::styled(
            " BARO ",
            Style::default().fg(theme::LOGO_1).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" \u{2502} ", Style::default().fg(theme::BORDER)),
        Span::styled(
            &app.project,
            Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" \u{2502} ", Style::default().fg(theme::BORDER)),
        Span::styled(
            &app.branch_name,
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(" \u{2502} ", Style::default().fg(theme::BORDER)),
        Span::styled(
            format!("{} active", active_count),
            Style::default().fg(theme::WARNING),
        ),
        Span::styled(" \u{2502} ", Style::default().fg(theme::BORDER)),
        Span::styled(
            format!("{:02}:{:02}", elapsed / 60, elapsed % 60),
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(" \u{2502} ", Style::default().fg(theme::BORDER)),
        Span::styled(
            format!("{}/{}", app.completed, app.total),
            Style::default().fg(theme::SUCCESS),
        ),
        Span::styled(" \u{2502} ", Style::default().fg(theme::BORDER)),
        Span::styled(
            if app.parallel_limit == 0 {
                "parallel: \u{221E}".to_string()
            } else {
                format!("parallel: {}", app.parallel_limit)
            },
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(" \u{2502} ", Style::default().fg(theme::BORDER)),
        Span::styled(
            format!("timeout: {}s", app.timeout_secs),
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(" │ ", Style::default().fg(theme::BORDER)),
        Span::styled(
            if let Some(ref name) = app.override_model {
                format!("model: {}", name)
            } else if app.model_routing {
                "model: routed".to_string()
            } else {
                "model: default".to_string()
            },
            Style::default().fg(
                if app.override_model.is_some() {
                    theme::WARNING
                } else if app.model_routing {
                    theme::ACCENT
                } else {
                    theme::MUTED
                },
            ),
        ),
    ]);

    let info = Paragraph::new(info_line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER)),
    );
    f.render_widget(info, header_chunks[0]);

    let tab_titles = vec![
        Span::styled(" 1:Dashboard ", Style::default()),
        Span::styled(" 2:DAG ", Style::default()),
        Span::styled(" 3:Stats ", Style::default()),
    ];

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER)),
        )
        .select(app.global_tab.index())
        .style(Style::default().fg(theme::MUTED))
        .highlight_style(
            Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled("\u{2502}", Style::default().fg(theme::BORDER)));

    f.render_widget(tabs, header_chunks[1]);
}

// --- Shared: Progress Bar ---

fn render_progress(f: &mut Frame, app: &App, area: Rect) {
    let ratio = if app.total > 0 {
        (app.completed as f64 / app.total as f64).min(1.0)
    } else {
        0.0
    };

    let label = format!(
        "{}% ({}/{} stories)",
        app.percentage, app.completed, app.total
    );

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER))
                .title(Span::styled(
                    " Progress ",
                    Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
                )),
        )
        .gauge_style(Style::default().fg(theme::GAUGE_FG))
        .ratio(ratio)
        .label(Span::styled(
            label,
            Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        ));

    f.render_widget(gauge, area);
}

// --- Shared: Footer ---

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let msg = if app.finalize_in_progress {
        " Finalizing...".to_string()
    } else if app.done {
        let stats = app.final_stats.as_ref();
        let completed = stats.map(|s| s.stories_completed).unwrap_or(0);
        let skipped = stats.map(|s| s.stories_skipped).unwrap_or(0);
        let elapsed = app.total_time_secs;
        format!(
            " Done! {} completed, {} skipped in {}:{:02} | q:exit",
            completed,
            skipped,
            elapsed / 60,
            elapsed % 60,
        )
    } else {
        " 1/2/3:tabs | Tab/Shift+Tab:logs | q:quit".to_string()
    };

    let footer = Paragraph::new(Span::styled(msg, Style::default().fg(theme::MUTED)));
    f.render_widget(footer, area);
}

// --- Helpers ---

pub(crate) fn status_icon_color(status: &StoryStatus) -> (&'static str, ratatui::style::Color) {
    match status {
        StoryStatus::Complete => ("✓", theme::SUCCESS),
        StoryStatus::Running => ("▶", theme::WARNING),
        StoryStatus::Failed => ("✗", theme::ERROR),
        StoryStatus::Retrying(_) => ("↻", theme::WARNING),
        StoryStatus::Skipped => ("⊘", theme::MUTED),
        StoryStatus::Pending => ("○", theme::MUTED),
    }
}
