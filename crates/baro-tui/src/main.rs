mod app;
mod dag;
mod events;
mod executor;
mod git;
mod screens;
mod theme;
mod ui;

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use crossterm::{
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::process::Command;
use tokio::sync::mpsc;

use app::{App, Planner, ReviewStory, Screen};
use events::BaroEvent;

#[derive(Parser)]
#[command(name = "baro", about = "AI-powered project execution")]
struct Cli {
    /// Project goal (if omitted, shows welcome screen)
    goal: Option<String>,

    /// Planner to use
    #[arg(long, default_value = "claude", value_parser = ["claude", "openai"])]
    planner: String,

    /// Working directory
    #[arg(long, default_value = ".")]
    cwd: String,

    /// Resume execution from existing prd.json
    #[arg(long)]
    resume: bool,

    /// Max parallel story executors (0 = unlimited)
    #[arg(long, default_value = "0")]
    parallel: u32,

    /// Per-story timeout in seconds
    #[arg(long, default_value = "600")]
    timeout: u64,

    /// Override model for all phases (valid: opus, sonnet, haiku)
    #[arg(long = "model", value_parser = ["opus", "sonnet", "haiku"])]
    model: Option<String>,

    /// Disable model routing (equivalent to --model opus)
    #[arg(long = "no-model-routing")]
    no_model_routing: bool,
}

enum AppEvent {
    Baro(BaroEvent),
    Key(crossterm::event::KeyEvent),
    PlanReady(Vec<ReviewStory>, String, String, String),
    PlanError(String),
    RefineReady(Vec<ReviewStory>, String, String, String),
    RefineError(String),
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
      "tests": ["npm test"],
      "model": "opus"
    }
  ]
}

Rules:
- Each story: single focused unit of work for one AI agent
- If a story is complex enough to require the most capable model (opus with 1M context) for implementation, set its "model" field to "opus". Otherwise omit the field entirely (defaults to the routing default, typically sonnet)
- Use dependsOn for dependencies; same-priority stories with no deps run IN PARALLEL
- Keep stories small (15-60 min of work each)
- Include testable acceptance criteria and test commands
- No circular dependencies
- Start with foundational stories, build up
- IDs: S1, S2, S3...
- Build on existing code, don't recreate what exists
- Output ONLY the JSON, nothing else"#;

#[derive(serde::Deserialize)]
struct PrdOutput {
    project: String,
    #[serde(default)]
    #[serde(rename = "branchName")]
    branch_name: String,
    #[serde(default)]
    description: String,
    #[serde(rename = "userStories")]
    user_stories: Vec<PrdStoryOutput>,
}

#[derive(serde::Deserialize)]
struct PrdStoryOutput {
    id: String,
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    #[serde(rename = "dependsOn")]
    depends_on: Vec<String>,
    #[serde(default)]
    model: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let mut tty = open_tty()?;
    enable_raw_mode()?;
    execute!(tty, EnterAlternateScreen)?;
    execute!(tty, Clear(ClearType::All))?;
    execute!(tty, Clear(ClearType::Purge))?;
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
    let cwd = std::fs::canonicalize(&cli.cwd)?;
    let parallel = cli.parallel;
    let timeout_secs = cli.timeout;

    app.parallel_limit = cli.parallel;
    app.timeout_secs = cli.timeout;

    app.planner = match cli.planner.as_str() {
        "openai" => Planner::OpenAI,
        _ => Planner::Claude,
    };

    if let Some(ref model) = cli.model {
        app.override_model = Some(model.clone());
        app.model_routing = false;
    } else if cli.no_model_routing {
        app.override_model = Some("opus".to_string());
        app.model_routing = false;
    }

    let (tx, mut rx) = mpsc::channel::<AppEvent>(256);

