//! Project membership belongs to tabs; filtering never owns or suspends a PTY.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gpui_component::menu::PopupMenuItem;

use super::*;
use crate::i18n::Message;
use crate::runtime_api::tabs::TabId;

mod library;

#[derive(Clone, Default, PartialEq, Eq)]
pub(super) enum ProjectFilter {
    #[default]
    All,
    Ungrouped,
    Root(PathBuf),
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn filtered_tabs_keep_identity_across_reordering_duplication_and_global_activation(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::gpui_shell::math_view::register(cx);
            crate::gpui_shell::file_editor::init(cx);
            super::super::init(cx);
            windowing::initialize(cx, crate::runtime_api::RuntimeHub::new());
            cx.set_reduce_motion(true);
        });
        let project_a = tempfile::tempdir().unwrap();
        let project_b = tempfile::tempdir().unwrap();
        let a = project_a.path().to_path_buf();
        let b = project_b.path().to_path_buf();
        let launch = crate::session::LaunchSession::Shell {
            name: "Unavailable test shell".into(),
            program: a.join("missing-project-test-shell.exe").to_string_lossy().into_owned(),
            args: vec![],
        };
        let (_, window) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| {
                let mut workspace = NebulaWorkspace::new(
                    window,
                    None,
                    None,
                    1,
                    crate::runtime_api::RuntimeHub::new(),
                    windowing::WorkspaceStartup::Empty,
                    windowing::WindowRole::Regular,
                    cx,
                );
                workspace.tabs_position = nebula_settings::TabsPositionName::Sidebar;
                for project in
                    [Some(a.clone()), Some(a.clone()), Some(b.clone()), Some(a.clone()), None]
                {
                    workspace.projects.filter =
                        project.clone().map_or(ProjectFilter::Ungrouped, ProjectFilter::Root);
                    workspace.add_terminal_with(launch.clone(), project, None, window, cx);
                }
                workspace.projects.filter = ProjectFilter::Root(a.clone());
                assert_eq!(workspace.sidebar_tab_indices(), vec![0, 1, 3]);
                workspace.active = 2;
                workspace.keep_active_in_project();
                assert_eq!(
                    workspace.active, 3,
                    "closing a filtered tab keeps the next visible tab"
                );
                let moved = workspace.meta(3).runtime_id;
                workspace.move_tab(3, 1, window, cx);
                assert_eq!(workspace.meta(1).runtime_id, moved);
                assert_eq!(workspace.sidebar_tab_indices(), vec![0, 1, 2]);
                workspace.projects.filter = ProjectFilter::All;
                assert_eq!(workspace.new_tab_project(), Some(a.clone()));
                workspace.projects.filter = ProjectFilter::Ungrouped;
                assert_eq!(workspace.new_tab_project(), None);
                workspace.projects.filter = ProjectFilter::Root(a.clone());
                workspace.active = 3;
                workspace.reveal_active_project();
                assert_eq!(workspace.sidebar_tab_indices(), vec![3]);
                workspace.duplicate_tab(3, window, cx);
                assert_eq!(workspace.meta(workspace.active).project_root, Some(b.clone()));
                assert_eq!(
                    workspace.snapshot_session(cx).tabs[workspace.active].project_root,
                    Some(b.clone())
                );
                workspace.tabs_position = nebula_settings::TabsPositionName::Top;
                assert_eq!(
                    workspace.sidebar_tab_indices(),
                    (0..workspace.tabs.len()).collect::<Vec<_>>()
                );
                workspace
            });
            Root::new(workspace, window, cx)
        });
        window.update(|window, cx| window.draw(cx).clear(cx));
    }
}

impl ProjectFilter {
    pub(super) fn matches(&self, root: Option<&PathBuf>) -> bool {
        match self {
            Self::All => true,
            Self::Ungrouped => root.is_none(),
            Self::Root(expected) => {
                root.is_some_and(|root| crate::project_library::same_root(root, expected))
            },
        }
    }
}

