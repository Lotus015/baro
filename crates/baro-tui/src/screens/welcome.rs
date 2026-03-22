use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;

// Giant blocky letters - each is ~12 wide, 9 rows tall
// Using double-width block chars for maximum chunkiness
const LETTER_B: [&str; 9] = [
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  ",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  ",
    "\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  ",
];

const LETTER_A: [&str; 9] = [
    "   \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}   ",
    "  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  ",
    " \u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
];

const LETTER_R: [&str; 9] = [
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  ",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}   ",
    "\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588}  ",
    "\u{2588}\u{2588}\u{2588}   \u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
];

const LETTER_O: [&str; 9] = [
    "  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  ",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    "\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}\u{2588}\u{2588}\u{2588}",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588} ",
    "  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  ",
];

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
            Constraint::Length(9),   // Logo (9 rows)
            Constraint::Length(1),   // Spacer
            Constraint::Length(1),   // Tagline
            Constraint::Length(2),   // Spacer
            Constraint::Length(5),   // Goal input (tall like Claude Code)
            Constraint::Length(1),   // Spacer
            Constraint::Length(3),   // Planner selector
            Constraint::Length(2),   // Spacer
            Constraint::Length(1),   // Help text
            Constraint::Length(1),   // Version
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

    // ── Giant logo with animated rainbow ──
    let tick = app.tick_count as usize;
    let phase = tick / 3;

    let mut logo_lines: Vec<Line> = Vec::new();
    for row in 0..9 {
        let b_color = rainbow(phase + row);
        let a_color = rainbow(phase + 2 + row);
        let r_color = rainbow(phase + 4 + row);
        let o_color = rainbow(phase + 6 + row);

        logo_lines.push(Line::from(vec![
            Span::styled(
                LETTER_B[row].to_string(),
                Style::default().fg(b_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   ".to_string(), Style::default()),
            Span::styled(
                LETTER_A[row].to_string(),
                Style::default().fg(a_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   ".to_string(), Style::default()),
            Span::styled(
                LETTER_R[row].to_string(),
                Style::default().fg(r_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   ".to_string(), Style::default()),
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
        Span::styled("autonomous ", Style::default().fg(Color::Cyan)),
        Span::styled(
            "parallel ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("coding", Style::default().fg(Color::Cyan)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(tagline, chunks[3]);

    // ── Goal input (tall, like Claude Code) ──
    let input_width = (w - 10).min(100);
    let input_area = center(chunks[5], input_width);

    // C64-style blinking block cursor
    let cursor_visible = (app.tick_count / 5) % 2 == 0;
    let cursor_char = if cursor_visible { "\u{2588}" } else { " " };

    let display_text = if app.goal_input.is_empty() {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    " What do you want to build?  ".to_string(),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    cursor_char.to_string(),
                    Style::default()
                        .fg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
        ]
    } else {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!(" {}", &app.goal_input),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    cursor_char.to_string(),
                    Style::default()
                        .fg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
        ]
    };

    let input = Paragraph::new(display_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if app.goal_input.is_empty() {
                Color::Gray
            } else {
                Color::LightCyan
            }))
            .title(Span::styled(
                " Goal ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(input, input_area);

    // ── Planner selector ──
    let planner_area = center(chunks[7], input_width);
    let is_claude = app.planner == crate::app::Planner::Claude;

    let claude_style = if is_claude {
        Style::default()
            .fg(Color::LightCyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let openai_style = if !is_claude {
        Style::default()
            .fg(Color::LightCyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
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
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Gray))
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
    f.render_widget(help, chunks[9]);

    // ── Version ──
    let version = Paragraph::new(Line::from(Span::styled(
        "v0.3.8",
        Style::default().fg(Color::DarkGray),
    )))
    .alignment(Alignment::Center);
    f.render_widget(version, chunks[10]);
}
