//! Workload-specific capacity calibrated against official percentage movement.

use super::codex::AuthGuard;
use super::{Issue, QuotaWindow};
use chrono::Utc;
use native::{Model, Tokens};
use serde::{Deserialize, Serialize};
use std::io::Read as _;

mod native;

pub(crate) struct Estimate {
    pub model: String,
    pub effort: String,
    pub tier: String,
    pub window: String,
    pub seconds: Option<u64>,
    pub total: (f64, f64),
    pub remaining: (f64, f64),
    pub requests: u32,
    pub points: f64,
    pub cached_ratio: f64,
}

#[derive(Default)]
pub(crate) struct Report {
    pub estimates: Vec<Estimate>,
    pub progress: Vec<Progress>,
    pub blocked: Option<SamplingIssue>,
}

pub(crate) struct Progress {
    pub model: String,
    pub window: String,
    pub seconds: Option<u64>,
    pub requests: u32,
    pub points: f64,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub(crate) enum SamplingIssue {
    Source,
    Incomplete,
    Model,
    Mixed,
    Interrupted,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct WindowKey {
    id: String,
    seconds: Option<u64>,
    reset: i64,
    plan: String,
}

#[derive(Serialize, Deserialize)]
struct Window {
    key: WindowKey,
    used: f64,
}

#[derive(Serialize, Deserialize)]
struct Observation {
    at: i64,
    windows: Vec<Window>,
    auth_revision: Option<(u64, u32, u64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    issue: Option<SamplingIssue>,
}

#[derive(Serialize, Deserialize)]
struct Segment {
    key: WindowKey,
    model: Model,
    tokens: Tokens,
    points: f64,
    requests: u32,
    intervals: u32,
    start: i64,
    end: i64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Store {
    previous: Option<Observation>,
    segments: Vec<Segment>,
}

fn path(key: &str) -> std::path::PathBuf {
    crate::platform::dirs::data_dir().join(format!("codex-capacity-{key}.json"))
}

fn load(path: &std::path::Path) -> Result<Store, Issue> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(_) => return Err(Issue::Storage),
    };
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|_| Issue::Storage)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(Issue::Storage);
    }
    let store: Store = serde_json::from_slice(&bytes).map_err(|_| Issue::Storage)?;
    if store.segments.len() > 256
        || store.segments.iter().any(|segment| {
            !segment.points.is_finite()
                || segment.points < 0.0
                || segment.points > 100.0
                || segment.tokens.cached > segment.tokens.input
                || segment.end < segment.start
        })
    {
        return Err(Issue::Storage);
    }
    Ok(store)
}

fn save(path: &std::path::Path, store: &Store) -> Result<(), Issue> {
    crate::atomic_file::write_private(path, &serde_json::to_vec(store).map_err(|_| Issue::Storage)?)
        .map_err(|_| Issue::Storage)
}

pub(super) fn break_chain(guard: &AuthGuard) {
    let path = path(&guard.key());
    let Ok(Some(_lock)) = crate::atomic_file::try_lifetime_lock(&path) else { return };
    let Ok(mut store) = load(&path) else { return };
    if guard.check().is_ok() && store.previous.take().is_some() {
        let _ = save(&path, &store);
    }
}

fn mix_matches(left: Tokens, right: Tokens) -> bool {
    let ratio = |tokens: Tokens| {
        (
            tokens.cached as f64 / tokens.input.max(1) as f64,
            tokens.output as f64 / tokens.total().max(1) as f64,
        )
    };
    let (lc, lo) = ratio(left);
    let (rc, ro) = ratio(right);
    (lc - rc).abs() <= 0.15 && (lo - ro).abs() <= 0.15
}