    // Resume detection: check for existing prd.json with incomplete stories
    let prd_path = cwd.join("prd.json");
    let mut entered_resume = false;
    if prd_path.exists() {
        if let Ok(prd_contents) = std::fs::read_to_string(&prd_path) {
            if let Ok(prd) = serde_json::from_str::<executor::PrdFile>(&prd_contents) {
                let has_incomplete = prd.user_stories.iter().any(|s| !s.passes);
                if cli.resume || (has_incomplete && cli.goal.is_none()) {
                    app.is_resume = true;
                    app.project = prd.project.clone();
                    app.branch_name = prd.branch_name.clone();
                    app.description = prd.description.clone();
                    let stories: Vec<ReviewStory> = prd.user_stories.iter().map(|s| ReviewStory {
                        id: s.id.clone(),
                        title: s.title.clone(),
                        description: s.description.clone(),
                        depends_on: s.depends_on.clone(),
                        completed: s.passes,
                        model: s.model.clone(),
                    }).collect();
                    app.show_review(stories);
                    entered_resume = true;
                }
            }
        }
    }

    // If goal provided via CLI (and not resuming), skip welcome and start planning immediately
    if !entered_resume {
        if let Some(goal) = cli.goal {
            app.goal_input = goal;
            app.start_planning();
            spawn_planner(&app, &cwd, tx.clone());
        }
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
            Some(AppEvent::Baro(ev)) => {
                // Fire notification immediately when stories complete
                if matches!(ev, BaroEvent::NotificationReady) {
                    // Terminal bell works from inside alternate screen
                    print!("\x07");
                    // OS notification via spawned process (works from alternate screen)
                    match std::env::consts::OS {
                        "macos" => {
                            let _ = std::process::Command::new("osascript")
                                .args(["-e", "display notification \"All stories complete\" with title \"baro\""])
                                .spawn();
                        }
                        "linux" => {
                            let _ = std::process::Command::new("notify-send")
                                .args(["baro", "All stories complete"])
                                .spawn();
                        }
                        _ => {}
                    }
                }
                app.handle_event(ev);
            }
            Some(AppEvent::PlanReady(stories, project, branch, description)) => {
                app.project = project;
                app.branch_name = branch;
                app.description = description;
                app.show_review(stories);
            }
            Some(AppEvent::PlanError(err)) => {
                app.planning_error = Some(err);
            }
            Some(AppEvent::RefineReady(stories, project, branch, description)) => {
                app.refining = false;
                app.project = project;
                app.branch_name = branch;
                app.description = description;
                app.show_review(stories);
            }
            Some(AppEvent::RefineError(err)) => {
                app.refining = false;
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
                                spawn_planner(&app, &cwd, tx.clone());
                            }
                        }
                        KeyCode::Char(c) => app.goal_input.push(c),
                        KeyCode::Backspace => { app.goal_input.pop(); }
                        KeyCode::Left | KeyCode::Right => app.toggle_planner(),
                        _ => {}
                    },
                    Screen::Planning => match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char('r') => {
                            if app.planning_error.is_some() {
                                app.planning_error = None;
                                app.start_planning();
                                spawn_planner(&app, &cwd, tx.clone());
                            }
                        }
                        _ => {}
                    },
                    Screen::Review => if app.refine_input.is_some() {
                        // Overlay is open — handle overlay keys only
                        match key.code {
                            KeyCode::Esc => { app.refine_input = None; }
                            KeyCode::Enter => {
                                let feedback = app.refine_input.as_ref().unwrap().clone();
                                if !feedback.is_empty() {
                                    app.refining = true;
                                    app.refine_input = None;
                                    spawn_refiner(&app, &feedback, &cwd, tx.clone());
                                }
                            }
                            KeyCode::Char(c) => { app.refine_input.as_mut().unwrap().push(c); }
                            KeyCode::Backspace => { app.refine_input.as_mut().unwrap().pop(); }
                            _ => {}
                        }
                    } else {
                        match key.code {
                        KeyCode::Char('r') => {
                            if !app.refining {
                                app.refine_input = Some(String::new());
                            }
                        }
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Enter => {
                            if app.is_resume {
                                // Resume mode: read existing prd.json (has full acceptance/tests data)
                                let prd_path = cwd.join("prd.json");
                                match std::fs::read_to_string(&prd_path)
                                    .map_err(|e| e.to_string())
                                    .and_then(|c| serde_json::from_str::<executor::PrdFile>(&c).map_err(|e| e.to_string()))
                                {
                                    Ok(prd) => {
                                        let full_branch = format!("baro/{}", prd.branch_name);
                                        let branch_cwd = cwd.clone();
                                        let branch_name_clone = full_branch.clone();
                                        app.branch_name = full_branch;
                                        app.start_execution();
                                        let exec_cwd = cwd.clone();
                                        let branch_tx = tx.clone();
                                        let mr = app.model_routing;
                                        let om = app.override_model.clone();
                                        tokio::spawn(async move {
                                            if let Err(e) = git::create_or_checkout_branch(&branch_cwd, &branch_name_clone).await {
                                                eprintln!("[baro] branch checkout failed: {}", e);
                                            }
                                            spawn_executor(prd, exec_cwd, branch_tx, parallel, timeout_secs, mr, om);
                                        });
                                    }
                                    Err(e) => {
                                        app.planning_error = Some(format!("Failed to read prd.json: {}", e));
                                    }
                                }
                            } else {
                                // Normal mode: write prd.json and start execution
                                let prd = executor::prd_from_review(
                                    &app.project,
                                    &app.branch_name,
                                    &app.description,
                                    &app.review_stories,
                                );
                                if let Err(e) = executor::write_prd(&prd, &cwd) {
                                    app.planning_error = Some(format!("Failed to write prd.json: {}", e));
                                } else {
                                    // Create git branch baro/<branchName>
                                    let full_branch = format!("baro/{}", app.branch_name);
                                    let branch_cwd = cwd.clone();
                                    let branch_name_clone = full_branch.clone();
                                    app.branch_name = full_branch;
                                    app.start_execution();
                                    let exec_prd = prd;
                                    let exec_cwd = cwd.clone();
                                    let branch_tx = tx.clone();
                                    let mr = app.model_routing;
                                    let om = app.override_model.clone();
                                    tokio::spawn(async move {
                                        if let Err(e) = git::create_or_checkout_branch(&branch_cwd, &branch_name_clone).await {
                                            eprintln!("[baro] branch creation failed: {}", e);
                                        }
                                        spawn_executor(exec_prd, exec_cwd, branch_tx, parallel, timeout_secs, mr, om);
                                    });
                                }
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => app.review_prev(),
                        KeyCode::Down | KeyCode::Char('j') => app.review_next(),
                        _ => {}
                    }},
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
            Some(AppEvent::Tick) => {
                app.tick_count += 1;
            }
            None => break,
        }
    }
    Ok(())
}

