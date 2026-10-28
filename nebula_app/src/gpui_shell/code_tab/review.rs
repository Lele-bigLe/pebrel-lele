//! Review drafts are anchored to the exact displayed patch, never to a mutable row index.

use super::*;
use crate::i18n::Message;
use gpui::{Focusable as _, InteractiveElement as _};
use sha2::{Digest as _, Sha256};

const MAX_REVIEW_BYTES: usize = 64 * 1024;

pub(super) fn digest(text: &str) -> [u8; 32] {
    let mut hash = [0; 32];
    hash.copy_from_slice(&Sha256::digest(text.as_bytes()));
    hash
}

#[derive(Clone)]
pub(in crate::gpui_shell) struct Comment {
    pub(in crate::gpui_shell) id: u64,
    pub(in crate::gpui_shell) anchor: String,
    excerpt: String,
    pub(in crate::gpui_shell) note: String,
    range: std::ops::Range<usize>,
}

pub(super) struct ReviewState {
    pub(super) open: bool,
    pub(super) pending_locate: Option<u64>,
    input: Entity<InputState>,
    _subscription: gpui::Subscription,
    snapshot: Option<[u8; 32]>,
    comments: Vec<Comment>,
    next_id: u64,
    copy: Entity<crate::gpui_shell::copy_feedback::CopyFeedback>,
    _copy_subscription: gpui::Subscription,
}

impl ReviewState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<CodeTabView>) -> Self {
        let language = crate::gpui_shell::config::ui_language(cx);
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(2)
                .soft_wrap(true)
                .placeholder(language.text(Message::ReviewPlaceholder))
        });
        let subscription = cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify());
        let copy = cx.new(|_| crate::gpui_shell::copy_feedback::CopyFeedback::new());
        let copy_subscription = cx.observe(&copy, |_, _, cx| cx.notify());
        Self {
            open: false,
            pending_locate: None,
            input,
            _subscription: subscription,
            snapshot: None,
            comments: vec![],
            next_id: 0,
            copy,
            _copy_subscription: copy_subscription,
        }
    }

    fn feedback(&self) -> String {
        feedback(&self.comments)
    }
}

fn feedback_bytes(comments: &[Comment]) -> usize {
    comments
        .iter()
        .map(|comment| comment.note.len() + comment.excerpt.len() * 2 + comment.anchor.len() + 128)
        .sum()
}

fn feedback(comments: &[Comment]) -> String {
    let mut text = String::new();
    for comment in comments {
        text.push_str(&format!("\n### {}\n", comment.anchor));
        for line in comment.excerpt.lines() {
            text.push_str(&format!("> {line}\n"));
        }
        text.push('\n');
        text.push_str(&comment.note);
        text.push('\n');
    }
    text
}

#[derive(Clone, PartialEq, Eq)]
pub(in crate::gpui_shell) struct Draft {
    pub(in crate::gpui_shell) location: GitLocation,
    path: String,
    staged: bool,
    snapshot: [u8; 32],
    display_path: String,
    feedback: String,
    comments: usize,
}

pub(in crate::gpui_shell) struct FileSummary {
    pub(in crate::gpui_shell) path: String,
    pub(in crate::gpui_shell) staged: bool,
    pub(in crate::gpui_shell) comments: usize,
    pub(in crate::gpui_shell) pending: bool,
    pub(in crate::gpui_shell) loading: bool,
    pub(in crate::gpui_shell) ready: bool,
}

/// Detached text only: closing a tab must release its editor and background tasks.
pub(in crate::gpui_shell) struct RetainedReview {
    pub(in crate::gpui_shell) location: GitLocation,
    pub(in crate::gpui_shell) relative_path: String,
    pub(in crate::gpui_shell) staged: bool,
    pub(in crate::gpui_shell) path: PathBuf,
    snapshot: Option<[u8; 32]>,
    comments: Vec<Comment>,
    next_id: u64,
    input: String,
    current: bool,
}

impl RetainedReview {
    pub(in crate::gpui_shell) fn feedback_bytes(&self) -> usize {
        feedback_bytes(&self.comments)
    }
    pub(in crate::gpui_shell) fn comments(&self) -> &[Comment] {
        &self.comments
    }
    pub(in crate::gpui_shell) fn remove_comment(&mut self, id: u64) {
        self.comments.retain(|comment| comment.id != id);
        if self.comments.is_empty() {
            self.snapshot = None;
        }
    }
    pub(in crate::gpui_shell) fn empty(&self) -> bool {
        self.comments.is_empty() && self.input.trim().is_empty()
    }
    pub(in crate::gpui_shell) fn draft(&self) -> Option<Draft> {
        if !self.current || self.comments.is_empty() {
            return None;
        }
        Some(Draft {
            location: self.location.clone(),
            path: self.relative_path.clone(),
            staged: self.staged,
            snapshot: self.snapshot?,
            display_path: MergeKey {
                location: self.location.clone(),
                relative_path: self.relative_path.clone(),
            }
            .display_path(),
            feedback: feedback(&self.comments),
            comments: self.comments.len(),
        })
    }
    pub(in crate::gpui_shell) fn bytes(&self) -> usize {
        self.input.len()
            + self
                .comments
                .iter()
                .map(|c| c.anchor.len() + c.excerpt.len() + c.note.len())
                .sum::<usize>()
            + self.relative_path.len()
            + self.path.as_os_str().len()
    }

