use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use tokio::process::Command;
use tokio::sync::{mpsc, Mutex, Semaphore};

use crate::app::ReviewStory;
use crate::dag::build_dag;
use crate::events::BaroEvent;
use crate::utils::{extract_json, format_commas, BaroResult};

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

/// Configuration passed to [`run_executor`] and friends. The legacy
/// in-process executor is no longer invoked at runtime (see
/// `orchestrator_client.rs`); this struct survives only as the call-site
/// shape `spawn_executor` accepts before forwarding to the TS orchestrator.
pub struct ExecutorConfig {
    pub parallel: u32,
    pub timeout_secs: u64,
    pub model_routing: bool,
    pub override_model: Option<String>,
    pub context_content: Option<String>,
    pub with_critic: bool,
    pub critic_model: Option<String>,
    pub with_librarian: bool,
    pub with_sentry: bool,
}

/// Per-story execution parameters (avoids too-many-arguments).
struct StoryExecConfig<'a> {
    timeout_secs: u64,
    model: Option<String>,
    context: Option<&'a str>,
}

/// Shared parameters threaded through DAG-level execution helpers.
struct DagExecParams<'a> {
    cwd: &'a Path,
    prd_path: &'a Path,
    tx: &'a mpsc::Sender<BaroEvent>,
    git_mutex: &'a Arc<Mutex<()>>,
    timeout_secs: u64,
    model_routing: bool,
    override_model: &'a Option<String>,
    context: Option<&'a str>,
}

// ─── Story prompt builder ───────────────────────────────────

fn build_prompt(story: &PrdStory, cwd: &Path, context: Option<&str>) -> String {
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
            "IMPORTANT: Before committing, you MUST verify the project builds successfully:",
            "- If Cargo.toml exists: run cargo build and fix any errors or warnings",
            "- If package.json exists: run npm run build (if build script exists) and fix errors",
            "- If go.mod exists: run go build ./... and fix errors",
            "- If Makefile exists: run make and fix errors",
            "- Fix ALL compiler warnings, not just errors",
            "- Do NOT use #[allow(dead_code)] or similar suppressions - fix the root cause",
            "",
            "Run tests: TEST_COMMANDS",
            "If build passes and tests pass, commit: git add . && git commit -m \"feat(STORY_ID): STORY_TITLE\"",
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

    let template_result = template
        .replace("STORY_ID", &story.id)
        .replace("STORY_TITLE", &story.title)
        .replace("STORY_DESCRIPTION", &story.description)
        .replace("ACCEPTANCE_CRITERIA", &acceptance)
        .replace("TEST_COMMANDS", &tests);

    match context {
        Some(ctx) => format!("Here is the project context:\n{}\n\n{}", ctx, template_result),
        None => template_result,
    }
}

// ─── Claude stream-json parser ──────────────────────────────


pub struct ParseResult {
    pub logs: Vec<String>,
    pub tokens: Option<(u64, u64)>,
}

