//! A projection of live diff drafts and detached window-lifetime review text.

use super::super::send_to_chat::ChatPicker;
use super::*;
use crate::display::side_panel::GitLocation;
use crate::gpui_shell::code_tab::CodeTabView;
use crate::gpui_shell::widgets::{SCROLLBAR_GUTTER, scrollbar_gutter};
use gpui::{EntityId, EventEmitter, WeakEntity};
use std::collections::HashSet;
mod actions;

#[derive(Clone)]
enum Source {
    Live(WeakEntity<CodeTabView>),
    Retained(u64),
}

enum QueueEvent {
    Open(Source, Option<u64>),
    Remove(Source, u64),
}

impl NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn open_review_queue(
        &mut self,
        origin: &Entity<CodeTabView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(location) = origin.read(cx).review_location().cloned() else { return };
        self.open_repository_review_queue(location, window, cx);
    }

    pub(in crate::gpui_shell::workspace) fn open_repository_review_queue(
        &mut self,
        location: GitLocation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = cx.entity().downgrade();
        // The workspace is leased during child construction; pass its sources in.
        let sources = Queue::sources(self, &location, cx);
        let project = match &location {
            GitLocation::Local { root } => Some(root.clone()),
            GitLocation::Wsl { .. } => None,
        };
        let picker = self.project_chat_picker(project, window, cx);
        let queue = cx.new(|cx| Queue::new(owner, location.clone(), sources, picker, cx));
        cx.subscribe_in(&queue, window, move |this, _, event: &QueueEvent, window, cx| {
            let (source, comment, remove) = match event {
                QueueEvent::Open(source, comment) => (source, *comment, false),
                QueueEvent::Remove(source, id) => (source, Some(*id), true),
            };
            let source = match source {
                Source::Live(view) => view.upgrade(),
                Source::Retained(id) => {
                    if remove {
                        if let Some(record) =
                            this.review.retained.iter_mut().find(|record| {
                                record.id == *id && record.draft.location == location
                            })
                        {
                            record.draft.remove_comment(comment.unwrap());
                        }
                        this.review.retained.retain(|record| !record.draft.empty());
                        cx.notify();
                        return;
                    }
                    if this.open_retained_review(*id, window, cx) {
                        window.close_dialog(cx);
                        if let Some(WorkspaceTab::Code { view, .. }) = this.tabs.get(this.active) {
                            view.update(cx, |view, cx| match comment {
                                Some(id) => view.locate_review_comment(id, window, cx),
                                None => view.open_review_tools(window, cx),
                            });
                        }
                    }
                    return;
                },
            };
            let index = source.as_ref().and_then(|source| {
                this.tabs.iter().position(|tab| {
                    matches!(tab, WorkspaceTab::Code { view, .. }
                        if view.entity_id() == source.entity_id())
                })
            });
            let Some((view, index)) = source
                .zip(index)
                .filter(|(view, _)| view.read(cx).review_location() == Some(&location))
            else {
                let language = crate::gpui_shell::config::ui_language(cx);
                crate::gpui_shell::toast::toast(
                    window,
                    cx,
                    crate::display::ToastKind::Warning,
                    language.text(Message::ReviewFileClosed),
                );
                cx.notify();
                return;
            };
            if remove {
                view.update(cx, |view, cx| view.remove_review_comment(comment.unwrap(), cx));
                cx.notify();
            } else {
                window.close_dialog(cx);
                this.activate_tab(index, window, cx);
                view.update(cx, |view, cx| match comment {
                    Some(id) => view.locate_review_comment(id, window, cx),
                    None => view.open_review_tools(window, cx),
                });
            }
        })
        .detach();
        let language = crate::gpui_shell::config::ui_language(cx);
        window.open_dialog(cx, move |dialog, window, _| {
            let width = (f32::from(window.viewport_size().width) - 48.0).clamp(240.0, 720.0);
            dialog
                .title(language.text(Message::ReviewQueueTitle))
                .w(px(width))
                .margin_top(px(24.0))
                .max_h(window.viewport_size().height - px(48.0))
                .on_ok(|_, _, _| false)
                .content({
                    let queue = queue.clone();
                    move |content, _, _| content.child(queue.clone())
                })
        });
    }
}

