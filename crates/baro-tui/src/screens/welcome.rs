use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;

// Each letter defined separately for individual coloring
const LETTER_B: [&str; 7] = [
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
];

const LETTER_A: [&str; 7] = [
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
];

const LETTER_R: [&str; 7] = [
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}\u{2580}  \u{2588}\u{2588}",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
    "\u{2588}\u{2588}   \u{2588}\u{2588}",
];

const LETTER_O: [&str; 7] = [
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}    \u{2588}\u{2588}",
    "\u{2588}\u{2588}    \u{2588}\u{2588}",
    "\u{2588}\u{2588}    \u{2588}\u{2588}",
    "\u{2588}\u{2588}    \u{2588}\u{2588}",
    "\u{2588}\u{2588}    \u{2588}\u{2588}",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
];

// Use ANSI colors that work in ALL terminals (including macOS Terminal.app)
// Color::Rgb does NOT work in Terminal.app - it renders as gray
fn rainbow(idx: usize) -> Color {
    match idx % 7 {
        0 => Color::LightRed,
        1 => Color::LightYellow,
        2 => Color::LightGreen,
        3 => Color::LightCyan,
        4 => Color::LightBlue,
        5 => Color::LightMagenta,
        6 => Color::Yellow,
        _ => Color::White,
    }
}

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let w = area.width;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(7),  // Logo
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Tagline
            Constraint::Length(1),  // Spacer
            Constraint::Length(1),  // Separator
            Constraint::Length(1),  // Spacer
            Constraint::Length(3),  // Goal input
            Constraint::Length(1),  // Spacer
            Constraint::Length(3),  // Planner selector
            Constraint::Length(2),  // Spacer
            Constraint::Length(1),  // Help text
            Constraint::Length(1),  // Version
            Constraint::Min(1),
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

    // ── Logo: each letter gets its own animated color ──
    let tick = app.tick_count as usize;
    let phase = tick / 3;

    let mut logo_lines: Vec<Line> = Vec::new();
    for row in 0..7 {
        let b_color = rainbow(phase + 0 + row);
        let a_color = rainbow(phase + 2 + row);
        let r_color = rainbow(phase + 4 + row);
        let o_color = rainbow(phase + 6 + row);

        logo_lines.push(Line::from(vec![
            Span::styled(
                LETTER_B[row].to_string(),
                Style::default().fg(b_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ".to_string(), Style::default()),
            Span::styled(
                LETTER_A[row].to_string(),
                Style::default().fg(a_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ".to_string(), Style::default()),
            Span::styled(
                LETTER_R[row].to_string(),
                Style::default().fg(r_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ".to_string(), Style::default()),
            Span::styled(
                LETTER_O[row].to_string(),
                Style::default().fg(o_color).add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    let logo = Paragraph::new(logo_lines).alignment(Alignment::Center);
    f.render_widget(logo, chunks[1]);

    // ── Tagline ──
    let tagline = Paragraph::new(Line::from(vec![
        Span::styled(
            "autonomous ",
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            "parallel ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "coding",
            Style::default().fg(Color::Cyan),
        ),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(tagline, chunks[3]);

    // ── Separator ──
    let sep_width = 50.min(w.saturating_sub(4));
    let sep_str: String = std::iter::repeat_n('\u{2550}', sep_width as usize).collect();
    let separator = Paragraph::new(Line::from(Span::styled(
        sep_str,
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center);
    f.render_widget(separator, chunks[5]);

    // ── Goal input ──
    let input_width = 70.min(w.saturating_sub(4));
    let input_area = center(chunks[7], input_width);

    let display_text = if app.goal_input.is_empty() {
        Line::from(Span::styled(
            " Describe what you want to build...",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(Span::styled(
            format!(" {}", &app.goal_input),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
    };

    let border_color = if app.goal_input.is_empty() {
        Color::Blue
    } else {
        Color::White
    };

    let input = Paragraph::new(display_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                " Goal ",
                Style::default()
                    .fg(Color::White)
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

    // ── Planner selector ──
    let planner_area = center(chunks[9], input_width);
    let is_claude = app.planner == crate::app::Planner::Claude;

    let active_color = match (app.tick_count / 6) % 3 {
        0 => Color::LightCyan,
        1 => Color::LightBlue,
        _ => Color::LightMagenta,
    };

    let claude_style = if is_claude {
        Style::default()
            .fg(active_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let openai_style = if !is_claude {
        Style::default()
            .fg(active_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let claude_marker = if is_claude { "\u{25c9}" } else { "\u{25cb}" };
    let openai_marker = if !is_claude { "\u{25c9}" } else { "\u{25cb}" };

    let planner = Paragraph::new(Line::from(vec![
        Span::styled("  ".to_string(), Style::default()),
        Span::styled(format!("{} ", claude_marker), claude_style),
        Span::styled("Claude".to_string(), claude_style),
        Span::styled("        ".to_string(), Style::default()),
        Span::styled(format!("{} ", openai_marker), openai_style),
        Span::styled("OpenAI".to_string(), openai_style),
        Span::styled("              ".to_string(), Style::default()),
        Span::styled(
            "\u{2190}\u{2192} switch".to_string(),
            Style::default().fg(Color::Gray),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue))
            .title(Span::styled(
                " Planner ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(planner, planner_area);

    // ── Keybinds ──
    let help = Paragraph::new(Line::from(vec![
        Span::styled(
            "Enter",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" start   ", Style::default().fg(Color::Gray)),
        Span::styled(
            "Esc",
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" quit", Style::default().fg(Color::Gray)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(help, chunks[11]);

    // ── Version ──
    let version = Paragraph::new(Line::from(Span::styled(
        "v0.3.7",
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center);
    f.render_widget(version, chunks[12]);
}
