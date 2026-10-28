//! Keep the commit draft while refreshing its placeholder after a language change.

use gpui::{App, AppContext as _, Entity, Subscription, Window};
use gpui_component::input::{InputEvent, InputState};

use crate::gpui_shell::config::ui_language;
use crate::i18n::{Message, UiLanguage};

pub(in crate::gpui_shell::workspace) struct CommitInput {
    pub(in crate::gpui_shell::workspace) input: Entity<InputState>,
    language: UiLanguage,
    pending: Option<PendingCommit>,
    _events: Option<Subscription>,
}

struct PendingCommit {
    message: String,
    location: crate::display::side_panel::GitLocation,
    result: std::sync::mpsc::Receiver<bool>,
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::*;
    use gpui::{Element as _, IntoElement as _, Render, RenderOnce as _, div};
    use gpui_component::{ElementExt as _, input::Input};
    use std::sync::{Arc, Mutex};

    struct CommitInputProbe {
        commit: CommitInput,
        placeholder: Arc<Mutex<Option<String>>>,
    }

    impl Render for CommitInputProbe {
        fn render(
            &mut self,
            window: &mut Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            self.commit.sync_language(window, cx);
            let input = self.commit.input.clone();
            let placeholder = self.placeholder.clone();
            div().on_prepaint(move |_, window, cx| {
                let element = Input::new(&input).render(window, cx).into_element();
                let mut node = gpui::accesskit::Node::new(gpui::accesskit::Role::TextInput);
                element.write_a11y_info(&mut node);
                *placeholder.lock().unwrap() = node.placeholder().map(str::to_owned);
            })
        }
    }

    #[gpui::test]
    fn commit_placeholder_tracks_language_without_replacing_the_draft(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(gpui_component::init);
        let placeholder = Arc::new(Mutex::new(None));
        let captured = placeholder.clone();
        // The early-startup fallback is English even before Settings is registered.
        let (probe, cx) = cx.add_window_view(|window, cx| CommitInputProbe {
            commit: CommitInput::new(window, cx),
            placeholder,
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert_eq!(captured.lock().unwrap().as_deref(), Some("Commit message..."));
        let input = probe.read_with(cx, |probe, _| probe.commit.input.clone());
        cx.update(|window, cx| {
            input.update(cx, |input, cx| {
                input.set_value("fix: 保留 draft", window, cx);
                input.set_selected_range(0..3, cx);
            });
        });
        let original_id = input.entity_id();
        let original_cursor = input.read_with(cx, |input, _| input.cursor_position());
        let original_selection = input.read_with(cx, |input, _| input.selected_value());
        assert_eq!(original_selection, "fix");
        for (language, expected) in [
            (UiLanguage::ZhCn, "提交信息…"),
            (UiLanguage::for_locale(Some("en-GB")), "Commit message..."),
            (UiLanguage::FrFr, "Commit message..."),
            (UiLanguage::ZhCn, "提交信息…"),
        ] {
            cx.update(|window, cx| {
                let mut settings =
                    crate::gpui_shell::config::Settings::load(nebula_settings::ThemeName::Nord);
                settings.ui_language = language;
                cx.set_global(settings);
                probe.update(cx, |_, cx| cx.notify());
                let _ = window.draw(cx);
            });
            assert_eq!(captured.lock().unwrap().as_deref(), Some(expected));
            assert_eq!(probe.read_with(cx, |probe, _| probe.commit.input.entity_id()), original_id);
            assert_eq!(input.read_with(cx, |input, _| input.value()), "fix: 保留 draft");
            assert_eq!(input.read_with(cx, |input, _| input.cursor_position()), original_cursor);
            assert_eq!(input.read_with(cx, |input, _| input.selected_value()), original_selection);
        }
    }
}

impl CommitInput {
    pub(in crate::gpui_shell::workspace) fn new(window: &mut Window, cx: &mut App) -> Self {
        let language = ui_language(cx);
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(1, 4)
                .soft_wrap(true)
                .placeholder(language.text(Message::VcsCommitPlaceholder))
        });
        Self { input, language, pending: None, _events: None }
    }

    pub(super) fn sync_language(&mut self, window: &mut Window, cx: &mut App) {
        let language = ui_language(cx);
        if self.language == language {
            return;
        }
        self.input.update(cx, |input, cx| {
            input.set_placeholder(language.text(Message::VcsCommitPlaceholder), window, cx);
        });
        self.language = language;
    }
}