    pub(in crate::gpui_shell) fn matches(&self, view: &CodeTabView) -> bool {
        view.matches_git_diff(&self.location, &self.relative_path, self.staged)
    }

    pub(in crate::gpui_shell) fn summary(&self) -> FileSummary {
        FileSummary {
            path: self.relative_path.clone(),
            staged: self.staged,
            comments: self.comments.len(),
            pending: !self.input.trim().is_empty(),
            loading: false,
            ready: self.current && self.snapshot.is_some(),
        }
    }
}

impl Draft {
    pub(in crate::gpui_shell) fn staged(&self) -> bool {
        self.staged
    }
    pub(in crate::gpui_shell) fn comment_count(&self) -> usize {
        self.comments
    }

    pub(in crate::gpui_shell) fn path(&self) -> &str {
        &self.path
    }

    pub(in crate::gpui_shell) fn append_to(
        &self,
        text: &mut String,
        language: crate::display::UiLanguage,
    ) {
        text.push_str(&language.format(
            Message::ReviewFile,
            &[
                ("path", &self.display_path),
                (
                    "scope",
                    language.text(if self.staged {
                        Message::ReviewScopeStaged
                    } else {
                        Message::ReviewScopeWorktree
                    }),
                ),
            ],
        ));
        text.push_str(&self.feedback);
    }

    pub(in crate::gpui_shell) fn validate(&self) -> Result<(), crate::runtime_api::ApiError> {
        let current =
            crate::runtime_api::git::read_diff(&self.location, self.path.clone(), self.staged)?;
        if digest(&current) != self.snapshot {
            return Err(crate::runtime_api::ApiError::new(
                "review_stale",
                "The reviewed patch changed",
            ));
        }
        Ok(())
    }
}

