use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::app::App;
use crate::screens::execute::status_icon_color;
use crate::theme;

pub fn render_dag_full(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    if app.dag_levels.is_empty() {
        lines.push(Line::from(Span::styled(
            "  Waiting for DAG data...",
            Style::default().fg(theme::MUTED),
        )));
    } else {
        for (i, level) in app.dag_levels.iter().enumerate() {
            // Level header with box
            let level_label = format!(" Level {} ", i);
            let story_count = level.len();
            lines.push(Line::from(vec![
                Span::styled("  \u{250c}", Style::default().fg(theme::ACCENT_DIM)),
                Span::styled(
                    "\u{2500}".repeat(level_label.len()),
                    Style::default().fg(theme::ACCENT_DIM),
                ),
                Span::styled("\u{2510}", Style::default().fg(theme::ACCENT_DIM)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  \u{2502}", Style::default().fg(theme::ACCENT_DIM)),
                Span::styled(
                    level_label.clone(),
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("\u{2502}", Style::default().fg(theme::ACCENT_DIM)),
                Span::styled(
                    format!("  {} {}", story_count, if story_count == 1 { "story" } else { "stories" }),
                    Style::default().fg(theme::MUTED),
                ),
                if story_count > 1 {
                    Span::styled(" (parallel)", Style::default().fg(theme::ACCENT_DIM))
                } else {
                    Span::raw("")
                },
            ]));
            lines.push(Line::from(vec![
                Span::styled("  \u{2514}", Style::default().fg(theme::ACCENT_DIM)),
                Span::styled(
                    "\u{2500}".repeat(level_label.len()),
                    Style::default().fg(theme::ACCENT_DIM),
                ),
                Span::styled("\u{2518}", Style::default().fg(theme::ACCENT_DIM)),
            ]));

            // Stories as cards
            for (j, story_id) in level.iter().enumerate() {
                if let Some(story) = app.stories.iter().find(|s| s.id == *story_id) {
                    let (icon, color) = status_icon_color(&story.status);
                    let duration = story
                        .duration_secs
                        .map(|d| format!(" {}:{:02}", d / 60, d % 60))
                        .unwrap_or_default();

                    // Connector from level box to story
                    let connector = if j == 0 && level.len() == 1 {
                        "  \u{2502}   \u{2514}\u{2500}\u{2500} "
                    } else if j == 0 {
                        "  \u{2502}   \u{251c}\u{2500}\u{2500} "
                    } else if j == level.len() - 1 {
                        "  \u{2502}   \u{2514}\u{2500}\u{2500} "
                    } else {
                        "  \u{2502}   \u{251c}\u{2500}\u{2500} "
                    };

                    let mut spans = vec![
                        Span::styled(connector, Style::default().fg(theme::BORDER)),
                        Span::styled(
                            format!("{} ", icon),
                            Style::default().fg(color),
                        ),
                        Span::styled(
                            story.id.to_string(),
                            Style::default()
                                .fg(color)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(" {}", story.title),
                            Style::default().fg(color),
                        ),
                    ];

                    if !duration.is_empty() {
                        spans.push(Span::styled(
                            duration,
                            Style::default().fg(theme::SUCCESS),
                        ));
                    }

                    if !story.depends_on.is_empty() {
                        spans.push(Span::styled(
                            format!("  \u{2190} {}", story.depends_on.join(", ")),
                            Style::default().fg(theme::MUTED),
                        ));
                    }

                    lines.push(Line::from(spans));

                    if let Some(ref err) = story.error {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "  \u{2502}        ",
                                Style::default().fg(theme::BORDER),
                            ),
                            Span::styled(
                                format!("\u{26a0} {}", err),
                                Style::default().fg(theme::ERROR),
                            ),
                        ]));
                    }
                }
            }

            // Arrow between levels
            if i < app.dag_levels.len() - 1 {
                lines.push(Line::from(Span::styled(
                    "  \u{2502}",
                    Style::default().fg(theme::BORDER),
                )));
                lines.push(Line::from(Span::styled(
                    "  \u{25bc}",
                    Style::default().fg(theme::ACCENT_DIM),
                )));
            }

            lines.push(Line::from(""));
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

    let total_lines = lines.len();
    let p = Paragraph::new(lines)
        .block(block)
        .scroll((app.dag_scroll_offset, 0));
    f.render_widget(p, area);

    // Scrollbar
    let inner_height = area.height.saturating_sub(2) as usize; // subtract block borders
    if total_lines > inner_height {
        let mut scrollbar_state = ScrollbarState::new(total_lines.saturating_sub(inner_height))
            .position(app.dag_scroll_offset as usize);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(theme::ACCENT_DIM));
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}