pub fn parse_claude_stream_line(line: &str, _story_id: &str) -> ParseResult {
    let mut logs = Vec::new();

    let Ok(ev) = serde_json::from_str::<serde_json::Value>(line) else {
        // Not JSON, emit as raw log
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            logs.push(trimmed.to_string());
        }
        return ParseResult { logs, tokens: None };
    };

    let ev_type = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let mut tokens: Option<(u64, u64)> = None;

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
            if let Some(usage) = ev.get("message").and_then(|m| m.get("usage")) {
                let input_tokens = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                tokens = Some((input_tokens, output_tokens));
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
            if let Some(usage) = ev.get("usage") {
                let input_tokens = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                let output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                tokens = Some((input_tokens, output_tokens));
            }
        }
        _ => {}
    }

    // Fallback: check root-level usage for any event type not already captured
    if tokens.is_none() {
        if let Some(usage) = ev.get("usage") {
            let input_tokens = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            tokens = Some((input_tokens, output_tokens));
        }
    }

    ParseResult { logs, tokens }
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
    cfg: StoryExecConfig<'_>,
) -> BaroResult<(u64, u32, u32, u64, u64)> {
    let StoryExecConfig { timeout_secs, model, context } = cfg;
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
        // Also capture the pre-story HEAD sha for accurate diff stats
        let pre_story_sha = {
            let _git_lock = git_mutex.lock().await;
            crate::git::safe_pull_rebase(cwd, &story.id, tx).await;
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(cwd)
                .output()
                .await
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };

        let start = Instant::now();
        let prompt = build_prompt(story, cwd, context);

        let result =
            run_claude_for_story(&story.id, &prompt, cwd, tx, timeout_secs, &model).await;

        let duration_secs = start.elapsed().as_secs();

        match result {
            Ok((story_input_tokens, story_output_tokens)) => {
                // Acquire git mutex for prd update and git stats
                let (files_created, files_modified) = {
                    let _git_lock = git_mutex.lock().await;

                    // Update prd.json
                    let _ = crate::git::update_prd_story(prd_path, &story.id, duration_secs);

                    // Get git stats using pre-story sha for accurate multi-commit diff
                    crate::git::get_git_file_stats(cwd, pre_story_sha.as_deref()).await
                };

                // Push with retry (acquires git_mutex internally)
                let push_result =
                    crate::git::git_push_with_retry(git_mutex, cwd, &story.id, tx).await;
                let (push_success, push_error) = match &push_result {
                    Ok(()) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                };
                let _ = tx
                    .send(BaroEvent::PushStatus {
                        id: story.id.clone(),
                        success: push_success,
                        error: push_error,
                    })
                    .await;

                return Ok((duration_secs, files_created, files_modified, story_input_tokens, story_output_tokens));
            }
            Err(err) => {
                let _ = tx
                    .send(BaroEvent::StoryError {
                        id: story.id.clone(),
                        error: err.to_string(),
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

    Err("All attempts exhausted".into())
}

async fn run_claude_for_story(
    story_id: &str,
    prompt: &str,
    cwd: &Path,
    tx: &mpsc::Sender<BaroEvent>,
    timeout_secs: u64,
    model: &Option<String>,
) -> BaroResult<(u64, u64)> {
    let config = crate::claude_runner::ClaudeRunConfig {
        prompt: prompt.to_string(),
        cwd: cwd.to_path_buf(),
        model: model.clone(),
        timeout_secs,
        stream_json: true,
    };

    let result = crate::claude_runner::spawn_claude_and_stream(
        &config,
        story_id,
        tx,
        parse_claude_stream_line,
    )
    .await?;

    Ok((result.input_tokens, result.output_tokens))
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

// ─── Final build verification with haiku ────────────────────

async fn verify_build_with_haiku(
    build_output: &str,
    tx: &mpsc::Sender<BaroEvent>,
    override_model: &Option<String>,
    model_routing: bool,
) {
    let verification_model = resolve_model(override_model, &None, model_routing, "review");
    let model_label = verification_model.as_deref().unwrap_or("default");

    let _ = tx
        .send(BaroEvent::StoryLog {
            id: "finalize".to_string(),
            line: format!("[model] final build verification using {}", model_label),
        })
        .await;

    let prompt = format!(
        "Analyze this build output and determine if the build succeeded or failed.\n\
         Respond with ONLY valid JSON (no markdown fences):\n\
         {{\"passed\": boolean, \"summary\": \"one-line summary of build result\"}}\n\n\
         Build output:\n{}",
        if build_output.len() > crate::constants::BUILD_OUTPUT_TRUNCATION {
            &build_output[..crate::constants::BUILD_OUTPUT_TRUNCATION]
        } else {
            build_output
        }
    );

    let config = crate::claude_runner::ClaudeRunConfig {
        prompt,
        cwd: std::env::current_dir().unwrap_or_default(),
        model: verification_model,
        timeout_secs: 120,
        stream_json: false,
    };

    let output = match crate::claude_runner::spawn_claude_json(&config).await {
        Ok(o) => o,
        Err(e) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[build-verify] failed to spawn: {}", e),
                })
                .await;
            return;
        }
    };

    let json_str = extract_json(&output.stdout);

    #[derive(serde::Deserialize)]
    struct BuildVerificationResult {
        passed: bool,
        summary: String,
    }

    match serde_json::from_str::<BuildVerificationResult>(&json_str) {
        Ok(result) => {
            let status = if result.passed { "PASSED" } else { "FAILED" };
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[build-verify] {} — {}", status, result.summary),
                })
                .await;
        }
        Err(_) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: "[build-verify] could not parse verification result".to_string(),
                })
                .await;
        }
    }
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

