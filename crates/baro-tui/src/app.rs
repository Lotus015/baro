#![allow(dead_code)]

use std::collections::HashMap;
use std::time::Instant;

use crate::events::{BaroEvent, DoneStats};

const MAX_LOG_LINES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Screen {
    Welcome,
    Planning,
    Review,
    Execute,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Planner {
    Claude,
    OpenAI,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlobalTab {
    Dashboard,
    Dag,
    Stats,
}

impl GlobalTab {
    pub fn next(self) -> Self {
        match self {
            Self::Dashboard => Self::Dag,
            Self::Dag => Self::Stats,
            Self::Stats => Self::Dashboard,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Dashboard => Self::Stats,
            Self::Dag => Self::Dashboard,
            Self::Stats => Self::Dag,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Dashboard => 0,
            Self::Dag => 1,
            Self::Stats => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StoryStatus {
    Pending,
    Running,
    Complete,
    Failed,
    Retrying(u32),
    Skipped,
}

#[derive(Debug, Clone)]
pub struct StoryState {
    pub id: String,
    pub title: String,
    pub depends_on: Vec<String>,
    pub status: StoryStatus,
    pub duration_secs: Option<u64>,
    pub error: Option<String>,
    pub files_created: u32,
    pub files_modified: u32,
}

#[derive(Debug, Clone)]
pub struct ActiveStory {
    pub id: String,
    pub title: String,
    pub logs: Vec<String>,
    pub start_time: Instant,
}

#[derive(Debug, Clone)]
pub struct ReviewStory {
    pub title: String,
    pub description: String,
    pub depends_on: Vec<String>,
}

pub struct App {
    // Screen state
    pub screen: Screen,
    pub planner: Planner,

    // Welcome screen
    pub goal_input: String,

    // Planning screen
    pub planning_start: Option<Instant>,

    // Review screen
    pub review_stories: Vec<ReviewStory>,
    pub review_scroll: usize,

    // Execute screen
    pub project: String,
    pub stories: Vec<StoryState>,
    pub dag_levels: Vec<Vec<String>>,
    pub active_stories: HashMap<String, ActiveStory>,
    pub completed: u32,
    pub total: u32,
    pub percentage: u32,
    pub current_level: usize,
    pub start_time: Instant,
    pub done: bool,
    pub final_stats: Option<DoneStats>,
    pub total_time_secs: u64,

    // UI state
    pub global_tab: GlobalTab,
    pub selected_log_index: usize,
    pub tick_count: u64,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Welcome,
            planner: Planner::Claude,

            goal_input: String::new(),

            planning_start: None,

            review_stories: Vec::new(),
            review_scroll: 0,

            project: String::new(),
            stories: Vec::new(),
            dag_levels: Vec::new(),
            active_stories: HashMap::new(),
            completed: 0,
            total: 0,
            percentage: 0,
            current_level: 0,
            start_time: Instant::now(),
            done: false,
            final_stats: None,
            total_time_secs: 0,
            global_tab: GlobalTab::Dashboard,
            selected_log_index: 0,
            tick_count: 0,
        }
    }

    pub fn tick(&mut self) {
        self.tick_count += 1;
    }

    // Screen transitions
    pub fn start_planning(&mut self) {
        self.screen = Screen::Planning;
        self.planning_start = Some(Instant::now());
    }

    pub fn show_review(&mut self, stories: Vec<ReviewStory>) {
        self.review_stories = stories;
        self.review_scroll = 0;
        self.screen = Screen::Review;
    }

    pub fn start_execution(&mut self) {
        self.screen = Screen::Execute;
        self.start_time = Instant::now();
    }

    pub fn planning_elapsed_secs(&self) -> u64 {
        self.planning_start
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }

    // Planner toggle
    pub fn toggle_planner(&mut self) {
        self.planner = match self.planner {
            Planner::Claude => Planner::OpenAI,
            Planner::OpenAI => Planner::Claude,
        };
    }

    // Execute screen tab navigation
    pub fn next_tab(&mut self) {
        self.global_tab = self.global_tab.next();
    }

    pub fn prev_tab(&mut self) {
        self.global_tab = self.global_tab.prev();
    }

    pub fn next_log(&mut self) {
        let count = self.active_stories.len();
        if count > 0 {
            self.selected_log_index = (self.selected_log_index + 1) % count;
        }
    }

    pub fn prev_log(&mut self) {
        let count = self.active_stories.len();
        if count > 0 {
            self.selected_log_index = if self.selected_log_index == 0 {
                count - 1
            } else {
                self.selected_log_index - 1
            };
        }
    }

    pub fn active_story_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.active_stories.keys().cloned().collect();
        ids.sort();
        ids
    }

    // Review screen navigation
    pub fn review_next(&mut self) {
        if !self.review_stories.is_empty() {
            self.review_scroll = (self.review_scroll + 1).min(self.review_stories.len() - 1);
        }
    }

    pub fn review_prev(&mut self) {
        self.review_scroll = self.review_scroll.saturating_sub(1);
    }

    pub fn handle_event(&mut self, event: BaroEvent) {
        match event {
            BaroEvent::Init { project, stories } => {
                self.project = project;
                self.total = stories.len() as u32;
                self.stories = stories
                    .into_iter()
                    .map(|s| StoryState {
                        id: s.id,
                        title: s.title,
                        depends_on: s.depends_on,
                        status: StoryStatus::Pending,
                        duration_secs: None,
                        error: None,
                        files_created: 0,
                        files_modified: 0,
                    })
                    .collect();
                self.start_time = Instant::now();
            }

            BaroEvent::Dag { levels } => {
                self.dag_levels = levels
                    .into_iter()
                    .map(|level| level.into_iter().map(|n| n.id).collect())
                    .collect();
            }

            BaroEvent::StoryStart { id, title } => {
                if let Some(story) = self.stories.iter_mut().find(|s| s.id == id) {
                    story.status = StoryStatus::Running;
                }
                self.active_stories.insert(
                    id.clone(),
                    ActiveStory {
                        id,
                        title,
                        logs: Vec::new(),
                        start_time: Instant::now(),
                    },
                );
                self.update_current_level();
            }

            BaroEvent::StoryLog { id, line } => {
                if let Some(active) = self.active_stories.get_mut(&id) {
                    active.logs.push(line);
                    if active.logs.len() > MAX_LOG_LINES {
                        active.logs.remove(0);
                    }
                }
            }

            BaroEvent::StoryComplete {
                id,
                duration_secs,
                files_created,
                files_modified,
            } => {
                if let Some(story) = self.stories.iter_mut().find(|s| s.id == id) {
                    story.status = StoryStatus::Complete;
                    story.duration_secs = Some(duration_secs);
                    story.files_created = files_created;
                    story.files_modified = files_modified;
                }
                self.active_stories.remove(&id);
                let count = self.active_stories.len();
                if count > 0 && self.selected_log_index >= count {
                    self.selected_log_index = count - 1;
                }
            }

            BaroEvent::StoryError {
                id,
                error,
                attempt,
                max_retries,
            } => {
                if let Some(story) = self.stories.iter_mut().find(|s| s.id == id) {
                    if attempt >= max_retries {
                        story.status = StoryStatus::Skipped;
                        story.error = Some(error);
                        self.active_stories.remove(&id);
                    } else {
                        story.status = StoryStatus::Failed;
                        story.error = Some(error);
                    }
                }
            }

            BaroEvent::StoryRetry { id, attempt } => {
                if let Some(story) = self.stories.iter_mut().find(|s| s.id == id) {
                    story.status = StoryStatus::Retrying(attempt);
                }
            }

            BaroEvent::Progress {
                completed,
                total,
                percentage,
            } => {
                self.completed = completed;
                self.total = total;
                self.percentage = percentage;
            }

            BaroEvent::Done {
                total_time_secs,
                stats,
            } => {
                self.done = true;
                self.total_time_secs = total_time_secs;
                self.final_stats = Some(stats);
            }
        }
    }

    fn update_current_level(&mut self) {
        for (i, level) in self.dag_levels.iter().enumerate() {
            let any_active = level.iter().any(|id| {
                self.stories.iter().any(|s| {
                    s.id == *id
                        && matches!(
                            s.status,
                            StoryStatus::Running | StoryStatus::Retrying(_)
                        )
                })
            });
            if any_active {
                self.current_level = i;
                return;
            }
        }
    }

    pub fn elapsed_secs(&self) -> u64 {
        if self.done {
            self.total_time_secs
        } else {
            self.start_time.elapsed().as_secs()
        }
    }
}