#[derive(Default)]
pub(super) struct ProjectState {
    pub(super) filter: ProjectFilter,
    picker: Option<gpui::Task<()>>,
    library: crate::project_library::Library,
    pending: std::collections::VecDeque<crate::project_library::Change>,
    store_task: Option<gpui::Task<()>>,
    store_error: Option<String>,
    pub(super) tree_initialized: bool,
    pub(super) expanded_sessions: std::collections::HashSet<TabId>,
    pub(super) documents_collapsed: bool,
    pub(super) session_scroll: gpui::ScrollHandle,
}

// Use the shortest unique path suffix, preserving equal-named projects without
// making every menu row repeat the full directory tree.
pub(super) fn project_labels(entries: &[(PathBuf, usize)]) -> Vec<String> {
    entries
        .iter()
        .map(|(root, _)| {
            let components: Vec<_> = root.components().collect();
            for start in (0..components.len()).rev() {
                if matches!(components[start], std::path::Component::RootDir) {
                    continue;
                }
                let suffix: PathBuf = components[start..].iter().copied().collect();
                if !entries.iter().any(|(other, _)| other != root && other.ends_with(&suffix)) {
                    return suffix.display().to_string();
                }
            }
            root.display().to_string()
        })
        .collect()
}

pub(super) fn project_menu_item(label: String, full_path: String) -> PopupMenuItem {
    PopupMenuItem::element(move |_, _| {
        let hint = full_path.clone();
        div()
            .id(SharedString::from(format!("project-path-{full_path}")))
            .flex_1()
            .min_w_0()
            .max_w(px(360.0))
            .truncate()
            .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
            .child(label.clone())
    })
}

