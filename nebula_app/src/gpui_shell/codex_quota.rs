//! One official quota request/snapshot for every window and Settings surface.
//! Reset-credit mutations remain owned by Settings, outside this read-only model.

use crate::ai_usage::{self, AuthRevision, CodexReport, Issue};
use gpui::{App, AppContext as _, Context, Entity, Global, Task};
use std::time::Instant;

pub(super) const INTERVAL: u64 = 60;
const BACKOFF: u64 = 300;

#[derive(Default)]
pub(super) struct Snapshot {
    pub report: Option<Result<CodexReport, Issue>>,
    pub checked_at: Option<chrono::DateTime<chrono::Local>>,
    pub failure: Option<Issue>,
    pub task: Option<Task<()>>,
    pub revision: Option<Option<AuthRevision>>,
    auth_observed: Option<Instant>,
    attempt: Option<Instant>,
    generation: u64,
}

struct Shared(Entity<Snapshot>);
impl Global for Shared {}

pub(super) fn shared(cx: &mut App) -> Entity<Snapshot> {
    if let Some(shared) = cx.try_global::<Shared>() {
        return shared.0.clone();
    }
    let entity = cx.new(|_| Snapshot::default());
    cx.set_global(Shared(entity.clone()));
    entity
}

impl Snapshot {
    pub fn invalidate(&mut self, cx: &mut Context<Self>) {
        self.generation = self.generation.wrapping_add(1);
        self.task = None;
        self.report = None;
        self.checked_at = None;
        self.failure = None;
        self.attempt = None;
        cx.notify();
    }

    pub fn observe_auth(
        &mut self,
        revision: Option<AuthRevision>,
        observed: Instant,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.auth_observed.is_some_and(|last| last > observed) {
            return self.revision == Some(revision);
        }
        self.auth_observed = Some(observed);
        let changed = self.revision.is_some_and(|previous| previous != revision);
        self.revision = Some(revision);
        if changed {
            self.invalidate(cx);
            self.refresh(true, cx);
        }
        true
    }

    pub fn refresh(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        let throttled = matches!(self.report, Some(Err(Issue::Http(429))))
            || matches!(self.failure, Some(Issue::Http(429)));
        let interval = if throttled { BACKOFF } else { INTERVAL };
        if (!force || throttled)
            && self.attempt.is_some_and(|last| {
                cx.background_executor().now().saturating_duration_since(last).as_secs() < interval
            })
        {
            return;
        }
        self.attempt = Some(cx.background_executor().now());
        let generation = self.generation;
        let worker = cx.background_executor().spawn(async {
            let start = ai_usage::auth_revision();
            let result = ai_usage::read_codex();
            let observed = Instant::now();
            (start, ai_usage::auth_revision(), observed, result)
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let (start, end, observed, result) = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.task = None;
                let newer_account = this.auth_observed.is_some_and(|last| last > observed)
                    && this.revision != Some(end);
                if start != end || newer_account {
                    this.invalidate(cx);
                    this.observe_auth(end, observed, cx);
                    this.refresh(true, cx);
                    return;
                }
                if this.revision.is_some_and(|previous| previous != end) {
                    this.report = None;
                    this.checked_at = None;
                    this.failure = None;
                }
                this.revision = Some(end);
                this.auth_observed =
                    Some(this.auth_observed.map_or(observed, |last| last.max(observed)));
                match result {
                    Ok(report) => {
                        this.report = Some(Ok(report));
                        this.checked_at = Some(chrono::Local::now());
                        this.failure = None;
                    },
                    Err(issue) => {
                        let authentication = matches!(
                            issue,
                            Issue::MissingAuth
                                | Issue::ApiKeyAuth
                                | Issue::InvalidAuth
                                | Issue::AuthChanged
                                | Issue::Authentication
                        );
                        if !authentication && matches!(this.report, Some(Ok(_))) {
                            this.failure = Some(issue);
                        } else {
                            this.report = Some(Err(issue));
                            this.checked_at = None;
                            this.failure = None;
                        }
                    },
                }
                cx.notify();
            });
        }));
        // Retain the last result while refreshing; its timestamp still describes that result.
        cx.notify();
    }
}
