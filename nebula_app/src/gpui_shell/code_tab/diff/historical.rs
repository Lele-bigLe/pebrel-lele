use super::*;

impl CodeTabView {
    pub(in crate::gpui_shell) fn new_turn_diff(
        root: PathBuf,
        path: String,
        patch: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let language = super::super::language_for_path(&path);
        let absolute = root.join(&path);
        let mut this =
            Self::new_diff(GitLocation::Local { root }, path, false, absolute, true, window, cx);
        if patch.len() > super::super::MAX_CODE_BYTES {
            this.notice = Some(
                crate::gpui_shell::config::ui_language(cx).text(Message::DiffTooLarge).to_owned(),
            );
            return this;
        }
        let worker = cx.background_executor().spawn(async move {
            let rows = rows::parse(&patch);
            let comparison = crate::runtime_api::git::patch::Comparison::from_patch(&patch)
                .map(|data| comparison::Prepared::new(data, language));
            (patch, rows, comparison)
        });
        let handle = window.window_handle();
        let diff = this.diff.as_mut().unwrap();
        diff.loading = true;
        diff.task = Some(cx.spawn(async move |this, cx| {
            let (patch, rows, comparison) = worker.await;
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    let Some(diff) = &mut this.diff else { return };
                    diff.loading = false;
                    diff.task = None;
                    diff.rows = std::sync::Arc::new(rows);
                    // No live Git/review mutation token is issued for historical patches.
                    diff.snapshot = None;
                    match comparison {
                        Ok(data) => diff.comparison = Some(std::rc::Rc::new(data)),
                        Err(_) => {
                            diff.raw = true;
                            this.notice = Some(
                                crate::gpui_shell::config::ui_language(cx)
                                    .text(Message::TurnChangesRawOnly)
                                    .to_owned(),
                            );
                        },
                    }
                    this.lines = patch.lines().count();
                    this.input.update(cx, |input, cx| input.set_value(patch, window, cx));
                    diff.comparison_scroll.reset(comparison::row_count(diff));
                    cx.notify();
                });
            });
        }));
        this
    }
}
