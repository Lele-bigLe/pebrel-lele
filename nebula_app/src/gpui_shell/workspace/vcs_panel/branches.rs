//! The dialog owns branch discovery; mutations use the existing VCS queue.
use super::*;
use crate::display::side_panel::GitLocation;
use gpui::{EventEmitter, InteractiveElement as _, StatefulInteractiveElement as _, uniform_list};

struct BranchChange {
    location: GitLocation,
    name: String,
    create: bool,
}

impl NebulaWorkspace {
    pub(super) fn open_branch_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(location) = self.side_panel.git_location() else { return };
        let manager = cx.new(|cx| Branches::new(location, window, cx));
        cx.subscribe_in(&manager, window, |this, _, action: &BranchChange, window, cx| {
            if this.side_panel.git_location().as_ref() != Some(&action.location)
                || !this.side_panel.git_switch_branch(&action.name, action.create)
            {
                let message =
                    crate::gpui_shell::config::ui_language(cx).text(Message::GitControlsChanged);
                crate::gpui_shell::toast::toast(
                    window,
                    cx,
                    crate::display::ToastKind::Warning,
                    message,
                );
            } else {
                window.close_dialog(cx);
                cx.notify();
            }
        })
        .detach();
        let language = crate::gpui_shell::config::ui_language(cx);
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(language.text(Message::GitControlsBranches))
                .w(px(520.0))
                .child(manager.clone())
        });
    }
}

struct Branches {
    location: GitLocation,
    query: Entity<InputState>,
    name: Entity<InputState>,
    entries: Vec<crate::runtime_api::git::BranchInfo>,
    task: Option<gpui::Task<()>>,
    error: Option<String>,
}
impl EventEmitter<BranchChange> for Branches {}

impl Branches {
    fn new(location: GitLocation, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let language = crate::gpui_shell::config::ui_language(cx);
        let query = cx.new(|cx| {
            InputState::new(window, cx).placeholder(language.text(Message::GitControlsSearch))
        });
        let name = cx.new(|cx| {
            InputState::new(window, cx).placeholder(language.text(Message::GitControlsNewName))
        });
        for input in [&query, &name] {
            cx.subscribe(input, |_, _, _: &gpui_component::input::InputEvent, cx| cx.notify())
                .detach();
        }
        let mut this = Self { location, query, name, entries: vec![], task: None, error: None };
        this.refresh(cx);
        this
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        let location = self.location.clone();
        self.error = None;
        let work = cx
            .background_executor()
            .spawn(async move { crate::runtime_api::git::read_branches(&location) });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.task = None;
                match result {
                    Ok(entries) => this.entries = entries,
                    Err(error) => this.error = Some(error.message),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

impl gpui::Render for Branches {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let query = self.query.read(cx).value().to_lowercase();
        let matches: Arc<Vec<_>> = Arc::new(
            self.entries
                .iter()
                .filter(|entry| entry.name.to_lowercase().contains(query.trim()))
                .map(|entry| (entry.name.clone(), entry.current))
                .collect(),
        );
        let name = self.name.read(cx).value().trim().to_owned();
        let disabled = self.task.is_some()
            || self.error.is_some()
            || name.is_empty()
            || name.starts_with('-')
            || self.entries.iter().any(|entry| entry.name == name);
        let path = match &self.location {
            GitLocation::Local { root } => root.display().to_string(),
            GitLocation::Wsl { distro, root } => format!("{distro}:{root}"),
        };
        v_flex()
            .w_full()
            .gap_3()
            .child(div().text_xs().text_color(cx.theme().muted_foreground).truncate().child(path))
            .child(div().text_sm().child(language.text(Message::GitControlsHint)))
            .child(
                h_flex().gap_2().child(div().flex_1().child(Input::new(&self.query))).child(
                    Button::new("branches-refresh")
                        .ghost()
                        .small()
                        .label(language.text(Message::UsageRefresh))
                        .loading(self.task.is_some())
                        .disabled(self.task.is_some())
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                ),
            )
            .when_some(self.error.as_ref(), |body, error| {
                body.child(div().text_sm().text_color(cx.theme().danger).child(error.clone()))
            })
            .child(
                uniform_list(
                    "local-branches",
                    matches.len(),
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|index| matches.get(index))
                            .map(|(name, current)| {
                                let current = *current;
                                let name = name.clone();
                                h_flex()
                                    .h(px(34.0))
                                    .w_full()
                                    .child(
                                        Button::new(SharedString::from(format!("branch-{name}")))
                                            .ghost()
                                            .small()
                                            .w_full()
                                            .label(name.clone())
                                            .selected(current)
                                            .disabled(
                                                current
                                                    || this.task.is_some()
                                                    || this.error.is_some(),
                                            )
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.emit(BranchChange {
                                                    location: this.location.clone(),
                                                    name: name.clone(),
                                                    create: false,
                                                })
                                            })),
                                    )
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .w_full()
                .h(px(240.0)),
            )
            .child(
                h_flex().gap_2().child(div().flex_1().child(Input::new(&self.name))).child(
                    Button::new("branch-create")
                        .primary()
                        .small()
                        .label(language.text(Message::GitControlsCreate))
                        .disabled(disabled)
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.emit(BranchChange {
                                location: this.location.clone(),
                                name: this.name.read(cx).value().trim().to_owned(),
                                create: true,
                            });
                        })),
                ),
            )
    }
}
