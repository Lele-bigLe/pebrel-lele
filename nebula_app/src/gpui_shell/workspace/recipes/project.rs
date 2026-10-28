//! A local project startup template uses the existing layout schema and CLI start spelling.

use super::*;
use crate::ai_agents::AgentKind;
use crate::session::LaunchSession;
use gpui_component::menu::PopupMenuItem;
use std::path::PathBuf;

pub(super) struct ProjectTemplate {
    name: Entity<InputState>,
    directory: Entity<InputState>,
    shells: Vec<crate::shell_detect::DetectedShell>,
    selected_shell: usize,
    agent: Option<AgentKind>,
    split: bool,
    task: Option<gpui::Task<()>>,
    error: Option<String>,
}

pub(super) enum ProjectTemplateEvent {
    Saved,
    Back,
}

impl EventEmitter<ProjectTemplateEvent> for ProjectTemplate {}

impl ProjectTemplate {
    pub(super) fn new(root: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let language = crate::gpui_shell::config::ui_language(cx);
        let directory = root.as_ref().map(|root| root.display().to_string()).unwrap_or_default();
        let name = root
            .as_ref()
            .and_then(|root| root.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        let mut this = Self {
            name: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(language.text(Message::RecipeName))
                    .default_value(name)
            }),
            directory: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(language.text(Message::ProjectTemplateDirectory))
                    .default_value(directory)
            }),
            shells: vec![],
            selected_shell: 0,
            agent: Some(AgentKind::Codex),
            split: true,
            task: None,
            error: None,
        };
        let work = cx.background_executor().spawn(async {
            crate::shell_detect::detect_shells()
                .into_iter()
                .filter(|shell| !crate::shell_detect::is_wsl_launcher(&shell.program))
                .collect::<Vec<_>>()
        });
        this.task = Some(cx.spawn(async move |this, cx| {
            let shells = work.await;
            let _ = this.update(cx, |this, cx| {
                this.shells = shells;
                this.task = None;
                cx.notify();
            });
        }));
        this
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.task.is_some() {
            return;
        }
        let Some(shell) = self.shells.get(self.selected_shell) else { return };
        let actual = shell.shell();
        let launch = LaunchSession::Shell {
            name: shell.name.clone(),
            program: actual.program().into(),
            args: actual.args().to_vec(),
        };
        let name = self.name.read(cx).value().to_string();
        let root = PathBuf::from(self.directory.read(cx).value().trim());
        let agent = self.agent;
        let split = self.split;
        self.error = None;
        let work = cx.background_executor().spawn(async move {
            let recipe = store::Recipe::project(name, root, launch, agent, split)?;
            store::save_at(&store::directory(), &recipe)
        });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.task = None;
                match result {
                    Ok(()) => cx.emit(ProjectTemplateEvent::Saved),
                    Err(error) => this.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
}

impl Render for ProjectTemplate {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = crate::gpui_shell::config::ui_language(cx);
        let busy = self.task.is_some();
        let muted = cx.theme().muted_foreground;
        let owner = cx.entity().downgrade();
        let shells = self.shells.clone();
        let selected = self.selected_shell;
        let shell_name =
            shells.get(selected).map(|shell| shell.name.clone()).unwrap_or_else(|| {
                language
                    .text(if busy {
                        Message::ProjectTemplateLoadingShells
                    } else {
                        Message::ProjectTemplateNoShell
                    })
                    .into()
            });
        let mut agents = h_flex().gap_1().flex_wrap();
        for (index, agent) in [
            None,
            Some(AgentKind::Codex),
            Some(AgentKind::Claude),
            Some(AgentKind::Gemini),
            Some(AgentKind::OpenCode),
        ]
        .into_iter()
        .enumerate()
        {
            agents = agents.child(
                Button::new(("project-template-agent", index))
                    .small()
                    .ghost()
                    .selected(self.agent == agent)
                    .disabled(busy)
                    .label(agent.map(|agent| agent.display_name()).unwrap_or("Shell"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.agent = agent;
                        cx.notify();
                    })),
            );
        }
        v_flex()
            .w_full()
            .gap_3()
            .child(
                Button::new("project-template-back")
                    .ghost()
                    .small()
                    .label(language.text(Message::ProjectTemplateBack))
                    .disabled(busy)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ProjectTemplateEvent::Back))),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(language.text(Message::ProjectTemplateHint)),
            )
            .child(Input::new(&self.name).disabled(busy))
            .child(Input::new(&self.directory).disabled(busy))
            .child(
                Button::new("project-template-shell")
                    .label(shell_name)
                    .disabled(busy || shells.is_empty())
                    .dropdown_menu(move |menu, _, _| {
                        shells.iter().enumerate().fold(menu, |menu, (index, shell)| {
                            let owner = owner.clone();
                            menu.item(
                                PopupMenuItem::new(shell.name.clone())
                                    .checked(selected == index)
                                    .on_click(move |_, _, cx| {
                                        let _ = owner.update(cx, |this, cx| {
                                            this.selected_shell = index;
                                            cx.notify();
                                        });
                                    }),
                            )
                        })
                    }),
            )
            .child(agents)
            .child(
                Checkbox::new("project-template-split")
                    .label(language.text(Message::ProjectTemplateSplit))
                    .checked(self.split)
                    .disabled(busy || self.agent.is_none())
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.split = *checked;
                        cx.notify();
                    })),
            )
            .when_some(self.error.clone(), |root, error| {
                root.child(
                    div().text_sm().text_color(cx.theme().danger).whitespace_normal().child(error),
                )
            })
            .child(
                Button::new("project-template-save")
                    .label(language.text(Message::RecipeSave))
                    .disabled(busy || self.shells.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            )
    }
}
