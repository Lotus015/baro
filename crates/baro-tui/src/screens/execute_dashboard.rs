use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Tabs, Wrap,
    },
    Frame,
};

use crate::app::{App, StoryStatus};
use crate::theme;
use crate::utils::format_token_display;

pub fn render_dashboard(f: &mut Frame, app: &App, area: Rect) {
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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(2)])
        .split(area);

    let mut items: Vec<ListItem> = Vec::new();

    if app.dag_levels.is_empty() {
        for story in &app.stories {
            items.push(story_list_item(story, &app.push_results));
        }
    } else {
        for (i, level) in app.dag_levels.iter().enumerate() {
            items.push(ListItem::new(Line::from(Span::styled(
                format!(" Level {}:", i),
                Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
            ))));

            for story_id in level {
                if let Some(story) = app.stories.iter().find(|s| s.id == *story_id) {
                    items.push(story_list_item(story, &app.push_results));
                }
            }

            // Show review spinner after stories for this level
            if app.review_in_progress && app.review_level == i {
                let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
                let spinner = spinner_chars[(app.tick_count as usize) % spinner_chars.len()];
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("   {} Reviewing Level {}...", spinner, i),
                    Style::default().fg(ratatui::style::Color::Cyan),
                ))));
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
                Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(list, chunks[0]);

    // Stats area: wall time + token counter
    let elapsed = app.total_time_secs;
    let wall_time = format!(" ⏱ {}:{:02}", elapsed / 60, elapsed % 60);
    let token_line = format!(" {}", format_token_display(app.total_input_tokens, app.total_output_tokens));

    let stats = Paragraph::new(vec![
        Line::from(Span::styled(wall_time, Style::default().fg(theme::MUTED))),
        Line::from(Span::styled(token_line, Style::default().fg(theme::MUTED))),
    ]);
    f.render_widget(stats, chunks[1]);
}

fn story_list_item(
    story: &crate::app::StoryState,
    push_results: &[(String, bool, Option<String>)],
) -> ListItem<'static> {
    let (icon, style) = match &story.status {
        StoryStatus::Complete => ("✓", Style::default().fg(theme::SUCCESS)),
        StoryStatus::Running => ("▶", Style::default().fg(theme::WARNING)),
        StoryStatus::Failed => ("✗", Style::default().fg(theme::ERROR)),
        StoryStatus::Retrying(_) => ("↻", Style::default().fg(theme::WARNING)),
        StoryStatus::Skipped => ("⊘", Style::default().fg(theme::MUTED)),
        StoryStatus::Pending => ("○", Style::default().fg(theme::MUTED)),
    };

    let duration = story
        .duration_secs
        .map(|d| format!(" ({}:{:02})", d / 60, d % 60))
        .unwrap_or_default();

    let retry_info = match &story.status {
        StoryStatus::Retrying(n) => format!(" retry #{}", n),
        _ => String::new(),
    };

    let push_indicator = if story.status == StoryStatus::Complete {
        if let Some((_, success, _)) = push_results.iter().find(|(id, _, _)| id == &story.id) {
            if *success {
                Some(Span::styled(" ↑", Style::default().fg(theme::SUCCESS)))
            } else {
                Some(Span::styled(" ↑!", Style::default().fg(theme::ERROR)))
            }
        } else {
            None
        }
    } else {
        None
    };

    let mut spans = vec![
        Span::raw("   "),
        Span::styled(
            format!(
                "{} {}: {}{}{}",
                icon, story.id, story.title, duration, retry_info
            ),
            style,
        ),
    ];
    if let Some(indicator) = push_indicator {
        spans.push(indicator);
    }

    ListItem::new(Line::from(spans))
}

fn render_logs(f: &mut Frame, app: &App, area: Rect) {
    let active_ids = app.active_story_ids();

    if active_ids.is_empty() {
        if !app.review_logs.is_empty() {
            let total_logs = app.review_logs.len();
            let inner_height = area.height.saturating_sub(2) as usize;
            let skip = total_logs.saturating_sub(inner_height);
            let visible_logs: Vec<Line> = app.review_logs[skip..]
                .iter()
                .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(theme::TEXT))))
                .collect();

            let title = if app.review_in_progress {
                let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
                let spinner = spinner_chars[(app.tick_count as usize) % spinner_chars.len()];
                format!(" {} Review Level {} ", spinner, app.review_level)
            } else {
                format!(" Review Level {} (done) ", app.review_level)
            };

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ratatui::style::Color::Cyan))
                .title(Span::styled(
                    title,
                    Style::default()
                        .fg(ratatui::style::Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ));

            let p = Paragraph::new(visible_logs)
                .block(block)
                .wrap(Wrap { trim: false });
            f.render_widget(p, area);

            if total_logs > inner_height {
                let mut scrollbar_state =
                    ScrollbarState::new(total_logs.saturating_sub(inner_height)).position(skip);
                let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .style(Style::default().fg(ratatui::style::Color::Cyan));
                f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
            }
        } else {
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
                        Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
                    )),
            );
            f.render_widget(p, area);
        }
        return;
    }

    let log_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(4),
        ])
        .split(area);

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
                    Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD),
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

    let selected_id = active_ids
        .get(app.selected_log_index)
        .cloned()
        .unwrap_or_default();

    if let Some(story) = app.active_stories.get(&selected_id) {
        let total_logs = story.logs.len();
        let inner_height = log_chunks[1].height.saturating_sub(2) as usize;
        let skip = total_logs.saturating_sub(inner_height);
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

        // Log scrollbar
        if total_logs > inner_height {
            let mut scrollbar_state =
                ScrollbarState::new(total_logs.saturating_sub(inner_height)).position(skip);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(theme::WARNING_DIM));
            f.render_stateful_widget(scrollbar, log_chunks[1], &mut scrollbar_state);
        }
    }
}
