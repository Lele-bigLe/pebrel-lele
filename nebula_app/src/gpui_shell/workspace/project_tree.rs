//! Project navigation groups live sessions and their panes, separately from document tabs.
use super::*;
use crate::i18n::Message;
use projects::ProjectFilter;
mod sessions;

pub(super) const TREE_INDENT: f32 = 16.0;
pub(super) const TREE_SLOT: f32 = 20.0;
pub(super) const TREE_GAP: f32 = 4.0;

fn section_content(collapsed: bool, label: SharedString) -> gpui::AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .gap(px(TREE_GAP))
        .child(
            div().w(px(TREE_SLOT)).flex_shrink_0().flex().justify_center().child(
                Icon::new(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown })
                    .xsmall(),
            ),
        )
        .child(div().flex_1().min_w_0().truncate().child(label))
        .into_any_element()
}

fn overview_content(label: SharedString, count: usize, cx: &App) -> gpui::AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(Icon::new(IconName::ChevronRight).xsmall())
        .child(Icon::new(IconName::Folder).small())
        .child(div().flex_1().min_w_0().truncate().text_sm().text_left().child(label))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(count.to_string()))
        .into_any_element()
}

impl NebulaWorkspace {
    pub(super) fn project_tree_mode(&self, cx: &App) -> bool {
        self.tabs_position == nebula_settings::TabsPositionName::Sidebar
            && !self.settings_open
            && !self.sidebar_collapsed
            && !self.reader_focus_active(cx)
            && matches!(self.projects.filter, ProjectFilter::Root(_))
            && self.side_panel_follow(cx).1.is_none()
            && self
                .tabs
                .get(self.active)
                .and_then(WorkspaceTab::focused_view)
                .is_none_or(|view| view.read(cx).ssh_destination.is_none())
    }

    pub(super) fn sync_project_explorer(&mut self, cx: &mut Context<Self>) {
        if !self.projects.tree_initialized
            && !self.settings_open
            && self.tabs_position == nebula_settings::TabsPositionName::Sidebar
        {
            self.projects.tree_initialized = true;
            if let Some(root) = self.new_tab_project() {
                self.projects.filter = ProjectFilter::Root(root);
            }
        }
        if self.side_panel.set_files_visible(false) {
            self.file_tree_menu = None;
        }
        self.ensure_side_panel_polling(cx);
    }

    pub(super) fn render_projects_overview(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let entries = self.project_entries();
        let ungrouped = self.tab_meta.iter().filter(|meta| meta.project_root.is_none()).count();
        let labels = projects::project_labels(&entries);
        let rows: Arc<Vec<_>> = Arc::new(
            entries
                .into_iter()
                .zip(labels)
                .map(|((root, count), fallback)| {
                    let label = self.project_display_name(&root, fallback);
                    (root, count, label)
                })
                .collect(),
        );
        v_flex()
            .w(px(self.sidebar_width))
            .h_full()
            .flex_shrink_0()
            .px_2()
            .pb_2()
            .gap_1()
            .bg(cx.theme().background)
            .child(self.render_project_switcher(cx))
            .child(
                div()
                    .px_2()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(language.text(Message::ProjectTreeChoose)),
            )
            .child(
                gpui::uniform_list(
                    "project-tree-projects",
                    rows.len(),
                    cx.processor(move |_, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|index| rows.get(index))
                            .map(|(root, count, label)| {
                                let root = root.clone();
                                let hint = root.display().to_string();
                                h_flex()
                                    .id(SharedString::from(format!(
                                        "project-node-{}",
                                        root.display()
                                    )))
                                    .h(px(34.0))
                                    .w_full()
                                    .min_w_0()
                                    .gap_1()
                                    .px_1()
                                    .cursor_pointer()
                                    .hover(|row| row.bg(cx.theme().list_hover))
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(hint.clone()).build(window, cx)
                                    })
                                    .child(overview_content(label.clone().into(), *count, cx))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_project(
                                            ProjectFilter::Root(root.clone()),
                                            window,
                                            cx,
                                        )
                                    }))
                                    .into_any_element()
                            })
                            .collect()
                    }),
                )
                .w_full()
                .flex_1()
                .min_h_0(),
            )
            .child(
                Button::new("project-tree-ungrouped")
                    .ghost()
                    .small()
                    .w_full()
                    .h(px(34.0))
                    .px_1()
                    .child(overview_content(
                        language.text(Message::ProjectUngrouped).into(),
                        ungrouped,
                        cx,
                    ))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.select_project(ProjectFilter::Ungrouped, window, cx)
                    })),
            )
            .child(self.render_usage_button(cx))
            .into_any_element()
    }
}
