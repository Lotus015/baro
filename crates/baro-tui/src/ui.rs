use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Row, Table, Tabs, Wrap},
    Frame,
};

use crate::app::{App, GlobalTab, StoryStatus};
use crate::theme;

// ─── Main Render ─────────────────────────────────────────────

pub fn render(f: &mut Frame, app: &App) {
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
}

// ─── Header with Tabs ────────────────────────────────────────

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(30),         // Info
            Constraint::Length(36),       // Tabs
        ])
        .split(area);

    // Left: project info
    let elapsed = app.elapsed_secs();
    let active_count = app.active_stories.len();

    let info_line = Line::from(vec![
        Span::styled(
            " BARO ",
            Style::default()
                .fg(theme::LOGO_1)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" \u{2502} ", Style::default().fg(theme::BORDER)),
        Span::styled(
            &app.project,
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
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
    ]);

    let info = Paragraph::new(info_line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER)),
    );
    f.render_widget(info, header_chunks[0]);

    // Right: tab selector
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
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled("\u{2502}", Style::default().fg(theme::BORDER)));

    f.render_widget(tabs, header_chunks[1]);
}

// ─── Tab 1: Dashboard (stories + logs) ──────────────────────

fn render_dashboard(f: &mut Frame, app: &App, area: Rect) {
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Percentage(65),
        ])
        .split(area);

    render_story_list(f, app, main_chunks[0]);
    render_logs(f, app, main_chunks[1]);
}

fn render_story_list(f: &mut Frame, app: &App, area: Rect) {
    let mut items: Vec<ListItem> = Vec::new();

    if app.dag_levels.is_empty() {
        for story in &app.stories {
            items.push(story_list_item(story));
        }
    } else {
        for (i, level) in app.dag_levels.iter().enumerate() {
            items.push(ListItem::new(Line::from(Span::styled(
                format!(" Level {}:", i),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ))));

            for story_id in level {
                if let Some(story) = app.stories.iter().find(|s| s.id == *story_id) {
                    items.push(story_list_item(story));
                }
            }

            if i < app.dag_levels.len() - 1 {
                items.push(ListItem::new(Line::from(Span::styled(
                    "   \u{2502}",
                    Style::default().fg(theme::MUTED),
                ))));
            }
        }
    }

    let list = List::new(items).block(
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
    f.render_widget(list, area);
}

fn story_list_item(story: &crate::app::StoryState) -> ListItem<'static> {
    let (icon, style) = match &story.status {
        StoryStatus::Complete => ("\u{2705}", Style::default().fg(theme::SUCCESS)),
        StoryStatus::Running => ("\u{1f504}", Style::default().fg(theme::WARNING)),
        StoryStatus::Failed => ("\u{274c}", Style::default().fg(theme::ERROR)),
        StoryStatus::Retrying(_) => ("\u{1f501}", Style::default().fg(theme::WARNING)),
        StoryStatus::Skipped => ("\u{23ed}\u{fe0f}", Style::default().fg(theme::MUTED)),
        StoryStatus::Pending => ("\u{23f8}\u{fe0f}", Style::default().fg(theme::MUTED)),
    };

    let duration = story
        .duration_secs
        .map(|d| format!(" ({}:{:02})", d / 60, d % 60))
        .unwrap_or_default();

    let retry_info = match &story.status {
        StoryStatus::Retrying(n) => format!(" retry #{}", n),
        _ => String::new(),
    };

    ListItem::new(Line::from(vec![
        Span::raw("   "),
        Span::styled(
            format!(
                "{} {}: {}{}{}",
                icon, story.id, story.title, duration, retry_info
            ),
            style,
        ),
    ]))
}

