use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

use crate::events::BaroEvent;
use crate::utils::BaroResult;

/// Configuration for spawning a Claude CLI process.
pub struct ClaudeRunConfig {
    pub prompt: String,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub timeout_secs: u64,
    pub stream_json: bool,
}

/// Output from a non-streaming (JSON mode) Claude invocation.
pub struct ClaudeJsonOutput {
    pub stdout: String,
}

/// Output from a streaming Claude invocation.
pub struct ClaudeStreamOutput {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Build the common `Command` for spawning Claude CLI.
fn build_claude_command(config: &ClaudeRunConfig) -> Command {
    let mut cmd = Command::new("claude");

    let mut args: Vec<&str> = vec![
        "--dangerously-skip-permissions",
        "--output-format",
    ];
    if config.stream_json {
        args.push("stream-json");
        args.push("--verbose");
    } else {
        args.push("json");
    }

    cmd.args(&args);

    if let Some(ref m) = config.model {
        cmd.arg("--model").arg(m);
    }

    cmd.arg("-p").arg(&config.prompt);
    cmd.current_dir(&config.cwd);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    cmd
}

/// Spawn Claude with `--output-format json`, wait for completion, and return
/// the raw stdout/stderr. The caller is responsible for parsing the output.
pub async fn spawn_claude_json(config: &ClaudeRunConfig) -> BaroResult<ClaudeJsonOutput> {
    let child = build_claude_command(config)
        .spawn()
        .map_err(|e| format!("Failed to spawn claude: {}", e))?;

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("Claude process error: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(
            format!("Claude exited with code {}: {}", output.status.code().unwrap_or(-1), stderr)
                .into(),
        );
    }

    Ok(ClaudeJsonOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
    })
}

/// Spawn Claude with `--output-format stream-json`, parse streaming output,
/// forward logs and token usage via the provided channel, and enforce a timeout.
///
/// This is the streaming counterpart of [`spawn_claude_json`]. It reads stdout
/// line-by-line, parses each line with `parse_line_fn`, and sends events through `tx`.
pub async fn spawn_claude_and_stream(
    config: &ClaudeRunConfig,
    story_id: &str,
    tx: &mpsc::Sender<BaroEvent>,
    parse_line_fn: fn(&str, &str) -> crate::executor::ParseResult,
) -> BaroResult<ClaudeStreamOutput> {
    let mut child = build_claude_command(config)
        .spawn()
        .map_err(|e| format!("Failed to spawn claude: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let story_id_owned = story_id.to_string();
    let tx_clone = tx.clone();

    let result = timeout(Duration::from_secs(config.timeout_secs), async {
        let story_id_out = story_id_owned.clone();
        let tx_out = tx_clone.clone();
        let stdout_task = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut acc_input: u64 = 0;
            let mut acc_output: u64 = 0;
            while let Ok(Some(line)) = lines.next_line().await {
                let parsed = parse_line_fn(&line, &story_id_out);
                for log in parsed.logs {
                    let _ = tx_out
                        .send(BaroEvent::StoryLog {
                            id: story_id_out.clone(),
                            line: log,
                        })
                        .await;
                }
                if let Some((input_tokens, output_tokens)) = parsed.tokens {
                    acc_input += input_tokens;
                    acc_output += output_tokens;
                    let _ = tx_out
                        .send(BaroEvent::TokenUsage {
                            id: story_id_out.clone(),
                            input_tokens,
                            output_tokens,
                        })
                        .await;
                }
            }
            (acc_input, acc_output)
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

        let token_totals = stdout_task.await.unwrap_or((0, 0));
        let _ = stderr_task.await;

        let status = child
            .wait()
            .await
            .map_err(|e| format!("Failed to wait for claude: {}", e))?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>((status, token_totals))
    })
    .await;

    match result {
        Ok(wait_result) => {
            let (status, (input_tokens, output_tokens)) = wait_result?;
            if status.success() {
                Ok(ClaudeStreamOutput {
                    input_tokens,
                    output_tokens,
                })
            } else {
                Err(
                    format!("claude exited with code {}", status.code().unwrap_or(-1)).into(),
                )
            }
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = tx
                .send(BaroEvent::StoryLog {
                    id: story_id.to_string(),
                    line: format!(
                        "[timeout] Story {} killed after {}s",
                        story_id, config.timeout_secs
                    ),
                })
                .await;
            Err(format!("Story timed out after {}s", config.timeout_secs).into())
        }
    }
}