/// Parse raw Claude output into a ReviewResult.
/// Handles the Claude JSON wrapper (extracting the "result" field) and then
/// extracts/parses the inner review JSON.
fn parse_review_result(stdout: &str) -> Result<ReviewResult, String> {
    let result_text = match serde_json::from_str::<serde_json::Value>(stdout) {
        Ok(wrapper) => wrapper
            .get("result")
            .and_then(|v| v.as_str())
            .unwrap_or(stdout)
            .to_string(),
        Err(_) => stdout.to_string(),
    };

    let json_str = extract_json(&result_text);
    serde_json::from_str(&json_str).map_err(|e| {
        format!(
            "Failed to parse review JSON: {}. Raw: {}",
            e,
            &json_str[..json_str.len().min(200)]
        )
    })
}

/// Spawn Claude for a single review cycle and return the parsed ReviewResult.
/// Returns `Err(String)` if spawning, waiting, or parsing fails.
async fn run_single_review_cycle(
    prompt: &str,
    review_model: &Option<String>,
    cwd: &Path,
) -> Result<ReviewResult, String> {
    let config = crate::claude_runner::ClaudeRunConfig {
        prompt: prompt.to_string(),
        cwd: cwd.to_path_buf(),
        model: review_model.clone(),
        timeout_secs: 300,
        stream_json: false,
    };

    let output = crate::claude_runner::spawn_claude_json(&config)
        .await
        .map_err(|e| e.to_string())?;

    parse_review_result(&output.stdout)
}

