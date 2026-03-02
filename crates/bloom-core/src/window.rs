// Window layout model for split panes and focus navigation.
//
// UI frontends consume this as pure state and drive rendering separately.

const DEFAULT_SPLIT_RATIO: f32 = 0.5;
const MIN_SPLIT_RATIO: f32 = 0.05;
const EPSILON: f32 = 0.000_1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(u64);

impl PaneId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitAxis {
    /// Left/right split (`SPC w v`).
    Vertical,
    /// Top/bottom split (`SPC w s`).
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// `h`
    Left,
    /// `j`
    Down,
    /// `k`
    Up,
    /// `l`
    Right,
}

impl Direction {
    pub fn from_hjkl(key: char) -> Option<Self> {
        match key {
            'h' => Some(Self::Left),
            'j' => Some(Self::Down),
            'k' => Some(Self::Up),
            'l' => Some(Self::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LayoutNode {
    Pane(PaneId),
    Split {
        axis: SplitAxis,
        /// Portion used by `first` child, in the range [0.0, 1.0].
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    fn first_pane(&self) -> PaneId {
        match self {
            LayoutNode::Pane(id) => *id,
            LayoutNode::Split { first, .. } => first.first_pane(),
        }
    }

    fn collect_panes(&self, out: &mut Vec<PaneId>) {
        match self {
            LayoutNode::Pane(id) => out.push(*id),
            LayoutNode::Split { first, second, .. } => {
                first.collect_panes(out);
                second.collect_panes(out);
            }
        }
    }

    fn collect_ratios(&self, out: &mut Vec<f32>) {
        match self {
            LayoutNode::Pane(_) => {}
            LayoutNode::Split {
                ratio,
                first,
                second,
                ..
            } => {
                out.push(*ratio);
                first.collect_ratios(out);
                second.collect_ratios(out);
            }
        }
    }

    fn contains_pane(&self, pane_id: PaneId) -> bool {
        match self {
            LayoutNode::Pane(id) => *id == pane_id,
            LayoutNode::Split { first, second, .. } => {
                first.contains_pane(pane_id) || second.contains_pane(pane_id)
            }
        }
    }

    fn balance(&mut self) {
        match self {
            LayoutNode::Pane(_) => {}
            LayoutNode::Split {
                ratio,
                first,
                second,
                ..
            } => {
                *ratio = DEFAULT_SPLIT_RATIO;
                first.balance();
                second.balance();
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowLayout {
    root: LayoutNode,
    focused: PaneId,
    next_pane_id: u64,
    maximized: Option<PaneId>,
}

impl Default for WindowLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowLayout {
    pub fn new() -> Self {
        Self {
            root: LayoutNode::Pane(PaneId(1)),
            focused: PaneId(1),
            next_pane_id: 2,
            maximized: None,
        }
    }

    pub fn root(&self) -> &LayoutNode {
        &self.root
    }

    pub fn focused(&self) -> PaneId {
        self.focused
    }

    pub fn maximized(&self) -> Option<PaneId> {
        self.maximized
    }

    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.root.collect_panes(&mut out);
        out
    }

    pub fn split_ratios(&self) -> Vec<f32> {
        let mut out = Vec::new();
        self.root.collect_ratios(&mut out);
        out
    }

    pub fn split_vertical(&mut self) -> PaneId {
        self.split_focused(SplitAxis::Vertical)
    }

    pub fn split_horizontal(&mut self) -> PaneId {
        self.split_focused(SplitAxis::Horizontal)
    }

    fn split_focused(&mut self, axis: SplitAxis) -> PaneId {
        let new_pane = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        let focused = self.focused;
        let did_split = Self::split_node(&mut self.root, focused, axis, new_pane);
        debug_assert!(did_split, "focused pane must exist in layout tree");
        self.focused = new_pane;

        if self.maximized == Some(focused) {
            self.maximized = Some(new_pane);
        }

        new_pane
    }

    fn split_node(
        node: &mut LayoutNode,
        target: PaneId,
        axis: SplitAxis,
        new_pane: PaneId,
    ) -> bool {
        match node {
            LayoutNode::Pane(existing) if *existing == target => {
                let first = LayoutNode::Pane(*existing);
                let second = LayoutNode::Pane(new_pane);
                *node = LayoutNode::Split {
                    axis,
                    ratio: DEFAULT_SPLIT_RATIO,
                    first: Box::new(first),
                    second: Box::new(second),
                };
                true
            }
            LayoutNode::Pane(_) => false,
            LayoutNode::Split { first, second, .. } => {
                Self::split_node(first, target, axis, new_pane)
                    || Self::split_node(second, target, axis, new_pane)
            }
        }
    }

    pub fn move_focus(&mut self, direction: Direction) -> bool {
        let next = self.neighbor_of(self.focused, direction);
        if let Some(next) = next {
            self.focused = next;
            true
        } else {
            false
        }
    }

    fn neighbor_of(&self, pane_id: PaneId, direction: Direction) -> Option<PaneId> {
        let panes = self.pane_rects();
        let current = panes
            .iter()
            .find_map(|(id, rect)| (*id == pane_id).then_some(*rect))?;

        let mut best_overlap: Option<(PaneId, f32, f32)> = None;
        let mut best_any: Option<(PaneId, f32, f32)> = None;

        for (id, candidate) in panes {
            if id == pane_id {
                continue;
            }
            let Some(metric) = NeighborMetric::from(current, candidate, direction) else {
                continue;
            };

            let replace_best_any = best_any
                .as_ref()
                .map(|(_, p, s)| is_better(metric.primary, metric.secondary, *p, *s))
                .unwrap_or(true);
            if replace_best_any {
                best_any = Some((id, metric.primary, metric.secondary));
            }

            if metric.overlap > EPSILON {
                let replace_best_overlap = best_overlap
                    .as_ref()
                    .map(|(_, p, s)| is_better(metric.primary, metric.secondary, *p, *s))
                    .unwrap_or(true);
                if replace_best_overlap {
                    best_overlap = Some((id, metric.primary, metric.secondary));
                }
            }
        }

        best_overlap.or(best_any).map(|(id, _, _)| id)
    }

    fn pane_rects(&self) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        Self::collect_rects(
            &self.root,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            &mut out,
        );
        out
    }

    fn collect_rects(node: &LayoutNode, rect: Rect, out: &mut Vec<(PaneId, Rect)>) {
        match node {
            LayoutNode::Pane(id) => out.push((*id, rect)),
            LayoutNode::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let ratio = ratio.clamp(MIN_SPLIT_RATIO, 1.0 - MIN_SPLIT_RATIO);
                match axis {
                    SplitAxis::Vertical => {
                        let first_w = rect.w * ratio;
                        let first_rect = Rect {
                            x: rect.x,
                            y: rect.y,
                            w: first_w,
                            h: rect.h,
                        };
                        let second_rect = Rect {
                            x: rect.x + first_w,
                            y: rect.y,
                            w: rect.w - first_w,
                            h: rect.h,
                        };
                        Self::collect_rects(first, first_rect, out);
                        Self::collect_rects(second, second_rect, out);
                    }
                    SplitAxis::Horizontal => {
                        let first_h = rect.h * ratio;
                        let first_rect = Rect {
                            x: rect.x,
                            y: rect.y,
                            w: rect.w,
                            h: first_h,
                        };
                        let second_rect = Rect {
                            x: rect.x,
                            y: rect.y + first_h,
                            w: rect.w,
                            h: rect.h - first_h,
                        };
                        Self::collect_rects(first, first_rect, out);
                        Self::collect_rects(second, second_rect, out);
                    }
                }
            }
        }
    }

    pub fn close_focused(&mut self) -> bool {
        if self.pane_ids().len() <= 1 {
            return false;
        }

        let target = self.focused;
        let root = std::mem::replace(&mut self.root, LayoutNode::Pane(target));
        let (new_root, removed, next_focus) = Self::remove_node(root, target);
        self.root = new_root;

        if !removed {
            return false;
        }

        self.focused = next_focus.unwrap_or_else(|| self.root.first_pane());
        if self.maximized == Some(target)
            || self
                .maximized
                .is_some_and(|maximized| !self.root.contains_pane(maximized))
        {
            self.maximized = None;
        }
        true
    }

    fn remove_node(node: LayoutNode, target: PaneId) -> (LayoutNode, bool, Option<PaneId>) {
        match node {
            LayoutNode::Pane(id) => (LayoutNode::Pane(id), false, None),
            LayoutNode::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let first_node = *first;
                let second_node = *second;

                if matches!(first_node, LayoutNode::Pane(id) if id == target) {
                    let focus = second_node.first_pane();
                    return (second_node, true, Some(focus));
                }
                if matches!(second_node, LayoutNode::Pane(id) if id == target) {
                    let focus = first_node.first_pane();
                    return (first_node, true, Some(focus));
                }

                let (new_first, removed_first, focus_first) = Self::remove_node(first_node, target);
                if removed_first {
                    return (
                        LayoutNode::Split {
                            axis,
                            ratio,
                            first: Box::new(new_first),
                            second: Box::new(second_node),
                        },
                        true,
                        focus_first,
                    );
                }

                let (new_second, removed_second, focus_second) =
                    Self::remove_node(second_node, target);
                (
                    LayoutNode::Split {
                        axis,
                        ratio,
                        first: Box::new(new_first),
                        second: Box::new(new_second),
                    },
                    removed_second,
                    focus_second,
                )
            }
        }
    }

    pub fn toggle_maximize_focused(&mut self) -> bool {
        if self.maximized == Some(self.focused) {
            self.maximized = None;
            false
        } else {
            self.maximized = Some(self.focused);
            true
        }
    }

    pub fn balance_panes(&mut self) {
        self.root.balance();
    }
}

fn is_better(primary: f32, secondary: f32, best_primary: f32, best_secondary: f32) -> bool {
    primary < best_primary - EPSILON
        || ((primary - best_primary).abs() <= EPSILON && secondary < best_secondary)
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn right(self) -> f32 {
        self.x + self.w
    }

    fn bottom(self) -> f32 {
        self.y + self.h
    }

    fn center_x(self) -> f32 {
        self.x + (self.w / 2.0)
    }

    fn center_y(self) -> f32 {
        self.y + (self.h / 2.0)
    }
}

#[derive(Debug, Clone, Copy)]
struct NeighborMetric {
    primary: f32,
    secondary: f32,
    overlap: f32,
}

impl NeighborMetric {
    fn from(current: Rect, candidate: Rect, direction: Direction) -> Option<Self> {
        match direction {
            Direction::Left => {
                if candidate.right() > current.x + EPSILON {
                    return None;
                }
                Some(Self {
                    primary: (current.x - candidate.right()).max(0.0),
                    secondary: (current.center_y() - candidate.center_y()).abs(),
                    overlap: overlap_1d(
                        current.y,
                        current.bottom(),
                        candidate.y,
                        candidate.bottom(),
                    ),
                })
            }
            Direction::Right => {
                if candidate.x < current.right() - EPSILON {
                    return None;
                }
                Some(Self {
                    primary: (candidate.x - current.right()).max(0.0),
                    secondary: (current.center_y() - candidate.center_y()).abs(),
                    overlap: overlap_1d(
                        current.y,
                        current.bottom(),
                        candidate.y,
                        candidate.bottom(),
                    ),
                })
            }
            Direction::Up => {
                if candidate.bottom() > current.y + EPSILON {
                    return None;
                }
                Some(Self {
                    primary: (current.y - candidate.bottom()).max(0.0),
                    secondary: (current.center_x() - candidate.center_x()).abs(),
                    overlap: overlap_1d(current.x, current.right(), candidate.x, candidate.right()),
                })
            }
            Direction::Down => {
                if candidate.y < current.bottom() - EPSILON {
                    return None;
                }
                Some(Self {
                    primary: (candidate.y - current.bottom()).max(0.0),
                    secondary: (current.center_x() - candidate.center_x()).abs(),
                    overlap: overlap_1d(current.x, current.right(), candidate.x, candidate.right()),
                })
            }
        }
    }
}

fn overlap_1d(a_start: f32, a_end: f32, b_start: f32, b_end: f32) -> f32 {
    (a_end.min(b_end) - a_start.max(b_start)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_creates_new_focused_pane() {
        let mut layout = WindowLayout::new();
        let first = layout.focused();

        let second = layout.split_vertical();
        assert_eq!(layout.focused(), second);
        assert_eq!(layout.pane_ids(), vec![first, second]);

        let third = layout.split_horizontal();
        assert_eq!(layout.focused(), third);
        assert_eq!(layout.pane_ids(), vec![first, second, third]);
    }

    #[test]
    fn focus_moves_with_hjkl_direction_semantics() {
        let mut layout = WindowLayout::new();
        let top_left = layout.focused();
        let right = layout.split_vertical();
        assert!(layout.move_focus(Direction::Left));
        let bottom_left = layout.split_horizontal();
        assert_eq!(layout.focused(), bottom_left);

        assert!(layout.move_focus(Direction::Up));
        assert_eq!(layout.focused(), top_left);

        assert!(layout.move_focus(Direction::Right));
        assert_eq!(layout.focused(), right);

        assert!(layout.move_focus(Direction::Left));
        assert_eq!(layout.focused(), top_left);

        assert!(layout.move_focus(Direction::Down));
        assert_eq!(layout.focused(), bottom_left);

        assert!(layout.move_focus(Direction::Right));
        assert_eq!(layout.focused(), right);

        assert!(!layout.move_focus(Direction::Down));
    }

    #[test]
    fn close_focused_promotes_sibling_and_keeps_last_pane() {
        let mut layout = WindowLayout::new();
        let top_left = layout.focused();
        let right = layout.split_vertical();
        assert!(layout.move_focus(Direction::Left));
        let bottom_left = layout.split_horizontal();
        assert_eq!(layout.focused(), bottom_left);

        assert!(layout.close_focused());
        assert_eq!(layout.focused(), top_left);
        assert_eq!(layout.pane_ids(), vec![top_left, right]);

        assert!(layout.close_focused());
        assert_eq!(layout.focused(), right);
        assert_eq!(layout.pane_ids(), vec![right]);

        assert!(!layout.close_focused());
        assert_eq!(layout.pane_ids(), vec![right]);
    }

    #[test]
    fn maximize_toggle_tracks_focused_pane() {
        let mut layout = WindowLayout::new();
        let left = layout.focused();
        let right = layout.split_vertical();
        assert_eq!(layout.maximized(), None);

        assert!(layout.toggle_maximize_focused());
        assert_eq!(layout.maximized(), Some(right));

        assert!(!layout.toggle_maximize_focused());
        assert_eq!(layout.maximized(), None);

        assert!(layout.move_focus(Direction::Left));
        assert!(layout.toggle_maximize_focused());
        assert_eq!(layout.maximized(), Some(left));

        assert!(layout.move_focus(Direction::Right));
        assert!(layout.toggle_maximize_focused());
        assert_eq!(layout.maximized(), Some(right));
    }

    #[test]
    fn balance_resets_split_ratios() {
        let mut layout = WindowLayout::new();
        layout.split_vertical();
        layout.split_horizontal();

        if let LayoutNode::Split { ratio, second, .. } = &mut layout.root {
            *ratio = 0.2;
            if let LayoutNode::Split { ratio, .. } = second.as_mut() {
                *ratio = 0.8;
            }
        }

        layout.balance_panes();
        for ratio in layout.split_ratios() {
            assert!(
                (ratio - 0.5).abs() < 0.000_1,
                "expected 0.5 ratio, got {ratio}"
            );
        }
    }
}
