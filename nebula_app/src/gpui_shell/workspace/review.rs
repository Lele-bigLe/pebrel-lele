//! Repository review text and validation; the dialog owns its pending preview/send task.
use super::*;
use crate::display::side_panel::GitLocation;
use crate::gpui_shell::code_tab::Draft;
use crate::i18n::Message;

mod panel;
mod retained;

#[derive(Default)]
pub(super) struct ReviewBatch {
    pub(super) retained: Vec<Retained>,
    next_retained_id: u64,
}

pub(super) struct Retained {
    pub(super) id: u64,
    pub(super) draft: crate::gpui_shell::code_tab::RetainedReview,
    project_root: Option<std::path::PathBuf>,
}

impl NebulaWorkspace {
    pub(super) fn review_comment_count(&self, location: &GitLocation, cx: &App) -> usize {
        let open: usize = self
            .tabs
            .iter()
            .filter_map(|tab| match tab {
                WorkspaceTab::Code { view, .. } => view.read(cx).review_summary(location, cx),
                _ => None,
            })
            .map(|file| file.comments)
            .sum();
        open + self
            .review
            .retained
            .iter()
            .filter(|record| &record.draft.location == location)
            .map(|record| record.draft.comments().len())
            .sum::<usize>()
    }

    pub(super) fn review_drafts(
        &self,
        location: &GitLocation,
        cx: &App,
    ) -> Result<Vec<Draft>, String> {
        let language = crate::gpui_shell::config::ui_language(cx);
        let bytes: usize = self
            .tabs
            .iter()
            .filter_map(|tab| match tab {
                WorkspaceTab::Code { view, .. }
                    if view.read(cx).review_location() == Some(location) =>
                {
                    Some(view.read(cx).review_feedback_bytes())
                },
                _ => None,
            })
            .sum::<usize>()
            + self
                .review
                .retained
                .iter()
                .filter(|record| &record.draft.location == location)
                .map(|record| record.draft.feedback_bytes())
                .sum::<usize>();
        if bytes > 64 * 1024 {
            return Err(language.text(Message::ReviewLimit).to_owned());
        }
        let mut drafts = Vec::new();
        for tab in &self.tabs {
            let WorkspaceTab::Code { view, .. } = tab else { continue };
            let model = view.read(cx);
            if !model.has_review_in(location) {
                continue;
            }
            let draft = model.review_draft().ok_or_else(|| {
                language.format(
                    Message::ReviewFileStale,
                    &[("path", &model.review_path().unwrap_or_else(|| model.title.clone()))],
                )
            })?;
            drafts.push(draft);
        }
        for record in self.review.retained.iter().filter(|record| {
            &record.draft.location == location && !record.draft.comments().is_empty()
        }) {
            drafts.push(record.draft.draft().ok_or_else(|| {
                language.format(Message::ReviewFileStale, &[("path", &record.draft.relative_path)])
            })?);
        }
        drafts.sort_by(|a, b| a.path().cmp(b.path()).then_with(|| a.staged().cmp(&b.staged())));
        Ok(drafts)
    }
}

fn validate(
    drafts: &[Draft],
    language: crate::display::UiLanguage,
) -> Result<String, crate::runtime_api::ApiError> {
    let files = drafts.iter().map(Draft::path).collect::<std::collections::HashSet<_>>().len();
    let comments: usize = drafts.iter().map(Draft::comment_count).sum();
    let mut message = language.format(
        Message::ReviewBatchSummary,
        &[("files", &files.to_string()), ("comments", &comments.to_string())],
    );
    message.push_str(language.text(Message::ReviewFeedbackHeader));
    for draft in drafts {
        draft.append_to(&mut message, language);
        if message.len() > 64 * 1024 {
            return Err(crate::runtime_api::ApiError::new(
                "review_limit",
                language.text(Message::ReviewLimit),
            ));
        }
    }
    for draft in drafts {
        if let Err(mut error) = draft.validate() {
            if error.code == "review_stale" {
                error.message =
                    language.format(Message::ReviewFileStale, &[("path", draft.path())]);
            }
            return Err(error);
        }
    }
    crate::runtime_api::validate_chat_message(&message)?;
    Ok(message)
}
