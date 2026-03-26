use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex, Semaphore};
use tokio::time::timeout;

use crate::app::ReviewStory;
use crate::dag::build_dag;
use crate::events::BaroEvent;

// ─── PRD types (for reading/writing prd.json) ───────────────

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PrdFile {
    pub project: String,
    #[serde(rename = "branchName", default)]
    pub branch_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "userStories")]
    pub user_stories: Vec<PrdStory>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PrdStory {
    pub id: String,
    pub priority: i32,
    pub title: String,
    pub description: String,
    #[serde(rename = "dependsOn", default)]
    pub depends_on: Vec<String>,
    #[serde(default = "default_retries")]
    pub retries: u32,
    #[serde(default)]
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub tests: Vec<String>,
    #[serde(default)]
    pub passes: bool,
    #[serde(rename = "completedAt", default)]
    pub completed_at: Option<String>,
    #[serde(rename = "durationSecs", default)]
    pub duration_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

fn default_retries() -> u32 {
    2
}

// ─── Story prompt builder ───────────────────────────────────

fn build_prompt(story: &PrdStory, cwd: &Path) -> String {
    let template_path = cwd.join("prompt.md");
    let template = if template_path.exists() {
        std::fs::read_to_string(&template_path).unwrap_or_default()
    } else {
        [
            "You are working on story STORY_ID: STORY_TITLE",
            "",
            "STORY_DESCRIPTION",
            "",
            "ACCEPTANCE CRITERIA:",
            "ACCEPTANCE_CRITERIA",
            "",
            "Run tests: TEST_COMMANDS",
            "If tests pass, commit: git add . && git commit -m \"feat(STORY_ID): STORY_TITLE\"",
        ]
        .join("\n")
    };

    let acceptance = story
        .acceptance
        .iter()
        .map(|a| format!("- {}", a))
        .collect::<Vec<_>>()
        .join("\n");
    let tests = if story.tests.is_empty() {
        "echo 'no tests'".to_string()
    } else {
        story.tests.join(" && ")
    };

    template
        .replace("STORY_ID", &story.id)
        .replace("STORY_TITLE", &story.title)
        .replace("STORY_DESCRIPTION", &story.description)
        .replace("ACCEPTANCE_CRITERIA", &acceptance)
        .replace("TEST_COMMANDS", &tests)
}

// ─── Claude stream-json parser ──────────────────────────────

fn parse_claude_stream_line(line: &str, _story_id: &str) -> Vec<String> {
    let mut logs = Vec::new();

    let Ok(ev) = serde_json::from_str::<serde_json::Value>(line) else {
        // Not JSON, emit as raw log
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            logs.push(trimmed.to_string());
        }
        return logs;
    };

    let ev_type = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match ev_type {
        "assistant" => {
            if let Some(content) = ev
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            {
                for block in content {
                    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match block_type {
                        "text" => {
                            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                for l in text.split('\n') {
                                    if !l.is_empty() {
                                        logs.push(l.to_string());
                                    }
                                }
                            }
                        }
                        "tool_use" => {
                            let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                            let input = block
                                .get("input")
                                .map(|i| i.to_string())
                                .unwrap_or_default();
                            let preview = if input.len() > 80 {
                                format!("{}...", &input[..80])
                            } else {
                                input
                            };
                            logs.push(format!("⚙ {} {}", name, preview));
                        }
                        _ => {}
                    }
                }
            }
        }
        "system" => {
            if ev.get("subtype").and_then(|s| s.as_str()) == Some("init") {
                let model = ev.get("model").and_then(|m| m.as_str()).unwrap_or("unknown");
                logs.push(format!("Model: {}", model));
            }
        }
        "result" => {
            if let Some(result) = ev.get("result").and_then(|r| r.as_str()) {
                for l in result.split('\n').take(3) {
                    let trimmed = l.trim();
                    if !trimmed.is_empty() {
                        logs.push(trimmed.to_string());
                    }
                }
            }
        }
        _ => {}
    }

    logs
}

// ─── Model resolution helper ────────────────────────────────

fn resolve_model(
    override_model: &Option<String>,
    story_model: &Option<String>,
    model_routing: bool,
    phase: &str, // "execute" or "review"
) -> Option<String> {
    if let Some(ref m) = override_model {
        return Some(m.clone());
    }
    if let Some(ref m) = story_model {
        return Some(m.clone());
    }
    if model_routing {
        return Some(
            match phase {
                "review" => "haiku",
                _ => "sonnet",
            }
            .to_string(),
        );
    }
    None
}

