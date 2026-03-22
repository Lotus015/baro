use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;
use crate::theme;

// Big ASCII art logo - each line is a row
const LOGO: &[&str] = &[
    " ____    _    ____   ___  ",
    "| __ )  / \\  |  _ \\ / _ \\ ",
    "|  _ \\ / _ \\ | |_) | | | |",
    "| |_) / ___ \\|  _ <| |_| |",
    "|____/_/   \\_\\_| \\_\\\\___/ ",
];

// Gradient colors for logo rows (indigo -> purple -> violet)
fn logo_color(row: usize, tick: u64) -> ratatui::style::Color {
    // Subtle shifting gradient based on tick
    let phase = ((tick / 3) % 5) as usize;
    let idx = (row + phase) % 5;
    match idx {
        0 => ratatui::style::Color::Rgb(90, 90, 255),
        1 => ratatui::style::Color::Rgb(110, 80, 255),
        2 => ratatui::style::Color::Rgb(140, 65, 255),
        3 => ratatui::style::Color::Rgb(170, 55, 255),
        4 => ratatui::style::Color::Rgb(130, 70, 255),
        _ => theme::LOGO_1,
    }
}

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = area.width;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(2),        // Top padding (flexible)
            Constraint::Length(5),      // ASCII logo
            Constraint::Length(1),      // Tagline
            Constraint::Length(1),      // Spacer
            Constraint::Length(1),      // Dim separator line
            Constraint::Length(1),      // Spacer
            Constraint::Length(3),      // Goal input
            Constraint::Length(1),      // Spacer
            Constraint::Length(3),      // Planner selector
            Constraint::Length(2),      // Spacer
            Constraint::Length(1),      // Help text
            Constraint::Length(1),      // Version
            Constraint::Min(1),        // Bottom padding
        ])
        .split(area);

    let center = |area: Rect, width: u16| -> Rect {
        let pad = area.width.saturating_sub(width) / 2;
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(pad),
                Constraint::Length(width.min(area.width)),
                Constraint::Min(0),
            ])
            .split(area)[1]
    };

    // ── ASCII Logo with animated gradient ──
    let logo_lines: Vec<Line> = LOGO
        .iter()
        .enumerate()
        .map(|(i, line)| {
            Line::from(Span::styled(
                *line,
                Style::default()
                    .fg(logo_color(i, app.tick_count))
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect();

    let logo = Paragraph::new(logo_lines).alignment(Alignment::Center);
    f.render_widget(logo, chunks[1]);

    // ── Tagline ──
    let tagline = Paragraph::new(Line::from(vec![
        Span::styled("autonomous ", Style::default().fg(theme::MUTED)),
        Span::styled("parallel ", Style::default().fg(theme::ACCENT_DIM)),
        Span::styled("coding", Style::default().fg(theme::MUTED)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(tagline, chunks[2]);

    // ── Separator ──
    let sep_width = 40.min(w.saturating_sub(4));
    let sep_str: String = std::iter::repeat_n('\u{2500}', sep_width as usize).collect();
    let separator = Paragraph::new(Line::from(Span::styled(
        sep_str,
        Style::default().fg(theme::BORDER),
    )))
    .alignment(Alignment::Center);
    f.render_widget(separator, chunks[4]);

    // ── Goal input ──
    let input_width = 64.min(w.saturating_sub(4));
    let input_area = center(chunks[6], input_width);

    let display_text = if app.goal_input.is_empty() {
        Line::from(Span::styled(
            " Describe what you want to build...",
            Style::default().fg(theme::MUTED),
        ))
    } else {
        Line::from(Span::styled(
            format!(" {}", &app.goal_input),
            Style::default().fg(theme::TEXT),
        ))
    };

    let border_color = if app.goal_input.is_empty() {
        theme::BORDER
    } else {
        theme::ACCENT
    };

    let input = Paragraph::new(display_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                " Goal ",
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(input, input_area);

    // Cursor
    let cursor_x = input_area.x + 2 + app.goal_input.len() as u16;
    let cursor_y = input_area.y + 1;
    if cursor_x < input_area.x + input_area.width - 1 {
        f.set_cursor_position((cursor_x, cursor_y));
    }

    // ── Planner selector (pill-style toggle) ──
    let planner_area = center(chunks[8], input_width);

    let is_claude = app.planner == crate::app::Planner::Claude;

    let claude_style = if is_claude {
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::MUTED)
    };
    let openai_style = if !is_claude {
        Style::default()
            .fg(theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::MUTED)
    };

    let claude_marker = if is_claude { "\u{25c9}" } else { "\u{25cb}" }; // ◉ vs ○
    let openai_marker = if !is_claude { "\u{25c9}" } else { "\u{25cb}" };

    let planner = Paragraph::new(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(format!("{} ", claude_marker), claude_style),
        Span::styled("Claude", claude_style),
        Span::styled("     ", Style::default()),
        Span::styled(format!("{} ", openai_marker), openai_style),
        Span::styled("OpenAI", openai_style),
        Span::styled("           ", Style::default()),
        Span::styled(
            "\u{2190}\u{2192} switch",
            Style::default().fg(theme::MUTED),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .title(Span::styled(
                " Planner ",
                Style::default().fg(theme::TEXT_DIM),
            )),
    );
    f.render_widget(planner, planner_area);

    // ── Keybinds ──
    let help = Paragraph::new(Line::from(vec![
        Span::styled(
            "Enter",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" start   ", Style::default().fg(theme::MUTED)),
        Span::styled(
            "Esc",
            Style::default()
                .fg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" quit", Style::default().fg(theme::MUTED)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(help, chunks[10]);

    // ── Version ──
    let version = Paragraph::new(Line::from(Span::styled(
        "v0.3.0",
        Style::default().fg(theme::BORDER),
    )))
    .alignment(Alignment::Center);
    f.render_widget(version, chunks[11]);
}
