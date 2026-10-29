use super::*;
use crate::project_library::{Change, Library, Project};

impl NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn load_project_library(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let roots = self.tab_meta.iter().filter_map(|meta| meta.project_root.clone()).collect();
        self.queue_project_change(Change::Remember(roots), cx);
    }

    pub(in crate::gpui_shell::workspace) fn accept_project_library(
        &mut self,
        library: Library,
        cx: &mut Context<Self>,
    ) {
        if library.revision >= self.projects.library.revision {
            self.projects.library = library;
            cx.notify();
        }
    }

    pub(super) fn project_metadata(&self, root: &Path) -> Option<&Project> {
        self.projects
            .library
            .projects
            .iter()
            .find(|p| crate::project_library::same_root(&p.root, root))
    }

    pub(in crate::gpui_shell::workspace) fn project_display_name(
        &self,
        root: &Path,
        fallback: String,
    ) -> String {
        let Some(project) = self.project_metadata(root) else { return fallback };
        let name = if project.name.is_empty() { fallback } else { project.name.clone() };
        let name =
            if project.group.is_empty() { name } else { format!("{} / {name}", project.group) };
        name
    }

    pub(in crate::gpui_shell::workspace) fn queue_project_change(
        &mut self,
        change: Change,
        cx: &mut Context<Self>,
    ) {
        self.projects.pending.push_back(change);
        self.run_project_change(cx);
    }

    fn run_project_change(&mut self, cx: &mut Context<Self>) {
        if self.projects.store_task.is_some() {
            return;
        }
        let Some(change) = self.projects.pending.pop_front() else { return };
        let work = cx.background_executor().spawn(async move {
            crate::project_library::update(change).map_err(|error| error.to_string())
        });
        self.projects.store_task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let notice = this
                .update(cx, |this, cx| {
                    let error = result.as_ref().err().cloned();
                    this.projects.store_task = None;
                    match result {
                        Ok(library) => {
                            this.projects.store_error = None;
                            this.accept_project_library(library.clone(), cx);
                            windowing::publish_project_library(this.runtime_window_id, library, cx);
                        },
                        Err(error) => this.projects.store_error = Some(error),
                    }
                    this.run_project_change(cx);
                    cx.notify();
                    (this.window_handle, error)
                })
                .ok();
            if let Some((handle, Some(error))) = notice {
                let _ = handle.update(cx, |_, window, cx| {
                    let language = crate::gpui_shell::config::ui_language(cx);
                    window.push_notification(
                        Notification::error(
                            language.format(Message::ProjectSaveFailed, &[("error", &error)]),
                        ),
                        cx,
                    );
                });
            }
        }));
    }

    pub(super) fn edit_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ProjectFilter::Root(root) = &self.projects.filter else { return };
        let project = self.project_metadata(root).cloned().unwrap_or_else(|| Project {
            root: root.clone(),
            name: String::new(),
            group: String::new(),
            favorite: false,
            last_opened: 0,
        });
        let language = crate::gpui_shell::config::ui_language(cx);
        let name = cx
            .new(|cx| InputState::new(window, cx).placeholder(language.text(Message::ProjectName)));
        let group = cx.new(|cx| {
            InputState::new(window, cx).placeholder(language.text(Message::ProjectGroup))
        });
        name.update(cx, |input, cx| input.set_value(project.name.clone(), window, cx));
        group.update(cx, |input, cx| input.set_value(project.group.clone(), window, cx));
        let owner = cx.entity().downgrade();
        let focus = name.clone();
        window.open_dialog(cx, move |dialog, window, cx| {
            let name = name.clone();
            let group = group.clone();
            let saved = project.clone();
            let owner = owner.clone();
            center_modal_dialog(dialog, window, 330.0)
                .title(language.text(Message::ProjectEdit))
                .child(div().min_w_0().truncate().child(project.root.display().to_string()))
                .child(Input::new(&name).w_full())
                .child(Input::new(&group).w_full())
                .footer(
                    DialogFooter::new()
                        .child(div().flex_1())
                        .child(
                            DialogClose::new().child(
                                Button::new("project-edit-cancel")
                                    .label(language.text(Message::CommonCancel)),
                            ),
                        )
                        .child(
                            DialogAction::new().child(
                                Button::new("project-edit-save")
                                    .primary()
                                    .label(language.text(Message::CommonSave)),
                            ),
                        ),
                )
                .on_ok(move |_, _, cx| {
                    let mut project = saved.clone();
                    project.name = name.read(cx).value().to_string();
                    project.group = group.read(cx).value().to_string();
                    if let Some(owner) = owner.upgrade() {
                        owner.update(cx, |this, cx| {
                            this.queue_project_change(Change::Edit(project), cx)
                        });
                    }
                    true
                })
        });
        focus.update(cx, |input, cx| input.focus(window, cx));
    }
}
