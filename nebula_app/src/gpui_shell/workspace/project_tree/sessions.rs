use super::*;

impl NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn render_project_tab_branch(
        &self,
        index: usize,
        tab: gpui::AnyElement,
        first_document: bool,
        label_px: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let row_height = 36.0;
        let mut branch = v_flex().w_full().flex_shrink_0();
        if let WorkspaceTab::Terminal { panes, tree, focused, .. } = &self.tabs[index] {
            let id = self.meta(index).runtime_id.clone();
            let has_children = panes.len() > 1;
            let collapsed = !self.projects.expanded_sessions.contains(&id);
            let selected = self.active == index && (!has_children || collapsed);
            branch = branch.child(
                h_flex()
                    .w_full()
                    .h(px(row_height))
                    .items_center()
                    .rounded_md()
                    .when(selected, |row| row.bg(cx.theme().sidebar_accent))
                    .when(!selected, |row| row.hover(|style| style.bg(cx.theme().list_hover)))
                    .child(div().flex_1().min_w_0().child(tab)),
            );
            if has_children && !collapsed {
                for pane_id in pane_header::pane_order(tree) {
                    let Some(pane) = panes.iter().find(|pane| pane.id == pane_id) else { continue };
                    let session_id = id.clone();
                    let title = self.pane_title(&pane.view, cx, cx.theme().is_dark());
                    let active = self.active == index && *focused == pane_id;
                    let text =
                        pane.custom_name.clone().map(SharedString::from).unwrap_or(title.text);
                    let tooltip = text.clone();
                    let agent = pane.view.read(cx).runtime_chat_agent();
                    let tool = agent
                        .as_ref()
                        .map_or("Shell", |agent| {
                            if agent.kind == "claude" { "Claude" } else { &agent.display_name }
                        })
                        .to_owned();
                    let activity = pane.view.read(cx).sidebar_activity();
                    branch = branch.child(
                        h_flex()
                            .id(("session-pane", pane_id as usize))
                            .h(px(row_height))
                            .w_full()
                            .pl(px(TREE_INDENT + 10.0))
                            .pr(px(10.0))
                            .gap(px(8.0))
                            .items_center()
                            .cursor_pointer()
                            .rounded_md()
                            .text_color(if active {
                                cx.theme().sidebar_accent_foreground
                            } else {
                                cx.theme().muted_foreground
                            })
                            .when(active, |row| row.bg(cx.theme().sidebar_accent))
                            .when(!active, |row| row.hover(|style| style.bg(cx.theme().list_hover)))
                            .tooltip(move |window, cx| {
                                Tooltip::new(tooltip.clone()).build(window, cx)
                            })
                            .child(
                                div()
                                    .w(px(TREE_SLOT))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when_some(title.logo, |slot, logo| {
                                        slot.child(
                                            img(logo).size(px(15.0)).object_fit(ObjectFit::Contain),
                                        )
                                    })
                                    .when_some(title.glyph, |slot, glyph| {
                                        slot.child(
                                            div()
                                                .font_family(
                                                    crate::font_install::REQUIRED_FONT_FAMILY,
                                                )
                                                .text_size(px(label_px))
                                                .child(glyph),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(label_px))
                                    .child(text),
                            )
                            .child(
                                div()
                                    .w(px(44.0))
                                    .flex_shrink_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(tool),
                            )
                            .child(
                                div()
                                    .w(px(((label_px * 1.18).max(15.0) * 1.25 + 10.0).max(32.0)))
                                    .flex_shrink_0()
                                    .flex()
                                    .justify_end()
                                    .children(self.session_status_dot(
                                        activity,
                                        agent.is_some(),
                                        format!("session-pane-status-{pane_id}").into(),
                                        cx,
                                    )),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let Some(index) = this
                                    .tab_meta
                                    .iter()
                                    .position(|meta| meta.runtime_id == session_id)
                                else {
                                    return;
                                };
                                let Some(WorkspaceTab::Terminal { panes, zoomed, .. }) =
                                    this.tabs.get_mut(index)
                                else {
                                    return;
                                };
                                if !panes.iter().any(|pane| pane.id == pane_id) {
                                    return;
                                }
                                let was_zoomed = std::mem::replace(zoomed, false);
                                this.focus_pane(index, pane_id, window, cx);
                                this.activate_tab(index, window, cx);
                                if was_zoomed {
                                    this.mark_structural_resize(index, cx);
                                }
                            })),
                    );
                }
            }
        } else {
            if first_document {
                branch = branch.child(
                    Button::new("project-document-tabs")
                        .ghost()
                        .small()
                        .w_full()
                        .h(px(row_height))
                        .px_0()
                        .mt_4()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .aria_label(language.text(Message::SessionTreeDocuments))
                        .child(section_content(
                            self.projects.documents_collapsed,
                            format!(
                                "{}  {}",
                                language.text(Message::SessionTreeDocuments),
                                self.sidebar_tab_indices()
                                    .iter()
                                    .filter(|&&ix| !self.tabs[ix].is_terminal())
                                    .count()
                            )
                            .into(),
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.projects.documents_collapsed = !this.projects.documents_collapsed;
                            cx.notify();
                        })),
                );
            }
            if !self.projects.documents_collapsed {
                let active = self.active == index;
                branch = branch.child(
                    h_flex()
                        .w_full()
                        .h(px(row_height))
                        .pl(px(TREE_INDENT))
                        .rounded_md()
                        .when(active, |row| row.bg(cx.theme().sidebar_accent))
                        .when(!active, |row| row.hover(|style| style.bg(cx.theme().list_hover)))
                        .child(tab),
                );
            }
        }
        branch.into_any_element()
    }

    pub(in crate::gpui_shell::workspace) fn render_project_session_sidebar(
        &self,
        switcher: gpui::AnyElement,
        items: Vec<gpui::AnyElement>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        v_flex()
            .w(px(self.sidebar_width))
            .h_full()
            .flex_shrink_0()
            .px(px(8.0))
            .pb_2()
            .gap_2()
            .bg(cx.theme().background)
            .child(switcher)
            .child(self.render_workbench_modes(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .pr(px(crate::gpui_shell::widgets::SCROLLBAR_GUTTER))
                    .child(
                        v_flex()
                            .id("project-session-scroll")
                            .size_full()
                            .gap(px(4.0))
                            .overflow_y_scroll()
                            .track_scroll(&self.projects.session_scroll)
                            .children(items),
                    )
                    .child(crate::gpui_shell::widgets::scrollbar_gutter(
                        gpui_component::scroll::Scrollbar::vertical(&self.projects.session_scroll),
                    )),
            )
            .child(self.render_usage_button(cx))
            .into_any_element()
    }
}
