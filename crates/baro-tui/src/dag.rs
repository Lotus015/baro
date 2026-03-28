use std::collections::{HashMap, HashSet};

use crate::executor::PrdStory;
use crate::utils::BaroResult;

pub(crate) struct DagLevel {
    pub story_ids: Vec<String>,
}

/// Build DAG levels from ALL stories (for stats/PR body after completion).
pub(crate) fn build_dag_all(stories: &[PrdStory]) -> BaroResult<Vec<DagLevel>> {
    let story_map: HashMap<&str, &PrdStory> =
        stories.iter().map(|s| (s.id.as_str(), s)).collect();

    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for s in stories {
        let dep_count = s
            .depends_on
            .iter()
            .filter(|d| story_map.contains_key(d.as_str()))
            .count();
        in_degree.insert(s.id.as_str(), dep_count);
        for dep in &s.depends_on {
            if story_map.contains_key(dep.as_str()) {
                dependents.entry(dep.as_str()).or_default().push(s.id.as_str());
            }
        }
    }

    let mut levels: Vec<DagLevel> = Vec::new();
    let mut queue: Vec<&PrdStory> = stories
        .iter()
        .filter(|s| *in_degree.get(s.id.as_str()).unwrap_or(&0) == 0)
        .collect();

    while !queue.is_empty() {
        queue.sort_by_key(|s| s.priority);
        let ids: Vec<String> = queue.iter().map(|s| s.id.clone()).collect();
        levels.push(DagLevel { story_ids: ids });

        let mut next_queue: Vec<&PrdStory> = Vec::new();
        for s in &queue {
            if let Some(deps) = dependents.get(s.id.as_str()) {
                for dep_id in deps {
                    if let Some(deg) = in_degree.get_mut(dep_id) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            if let Some(story) = story_map.get(dep_id) {
                                next_queue.push(story);
                            }
                        }
                    }
                }
            }
        }
        queue = next_queue;
    }

    let total_in_levels: usize = levels.iter().map(|l| l.story_ids.len()).sum();
    if total_in_levels != stories.len() {
        let placed: HashSet<&str> = levels
            .iter()
            .flat_map(|l| l.story_ids.iter().map(|s| s.as_str()))
            .collect();
        let cycled: Vec<&str> = stories
            .iter()
            .filter(|s| !placed.contains(s.id.as_str()))
            .map(|s| s.id.as_str())
            .collect();
        return Err(format!("Dependency cycle detected: {}", cycled.join(", ")).into());
    }

    Ok(levels)
}

/// Build DAG levels from only incomplete stories (for execution).
pub(crate) fn build_dag(stories: &[PrdStory]) -> BaroResult<Vec<DagLevel>> {
    let incomplete: Vec<&PrdStory> = stories.iter().filter(|s| !s.passes).collect();
    let completed_ids: HashSet<&str> = stories
        .iter()
        .filter(|s| s.passes)
        .map(|s| s.id.as_str())
        .collect();
    let story_map: HashMap<&str, &PrdStory> =
        incomplete.iter().map(|s| (s.id.as_str(), *s)).collect();

    // Build in-degree and reverse edges
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for s in &incomplete {
        let active_deps: Vec<&str> = s
            .depends_on
            .iter()
            .map(|d| d.as_str())
            .filter(|d| story_map.contains_key(d) && !completed_ids.contains(d))
            .collect();
        in_degree.insert(s.id.as_str(), active_deps.len());
        for dep in active_deps {
            dependents.entry(dep).or_default().push(s.id.as_str());
        }
    }

    let mut levels: Vec<DagLevel> = Vec::new();
    let mut queue: Vec<&PrdStory> = incomplete
        .iter()
        .filter(|s| *in_degree.get(s.id.as_str()).unwrap_or(&0) == 0)
        .copied()
        .collect();

    while !queue.is_empty() {
        queue.sort_by_key(|s| s.priority);
        let ids: Vec<String> = queue.iter().map(|s| s.id.clone()).collect();
        levels.push(DagLevel { story_ids: ids });

        let mut next_queue: Vec<&PrdStory> = Vec::new();
        for s in &queue {
            if let Some(deps) = dependents.get(s.id.as_str()) {
                for dep_id in deps {
                    if let Some(deg) = in_degree.get_mut(dep_id) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            if let Some(story) = story_map.get(dep_id) {
                                next_queue.push(story);
                            }
                        }
                    }
                }
            }
        }
        queue = next_queue;
    }

    // Cycle detection
    let total_in_levels: usize = levels.iter().map(|l| l.story_ids.len()).sum();
    if total_in_levels != incomplete.len() {
        let placed: HashSet<&str> = levels
            .iter()
            .flat_map(|l| l.story_ids.iter().map(|s| s.as_str()))
            .collect();
        let cycled: Vec<&str> = incomplete
            .iter()
            .filter(|s| !placed.contains(s.id.as_str()))
            .map(|s| s.id.as_str())
            .collect();
        return Err(format!("Dependency cycle detected: {}", cycled.join(", ")).into());
    }

    Ok(levels)
}
