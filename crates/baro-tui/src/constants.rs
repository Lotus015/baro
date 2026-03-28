/// Maximum number of review cycles per level before continuing.
pub const MAX_REVIEW_CYCLES: u32 = 2;

/// Maximum number of log lines retained per story in the TUI.
pub const MAX_LOG_LINES: usize = 200;

/// Maximum directory tree depth when scanning project structure.
pub const DIRECTORY_TREE_DEPTH: u32 = 3;

/// Maximum number of git push attempts before giving up.
pub const GIT_PUSH_MAX_ATTEMPTS: u32 = 3;

/// Maximum number of characters of build output sent to the review/verification model.
pub const BUILD_OUTPUT_TRUNCATION: usize = 5000;
