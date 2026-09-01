use gpui::{App, Pixels, Rems, Styled, px};

use crate::rems_from_px;

/// Extends [`gpui::Styled`] with typography-related styling methods.
pub trait StyledTypography: Styled + Sized {
    /// Sets the font family to the buffer font.
    fn font_buffer(self, cx: &App) -> Self {
        let settings = theme::theme_settings(cx);
        let buffer_font_family = settings.buffer_font(cx).family.clone();

        self.font_family(buffer_font_family)
    }

    /// Sets the font family to the UI font.
    fn font_ui(self, cx: &App) -> Self {
        let settings = theme::theme_settings(cx);
        let ui_font_family = settings.ui_font(cx).family.clone();

        self.font_family(ui_font_family)
    }

    /// Sets the text size using a [`TextSize`].
    fn text_ui_size(self, size: TextSize, cx: &App) -> Self {
        self.text_size(size.rems(cx))
    }

    /// The large size for UI text.
    ///
    /// `1rem` or `16px` at the default scale of `1rem` = `16px`.
    ///
    /// Note: The absolute size of this text will change based on a user's `ui_scale` setting.
    ///
    /// Use `text_ui` for regular-sized text.
    fn text_ui_lg(self, cx: &App) -> Self {
        self.text_size(TextSize::Large.rems(cx))
    }

    /// The default size for UI text.
    ///
    /// `0.825rem` or `14px` at the default scale of `1rem` = `16px`.
    ///
    /// Note: The absolute size of this text will change based on a user's `ui_scale` setting.
    ///
    /// Use `text_ui_sm` for smaller text.
    fn text_ui(self, cx: &App) -> Self {
        self.text_size(TextSize::default().rems(cx))
    }

    /// The small size for UI text.
    ///
    /// `0.75rem` or `12px` at the default scale of `1rem` = `16px`.
    ///
    /// Note: The absolute size of this text will change based on a user's `ui_scale` setting.
    ///
    /// Use `text_ui` for regular-sized text.
    fn text_ui_sm(self, cx: &App) -> Self {
        self.text_size(TextSize::Small.rems(cx))
    }

    /// The extra small size for UI text.
    ///
    /// `0.625rem` or `10px` at the default scale of `1rem` = `16px`.
    ///
    /// Note: The absolute size of this text will change based on a user's `ui_scale` setting.
    ///
    /// Use `text_ui` for regular-sized text.
    fn text_ui_xs(self, cx: &App) -> Self {
        self.text_size(TextSize::XSmall.rems(cx))
    }

    /// The font size for buffer text.
    ///
    /// Retrieves the default font size, or the user's custom font size if set.
    ///
    /// This should only be used for text that is displayed in a buffer,
    /// or other places that text needs to match the user's buffer font size.
    fn text_buffer(self, cx: &App) -> Self {
        let settings = theme::theme_settings(cx);
        self.text_size(settings.buffer_font_size(cx))
    }
}

impl<E: Styled> StyledTypography for E {}

/// A utility for getting the size of various semantic text sizes.
#[derive(Debug, Default, Clone)]
pub enum TextSize {
    /// The default size for UI text.
    ///
    /// `0.825rem` or `14px` at the default scale of `1rem` = `16px`.
    ///
    /// Note: The absolute size of this text will change based on a user's `ui_scale` setting.
    #[default]
    Default,
    /// The large size for UI text.
    ///
    /// `1rem` or `16px` at the default scale of `1rem` = `16px`.
    ///
    /// Note: The absolute size of this text will change based on a user's `ui_scale` setting.
    Large,

    /// The small size for UI text.
    ///
    /// `0.75rem` or `12px` at the default scale of `1rem` = `16px`.
    ///
    /// Note: The absolute size of this text will change based on a user's `ui_scale` setting.
    Small,

    /// The extra small size for UI text.
    ///
    /// `0.625rem` or `10px` at the default scale of `1rem` = `16px`.
    ///
    /// Note: The absolute size of this text will change based on a user's `ui_scale` setting.
    XSmall,

    /// The `ui_font_size` set by the user.
    Ui,
    /// The `buffer_font_size` set by the user.
    Editor,
}

impl TextSize {
    /// Returns the text size in rems.
    pub fn rems(self, cx: &App) -> Rems {
        let settings = theme::theme_settings(cx);

        match self {
            Self::Large => rems_from_px(16_f32),
            Self::Default => rems_from_px(14_f32),
            Self::Small => rems_from_px(12_f32),
            Self::XSmall => rems_from_px(10_f32),
            Self::Ui => rems_from_px(settings.ui_font_size(cx)),
            Self::Editor => rems_from_px(settings.buffer_font_size(cx)),
        }
    }

    pub fn pixels(self, cx: &App) -> Pixels {
        let settings = theme::theme_settings(cx);

        match self {
            Self::Large => px(16.),
            Self::Default => px(14.),
            Self::Small => px(12.),
            Self::XSmall => px(10.),
            Self::Ui => settings.ui_font_size(cx),
            Self::Editor => settings.buffer_font_size(cx),
        }
    }
}