struct Queue {
    owner: WeakEntity<NebulaWorkspace>,
    location: GitLocation,
    views: HashSet<EntityId>,
    view_subscriptions: Vec<Subscription>,
    _owner_subscription: Option<Subscription>,
    picker: ChatPicker,
    _picker_subscription: Subscription,
    prepared: Option<(Vec<Draft>, String)>,
    show_preview: bool,
    scroll: gpui::ScrollHandle,
    task: Option<gpui::Task<()>>,
    issue: Option<String>,
}

impl EventEmitter<QueueEvent> for Queue {}

impl Queue {
    fn new(
        owner: WeakEntity<NebulaWorkspace>,
        location: GitLocation,
        sources: Vec<Entity<CodeTabView>>,
        picker: ChatPicker,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            owner,
            location,
            views: HashSet::new(),
            view_subscriptions: vec![],
            _owner_subscription: None,
            _picker_subscription: cx.observe(&picker.select, |_, _, cx| cx.notify()),
            picker,
            prepared: None,
            show_preview: false,
            scroll: gpui::ScrollHandle::default(),
            task: None,
            issue: None,
        };
        if let Some(owner) = this.owner.upgrade() {
            this._owner_subscription = Some(cx.observe(&owner, |this, _, cx| {
                this.sync_views(cx);
                cx.notify();
            }));
        }
        this.track_views(sources, cx);
        this
    }

    fn sources(
        owner: &NebulaWorkspace,
        location: &GitLocation,
        cx: &App,
    ) -> Vec<Entity<CodeTabView>> {
        owner
            .tabs
            .iter()
            .filter_map(|tab| match tab {
                WorkspaceTab::Code { view, .. }
                    if view.read(cx).review_location() == Some(location) =>
                {
                    Some(view.clone())
                },
                _ => None,
            })
            .collect()
    }

    fn sync_views(&mut self, cx: &mut Context<Self>) {
        let sources: Vec<_> = self
            .owner
            .upgrade()
            .map(|owner| Self::sources(owner.read(cx), &self.location, cx))
            .unwrap_or_default();
        self.track_views(sources, cx);
    }

    fn track_views(&mut self, sources: Vec<Entity<CodeTabView>>, cx: &mut Context<Self>) {
        let ids = sources.iter().map(Entity::entity_id).collect::<HashSet<_>>();
        if self.views == ids {
            return;
        }
        self.views = ids;
        self.view_subscriptions =
            sources.iter().map(|view| cx.observe(view, |_, _, cx| cx.notify())).collect();
    }
}

