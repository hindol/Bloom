/// Trait for items that can be displayed and matched in a picker.
pub trait PickerItem: Clone {
    fn match_text(&self) -> &str;
    fn display(&self) -> PickerRow;
    fn preview(&self) -> Option<String>;
}

#[derive(Debug, Clone)]
pub struct PickerRow {
    pub label: String,
    pub marginalia: Vec<String>,
}