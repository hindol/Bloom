use crate::types::PaneId;

pub enum SplitDirection {
    Vertical,
    Horizontal,
}

pub enum LayoutTree {
    Leaf(PaneId),
    Split {
        direction: SplitDirection,
        children: Vec<(f32, LayoutTree)>,
    },
}