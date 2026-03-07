#[allow(clippy::module_inception)]
pub mod template;

pub use template::{
    ExpandedTemplate, MirrorEdit, Placeholder, TabStop, Template, TemplateAdvanceResult,
    TemplateEngine, TemplateModeState,
};
