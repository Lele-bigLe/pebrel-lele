//! A virtualized projection of existing projects, tabs and panes; no new sessions.
use super::*;
use crate::gpui_shell::widgets::{SCROLLBAR_GUTTER, scrollbar_gutter};
use crate::runtime_api::tabs::TabId;

enum Row {
    Project { root: Option<std::path::PathBuf>, label: String, count: usize },
    Session { tab: TabId, pane: Option<u64> },
    Documents { root: Option<std::path::PathBuf>, count: usize },
}

impl NebulaWorkspace {
    fn overview_rows(&self, cx: &App) -> Vec<Row> {
        let entries = self.project_overview_entries();
        let labels = projects::project_labels(&entries);
        let mut groups: Vec<_> = entries
            .iter()
            .zip(labels)
            .map(|((root, _), label)| (Some(root.clone()), self.project_display_name(root, label)))
            .collect();
        if self.tab_meta.iter().any(|meta| meta.project_root.is_none()) {
            groups.push((
                None,
                crate::gpui_shell::config::ui_language(cx)
                    .text(Message::ProjectUngrouped)
                    .to_owned(),
            ));
        }
        let mut rows = Vec::new();
        for (root, label) in groups {
            let filter = root.clone().map_or(ProjectFilter::Ungrouped, ProjectFilter::Root);
            let indices: Vec<_> = self
                .tab_meta
                .iter()
                .enumerate()
                .filter(|(_, meta)| filter.matches(meta.project_root.as_ref()))
                .map(|(index, _)| index)
                .collect();
            let count = indices.iter().filter(|&&index| self.tabs[index].is_terminal()).count();
            rows.push(Row::Project { root: root.clone(), label, count });
            if self.projects.collapsed_projects.contains(&root) {
                continue;
            }
            for &index in indices.iter().filter(|&&index| self.tabs[index].is_terminal()) {
                let tab = self.meta(index).runtime_id;
                rows.push(Row::Session { tab: tab.clone(), pane: None });
                if self.projects.expanded_sessions.contains(&tab)
                    && let WorkspaceTab::Terminal { tree, panes, .. } = &self.tabs[index]
                    && panes.len() > 1
                {
                    for pane in pane_header::pane_order(tree) {
                        rows.push(Row::Session { tab: tab.clone(), pane: Some(pane) });
                    }
                }
            }
            let documents: Vec<_> =
                indices.into_iter().filter(|&index| !self.tabs[index].is_terminal()).collect();
            if !documents.is_empty() {
                rows.push(Row::Documents { root: root.clone(), count: documents.len() });
                if self.projects.expanded_project_documents.contains(&root) {
                    rows.extend(documents.into_iter().map(|index| Row::Session {
                        tab: self.meta(index).runtime_id,
                        pane: None,
                    }));
                }
            }
        }
        rows
    }