impl super::NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn new_commit_input(
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> CommitInput {
        let mut commit = CommitInput::new(window, cx);
        commit._events = Some(cx.subscribe(&commit.input, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        }));
        commit
    }

    pub(super) fn sync_vcs_commit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        use std::sync::mpsc::TryRecvError;
        let success =
            match self.git_commit_input.pending.as_ref().map(|pending| pending.result.try_recv()) {
                Some(Ok(success)) => success,
                Some(Err(TryRecvError::Disconnected)) => {
                    self.git_commit_input.pending = None;
                    return;
                },
                _ => return,
            };
        let Some(pending) = self.git_commit_input.pending.take() else { return };
        if success
            && self.side_panel.git_location().as_ref() == Some(&pending.location)
            && self.git_commit_input.input.read(cx).value().as_ref() == pending.message.as_str()
        {
            self.git_commit_input.input.update(cx, |input, cx| input.set_value("", window, cx));
        }
    }

    fn submit_vcs_commit_with_push(
        &mut self,
        push_after: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.sync_vcs_commit(window, cx);
        let message = self.git_commit_input.input.read(cx).value().to_string();
        if message.trim().is_empty()
            || self.side_panel.op_running()
            || self.git_commit_input.pending.is_some()
        {
            return;
        }
        if let Some(location) = self.side_panel.git_location() {
            let Some(result) = self.side_panel.git_commit_with_push(&message, push_after) else {
                return;
            };
            self.git_commit_input.pending = Some(PendingCommit { message, location, result });
        } else if !push_after {
            self.side_panel.vcs_commit_message(&message);
            self.git_commit_input.input.update(cx, |input, cx| input.set_value("", window, cx));
        }
        cx.notify();
    }

    fn submit_vcs_commit_at(
        &mut self,
        root: &Option<std::path::PathBuf>,
        push_after: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if root.is_some() && self.side_panel.vcs_root() == root.as_deref() {
            self.submit_vcs_commit_with_push(push_after, window, cx);
        }
    }

    pub(super) fn render_vcs_commit_form(
        &self,
        ready: bool,
        is_git: bool,
        cx: &gpui::Context<Self>,
    ) -> gpui::AnyElement {
        use super::*;
        use gpui_component::button::DropdownButton;
        let language = crate::gpui_shell::config::ui_language(cx);
        let root = self.side_panel.vcs_root().map(std::path::Path::to_path_buf);
        let button_root = root.clone();
        let menu_root = root.clone();
        let empty = self.git_commit_input.input.read(cx).value().trim().is_empty();
        let hint = language.text(if !is_git {
            Message::VcsCommit
        } else if self.side_panel.git_snapshot().is_some_and(|info| !info.conflicts.is_empty()) {
            Message::GitControlsResolveBeforeCommit
        } else if self.side_panel.git_snapshot().is_none_or(|info| info.staged.is_empty()) {
            Message::GitControlsStageBeforeCommit
        } else if empty {
            Message::VcsCommitPlaceholder
        } else {
            Message::GitControlsCommitShortcut
        });
        let button = Button::new("vcs-commit")
            .primary()
            .small()
            .flex_1()
            .icon(IconName::Check)
            .label(language.text(Message::VcsCommit))
            .tooltip(hint)
            .disabled(!ready || empty)
            .loading(self.git_commit_input.pending.is_some())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.submit_vcs_commit_at(&button_root, false, window, cx)
            }));
        let action = if is_git {
            let owner = cx.entity().downgrade();
            DropdownButton::new("git-commit-actions")
                .primary()
                .small()
                .w_full()
                .min_w_0()
                .disabled(!ready || empty)
                .loading(self.git_commit_input.pending.is_some())
                .tooltip(language.text(Message::GitControlsCommitOptions))
                .button(button)
                .dropdown_menu(move |menu, _, _| {
                    let commit = owner.clone();
                    let push = owner.clone();
                    let commit_root = menu_root.clone();
                    let push_root = menu_root.clone();
                    menu.item(PopupMenuItem::new(language.text(Message::VcsCommit)).on_click(
                        move |_, window, cx| {
                            if let Some(owner) = commit.upgrade() {
                                owner.update(cx, |this, cx| {
                                    this.submit_vcs_commit_at(&commit_root, false, window, cx)
                                });
                            }
                        },
                    ))
                    .item(
                        PopupMenuItem::new(language.text(Message::GitControlsCommitAndPush))
                            .on_click(move |_, window, cx| {
                                if let Some(owner) = push.upgrade() {
                                    owner.update(cx, |this, cx| {
                                        this.submit_vcs_commit_at(&push_root, true, window, cx)
                                    });
                                }
                            }),
                    )
                })
                .into_any_element()
        } else {
            button.w_full().into_any_element()
        };
        v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .w_full()
                    .capture_action(cx.listener(
                        move |this, action: &gpui_component::input::Enter, window, cx| {
                            if action.secondary && !action.shift {
                                this.submit_vcs_commit_at(&root, false, window, cx);
                                cx.stop_propagation();
                            } else {
                                cx.propagate();
                            }
                        },
                    ))
                    .child(Input::new(&self.git_commit_input.input).w_full()),
            )
            .child(action)
            .into_any_element()
    }
}
