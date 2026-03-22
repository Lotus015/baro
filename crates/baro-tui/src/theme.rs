#![allow(dead_code)]

use ratatui::style::Color;

// ─── Modern dark theme with vibrant accents ──────────────────

// Primary palette - electric blue/purple gradient feel
pub const ACCENT: Color = Color::Rgb(130, 170, 255);      // soft blue
pub const ACCENT_BRIGHT: Color = Color::Rgb(100, 100, 255); // electric indigo
pub const ACCENT_DIM: Color = Color::Rgb(80, 100, 180);    // muted blue

// Semantic colors
pub const SUCCESS: Color = Color::Rgb(80, 250, 123);       // neon green
pub const SUCCESS_DIM: Color = Color::Rgb(50, 160, 80);    // muted green
pub const WARNING: Color = Color::Rgb(255, 184, 108);      // warm amber
pub const WARNING_DIM: Color = Color::Rgb(180, 130, 70);   // muted amber
pub const ERROR: Color = Color::Rgb(255, 85, 85);          // coral red
pub const ERROR_DIM: Color = Color::Rgb(180, 60, 60);      // muted red

// Text
pub const TEXT: Color = Color::Rgb(230, 230, 240);         // soft white
pub const TEXT_DIM: Color = Color::Rgb(160, 160, 180);     // secondary text
pub const MUTED: Color = Color::Rgb(100, 100, 120);        // subtle gray

// Structure
pub const BORDER: Color = Color::Rgb(60, 60, 80);          // subtle border
pub const BORDER_ACTIVE: Color = Color::Rgb(100, 100, 255); // active panel border
pub const SURFACE: Color = Color::Rgb(30, 30, 45);         // panel background hint

// Logo / brand
pub const LOGO_1: Color = Color::Rgb(100, 100, 255);       // gradient start: indigo
pub const LOGO_2: Color = Color::Rgb(130, 80, 255);        // gradient mid: purple
pub const LOGO_3: Color = Color::Rgb(170, 60, 255);        // gradient end: violet
pub const LOGO_GLOW: Color = Color::Rgb(80, 80, 200);      // dim glow version

// Progress bar
pub const GAUGE_FG: Color = Color::Rgb(100, 100, 255);     // indigo fill
pub const GAUGE_BG: Color = Color::Rgb(40, 40, 60);        // dark track

// Tab colors
pub const TAB_ACTIVE: Color = Color::Rgb(100, 100, 255);
pub const TAB_INACTIVE: Color = Color::Rgb(80, 80, 100);

// Status-specific
pub const RUNNING: Color = Color::Rgb(255, 184, 108);      // amber for running
pub const PENDING: Color = Color::Rgb(80, 80, 100);        // dim for pending
pub const RETRY: Color = Color::Rgb(255, 200, 50);         // bright yellow for retry
