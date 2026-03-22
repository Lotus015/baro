use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Length(3),  // Logo
            Constraint::Length(2),  // Spacer
            Constraint::Length(3),  // Goal input
            Constraint::Length(2),  // Spacer
            Constraint::Length(3),  // Planner selector
            Constraint::Length(2),  // Spacer
            Constraint::Length(1),  // Help text
            Constraint::Min(0),
        ])
        .split(area);

    // Center horizontally
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

    // Logo
    let logo = Paragraph::new(Line::from(vec![
        Span::styled("B", Style::default().fg(theme::LOGO_1).add_modifier(Modifier::BOLD)),
        Span::styled("A", Style::default().fg(theme::LOGO_2).add_modifier(Modifier::BOLD)),
        Span::styled("R", Style::default().fg(theme::LOGO_3).add_modifier(Modifier::BOLD)),
        Span::styled("O", Style::default().fg(theme::LOGO_1).add_modifier(Modifier::BOLD)),
    ]));
    f.render_widget(logo, center(chunks[1], 10));

    // Goal input
    let input_width = 60.min(area.width.saturating_sub(4));
    let input_area = center(chunks[3], input_width);

    let display_text = if app.goal_input.is_empty() {
        Span::styled("Enter your goal...", Style::default().fg(theme::MUTED))
    } else {
        Span::styled(&app.goal_input, Style::default().fg(theme::TEXT))
    };

    let input = Paragraph::new(Line::from(display_text)).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if app.goal_input.is_empty() {
                theme::BORDER
            } else {
                theme::ACCENT
            }))
            .title(Span::styled(
                " Goal ",
                Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(input, input_area);

    // Show cursor position
    if !app.goal_input.is_empty() || true {
        let cursor_x = input_area.x + 1 + app.goal_input.len() as u16;
        let cursor_y = input_area.y + 1;
        if cursor_x < input_area.x + input_area.width - 1 {
            f.set_cursor_position((cursor_x, cursor_y));
        }
    }

    // Planner selector
    let planner_area = center(chunks[5], input_width);
    let planner_label = match app.planner {
        crate::app::Planner::Claude => "claude",
        crate::app::Planner::OpenAI => "openai",
    };

    let planner = Paragraph::new(Line::from(vec![
        Span::styled(" Planner: ", Style::default().fg(theme::MUTED)),
        Span::styled(
            planner_label,
            Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  (left/right to change)",
            Style::default().fg(theme::MUTED),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER)),
    );
    f.render_widget(planner, planner_area);

    // Help text
    let help = Paragraph::new(Line::from(vec![
        Span::styled("Enter", Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(":start  ", Style::default().fg(theme::MUTED)),
        Span::styled("Esc", Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(":quit", Style::default().fg(theme::MUTED)),
    ]));
    f.render_widget(help, center(chunks[7], 30));
}