pub(super) fn observe(
    guard: &AuthGuard,
    plan: &str,
    windows: &[QuotaWindow],
) -> Result<Report, Issue> {
    guard.check()?;
    if plan.len() > 80 {
        return Err(Issue::InvalidQuota);
    }
    let path = path(&guard.key());
    let _lock = crate::atomic_file::try_lifetime_lock(&path)
        .map_err(|_| Issue::Storage)?
        .ok_or(Issue::OperationBusy)?;
    let mut store = load(&path)?;
    let now = Utc::now();
    let auth_revision = super::auth_revision().and_then(|(modified, length)| {
        let time = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some((time.as_secs(), time.subsec_nanos(), length))
    });
    let mut current = Observation {
        at: now.timestamp_millis(),
        auth_revision,
        issue: None,
        windows: windows
            .iter()
            .filter_map(|window| {
                Some(Window {
                    key: WindowKey {
                        id: window.id.clone(),
                        seconds: window.seconds,
                        reset: window.resets_at?.timestamp(),
                        plan: plan.to_owned(),
                    },
                    used: 100.0 - window.remaining_percent,
                })
            })
            .collect(),
    };
    if let Some(previous) = &store.previous {
        store.segments.retain(|segment| {
            !current.windows.iter().any(|window| {
                window.key == segment.key
                    && previous
                        .windows
                        .iter()
                        .any(|old| old.key == window.key && old.used > window.used)
            })
        });
    }
    if store
        .previous
        .as_ref()
        .is_some_and(|previous| current.at >= previous.at && current.at - previous.at < 30_000)
    {
        current.issue = store
            .previous
            .as_ref()
            .filter(|previous| {
                previous.auth_revision == current.auth_revision
                    && current
                        .windows
                        .iter()
                        .all(|window| previous.windows.iter().any(|old| old.key == window.key))
            })
            .and_then(|previous| previous.issue);
        return Ok(report(&store, &current));
    }
    if let Some(previous) = &store.previous
        && (30_000..=120_000).contains(&(current.at - previous.at))
        && previous.auth_revision == current.auth_revision
        && let Some(since) = chrono::DateTime::<Utc>::from_timestamp_millis(previous.at)
    {
        let interval = native::read(since, now)?;
        current.issue = if !interval.complete {
            Some(interval.issue.unwrap_or(SamplingIssue::Incomplete))
        } else if interval
            .model
            .as_ref()
            .is_some_and(|model| model.effort == "unknown" || model.tier == "unknown")
        {
            Some(SamplingIssue::Model)
        } else {
            None
        };
        if interval.complete && !plan.is_empty() {
            for window in &current.windows {
                if window.key.reset <= now.timestamp() {
                    continue;
                }
                let Some(old) = previous.windows.iter().find(|old| old.key == window.key) else {
                    continue;
                };
                let points = window.used - old.used;
                if !(0.0..=100.0).contains(&points) {
                    continue;
                }
                let active = store
                    .segments
                    .iter()
                    .rposition(|segment| segment.key == window.key && segment.end == previous.at);
                if interval.tokens.total() == 0 && points == 0.0 {
                    if let Some(index) = active {
                        store.segments[index].end = current.at;
                    }
                    continue;
                }
                let Some(model) = interval
                    .model
                    .as_ref()
                    .filter(|model| model.effort != "unknown" && model.tier != "unknown")
                else {
                    continue;
                };
                if interval.tokens.total() == 0 {
                    continue;
                }
                if let Some(index) = active.filter(|index| {
                    let segment = &store.segments[*index];
                    segment.model == *model && mix_matches(segment.tokens, interval.tokens)
                }) {
                    let segment = &mut store.segments[index];
                    segment.tokens = segment.tokens.plus(interval.tokens);
                    segment.points += points;
                    segment.requests = segment.requests.saturating_add(interval.requests);
                    segment.intervals = segment.intervals.saturating_add(1);
                    segment.end = current.at;
                } else {
                    store.segments.push(Segment {
                        key: window.key.clone(),
                        model: model.clone(),
                        tokens: interval.tokens,
                        points,
                        requests: interval.requests,
                        intervals: 1,
                        start: previous.at,
                        end: current.at,
                    });
                }
            }
        }
    } else if store.previous.is_some() {
        current.issue = Some(SamplingIssue::Interrupted);
    }
    store.segments.retain(|segment| current.windows.iter().any(|window| window.key == segment.key));
    if store.segments.len() > 256 {
        store.segments.drain(..store.segments.len() - 256);
    }
    let result = report(&store, &current);
    store.previous = Some(current);
    guard.check()?;
    save(&path, &store)?;
    Ok(result)
}

fn report(store: &Store, current: &Observation) -> Report {
    let mut estimates = Vec::new();
    let mut progress = Vec::new();
    for window in &current.windows {
        if window.key.reset * 1000 <= current.at {
            continue;
        }
        let mut models: Vec<&Model> = Vec::new();
        for segment in store.segments.iter().rev().filter(|segment| segment.key == window.key) {
            if !models.contains(&&segment.model) {
                models.push(&segment.model);
            }
        }
        for model in models.into_iter().take(4) {
            if let Some(segment) = store
                .segments
                .iter()
                .rev()
                .find(|segment| segment.key == window.key && segment.model == *model)
            {
                progress.push(Progress {
                    model: format!("{} · {} · {}", model.name, model.effort, model.tier),
                    window: window.key.id.clone(),
                    seconds: window.key.seconds,
                    requests: segment.requests,
                    points: segment.points,
                });
            }
            let segments: Vec<_> = store
                .segments
                .iter()
                .rev()
                .filter(|segment| {
                    segment.key == window.key
                        && segment.model == *model
                        && segment.points >= 3.0
                        && segment.intervals >= 2
                        && segment.end - segment.start >= 60_000
                })
                .collect();
            let Some(latest) = segments.first() else { continue };
            let compatible: Vec<_> = segments
                .iter()
                .copied()
                .filter(|segment| mix_matches(latest.tokens, segment.tokens))
                .take(8)
                .collect();
            let points: f64 = compatible.iter().map(|segment| segment.points).sum();
            let tokens = compatible
                .iter()
                .fold(Tokens::default(), |total, segment| total.plus(segment.tokens));
            let requests = compatible
                .iter()
                .fold(0_u32, |total, segment| total.saturating_add(segment.requests));
            // Conservatively allow two percentage points of endpoint rounding
            // for each independent segment; ratios are formed from pooled sums.
            let uncertainty = compatible.len() as f64 * 2.0;
            if points < 6.0 || points <= uncertainty || requests < 5 {
                continue;
            }
            let low = tokens.total() as f64 * 100.0 / (points + uncertainty);
            let high = tokens.total() as f64 * 100.0 / (points - uncertainty);
            if !high.is_finite() || high / low > 3.0 {
                continue;
            }
            let fraction = ((100.0 - window.used) / 100.0).clamp(0.0, 1.0);
            estimates.push(Estimate {
                model: model.name.clone(),
                effort: model.effort.clone(),
                tier: model.tier.clone(),
                window: window.key.id.clone(),
                seconds: window.key.seconds,
                total: (low, high),
                remaining: (low * fraction, high * fraction),
                requests,
                points,
                cached_ratio: tokens.cached as f64 / tokens.input.max(1) as f64,
            });
        }
    }
    Report { estimates, progress, blocked: current.issue }
}
