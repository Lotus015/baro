mod app;
mod events;
mod screens;
mod theme;
mod ui;

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::time::Duration;

use clap::Parser;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::process::Command;
use tokio::sync::mpsc;

use app::{App, Planner, ReviewStory, Screen};
#[allow(unused_imports)]
use events::BaroEvent;

#[derive(Parser)]
#[command(name = "baro", about = "AI-powered project execution")]
struct Cli {
    /// Project goal (if omitted, shows welcome screen)
    goal: Option<String>,

    /// Planner to use
    #[arg(long, default_value = "claude", value_parser = ["claude", "openai"])]
    planner: String,
}

#[allow(dead_code)]
enum AppEvent {
    Baro(BaroEvent),
    Key(crossterm::event::KeyEvent),
    PlanReady(Vec<ReviewStory>, String, String),
    PlanError(String),
    StdinClosed,
    Tick,
}

fn open_tty() -> io::Result<std::fs::File> {
    OpenOptions::new().read(true).write(true).open("/dev/tty")
}

const CLAUDE_PLANNER_PROMPT: &str = r#"You are an expert software architect. Break down the user's project goal into concrete user stories that form a dependency DAG.

You MUST explore the existing codebase first using your tools (read files, list directories, etc.) before generating the plan.

Output ONLY valid JSON matching this exact schema (no markdown, no explanation, just JSON):
{
  "project": "short project name",
  "branchName": "kebab-case-branch-name",
  "description": "one-line description",
  "userStories": [
    {
      "id": "S1",
      "priority": 1,
      "title": "short title",
      "description": "what to implement",
      "dependsOn": [],
      "retries": 2,
      "acceptance": ["testable criterion"],
      "tests": ["npm test"]
    }
  ]
}

Rules:
- Each story: single focused unit of work for one AI agent
- Use dependsOn for dependencies; same-priority stories with no deps run IN PARALLEL
- Keep stories small (15-60 min of work each)
- Include testable acceptance criteria and test commands
- No circular dependencies
- Start with foundational stories, build up
- IDs: S1, S2, S3...
- Build on existing code, don't recreate what exists
- Output ONLY the JSON, nothing else"#;

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct PrdOutput {
    project: String,
    #[serde(default)]
    #[serde(rename = "branchName")]
    branch_name: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "userStories")]
    user_stories: Vec<PrdStory>,
}

#[derive(serde::Deserialize)]
struct PrdStory {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    #[serde(rename = "dependsOn")]
    depends_on: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let mut tty = open_tty()?;
    enable_raw_mode()?;
    execute!(tty, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(tty);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, cli).await;

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
    cli: Cli,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();

    // Apply CLI args
    app.planner = match cli.planner.as_str() {
        "openai" => Planner::OpenAI,
        _ => Planner::Claude,
    };

    let (tx, mut rx) = mpsc::channel::<AppEvent>(256);

    // If goal provided via CLI, skip welcome and start planning immediately
    if let Some(goal) = cli.goal {
        app.goal_input = goal;
        app.start_planning();
        spawn_planner(&app, tx.clone());
    }

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
            Some(AppEvent::PlanReady(stories, project, branch)) => {
                app.project = project;
                app.branch_name = branch;
                app.show_review(stories);
            }
            Some(AppEvent::PlanError(err)) => {
                app.planning_error = Some(err);
            }
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
                                spawn_planner(&app, tx.clone());
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
                        KeyCode::Enter => {
                            app.start_execution();
                            // TODO: spawn executor processes
                        }
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

fn spawn_planner(app: &App, tx: mpsc::Sender<AppEvent>) {
    let goal = app.goal_input.clone();
    let planner = app.planner;

    tokio::spawn(async move {
        let result = match planner {
            Planner::Claude => run_claude_planner(&goal).await,
            Planner::OpenAI => run_openai_planner(&goal).await,
        };

        match result {
            Ok((stories, project, branch)) => {
                let _ = tx.send(AppEvent::PlanReady(stories, project, branch)).await;
            }
            Err(e) => {
                let _ = tx.send(AppEvent::PlanError(e.to_string())).await;
            }
        }
    });
}

async fn run_claude_planner(goal: &str) -> Result<(Vec<ReviewStory>, String, String), Box<dyn std::error::Error + Send + Sync>> {
    let prompt = format!("{}\n\nUser goal: {}", CLAUDE_PLANNER_PROMPT, goal);

    let output = Command::new("claude")
        .args([
            "--dangerously-skip-permissions",
            "--output-format", "json",
            "-p",
            &prompt,
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?
        .wait_with_output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Claude exited with {}: {}", output.status, stderr).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Claude --output-format json wraps the result; extract the text content
    let claude_output: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse Claude JSON wrapper: {}", e))?;

    // The actual plan JSON is in the "result" field as a text string
    let plan_text = claude_output
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or(&stdout);

    // Try to extract JSON from the text (Claude sometimes wraps in markdown)
    let json_str = extract_json(plan_text);

    let prd: PrdOutput = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse PRD JSON: {}\nRaw: {}", e, &json_str[..json_str.len().min(500)]))?;

    let stories: Vec<ReviewStory> = prd.user_stories
        .into_iter()
        .map(|s| ReviewStory {
            id: s.id,
            title: s.title,
            description: s.description,
            depends_on: s.depends_on,
        })
        .collect();

    Ok((stories, prd.project, prd.branch_name))
}

async fn run_openai_planner(goal: &str) -> Result<(Vec<ReviewStory>, String, String), Box<dyn std::error::Error + Send + Sync>> {
    // Spawn the TS OpenAI planner
    let cwd = std::env::current_dir()?;
    let output = Command::new("node")
        .args([
            "openai-planner.js",
            goal,
            "--cwd",
            &cwd.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?
        .wait_with_output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("OpenAI planner failed: {}", stderr).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let prd: PrdOutput = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse OpenAI PRD: {}", e))?;

    let stories: Vec<ReviewStory> = prd.user_stories
        .into_iter()
        .map(|s| ReviewStory {
            id: s.id,
            title: s.title,
            description: s.description,
            depends_on: s.depends_on,
        })
        .collect();

    Ok((stories, prd.project, prd.branch_name))
}

/// Extract JSON object from text that might contain markdown fences or other wrapping
fn extract_json(text: &str) -> String {
    // Try to find JSON between ```json ... ``` or ``` ... ```
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }

    // Try to find a JSON object directly
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return text[start..=end].to_string();
        }
    }

    text.trim().to_string()
}