// ─── Execute a single story ─────────────────────────────────

async fn execute_story(
    story: &PrdStory,
    cwd: &Path,
    prd_path: &Path,
    tx: &mpsc::Sender<BaroEvent>,
    git_mutex: &Mutex<()>,
    timeout_secs: u64,
    model: Option<String>,
) -> Result<(u64, u32, u32), String> {
    let max_attempts = story.retries + 1;

    for attempt in 1..=max_attempts {
        let _ = tx
            .send(BaroEvent::StoryStart {
                id: story.id.clone(),
                title: story.title.clone(),
            })
            .await;

        let model_label = model
            .as_deref()
            .unwrap_or("default");
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: story.id.clone(),
                line: format!("[model] using {}", model_label),
            })
            .await;

        // Git pull --rebase before running claude (best-effort, never fatal)
        {
            let _git_lock = git_mutex.lock().await;
            crate::git::safe_pull_rebase(cwd, &story.id, tx).await;
        }

        let start = Instant::now();
        let prompt = build_prompt(story, cwd);

        let result =
            run_claude_for_story(&story.id, &prompt, cwd, tx, timeout_secs, &model).await;

        let duration_secs = start.elapsed().as_secs();

        match result {
            Ok(()) => {
                // Acquire git mutex for prd update and git stats
                let (files_created, files_modified) = {
                    let _git_lock = git_mutex.lock().await;

                    // Update prd.json
                    let _ = crate::git::update_prd_story(prd_path, &story.id, duration_secs);

                    // Get git stats
                    crate::git::get_git_file_stats(cwd).await
                };

                // Push with retry (acquires git_mutex internally)
                let push_result =
                    crate::git::git_push_with_retry(git_mutex, cwd, &story.id, tx).await;
                let (push_success, push_error) = match &push_result {
                    Ok(()) => (true, None),
                    Err(e) => (false, Some(e.clone())),
                };
                let _ = tx
                    .send(BaroEvent::PushStatus {
                        id: story.id.clone(),
                        success: push_success,
                        error: push_error,
                    })
                    .await;

                return Ok((duration_secs, files_created, files_modified));
            }
            Err(err) => {
                let _ = tx
                    .send(BaroEvent::StoryError {
                        id: story.id.clone(),
                        error: err.clone(),
                        attempt,
                        max_retries: max_attempts,
                    })
                    .await;

                if attempt < max_attempts {
                    let _ = tx
                        .send(BaroEvent::StoryRetry {
                            id: story.id.clone(),
                            attempt: attempt + 1,
                        })
                        .await;
                } else {
                    return Err(err);
                }
            }
        }
    }

    Err("All attempts exhausted".to_string())
}

async fn run_claude_for_story(
    story_id: &str,
    prompt: &str,
    cwd: &Path,
    tx: &mpsc::Sender<BaroEvent>,
    timeout_secs: u64,
    model: &Option<String>,
) -> Result<(), String> {
    let mut args = vec![
        "--dangerously-skip-permissions",
        "--output-format",
        "stream-json",
        "--verbose",
        "-p",
        prompt,
    ];
    let model_owned;
    if let Some(ref m) = model {
        model_owned = m.clone();
        args.push("--model");
        args.push(&model_owned);
    }
    let mut child = Command::new("claude")
        .args(&args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn claude: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let story_id_owned = story_id.to_string();
    let tx_clone = tx.clone();

    let result = timeout(Duration::from_secs(timeout_secs), async {
        let story_id_out = story_id_owned.clone();
        let tx_out = tx_clone.clone();
        let stdout_task = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let logs = parse_claude_stream_line(&line, &story_id_out);
                for log in logs {
                    let _ = tx_out
                        .send(BaroEvent::StoryLog {
                            id: story_id_out.clone(),
                            line: log,
                        })
                        .await;
                }
            }
        });

        let story_id_err = story_id_owned.clone();
        let tx_err = tx_clone.clone();
        let stderr_task = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim().to_string();
                if !trimmed.is_empty() {
                    let _ = tx_err
                        .send(BaroEvent::StoryLog {
                            id: story_id_err.clone(),
                            line: trimmed,
                        })
                        .await;
                }
            }
        });

        let _ = stdout_task.await;
        let _ = stderr_task.await;

        child
            .wait()
            .await
            .map_err(|e| format!("Failed to wait for claude: {}", e))
    })
    .await;

    match result {
        Ok(wait_result) => {
            let status = wait_result?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("claude exited with code {}", status.code().unwrap_or(-1)))
            }
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: story_id.to_string(),
                    line: format!("[timeout] Story {} killed after {}s", story_id, timeout_secs),
                })
                .await;
            Err(format!("Story timed out after {}s", timeout_secs))
        }
    }
}

