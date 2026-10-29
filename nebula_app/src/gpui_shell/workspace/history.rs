//! History uses the existing palette input, keyboard navigation and resume path.

use super::*;
use crate::ai_agents::AgentKind;
use crate::ai_sessions::AiSession;
use crate::i18n::Message;
use gpui_component::IconNamed as _;
use gpui_component::menu::PopupMenuItem;
use std::path::PathBuf;

const LIMIT: usize = 500;
mod details;
mod resume;
use crate::ai_sessions::annotations;

struct HistoryEntry {
    session: AiSession,
    cwd: Option<PathBuf>,
}

#[derive(Default)]
pub(super) struct HistoryState {
    pub(super) active: bool,
    project: Option<PathBuf>,
    source: Option<AgentKind>,
    entries: Vec<HistoryEntry>,
    task: Option<gpui::Task<()>>,
    restoring: Vec<(AgentKind, String, u64, std::time::Instant)>,
    annotations: Option<Result<annotations::Store, String>>,
    favorites_only: bool,
    resume_target: Option<gpui::WeakEntity<TerminalView>>,
}

impl HistoryState {
    pub(super) fn dismiss(&mut self) {
        self.active = false;
        self.task = None;
        self.entries.clear();
        self.annotations = None;
        self.resume_target = None;
    }
}

impl NebulaWorkspace {
    pub(super) fn choose_saved_agent_history(
        &mut self,
        view: &Entity<TerminalView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source =
            view.read(cx).session_agent().and_then(|agent| AgentKind::parse(&agent.source));
        if !view.read(cx).can_retry_recovery() {
            return;
        }
        if self.history.active {
            self.close_command_palette(window, cx);
        }
        self.open_ai_history(window, cx);
        self.history.source = source;
        self.history.resume_target = Some(view.downgrade());
        self.reset_history_selection(cx);
    }

    pub(super) fn resume_saved_history_target(
        &mut self,
        source: AgentKind,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self.history.resume_target.clone() else { return false };
        self.dismiss_palette_state();
        if let Some(view) = target.upgrade() {
            let pane = view.read(cx).pane_id;
            if let Some(tab) = self.tab_of_pane(pane) {
                self.activate_tab(tab, window, cx);
                self.focus_pane(tab, pane, window, cx);
            }
            view.update(cx, |view, cx| {
                view.continue_saved_agent(
                    Some(crate::session::AgentSession {
                        source: source.slug().to_owned(),
                        session_id: Some(id.to_owned()),
                        session_file: None,
                    }),
                    cx,
                )
            });
            self.focus_active(window, cx);
        }
        true
    }