/// Execute fix stories generated by a failed review.
/// Returns the number of fixes (same as `fixes.len()`).
async fn apply_review_fixes(
    fixes: &[ReviewFix],
    level_index: usize,
    params: &DagExecParams<'_>,
) {
    for (i, fix) in fixes.iter().enumerate() {
        let fix_id = format!("S{}-fix{}", level_index, i + 1);
        let _ = params.tx
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

        let fix_model = resolve_model(params.override_model, &None, params.model_routing, "execute");
        match execute_story(&fix_story, params.cwd, params.prd_path, params.tx, params.git_mutex, StoryExecConfig { timeout_secs: params.timeout_secs, model: fix_model, context: params.context })
            .await
        {
            Ok((duration_secs, files_created, files_modified, _, _)) => {
                let _ = params.tx
                    .send(BaroEvent::ReviewLog {
                        line: format!("Fix {} completed", fix_id),
                    })
                    .await;
                let _ = params.tx
                    .send(BaroEvent::StoryComplete {
                        id: fix_id.clone(),
                        duration_secs,
                        files_created,
                        files_modified,
                    })
                    .await;
            }
            Err(e) => {
                let _ = params.tx
                    .send(BaroEvent::ReviewLog {
                        line: format!("Fix {} failed: {}", fix_id, e),
                    })
                    .await;
                let _ = params.tx
                    .send(BaroEvent::StoryComplete {
                        id: fix_id.clone(),
                        duration_secs: 0,
                        files_created: 0,
                        files_modified: 0,
                    })
                    .await;
            }
        }
    }
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
    context: Option<&str>,
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

    let max_cycles = crate::constants::MAX_REVIEW_CYCLES;

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
                let truncated = if output.len() > crate::constants::BUILD_OUTPUT_TRUNCATION {
                    &output[..crate::constants::BUILD_OUTPUT_TRUNCATION]
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
        let base_review_prompt = format!(
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
        let prompt = match context {
            Some(ctx) => format!("Here is the project context:\n{}\n\n{}", ctx, base_review_prompt),
            None => base_review_prompt,
        };

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

        let review = match run_single_review_cycle(&prompt, &review_model, cwd).await {
            Ok(r) => r,
            Err(e) => {
                let is_parse_error = e.starts_with("Failed to parse review JSON");
                let _ = tx
                    .send(BaroEvent::ReviewLog { line: e })
                    .await;
                if is_parse_error {
                    let _ = tx
                        .send(BaroEvent::ReviewComplete {
                            level: level_index,
                            passed: false,
                            fix_count: 0,
                        })
                        .await;
                }
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

        apply_review_fixes(
            &review.fixes,
            level_index,
            &DagExecParams {
                cwd,
                prd_path,
                tx,
                git_mutex,
                timeout_secs,
                model_routing,
                override_model,
                context,
            },
        )
        .await;

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

// ─── Accumulated stats from DAG execution ──────────────────

struct ExecutionStats {
    completed: u32,
    skipped: u32,
    files_created: u32,
    files_modified: u32,
    commits: u32,
    review_cycles: u32,
    review_fixes_applied: u32,
    input_tokens: u64,
    output_tokens: u64,
}

// ─── PR creation data passed to create_pull_request ────────

struct PrData {
    project: String,
    total_time_secs: u64,
    stats: ExecutionStats,
}

// ─── Execute DAG levels ────────────────────────────────────

async fn execute_dag_levels(
    levels: &[crate::dag::DagLevel],
    stories: &[PrdStory],
    semaphore: &Option<Arc<Semaphore>>,
    params: &DagExecParams<'_>,
) -> ExecutionStats {
    let story_map: HashMap<&str, &PrdStory> =
        stories.iter().map(|s| (s.id.as_str(), s)).collect();

    let total: u32 = stories.iter().filter(|s| !s.passes).count() as u32;

    let mut stats = ExecutionStats {
        completed: 0,
        skipped: 0,
        files_created: 0,
        files_modified: 0,
        commits: 0,
        review_cycles: 0,
        review_fixes_applied: 0,
        input_tokens: 0,
        output_tokens: 0,
    };

    for (level_index, level) in levels.iter().enumerate() {
        // Save current commit hash before executing stories in this level
        let saved_hash = {
            let _git_lock = params.git_mutex.lock().await;
            let output = Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(params.cwd)
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
            let cwd = params.cwd.to_path_buf();
            let prd_path = params.prd_path.to_path_buf();
            let tx = params.tx.clone();
            let git_mutex = Arc::clone(params.git_mutex);

            let semaphore = semaphore.clone();
            let story_model =
                resolve_model(params.override_model, &story.model, params.model_routing, "execute");
            let ctx = params.context.map(str::to_string);
            let timeout_secs = params.timeout_secs;
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
                    StoryExecConfig { timeout_secs, model: story_model, context: ctx.as_deref() },
                )
                .await
            });
            handles.push((story_id.clone(), handle));
        }

        let mut level_completed_ids: Vec<String> = Vec::new();

        for (story_id, handle) in handles {
            match handle.await {
                Ok(Ok((duration_secs, files_created, files_modified, story_input, story_output))) => {
                    stats.completed += 1;
                    stats.files_created += files_created;
                    stats.files_modified += files_modified;
                    stats.commits += 1;
                    stats.input_tokens += story_input;
                    stats.output_tokens += story_output;
                    level_completed_ids.push(story_id.clone());

                    let _ = params.tx
                        .send(BaroEvent::StoryComplete {
                            id: story_id,
                            duration_secs,
                            files_created,
                            files_modified,
                        })
                        .await;

                    let percentage = if total > 0 {
                        (stats.completed as f64 / total as f64 * 100.0) as u32
                    } else {
                        0
                    };
                    let _ = params.tx
                        .send(BaroEvent::Progress {
                            completed: stats.completed,
                            total,
                            percentage,
                        })
                        .await;
                }
                Ok(Err(_)) => {
                    stats.skipped += 1;
                }
                Err(e) => {
                    stats.skipped += 1;
                    let _ = params.tx
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
                params.cwd,
                &completed_stories,
                params.tx,
                params.git_mutex,
                params.prd_path,
                level_index,
                params.timeout_secs,
                params.model_routing,
                params.override_model,
                params.context,
            )
            .await;
            stats.review_cycles += cycles;
            stats.review_fixes_applied += fixes;
        }
    }

    stats
}

// ─── Collect stats and emit Done event ─────────────────────

async fn collect_execution_stats(
    tx: &mpsc::Sender<BaroEvent>,
    total_time_secs: u64,
    stats: &ExecutionStats,
) {
    let _ = tx.send(BaroEvent::NotificationReady).await;

    let _ = tx
        .send(BaroEvent::Done {
            total_time_secs,
            stats: crate::events::DoneStats {
                stories_completed: stats.completed,
                stories_skipped: stats.skipped,
                total_commits: stats.commits,
                files_created: stats.files_created,
                files_modified: stats.files_modified,
            },
        })
        .await;
}

// ─── Create GitHub pull request ────────────────────────────

async fn create_pull_request(
    cwd: &Path,
    tx: &mpsc::Sender<BaroEvent>,
    git_mutex: &Arc<Mutex<()>>,
    pr_data: &PrData,
) -> Option<String> {
    // Check if gh CLI is available
    let _ = tx
        .send(BaroEvent::StoryLog {
            id: "finalize".to_string(),
            line: "[pr] checking gh CLI availability...".to_string(),
        })
        .await;
    let gh_check = match Command::new("gh").arg("--version").output().await {
        Ok(output) => output,
        Err(e) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[pr] gh CLI not available: {}", e),
                })
                .await;
            return None;
        }
    };
    let gh_stdout = String::from_utf8_lossy(&gh_check.stdout).trim().to_string();
    let gh_stderr = String::from_utf8_lossy(&gh_check.stderr).trim().to_string();
    let _ = tx
        .send(BaroEvent::StoryLog {
            id: "finalize".to_string(),
            line: format!("[pr] gh --version stdout: {}", gh_stdout),
        })
        .await;
    if !gh_stderr.is_empty() {
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: "finalize".to_string(),
                line: format!("[pr] gh --version stderr: {}", gh_stderr),
            })
            .await;
    }
    if !gh_check.status.success() {
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: "finalize".to_string(),
                line: "[pr] gh CLI not available (non-zero exit)".to_string(),
            })
            .await;
        return None;
    }

    // Get current branch
    let _ = tx
        .send(BaroEvent::StoryLog {
            id: "finalize".to_string(),
            line: "[pr] getting current branch...".to_string(),
        })
        .await;
    let branch = match crate::git::get_current_branch(cwd).await {
        Ok(b) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[pr] current branch: {}", b),
                })
                .await;
            b
        }
        Err(e) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[pr] failed to get current branch: {}", e),
                })
                .await;
            return None;
        }
    };

    // Check if remote branch exists
    let ls_remote = match Command::new("git")
        .args(["ls-remote", "--heads", "origin", &branch])
        .current_dir(cwd)
        .output()
        .await
    {
        Ok(output) => output,
        Err(e) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[git] ls-remote failed: {}", e),
                })
                .await;
            return None;
        }
    };
    let remote_branch_exists = ls_remote.status.success()
        && !String::from_utf8_lossy(&ls_remote.stdout).trim().is_empty();

    let _ = tx
        .send(BaroEvent::StoryLog {
            id: "finalize".to_string(),
            line: format!(
                "[git] remote branch '{}' {}",
                branch,
                if remote_branch_exists { "exists" } else { "does not exist" }
            ),
        })
        .await;

    if remote_branch_exists {
        // Remote branch exists, use normal push with retry
        match crate::git::git_push_with_retry(git_mutex, cwd, "finalize", tx).await {
            Ok(()) => {}
            Err(e) => {
                let _ = tx
                    .send(BaroEvent::StoryLog {
                        id: "finalize".to_string(),
                        line: format!("[git] push failed before PR creation: {}", e),
                    })
                    .await;
            }
        }
    } else {
        // Remote branch does not exist, push with -u to set upstream tracking
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: "finalize".to_string(),
                line: format!("[git] pushing with -u flag to set upstream for '{}'", branch),
            })
            .await;

        let push_output = match Command::new("git")
            .args(["push", "-u", "origin", &branch])
            .current_dir(cwd)
            .output()
            .await
        {
            Ok(output) => output,
            Err(e) => {
                let _ = tx
                    .send(BaroEvent::StoryLog {
                        id: "finalize".to_string(),
                        line: format!("[git] push -u failed to spawn: {}", e),
                    })
                    .await;
                return None;
            }
        };

        if push_output.status.success() {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: "[git] push -u ok".to_string(),
                })
                .await;
        } else {
            let stderr = String::from_utf8_lossy(&push_output.stderr);
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[git] push -u failed: {}", stderr),
                })
                .await;
        }
    }

    // Check if a PR already exists for this branch
    let pr_view = match Command::new("gh")
        .args(["pr", "view", &branch, "--json", "url", "--jq", ".url"])
        .current_dir(cwd)
        .output()
        .await
    {
        Ok(output) => output,
        Err(e) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[pr] failed to check existing PR: {}", e),
                })
                .await;
            return None;
        }
    };

    if pr_view.status.success() {
        let existing_url = String::from_utf8_lossy(&pr_view.stdout).trim().to_string();
        if !existing_url.is_empty() {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[pr] PR already exists: {}", existing_url),
                })
                .await;
            return Some(existing_url);
        }
    }

    // Re-read prd.json from disk for up-to-date completion status
    let _ = tx
        .send(BaroEvent::StoryLog {
            id: "finalize".to_string(),
            line: "[pr] re-reading prd.json...".to_string(),
        })
        .await;
    let prd_data = match tokio::fs::read_to_string(cwd.join("prd.json")).await {
        Ok(data) => data,
        Err(e) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[pr] failed to read prd.json: {}", e),
                })
                .await;
            return None;
        }
    };
    let current_prd: PrdFile = match serde_json::from_str(&prd_data) {
        Ok(prd) => prd,
        Err(e) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[pr] failed to parse prd.json: {}", e),
                })
                .await;
            return None;
        }
    };
    let _ = tx
        .send(BaroEvent::StoryLog {
            id: "finalize".to_string(),
            line: format!(
                "[pr] prd.json loaded: {} stories",
                current_prd.user_stories.len()
            ),
        })
        .await;

    // Calculate per-level parallelism gain using DAG (use all stories, not just incomplete)
    let dag_levels = crate::dag::build_dag_all(&current_prd.user_stories).unwrap_or_default();
    let (level_saved, sequential_time) = {
        let mut tseq = 0u64;
        let mut tpar = 0u64;
        for level in &dag_levels {
            let mut lsum = 0u64;
            let mut lmax = 0u64;
            for sid in &level.story_ids {
                if let Some(s) = current_prd.user_stories.iter().find(|s| s.id == *sid) {
                    if let Some(d) = s.duration_secs {
                        lsum += d;
                        lmax = lmax.max(d);
                    }
                }
            }
            tseq += lsum;
            tpar += lmax;
        }
        (tseq.saturating_sub(tpar), tseq)
    };

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
    let total_time_secs = pr_data.total_time_secs;
    let wall_time_str = if total_time_secs >= 60 {
        format!("{}m {}s", total_time_secs / 60, total_time_secs % 60)
    } else {
        format!("{}s", total_time_secs)
    };
    let parallelism_stats = if level_saved > 0 {
        let time_saved_str = if level_saved >= 60 {
            format!("{}m {}s", level_saved / 60, level_saved % 60)
        } else {
            format!("{}s", level_saved)
        };
        let parallel_time = sequential_time.saturating_sub(level_saved);
        let speedup = if parallel_time > 0 {
            sequential_time as f64 / parallel_time as f64
        } else {
            1.0
        };
        format!(
            "- **Time saved:** {} (parallelism)\n\
             - **Speedup:** {:.1}x\n",
            time_saved_str, speedup
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
         - **Tokens:** {} input / {} output\n\
         - **Stories:** {}/{} completed, {} skipped\n",
        wall_time_str,
        parallelism_stats,
        pr_data.stats.files_created,
        pr_data.stats.files_modified,
        format_commas(pr_data.stats.input_tokens),
        format_commas(pr_data.stats.output_tokens),
        pr_data.stats.completed,
        current_prd.user_stories.len(),
        pr_data.stats.skipped
    ));

    // Review section
    body.push_str(&format!(
        "\n## Review\n\n\
         - **Review cycles:** {}\n\
         - **Fixes auto-applied:** {}\n",
        pr_data.stats.review_cycles, pr_data.stats.review_fixes_applied
    ));

    // Footer
    body.push_str(
        "\n---\n\nBuilt with [baro](https://www.npmjs.com/package/baro-ai) \
         — Background Agent Runtime Orchestrator\n",
    );

    let pr_args = [
        "pr",
        "create",
        "--title",
        &pr_data.project,
        "--body",
        &body,
        "--base",
        "main",
        "--head",
        &branch,
    ];
    let _ = tx
        .send(BaroEvent::StoryLog {
            id: "finalize".to_string(),
            line: format!("[pr] running: gh {}", pr_args.join(" ")),
        })
        .await;

    let pr_output = match Command::new("gh")
        .args(&pr_args)
        .current_dir(cwd)
        .output()
        .await
    {
        Ok(output) => output,
        Err(e) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: format!("[pr] failed to spawn gh pr create: {}", e),
                })
                .await;
            return None;
        }
    };

    let stdout = String::from_utf8_lossy(&pr_output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&pr_output.stderr).trim().to_string();
    let _ = tx
        .send(BaroEvent::StoryLog {
            id: "finalize".to_string(),
            line: format!("[pr] gh pr create stdout: {}", stdout),
        })
        .await;
    if !stderr.is_empty() {
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: "finalize".to_string(),
                line: format!("[pr] gh pr create stderr: {}", stderr),
            })
            .await;
    }

    if pr_output.status.success() {
        if stdout.is_empty() {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: "[pr] gh pr create succeeded but returned empty stdout".to_string(),
                })
                .await;
            None
        } else {
            // Verify PR by running gh pr view
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: "finalize".to_string(),
                    line: "[pr] verifying PR with gh pr view...".to_string(),
                })
                .await;
            match Command::new("gh")
                .args(["pr", "view", "--json", "url", "--jq", ".url"])
                .current_dir(cwd)
                .output()
                .await
            {
                Ok(verify_output) => {
                    let verified_url =
                        String::from_utf8_lossy(&verify_output.stdout).trim().to_string();
                    let _ = tx
                        .send(BaroEvent::StoryLog {
                            id: "finalize".to_string(),
                            line: format!("[pr] verified PR URL: {}", verified_url),
                        })
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(BaroEvent::StoryLog {
                            id: "finalize".to_string(),
                            line: format!("[pr] PR verification failed: {}", e),
                        })
                        .await;
                }
            }
            Some(stdout)
        }
    } else {
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: "finalize".to_string(),
                line: format!("[pr] PR creation failed (exit {}): {}", pr_output.status, stderr),
            })
            .await;
        None
    }
}