// ─── JSON extraction helper ─────────────────────────────────

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

// ─── Build detection & execution ────────────────────────────

async fn detect_project_and_build(cwd: &Path) -> Option<String> {
    let build_systems: &[(&str, &[&str])] = &[
        ("Cargo.toml", &["cargo", "build"]),
        ("package.json", &["npm", "run", "build"]),
        ("go.mod", &["go", "build", "./..."]),
        ("pyproject.toml", &[]),
        ("Makefile", &["make"]),
    ];

    for (file, cmd) in build_systems {
        if !cwd.join(file).exists() {
            continue;
        }

        // Python: compile all .py files
        if *file == "pyproject.toml" {
            let mut py_files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(cwd) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("py") {
                        py_files.push(path);
                    }
                }
            }
            // Also check src/ directory
            let src_dir = cwd.join("src");
            if src_dir.is_dir() {
                fn collect_py_files(dir: &Path, files: &mut Vec<PathBuf>) {
                    if let Ok(entries) = std::fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.is_dir() {
                                collect_py_files(&path, files);
                            } else if path.extension().and_then(|e| e.to_str()) == Some("py") {
                                files.push(path);
                            }
                        }
                    }
                }
                collect_py_files(&src_dir, &mut py_files);
            }

            if py_files.is_empty() {
                return None;
            }

            let py_args: Vec<String> = std::iter::once("-m".to_string())
                .chain(std::iter::once("py_compile".to_string()))
                .chain(py_files.iter().map(|p| p.to_string_lossy().to_string()))
                .collect();

            let output = Command::new("python")
                .args(&py_args)
                .current_dir(cwd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
                .await
                .ok()?;

            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if combined.trim().is_empty() {
                return None;
            }
            return Some(combined);
        }

        // All other build systems
        let output = Command::new(cmd[0])
            .args(&cmd[1..])
            .current_dir(cwd)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await
            .ok()?;

        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if combined.trim().is_empty() {
            return None;
        }
        return Some(combined);
    }

    None
}

// ─── Review agent ───────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct ReviewResult {
    passed: bool,
    #[serde(default)]
    fixes: Vec<ReviewFix>,
}

#[derive(Debug, serde::Deserialize)]
struct ReviewFix {
    title: String,
    description: String,
}

