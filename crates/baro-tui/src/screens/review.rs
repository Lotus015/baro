use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame,
};

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(8),   // Plan content (scrollable)
            Constraint::Length(1), // Footer
        ])
        .split(area);

    // Header
    let story_count = app.review_stories.len();
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " BARO ",
            Style::default()
                .fg(theme::LOGO_1)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" | ", Style::default().fg(theme::BORDER)),
        Span::styled(
            "Plan Review",
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" | ", Style::default().fg(theme::BORDER)),
        Span::styled(
            format!("{} stories", story_count),
            Style::default().fg(theme::ACCENT),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER)),
    );
    f.render_widget(header, chunks[0]);

    // Plan content
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    if app.review_stories.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No stories in plan.",
            Style::default().fg(theme::MUTED),
        )));
    } else {
        for (i, story) in app.review_stories.iter().enumerate() {
            let is_selected = i == app.review_scroll;

            let marker = if is_selected { "\u{25b6}" } else { " " };
            let marker_style = if is_selected {
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::MUTED)
            };

            let title_style = if is_selected {
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_DIM)
            };

            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", marker), marker_style),
                Span::styled(
                    format!("{}: ", story.id),
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(&story.title, title_style),
            ]));

            if !story.description.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("     "),
                    Span::styled(&story.description, Style::default().fg(theme::MUTED)),
                ]));
            }

            if !story.depends_on.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("     "),
                    Span::styled(
                        format!("\u{2514} deps: {}", story.depends_on.join(", ")),
                        Style::default().fg(theme::ACCENT_DIM),
                    ),
                ]));
            }

            lines.push(Line::from(""));
        }
    }

    let inner_height = chunks[1].height.saturating_sub(2) as usize;
    let total_lines = lines.len();
    let scroll_offset = if app.review_scroll * 4 > inner_height {
        (app.review_scroll * 4).saturating_sub(inner_height / 2)
    } else {
        0
    };

    let plan = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER))
                .title(Span::styled(
                    " Plan ",
                    Style::default()
                        .fg(theme::ACCENT)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .scroll((scroll_offset as u16, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(plan, chunks[1]);

    // Scrollbar
    if total_lines > inner_height {
        let mut scrollbar_state = ScrollbarState::new(total_lines.saturating_sub(inner_height))
            .position(scroll_offset);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(theme::ACCENT_DIM))
            .begin_symbol(Some("\u{25b2}"))
            .end_symbol(Some("\u{25bc}"));
        f.render_stateful_widget(scrollbar, chunks[1], &mut scrollbar_state);
    }

    // Footer
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(
            "Enter",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":accept  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "\u{2191}/\u{2193}",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":scroll  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "r",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":refine  ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "q",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(":quit", Style::default().fg(theme::MUTED)),
    ]));
    f.render_widget(footer, chunks[2]);
}
