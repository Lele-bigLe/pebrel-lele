//! The shared native Agent picker and delivery boundary for quotes and Git reviews.
use super::*;

#[derive(Clone)]
pub(in crate::gpui_shell::workspace) struct ChatPicker {
    pub(in crate::gpui_shell::workspace) select: Entity<SelectState<Vec<SendToChatTarget>>>,
    pub(super) labels: Arc<HashMap<ChatTarget, (SharedString, Option<WeakEntity<TerminalView>>)>>,
    pub(super) project: Option<PathBuf>,
    pub(super) count: usize,
    choices: Arc<Vec<SendToChatTarget>>,
    chooser_open: std::rc::Rc<std::cell::Cell<bool>>,
    chooser_focus: gpui::FocusHandle,
    query: Entity<InputState>,
    _query_subscription: std::rc::Rc<gpui::Subscription>,
}

impl ChatPicker {
    pub(in crate::gpui_shell::workspace) fn empty(&self) -> bool {
        self.count == 0
    }
    pub(in crate::gpui_shell::workspace) fn selected(&self, cx: &App) -> Option<ChatTarget> {
        self.select.read(cx).selected_value().copied()
    }
    pub(in crate::gpui_shell::workspace) fn title(&self, target: ChatTarget) -> SharedString {
        self.labels.get(&target).map(|(title, _)| title.clone()).unwrap_or_default()
    }
    pub(super) fn focus_handle(&self) -> gpui::FocusHandle {
        self.chooser_focus.clone()
    }

