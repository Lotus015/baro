use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;

// Each letter defined separately for individual coloring
// B letter (7 rows, 8 cols)
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

// Rainbow color cycle
fn rainbow(idx: usize) -> Color {
    match idx % 8 {
        0 => Color::Rgb(255, 80, 80),   // red
        1 => Color::Rgb(255, 160, 50),  // orange
        2 => Color::Rgb(255, 220, 50),  // yellow
        3 => Color::Rgb(80, 255, 120),  // green
        4 => Color::Rgb(80, 220, 255),  // cyan
        5 => Color::Rgb(100, 120, 255), // blue
        6 => Color::Rgb(180, 80, 255),  // purple
        7 => Color::Rgb(255, 80, 200),  // pink
        _ => Color::White,
    }
}

const BRIGHT_WHITE: Color = Color::Rgb(255, 255, 255);
const SOFT_WHITE: Color = Color::Rgb(220, 220, 230);
const LIGHT_BLUE: Color = Color::Rgb(150, 180, 255);
const BRIGHT_CYAN: Color = Color::Rgb(80, 220, 255);
const BRIGHT_GREEN: Color = Color::Rgb(80, 255, 160);
const DIM_BLUE: Color = Color::Rgb(60, 80, 140);

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

    // ── Logo: 4 letters, each gets its own color that shifts over time ──
    let tick = app.tick_count as usize;
    let phase = tick / 3; // color shifts every 300ms

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
            Span::styled("  ", Style::default()),
            Span::styled(
                LETTER_A[row].to_string(),
                Style::default().fg(a_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(
                LETTER_R[row].to_string(),
                Style::default().fg(r_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
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
        Span::styled("autonomous ", Style::default().fg(BRIGHT_CYAN)),
        Span::styled(
            "parallel ",
            Style::default()
                .fg(BRIGHT_WHITE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("coding", Style::default().fg(BRIGHT_CYAN)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(tagline, chunks[3]);

    // ── Separator ──
    let sep_width = 50.min(w.saturating_sub(4));
    let sep_str: String = std::iter::repeat_n('\u{2550}', sep_width as usize).collect();
    let separator = Paragraph::new(Line::from(Span::styled(
        sep_str,
        Style::default().fg(DIM_BLUE),
    )))
    .alignment(Alignment::Center);
    f.render_widget(separator, chunks[5]);

    // ── Goal input ──
    let input_width = 70.min(w.saturating_sub(4));
    let input_area = center(chunks[7], input_width);

    let display_text = if app.goal_input.is_empty() {
        Line::from(Span::styled(
            " Describe what you want to build...",
            Style::default().fg(LIGHT_BLUE),
        ))
    } else {
        Line::from(Span::styled(
            format!(" {}", &app.goal_input),
            Style::default()
                .fg(BRIGHT_WHITE)
                .add_modifier(Modifier::BOLD),
        ))
    };

    let border_color = if app.goal_input.is_empty() {
        LIGHT_BLUE
    } else {
        BRIGHT_WHITE
    };

    let input = Paragraph::new(display_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                " Goal ",
                Style::default()
                    .fg(BRIGHT_WHITE)
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
        0 => Color::Rgb(100, 200, 255),
        1 => Color::Rgb(150, 150, 255),
        _ => Color::Rgb(200, 130, 255),
    };

    let claude_style = if is_claude {
        Style::default()
            .fg(active_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(SOFT_WHITE)
    };
    let openai_style = if !is_claude {
        Style::default()
            .fg(active_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(SOFT_WHITE)
    };

    let claude_marker = if is_claude { "\u{25c9}" } else { "\u{25cb}" };
    let openai_marker = if !is_claude { "\u{25c9}" } else { "\u{25cb}" };

    let planner = Paragraph::new(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(format!("{} ", claude_marker), claude_style),
        Span::styled("Claude", claude_style),
        Span::styled("        ", Style::default()),
        Span::styled(format!("{} ", openai_marker), openai_style),
        Span::styled("OpenAI", openai_style),
        Span::styled("              ", Style::default()),
        Span::styled(
            "\u{2190}\u{2192} switch",
            Style::default().fg(SOFT_WHITE),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(LIGHT_BLUE))
            .title(Span::styled(
                " Planner ",
                Style::default()
                    .fg(BRIGHT_WHITE)
                    .add_modifier(Modifier::BOLD),
            )),
    );
    f.render_widget(planner, planner_area);

    // ── Keybinds ──
    let help = Paragraph::new(Line::from(vec![
        Span::styled(
            "Enter",
            Style::default()
                .fg(BRIGHT_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" start   ", Style::default().fg(SOFT_WHITE)),
        Span::styled(
            "Esc",
            Style::default()
                .fg(Color::Rgb(255, 100, 100))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" quit", Style::default().fg(SOFT_WHITE)),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(help, chunks[11]);

    // ── Version ──
    let version = Paragraph::new(Line::from(Span::styled(
        "v0.3.6",
        Style::default().fg(DIM_BLUE),
    )))
    .alignment(Alignment::Center);
    f.render_widget(version, chunks[12]);
}
