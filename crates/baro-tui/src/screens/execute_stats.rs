use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{BarChart, Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use crate::app::{App, StoryStatus};
use crate::theme;

pub fn render_stats_full(f: &mut Frame, app: &App, area: Rect) {
    let has_bar_data = app.stories.iter().any(|s| s.duration_secs.is_some());
    let mut constraints = vec![Constraint::Length(6)]; // Summary
    if has_bar_data {
        constraints.push(Constraint::Length(10)); // Bar chart
    }
    constraints.push(Constraint::Min(4)); // Table

    let stats_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let elapsed = app.elapsed_secs();
    let avg = if app.completed > 0 {
        elapsed / app.completed as u64
    } else {
        0
    };

    let completed_stories: Vec<&crate::app::StoryState> = app
        .stories
        .iter()
        .filter(|s| s.duration_secs.is_some())
        .collect();
    let fastest = completed_stories
        .iter()
        .filter_map(|s| s.duration_secs)
        .min()
        .unwrap_or(0);
    let slowest = completed_stories
        .iter()
        .filter_map(|s| s.duration_secs)
        .max()
        .unwrap_or(0);
    let total_files_created: u32 = app.stories.iter().map(|s| s.files_created).sum();
    let total_files_modified: u32 = app.stories.iter().map(|s| s.files_modified).sum();
    let final_stats = app.final_stats.as_ref();

    // -- Summary --
    let summary_lines = vec![
        Line::from(vec![
            Span::styled("  Stories: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}", app.completed),
                Style::default()
                    .fg(theme::SUCCESS)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("/{}", app.total), Style::default().fg(theme::MUTED)),
            Span::styled("    ", Style::default()),
            Span::styled("Time: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}:{:02}", elapsed / 60, elapsed % 60),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("    ", Style::default()),
            Span::styled("Avg: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}:{:02}", avg / 60, avg % 60),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("    ", Style::default()),
            Span::styled("Fast: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}:{:02}", fastest / 60, fastest % 60),
                Style::default()
                    .fg(theme::SUCCESS)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Slow: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}:{:02}", slowest / 60, slowest % 60),
                Style::default()
                    .fg(theme::WARNING)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Files: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("+{}", total_files_created),
                Style::default()
                    .fg(theme::SUCCESS)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default()),
            Span::styled(
                format!("~{}", total_files_modified),
                Style::default()
                    .fg(theme::WARNING)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("    ", Style::default()),
            Span::styled("Skipped: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!(
                    "{}",
                    final_stats.map(|s| s.stories_skipped).unwrap_or(0)
                ),
                Style::default()
                    .fg(
                        if final_stats.map(|s| s.stories_skipped).unwrap_or(0) > 0 {
                            theme::ERROR
                        } else {
                            theme::SUCCESS
                        },
                    )
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("    ", Style::default()),
            Span::styled("Commits: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!(
                    "{}",
                    final_stats
                        .map(|s| s.total_commits)
                        .unwrap_or(app.completed)
                ),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let summary = Paragraph::new(summary_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(Span::styled(
                " Summary ",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(summary, stats_chunks[0]);

    let mut next_chunk = 1;

    // -- Bar chart of story durations --
    if has_bar_data {
        let bar_data: Vec<(String, u64)> = app
            .stories
            .iter()
            .filter_map(|s| s.duration_secs.map(|d| (s.id.clone(), d)))
            .collect();

        let bar_items: Vec<(&str, u64)> =
            bar_data.iter().map(|(id, d)| (id.as_str(), *d)).collect();

        let chart = BarChart::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme::BORDER))
                    .title(Span::styled(
                        " Duration (seconds) ",
                        Style::default()
                            .fg(theme::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    )),
            )
            .data(&bar_items)
            .bar_width(5)
            .bar_gap(1)
            .bar_style(Style::default().fg(theme::ACCENT_BRIGHT))
            .value_style(
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            )
            .label_style(Style::default().fg(theme::TEXT_DIM));

        f.render_widget(chart, stats_chunks[next_chunk]);
        next_chunk += 1;
    }

    let table_chunk_idx = next_chunk;

    // -- Story table --
    let header = Row::new(vec!["  ID", "Title", "Status", "Time", "Files", "Deps"]).style(
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD),
    );

    let rows: Vec<Row> = app
        .stories
        .iter()
        .map(|s| {
            let (status_str, color) = match &s.status {
                StoryStatus::Complete => ("Done", theme::SUCCESS),
                StoryStatus::Running => ("Running", theme::WARNING),
                StoryStatus::Failed => ("Failed", theme::ERROR),
                StoryStatus::Retrying(_) => ("Retry", theme::WARNING),
                StoryStatus::Skipped => ("Skipped", theme::MUTED),
                StoryStatus::Pending => ("Pending", theme::MUTED),
            };

            let time = s
                .duration_secs
                .map(|d| format!("{}:{:02}", d / 60, d % 60))
                .unwrap_or_else(|| {
                    if s.status == StoryStatus::Running {
                        if let Some(active) = app.active_stories.get(&s.id) {
                            let e = active.start_time.elapsed().as_secs();
                            return format!("{}:{:02}...", e / 60, e % 60);
                        }
                    }
                    "-".to_string()
                });

            let files_cell = if s.files_created > 0 || s.files_modified > 0 {
                Cell::from(Line::from(vec![
                    Span::styled(
                        format!("+{}", s.files_created),
                        Style::default().fg(theme::SUCCESS),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("~{}", s.files_modified),
                        Style::default().fg(theme::WARNING),
                    ),
                ]))
            } else {
                Cell::from("-")
            };

            let deps = if s.depends_on.is_empty() {
                "-".to_string()
            } else {
                s.depends_on.join(",")
            };

            Row::new(vec![
                Cell::from(format!("  {}", s.id)),
                Cell::from(s.title.clone()),
                Cell::from(status_str.to_string()),
                Cell::from(time),
                files_cell,
                Cell::from(deps),
            ])
            .style(Style::default().fg(color))
        })
        .collect();

    let widths = [
        Constraint::Length(6),
        Constraint::Min(15),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths).header(header).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(Span::styled(
                " Stories ",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )),
    );

    f.render_widget(table, stats_chunks[table_chunk_idx]);
}