impl CodeTabView {
    pub(in crate::gpui_shell) fn review_feedback_bytes(&self) -> usize {
        feedback_bytes(self.review_comments())
    }
    pub(in crate::gpui_shell) fn review_comments(&self) -> &[Comment] {
        self.diff.as_ref().map_or(&[], |diff| diff.review.comments.as_slice())
    }
    pub(in crate::gpui_shell) fn remove_review_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(diff) = &mut self.diff {
            diff.review.comments.retain(|comment| comment.id != id);
            if diff.review.comments.is_empty() {
                diff.review.snapshot = None;
            }
            diff.review.copy.update(cx, |copy, cx| copy.clear(cx));
            cx.emit(CodeTabViewEvent::Changed);
            cx.notify();
        }
    }
    pub(in crate::gpui_shell) fn locate_review_comment(
        &mut self,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(diff) = &mut self.diff {
            diff.review.pending_locate = diff.loading.then_some(id);
        }
        if self.review_is_current() {
            let range = self
                .review_comments()
                .iter()
                .find(|comment| comment.id == id)
                .map(|comment| comment.range.clone());
            if let Some(range) = range {
                if let Some(diff) = &mut self.diff {
                    diff.raw = true;
                }
                self.input.update(cx, |input, cx| {
                    input.set_selected_range(range.clone(), cx);
                    input.focus(window, cx);
                });
                // Raw mode may be entering its first layout; reveal again once bounds exist.
                cx.on_next_frame(window, move |this, window, cx| {
                    if this.review_is_current()
                        && this.diff.as_ref().is_some_and(|diff| diff.raw)
                        && this.review_comments().iter().any(|comment| comment.id == id)
                        && this.input.read(cx).focus_handle(cx).is_focused(window)
                    {
                        this.input.update(cx, |input, cx| input.set_selected_range(range, cx));
                    }
                });
            }
        }
        if let Some(diff) = &mut self.diff {
            diff.review.open = true;
        }
        cx.notify();
    }
    pub(in crate::gpui_shell) fn retain_review(
        &self,
        budget: usize,
        cx: &gpui::App,
    ) -> Result<Option<RetainedReview>, ()> {
        let Some(diff) = self.diff.as_ref() else { return Ok(None) };
        let input = diff.review.input.read(cx).value();
        if diff.review.comments.is_empty() && input.trim().is_empty() {
            return Ok(None);
        }
        let bytes = input.len()
            + diff
                .review
                .comments
                .iter()
                .map(|c| c.anchor.len() + c.excerpt.len() + c.note.len())
                .sum::<usize>()
            + diff.key.relative_path.len()
            + self.path.as_os_str().len();
        if bytes > budget {
            return Err(());
        }
        Ok(Some(RetainedReview {
            location: diff.key.location.clone(),
            relative_path: diff.key.relative_path.clone(),
            staged: diff.staged,
            path: self.path.clone(),
            snapshot: diff.review.snapshot,
            comments: diff.review.comments.clone(),
            next_id: diff.review.next_id,
            input: input.to_string(),
            current: diff.snapshot.is_some()
                && (diff.review.comments.is_empty() || diff.review.snapshot == diff.snapshot),
        }))
    }

    pub(in crate::gpui_shell) fn restore_review(
        &mut self,
        retained: RetainedReview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(diff) = &mut self.diff else { return };
        diff.review.snapshot = retained.snapshot;
        diff.review.comments = retained.comments;
        diff.review.next_id = retained.next_id;
        diff.review.open = true;
        diff.review.input.update(cx, |input, cx| input.set_value(retained.input, window, cx));
        // The patch is reloaded independently. Its digest must still match before adding/sending.
        cx.notify();
    }

    fn copy_review_feedback(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.stored_review_draft() else { return };
        let language = crate::gpui_shell::config::ui_language(cx);
        let mut text = language.text(Message::ReviewFeedbackHeader).to_owned();
        draft.append_to(&mut text, language);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        if let Some(diff) = &self.diff {
            diff.review.copy.update(cx, |copy, cx| copy.mark_copied(cx));
        }
    }

    pub(in crate::gpui_shell) fn has_review_in(&self, location: &GitLocation) -> bool {
        self.diff
            .as_ref()
            .is_some_and(|diff| &diff.key.location == location && !diff.review.comments.is_empty())
    }

    pub(in crate::gpui_shell) fn review_path(&self) -> Option<String> {
        self.diff.as_ref().map(|diff| diff.key.display_path())
    }

    pub(in crate::gpui_shell) fn review_location(&self) -> Option<&GitLocation> {
        self.diff.as_ref().map(|diff| &diff.key.location)
    }

    pub(in crate::gpui_shell) fn review_summary(
        &self,
        location: &GitLocation,
        cx: &gpui::App,
    ) -> Option<FileSummary> {
        let diff = self.diff.as_ref()?;
        if &diff.key.location != location {
            return None;
        }
        let pending = !diff.review.input.read(cx).value().trim().is_empty();
        if diff.review.comments.is_empty() && !pending {
            return None;
        }
        Some(FileSummary {
            path: diff.key.relative_path.clone(),
            staged: diff.staged,
            comments: diff.review.comments.len(),
            pending,
            loading: diff.loading,
            ready: self.review_is_current(),
        })
    }

    fn review_is_current(&self) -> bool {
        self.diff.as_ref().is_some_and(|diff| {
            !diff.loading && diff.snapshot.is_some() && diff.review.snapshot == diff.snapshot
        })
    }

    pub(in crate::gpui_shell) fn review_draft(&self) -> Option<Draft> {
        if !self.review_is_current() {
            return None;
        }
        self.stored_review_draft()
    }

    pub(in crate::gpui_shell) fn open_review_tools(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(diff) = self.diff.as_mut().filter(|diff| !diff.historical) {
            diff.review.open = true;
            self.focus_review_comment(window, cx);
            cx.notify();
        }
    }

    fn stored_review_draft(&self) -> Option<Draft> {
        let diff = self.diff.as_ref()?;
        if diff.review.comments.is_empty() {
            return None;
        }
        Some(Draft {
            location: diff.key.location.clone(),
            path: diff.key.relative_path.clone(),
            staged: diff.staged,
            snapshot: diff.review.snapshot?,
            display_path: diff.key.display_path(),
            feedback: diff.review.feedback(),
            comments: diff.review.comments.len(),
        })
    }

    fn add_review_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(diff) = self.diff.as_mut() else { return };
        if diff.historical || diff.loading || diff.snapshot.is_none() {
            return;
        }
        let language = crate::gpui_shell::config::ui_language(cx);
        let issue = if !diff.review.comments.is_empty() && diff.review.snapshot != diff.snapshot {
            Some(Message::ReviewStale)
        } else {
            let note = diff.review.input.read(cx).value().trim().to_owned();
            let input = self.input.read(cx);
            let text = input.value();
            let range = input.selected_range();
            if note.is_empty() || note.len() > 4000 {
                Some(Message::ReviewNoteRequired)
            } else if range.len() > 8 * 1024 {
                Some(Message::ReviewTooLarge)
            } else if let Some((anchor, excerpt)) = selected_context(&text, range.clone()) {
                let size: usize = diff
                    .review
                    .comments
                    .iter()
                    .map(|comment| comment.note.len() + comment.excerpt.len() * 2 + 128)
                    .sum();
                if diff.review.comments.len() >= 32
                    || size + note.len() + excerpt.len() * 2 + 128 > MAX_REVIEW_BYTES
                {
                    Some(Message::ReviewLimit)
                } else {
                    diff.review.snapshot = diff.snapshot;
                    diff.review.next_id = diff.review.next_id.wrapping_add(1);
                    diff.review.comments.push(Comment {
                        id: diff.review.next_id,
                        anchor,
                        excerpt,
                        note,
                        range,
                    });
                    diff.review.copy.update(cx, |copy, cx| copy.clear(cx));
                    diff.review.input.update(cx, |input, cx| input.set_value("", window, cx));
                    None
                }
            } else {
                Some(Message::ReviewEmpty)
            }
        };
        self.notice = issue.map(|message| language.text(message).to_owned());
        cx.emit(CodeTabViewEvent::Changed);
        cx.notify();
    }

    pub(super) fn focus_review_comment(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(diff) = &self.diff {
            diff.review.input.update(cx, |input, cx| input.focus(window, cx));
        }
    }

    pub(super) fn render_review_tools(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let diff = self.diff.as_ref().expect("review tools require a diff");
        let review = &diff.review;
        let copied = review.copy.read(cx).is_copied();
        let language = crate::gpui_shell::config::ui_language(cx);
        let stale = !review.comments.is_empty() && review.snapshot != diff.snapshot;
        let count =
            language.format(Message::ReviewCount, &[("count", &review.comments.len().to_string())]);
        let mut comments = v_flex().gap_1();
        for comment in &review.comments {
            let id = comment.id;
            comments = comments.child(
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .whitespace_normal()
                            .child(format!("{} · {}", comment.anchor, comment.note)),
                    )
                    .child(
                        Button::new(("review-remove", id))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .aria_label(language.text(Message::CommonDelete))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_review_comment(id, cx)
                            })),
                    ),
            );
        }
        v_flex()
            .w_full()
            .max_h(px(280.0))
            .gap_2()
            .p_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .items_start()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .whitespace_normal()
                            .text_color(cx.theme().muted_foreground)
                            .child(language.text(if stale {
                                Message::ReviewStale
                            } else {
                                Message::ReviewHint
                            })),
                    )
                    .child(
                        Button::new("review-hide")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .tooltip(language.text(Message::CommonClose))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(diff) = &mut this.diff {
                                    diff.review.open = false;
                                }
                                cx.emit(CodeTabViewEvent::Changed);
                                cx.notify();
                            })),
                    ),
            )
            .child(Input::new(&review.input).small())
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("review-add")
                            .small()
                            .label(language.text(Message::ReviewAdd))
                            .disabled(diff.loading || diff.snapshot.is_none() || stale)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.add_review_comment(window, cx)
                            })),
                    )
                    .child(
                        Button::new("review-clear")
                            .ghost()
                            .small()
                            .label(language.text(Message::ReviewClear))
                            .disabled(review.comments.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(diff) = &mut this.diff {
                                    diff.review.comments.clear();
                                    diff.review.snapshot = None;
                                }
                                if let Some(diff) = &this.diff {
                                    diff.review.copy.update(cx, |copy, cx| copy.clear(cx));
                                }
                                this.notice = None;
                                cx.emit(CodeTabViewEvent::Changed);
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1().text_xs().child(count))
                    .child(
                        Button::new("review-copy")
                            .ghost()
                            .small()
                            .label(language.text(if copied {
                                Message::ReviewCopied
                            } else {
                                Message::ReviewCopy
                            }))
                            .disabled(review.comments.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| this.copy_review_feedback(cx))),
                    )
                    .child(
                        Button::new("review-send")
                            .small()
                            .label(language.text(Message::ReviewSend))
                            .tooltip(language.text(Message::ReviewScope))
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(CodeTabViewEvent::ReviewRequested)
                            })),
                    ),
            )
            .child(
                div().id("review-comments-scroll").min_h_0().overflow_y_scrollbar().child(comments),
            )
            .into_any_element()
    }
}

fn selected_context(text: &str, range: std::ops::Range<usize>) -> Option<(String, String)> {
    let prefix = text.get(..range.start)?;
    let start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let end = if range.end > range.start && text.as_bytes().get(range.end - 1) == Some(&b'\n') {
        range.end - 1
    } else {
        range.end + text.get(range.end..)?.find('\n').unwrap_or(text.len() - range.end)
    };
    let excerpt = text.get(start..end)?;
    if excerpt.is_empty() || excerpt.len() > 8 * 1024 {
        return None;
    }
    super::diff::rows::selected_context(text, start..end)
}
