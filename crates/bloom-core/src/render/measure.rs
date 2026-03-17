/// Measure the display width of a text slice.
///
/// Returns a unit appropriate for the frontend — columns for TUI, pixels
/// for GUI.  The wrap algorithm is generic over this trait so the same logic
/// works for monospace columns, proportional fonts, and mixed-width text.
pub trait MeasureWidth {
    fn width(&self, text: &str) -> usize;
}