fn render_logs(f: &mut Frame, app: &App, area: Rect) {
    let active_ids = app.active_story_ids();

    if active_ids.is_empty() {
        let msg = if app.done {
            "All done!"
        } else if app.stories.is_empty() {
            "Waiting for events..."
        } else {
            "Waiting for next story..."
        };

        let p = Paragraph::new(Span::styled(msg, Style::default().fg(theme::MUTED))).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER))
                .title(Span::styled(
                    " Logs ",
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )),
        );
        f.render_widget(p, area);
        return;
    }

    // Log tab bar + full log for selected story
    let log_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Log tabs
            Constraint::Min(4),   // Log content
        ])
        .split(area);

    // Tab bar for active stories
    let tab_titles: Vec<Span> = active_ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let story = app.active_stories.get(id);
            let title = story.map(|s| s.title.as_str()).unwrap_or(id.as_str());
            let elapsed = story
                .map(|s| s.start_time.elapsed().as_secs())
                .unwrap_or(0);
            let label = format!(" {}:{} {:02}:{:02} ", id, title, elapsed / 60, elapsed % 60);

            if i == app.selected_log_index {
                Span::styled(
                    label,
                    Style::default()
                        .fg(theme::WARNING)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(label, Style::default().fg(theme::MUTED))
            }
        })
        .collect();

    let log_tabs = Tabs::new(tab_titles)
        .select(app.selected_log_index)
        .style(Style::default().fg(theme::MUTED))
        .highlight_style(
            Style::default()
                .fg(theme::WARNING)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .divider(Span::styled("\u{2502}", Style::default().fg(theme::BORDER)));

    f.render_widget(log_tabs, log_chunks[0]);

    // Full log content for selected story
    let selected_id = active_ids
        .get(app.selected_log_index)
        .cloned()
        .unwrap_or_default();

    if let Some(story) = app.active_stories.get(&selected_id) {
        let inner_height = log_chunks[1].height.saturating_sub(2) as usize;
        let skip = story.logs.len().saturating_sub(inner_height);
        let visible_logs: Vec<Line> = story.logs[skip..]
            .iter()
            .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(theme::TEXT))))
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::WARNING))
            .title(Span::styled(
                format!(" {} ", story.id),
                Style::default()
                    .fg(theme::WARNING)
                    .add_modifier(Modifier::BOLD),
            ));

        let p = Paragraph::new(visible_logs)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(p, log_chunks[1]);
    }
}

// ─── Tab 2: DAG Full View ───────────────────────────────────

fn render_dag_full(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    if app.dag_levels.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Waiting for DAG data...",
            Style::default().fg(theme::MUTED),
        )));
    } else {
        for (i, level) in app.dag_levels.iter().enumerate() {
            // Level header
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  Level {} ", i),
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({} stories)", level.len()),
                    Style::default().fg(theme::MUTED),
                ),
            ]));

            // Stories in level
            for story_id in level {
                if let Some(story) = app.stories.iter().find(|s| s.id == *story_id) {
                    let (icon, color) = status_icon_color(&story.status);
                    let duration = story
                        .duration_secs
                        .map(|d| format!(" {}:{:02}", d / 60, d % 60))
                        .unwrap_or_default();

                    let deps = if story.depends_on.is_empty() {
                        String::new()
                    } else {
                        format!(" \u{2190} {}", story.depends_on.join(", "))
                    };

                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(
                            format!("{} {}", icon, story.id),
                            Style::default().fg(color).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(": {}", story.title),
                            Style::default().fg(color),
                        ),
                        Span::styled(duration, Style::default().fg(theme::SUCCESS)),
                        Span::styled(deps, Style::default().fg(theme::MUTED)),
                    ]));

                    if let Some(ref err) = story.error {
                        lines.push(Line::from(vec![
                            Span::raw("         "),
                            Span::styled(
                                format!("\u{26a0} {}", err),
                                Style::default().fg(theme::ERROR),
                            ),
                        ]));
                    }
                }
            }

            // Connector between levels
            if i < app.dag_levels.len() - 1 {
                lines.push(Line::from(Span::styled(
                    "    \u{2502}",
                    Style::default().fg(theme::MUTED),
                )));
                lines.push(Line::from(Span::styled(
                    "    \u{25bc}",
                    Style::default().fg(theme::MUTED),
                )));
            }
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .title(Span::styled(
            " Dependency Graph ",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));

    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, area);
}

// ─── Tab 3: Stats Full View ─────────────────────────────────

