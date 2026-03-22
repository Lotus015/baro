use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::App;
use crate::theme;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Length(3),  // Spinner + status
            Constraint::Length(2),  // Goal
            Constraint::Length(2),  // Elapsed
            Constraint::Min(0),
        ])
        .split(area);

    let center = |area: Rect, width: u16| -> Rect {
        let pad = area.width.saturating_sub(width) / 2;
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(pad),
                Constraint::Length(width),
                Constraint::Min(0),
            ])
            .split(area)[1]
    };

    // Spinner
    let frame_idx = (app.tick_count / 2) as usize % SPINNER_FRAMES.len();
    let spinner = SPINNER_FRAMES[frame_idx];

    let planner_name = match app.planner {
        crate::app::Planner::Claude => "Claude",
        crate::app::Planner::OpenAI => "OpenAI",
    };

    let status = Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {} ", spinner),
            Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("Planning with {}...", planner_name),
            Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        ),
    ]));
    f.render_widget(status, center(chunks[1], 40));

    // Goal
    let goal_display = if app.goal_input.len() > 60 {
        format!("{}...", &app.goal_input[..57])
    } else {
        app.goal_input.clone()
    };
    let goal = Paragraph::new(Line::from(vec![
        Span::styled("Goal: ", Style::default().fg(theme::MUTED)),
        Span::styled(goal_display, Style::default().fg(theme::TEXT_DIM)),
    ]));
    f.render_widget(goal, center(chunks[2], 70));

    // Elapsed
    let elapsed = app.planning_elapsed_secs();
    let elapsed_text = Paragraph::new(Line::from(vec![
        Span::styled("Elapsed: ", Style::default().fg(theme::MUTED)),
        Span::styled(
            format!("{}:{:02}", elapsed / 60, elapsed % 60),
            Style::default().fg(theme::ACCENT),
        ),
    ]));
    f.render_widget(elapsed_text, center(chunks[3], 20));
}