#[allow(clippy::too_many_arguments)]
async fn run_review_for_level(
    saved_hash: &str,
    cwd: &Path,
    completed_stories: &[&PrdStory],
    tx: &mpsc::Sender<BaroEvent>,
    git_mutex: &Arc<Mutex<()>>,
    prd_path: &Path,
    level_index: usize,
    timeout_secs: u64,
    model_routing: bool,
    override_model: &Option<String>,
) -> (u32, u32) {
    let mut cycles_run: u32 = 0;
    let mut total_fixes_applied: u32 = 0;

    let _ = tx
        .send(BaroEvent::ReviewStart {
            level: level_index,
        })
        .await;

    let _ = tx
        .send(BaroEvent::ReviewLog {
            line: format!(
                "Starting review for level {} ({} stories)",
                level_index,
                completed_stories.len()
            ),
        })
        .await;

    let max_cycles = 2;

    for cycle in 0..max_cycles {
        cycles_run += 1;
        let _ = tx
            .send(BaroEvent::ReviewLog {
                line: format!("Review cycle {}/{}", cycle + 1, max_cycles),
            })
            .await;

        // Get diff from saved hash to HEAD
        let diff_output = {
            let _git_lock = git_mutex.lock().await;
            Command::new("git")
                .args(["diff", saved_hash, "HEAD"])
                .current_dir(cwd)
                .output()
                .await
        };

        let diff = match diff_output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
            Ok(out) => {
                let _ = tx
                    .send(BaroEvent::ReviewLog {
                        line: format!(
                            "git diff failed: {}",
                            String::from_utf8_lossy(&out.stderr)
                        ),
                    })
                    .await;
                return (cycles_run, total_fixes_applied);
            }
            Err(e) => {
                let _ = tx
                    .send(BaroEvent::ReviewLog {
                        line: format!("git diff error: {}", e),
                    })
                    .await;
                return (cycles_run, total_fixes_applied);
            }
        };

        if diff.trim().is_empty() {
            let _ = tx
                .send(BaroEvent::ReviewLog {
                    line: "No changes to review".to_string(),
                })
                .await;
            let _ = tx
                .send(BaroEvent::ReviewComplete {
                    level: level_index,
                    passed: true,
                    fix_count: 0,
                })
                .await;
            return (cycles_run, total_fixes_applied);
        }

        // Collect acceptance criteria from completed stories
        let mut criteria = String::new();
        for story in completed_stories {
            criteria.push_str(&format!("## Story {}: {}\n", story.id, story.title));
            for ac in &story.acceptance {
                criteria.push_str(&format!("- {}\n", ac));
            }
            criteria.push('\n');
        }

        // Run build to capture output for reviewer context
        let build_section = match detect_project_and_build(cwd).await {
            Some(output) => {
                let truncated = if output.len() > 5000 {
                    &output[..5000]
                } else {
                    &output
                };
                format!(
                    "\nBUILD OUTPUT (from running the project build command):\n{}\n\n",
                    truncated
                )
            }
            None => String::new(),
        };

        // Build review prompt — acceptance criteria + code quality checks
        let prompt = format!(
            "You are a focused code reviewer. Check whether the acceptance criteria are met by the diff, \
             and also check for obvious code quality problems.\n\n\
             ACCEPTANCE CRITERIA:\n{}\n\n\
             {}\
             GIT DIFF:\n```\n{}\n```\n\n\
             Rules:\n\
             1. Check if the acceptance criteria are satisfied by the changes\n\
             2. Check for obvious bugs visible in the diff: undefined variables, missing imports, \
             broken function calls, type mismatches\n\
             3. Check for leftover debug code: console.log/println!/fmt.Println debugging statements \
             that should not be in production, commented-out code blocks\n\
             4. If build output is provided above, check for build errors\n\
             - Do NOT suggest refactoring, style improvements, or architecture changes\n\
             - Do NOT flag missing tests unless tests are in the acceptance criteria\n\
             - Do NOT flag linting or formatting issues\n\
             - If acceptance criteria are empty, only check for bugs, debug code, and build errors\n\
             - Be lenient: pass if code works correctly even if not perfectly clean\n\
             - Only flag things that are actually broken or clearly wrong\n\n\
             Respond with ONLY valid JSON (no markdown fences):\n\
             {{\"passed\": boolean, \"fixes\": [{{\"title\": \"short title\", \"description\": \"what is wrong\"}}]}}\n\n\
             If criteria are met and no actual bugs/debug code/build errors are found, set passed=true and fixes=[].",
            criteria,
            build_section,
            if diff.len() > 30000 {
                &diff[..30000]
            } else {
                &diff
            }
        );

        let review_model = resolve_model(override_model, &None, model_routing, "review");
        let review_model_label = review_model.as_deref().unwrap_or("default");
        let _ = tx
            .send(BaroEvent::ReviewLog {
                line: format!("[model] review using {}", review_model_label),
            })
            .await;

        let _ = tx
            .send(BaroEvent::ReviewLog {
                line: "Spawning Claude for review...".to_string(),
            })
            .await;

        // Spawn claude for review
        let mut review_args = vec![
            "--dangerously-skip-permissions".to_string(),
            "--output-format".to_string(),
            "json".to_string(),
            "-p".to_string(),
            prompt.clone(),
        ];
        if let Some(ref m) = review_model {
            review_args.push("--model".to_string());
            review_args.push(m.clone());
        }
        let child_result = Command::new("claude")
            .args(&review_args)
            .current_dir(cwd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        let child = match child_result {
            Ok(c) => c,
            Err(e) => {
                let _ = tx
                    .send(BaroEvent::ReviewLog {
                        line: format!("Failed to spawn claude for review: {}", e),
                    })
                    .await;
                return (cycles_run, total_fixes_applied);
            }
        };

        let output = match child.wait_with_output().await {
            Ok(o) => o,
            Err(e) => {
                let _ = tx
                    .send(BaroEvent::ReviewLog {
                        line: format!("Review claude process error: {}", e),
                    })
                    .await;
                return (cycles_run, total_fixes_applied);
            }
        };

        if !output.status.success() {
            let _ = tx
                .send(BaroEvent::ReviewLog {
                    line: format!(
                        "Review claude exited with code {}",
                        output.status.code().unwrap_or(-1)
                    ),
                })
                .await;
            return (cycles_run, total_fixes_applied);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse Claude JSON wrapper (like the planner does)
        let result_text = match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(wrapper) => wrapper
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or(&stdout)
                .to_string(),
            Err(_) => stdout.to_string(),
        };

        let json_str = extract_json(&result_text);
        let review: ReviewResult = match serde_json::from_str(&json_str) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx
                    .send(BaroEvent::ReviewLog {
                        line: format!(
                            "Failed to parse review JSON: {}. Raw: {}",
                            e,
                            &json_str[..json_str.len().min(200)]
                        ),
                    })
                    .await;
                let _ = tx
                    .send(BaroEvent::ReviewComplete {
                        level: level_index,
                        passed: false,
                        fix_count: 0,
                    })
                    .await;
                return (cycles_run, total_fixes_applied);
            }
        };

        if review.passed {
            let _ = tx
                .send(BaroEvent::ReviewLog {
                    line: "Review passed!".to_string(),
                })
                .await;
            let _ = tx
                .send(BaroEvent::ReviewComplete {
                    level: level_index,
                    passed: true,
                    fix_count: 0,
                })
                .await;
            return (cycles_run, total_fixes_applied);
        }

        // Review failed - create fix stories and execute them
        let fix_count = review.fixes.len() as u32;
        total_fixes_applied += fix_count;
        let _ = tx
            .send(BaroEvent::ReviewLog {
                line: format!(
                    "Review failed with {} fixes needed (cycle {}/{})",
                    fix_count,
                    cycle + 1,
                    max_cycles
                ),
            })
            .await;

        for (i, fix) in review.fixes.iter().enumerate() {
            let fix_id = format!("S{}-fix{}", level_index, i + 1);
            let _ = tx
                .send(BaroEvent::ReviewLog {
                    line: format!("Executing fix: {} - {}", fix_id, fix.title),
                })
                .await;

            let fix_story = PrdStory {
                id: fix_id.clone(),
                priority: (i + 1) as i32,
                title: fix.title.clone(),
                description: fix.description.clone(),
                depends_on: Vec::new(),
                retries: 1,
                acceptance: Vec::new(),
                tests: Vec::new(),
                passes: false,
                completed_at: None,
                duration_secs: None,
                model: None,
            };

            let fix_model = resolve_model(override_model, &None, model_routing, "execute");
            match execute_story(&fix_story, cwd, prd_path, tx, git_mutex, timeout_secs, fix_model)
                .await
            {
                Ok(_) => {
                    let _ = tx
                        .send(BaroEvent::ReviewLog {
                            line: format!("Fix {} completed", fix_id),
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(BaroEvent::ReviewLog {
                            line: format!("Fix {} failed: {}", fix_id, e),
                        })
                        .await;
                }
            }
        }

        // If this is the last cycle, emit warning and complete
        if cycle == max_cycles - 1 {
            let _ = tx
                .send(BaroEvent::ReviewLog {
                    line: format!(
                        "Warning: review still failing after {} cycles, continuing",
                        max_cycles
                    ),
                })
                .await;
            let _ = tx
                .send(BaroEvent::ReviewComplete {
                    level: level_index,
                    passed: false,
                    fix_count,
                })
                .await;
            return (cycles_run, total_fixes_applied);
        }

        // Otherwise loop for re-review with new diff
    }
    (cycles_run, total_fixes_applied)
}

