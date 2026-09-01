use gpui::{App, Pixels, Rems, px, rems};
use theme::UiDensity;

/// A dynamic spacing system that adjusts spacing based on [UiDensity].
///
/// The number following "Base" refers to the base pixel size
/// at the default rem size and spacing settings.
///
/// When possible, [DynamicSpacing] should be used over manual
/// or built-in spacing values in places dynamic spacing is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DynamicSpacing {
    /// `0px`|`0px`|`0px (@16px/rem)` - Scales with the user's rem size.
    Base00,
    /// `1px`|`1px`|`2px (@16px/rem)` - Scales with the user's rem size.
    Base01,
    /// `1px`|`2px`|`4px (@16px/rem)` - Scales with the user's rem size.
    Base02,
    /// `2px`|`3px`|`4px (@16px/rem)` - Scales with the user's rem size.
    Base03,
    /// `2px`|`4px`|`6px (@16px/rem)` - Scales with the user's rem size.
    Base04,
    /// `3px`|`6px`|`8px (@16px/rem)` - Scales with the user's rem size.
    Base06,
    /// `4px`|`8px`|`10px (@16px/rem)` - Scales with the user's rem size.
    Base08,
    /// `10px`|`12px`|`14px (@16px/rem)` - Scales with the user's rem size.
    Base12,
    /// `14px`|`16px`|`18px (@16px/rem)` - Scales with the user's rem size.
    Base16,
    /// `18px`|`20px`|`22px (@16px/rem)` - Scales with the user's rem size.
    Base20,
    /// `20px`|`24px`|`28px (@16px/rem)` - Scales with the user's rem size.
    Base24,
    /// `28px`|`32px`|`36px (@16px/rem)` - Scales with the user's rem size.
    Base32,
    /// `36px`|`40px`|`44px (@16px/rem)` - Scales with the user's rem size.
    Base40,
    /// `44px`|`48px`|`52px (@16px/rem)` - Scales with the user's rem size.
    Base48,
}

impl DynamicSpacing {
    /// Returns the spacing value in pixels at the default density, one of
    /// `(compact, default, comfortable)`.
    fn pixels_for_density(self) -> (f32, f32, f32) {
        match self {
            DynamicSpacing::Base00 => (0., 0., 0.),
            DynamicSpacing::Base01 => (1., 1., 2.),
            DynamicSpacing::Base02 => (1., 2., 4.),
            DynamicSpacing::Base03 => (2., 3., 4.),
            DynamicSpacing::Base04 => (2., 4., 6.),
            DynamicSpacing::Base06 => (3., 6., 8.),
            DynamicSpacing::Base08 => (4., 8., 10.),
            DynamicSpacing::Base12 => (10., 12., 14.),
            DynamicSpacing::Base16 => (14., 16., 18.),
            DynamicSpacing::Base20 => (18., 20., 22.),
            DynamicSpacing::Base24 => (20., 24., 28.),
            DynamicSpacing::Base32 => (28., 32., 36.),
            DynamicSpacing::Base40 => (36., 40., 44.),
            DynamicSpacing::Base48 => (44., 48., 52.),
        }
    }

    /// Returns the spacing ratio, should only be used internally.
    fn spacing_ratio(&self, cx: &App) -> f32 {
        const BASE_REM_SIZE_IN_PX: f32 = 16.0;
        let (compact, default, comfortable) = self.pixels_for_density();
        let px = match theme::theme_settings(cx).ui_density(cx) {
            UiDensity::Compact => compact,
            UiDensity::Default => default,
            UiDensity::Comfortable => comfortable,
        };
        px / BASE_REM_SIZE_IN_PX
    }

    /// Returns the spacing value in rems.
    pub fn rems(&self, cx: &App) -> Rems {
        rems(self.spacing_ratio(cx))
    }

    /// Returns the spacing value in pixels.
    pub fn px(&self, cx: &App) -> Pixels {
        let ui_font_size_f32: f32 = theme::theme_settings(cx).ui_font_size(cx).into();
        px(ui_font_size_f32 * self.spacing_ratio(cx))
    }
}

/// Returns the current [`UiDensity`] setting. Use this to
/// modify or show something in the UI other than spacing.
///
/// Do not use this to calculate spacing values.
///
/// Always use [DynamicSpacing] for spacing values.
pub fn ui_density(cx: &mut App) -> UiDensity {
    theme::theme_settings(cx).ui_density(cx)
}
