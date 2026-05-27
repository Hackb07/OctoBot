use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static AGENT_NAME_COUNTER: AtomicUsize = AtomicUsize::new(0);
static SUB_AGENT_COUNTER: AtomicUsize = AtomicUsize::new(0);

const AGENT_NAMES: &[&str] = &[
    "atlas", "nova", "helix", "pulse", "vertex", "axiom", "nexus", "orbit", "zen", "flux",
    "cypher", "echo", "phantom", "solar", "vector",
];

const SUB_AGENT_NAMES: &[&str] = &[
    "bolt", "rivet", "stitch", "pin", "gear", "sprocket", "weld", "shim", "latch", "dowel",
    "grommet", "cam", "pivot", "clamp", "spindle",
];

pub(crate) fn next_agent_name() -> String {
    let idx = AGENT_NAME_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = AGENT_NAMES[idx % AGENT_NAMES.len()];
    let group = idx / AGENT_NAMES.len();
    if group == 0 {
        name.to_string()
    } else {
        format!("{name}-{}", group + 1)
    }
}

pub(crate) fn next_sub_agent_name() -> String {
    let idx = SUB_AGENT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = SUB_AGENT_NAMES[idx % SUB_AGENT_NAMES.len()];
    let group = idx / SUB_AGENT_NAMES.len();
    if group == 0 {
        name.to_string()
    } else {
        format!("{name}-{}", group + 1)
    }
}

pub(crate) fn now_ts() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    secs.to_string()
}

pub(crate) fn next_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{prefix}-{nanos}")
}
pub(crate) fn trim_preview(input: String) -> String {
    let max_lines = std::env::var("OCTOBOT_PREVIEW_LINES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(12);
    let max_chars = std::env::var("OCTOBOT_PREVIEW_CHARS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1200);
    input
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(max_chars)
        .collect()
}
