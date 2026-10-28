//! One operation per local CLI, retained across dialog and workspace changes.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::ai_agents::AgentKind;
use crate::ai_hook::integrations::updates::{self, Check, Error};
use crate::gpui_shell::prelude::*;
use crate::i18n::Message;
use gpui::{
    App, AppContext as _, Context, Entity, Global, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window, div, px,
};

#[derive(Default)]
struct CliUpdates(HashMap<AgentKind, Entity<CliUpdate>>);
impl Global for CliUpdates {}

pub(super) fn open(kind: AgentKind, window: &mut Window, cx: &mut App) {
    if !cx.has_global::<CliUpdates>() {
        cx.set_global(CliUpdates::default());
    }
    let panel = if let Some(panel) = cx.global::<CliUpdates>().0.get(&kind).cloned() {
        panel
    } else {
        let panel = cx.new(|_| CliUpdate {
            kind,
            checked: None,
            error: None,
            task: None,
            upgrading: false,
            upgraded: false,
        });
        cx.global_mut::<CliUpdates>().0.insert(kind, panel.clone());
        panel
    };
    panel.update(cx, |panel, cx| {
        if panel.task.is_none() {
            panel.refresh(cx);
        }
    });
    let language = crate::gpui_shell::config::ui_language(cx);
    window.open_dialog(cx, move |dialog, window, _| {
        dialog
            .title(language.format(Message::CliUpdateTitle, &[("agent", kind.label())]))
            .w(px((f32::from(window.viewport_size().width) - 48.0).clamp(240.0, 560.0)))
            .child(panel.clone())
    });
}

struct CliUpdate {
    kind: AgentKind,
    checked: Option<Check>,
    error: Option<Error>,
    task: Option<gpui::Task<()>>,
    upgrading: bool,
    upgraded: bool,
}

impl CliUpdate {
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        self.checked = None;
        self.error = None;
        self.upgraded = false;
        let kind = self.kind;
        let worker = cx.background_executor().spawn(async move { updates::check(kind) });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                this.task = None;
                match result {
                    Ok(checked) => this.checked = Some(checked),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn upgrade(&mut self, cx: &mut Context<Self>) {
        if self.task.is_some() || self.error.is_some() {
            return;
        }
        let Some(checked) = self
            .checked
            .as_ref()
            .filter(|check| check.can_upgrade() && check.comparison() == Ordering::Less)
            .cloned()
        else {
            return;
        };
        self.upgrading = true;
        let worker = cx.background_executor().spawn(async move { updates::upgrade(&checked) });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = this.update(cx, |this, cx| {
                this.task = None;
                this.upgrading = false;
                match result {
                    Ok(checked) => {
                        this.checked = Some(checked);
                        this.upgraded = true;
                    },
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn error_text(&self, language: crate::display::UiLanguage) -> Option<String> {
        Some(match self.error.as_ref()? {
            Error::NotFound => language.text(Message::CliUpdateNotFound).to_owned(),
            Error::Version(error) => {
                language.format(Message::CliUpdateVersionFailed, &[("error", error)])
            },
            Error::Registry(error) => {
                language.format(Message::CliUpdateCheckFailed, &[("error", error)])
            },
            Error::Unsupported => language.text(Message::CliUpdateManual).to_owned(),
            Error::Changed => language.text(Message::CliUpdateChanged).to_owned(),
            Error::Upgrade(error) => language.format(Message::CliUpdateFailed, &[("error", error)]),
            Error::Verify { expected, actual } => language.format(
                Message::CliUpdateVerifyFailed,
                &[("expected", expected), ("actual", actual)],
            ),
        })
    }
}

impl Render for CliUpdate {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let busy = self.task.is_some();
        let mut body = v_flex().w_full().gap_3().text_sm();
        if let Some(checked) = &self.checked {
            for (label, version) in [
                (Message::CliUpdateCurrent, checked.current()),
                (Message::CliUpdateStable, checked.stable()),
            ] {
                body = body.child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .child(language.text(label))
                        .child(SharedString::from(version.to_owned())),
                );
            }
            body = body.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(checked.executable.to_string_lossy().into_owned())),
            );
            let status = if self.upgraded {
                Message::CliUpdateSuccess
            } else {
                match checked.comparison() {
                    Ordering::Equal => Message::CliUpdateLatest,
                    Ordering::Greater => Message::CliUpdateNewer,
                    Ordering::Less if !checked.can_upgrade() => Message::CliUpdateManual,
                    Ordering::Less => Message::CliUpdateConfirm,
                }
            };
            body = body.child(language.text(status));
        }
        if busy {
            body = body.child(language.text(if self.upgrading {
                Message::CliUpdateUpgrading
            } else {
                Message::CliUpdateChecking
            }));
        }
        if let Some(error) = self.error_text(language) {
            body = body.child(div().text_color(cx.theme().danger).child(error));
        }
        let mut actions = h_flex().w_full().justify_end().gap_2().child(
            Button::new("cli-update-check")
                .small()
                .label(language.text(Message::CliUpdateCheck))
                .disabled(busy)
                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
        );
        if self
            .checked
            .as_ref()
            .is_some_and(|check| check.can_upgrade() && check.comparison() == Ordering::Less)
        {
            actions = actions.child(
                Button::new("cli-update-upgrade")
                    .primary()
                    .small()
                    .label(language.text(Message::CliUpdateUpgrade))
                    .disabled(busy || self.error.is_some())
                    .on_click(cx.listener(|this, _, _, cx| this.upgrade(cx))),
            );
        }
        body.child(actions)
    }
}