    pub(in crate::gpui_shell::workspace) fn render_projects_overview(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let focus = self.tab_meta.get(self.active).map(|meta| {
            let pane = match &self.tabs[self.active] {
                WorkspaceTab::Terminal { focused, .. } => Some(*focused),
                _ => None,
            };
            (meta.runtime_id.clone(), pane)
        });
        let reveal = focus != self.projects.overview_focus;
        if reveal {
            self.projects.overview_focus = focus.clone();
            if let Some(meta) = self.tab_meta.get(self.active) {
                self.projects.collapsed_projects.remove(&meta.project_root);
                match &self.tabs[self.active] {
                    WorkspaceTab::Terminal { panes, .. } if panes.len() > 1 => {
                        self.projects.expanded_sessions.insert(meta.runtime_id.clone());
                    },
                    tab if !tab.is_terminal() => {
                        self.projects.expanded_project_documents.insert(meta.project_root.clone());
                    },
                    _ => {},
                }
            }
        }
        let rows = Arc::new(self.overview_rows(cx));
        if reveal && let Some((tab, pane)) = focus {
            let target = rows.iter().position(|row| matches!(row, Row::Session { tab: id, pane: id_pane } if id == &tab && id_pane == &pane))
                .or_else(|| rows.iter().position(|row| matches!(row, Row::Session { tab: id, pane: None } if id == &tab)));
            if let Some(target) = target {
                self.projects.overview_scroll.scroll_to_item(target, gpui::ScrollStrategy::Nearest);
            }
        }
        v_flex()
            .w(px(self.sidebar_width))
            .h_full()
            .flex_shrink_0()
            .px_2()
            .pb_2()
            .gap_2()
            .bg(cx.theme().background)
            .child(self.render_project_switcher(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .pr(px(SCROLLBAR_GUTTER))
                    .child(
                        gpui::uniform_list(
                            "project-tree-projects",
                            rows.len(),
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .filter_map(|index| rows.get(index))
                                    .map(|row| this.render_overview_row(row, cx))
                                    .collect()
                            }),
                        )
                        .track_scroll(&self.projects.overview_scroll)
                        .size_full(),
                    )
                    .child(scrollbar_gutter(gpui_component::scroll::Scrollbar::vertical(
                        &self.projects.overview_scroll,
                    ))),
            )
            .child(self.render_usage_button(cx))
            .into_any_element()
    }

    fn render_overview_row(&self, row: &Row, cx: &mut Context<Self>) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        match row {
            Row::Project { root, label, count } => {
                let root = root.clone();
                let collapse_root = root.clone();
                let new_root = root.clone();
                let active = self.tab_meta.get(self.active).is_some_and(|meta| {
                    match (meta.project_root.as_ref(), root.as_ref()) {
                        (Some(current), Some(root)) => {
                            crate::project_library::same_root(current, root)
                        },
                        (None, None) => true,
                        _ => false,
                    }
                });
                let collapsed = self.projects.collapsed_projects.contains(&root);
                let hint =
                    root.as_ref().map_or_else(|| label.clone(), |root| root.display().to_string());
                h_flex()
                    .h(px(36.0))
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new(SharedString::from(format!("overview-collapse-{root:?}")))
                            .ghost()
                            .xsmall()
                            .w(px(TREE_SLOT))
                            .p_0()
                            .icon(if collapsed {
                                IconName::ChevronRight
                            } else {
                                IconName::ChevronDown
                            })
                            .aria_label(label.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.projects.collapsed_projects.remove(&collapse_root) {
                                    this.projects.collapsed_projects.insert(collapse_root.clone());
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("overview-project-{root:?}")))
                            .ghost()
                            .small()
                            .flex_1()
                            .min_w_0()
                            .px_0()
                            .tooltip(hint)
                            .child(
                                h_flex()
                                    .w_full()
                                    .gap(px(TREE_GAP))
                                    .items_center()
                                    .child(
                                        div()
                                            .w(px(TREE_SLOT))
                                            .flex_shrink_0()
                                            .flex()
                                            .justify_center()
                                            .child(Icon::new(IconName::Folder).small()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_left()
                                            .font_semibold()
                                            .text_color(if active {
                                                cx.theme().foreground
                                            } else {
                                                cx.theme().muted_foreground
                                            })
                                            .child(label.clone()),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_project(
                                    root.clone()
                                        .map_or(ProjectFilter::Ungrouped, ProjectFilter::Root),
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if *count == 0 { String::new() } else { count.to_string() }),
                    )
                    .when_some(new_root, |row, root| {
                        row.child(
                            Button::new(SharedString::from(format!("overview-new-{root:?}")))
                                .ghost()
                                .small()
                                .icon(IconName::Plus)
                                .tooltip(language.text(Message::WorkbenchNewTerminal))
                                .aria_label(language.text(Message::WorkbenchNewTerminal))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.add_terminal_at(Some(root.clone()), None, window, cx);
                                })),
                        )
                    })
                    .into_any_element()
            },
            Row::Documents { root, count } => {
                let root = root.clone();
                Button::new(SharedString::from(format!("overview-docs-{root:?}")))
                    .ghost()
                    .small()
                    .h(px(36.0))
                    .w_full()
                    .pl(px(TREE_INDENT + TREE_SLOT + TREE_GAP))
                    .icon(if self.projects.expanded_project_documents.contains(&root) {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .label(format!("{}  {count}", language.text(Message::SessionTreeDocuments)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.projects.expanded_project_documents.remove(&root) {
                            this.projects.expanded_project_documents.insert(root.clone());
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            },
            Row::Session { tab, pane } => self.render_overview_session(tab, *pane, cx),
        }
    }

    fn render_overview_session(
        &self,
        id: &TabId,
        pane_id: Option<u64>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(index) = self.tab_meta.iter().position(|meta| &meta.runtime_id == id) else {
            return div().into_any_element();
        };
        let tab = &self.tabs[index];
        let presentation = self.tab_presentation(index, cx, cx.theme().is_dark());
        let view = match (pane_id, tab) {
            (Some(id), WorkspaceTab::Terminal { panes, .. }) => {
                panes.iter().find(|pane| pane.id == id).map(|pane| &pane.view)
            },
            _ => tab.focused_view(),
        };
        let agent = view.and_then(|view| view.read(cx).session_agent());
        let tool = agent
            .as_ref()
            .map(|agent| {
                if agent.source == "claude" {
                    "Claude".to_owned()
                } else {
                    crate::ai_agents::AgentKind::parse(&agent.source)
                        .map_or_else(|| agent.source.clone(), |kind| kind.display_name().to_owned())
                }
            })
            .or_else(|| tab.is_terminal().then(|| "Shell".to_owned()));
        let pane_title = pane_id.and_then(|id| match tab {
            WorkspaceTab::Terminal { panes, .. } => panes
                .iter()
                .find(|pane| pane.id == id)
                .map(|pane| self.pane_title(&pane.view, cx, cx.theme().is_dark())),
            _ => None,
        });
        let logo = if pane_id.is_some() {
            pane_title.as_ref().and_then(|title| title.logo.clone())
        } else {
            presentation.logo_image
        };
        let glyph = if pane_id.is_some() {
            pane_title.as_ref().and_then(|title| title.glyph)
        } else {
            presentation.program_glyph
        };
        let title = if let Some(pane) = pane_id {
            match tab {
                WorkspaceTab::Terminal { panes, .. } => panes
                    .iter()
                    .find(|entry| entry.id == pane)
                    .map(|entry| {
                        entry.custom_name.clone().map(SharedString::from).unwrap_or_else(|| {
                            self.pane_title(&entry.view, cx, cx.theme().is_dark()).text
                        })
                    })
                    .unwrap_or_else(|| presentation.title.clone()),
                _ => presentation.title.clone(),
            }
        } else {
            presentation.title.clone()
        };
        let focused = matches!(tab, WorkspaceTab::Terminal { focused, .. } if pane_id.is_none_or(|pane| pane == *focused));
        let expanded = self.projects.expanded_sessions.contains(id);
        let selected = index == self.active
            && if pane_id.is_some() { focused } else { !expanded || presentation.pane_count <= 1 };
        let key = format!("overview-session-{}-{pane_id:?}", id.0);
        let activate_id = id.clone();
        let menu_id = id.clone();
        let more_id = id.clone();
        let rename_id = id.clone();
        let close_id = id.clone();
        let expand_id = id.clone();
        let hint = presentation.tooltip.clone().unwrap_or_else(|| title.clone());
        let activity = view.map_or(presentation.activity, |view| view.read(cx).sidebar_activity());
        h_flex()
            .id(SharedString::from(key.clone()))
            .h(px(36.0))
            .w_full()
            .min_w_0()
            .pl(px(TREE_SLOT
                + TREE_GAP
                + TREE_INDENT * if pane_id.is_some() || !tab.is_terminal() { 2.0 } else { 1.0 }))
            .pr_1()
            .gap_1()
            .items_center()
            .rounded_md()
            .cursor_pointer()
            .hover(|row| row.bg(cx.theme().list_hover))
            .when(selected, |row| row.bg(cx.theme().sidebar_accent))
            .when(selected, |row| row.text_color(cx.theme().sidebar_accent_foreground))
            .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
            .child(
                div()
                    .w(px(TREE_SLOT))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(pane_id.is_none() && presentation.pane_count > 1, |slot| {
                        slot.child(
                            Button::new(SharedString::from(format!("{key}-expand")))
                                .ghost()
                                .xsmall()
                                .w(px(TREE_SLOT))
                                .p_0()
                                .icon(if expanded {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if !this.projects.expanded_sessions.remove(&expand_id) {
                                        this.projects.expanded_sessions.insert(expand_id.clone());
                                    }
                                    cx.notify();
                                })),
                        )
                    })
                    .when(pane_id.is_some() || presentation.pane_count <= 1, |slot| {
                        if let Some(logo) = logo {
                            slot.child(img(logo).size(px(16.0)).object_fit(ObjectFit::Contain))
                        } else if let Some(glyph) = glyph {
                            slot.child(
                                div()
                                    .font_family(crate::font_install::REQUIRED_FONT_FAMILY)
                                    .child(glyph),
                            )
                        } else {
                            slot.child(Icon::new(IconName::SquareTerminal).small())
                        }
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .when_some(
                        presentation.renaming.filter(|_| pane_id.is_none()),
                        |slot, input| slot.child(Input::new(&input)),
                    )
                    .when(
                        self.tab_rename.as_ref().is_none_or(|rename| rename.ix != index)
                            || pane_id.is_some(),
                        |slot| slot.truncate().child(title),
                    ),
            )
            .when_some(tool, |row, tool| {
                row.child(
                    div()
                        .w(px(56.0))
                        .flex_shrink_0()
                        .truncate()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tool),
                )
            })
            .child(div().w(px(12.0)).flex_shrink_0().flex().justify_center().children(
                self.session_status_dot(
                    activity,
                    agent.is_some(),
                    SharedString::from(format!("{key}-status")),
                    cx,
                ),
            ))
            .child(
                Button::new(SharedString::from(format!("{key}-more")))
                    .ghost()
                    .small()
                    .icon(IconName::Ellipsis)
                    .tooltip(crate::gpui_shell::config::ui_language(cx).text(Message::ChromeMore))
                    .aria_label(
                        crate::gpui_shell::config::ui_language(cx).text(Message::ChromeMore),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        if let Some(index) =
                            this.tab_meta.iter().position(|meta| meta.runtime_id == more_id)
                        {
                            this.open_tab_context_menu(index, window.mouse_position(), window, cx);
                        }
                    })),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                let Some(index) =
                    this.tab_meta.iter().position(|meta| meta.runtime_id == activate_id)
                else {
                    return;
                };
                if let Some(pane) = pane_id {
                    this.reveal_session_pane(index, pane, window, cx);
                } else {
                    this.activate_tab(index, window, cx);
                    this.focus_active(window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if let Some(index) =
                        this.tab_meta.iter().position(|meta| meta.runtime_id == menu_id)
                    {
                        this.open_tab_context_menu(index, event.position, window, cx);
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    if let Some(index) =
                        this.tab_meta.iter().position(|meta| meta.runtime_id == close_id)
                    {
                        this.request_close_tab(index, window, cx);
                    }
                }),
            )
            .on_double_click(cx.listener(move |this, _, window, cx| {
                if pane_id.is_none()
                    && let Some(index) =
                        this.tab_meta.iter().position(|meta| meta.runtime_id == rename_id)
                {
                    this.begin_rename(index, window, cx);
                }
            }))
            .when_some(
                self.cross_window_drag_payload(index, cx).filter(|_| pane_id.is_none()),
                |row, payload| {
                    row.on_drag(payload, |payload, _, _, cx| {
                        Self::cross_window_drag_preview(payload, cx)
                    })
                },
            )
            .into_any_element()
    }
}