fn spawn_planner(app: &App, cwd: &Path, tx: mpsc::Sender<AppEvent>) {
    let goal = app.goal_input.clone();
    let planner = app.planner;
    let cwd = cwd.to_path_buf();
    let model = app.model_for_phase("planning");

    tokio::spawn(async move {
        let result = match planner {
            Planner::Claude => run_claude_planner(&goal, &cwd, model.as_deref()).await,
            Planner::OpenAI => run_openai_planner(&goal, &cwd).await,
        };

        match result {
            Ok((stories, project, branch, description)) => {
                let _ = tx.send(AppEvent::PlanReady(stories, project, branch, description)).await;
            }
            Err(e) => {
                let _ = tx.send(AppEvent::PlanError(e.to_string())).await;
            }
        }
    });
}

fn spawn_refiner(app: &App, feedback: &str, cwd: &Path, tx: mpsc::Sender<AppEvent>) {
    let feedback = feedback.to_string();
    let cwd = cwd.to_path_buf();
    let model = app.model_for_phase("planning");

    // Build current plan JSON from app state
    let stories_json: Vec<serde_json::Value> = app.review_stories.iter().map(|s| {
        serde_json::json!({
            "id": s.id,
            "title": s.title,
            "description": s.description,
            "dependsOn": s.depends_on,
        })
    }).collect();
    let plan_json = serde_json::json!({
        "project": app.project,
        "branchName": app.branch_name,
        "description": app.description,
        "userStories": stories_json,
    });
    let plan_str = serde_json::to_string_pretty(&plan_json).unwrap_or_default();

    tokio::spawn(async move {
        let prompt = format!(
            "Here is the current plan:\n{}\nThe user wants these changes: {}\nGenerate an updated plan with the same JSON schema. Keep stories the user did not mention unchanged. Output ONLY valid JSON, no markdown, no explanation.",
            plan_str, feedback
        );

        let result = async {
            let mut args = vec![
                "--dangerously-skip-permissions".to_string(),
                "--output-format".to_string(), "json".to_string(),
            ];
            if let Some(ref model_name) = model {
                args.push("--model".to_string());
                args.push(model_name.clone());
            }
            args.push("-p".to_string());
            args.push(prompt.clone());

            let output = Command::new("claude")
                .args(&args)
                .current_dir(&cwd)
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
            let claude_output: serde_json::Value = serde_json::from_str(&stdout)
                .map_err(|e| format!("Failed to parse Claude JSON wrapper: {}", e))?;

            let plan_text = claude_output
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or(&stdout);

            let json_str = extract_json(plan_text);

            let prd: PrdOutput = serde_json::from_str(&json_str)
                .map_err(|e| format!("Failed to parse refined PRD JSON: {}\nRaw: {}", e, &json_str[..json_str.len().min(500)]))?;

            let stories: Vec<ReviewStory> = prd.user_stories
                .into_iter()
                .map(|s| ReviewStory {
                    id: s.id,
                    title: s.title,
                    description: s.description,
                    depends_on: s.depends_on,
                    completed: false,
                    model: s.model,
                })
                .collect();

            Ok::<_, Box<dyn std::error::Error + Send + Sync>>((stories, prd.project, prd.branch_name, prd.description))
        }.await;

        match result {
            Ok((stories, project, branch, description)) => {
                let _ = tx.send(AppEvent::RefineReady(stories, project, branch, description)).await;
            }
            Err(e) => {
                let _ = tx.send(AppEvent::RefineError(e.to_string())).await;
            }
        }
    });
}