// ─── Main executor entry point ──────────────────────────────

pub async fn run_executor(
    prd: PrdFile,
    cwd: PathBuf,
    tx: mpsc::Sender<BaroEvent>,
    parallel: u32,
    timeout_secs: u64,
    model_routing: bool,
    override_model: Option<String>,
) {
    let prd_path = cwd.join("prd.json");
    let stories = &prd.user_stories;
    let incomplete: Vec<&PrdStory> = stories.iter().filter(|s| !s.passes).collect();
    let total = incomplete.len() as u32;

    // Emit init
    let _ = tx
        .send(BaroEvent::Init {
            project: prd.project.clone(),
            stories: incomplete
                .iter()
                .map(|s| crate::events::StoryInfo {
                    id: s.id.clone(),
                    title: s.title.clone(),
                    depends_on: s.depends_on.clone(),
                })
                .collect(),
        })
        .await;

    // Build DAG
    let levels = match build_dag(stories) {
        Ok(levels) => levels,
        Err(e) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "system".to_string(),
                    line: format!("DAG error: {}", e),
                })
                .await;
            return;
        }
    };

    // Emit DAG structure
    let dag_levels: Vec<Vec<crate::events::DagNode>> = levels
        .iter()
        .map(|level| {
            level
                .story_ids
                .iter()
                .filter_map(|id| {
                    stories.iter().find(|s| s.id == *id).map(|s| {
                        crate::events::DagNode {
                            id: s.id.clone(),
                        }
                    })
                })
                .collect()
        })
        .collect();

    let _ = tx.send(BaroEvent::Dag { levels: dag_levels }).await;

    let git_mutex = Arc::new(Mutex::new(()));

    let start = Instant::now();
    let mut completed = 0u32;
    let mut skipped = 0u32;
    let mut total_files_created = 0u32;
    let mut total_files_modified = 0u32;
    let mut total_commits = 0u32;
    let mut review_cycles = 0u32;
    let mut review_fixes_applied = 0u32;

    // Create semaphore for parallelism limiting (0 = unlimited)
    let semaphore = if parallel > 0 {
        Some(Arc::new(Semaphore::new(parallel as usize)))
    } else {
        None
    };

    // Execute level by level
    let story_map: HashMap<&str, &PrdStory> =
        stories.iter().map(|s| (s.id.as_str(), s)).collect();

    for (level_index, level) in levels.iter().enumerate() {
        // Save current commit hash before executing stories in this level
        let saved_hash = {
            let _git_lock = git_mutex.lock().await;
            let output = Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&cwd)
                .output()
                .await;
            match output {
                Ok(o) if o.status.success() => {
                    String::from_utf8_lossy(&o.stdout).trim().to_string()
                }
                _ => String::new(),
            }
        };

        // All stories in a level run in parallel
        let mut handles = Vec::new();

        for story_id in &level.story_ids {
            let Some(story) = story_map.get(story_id.as_str()) else {
                continue;
            };
            let story = (*story).clone();
            let cwd = cwd.clone();
            let prd_path = prd_path.clone();
            let tx = tx.clone();
            let git_mutex = Arc::clone(&git_mutex);

            let semaphore = semaphore.clone();
            let story_model =
                resolve_model(&override_model, &story.model, model_routing, "execute");
            let handle = tokio::spawn(async move {
                let _permit = if let Some(ref sem) = semaphore {
                    Some(sem.acquire().await.expect("semaphore closed"))
                } else {
                    None
                };
                execute_story(
                    &story,
                    &cwd,
                    &prd_path,
                    &tx,
                    &git_mutex,
                    timeout_secs,
                    story_model,
                )
                .await
            });
            handles.push((story_id.clone(), handle));
        }

        let mut level_completed_ids: Vec<String> = Vec::new();

        for (story_id, handle) in handles {
            match handle.await {
                Ok(Ok((duration_secs, files_created, files_modified))) => {
                    completed += 1;
                    total_files_created += files_created;
                    total_files_modified += files_modified;
                    total_commits += 1;
                    level_completed_ids.push(story_id.clone());

                    let _ = tx
                        .send(BaroEvent::StoryComplete {
                            id: story_id,
                            duration_secs,
                            files_created,
                            files_modified,
                        })
                        .await;

                    let percentage = if total > 0 {
                        (completed as f64 / total as f64 * 100.0) as u32
                    } else {
                        0
                    };
                    let _ = tx
                        .send(BaroEvent::Progress {
                            completed,
                            total,
                            percentage,
                        })
                        .await;
                }
                Ok(Err(_)) => {
                    skipped += 1;
                }
                Err(e) => {
                    skipped += 1;
                    let _ = tx
                        .send(BaroEvent::StoryLog {
                            id: story_id,
                            line: format!("Task panicked: {}", e),
                        })
                        .await;
                }
            }
        }

        // Run review for this level if we have a saved hash and completed stories
        if !saved_hash.is_empty() && !level_completed_ids.is_empty() {
            let completed_stories: Vec<&PrdStory> = level_completed_ids
                .iter()
                .filter_map(|id| story_map.get(id.as_str()).copied())
                .collect();

            let (cycles, fixes) = run_review_for_level(
                &saved_hash,
                &cwd,
                &completed_stories,
                &tx,
                &git_mutex,
                &prd_path,
                level_index,
                timeout_secs,
                model_routing,
                &override_model,
            )
            .await;
            review_cycles += cycles;
            review_fixes_applied += fixes;
        }
    }

    // Final push of prd.json completion status
    let _prd_push = async {
        {
            let _git_lock = git_mutex.lock().await;

            let add = Command::new("git")
                .args(["add", "prd.json"])
                .current_dir(&cwd)
                .output()
                .await
                .map_err(|e| format!("git add failed: {}", e))?;
            if !add.status.success() {
                return Err(format!(
                    "git add prd.json failed: {}",
                    String::from_utf8_lossy(&add.stderr)
                ));
            }

            let commit = Command::new("git")
                .args(["commit", "-m", "chore: update prd.json completion status"])
                .current_dir(&cwd)
                .output()
                .await
                .map_err(|e| format!("git commit failed: {}", e))?;
            if !commit.status.success() {
                let stderr = String::from_utf8_lossy(&commit.stderr);
                if !stderr.contains("nothing to commit") {
                    return Err(format!("git commit failed: {}", stderr));
                }
            }
        }

        crate::git::git_push_with_retry(&git_mutex, &cwd, "prd", &tx).await
    }
    .await;

    let total_time_secs = start.elapsed().as_secs();
    let _ = tx
        .send(BaroEvent::Done {
            total_time_secs,
            stats: crate::events::DoneStats {
                stories_completed: completed,
                stories_skipped: skipped,
                total_commits,
                files_created: total_files_created,
                files_modified: total_files_modified,
            },
        })
        .await;

    // ─── Completion notification (best-effort) ───────────────────
    print!("\x07"); // terminal bell
    match std::env::consts::OS {
        "macos" => {
            let _ = std::process::Command::new("osascript")
                .args([
                    "-e",
                    "display notification \"baro: all stories complete\" with title \"baro\"",
                ])
                .spawn();
        }
        "linux" => {
            let _ = std::process::Command::new("notify-send")
                .args(["baro", "all stories complete"])
                .spawn();
        }
        _ => {}
    }

    // ─── Finalize phase ─────────────────────────────────────────
    let _ = tx.send(BaroEvent::FinalizeStart).await;

    // Step 1: Run build detection
    if let Some(output) = detect_project_and_build(&cwd).await {
        if output.to_lowercase().contains("error") {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("Build warning: {}", output),
                })
                .await;
        }
    }

    // Step 2: Try to create a GitHub PR
    let pr_url = async {
        // Check if gh CLI is available
        let gh_check = Command::new("gh")
            .arg("--version")
            .output()
            .await
            .ok()?;
        if !gh_check.status.success() {
            return None;
        }

        // Get current branch
        let branch = crate::git::get_current_branch(&cwd).await.ok()?;

        // Re-read prd.json from disk for up-to-date completion status
        let prd_data = tokio::fs::read_to_string(cwd.join("prd.json"))
            .await
            .ok()?;
        let current_prd: PrdFile = serde_json::from_str(&prd_data).ok()?;

        let sequential_time: u64 = current_prd
            .user_stories
            .iter()
            .filter_map(|s| s.duration_secs)
            .sum();

        // Build PR body
        let summary = current_prd
            .description
            .split('.')
            .next()
            .unwrap_or(&current_prd.description)
            .trim();
        let summary = if summary.is_empty() {
            &current_prd.description
        } else {
            summary
        };

        let mut body = format!("{}\n\n", summary);

        // Stories table
        body.push_str("## Stories\n\n");
        body.push_str("| ID | Title | Duration | Status |\n");
        body.push_str("|:---|:------|:---------|:-------|\n");
        for story in &current_prd.user_stories {
            let duration_str = match story.duration_secs {
                Some(d) if d >= 60 => format!("{}m {}s", d / 60, d % 60),
                Some(d) => format!("{}s", d),
                None => "–".to_string(),
            };
            let status = if story.passes { "Done" } else { "Skipped" };
            body.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                story.id, story.title, duration_str, status
            ));
        }

        // Stats section
        let wall_time_str = if total_time_secs >= 60 {
            format!("{}m {}s", total_time_secs / 60, total_time_secs % 60)
        } else {
            format!("{}s", total_time_secs)
        };
        let total_stories = current_prd.user_stories.len();
        let parallelism_stats = if total_stories > 1 {
            let time_saved = sequential_time.saturating_sub(total_time_secs);
            let time_saved_str = if time_saved >= 60 {
                format!("{}m {}s", time_saved / 60, time_saved % 60)
            } else {
                format!("{}s", time_saved)
            };
            let raw_speedup = if total_time_secs > 0 {
                sequential_time as f64 / total_time_secs as f64
            } else {
                1.0
            };
            let clamped_speedup = if raw_speedup < 1.0 { 1.0 } else { raw_speedup };
            format!(
                "- **Time saved:** {} (parallelism)\n\
                 - **Speedup:** {:.1}x\n",
                time_saved_str, clamped_speedup
            )
        } else {
            String::new()
        };
        body.push_str(&format!(
            "\n## Stats\n\n\
             - **Wall time:** {}\n\
             {}\
             - **Files created:** {}\n\
             - **Files modified:** {}\n\
             - **Stories:** {}/{} completed, {} skipped\n",
            wall_time_str,
            parallelism_stats,
            total_files_created,
            total_files_modified,
            completed,
            total_stories,
            skipped
        ));

        // Review section
        body.push_str(&format!(
            "\n## Review\n\n\
             - **Review cycles:** {}\n\
             - **Fixes auto-applied:** {}\n",
            review_cycles, review_fixes_applied
        ));

        // Footer
        body.push_str(
            "\n---\n\nBuilt with [baro](https://www.npmjs.com/package/baro-ai) \
             — Background Agent Runtime Orchestrator\n",
        );

        let pr_output = Command::new("gh")
            .args([
                "pr",
                "create",
                "--title",
                &current_prd.project,
                "--body",
                &body,
                "--base",
                "main",
                "--head",
                &branch,
            ])
            .current_dir(&cwd)
            .output()
            .await
            .ok()?;

        if pr_output.status.success() {
            let stdout = String::from_utf8_lossy(&pr_output.stdout)
                .trim()
                .to_string();
            if stdout.is_empty() {
                None
            } else {
                Some(stdout)
            }
        } else {
            let stderr = String::from_utf8_lossy(&pr_output.stderr);
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("PR creation failed: {}", stderr),
                })
                .await;
            None
        }
    }
    .await;

    let _ = tx.send(BaroEvent::FinalizeComplete { pr_url }).await;
}

// ─── Helper: Build PrdFile from ReviewStories ───────────────

pub fn prd_from_review(
    project: &str,
    branch_name: &str,
    description: &str,
    stories: &[ReviewStory],
) -> PrdFile {
    PrdFile {
        project: project.to_string(),
        branch_name: branch_name.to_string(),
        description: description.to_string(),
        user_stories: stories
            .iter()
            .enumerate()
            .map(|(i, s)| PrdStory {
                id: s.id.clone(),
                priority: (i + 1) as i32,
                title: s.title.clone(),
                description: s.description.clone(),
                depends_on: s.depends_on.clone(),
                retries: 2,
                acceptance: Vec::new(),
                tests: Vec::new(),
                passes: false,
                completed_at: None,
                duration_secs: None,
                model: s.model.clone(),
            })
            .collect(),
    }
}

/// Write PRD to disk
pub fn write_prd(prd: &PrdFile, cwd: &Path) -> std::io::Result<()> {
    let prd_path = cwd.join("prd.json");
    let content = serde_json::to_string_pretty(prd)
        .map_err(std::io::Error::other)?;
    std::fs::write(prd_path, format!("{}\n", content))
}
