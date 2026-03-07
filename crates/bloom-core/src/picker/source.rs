/// Trait for items that can be displayed and matched in a picker.
pub trait PickerItem: Clone {
    fn match_text(&self) -> &str;
    fn display(&self) -> PickerRow;
    fn preview(&self) -> Option<String>;
    /// Extra score added to the fuzzy match score (e.g., frecency boost).
    fn score_boost(&self) -> u32 {
        0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnStyle {
    Normal,
    Faded,
}

#[derive(Debug, Clone)]
pub struct PickerColumn {
    pub text: String,
    pub style: ColumnStyle,
}

#[derive(Debug, Clone)]
pub struct PickerRow {
    pub label: String,
    pub middle: Option<PickerColumn>,
    pub right: Option<PickerColumn>,
}
