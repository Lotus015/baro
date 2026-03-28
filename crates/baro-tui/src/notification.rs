use std::io::{self, Write};

/// Send a completion notification: terminal bell + OS-specific notification.
pub fn notify_completion() {
    // Terminal bell works from inside alternate screen
    print!("\x07");
    let _ = io::stdout().flush();

    // OS-specific notification
    match std::env::consts::OS {
        "macos" => {
            notify_macos_dock();
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

/// Clear the macOS dock badge label. No-op on other platforms.
pub fn clear_badge() {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;

        let mtm = unsafe { MainThreadMarker::new_unchecked() };
        let app = NSApplication::sharedApplication(mtm);
        let dock_tile = app.dockTile();
        dock_tile.setBadgeLabel(None);
    }
}

/// Bounce the dock icon and set a badge label on macOS using native AppKit APIs.
#[cfg(target_os = "macos")]
fn notify_macos_dock() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSRequestUserAttentionType};
    use objc2_foundation::NSString;

    // We must be on the main thread to interact with NSApplication.
    // In a TUI context the main thread is available but not running an NSRunLoop,
    // so we use an unchecked marker.
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    let app = NSApplication::sharedApplication(mtm);
    app.requestUserAttention(NSRequestUserAttentionType::CriticalRequest);
    let dock_tile = app.dockTile();
    let label = NSString::from_str("!");
    dock_tile.setBadgeLabel(Some(&label));
}

/// No-op on non-macOS platforms.
#[cfg(not(target_os = "macos"))]
fn notify_macos_dock() {}
