//! The GPUI prelude is a collection of traits and types that are widely used
//! throughout the library. It is recommended to import this prelude into your
//! application to avoid having to import each trait individually.

pub use crate::{
    AppContext as _, BorrowAppContext, Context, IntoElement, Refineable, Render, TaskExt as _,
    util::FluentBuilder,
};
#[cfg(any(test, feature = "test-support", feature = "ui"))]
pub use crate::VisualContext;
#[cfg(feature = "ui")]
pub use crate::StyledImage;
