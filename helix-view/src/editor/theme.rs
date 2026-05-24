use anyhow::bail;

use crate::theme::Theme;

use super::{ConfigEvent, Editor};

enum ThemeAction {
    Set,
    Preview,
}

impl Editor {
    pub fn unset_theme_preview(&mut self) -> anyhow::Result<()> {
        if let Some(last_theme) = self.last_theme.take() {
            self.set_theme(last_theme)?;
        }
        // None likely occurs when the user types ":theme" and then exits before previewing
        Ok(())
    }

    pub fn set_theme_preview(&mut self, theme: Theme) -> anyhow::Result<()> {
        self.set_theme_impl(theme, ThemeAction::Preview)
    }

    pub fn set_theme(&mut self, theme: Theme) -> anyhow::Result<()> {
        self.set_theme_impl(theme, ThemeAction::Set)
    }

    fn set_theme_impl(&mut self, theme: Theme, preview: ThemeAction) -> anyhow::Result<()> {
        // `ui.selection` is the only scope required to be able to render a theme.
        if theme.find_highlight_exact("ui.selection").is_none() {
            bail!("Invalid theme: `ui.selection` required");
        }

        let scopes = theme.scopes();
        (*self.syn_loader).load().set_scopes(scopes.to_vec());

        match preview {
            ThemeAction::Preview => {
                let last_theme = std::mem::replace(&mut self.theme, theme);
                // only insert on first preview: this will be the last theme the user has saved
                self.last_theme.get_or_insert(last_theme);
            }
            ThemeAction::Set => {
                self.last_theme = None;
                self.theme = theme;
            }
        }

        self._refresh();
        self.config_events.0.send(ConfigEvent::ThemeChanged)?;

        Ok(())
    }
}
