use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::App;
use crate::theme;
use crate::utils::format_token_display;

pub fn render_completion(f: &mut Frame, app: &App) {
    let area = f.area();
    let box_width = 50u16.min(area.width.saturating_sub(4));
    let pr_extra: u16 = if app.pr_url.is_some() { 1 } else { 0 };
    let box_height = (17u16 + pr_extra).min(area.height.saturating_sub(2));
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

    // Calculate time saved using DAG levels:
    // For each level, sequential = sum of all story durations, parallel = max story duration
    // Time saved = sum of (sequential - parallel) per level
    let (saved_secs, sequential_time) = {
        let mut total_sequential = 0u64;
        let mut total_parallel = 0u64;

        for level in &app.dag_levels {
            let mut level_sum = 0u64;
            let mut level_max = 0u64;
            for story_id in level {
                if let Some(story) = app.stories.iter().find(|s| s.id == *story_id) {
                    if let Some(dur) = story.duration_secs {
                        level_sum += dur;
                        level_max = level_max.max(dur);
                    }
                }
            }
            total_sequential += level_sum;
            total_parallel += level_max;
        }

        // Also count fix stories not in original DAG
        for story in &app.stories {
            if story.id.contains("-fix") {
                if let Some(dur) = story.duration_secs {
                    total_sequential += dur;
                    total_parallel += dur; // fix stories run sequentially
                }
            }
        }

        let saved = total_sequential.saturating_sub(total_parallel);
        (saved, total_sequential)
    };

    let multiplier = if sequential_time > 0 && saved_secs > 0 {
        let parallel_time = sequential_time.saturating_sub(saved_secs);
        if parallel_time > 0 {
            sequential_time as f64 / parallel_time as f64
        } else {
            1.0
        }
    } else {
        1.0
    };

    let mut lines = vec![
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
    ];

    if saved_secs > 0 {
        lines.push(Line::from(vec![
            Span::styled("  Time saved:     ", Style::default().fg(theme::MUTED)),
            Span::styled(
                format!(
                    "{}:{:02} with parallel execution ({:.1}x speedup)",
                    saved_secs / 60,
                    saved_secs % 60,
                    multiplier
                ),
                Style::default()
                    .fg(theme::SUCCESS)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    lines.extend(vec![
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
    ]);

    lines.push(Line::from(vec![
        Span::styled("  Model:          ", Style::default().fg(theme::MUTED)),
        Span::styled(
            if let Some(ref name) = app.override_model {
                name.to_string()
            } else if app.model_routing {
                "routed".to_string()
            } else {
                "default".to_string()
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
    ]));

    lines.push(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            format_token_display(app.total_input_tokens, app.total_output_tokens),
            Style::default().fg(theme::ACCENT),
        ),
    ]));

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
