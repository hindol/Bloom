/// Reserved for future use: trait-based keymap abstraction.
pub trait KeyMapper {
    /// Map a key event to zero or more actions given the current context.
    fn map(
        &self,
        key: &crate::types::KeyEvent,
        context: &super::dispatch::EditorContext,
    ) -> Vec<super::dispatch::Action>;
}