// ─── Main executor entry point ──────────────────────────────

pub async fn run_executor(
    prd: PrdFile,
    cwd: PathBuf,
    tx: mpsc::Sender<BaroEvent>,
    config: ExecutorConfig,
) {
    let ExecutorConfig { parallel, timeout_secs, model_routing, override_model, context_content, .. } = config;
    let prd_path = cwd.join("prd.json");
    let stories = &prd.user_stories;
    let incomplete: Vec<&PrdStory> = stories.iter().filter(|s| !s.passes).collect();

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

    // Create semaphore for parallelism limiting (0 = unlimited)
    let semaphore = if parallel > 0 {
        Some(Arc::new(Semaphore::new(parallel as usize)))
    } else {
        None
    };

    let start = Instant::now();

    // Execute level by level
    let stats = execute_dag_levels(
        &levels,
        stories,
        &semaphore,
        &DagExecParams {
            cwd: &cwd,
            prd_path: &prd_path,
            tx: &tx,
            git_mutex: &git_mutex,
            timeout_secs,
            model_routing,
            override_model: &override_model,
            context: context_content.as_deref(),
        },
    )
    .await;

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
                ).into());
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
                    return Err(format!("git commit failed: {}", stderr).into());
                }
            }
        }

        crate::git::git_push_with_retry(&git_mutex, &cwd, "prd", &tx).await
    }
    .await;

    let total_time_secs = start.elapsed().as_secs();

    // Signal notifications and emit Done event with stats
    collect_execution_stats(&tx, total_time_secs, &stats).await;

    // ─── Finalize phase ─────────────────────────────────────────
    let _ = tx.send(BaroEvent::FinalizeStart).await;

    // Step 1: Run build detection and verify with haiku model
    if let Some(output) = detect_project_and_build(&cwd).await {
        verify_build_with_haiku(&output, &tx, &override_model, model_routing).await;
    }

    // Step 2: Try to create a GitHub PR
    let pr_data = PrData {
        project: prd.project.clone(),
        total_time_secs,
        stats,
    };
    let pr_url = create_pull_request(&cwd, &tx, &git_mutex, &pr_data).await;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(v: serde_json::Value) -> String {
        v.to_string()
    }

    // 1) result event with usage field returns correct tokens
    #[test]
    fn test_result_event_usage() {
        let ev = line(json!({
            "type": "result",
            "result": "Task complete",
            "usage": { "input_tokens": 100, "output_tokens": 50 }
        }));
        let r = parse_claude_stream_line(&ev, "s1");
        assert_eq!(r.tokens, Some((100, 50)));
    }

    // 2) assistant event with message.usage returns correct tokens
    #[test]
    fn test_assistant_event_message_usage() {
        let ev = line(json!({
            "type": "assistant",
            "message": {
                "content": [],
                "usage": { "input_tokens": 200, "output_tokens": 75 }
            }
        }));
        let r = parse_claude_stream_line(&ev, "s1");
        assert_eq!(r.tokens, Some((200, 75)));
    }

    // 3) event with root-level usage returns correct tokens (fallback path)
    #[test]
    fn test_root_level_usage_fallback() {
        let ev = line(json!({
            "type": "unknown_event",
            "usage": { "input_tokens": 300, "output_tokens": 120 }
        }));
        let r = parse_claude_stream_line(&ev, "s1");
        assert_eq!(r.tokens, Some((300, 120)));
    }

    // 4) non-JSON line returns no tokens
    #[test]
    fn test_non_json_line_no_tokens() {
        let r = parse_claude_stream_line("this is not json", "s1");
        assert_eq!(r.tokens, None);
        assert_eq!(r.logs, vec!["this is not json"]);
    }

    // 5) assistant event without usage returns None for tokens
    #[test]
    fn test_assistant_event_no_usage() {
        let ev = line(json!({
            "type": "assistant",
            "message": {
                "content": [{ "type": "text", "text": "Hello" }]
            }
        }));
        let r = parse_claude_stream_line(&ev, "s1");
        assert_eq!(r.tokens, None);
        assert!(r.logs.contains(&"Hello".to_string()));
    }

    // 6) tool_use blocks are logged correctly
    #[test]
    fn test_tool_use_logged() {
        let ev = line(json!({
            "type": "assistant",
            "message": {
                "content": [{
                    "type": "tool_use",
                    "name": "bash",
                    "input": { "command": "ls -la" }
                }]
            }
        }));
        let r = parse_claude_stream_line(&ev, "s1");
        assert!(r.logs.iter().any(|l| l.starts_with("⚙ bash")));
    }

    // 7) realistic multi-turn scenario where last event has cumulative usage
    #[test]
    fn test_multi_turn_cumulative_usage() {
        // First turn: assistant responds, some tokens used
        let turn1 = line(json!({
            "type": "assistant",
            "message": {
                "content": [{ "type": "text", "text": "I'll help with that." }],
                "usage": { "input_tokens": 50, "output_tokens": 10 }
            }
        }));
        // Last turn: result event with cumulative totals
        let turn_final = line(json!({
            "type": "result",
            "result": "Done",
            "usage": { "input_tokens": 1500, "output_tokens": 400 }
        }));

        let r1 = parse_claude_stream_line(&turn1, "s1");
        assert_eq!(r1.tokens, Some((50, 10)));

        let r_final = parse_claude_stream_line(&turn_final, "s1");
        assert_eq!(r_final.tokens, Some((1500, 400)));
    }
}