    /// Keep options inside the modal: the library's deferred Select menu is
    /// painted below the workspace modal layer.
    pub(in crate::gpui_shell::workspace) fn render_control(
        &self,
        disabled: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::AnyElement {
        let selected = self.selected(cx);
        let view = window.current_view();
        let open = self.chooser_open.clone();
        let escape = self.chooser_open.clone();
        let trigger_focus = self.chooser_focus.clone();
        let language = workspace_ui_language();
        let enabled = !disabled && !self.empty();
        let query = self.query.read(cx).value().trim().to_lowercase();
        let key_open = self.chooser_open.clone();
        let key_focus = self.chooser_focus.clone();
        v_flex()
            .w_full()
            .min_w_0()
            .gap_1()
            .track_focus(&self.chooser_focus)
            .on_key_down(move |event, window, cx| {
                if event.keystroke.key == "escape" && escape.replace(false) {
                    trigger_focus.focus(window, cx);
                    cx.notify(view);
                    cx.stop_propagation();
                } else if enabled
                    && key_focus.is_focused(window)
                    && matches!(event.keystroke.key.as_str(), "enter" | "space")
                {
                    key_open.set(!key_open.get());
                    cx.notify(view);
                    cx.stop_propagation();
                }
            })
            .child(
                Button::new("chat-target-chooser")
                    .w_full()
                    .h(px(32.0))
                    .small()
                    .dropdown_caret(true)
                    .disabled(disabled || self.empty())
                    .selected(self.chooser_open.get())
                    .child(div().flex_1().min_w_0().truncate().text_left().child(
                        selected.map(|target| self.title(target)).unwrap_or_else(|| {
                            language.text(Message::ReviewTargetPlaceholder).into()
                        }),
                    ))
                    .on_click(move |_, _, cx| {
                        open.set(!open.get());
                        cx.notify(view);
                    }),
            )
            .when(self.chooser_open.get(), |body| {
                body.child(
                    v_flex()
                        .id("chat-target-options")
                        .w_full()
                        .min_w_0()
                        .max_h(px(
                            (f32::from(window.viewport_size().height) * 0.3).clamp(64.0, 160.0)
                        ))
                        .flex_shrink_0()
                        .overflow_y_scrollbar()
                        .p_1()
                        .border_1()
                        .border_color(cx.theme().border)
                        .rounded_md()
                        .when(self.count > 6, |body| {
                            body.child(Input::new(&self.query).small().w_full())
                        })
                        .children(
                            self.choices
                                .iter()
                                .enumerate()
                                .filter(|(_, choice)| choice.matches(&query))
                                .map(|(index, choice)| {
                                    let target = choice.target;
                                    let select = self.select.clone();
                                    let open = self.chooser_open.clone();
                                    let focus = self.chooser_focus.clone();
                                    Button::new(("chat-target-option", index))
                                        .ghost()
                                        .w_full()
                                        .h(px(42.0))
                                        .flex_shrink_0()
                                        .p_0()
                                        .px_2()
                                        .disabled(disabled)
                                        .selected(selected == Some(target))
                                        .child(choice.render(window, cx))
                                        .on_click(move |_, window, cx| {
                                            select.update(cx, |select, cx| {
                                                select.set_selected_value(&target, window, cx)
                                            });
                                            open.set(false);
                                            focus.focus(window, cx);
                                            cx.notify(view);
                                        })
                                }),
                        ),
                )
            })
            .into_any_element()
    }
}

impl NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn project_chat_picker(
        &self,
        project: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ChatPicker {
        let language = workspace_ui_language();
        let mut targets = self.send_to_chat_targets(cx);
        targets.extend(super::super::windowing::other_chat_targets(self.runtime_window_id, cx));
        let project = project.filter(|root| root.is_absolute());
        if project.is_some() {
            for kind in
                [AgentKind::Codex, AgentKind::Claude, AgentKind::Gemini, AgentKind::OpenCode]
            {
                targets.push(SendToChatTarget {
                    target: ChatTarget::New(kind),
                    view: None,
                    project: project.clone(),
                    title: language
                        .format(Message::ChatTargetCreate, &[("agent", kind.display_name())])
                        .into(),
                    agent: kind.display_name().into(),
                    remote: None,
                    search: kind.display_name().to_lowercase(),
                });
            }
        }
        let default = targets
            .iter()
            .find(|target| {
                target.remote.is_none()
                    && target.agent.as_ref() == "Codex"
                    && target.view.is_some()
                    && match (&target.project, &project) {
                        (Some(a), Some(b)) => crate::project_library::same_root(a, b),
                        (None, None) => true,
                        _ => false,
                    }
            })
            .or_else(|| {
                targets.iter().find(|target| target.target == ChatTarget::New(AgentKind::Codex))
            })
            .or_else(|| {
                targets
                    .iter()
                    .find(|target| target.remote.is_none() && target.agent.as_ref() == "Codex")
            })
            .map(|target| target.target);
        let count = targets.len();
        let labels = Arc::new(
            targets
                .iter()
                .map(|target| (target.target, (target.title.clone(), target.view.clone())))
                .collect(),
        );
        let choices = Arc::new(targets.clone());
        let query = cx.new(|cx| {
            InputState::new(window, cx).placeholder(language.text(Message::ChatTargetSearch))
        });
        let query_subscription = std::rc::Rc::new(cx.observe(&query, |_, _, cx| cx.notify()));
        let select = cx.new(|cx| {
            let mut state = SelectState::new(targets, None, window, cx).searchable(count > 6);
            if let Some(default) = default {
                state.set_selected_value(&default, window, cx);
            }
            state
        });
        ChatPicker {
            select,
            labels,
            project,
            count,
            choices,
            chooser_open: Default::default(),
            chooser_focus: cx.focus_handle(),
            query,
            _query_subscription: query_subscription,
        }
    }

    pub(in crate::gpui_shell::workspace) fn deliver_project_chat(
        &mut self,
        picker: &ChatPicker,
        target: ChatTarget,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), crate::runtime_api::ApiError> {
        match target {
            ChatTarget::New(kind) => {
                self.start_chat_target(kind, picker.project.clone(), message, window, cx)
            },
            ChatTarget::Pane(_) => picker
                .labels
                .get(&target)
                .and_then(|(_, view)| view.as_ref()?.upgrade())
                .ok_or_else(|| {
                    crate::runtime_api::ApiError::new(
                        "target_not_found",
                        workspace_ui_language().text(Message::ChatTargetClosed),
                    )
                })
                .and_then(|view| {
                    view.update(cx, |view, cx| {
                        view.runtime_chat_message(message, InputOrigin::User, cx)
                    })
                }),
        }
    }
}
