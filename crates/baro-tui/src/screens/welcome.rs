use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;

// Massive C64-style blocky logo using full block characters
// Each letter is 8 chars wide, 7 rows tall
const LOGO: &[&str] = &[
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}   \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}   \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}   \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  ",
    " \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}    \u{2588}\u{2588} ",
    " \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}    \u{2588}\u{2588} ",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}   \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}   \u{2588}\u{2588}    \u{2588}\u{2588} ",
    " \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}\u{2580}  \u{2588}\u{2588}  \u{2588}\u{2588}    \u{2588}\u{2588} ",
    " \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}    \u{2588}\u{2588} ",
    " \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}   \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}   \u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  ",
];

// C64-inspired bright color palette - animated rainbow cycle
fn logo_color(row: usize, col: usize, tick: u64) -> Color {
    let phase = (tick / 2) as usize;
    let idx = (row + col / 4 + phase) % 8;
    match idx {
        0 => Color::Rgb(100, 100, 255), // blue
        1 => Color::Rgb(140, 80, 255),  // purple
        2 => Color::Rgb(200, 60, 255),  // magenta
        3 => Color::Rgb(255, 80, 180),  // pink
        4 => Color::Rgb(255, 120, 80),  // orange
        5 => Color::Rgb(255, 200, 50),  // yellow
        6 => Color::Rgb(80, 255, 120),  // green
        7 => Color::Rgb(80, 200, 255),  // cyan
        _ => Color::White,
    }
}

// Per-character colored line for the logo
fn colored_logo_line<'a>(line: &'a str, row: usize, tick: u64) -> Line<'a> {
    let spans: Vec<Span> = line
        .char_indices()
        .map(|(i, ch)| {
            let color = if ch == ' ' {
                Color::Reset
            } else {
                logo_color(row, i, tick)
            };
            Span::styled(
                &line[i..i + ch.len_utf8()],
                Style::default()
                    .fg(color)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    Line::from(spans)
}

// Bright white with full intensity
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
            Constraint::Min(1),        // Top padding
            Constraint::Length(7),      // Logo (7 rows)
            Constraint::Length(1),      // Spacer
            Constraint::Length(1),      // Tagline
            Constraint::Length(1),      // Spacer
            Constraint::Length(1),      // Separator
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

    // ── Massive C64 logo with per-character rainbow animation ──
    let logo_lines: Vec<Line> = LOGO
        .iter()
        .enumerate()
        .map(|(i, line)| colored_logo_line(line, i, app.tick_count))
        .collect();

    let logo = Paragraph::new(logo_lines).alignment(Alignment::Center);
    f.render_widget(logo, chunks[1]);

    // ── Tagline - bright and punchy ──
    let tagline = Paragraph::new(Line::from(vec![
        Span::styled(
            "autonomous ",
            Style::default().fg(BRIGHT_CYAN),
        ),
        Span::styled(
            "parallel ",
            Style::default()
                .fg(BRIGHT_WHITE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "coding",
            Style::default().fg(BRIGHT_CYAN),
        ),
    ]))
    .alignment(Alignment::Center);
    f.render_widget(tagline, chunks[3]);

    // ── Separator - bright line ──
    let sep_width = 50.min(w.saturating_sub(4));
    let sep_str: String = std::iter::repeat_n('\u{2550}', sep_width as usize).collect();
    let separator = Paragraph::new(Line::from(Span::styled(
        sep_str,
        Style::default().fg(DIM_BLUE),
    )))
    .alignment(Alignment::Center);
    f.render_widget(separator, chunks[5]);

    // ── Goal input - bright white borders ──
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

    // ── Planner selector - bright white with colored active state ──
    let planner_area = center(chunks[9], input_width);
    let is_claude = app.planner == crate::app::Planner::Claude;

    // Active planner gets a bright animated color
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

    // ── Keybinds - bright ──
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
        "v0.3.5",
        Style::default().fg(DIM_BLUE),
    )))
    .alignment(Alignment::Center);
    f.render_widget(version, chunks[12]);
}
