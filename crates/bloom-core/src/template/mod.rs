//! Template engine with tab stops and mirror editing.
//!
//! Expands markdown templates containing magic variables (`${AUTO}`, `${DATE}`,
//! `${TITLE}`) and numbered placeholders (`${1:description}`). After expansion,
//! the user tabs through stops; mirror edits synchronize duplicate placeholders.

pub mod builtins;
#[allow(clippy::module_inception)]
pub mod template;

pub use template::{
    ExpandedTemplate, MirrorEdit, Placeholder, TabStop, Template, TemplateAdvanceResult,
    TemplateEngine, TemplateModeState,
};
