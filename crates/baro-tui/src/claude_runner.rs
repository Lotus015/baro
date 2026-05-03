//! Thin wrapper around `claude --print --output-format json` for the
//! non-streaming planner step in main.rs (Claude planner).
//!
//! The streaming-json variant lived here too, but the TS Mozaik
//! orchestrator now owns story execution end-to-end so the streaming
//! half is gone. This file is a single-shot run helper.

use std::path::PathBuf;

use tokio::process::Command;

use crate::utils::BaroResult;

/// Configuration for spawning a Claude CLI process in JSON mode.
pub struct ClaudeRunConfig {
    pub prompt: String,
    pub cwd: PathBuf,
    pub model: Option<String>,
}

/// Output from a non-streaming (JSON mode) Claude invocation.
pub struct ClaudeJsonOutput {
    pub stdout: String,
}

/// Spawn Claude with `--print --output-format json`, wait for completion,
/// return the raw stdout. The caller is responsible for parsing the
/// JSON wrapper Claude emits.
pub async fn spawn_claude_json(config: &ClaudeRunConfig) -> BaroResult<ClaudeJsonOutput> {
    let mut cmd = Command::new("claude");
    cmd.args([
        "--print",
        "--dangerously-skip-permissions",
        "--output-format",
        "json",
    ]);
    if let Some(ref m) = config.model {
        cmd.arg("--model").arg(m);
    }
    cmd.arg("-p").arg(&config.prompt);
    cmd.current_dir(&config.cwd);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let output = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn claude: {}", e))?
        .wait_with_output()
        .await
        .map_err(|e| format!("Claude process error: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Claude exited with code {}: {}",
            output.status.code().unwrap_or(-1),
            stderr
        )
        .into());
    }

    Ok(ClaudeJsonOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
    })
}
