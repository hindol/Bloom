//! Keymap dispatch and action types.
//!
//! Routes keyboard input through a priority chain — platform shortcuts, picker
//! input, quick-capture, Vim processing — and maps the result to [`Action`]
//! variants that the editor executes. Also defines [`PickerKind`],
//! [`EditorContext`], and supporting enums.

pub mod dispatch;
pub mod platform;
pub mod traits;
pub mod user;

pub use dispatch::{
    Action, DatePickerPurpose, EditorContext, KeymapConfig, KeymapDispatcher, MotionResult,
    PickerInputAction, PickerKind, QuickCaptureKind, RefactorOp, ResizeOp,
};
pub use platform::platform_shortcut;
pub use user::UserKeymap;
