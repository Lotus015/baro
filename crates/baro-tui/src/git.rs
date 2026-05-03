//! Pre-orchestration git helpers used by main.rs.
//!
//! The TS Mozaik orchestrator (`packages/baro-orchestrator/src/git.ts`)
//! owns all per-story git activity (push with retry, pull --rebase,
//! file-stat collection). This module survives only for the
//! welcome-screen → planning flow which still needs to set up the
//! `baro/<name>` branch in Rust before handing control off to the
//! orchestrator.

use std::path::Path;

use tokio::process::Command;

use crate::utils::BaroResult;

/// Return the name of the currently checked-out branch in `cwd`.
pub(crate) async fn get_current_branch(cwd: &Path) -> BaroResult<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| format!("Failed to get branch: {}", e))?;

    let branch_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch_name.is_empty() {
        return Err("Could not determine current branch".into());
    }
    Ok(branch_name)
}

/// Create a new branch (or checkout if it already exists) and best-effort
/// push it with upstream tracking. Push failures are non-fatal.
pub async fn create_or_checkout_branch(cwd: &Path, branch_name: &str) -> BaroResult<()> {
    let create = Command::new("git")
        .args(["checkout", "-b", branch_name])
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| format!("Failed to run git checkout -b: {}", e))?;

    if !create.status.success() {
        let checkout = Command::new("git")
            .args(["checkout", branch_name])
            .current_dir(cwd)
            .output()
            .await
            .map_err(|e| format!("Failed to run git checkout: {}", e))?;

        if !checkout.status.success() {
            let stderr = String::from_utf8_lossy(&checkout.stderr).trim().to_string();
            return Err(
                format!("Failed to checkout branch '{}': {}", branch_name, stderr).into(),
            );
        }
    }

    let push = Command::new("git")
        .args(["push", "-u", "origin", branch_name])
        .current_dir(cwd)
        .output()
        .await;

    match push {
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            eprintln!(
                "[git] push -u origin {} failed (best-effort): {}",
                branch_name, stderr
            );
        }
        Err(e) => {
            eprintln!(
                "[git] push -u origin {} failed (best-effort): {}",
                branch_name, e
            );
        }
        _ => {}
    }

    Ok(())
}