impl NebulaWorkspace {
    pub(super) fn add_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let ProjectFilter::Root(root) = &self.projects.filter {
            self.add_terminal_at(Some(root.clone()), None, window, cx);
            return;
        }
        // 无明确项目时，沿用现有的启动目录、聚焦 pane 与 WSL 继承合同。
        let (launch, cwd) = match Self::startup_directory() {
            Some(dir) => (shell_launch::configured_local_launch(cx), Some(dir)),
            None => self.new_tab_from_focused(cx),
        };
        self.add_terminal_with(launch, cwd, None, window, cx);
    }

    /// 设置页「启动目录」：非空且确实存在的目录才生效。
    pub(super) fn startup_directory() -> Option<PathBuf> {
        nebula_settings::RuntimeSettings::load().startup_directory_path()
    }

    pub(super) fn new_tab_project(&self) -> Option<PathBuf> {
        match &self.projects.filter {
            ProjectFilter::All => self.tab_meta.get(self.active)?.project_root.clone(),
            ProjectFilter::Ungrouped => None,
            ProjectFilter::Root(root) => Some(root.clone()),
        }
    }

    pub(super) fn sidebar_tab_indices(&self) -> Vec<usize> {
        (0..self.tabs.len())
            .filter(|&ix| {
                self.tabs_position != nebula_settings::TabsPositionName::Sidebar
                    || self
                        .projects
                        .filter
                        .matches(self.tab_meta.get(ix).and_then(|meta| meta.project_root.as_ref()))
            })
            .collect()
    }

    pub(super) fn reveal_active_project(&mut self) {
        let root = self.tab_meta.get(self.active).and_then(|meta| meta.project_root.as_ref());
        if !self.projects.filter.matches(root) {
            self.projects.filter =
                root.cloned().map_or(ProjectFilter::Ungrouped, ProjectFilter::Root);
            self.tabs_scroll = 0;
        }
    }

    pub(super) fn keep_active_in_project(&mut self) {
        if self.tabs_position == nebula_settings::TabsPositionName::Sidebar {
            let indices = self.sidebar_tab_indices();
            if !indices.contains(&self.active)
                && let Some(&ix) =
                    indices.iter().find(|&&ix| ix >= self.active).or_else(|| indices.last())
            {
                self.active = ix;
            }
        }
    }

    pub(super) fn project_entries(&self) -> Vec<(PathBuf, usize)> {
        let mut entries = BTreeMap::new();
        for project in &self.projects.library.projects {
            entries.insert(project.root.clone(), 0);
        }
        for meta in &self.tab_meta {
            if let Some(root) = &meta.project_root {
                let key = entries
                    .keys()
                    .find(|key| crate::project_library::same_root(key, root))
                    .cloned()
                    .unwrap_or_else(|| root.clone());
                *entries.entry(key).or_insert(0) += 1;
            }
        }
        let mut entries: Vec<_> = entries.into_iter().collect();
        entries.sort_by_key(|(root, _)| {
            let project = self.project_metadata(root);
            (
                project.is_none_or(|p| !p.favorite),
                project.map(|p| p.group.clone()).unwrap_or_default(),
                std::cmp::Reverse(project.map_or(0, |p| p.last_opened)),
            )
        });
        entries
    }

    pub(super) fn select_project(
        &mut self,
        filter: ProjectFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(drag) = self.tab_drag.take() {
            windowing::clear_cross_window_drag_target(drag.cross_window_target, cx);
        }
        self.tab_menu = None;
        self.projects.filter = filter;
        self.tabs_scroll = 0;
        let indices = self.sidebar_tab_indices();
        if let Some(&ix) = indices.iter().find(|&&ix| ix == self.active).or_else(|| indices.first())
        {
            self.activate_tab(ix, window, cx);
            self.reveal_active_tab();
            self.focus_active(window, cx);
        } else if let ProjectFilter::Root(root) = &self.projects.filter {
            self.add_terminal_at(Some(root.clone()), None, window, cx);
        }
        if let ProjectFilter::Root(root) = &self.projects.filter {
            self.queue_project_change(crate::project_library::Change::Open(root.clone()), cx);
        }
        cx.notify();
    }

    fn assign_tab_project(
        &mut self,
        tab: &TabId,
        root: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.tab_meta.iter().position(|meta| &meta.runtime_id == tab) else {
            let language = crate::gpui_shell::config::ui_language(cx);
            window.push_notification(
                Notification::error(language.text(Message::ProjectSessionClosed)),
                cx,
            );
            return;
        };
        self.tab_meta[ix].project_root = root.clone();
        if let Some(root) = &root {
            self.queue_project_change(crate::project_library::Change::Open(root.clone()), cx);
        }
        self.projects.filter = root.map_or(ProjectFilter::Ungrouped, ProjectFilter::Root);
        self.tab_menu = None;
        self.activate_tab(ix, window, cx);
        self.reveal_active_tab();
        self.focus_active(window, cx);
        cx.notify();
    }

    fn pick_project(&mut self, tab: Option<TabId>, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects.picker.is_some() {
            return;
        }
        let language = crate::gpui_shell::config::ui_language(cx);
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(language.text(Message::ProjectChooseDirectory).into()),
        });
        self.projects.picker = Some(cx.spawn_in(window, async move |this, cx| {
            let result = match picked.await {
                Ok(result) => result.map_err(|error| error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.projects.picker = None;
                match result {
                    Ok(Some(paths)) => {
                        if let Some(root) = paths.into_iter().next() {
                            if let Some(tab) = tab {
                                this.assign_tab_project(&tab, Some(root), window, cx);
                            } else {
                                this.projects.filter = ProjectFilter::Root(root.clone());
                                this.queue_project_change(
                                    crate::project_library::Change::Open(root.clone()),
                                    cx,
                                );
                                this.add_terminal_at(Some(root), None, window, cx);
                            }
                        }
                    },
                    Ok(None) => {},
                    Err(error) => {
                        let language = crate::gpui_shell::config::ui_language(cx);
                        window.push_notification(
                            Notification::error(
                                language.format(Message::ProjectPickFailed, &[("error", &error)]),
                            ),
                            cx,
                        );
                    },
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn render_project_switcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let (label, hint) = match &self.projects.filter {
            ProjectFilter::All => {
                let label = language.text(Message::ProjectAll).to_owned();
                (label.clone(), label)
            },
            ProjectFilter::Ungrouped => {
                let label = language.text(Message::ProjectUngrouped).to_owned();
                (label.clone(), label)
            },
            ProjectFilter::Root(root) => (
                self.project_metadata(root)
                    .filter(|p| !p.name.is_empty())
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| {
                        root.file_name().unwrap_or(root.as_os_str()).to_string_lossy().into_owned()
                    }),
                root.display().to_string(),
            ),
        };
        let workspace = cx.entity().downgrade();
        let hint = self
            .projects
            .store_error
            .as_ref()
            .map_or(hint.clone(), |error| format!("{hint}\n{error}"));
        let busy = self.projects.picker.is_some();
        let tree_mode = self.project_tree_mode(cx);
        h_flex()
            .w_full()
            .flex_shrink_0()
            .min_w_0()
            .py_1()
            .when(tree_mode, |row| row.h(px(52.0)))
            .child(
                Button::new("workspace-project-picker")
                    .debug_selector(|| "workspace-project-picker".into())
                    .ghost()
                    .small()
                    .flex_1()
                    .min_w_0()
                    .icon(IconName::FolderClosed)
                    .dropdown_caret(true)
                    .aria_label(SharedString::from(hint.clone()))
                    .tooltip(SharedString::from(hint))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_left()
                            .font_semibold()
                            .child(label),
                    )
                    .dropdown_menu(move |mut menu, _, cx| {
                        let Some(owner) = workspace.upgrade() else { return menu };
                        let state = owner.read(cx);
                        let entries = state.project_entries();
                        let current = state.projects.filter.clone();
                        let total = state.tabs.len();
                        let ungrouped = state
                            .tab_meta
                            .iter()
                            .filter(|meta| meta.project_root.is_none())
                            .count();
                        let mut options = vec![
                            (
                                ProjectFilter::All,
                                language.text(Message::ProjectAll).to_owned(),
                                total,
                            ),
                            (
                                ProjectFilter::Ungrouped,
                                language.text(Message::ProjectUngrouped).to_owned(),
                                ungrouped,
                            ),
                        ];
                        options.extend(entries.iter().zip(project_labels(&entries)).map(
                            |((root, count), label)| {
                                (
                                    ProjectFilter::Root(root.clone()),
                                    state.project_display_name(root, label),
                                    *count,
                                )
                            },
                        ));
                        for (filter, name, count) in options {
                            let owner = workspace.clone();
                            let hint = match &filter {
                                ProjectFilter::Root(root) => root.display().to_string(),
                                _ => name.clone(),
                            };
                            menu = menu.item(
                                project_menu_item(
                                    language.format(
                                        Message::ProjectMenuItem,
                                        &[("name", &name), ("count", &count.to_string())],
                                    ),
                                    hint,
                                )
                                .checked(current == filter)
                                .disabled(count == 0 && filter == ProjectFilter::Ungrouped)
                                .on_click(move |_, window, cx| {
                                    if let Some(owner) = owner.upgrade() {
                                        owner.update(cx, |this, cx| {
                                            this.select_project(filter.clone(), window, cx);
                                        });
                                    }
                                }),
                            );
                        }
                        let owner = workspace.clone();
                        let edit_owner = workspace.clone();
                        let recipe_owner = workspace.clone();
                        let shell_owner = workspace.clone();
                        let tasks_owner = workspace.clone();
                        let can_edit = matches!(current, ProjectFilter::Root(_));
                        if let Some(error) = &state.projects.store_error {
                            menu = menu
                                .separator()
                                .item(PopupMenuItem::new(error.clone()).disabled(true));
                        }
                        menu.separator()
                            .item(
                                PopupMenuItem::new(language.text(Message::SessionTreeChooseShell))
                                    .icon(IconName::SquareTerminal)
                                    .on_click(move |_, window, cx| {
                                        if let Some(owner) = shell_owner.upgrade() {
                                            owner.update(cx, |this, cx| {
                                                this.open_shell_palette(window, cx)
                                            });
                                        }
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(language.text(Message::TasksTitle))
                                    .icon(IconName::Bell)
                                    .on_click(move |_, window, cx| {
                                        if let Some(owner) = tasks_owner.upgrade() {
                                            owner.update(cx, |this, cx| {
                                                this.toggle_task_inbox(window, cx)
                                            });
                                        }
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(language.text(Message::RecipeOpenLibrary))
                                    .on_click(move |_, window, cx| {
                                        if let Some(owner) = recipe_owner.upgrade() {
                                            owner.update(cx, |this, cx| {
                                                this.open_layout_recipes(window, cx)
                                            });
                                        }
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(language.text(Message::ProjectEdit))
                                    .disabled(!can_edit || busy)
                                    .on_click(move |_, window, cx| {
                                        if let Some(owner) = edit_owner.upgrade() {
                                            owner.update(cx, |this, cx| {
                                                this.edit_project(window, cx)
                                            });
                                        }
                                    }),
                            )
                            .item(
                                PopupMenuItem::new(language.text(Message::ProjectAdd))
                                    .icon(IconName::FolderOpen)
                                    .disabled(busy)
                                    .on_click(move |_, window, cx| {
                                        if let Some(owner) = owner.upgrade() {
                                            owner.update(cx, |this, cx| {
                                                this.pick_project(None, window, cx)
                                            });
                                        }
                                    }),
                            )
                            .max_w(px(480.0))
                            .scrollable(true)
                    }),
            )
            .child(self.render_history_button(cx))
            .when(!tree_mode, |row| row.child(self.render_task_button(cx)))
    }

    pub(super) fn project_assignment_item(
        &self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PopupMenuItem {
        let language = crate::gpui_shell::config::ui_language(cx);
        let meta = self.meta(ix);
        let entries = self.project_entries();
        let workspace = cx.entity().downgrade();
        let busy = self.projects.picker.is_some();
        let labels: Vec<_> = entries
            .iter()
            .zip(project_labels(&entries))
            .map(|((root, _), label)| self.project_display_name(root, label))
            .collect();
        let menu = PopupMenu::build(window, cx, move |mut menu, _, _| {
            let mut options = vec![(None, language.text(Message::ProjectUngrouped).to_owned())];
            options.extend(entries.iter().zip(labels).map(|((root, _), label)| {
                let root = root.clone();
                (Some(root), label)
            }));
            for (root, label) in options {
                let owner = workspace.clone();
                let tab = meta.runtime_id.clone();
                menu = menu.item(
                    project_menu_item(
                        label.clone(),
                        root.as_ref().map_or(label, |root| root.display().to_string()),
                    )
                    .checked(meta.project_root == root)
                    .on_click(move |_, window, cx| {
                        if let Some(owner) = owner.upgrade() {
                            owner.update(cx, |this, cx| {
                                this.assign_tab_project(&tab, root.clone(), window, cx);
                            });
                        }
                    }),
                );
            }
            menu.separator()
                .item(
                    PopupMenuItem::new(language.text(Message::ProjectChooseDirectory))
                        .icon(IconName::FolderOpen)
                        .disabled(busy)
                        .on_click(move |_, window, cx| {
                            if let Some(owner) = workspace.upgrade() {
                                owner.update(cx, |this, cx| {
                                    this.pick_project(Some(meta.runtime_id.clone()), window, cx);
                                });
                            }
                        }),
                )
                .max_w(px(480.0))
                .scrollable(true)
        });
        PopupMenuItem::submenu(language.text(Message::ProjectMoveSession), menu)
            .icon(IconName::FolderClosed)
    }
}
