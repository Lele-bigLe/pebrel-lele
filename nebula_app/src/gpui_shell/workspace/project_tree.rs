//! Project navigation groups live sessions and their panes, separately from document tabs.
use super::*;
use crate::i18n::Message;
use projects::ProjectFilter;
mod overview;
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
}
