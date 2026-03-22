#![allow(dead_code)]

use ratatui::style::Color;

// ─── ANSI color theme for maximum terminal compatibility ──────────────────

// Primary palette
pub const ACCENT: Color = Color::LightBlue;
pub const ACCENT_BRIGHT: Color = Color::LightCyan;
pub const ACCENT_DIM: Color = Color::Blue;

// Semantic colors
pub const SUCCESS: Color = Color::Green;
pub const SUCCESS_DIM: Color = Color::DarkGray;
pub const WARNING: Color = Color::Yellow;
pub const WARNING_DIM: Color = Color::DarkGray;
pub const ERROR: Color = Color::Red;
pub const ERROR_DIM: Color = Color::DarkGray;

// Text
pub const TEXT: Color = Color::White;
pub const TEXT_DIM: Color = Color::Gray;
pub const MUTED: Color = Color::DarkGray;

// Structure
pub const BORDER: Color = Color::Gray;
pub const BORDER_ACTIVE: Color = Color::LightCyan;
pub const SURFACE: Color = Color::Black;

// Logo / brand
pub const LOGO_1: Color = Color::LightBlue;
pub const LOGO_2: Color = Color::LightMagenta;
pub const LOGO_3: Color = Color::Magenta;
pub const LOGO_GLOW: Color = Color::Blue;

// Progress bar
pub const GAUGE_FG: Color = Color::Green;
pub const GAUGE_BG: Color = Color::DarkGray;

// Tab colors
pub const TAB_ACTIVE: Color = Color::LightCyan;
pub const TAB_INACTIVE: Color = Color::DarkGray;

// Status-specific
pub const RUNNING: Color = Color::Yellow;
pub const PENDING: Color = Color::DarkGray;
pub const RETRY: Color = Color::LightYellow;
