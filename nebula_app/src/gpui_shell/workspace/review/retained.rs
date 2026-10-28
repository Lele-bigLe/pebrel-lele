//! Window-lifetime review text, detached only on actual close, never on tab transfer.

use super::*;
use crate::gpui_shell::code_tab::CodeTabView;

const MAX_FILES: usize = 64;
const MAX_BYTES: usize = 4 * 1024 * 1024;

impl NebulaWorkspace {
    pub(in crate::gpui_shell::workspace) fn retain_review_before_close(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(WorkspaceTab::Code { view, .. }) = self.tabs.get(index) else { return true };
        let bytes: usize = self.review.retained.iter().map(|record| record.draft.bytes()).sum();
        let retained = view.read(cx).retain_review(MAX_BYTES.saturating_sub(bytes), cx);
        let retained = match retained {
            Ok(None) => return true,
            Ok(Some(draft))
                if self.review.retained.len() < MAX_FILES
                    && self.review.next_retained_id < u64::MAX =>
            {
                draft
            },
            _ => {
                let language = crate::gpui_shell::config::ui_language(cx);
                crate::gpui_shell::toast::toast(
                    window,
                    cx,
                    crate::display::ToastKind::Warning,
                    language.text(Message::ReviewRetentionLimit),
                );
                return false;
            },
        };
        self.review.next_retained_id += 1;
        self.review.retained.push(Retained {
            id: self.review.next_retained_id,
            draft: retained,
            project_root: self.tab_meta.get(index).and_then(|meta| meta.project_root.clone()),
        });
        true
    }

    pub(in crate::gpui_shell::workspace) fn restore_retained_review(
        &mut self,
        view: &Entity<CodeTabView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) =
            self.review.retained.iter().position(|record| record.draft.matches(view.read(cx)))
        else {
            return;
        };
        let record = self.review.retained.remove(index);
        view.update(cx, |view, cx| view.restore_review(record.draft, window, cx));
    }

    pub(in crate::gpui_shell::workspace) fn open_retained_review(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.review.retained.iter().position(|record| record.id == id) else {
            return false;
        };
        let retained = &self.review.retained[index].draft;
        let existing = self.tabs.iter().enumerate().find_map(|(index, tab)| match tab {
            WorkspaceTab::Code { view, .. } if retained.matches(view.read(cx)) => {
                Some((index, view.clone()))
            },
            _ => None,
        });
        if let Some((tab_index, view)) = existing {
            if view.read(cx).review_summary(&retained.location, cx).is_some() {
                let language = crate::gpui_shell::config::ui_language(cx);
                crate::gpui_shell::toast::toast(
                    window,
                    cx,
                    crate::display::ToastKind::Warning,
                    language.text(Message::ReviewRestoreConflict),
                );
                return false;
            }
            let record = self.review.retained.remove(index);
            view.update(cx, |view, cx| {
                view.restore_review(record.draft, window, cx);
                view.reload_git_diff(window, cx);
            });
            self.activate_tab(tab_index, window, cx);
        } else {
            if self.workbench_changes(cx) {
                let retained = &self.review.retained[index].draft;
                let project_root = self.review.retained[index].project_root.clone();
                let view = cx.new(|cx| {
                    CodeTabView::new_git_diff(
                        retained.location.clone(),
                        retained.relative_path.clone(),
                        retained.staged,
                        retained.path.clone(),
                        window,
                        cx,
                    )
                });
                // Keep the selected payload until replacement succeeds. The preview restores it.
                self.review.retained.swap(0, index);
                if self.insert_workbench_preview(view.clone(), window, cx) {
                    let opened = self.tabs.iter().any(|tab| matches!(tab, WorkspaceTab::Code { view: current, .. } if current.entity_id() == view.entity_id()));
                    if opened {
                        if let Some(meta) = self.tab_meta.get_mut(self.active) {
                            meta.project_root = project_root;
                        }
                        self.keep_active_in_project();
                        self.sync_side_panel_to_active(true, cx);
                        cx.notify();
                    }
                    return opened;
                }
                self.review.retained.swap(0, index);
            }
            let Retained { draft, project_root, .. } = self.review.retained.remove(index);
            let view = cx.new(|cx| {
                let mut view = CodeTabView::new_git_diff(
                    draft.location.clone(),
                    draft.relative_path.clone(),
                    draft.staged,
                    draft.path.clone(),
                    window,
                    cx,
                );
                view.restore_review(draft, window, cx);
                view
            });
            let subscription = cx.subscribe_in(&view, window, Self::on_code_tab_event);
            self.insert_new_tab(WorkspaceTab::Code { view, _subscription: subscription });
            if let Some(meta) = self.tab_meta.get_mut(self.active) {
                meta.project_root = project_root;
            }
            self.leave_settings(window, cx);
            self.sync_side_panel_to_active(true, cx);
            self.focus_active(window, cx);
        }
        self.keep_active_in_project();
        cx.notify();
        true
    }
}
