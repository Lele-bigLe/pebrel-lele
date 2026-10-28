//! The workbench's new-session target is independent of the selected shell.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NewSessionTarget {
    #[default]
    Shell,
    Codex,
    Claude,
    Gemini,
    OpenCode,
}

impl NewSessionTarget {
    pub const ALL: [Self; 5] =
        [Self::Shell, Self::Codex, Self::Claude, Self::Gemini, Self::OpenCode];

    pub const fn settings_value(self) -> &'static str {
        match self {
            Self::Shell => "shell",
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::OpenCode => "opencode",
        }
    }

    pub fn from_settings(value: &str) -> Option<Self> {
        let value = value.trim();
        Self::ALL.into_iter().find(|target| target.settings_value().eq_ignore_ascii_case(value))
    }
}
