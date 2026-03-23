use std::path::Path;

use tokio::process::Command;
use tokio::sync::Mutex;

use crate::events::BaroEvent;
use tokio::sync::mpsc;

// ─── Get current branch ──────────────────────────────────────

pub(crate) async fn get_current_branch(cwd: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| format!("Failed to get branch: {}", e))?;

    let branch_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch_name.is_empty() {
        return Err("Could not determine current branch".to_string());
    }
    Ok(branch_name)
}

// ─── Safe pull rebase (best-effort, never fatal) ────────────

pub(crate) async fn safe_pull_rebase(
    cwd: &Path,
    story_id: &str,
    tx: &mpsc::Sender<BaroEvent>,
) {
    // Check if remote "origin" exists
    let remote_check = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(cwd)
        .output()
        .await;

    let has_remote = remote_check.map(|o| o.status.success()).unwrap_or(false);
    if !has_remote {
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: story_id.to_string(),
                line: "[git] no remote, skipping pull".to_string(),
            })
            .await;
        return;
    }

    let branch_name = match get_current_branch(cwd).await {
        Ok(b) => b,
        Err(_) => {
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: story_id.to_string(),
                    line: "[git] no branch, skipping pull".to_string(),
                })
                .await;
            return;
        }
    };

    // Check if remote branch exists
    let remote_branch_check = Command::new("git")
        .args(["ls-remote", "--heads", "origin", &branch_name])
        .current_dir(cwd)
        .output()
        .await;

    let has_remote_branch = remote_branch_check
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false);

    if !has_remote_branch {
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: story_id.to_string(),
                line: "[git] remote branch not found, skipping pull".to_string(),
            })
            .await;
        return;
    }

    let _ = tx
        .send(BaroEvent::StoryLog {
            id: story_id.to_string(),
            line: "[git] pulling latest...".to_string(),
        })
        .await;

    // Stash any local changes (prd.json updates etc.)
    let _ = Command::new("git")
        .args(["stash", "--include-untracked"])
        .current_dir(cwd)
        .output()
        .await;

    // Pull --rebase
    let pull = Command::new("git")
        .args(["pull", "--rebase", "origin", &branch_name])
        .current_dir(cwd)
        .output()
        .await;

    let pull_ok = pull.map(|o| o.status.success()).unwrap_or(false);

    if !pull_ok {
        // Abort failed rebase
        let _ = Command::new("git")
            .args(["rebase", "--abort"])
            .current_dir(cwd)
            .output()
            .await;

        let _ = tx
            .send(BaroEvent::StoryLog {
                id: story_id.to_string(),
                line: "[git] pull conflict, continuing without pull".to_string(),
            })
            .await;
    } else {
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: story_id.to_string(),
                line: "[git] pull ok".to_string(),
            })
            .await;
    }

    // Pop stash (best-effort)
    let _ = Command::new("git")
        .args(["stash", "pop"])
        .current_dir(cwd)
        .output()
        .await;
}

// ─── Git file stats ──────────────────────────────────────────

pub(crate) async fn get_git_file_stats(cwd: &Path) -> (u32, u32) {
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

// ─── Update prd.json ─────────────────────────────────────────

pub(crate) fn update_prd_story(prd_path: &Path, story_id: &str, duration_secs: u64) -> std::io::Result<()> {
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
        .map_err(std::io::Error::other)?;
    std::fs::write(prd_path, format!("{}\n", output))?;
    Ok(())
}

// ─── Push with retry ─────────────────────────────────────────

pub(crate) async fn git_push_with_retry(
    git_mutex: &Mutex<()>,
    cwd: &Path,
    story_id: &str,
    tx: &mpsc::Sender<BaroEvent>,
) -> Result<(), String> {
    let _git_lock = git_mutex.lock().await;

    // Check if remote exists
    let remote_check = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(cwd)
        .output()
        .await;
    if !remote_check.map(|o| o.status.success()).unwrap_or(false) {
        let _ = tx
            .send(BaroEvent::StoryLog {
                id: story_id.to_string(),
                line: "[git] no remote, skipping push".to_string(),
            })
            .await;
        return Ok(());
    }

    let branch_name = get_current_branch(cwd).await?;

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
