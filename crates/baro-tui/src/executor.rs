use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};

use crate::app::ReviewStory;
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
}

fn default_retries() -> u32 {
    2
}

// ─── DAG Engine (Kahn's algorithm) ──────────────────────────

struct DagLevel {
    story_ids: Vec<String>,
}

fn build_dag(stories: &[PrdStory]) -> Result<Vec<DagLevel>, String> {
    let incomplete: Vec<&PrdStory> = stories.iter().filter(|s| !s.passes).collect();
    let completed_ids: std::collections::HashSet<&str> = stories
        .iter()
        .filter(|s| s.passes)
        .map(|s| s.id.as_str())
        .collect();
    let story_map: HashMap<&str, &PrdStory> =
        incomplete.iter().map(|s| (s.id.as_str(), *s)).collect();

    // Build in-degree and reverse edges
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for s in &incomplete {
        let active_deps: Vec<&str> = s
            .depends_on
            .iter()
            .map(|d| d.as_str())
            .filter(|d| story_map.contains_key(d) && !completed_ids.contains(d))
            .collect();
        in_degree.insert(s.id.as_str(), active_deps.len());
        for dep in active_deps {
            dependents.entry(dep).or_default().push(s.id.as_str());
        }
    }

    let mut levels: Vec<DagLevel> = Vec::new();
    let mut queue: Vec<&PrdStory> = incomplete
        .iter()
        .filter(|s| *in_degree.get(s.id.as_str()).unwrap_or(&0) == 0)
        .copied()
        .collect();

    while !queue.is_empty() {
        queue.sort_by_key(|s| s.priority);
        let ids: Vec<String> = queue.iter().map(|s| s.id.clone()).collect();
        levels.push(DagLevel { story_ids: ids });

        let mut next_queue: Vec<&PrdStory> = Vec::new();
        for s in &queue {
            if let Some(deps) = dependents.get(s.id.as_str()) {
                for dep_id in deps {
                    if let Some(deg) = in_degree.get_mut(dep_id) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            if let Some(story) = story_map.get(dep_id) {
                                next_queue.push(story);
                            }
                        }
                    }
                }
            }
        }
        queue = next_queue;
    }

    // Cycle detection
    let total_in_levels: usize = levels.iter().map(|l| l.story_ids.len()).sum();
    if total_in_levels != incomplete.len() {
        let placed: std::collections::HashSet<&str> = levels
            .iter()
            .flat_map(|l| l.story_ids.iter().map(|s| s.as_str()))
            .collect();
        let cycled: Vec<&str> = incomplete
            .iter()
            .filter(|s| !placed.contains(s.id.as_str()))
            .map(|s| s.id.as_str())
            .collect();
        return Err(format!("Dependency cycle detected: {}", cycled.join(", ")));
    }

    Ok(levels)
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

// ─── Git file stats ─────────────────────────────────────────

async fn get_git_file_stats(cwd: &Path) -> (u32, u32) {
    let output = Command::new("git")
        .args(["diff", "--name-status", "HEAD~1", "HEAD"])
        .current_dir(cwd)
        .output()
        .await;

    let Ok(output) = output else {
        return (0, 0);
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut created = 0u32;
    let mut modified = 0u32;
    for line in text.lines() {
        if line.starts_with('A') {
            created += 1;
        } else if line.starts_with('M') || line.starts_with('R') {
            modified += 1;
        }
    }
    (created, modified)
}

// ─── Update prd.json ────────────────────────────────────────

fn update_prd_story(prd_path: &Path, story_id: &str, duration_secs: u64) -> std::io::Result<()> {
    let content = std::fs::read_to_string(prd_path)?;
    let mut prd: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    if let Some(stories) = prd.get_mut("userStories").and_then(|s| s.as_array_mut()) {
        for story in stories {
            if story.get("id").and_then(|id| id.as_str()) == Some(story_id) {
                story["passes"] = serde_json::Value::Bool(true);
                story["completedAt"] =
                    serde_json::Value::String(chrono::Utc::now().to_rfc3339());
                story["durationSecs"] =
                    serde_json::Value::Number(serde_json::Number::from(duration_secs));
            }
        }
    }

    let output = serde_json::to_string_pretty(&prd)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(prd_path, format!("{}\n", output))?;
    Ok(())
}

// ─── Push with retry ────────────────────────────────────────

async fn git_push_with_retry(
    git_mutex: &Mutex<()>,
    cwd: &Path,
    story_id: &str,
    tx: &mpsc::Sender<BaroEvent>,
) -> Result<(), String> {
    let _git_lock = git_mutex.lock().await;

    // Get current branch
    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| format!("Failed to get branch: {}", e))?;

    let branch_name = String::from_utf8_lossy(&branch.stdout).trim().to_string();
    if branch_name.is_empty() {
        return Err("Could not determine current branch".to_string());
    }

    let max_attempts = 3;
    for attempt in 1..=max_attempts {
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: story_id.to_string(),
                line: "[git] pushing...".to_string(),
            })
            .await;

        let push = Command::new("git")
            .args(["push", "origin", &branch_name])
            .current_dir(cwd)
            .output()
            .await
            .map_err(|e| format!("Failed to push: {}", e))?;

        if push.status.success() {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: story_id.to_string(),
                    line: "[git] push ok".to_string(),
                })
                .await;
            return Ok(());
        }

        if attempt == max_attempts {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: story_id.to_string(),
                    line: "[git] push failed after 3 attempts".to_string(),
                })
                .await;
            let stderr = String::from_utf8_lossy(&push.stderr).trim().to_string();
            return Err(format!("Push failed after 3 attempts: {}", stderr));
        }

        // Pull --rebase and retry
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: story_id.to_string(),
                line: "[git] push failed, pulling and retrying...".to_string(),
            })
            .await;

        let pull = Command::new("git")
            .args(["pull", "--rebase", "origin", &branch_name])
            .current_dir(cwd)
            .output()
            .await
            .map_err(|e| format!("Failed to pull --rebase: {}", e))?;

        if !pull.status.success() {
            // Rebase conflict — abort and return error
            let _ = Command::new("git")
                .args(["rebase", "--abort"])
                .current_dir(cwd)
                .output()
                .await;

            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: story_id.to_string(),
                    line: "[git] conflict detected, skipping".to_string(),
                })
                .await;
            return Err("Rebase conflict detected, push skipped".to_string());
        }
    }

    unreachable!()
}

