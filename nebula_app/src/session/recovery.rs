//! Restore workspace identities without starting saved Codex or Claude processes.
use std::collections::HashSet;

use super::{LaunchSession, LayoutSession, Session};

pub(crate) struct Prepared {
    pub session: Session,
    pub busy: usize,
    pub unknown: usize,
    pub deferred: Vec<super::AgentSession>,
}

pub(crate) fn prepare(
    mut session: Session,
    resume_ai: bool,
    default_launch: LaunchSession,
) -> Prepared {
    let mut busy = 0;
    let mut unknown = 0;
    let mut deferred = Vec::new();
    if resume_ai {
        let mut seen = HashSet::new();
        let old_active = session.active_tab;
        let mut new_active = None;
        let mut kept_tabs = 0;
        session.tabs = session
            .tabs
            .into_iter()
            .enumerate()
            .filter_map(|(index, mut tab)| {
                let Some(layout) = tab.layout.take() else {
                    if index == old_active {
                        new_active = Some(kept_tabs);
                    }
                    kept_tabs += 1;
                    return Some(tab);
                };
                let first_launch = tab.launch.clone().unwrap_or_else(|| default_launch.clone());
                let mut old_leaf = 0;
                let mut new_leaf = 0;
                let mut active = 0;
                let layout = prune(layout, &mut |leaf| {
                    let LayoutSession::Pane { agent, launch, .. } = leaf else { return false };
                    let original = old_leaf;
                    old_leaf += 1;
                    let launch = launch.get_or_insert_with(|| {
                        if original == 0 { first_launch.clone() } else { default_launch.clone() }
                    });
                    if matches!(launch, LaunchSession::Default) {
                        *launch = default_launch.clone();
                    }
                    if let Some(agent) = agent {
                        agent.normalize_identity();
                        let remote = match launch {
                            LaunchSession::Ssh { .. } => true,
                            LaunchSession::Shell { program, .. }
                            | LaunchSession::Profile { command: program, .. } => {
                                crate::runtime_exec::is_wsl_program(program)
                            },
                            LaunchSession::Default => false,
                        };
                        if remote
                            || agent
                                .session_file
                                .as_deref()
                                .is_some_and(|path| !std::path::Path::new(path).is_absolute())
                        {
                            unknown += 1;
                            return false;
                        }
                        if matches!(agent.source.as_str(), "codex" | "claude") {
                            if agent.session_id.is_none() {
                                unknown += 1;
                            }
                            deferred.push(agent.clone());
                        } else if agent.session_id.is_none() {
                            unknown += 1;
                            return false;
                        } else {
                            let key = (agent.source.clone(), agent.session_id.clone());
                            if seen.contains(&key) {
                                busy += 1;
                                return false;
                            }
                            match crate::platform::ai_session_identity::recovery::available(agent) {
                                Ok(true) => {
                                    seen.insert(key);
                                },
                                Ok(false) => {
                                    busy += 1;
                                    return false;
                                },
                                Err(()) => {
                                    unknown += 1;
                                    return false;
                                },
                            }
                        }
                    }
                    if original == tab.active_pane {
                        active = new_leaf;
                    }
                    new_leaf += 1;
                    true
                })?;
                tab.active_pane = active;
                tab.launch = layout.leaves().first().and_then(|leaf| match leaf {
                    LayoutSession::Pane { launch, .. } => launch.clone(),
                    _ => None,
                });
                tab.layout = Some(layout);
                if index == old_active {
                    new_active = Some(kept_tabs);
                }
                kept_tabs += 1;
                Some(tab)
            })
            .collect();
        session.active_tab = new_active.unwrap_or(0).min(session.tabs.len().saturating_sub(1));
    }
    Prepared { session, busy, unknown, deferred }
}

fn prune(
    mut layout: LayoutSession,
    keep: &mut impl FnMut(&mut LayoutSession) -> bool,
) -> Option<LayoutSession> {
    match layout {
        LayoutSession::Pane { .. } => keep(&mut layout).then_some(layout),
        LayoutSession::Split { axis, ratio_permille, first, second } => {
            match (prune(*first, keep), prune(*second, keep)) {
                (Some(first), Some(second)) => Some(LayoutSession::Split {
                    axis,
                    ratio_permille,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
                (Some(only), None) | (None, Some(only)) => Some(only),
                (None, None) => None,
            }
        },
    }
}
