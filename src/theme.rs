use egui_code_editor::ColorTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    GithubDark,
    GithubLight,
}

impl Theme {
    pub const ALL: &'static [Theme] = &[Theme::GithubDark, Theme::GithubLight];

    pub fn name(&self) -> &'static str {
        match self {
            Theme::GithubDark => ColorTheme::GITHUB_DARK.name,
            Theme::GithubLight => ColorTheme::GITHUB_LIGHT.name,
        }
    }

    pub fn to_color_theme(self) -> ColorTheme {
        match self {
            Theme::GithubDark => ColorTheme::GITHUB_DARK,
            Theme::GithubLight => ColorTheme::GITHUB_LIGHT,
        }
    }

    pub fn is_dark(self) -> bool {
        self.to_color_theme().is_dark()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::GithubDark
    }
}
