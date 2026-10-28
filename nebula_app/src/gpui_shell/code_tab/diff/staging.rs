use super::*;
use crate::runtime_api::git::patch;

impl CodeTabView {
    pub(super) fn render_stage_hunk(&self, cx: &Context<Self>) -> impl IntoElement {
        let diff = self.diff.as_ref().expect("staging requires a diff");
        let language = crate::gpui_shell::config::ui_language(cx);
        let local = matches!(diff.key.location, GitLocation::Local { .. });
        let supported = patch::supports_hunks(&diff.rows);
        let selected =
            patch::selected_hunk(&diff.rows, self.input.read(cx).selected_range()).is_some();
        Button::new("diff-stage-hunk")
            .ghost()
            .small()
            .label(language.text(Message::GitHunksStage))
            .disabled(diff.loading || diff.snapshot.is_none() || !local || !supported || !selected)
            .tooltip(language.text(if !local {
                Message::GitHunksLocalOnly
            } else if !supported {
                Message::GitHunksUnsupported
            } else {
                Message::GitHunksHint
            }))
            .on_click(cx.listener(|this, _, window, cx| this.stage_selected_hunk(window, cx)))
    }

    fn stage_selected_hunk(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(diff) = self.diff.as_mut() else { return };
        if diff.loading || diff.staged || diff.historical {
            return;
        }
        let Some(expected) = diff.snapshot else { return };
        let Some(selection) =
            patch::selected_hunk(&diff.rows, self.input.read(cx).selected_range())
        else {
            return;
        };
        let key = diff.key.clone();
        diff.loading = true;
        self.notice = None;
        let worker = cx.background_executor().spawn(async move {
            crate::runtime_api::git::stage_hunk(
                &key.location,
                key.relative_path,
                expected,
                selection,
            )
        });
        let handle = window.window_handle();
        diff.task = Some(cx.spawn(async move |this, cx| {
            let result = worker.await;
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    let Some(diff) = &mut this.diff else { return };
                    diff.loading = false;
                    diff.task = None;
                    match result {
                        Ok(()) => {
                            cx.emit(CodeTabViewEvent::GitStateInvalidated);
                            this.reload_git_diff(window, cx);
                        },
                        Err(error) => {
                            diff.snapshot = None;
                            cx.emit(CodeTabViewEvent::GitStateInvalidated);
                            this.notice = Some(
                                crate::gpui_shell::config::ui_language(cx)
                                    .format(Message::GitHunksFailed, &[("error", &error.message)]),
                            );
                            cx.notify();
                        },
                    }
                });
            });
        }));
        cx.notify();
    }
}
