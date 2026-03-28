use std::io::{self, Write};

/// Send a completion notification: terminal bell + OS-specific notification.
pub fn notify_completion() {
    // Terminal bell
    print!("\x07");
    // OSC 9 notification (supported by Ghostty, iTerm2, Windows Terminal)
    print!("\x1b]9;All stories complete\x1b\\");
    let _ = io::stdout().flush();

    // OS-specific notification
    match std::env::consts::OS {
        "macos" => {
            // Banner notification
            let _ = std::process::Command::new("osascript")
                .args(["-e", "display notification \"All stories complete\" with title \"baro\""])
                .spawn();
            // Bounce dock icon
            let _ = std::process::Command::new("osascript")
                .args(["-e", concat!(
                    "tell application \"System Events\"\n",
                    "  set frontApp to name of first application process whose frontmost is true\n",
                    "end tell\n",
                    "tell application frontApp to activate"
                )])
                .spawn();
        }
        "linux" => {
            let _ = std::process::Command::new("notify-send")
                .args(["baro", "All stories complete"])
                .spawn();
        }
        "windows" => {
            let _ = std::process::Command::new("powershell")
                .args(["-Command", "[console]::beep(1000,500)"])
                .spawn();
        }
        _ => {}
    }
}

/// Clear the dock badge. Currently a no-op — badge clearing is handled
/// by the terminal itself when the user focuses the window.
pub fn clear_badge() {}
