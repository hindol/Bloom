use crate::types::PaneId;
use serde::Serialize;

/// Serializable snapshot of the pane split structure.
///
/// The TUI uses [`PaneRectFrame`](super::PaneRectFrame) (cell coordinates)
/// for layout.  The GUI reads this tree and computes pixel rects itself
/// from the split ratios and its own font metrics.
#[derive(Debug, Clone, Serialize)]
pub enum LayoutTree {
    Leaf(PaneId),
    Split {
        direction: SplitDirection,
        children: Vec<(f32, LayoutTree)>,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}
