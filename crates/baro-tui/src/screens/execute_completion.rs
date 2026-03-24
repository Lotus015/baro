use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::theme;

pub fn render_completion(f: &mut Frame, app: &App) {
    let area = f.area();
    let box_width = 50u16.min(area.width.saturating_sub(4));
    let pr_extra: u16 = if app.pr_url.is_some() { 1 } else { 0 };
    let box_height = (15u16 + pr_extra).min(area.height.saturating_sub(2));
    let x = (area.width.saturating_sub(box_width)) / 2;
    let y = (area.height.saturating_sub(box_height)) / 2;
    let popup_area = Rect::new(x, y, box_width, box_height);

    f.render_widget(Clear, popup_area);

    let stats = app.final_stats.as_ref();
    let completed = stats.map(|s| s.stories_completed).unwrap_or(app.completed);
    let skipped = stats.map(|s| s.stories_skipped).unwrap_or(0);
    let total_time = app.total_time_secs;
    let files_created: u32 = stats
        .map(|s| s.files_created)
        .unwrap_or_else(|| app.stories.iter().map(|s| s.files_created).sum());
    let files_modified: u32 = stats
        .map(|s| s.files_modified)
        .unwrap_or_else(|| app.stories.iter().map(|s| s.files_modified).sum());

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "ALL STORIES COMPLETE",
            Style::default()
                .fg(theme::SUCCESS)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Total stories:  ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}", app.total),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Completed:      ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}", completed),
                Style::default()
                    .fg(theme::SUCCESS)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Skipped:        ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}", skipped),
                Style::default().fg(if skipped > 0 {
                    theme::WARNING
                } else {
                    theme::SUCCESS
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Total time:     ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{:02}:{:02}", total_time / 60, total_time % 60),
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Files created:  ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}", files_created),
                Style::default().fg(theme::ACCENT),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Files modified: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!("{}", files_modified),
                Style::default().fg(theme::ACCENT),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Pushed:         ", Style::default().fg(theme::MUTED)),
            Span::styled(
                {
                    let pushed_ok = app.push_results.iter().filter(|(_, ok, _)| *ok).count();
                    let push_total = app.push_results.len();
                    format!("{}/{} stories", pushed_ok, push_total)
                },
                Style::default().fg(
                    if app.push_results.iter().all(|(_, ok, _)| *ok) && !app.push_results.is_empty()
                    {
                        theme::SUCCESS
                    } else {
                        theme::WARNING
                    },
                ),
            ),
        ]),
    ];

    if let Some(ref url) = app.pr_url {
        lines.push(Line::from(vec![
            Span::styled("  PR: ", Style::default().fg(theme::MUTED)),
            Span::styled(url.clone(), Style::default().fg(theme::ACCENT)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("q quit", Style::default().fg(theme::MUTED))));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::SUCCESS))
        .title(Span::styled(
            " Complete ",
            Style::default()
                .fg(theme::SUCCESS)
                .add_modifier(Modifier::BOLD),
        ));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);

    f.render_widget(paragraph, popup_area);
}