fn spawn_executor(
    prd: executor::PrdFile,
    cwd: PathBuf,
    tx: mpsc::Sender<AppEvent>,
    parallel: u32,
    timeout_secs: u64,
    model_routing: bool,
    override_model: Option<String>,
) {
    // Create a channel bridge: executor sends BaroEvent, we wrap them as AppEvent::Baro
    let (exec_tx, mut exec_rx) = mpsc::channel::<BaroEvent>(256);

    // Forward BaroEvents to AppEvents
    let tx_fwd = tx.clone();
    tokio::spawn(async move {
        while let Some(ev) = exec_rx.recv().await {
            if tx_fwd.send(AppEvent::Baro(ev)).await.is_err() {
                break;
            }
        }
    });

    // Run executor
    tokio::spawn(async move {
        executor::run_executor(prd, cwd, exec_tx, parallel, timeout_secs, model_routing, override_model).await;
    });
}

async fn run_claude_planner(goal: &str, cwd: &Path, model: Option<&str>) -> Result<(Vec<ReviewStory>, String, String, String), Box<dyn std::error::Error + Send + Sync>> {
    let prompt = format!("{}\n\nUser goal: {}", CLAUDE_PLANNER_PROMPT, goal);

    let mut args = vec![
        "--dangerously-skip-permissions",
        "--output-format", "json",
    ];
    if let Some(model_name) = model {
        args.push("--model");
        args.push(model_name);
    }
    args.push("-p");
    args.push(&prompt);

    let output = Command::new("claude")
        .args(&args)
        .current_dir(cwd)
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
            completed: false,
            model: s.model,
        })
        .collect();

    Ok((stories, prd.project, prd.branch_name, prd.description))
}

async fn run_openai_planner(goal: &str, cwd: &Path) -> Result<(Vec<ReviewStory>, String, String, String), Box<dyn std::error::Error + Send + Sync>> {
    // Find the openai-planner.js relative to the binary or use node_modules
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    // Try multiple locations for the planner script
    let script_paths = [
        exe_dir.as_ref().map(|d| d.join("openai-planner.js")),
        Some(cwd.join("node_modules/baro-ai/dist/openai-planner.js")),
        Some(cwd.join("openai-planner.js")),
    ];

    let script_path = script_paths
        .iter()
        .filter_map(|p| p.as_ref())
        .find(|p| p.exists())
        .ok_or("Could not find openai-planner.js")?
        .clone();

    let output = Command::new("node")
        .args([
            script_path.to_string_lossy().as_ref(),
            goal,
            "--cwd",
            &cwd.to_string_lossy(),
        ])
        .current_dir(cwd)
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
            completed: false,
            model: s.model,
        })
        .collect();

    Ok((stories, prd.project, prd.branch_name, prd.description))
}

fn extract_json(text: &str) -> String {
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

    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return text[start..=end].to_string();
        }
    }

    text.trim().to_string()
}
