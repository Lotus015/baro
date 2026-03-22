#![allow(dead_code)]

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct StoryInfo {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DagNode {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DoneStats {
    pub stories_completed: u32,
    pub stories_skipped: u32,
    pub total_commits: u32,
    pub files_created: u32,
    pub files_modified: u32,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum BaroEvent {
    #[serde(rename = "init")]
    Init {
        project: String,
        stories: Vec<StoryInfo>,
    },

    #[serde(rename = "dag")]
    Dag {
        levels: Vec<Vec<DagNode>>,
    },

    #[serde(rename = "story_start")]
    StoryStart {
        id: String,
        title: String,
    },

    #[serde(rename = "story_log")]
    StoryLog {
        id: String,
        line: String,
    },

    #[serde(rename = "story_complete")]
    StoryComplete {
        id: String,
        duration_secs: u64,
        files_created: u32,
        files_modified: u32,
    },

    #[serde(rename = "story_error")]
    StoryError {
        id: String,
        error: String,
        attempt: u32,
        max_retries: u32,
    },

    #[serde(rename = "story_retry")]
    StoryRetry {
        id: String,
        attempt: u32,
    },

    #[serde(rename = "progress")]
    Progress {
        completed: u32,
        total: u32,
        percentage: u32,
    },

    #[serde(rename = "done")]
    Done {
        total_time_secs: u64,
        stats: DoneStats,
    },
}