fn render_stats_full(f: &mut Frame, app: &App, area: Rect) {
    let stats_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Summary
            Constraint::Min(4),   // Story table
        ])
        .split(area);

    // Summary box
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

    let summary_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Stories: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}", app.completed),
                Style::default()
                    .fg(theme::SUCCESS)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("/{} complete", app.total),
                Style::default().fg(theme::MUTED),
            ),
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
                Style::default().fg(theme::ACCENT),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Fastest: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}:{:02}", fastest / 60, fastest % 60),
                Style::default().fg(theme::SUCCESS),
            ),
            Span::styled("    ", Style::default()),
            Span::styled("Slowest: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}:{:02}", slowest / 60, slowest % 60),
                Style::default().fg(theme::WARNING),
            ),
            Span::styled("    ", Style::default()),
            Span::styled("Files: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("+{} ~{}", total_files_created, total_files_modified),
                Style::default().fg(theme::ACCENT),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Skipped: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!(
                    "{}",
                    final_stats.map(|s| s.stories_skipped).unwrap_or(0)
                ),
                Style::default().fg(if final_stats.map(|s| s.stories_skipped).unwrap_or(0) > 0 {
                    theme::ERROR
                } else {
                    theme::SUCCESS
                }),
            ),
            Span::styled("    ", Style::default()),
            Span::styled("Commits: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!(
                    "{}",
                    final_stats.map(|s| s.total_commits).unwrap_or(app.completed)
                ),
                Style::default().fg(theme::ACCENT),
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

    // Story table
    let header = Row::new(vec![
        "  ID",
        "Title",
        "Status",
        "Time",
        "Files",
        "Deps",
    ])
    .style(
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

            let files = if s.files_created > 0 || s.files_modified > 0 {
                format!("+{} ~{}", s.files_created, s.files_modified)
            } else {
                "-".to_string()
            };

            let deps = if s.depends_on.is_empty() {
                "-".to_string()
            } else {
                s.depends_on.join(",")
            };

            Row::new(vec![
                format!("  {}", s.id),
                s.title.clone(),
                status_str.to_string(),
                time,
                files,
                deps,
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

    let table = Table::new(rows, widths)
        .header(header)
        .block(
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

    f.render_widget(table, stats_chunks[1]);
}

// ─── Shared: Progress Bar ────────────────────────────────────

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
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .gauge_style(Style::default().fg(theme::SUCCESS))
        .ratio(ratio)
        .label(Span::styled(
            label,
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ));

    f.render_widget(gauge, area);
}

// ─── Shared: Footer ──────────────────────────────────────────

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let msg = if app.done {
        let stats = app.final_stats.as_ref();
        let completed = stats.map(|s| s.stories_completed).unwrap_or(0);
        let skipped = stats.map(|s| s.stories_skipped).unwrap_or(0);
        let elapsed = app.total_time_secs;
        format!(
            " \u{1f389} Done! {} completed, {} skipped in {}:{:02} \u{2502} q:exit",
            completed,
            skipped,
            elapsed / 60,
            elapsed % 60,
        )
    } else {
        format!(
            " 1/2/3:tabs \u{2502} Tab/Shift+Tab:logs \u{2502} q:quit"
        )
    };

    let footer = Paragraph::new(Span::styled(msg, Style::default().fg(theme::MUTED)));
    f.render_widget(footer, area);
}

// ─── Helpers ─────────────────────────────────────────────────

fn status_icon_color(status: &StoryStatus) -> (&'static str, ratatui::style::Color) {
    match status {
        StoryStatus::Complete => ("\u{2705}", theme::SUCCESS),
        StoryStatus::Running => ("\u{1f504}", theme::WARNING),
        StoryStatus::Failed => ("\u{274c}", theme::ERROR),
        StoryStatus::Retrying(_) => ("\u{1f501}", theme::WARNING),
        StoryStatus::Skipped => ("\u{23ed}\u{fe0f}", theme::MUTED),
        StoryStatus::Pending => ("\u{23f8}\u{fe0f}", theme::MUTED),
    }
}
