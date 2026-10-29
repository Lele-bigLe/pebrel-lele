//! Compact per-pane commands share the existing Agent input and focus owners.

use super::super::*;
use crate::display::UiLanguage;
use crate::gpui_shell::terminal::view::shortcuts::{AgentShortcut, AgentShortcutTarget};
use crate::i18n::Message;
use gpui::WeakEntity;
use gpui_component::menu::PopupMenuItem;

fn label(shortcut: AgentShortcut, language: UiLanguage) -> &'static str {
    language.text(match shortcut {
        AgentShortcut::Model => Message::AgentShortcutsModel,
        AgentShortcut::Usage => Message::AgentShortcutsUsage,
        AgentShortcut::Presets => Message::AgentShortcutsPresets,
        AgentShortcut::Status => Message::AgentShortcutsStatus,
        AgentShortcut::Plan => Message::AgentShortcutsPlan,
        AgentShortcut::Permissions => Message::AgentShortcutsPermissions,
        AgentShortcut::Context => Message::AgentShortcutsContext,
        AgentShortcut::Compact => Message::AgentShortcutsCompact,
        AgentShortcut::Diff => Message::AgentShortcutsDiff,
    })
}

impl NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn render_agent_shortcuts(
        &self,
        view: &Entity<TerminalView>,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let state = view.read(cx);
        let target = state.agent_shortcut_target()?;
        let ready = state.agent_shortcuts_ready();
        let local = !state.is_remote_session();
        let history_visible = local
            && state.wsl_distro().is_none()
            && target.kind == crate::ai_agents::AgentKind::Codex;
        let pane = state.pane_id;
        let history_target = history_visible
            .then(|| state.session_agent())
            .flatten()
            .filter(|agent| agent.source == "codex")
            .and_then(|mut agent| {
                agent.normalize_identity();
                agent.session_id
            });
        let history_root = state.working_directory().map(std::path::PathBuf::from);
        let history_owner = cx.entity().downgrade();
        let language = crate::gpui_shell::config::ui_language(cx);
        let owner = cx.entity().downgrade();
        let weak_view = view.downgrade();
        let model_target = target.clone();
        let model_view = weak_view.clone();
        let model_owner = owner.clone();
        let hint = if ready {
            language.format(Message::AgentShortcutsHint, &[("command", "/model")])
        } else {
            language.text(Message::AgentShortcutsBusy).to_owned()
        };
        let model = Button::new(SharedString::from(format!("agent-model-{pane}")))
            .ghost()
            .small()
            .label(label(AgentShortcut::Model, language))
            .disabled(!ready)
            .aria_label(hint.clone())
            .tooltip(hint)
            .on_click(move |_, window, cx| {
                if let Some(owner) = model_owner.upgrade() {
                    owner.update(cx, |this, cx| {
                        this.apply_agent_shortcut(
                            &model_view,
                            &model_target,
                            AgentShortcut::Model,
                            window,
                            cx,
                        );
                    });
                }
            });
        let commands = Button::new(SharedString::from(format!("agent-commands-{pane}")))
            .ghost()
            .small()
            .icon(IconName::Ellipsis)
            .aria_label(language.text(Message::AgentShortcutsMenu))
            .tooltip(language.text(Message::AgentShortcutsMenu))
            .dropdown_menu(move |mut menu, _, _| {
                for shortcut in [
                    AgentShortcut::Usage,
                    AgentShortcut::Presets,
                    AgentShortcut::Status,
                    AgentShortcut::Plan,
                    AgentShortcut::Permissions,
                    AgentShortcut::Context,
                    AgentShortcut::Compact,
                    AgentShortcut::Diff,
                ] {
                    let Some(command) = shortcut.command(target.kind) else { continue };
                    let owner = owner.clone();
                    let view = weak_view.clone();
                    let target = target.clone();
                    menu = menu.item(
                        PopupMenuItem::new(format!("{}  {command}", label(shortcut, language)))
                            .disabled(!ready)
                            .on_click(move |_, window, cx| {
                                if let Some(owner) = owner.upgrade() {
                                    owner.update(cx, |this, cx| {
                                        this.apply_agent_shortcut(
                                            &view, &target, shortcut, window, cx,
                                        );
                                    });
                                }
                            }),
                    );
                }
                if local {
                    let kind = target.kind;
                    menu = menu.item(PopupMenuItem::separator()).item(
                        PopupMenuItem::new(language.text(Message::CliUpdateCheck))
                            .on_click(move |_, window, cx| super::updates::open(kind, window, cx)),
                    );
                }
                menu
            });
        Some(
            h_flex()
                .flex_shrink_0()
                .gap_1()
                .child(model)
                .when(history_visible, |row| {
                    row.child(
                        Button::new(SharedString::from(format!("agent-turn-changes-{pane}")))
                            .ghost()
                            .small()
                            .label(language.text(Message::TurnChangesTitle))
                            .disabled(history_target.is_none() || history_root.is_none())
                            .tooltip(language.text(if history_target.is_some() {
                                Message::TurnChangesHint
                            } else {
                                Message::TurnChangesIdentifying
                            }))
                            .on_click(move |_, window, cx| {
                                if let (Some(thread), Some(root)) = (&history_target, &history_root)
                                {
                                    if let Some(owner) = history_owner.upgrade() {
                                        owner.update(cx, |this, cx| {
                                            this.open_turn_review(thread.clone(), root.clone(), pane, window, cx);
                                        });
                                    }
                                }
                            }),
                    )
                })
                .child(commands)
                .into_any_element(),
        )
    }

    fn apply_agent_shortcut(
        &mut self,
        view: &WeakEntity<TerminalView>,
        target: &AgentShortcutTarget,
        shortcut: AgentShortcut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let language = crate::gpui_shell::config::ui_language(cx);
        let result = (|| {
            let view = view.upgrade().ok_or("the terminal has closed".to_owned())?;
            let (tab, pane) =
                self.locate_pane(view.entity_id()).ok_or("the pane has closed".to_owned())?;
            if tab != self.active || self.settings_open || self.workbench_changes(cx) {
                return Err("the terminal is no longer visible".to_owned());
            }
            let command = view
                .update(cx, |view, cx| view.run_agent_shortcut(target, shortcut, cx))
                .map_err(|error| {
                    if error.code == "input_not_empty" {
                        language.text(Message::AgentShortcutsDraft).to_owned()
                    } else {
                        error.message
                    }
                })?;
            self.focus_pane(tab, pane, window, cx);
            let focus = view.read(cx).focus_handle.clone();
            focus.focus(window, cx);
            Ok(command)
        })();
        let (kind, text) = match result {
            Ok(command) => (
                crate::display::ToastKind::Info,
                language.format(Message::AgentShortcutsInserted, &[("command", command)]),
            ),
            Err(error) => (
                crate::display::ToastKind::Warning,
                language.format(Message::AgentShortcutsFailed, &[("error", &error)]),
            ),
        };
        crate::gpui_shell::toast::toast(window, cx, kind, text);
    }
}