impl Render for Queue {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let muted = cx.theme().muted_foreground;
        let repository = match &self.location {
            GitLocation::Local { root } => root.display().to_string(),
            GitLocation::Wsl { distro, root } => format!("{distro}:{root}"),
        };
        let hint = repository.clone();
        let height = (f32::from(window.viewport_size().height) - 180.0).clamp(120.0, 640.0);
        let mut body = v_flex()
            .id("review-queue-body")
            .w_full()
            .min_w_0()
            .h(px(height))
            .flex_shrink_0()
            .min_h_0()
            .gap_3()
            .child(
                div()
                    .id("review-repository")
                    .flex_shrink_0()
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
                    .child(repository),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .whitespace_normal()
                    .text_color(muted)
                    .child(language.text(Message::ReviewScope)),
            );
        let Some(owner) = self.owner.upgrade() else {
            return body.child(language.text(Message::ReviewQueueEmpty)).into_any_element();
        };
        let mut files: Vec<_> = owner
            .read(cx)
            .tabs
            .iter()
            .filter_map(|tab| match tab {
                WorkspaceTab::Code { view, .. } => {
                    view.read(cx).review_summary(&self.location, cx).map(|summary| {
                        (
                            Source::Live(view.downgrade()),
                            summary,
                            view.read(cx).review_comments().to_vec(),
                        )
                    })
                },
                _ => None,
            })
            .collect();
        files.extend(
            owner
                .read(cx)
                .review
                .retained
                .iter()
                .filter(|record| record.draft.location == self.location)
                .map(|record| {
                    (
                        Source::Retained(record.id),
                        record.draft.summary(),
                        record.draft.comments().to_vec(),
                    )
                }),
        );
        let file_count = files
            .iter()
            .filter(|(_, file, _)| file.comments > 0)
            .map(|(_, file, _)| file.path.as_str())
            .collect::<HashSet<_>>()
            .len();
        let comment_count: usize = files.iter().map(|(_, file, _)| file.comments).sum();
        let pending = files.iter().filter(|(_, file, _)| file.pending).count();
        let blocked = files.iter().any(|(_, file, _)| file.comments > 0 && !file.ready);
        let ready = self.preview_ready(cx);
        if comment_count > 0 && !blocked && self.task.is_none() && self.issue.is_none() && !ready {
            self.prepare(false, window, cx);
        }
        let busy = self.task.is_some();
        body = body.child(
            h_flex()
                .w_full()
                .min_w_0()
                .flex_shrink_0()
                .gap_2()
                .flex_wrap()
                .child(div().flex_1().text_sm().child(language.format(
                    Message::ReviewQueueCount,
                    &[("files", &file_count.to_string()), ("comments", &comment_count.to_string())],
                )))
                .child(
                    Button::new("review-queue-comments")
                        .ghost()
                        .small()
                        .label(language.text(Message::ReviewQueueComments))
                        .toggled(!self.show_preview)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_preview = false;
                            this.scroll.set_offset(gpui::point(px(0.0), px(0.0)));
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("review-queue-preview")
                        .ghost()
                        .small()
                        .label(language.text(Message::ReviewQueuePreview))
                        .toggled(self.show_preview)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.show_preview = true;
                            this.scroll.set_offset(gpui::point(px(0.0), px(0.0)));
                            if !this.preview_ready(cx) {
                                this.prepare(false, window, cx);
                            }
                            cx.notify();
                        })),
                ),
        );
        let mut rows = v_flex().w_full().min_w_0().gap_2();
        if comment_count == 0 {
            rows = rows.child(
                div()
                    .py_4()
                    .text_sm()
                    .text_color(muted)
                    .child(language.text(Message::ReviewQueueEmpty)),
            );
        }
        for (index, (view, file, comments)) in files.into_iter().enumerate() {
            let closed = matches!(view, Source::Retained(_));
            let hint = file.path.clone();
            let name = std::path::Path::new(&file.path)
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new(&file.path))
                .to_string_lossy()
                .into_owned();
            let mut labels = h_flex()
                .gap_2()
                .flex_wrap()
                .text_xs()
                .text_color(muted)
                .child(language.text(if file.staged {
                    Message::ReviewScopeStaged
                } else {
                    Message::ReviewScopeWorktree
                }))
                .child(
                    language.format(Message::ReviewCount, &[("count", &file.comments.to_string())]),
                );
            if closed {
                labels = labels.child(language.text(Message::ReviewRetainedLabel));
            }
            if file.loading {
                labels = labels.child(language.text(Message::DiffLoading));
            } else if file.comments > 0 && !file.ready {
                labels = labels.child(
                    div()
                        .text_color(cx.theme().warning)
                        .child(language.text(Message::ReviewQueueStale)),
                );
            }
            if file.pending {
                labels = labels.child(language.text(Message::ReviewQueueDraft));
            }
            let open = view.clone();
            rows = rows.child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_3()
                    .py_2()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .id(("review-file-path", index))
                                    .truncate()
                                    .text_sm()
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(hint.clone()).build(window, cx)
                                    })
                                    .child(name),
                            )
                            .child(div().truncate().text_xs().text_color(muted).child(file.path))
                            .child(labels),
                    )
                    .child(
                        Button::new(("review-file-open", index))
                            .ghost()
                            .small()
                            .label(language.text(Message::CommonOpen))
                            .disabled(busy)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(QueueEvent::Open(open.clone(), None));
                            })),
                    ),
            );
            for (comment_index, comment) in comments.into_iter().enumerate() {
                let id = comment.id;
                let locate = view.clone();
                let remove = view.clone();
                rows = rows.child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .p_3()
                        .border_1()
                        .border_color(cx.theme().border)
                        .rounded_md()
                        .items_start()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_1()
                                .child(div().text_xs().text_color(muted).child(comment.anchor))
                                .child(div().text_sm().whitespace_normal().child(comment.note)),
                        )
                        .child(
                            Button::new(("review-locate", index * 32 + comment_index))
                                .ghost()
                                .xsmall()
                                .label(language.text(Message::ReviewLocate))
                                .disabled(busy)
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(QueueEvent::Open(locate.clone(), Some(id)))
                                })),
                        )
                        .child(
                            Button::new(("review-delete", index * 32 + comment_index))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Close)
                                .aria_label(language.text(Message::CommonDelete))
                                .disabled(busy)
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.emit(QueueEvent::Remove(remove.clone(), id))
                                })),
                        ),
                );
            }
        }
        let content = if self.show_preview {
            div()
                .id("review-message-preview")
                .w_full()
                .text_sm()
                .whitespace_normal()
                .font_family(cx.theme().mono_font_family.clone())
                .child(self.prepared.as_ref().map(|(_, message)| message.clone()).unwrap_or_else(
                    || {
                        language
                            .text(if busy {
                                Message::ReviewWorking
                            } else {
                                Message::ReviewPreviewHint
                            })
                            .to_owned()
                    },
                ))
                .into_any_element()
        } else {
            rows.into_any_element()
        };
        let content = v_flex()
            .w_full()
            .min_w_0()
            .gap_3()
            .when(self.prepared.is_some() && !ready && !busy, |content| {
                content.child(
                    div()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(cx.theme().warning)
                        .child(language.text(Message::ReviewStale)),
                )
            })
            .when_some(self.issue.as_ref(), |content, issue| {
                content.child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .items_start()
                        .gap_2()
                        .child(
                            div()
                                .w_full()
                                .text_sm()
                                .whitespace_normal()
                                .text_color(cx.theme().warning)
                                .child(issue.clone()),
                        )
                        .child(
                            Button::new("review-queue-retry")
                                .small()
                                .label(language.text(Message::ReviewQueueRetry))
                                .disabled(blocked || busy || comment_count == 0)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.prepare(false, window, cx);
                                })),
                        ),
                )
            })
            .child(content);
        body = body.child(
            div()
                .relative()
                .w_full()
                .flex_1()
                .min_h_0()
                .pr(px(SCROLLBAR_GUTTER))
                .overflow_hidden()
                .child(
                    div()
                        .id("review-queue-scroll")
                        .size_full()
                        .min_w_0()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                        .child(content),
                )
                .child(scrollbar_gutter(gpui_component::scroll::Scrollbar::vertical(&self.scroll))),
        );
        if pending > 0 {
            body = body.child(
                div().flex_shrink_0().text_xs().whitespace_normal().text_color(muted).child(
                    language
                        .format(Message::ReviewQueuePending, &[("count", &pending.to_string())]),
                ),
            );
        }
        if blocked {
            body = body.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .whitespace_normal()
                    .text_color(cx.theme().warning)
                    .child(language.text(Message::ReviewStale)),
            );
        }
        body.child(self.render_actions(comment_count == 0 || blocked, ready, window, cx))
            .into_any_element()
    }
}