    pub(super) fn open_ai_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.history.active {
            self.close_command_palette(window, cx);
            return;
        }
        self.dismiss_usage();
        self.dismiss_palette_state();
        self.command_manager_open = false;
        self.history.active = true;
        self.history.source = None;
        self.history.project = match &self.projects.filter {
            projects::ProjectFilter::Root(root) => Some(root.clone()),
            _ => None,
        };
        self.command_palette_open = true;
        self.command_palette_selected = 0;
        self.palette_override = Some(Vec::new());
        self.reset_palette_query(
            crate::gpui_shell::config::ui_language(cx).text(Message::HistorySearch),
            window,
            cx,
        );
        self.refresh_ai_history(cx);
    }

    fn refresh_ai_history(&mut self, cx: &mut Context<Self>) {
        if self.history.task.is_some() {
            return;
        }
        let worker = cx.background_executor().spawn(async {
            let annotations = annotations::load().map_err(|error| error.to_string());
            let mut sessions = crate::ai_sessions::scan_history(LIMIT);
            if let Ok(store) = &annotations {
                store.append_saved(&mut sessions);
            }
            let entries = sessions
                .into_iter()
                .filter(|session| session.resume_command().is_some())
                .map(|session| {
                    let path = PathBuf::from(&session.project);
                    // Do not stat historical WSL/UNC paths while browsing. The
                    // terminal launch reports unavailable directories on resume.
                    let cwd = path.is_absolute().then_some(path);
                    HistoryEntry { session, cwd }
                })
                .collect::<Vec<_>>();
            (entries, annotations)
        });
        self.history.task = Some(cx.spawn(async move |this, cx| {
            let (entries, annotations) = worker.await;
            let _ = this.update(cx, |this, cx| {
                this.history.task = None;
                this.history.entries = entries;
                if annotations.is_err() {
                    this.history.favorites_only = false;
                }
                this.history.annotations = Some(annotations);
                this.reset_history_selection(cx);
            });
        }));
        cx.notify();
    }

    pub(super) fn history_palette_rows(
        &self,
        language: crate::display::UiLanguage,
    ) -> Vec<WorkspacePaletteRow> {
        let store = self.history.annotations.as_ref().and_then(|result| result.as_ref().ok());
        let mut rows: Vec<_> = self
            .history
            .entries
            .iter()
            .filter(|entry| {
                self.history.source.is_none_or(|source| entry.session.source == source)
                    && self.history.project.as_ref().is_none_or(|project| {
                        Path::new(&entry.session.project).starts_with(project)
                    })
            })
            .filter_map(|entry| {
                let session = &entry.session;
                let annotation = store.map(|store| store.get(session)).unwrap_or_default();
                if self.history.favorites_only && store.is_some() && !annotation.favorite {
                    return None;
                }
                let command = session.resume_command()?;
                let time: chrono::DateTime<chrono::Local> = session.modified.into();
                Some(WorkspacePaletteRow {
                    // Native scan order is already newest first across providers.
                    group_order: u8::from(!annotation.favorite) as usize,
                    group: language.text(Message::HistoryTitle).to_owned(),
                    label: session.title.clone(),
                    hint: format!(
                        "{} · {} · {}{}",
                        session.source.display_name(),
                        session.place_label(),
                        if session.modified == std::time::UNIX_EPOCH {
                            language.text(Message::HistoryDetailsSavedLocator).to_owned()
                        } else {
                            time.format("%m-%d %H:%M").to_string()
                        },
                        if annotation.note.is_empty() {
                            String::new()
                        } else {
                            format!(" · {}", annotation.note.lines().next().unwrap_or_default())
                        }
                    ),
                    hint_style: WorkspacePaletteHintStyle::Metadata,
                    search: format!(
                        "{} {} {} {} {}",
                        session.title,
                        session.project,
                        session.source.display_name(),
                        session.source.slug(),
                        annotation.note
                    ),
                    action: WorkspacePaletteAction::RunAiSession {
                        command,
                        cwd: entry.cwd.clone(),
                        resume: Some((session.source, session.id.clone())),
                    },
                    icon: None,
                    icon_glyph: None,
                    icon_path: Some(if annotation.favorite {
                        IconName::Star.path()
                    } else {
                        IconName::BookOpen.path()
                    }),
                })
            })
            .collect();
        rows.sort_by_key(|row| row.group_order);
        // One visual group; sorting must not add duplicate history headers.
        for row in &mut rows {
            row.group_order = 0;
        }
        rows
    }

    pub(super) fn render_history_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let label = crate::gpui_shell::config::ui_language(cx).text(Message::HistoryTitle);
        Button::new("agent-history")
            .ghost()
            .small()
            .flex_shrink_0()
            .icon(Icon::new(Icon::empty()).path("icons/nebula-clock.svg"))
            .aria_label(label)
            .tooltip(label)
            .selected(self.history.active)
            .on_click(cx.listener(|this, _, window, cx| this.open_ai_history(window, cx)))
    }

    pub(super) fn history_resume_project(&self, cwd: Option<&PathBuf>) -> Option<PathBuf> {
        self.history.project.clone().or_else(|| {
            let cwd = cwd?;
            self.tab_meta
                .iter()
                .filter_map(|meta| meta.project_root.as_ref())
                .filter(|root| cwd.starts_with(root))
                .max_by_key(|root| root.components().count())
                .cloned()
                .or_else(|| Some(cwd.clone()))
        })
    }

    pub(super) fn history_empty_label(&self, language: crate::display::UiLanguage) -> String {
        if self.history.task.is_some() {
            language.text(Message::HistoryLoading).to_owned()
        } else {
            language.format(Message::HistoryEmpty, &[("limit", &LIMIT.to_string())])
        }
    }

    pub(super) fn render_history_controls(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let mut paths = std::collections::BTreeMap::<PathBuf, usize>::new();
        for entry in &self.history.entries {
            let path = PathBuf::from(&entry.session.project);
            if path.is_absolute() {
                *paths.entry(path).or_default() += 1;
            }
        }
        if let Some(path) = &self.history.project {
            paths.entry(path.clone()).or_default();
        }
        let paths: Vec<_> = paths.into_iter().collect();
        let labels = projects::project_labels(&paths);
        let selected = self.history.project.clone();
        let label = selected
            .as_ref()
            .and_then(|root| paths.iter().position(|(path, _)| path == root))
            .map(|index| labels[index].clone())
            .unwrap_or_else(|| language.text(Message::ProjectAll).to_owned());
        let owner = cx.entity().downgrade();
        let project = Button::new("history-project")
            .ghost()
            .small()
            .flex_1()
            .min_w_0()
            .dropdown_caret(true)
            .tooltip(SharedString::from(
                selected.as_ref().map_or_else(|| label.clone(), |p| p.display().to_string()),
            ))
            .child(div().flex_1().min_w_0().truncate().child(label))
            .dropdown_menu(move |mut menu, _, _| {
                let options =
                    std::iter::once((None, language.text(Message::ProjectAll).to_owned())).chain(
                        paths
                            .iter()
                            .zip(&labels)
                            .map(|((path, _), label)| (Some(path.clone()), label.clone())),
                    );
                for (root, label) in options {
                    let target = owner.clone();
                    let hint =
                        root.as_ref().map_or_else(|| label.clone(), |p| p.display().to_string());
                    menu = menu.item(
                        projects::project_menu_item(label, hint)
                            .checked(selected == root)
                            .on_click(move |_, _, cx| {
                                if let Some(owner) = target.upgrade() {
                                    owner.update(cx, |this, cx| {
                                        this.history.project = root.clone();
                                        this.reset_history_selection(cx);
                                    });
                                }
                            }),
                    );
                }
                menu.max_w(px(420.0)).scrollable(true)
            });
        let mut sources =
            vec![AgentKind::Codex, AgentKind::Claude, AgentKind::Gemini, AgentKind::OpenCode];
        for entry in &self.history.entries {
            if !sources.contains(&entry.session.source) {
                sources.push(entry.session.source);
            }
        }
        let source = self.history.source;
        let owner = cx.entity().downgrade();
        let provider = Button::new("history-provider")
            .ghost()
            .small()
            .dropdown_caret(true)
            .label(source.map_or(language.text(Message::HistoryAllTools), AgentKind::display_name))
            .dropdown_menu(move |mut menu, _, _| {
                for value in std::iter::once(None).chain(sources.iter().copied().map(Some)) {
                    let target = owner.clone();
                    menu = menu.item(
                        PopupMenuItem::new(value.map_or(
                            language.text(Message::HistoryAllTools),
                            AgentKind::display_name,
                        ))
                        .checked(source == value)
                        .on_click(move |_, _, cx| {
                            if let Some(owner) = target.upgrade() {
                                owner.update(cx, |this, cx| {
                                    this.history.source = value;
                                    this.reset_history_selection(cx);
                                });
                            }
                        }),
                    );
                }
                menu
            });
        let controls = h_flex()
            .h(px(44.0))
            .flex_shrink_0()
            .px_3()
            .gap_2()
            .items_center()
            .child(project)
            .child(provider)
            .child(
                Button::new("history-favorites")
                    .ghost()
                    .small()
                    .icon(IconName::Star)
                    .selected(self.history.favorites_only)
                    .tooltip(language.text(Message::HistoryDetailsFavorites))
                    .aria_label(language.text(Message::HistoryDetailsFavorites))
                    .disabled(
                        self.history.annotations.as_ref().is_none_or(|result| result.is_err()),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.history.favorites_only = !this.history.favorites_only;
                        this.reset_history_selection(cx);
                    })),
            )
            .child(
                Button::new("history-refresh")
                    .ghost()
                    .small()
                    .label(language.text(Message::UsageRefresh))
                    .tooltip(SharedString::from(
                        language
                            .format(Message::HistoryRangeHint, &[("limit", &LIMIT.to_string())]),
                    ))
                    .disabled(self.history.task.is_some())
                    .loading(self.history.task.is_some())
                    .on_click(cx.listener(|this, _, _, cx| this.refresh_ai_history(cx))),
            );
        v_flex()
            .flex_shrink_0()
            .child(controls)
            .when_some(
                self.history.annotations.as_ref().and_then(|result| result.as_ref().err()),
                |view, error| {
                    view.child(div().px_3().pb_2().text_xs().text_color(cx.theme().warning).child(
                        language.format(Message::HistoryDetailsMetadataError, &[("error", error)]),
                    ))
                },
            )
            .into_any_element()
    }

    fn reset_history_selection(&mut self, cx: &mut Context<Self>) {
        self.command_palette_selected = 0;
        self.command_palette_scroll.scroll_to_item(0);
        cx.notify();
    }
}