// ─── Execute a single story ─────────────────────────────────

async fn execute_story(
    story: &PrdStory,
    cwd: &Path,
    prd_path: &Path,
    tx: &mpsc::Sender<BaroEvent>,
    git_mutex: &Mutex<()>,
) -> Result<(u64, u32, u32), String> {
    let max_attempts = story.retries + 1;

    for attempt in 1..=max_attempts {
        let _ = tx
            .send(BaroEvent::StoryStart {
                id: story.id.clone(),
                title: story.title.clone(),
            })
            .await;

        // Git pull --rebase before running claude
        {
            let _git_lock = git_mutex.lock().await;

            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: story.id.clone(),
                    line: "[git] pulling latest...".to_string(),
                })
                .await;

            // Get current branch
            let branch_output = Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(cwd)
                .output()
                .await
                .map_err(|e| format!("Failed to get branch: {}", e))?;

            let branch_name = String::from_utf8_lossy(&branch_output.stdout)
                .trim()
                .to_string();

            let pull_output = Command::new("git")
                .args(["pull", "--rebase", "origin", &branch_name])
                .current_dir(cwd)
                .output()
                .await
                .map_err(|e| format!("Failed to run git pull: {}", e))?;

            if pull_output.status.success() {
                let _ = tx
                    .send(BaroEvent::StoryLog {
                        id: story.id.clone(),
                        line: "[git] pull ok".to_string(),
                    })
                    .await;
            } else {
                // Abort the failed rebase
                let _ = Command::new("git")
                    .args(["rebase", "--abort"])
                    .current_dir(cwd)
                    .output()
                    .await;

                let _ = tx
                    .send(BaroEvent::StoryLog {
                        id: story.id.clone(),
                        line: "[git] conflict detected on pull, skipping".to_string(),
                    })
                    .await;

                let _ = tx
                    .send(BaroEvent::StoryError {
                        id: story.id.clone(),
                        error: "git pull --rebase conflict".to_string(),
                        attempt,
                        max_retries: max_attempts,
                    })
                    .await;

                return Err("git pull --rebase conflict".to_string());
            }
        }

        let start = Instant::now();
        let prompt = build_prompt(story, cwd);

        let result = run_claude_for_story(&story.id, &prompt, cwd, tx).await;

        let duration_secs = start.elapsed().as_secs();

        match result {
            Ok(()) => {
                // Acquire git mutex for prd update and git stats
                let (files_created, files_modified) = {
                    let _git_lock = git_mutex.lock().await;

                    // Update prd.json
                    let _ = update_prd_story(prd_path, &story.id, duration_secs);

                    // Get git stats
                    get_git_file_stats(cwd).await
                };

                // Push with retry (acquires git_mutex internally)
                let push_result =
                    git_push_with_retry(git_mutex, cwd, &story.id, tx).await;
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
) -> Result<(), String> {
    let mut child = Command::new("claude")
        .args([
            "--dangerously-skip-permissions",
            "--output-format",
            "stream-json",
            "--verbose",
            "-p",
            prompt,
        ])
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn claude: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let story_id_out = story_id.to_string();
    let tx_out = tx.clone();
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

    let story_id_err = story_id.to_string();
    let tx_err = tx.clone();
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

    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for claude: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("claude exited with code {}", status.code().unwrap_or(-1)))
    }
}

// ─── Main executor entry point ──────────────────────────────

pub async fn run_executor(
    prd: PrdFile,
    cwd: PathBuf,
    tx: mpsc::Sender<BaroEvent>,
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
                            title: s.title.clone(),
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

    // Execute level by level
    let story_map: HashMap<&str, &PrdStory> =
        stories.iter().map(|s| (s.id.as_str(), s)).collect();

    for level in &levels {
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

            let handle = tokio::spawn(async move {
                execute_story(&story, &cwd, &prd_path, &tx, &git_mutex).await
            });
            handles.push((story_id.clone(), handle));
        }

        for (story_id, handle) in handles {
            match handle.await {
                Ok(Ok((duration_secs, files_created, files_modified))) => {
                    completed += 1;
                    total_files_created += files_created;
                    total_files_modified += files_modified;
                    total_commits += 1;

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

        git_push_with_retry(&git_mutex, &cwd, "prd", &tx).await
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
            })
            .collect(),
    }
}

/// Write PRD to disk
pub fn write_prd(prd: &PrdFile, cwd: &Path) -> std::io::Result<()> {
    let prd_path = cwd.join("prd.json");
    let content = serde_json::to_string_pretty(prd)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(prd_path, format!("{}\n", content))
}
