mod app;
mod events;
mod screens;
mod theme;
mod ui;

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::time::Duration;

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use app::{App, Screen};
use events::BaroEvent;

enum AppEvent {
    Baro(BaroEvent),
    Key(crossterm::event::KeyEvent),
    StdinClosed,
    Tick,
}

fn open_tty() -> io::Result<std::fs::File> {
    OpenOptions::new().read(true).write(true).open("/dev/tty")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut tty = open_tty()?;
    enable_raw_mode()?;
    execute!(tty, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(tty);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    terminal.backend_mut().flush()?;

    if let Err(err) = result {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<std::fs::File>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    let (tx, mut rx) = mpsc::channel::<AppEvent>(256);

    // Stdin reader for execute mode (JSON events from pipe)
    let tx_stdin = tx.clone();
    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if let Ok(ev) = serde_json::from_str::<BaroEvent>(&line) {
                        if tx_stdin.send(AppEvent::Baro(ev)).await.is_err() { break; }
                    }
                }
                _ => { let _ = tx_stdin.send(AppEvent::StdinClosed).await; break; }
            }
        }
    });

    // Keyboard input from /dev/tty
    let tx_key = tx.clone();
    std::thread::spawn(move || loop {
        match crossterm::event::poll(Duration::from_millis(100)) {
            Ok(true) => {
                if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                    if tx_key.blocking_send(AppEvent::Key(key)).is_err() { break; }
                }
            }
            Ok(false) => {}
            Err(_) => break,
        }
    });

    // Tick timer
    let tx_tick = tx.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if tx_tick.send(AppEvent::Tick).await.is_err() { break; }
        }
    });

    loop {
        terminal.draw(|f| ui::render(f, &app))?;
        match rx.recv().await {
            Some(AppEvent::Baro(ev)) => app.handle_event(ev),
            Some(AppEvent::Key(key)) => {
                use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match app.screen {
                    Screen::Welcome => match key.code {
                        KeyCode::Esc => return Ok(()),
                        KeyCode::Enter => {
                            if !app.goal_input.is_empty() {
                                app.start_planning();
                            }
                        }
                        KeyCode::Char(c) => app.goal_input.push(c),
                        KeyCode::Backspace => { app.goal_input.pop(); }
                        KeyCode::Left | KeyCode::Right => app.toggle_planner(),
                        _ => {}
                    },
                    Screen::Planning => match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
                        _ => {}
                    },
                    Screen::Review => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Enter => app.start_execution(),
                        KeyCode::Up | KeyCode::Char('k') => app.review_prev(),
                        KeyCode::Down | KeyCode::Char('j') => app.review_next(),
                        _ => {}
                    },
                    Screen::Execute => match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('1') => app.global_tab = app::GlobalTab::Dashboard,
                        KeyCode::Char('2') => app.global_tab = app::GlobalTab::Dag,
                        KeyCode::Char('3') => app.global_tab = app::GlobalTab::Stats,
                        KeyCode::Tab => {
                            if key.modifiers.contains(KeyModifiers::SHIFT) { app.prev_log(); }
                            else { app.next_log(); }
                        }
                        KeyCode::BackTab => app.prev_log(),
                        KeyCode::Left => app.prev_tab(),
                        KeyCode::Right => app.next_tab(),
                        _ => {}
                    },
                }
            }
            Some(AppEvent::StdinClosed) => {
                if app.screen == Screen::Execute && !app.done {
                    app.done = true;
                }
            }
            Some(AppEvent::Tick) => { app.tick_count += 1; }
            None => break,
        }
    }
    Ok(())
}
