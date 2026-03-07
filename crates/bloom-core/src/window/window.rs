use std::collections::HashMap;

use crate::error::BloomError;
use crate::types::PaneId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone)]
pub enum LayoutTree {
    Leaf(PaneId),
    Split {
        direction: SplitDirection,
        ratio: f32,
        left: Box<LayoutTree>,
        right: Box<LayoutTree>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum PaneKind {
    Editor,
    UndoTree,
    Agenda,
    Timeline,
    SetupWizard,
}

pub struct WindowManager {
    tree: LayoutTree,
    active: PaneId,
    next_pane_id: u64,
    pane_kinds: HashMap<PaneId, PaneKind>,
    maximized: bool,
    pre_maximize_tree: Option<LayoutTree>,
    pre_maximize_active: Option<PaneId>,
}

// ---------------------------------------------------------------------------
// LayoutTree helpers
// ---------------------------------------------------------------------------

fn count_panes(tree: &LayoutTree) -> usize {
    match tree {
        LayoutTree::Leaf(_) => 1,
        LayoutTree::Split { left, right, .. } => count_panes(left) + count_panes(right),
    }
}

fn collect_pane_ids(tree: &LayoutTree, out: &mut Vec<PaneId>) {
    match tree {
        LayoutTree::Leaf(id) => out.push(*id),
        LayoutTree::Split { left, right, .. } => {
            collect_pane_ids(left, out);
            collect_pane_ids(right, out);
        }
    }
}

fn contains_pane(tree: &LayoutTree, pane: PaneId) -> bool {
    match tree {
        LayoutTree::Leaf(id) => *id == pane,
        LayoutTree::Split { left, right, .. } => {
            contains_pane(left, pane) || contains_pane(right, pane)
        }
    }
}

fn first_leaf(tree: &LayoutTree) -> PaneId {
    match tree {
        LayoutTree::Leaf(id) => *id,
        LayoutTree::Split { left, .. } => first_leaf(left),
    }
}

/// Replace the leaf `target` with `replacement` in-place, returning true if found.
fn replace_leaf(tree: &mut LayoutTree, target: PaneId, replacement: LayoutTree) -> bool {
    match tree {
        LayoutTree::Leaf(id) if *id == target => {
            *tree = replacement;
            true
        }
        LayoutTree::Leaf(_) => false,
        LayoutTree::Split { left, right, .. } => {
            replace_leaf(left, target, replacement.clone())
                || replace_leaf(right, target, replacement)
        }
    }
}

/// Remove `target` pane from the tree, promoting the sibling.
/// Returns the new tree if the removal happened at the top level, or modifies in place.
fn remove_pane(tree: &mut LayoutTree, target: PaneId) -> bool {
    match tree {
        LayoutTree::Leaf(_) => false,
        LayoutTree::Split { left, right, .. } => {
            // Check if one of our direct children is the target leaf
            if matches!(left.as_ref(), LayoutTree::Leaf(id) if *id == target) {
                *tree = *right.clone();
                return true;
            }
            if matches!(right.as_ref(), LayoutTree::Leaf(id) if *id == target) {
                *tree = *left.clone();
                return true;
            }
            // Recurse
            remove_pane(left, target) || remove_pane(right, target)
        }
    }
}

fn balance_tree(tree: &mut LayoutTree) {
    if let LayoutTree::Split {
        ratio, left, right, ..
    } = tree
    {
        *ratio = 0.5;
        balance_tree(left);
        balance_tree(right);
    }
}

/// Rotate the split direction of the parent node containing `pane`.
fn rotate_parent(tree: &mut LayoutTree, pane: PaneId) -> bool {
    match tree {
        LayoutTree::Leaf(_) => false,
        LayoutTree::Split {
            direction,
            left,
            right,
            ..
        } => {
            if contains_pane(left, pane) || contains_pane(right, pane) {
                // Check if pane is a direct child
                let is_direct_child = matches!(left.as_ref(), LayoutTree::Leaf(id) if *id == pane)
                    || matches!(right.as_ref(), LayoutTree::Leaf(id) if *id == pane);
                if is_direct_child {
                    *direction = match *direction {
                        SplitDirection::Vertical => SplitDirection::Horizontal,
                        SplitDirection::Horizontal => SplitDirection::Vertical,
                    };
                    return true;
                }
                // Try deeper
                rotate_parent(left, pane) || rotate_parent(right, pane)
            } else {
                false
            }
        }
    }
}

/// Adjust the ratio of the parent split containing `pane`.
/// `delta` is in abstract units; we translate to ratio change.
fn resize_parent(tree: &mut LayoutTree, pane: PaneId, delta: i32, axis: SplitDirection) -> bool {
    match tree {
        LayoutTree::Leaf(_) => false,
        LayoutTree::Split {
            direction,
            ratio,
            left,
            right,
            ..
        } => {
            if *direction != axis {
                return resize_parent(left, pane, delta, axis)
                    || resize_parent(right, pane, delta, axis);
            }

            let in_left = contains_pane(left, pane);
            let in_right = contains_pane(right, pane);

            if !in_left && !in_right {
                return false;
            }

            // If the pane is a direct child or in a subtree, try deeper first
            let handled = if in_left {
                resize_parent(left, pane, delta, axis)
            } else {
                resize_parent(right, pane, delta, axis)
            };
            if handled {
                return true;
            }

            // This is the innermost matching split — adjust ratio
            let step = delta as f32 * 0.02;
            let new_ratio = if in_left {
                (*ratio + step).clamp(0.1, 0.9)
            } else {
                (*ratio - step).clamp(0.1, 0.9)
            };
            *ratio = new_ratio;
            true
        }
    }
}

/// Compute bounding rectangles for each pane in normalised [0,1] coordinates.
struct PaneRect {
    pane: PaneId,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn compute_rects(tree: &LayoutTree, x: f32, y: f32, w: f32, h: f32, out: &mut Vec<PaneRect>) {
    match tree {
        LayoutTree::Leaf(id) => out.push(PaneRect {
            pane: *id,
            x,
            y,
            w,
            h,
        }),
        LayoutTree::Split {
            direction,
            ratio,
            left,
            right,
        } => match direction {
            SplitDirection::Vertical => {
                let lw = w * ratio;
                let rw = w - lw;
                compute_rects(left, x, y, lw, h, out);
                compute_rects(right, x + lw, y, rw, h, out);
            }
            SplitDirection::Horizontal => {
                let lh = h * ratio;
                let rh = h - lh;
                compute_rects(left, x, y, w, lh, out);
                compute_rects(right, x, y + lh, w, rh, out);
            }
        },
    }
}

/// Swap the two direct-child leaves of the split containing `pane`.
fn swap_leaves_at_parent(tree: &mut LayoutTree, pane: PaneId) -> bool {
    match tree {
        LayoutTree::Leaf(_) => false,
        LayoutTree::Split { left, right, .. } => {
            let in_left = contains_pane(left, pane);
            let in_right = contains_pane(right, pane);
            if !in_left && !in_right {
                return false;
            }
            // If pane is a direct child, swap left and right subtrees
            let is_direct = matches!(left.as_ref(), LayoutTree::Leaf(id) if *id == pane)
                || matches!(right.as_ref(), LayoutTree::Leaf(id) if *id == pane);
            if is_direct {
                std::mem::swap(left, right);
                return true;
            }
            // Recurse into the branch containing the pane
            if in_left {
                swap_leaves_at_parent(left, pane)
            } else {
                swap_leaves_at_parent(right, pane)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// WindowManager
// ---------------------------------------------------------------------------

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowManager {
    pub fn new() -> Self {
        let first = PaneId(0);
        let mut pane_kinds = HashMap::new();
        pane_kinds.insert(first, PaneKind::Editor);
        Self {
            tree: LayoutTree::Leaf(first),
            active: first,
            next_pane_id: 1,
            pane_kinds,
            maximized: false,
            pre_maximize_tree: None,
            pre_maximize_active: None,
        }
    }

    pub fn active_pane(&self) -> PaneId {
        self.active
    }

    pub fn pane_count(&self) -> usize {
        count_panes(&self.tree)
    }

    pub fn is_maximized(&self) -> bool {
        self.maximized
    }

    pub fn hidden_pane_count(&self) -> usize {
        if self.maximized {
            if let Some(ref full_tree) = self.pre_maximize_tree {
                count_panes(full_tree) - 1
            } else {
                0
            }
        } else {
            0
        }
    }

    fn alloc_pane(&mut self) -> PaneId {
        let id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;
        id
    }

    /// Split the active pane. The active pane becomes the left child, the new
    /// pane becomes the right child.
    pub fn split(&mut self, direction: SplitDirection) -> Result<PaneId, BloomError> {
        // Only editor panes can be split
        if self.pane_kinds.get(&self.active) != Some(&PaneKind::Editor) {
            return Err(BloomError::PaneTooSmall);
        }

        let new_pane = self.alloc_pane();
        self.pane_kinds.insert(new_pane, PaneKind::Editor);

        let old_leaf = LayoutTree::Leaf(self.active);
        let new_leaf = LayoutTree::Leaf(new_pane);
        let split_node = LayoutTree::Split {
            direction,
            ratio: 0.5,
            left: Box::new(old_leaf),
            right: Box::new(new_leaf),
        };

        replace_leaf(&mut self.tree, self.active, split_node);
        self.active = new_pane;
        Ok(new_pane)
    }

    /// Close a pane. Returns false without closing if it's the last pane.
    pub fn close(&mut self, pane: PaneId) -> bool {
        if count_panes(&self.tree) <= 1 {
            return false;
        }
        let was_active = self.active == pane;
        remove_pane(&mut self.tree, pane);
        self.pane_kinds.remove(&pane);

        if was_active {
            self.active = first_leaf(&self.tree);
        }
        true
    }

    /// Close all panes except the active one.
    pub fn close_others(&mut self) {
        let keep = self.active;
        // Remove all pane_kinds except the active one
        self.pane_kinds.retain(|id, _| *id == keep);
        self.tree = LayoutTree::Leaf(keep);
    }

    /// Navigate to the nearest spatial neighbour in the given direction.
    pub fn navigate(&mut self, direction: Direction, cursor_line: usize) {
        let mut rects = Vec::new();
        compute_rects(&self.tree, 0.0, 0.0, 1.0, 1.0, &mut rects);

        let active_rect = match rects.iter().find(|r| r.pane == self.active) {
            Some(r) => r,
            None => return,
        };

        // Normalise cursor_line into the active pane's vertical extent.
        // We assume a default terminal height and map cursor_line into the pane rect.
        let cursor_y_norm = active_rect.y + active_rect.h * 0.5; // centre by default
        let _ = cursor_line; // used conceptually — with real terminal size we'd be more precise
        let cursor_x_norm = active_rect.x + active_rect.w * 0.5;

        let candidates: Vec<&PaneRect> = rects
            .iter()
            .filter(|r| r.pane != self.active)
            .filter(|r| {
                let cx = r.x + r.w * 0.5;
                let cy = r.y + r.h * 0.5;
                match direction {
                    Direction::Left => cx < active_rect.x,
                    Direction::Right => cx > active_rect.x + active_rect.w,
                    Direction::Up => cy < active_rect.y,
                    Direction::Down => cy > active_rect.y + active_rect.h,
                }
            })
            .collect();

        if let Some(best) = candidates.iter().min_by(|a, b| {
            let dist_a = match direction {
                Direction::Left | Direction::Right => ((a.y + a.h * 0.5) - cursor_y_norm).abs(),
                Direction::Up | Direction::Down => ((a.x + a.w * 0.5) - cursor_x_norm).abs(),
            };
            let dist_b = match direction {
                Direction::Left | Direction::Right => ((b.y + b.h * 0.5) - cursor_y_norm).abs(),
                Direction::Up | Direction::Down => ((b.x + b.w * 0.5) - cursor_x_norm).abs(),
            };
            dist_a
                .partial_cmp(&dist_b)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            self.active = best.pane;
        }
    }

    /// Resize the pane by adjusting the parent split ratio.
    pub fn resize(&mut self, pane: PaneId, delta: i32, axis: SplitDirection) {
        resize_parent(&mut self.tree, pane, delta, axis);
    }

    /// Balance all split ratios to 0.5.
    pub fn balance(&mut self) {
        balance_tree(&mut self.tree);
    }

    /// Toggle maximise: when maximising, collapse to single pane; when
    /// un-maximising, restore the full tree.
    pub fn maximize_toggle(&mut self) {
        if self.maximized {
            // Restore
            if let Some(full_tree) = self.pre_maximize_tree.take() {
                self.tree = full_tree;
            }
            if let Some(prev) = self.pre_maximize_active.take() {
                self.active = prev;
            }
            self.maximized = false;
        } else {
            self.pre_maximize_tree = Some(self.tree.clone());
            self.pre_maximize_active = Some(self.active);
            self.tree = LayoutTree::Leaf(self.active);
            self.maximized = true;
        }
    }

    /// Swap the active pane's position with its sibling in the parent split.
    pub fn swap_with_next(&mut self) {
        swap_leaves_at_parent(&mut self.tree, self.active);
    }

    /// Toggle the split direction (V↔H) of the parent node of the active pane.
    pub fn rotate_layout(&mut self) {
        rotate_parent(&mut self.tree, self.active);
    }

    /// Move the active pane's buffer to the neighbour in the given direction.
    /// Implemented as a navigate + swap of pane ids.
    pub fn move_buffer(&mut self, direction: Direction) {
        let source = self.active;
        self.navigate(direction, 0);
        let target = self.active;
        if target == source {
            return;
        }
        // Swap the two pane ids in the tree
        let temp = PaneId(self.next_pane_id);
        self.next_pane_id += 1;

        // source -> temp, target -> source, temp -> target
        replace_leaf(&mut self.tree, source, LayoutTree::Leaf(temp));
        replace_leaf(&mut self.tree, target, LayoutTree::Leaf(source));
        replace_leaf(&mut self.tree, temp, LayoutTree::Leaf(target));
        self.next_pane_id -= 1; // reclaim temp id

        // Swap pane_kinds
        let sk = self.pane_kinds.remove(&source);
        let tk = self.pane_kinds.remove(&target);
        if let Some(k) = sk {
            self.pane_kinds.insert(target, k);
        }
        if let Some(k) = tk {
            self.pane_kinds.insert(source, k);
        }

        // Active stays at the position we navigated to, but now holds the
        // original buffer — so set active to source (which is now at the target
        // position).
        self.active = source;
    }

    /// Open (or toggle) a special view in a new split beside the active pane.
    pub fn open_special_view(&mut self, kind: PaneKind, direction: SplitDirection) -> PaneId {
        // If a pane of this kind already exists, close it (toggle)
        if let Some(existing) = self.find_pane_by_kind(&kind) {
            self.close(existing);
            return self.active;
        }

        let new_pane = self.alloc_pane();
        self.pane_kinds.insert(new_pane, kind);

        let old_leaf = LayoutTree::Leaf(self.active);
        let new_leaf = LayoutTree::Leaf(new_pane);
        let split_node = LayoutTree::Split {
            direction,
            ratio: 0.5,
            left: Box::new(old_leaf),
            right: Box::new(new_leaf),
        };

        replace_leaf(&mut self.tree, self.active, split_node);
        new_pane
    }

    /// Find the first pane with the given kind.
    pub fn find_pane_by_kind(&self, kind: &PaneKind) -> Option<PaneId> {
        self.pane_kinds
            .iter()
            .find(|(_, k)| *k == kind)
            .map(|(id, _)| *id)
    }

    /// Get the kind of a pane.
    pub fn pane_kind(&self, pane: PaneId) -> Option<&PaneKind> {
        self.pane_kinds.get(&pane)
    }

    /// Return a reference to the current layout tree.
    pub fn layout(&self) -> &LayoutTree {
        &self.tree
    }

    /// Collect all pane ids in tree order.
    pub fn all_pane_ids(&self) -> Vec<PaneId> {
        let mut ids = Vec::new();
        collect_pane_ids(&self.tree, &mut ids);
        ids
    }

    /// Compute concrete cell rects for every pane given the total available area.
    /// Each pane gets a content area (for editor/view) and a 1-line status bar.
    pub fn compute_pane_rects(&self, total_width: u16, total_height: u16) -> Vec<CellRect> {
        let mut rects = Vec::new();
        let tree = if self.maximized {
            &LayoutTree::Leaf(self.active)
        } else {
            &self.tree
        };
        compute_cell_rects(tree, 0, 0, total_width, total_height, &mut rects);
        rects
    }
}

/// Concrete cell rect computed by the core layout engine for each pane.
#[derive(Debug, Clone, Copy)]
pub struct CellRect {
    pub pane_id: PaneId,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,         // total pane height (content + status bar)
    pub content_height: u16, // rows for content (height - 1 for status bar)
}

fn compute_cell_rects(
    tree: &LayoutTree,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    out: &mut Vec<CellRect>,
) {
    match tree {
        LayoutTree::Leaf(id) => {
            let content_h = height.saturating_sub(1);
            out.push(CellRect {
                pane_id: *id,
                x,
                y,
                width,
                height,
                content_height: content_h,
            });
        }
        LayoutTree::Split {
            direction,
            ratio,
            left,
            right,
        } => match direction {
            SplitDirection::Vertical => {
                let left_w = ((width as f32) * ratio) as u16;
                let right_w = width.saturating_sub(left_w);
                compute_cell_rects(left, x, y, left_w, height, out);
                compute_cell_rects(right, x + left_w, y, right_w, height, out);
            }
            SplitDirection::Horizontal => {
                let top_h = ((height as f32) * ratio) as u16;
                let bottom_h = height.saturating_sub(top_h);
                compute_cell_rects(left, x, y, width, top_h, out);
                compute_cell_rects(right, x, y + top_h, width, bottom_h, out);
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_single_pane() {
        let wm = WindowManager::new();
        assert_eq!(wm.pane_count(), 1);
        assert_eq!(wm.active_pane(), PaneId(0));
        assert!(!wm.is_maximized());
        assert_eq!(wm.hidden_pane_count(), 0);
    }

    #[test]
    fn split_vertical_creates_two_panes() {
        let mut wm = WindowManager::new();
        let new_id = wm.split(SplitDirection::Vertical).unwrap();
        assert_eq!(wm.pane_count(), 2);
        assert_eq!(wm.active_pane(), new_id);
    }

    #[test]
    fn close_last_pane_returns_false() {
        let mut wm = WindowManager::new();
        assert!(!wm.close(wm.active_pane()));
        assert_eq!(wm.pane_count(), 1);
    }

    #[test]
    fn close_removes_pane() {
        let mut wm = WindowManager::new();
        let first = wm.active_pane();
        let _ = wm.split(SplitDirection::Vertical).unwrap();
        assert!(wm.close(first));
        assert_eq!(wm.pane_count(), 1);
    }

    #[test]
    fn close_others_keeps_active() {
        let mut wm = WindowManager::new();
        let _ = wm.split(SplitDirection::Vertical).unwrap();
        let _ = wm.split(SplitDirection::Horizontal).unwrap();
        let active = wm.active_pane();
        wm.close_others();
        assert_eq!(wm.pane_count(), 1);
        assert_eq!(wm.active_pane(), active);
    }

    #[test]
    fn balance_sets_equal_ratios() {
        let mut wm = WindowManager::new();
        let _ = wm.split(SplitDirection::Vertical).unwrap();
        wm.resize(wm.active_pane(), 5, SplitDirection::Vertical);
        wm.balance();
        if let LayoutTree::Split { ratio, .. } = wm.layout() {
            assert!((ratio - 0.5).abs() < f32::EPSILON);
        } else {
            panic!("expected split");
        }
    }

    #[test]
    fn maximize_toggle_hides_and_restores() {
        let mut wm = WindowManager::new();
        let _ = wm.split(SplitDirection::Vertical).unwrap();
        let _ = wm.split(SplitDirection::Horizontal).unwrap();
        assert_eq!(wm.pane_count(), 3);

        wm.maximize_toggle();
        assert!(wm.is_maximized());
        assert_eq!(wm.pane_count(), 1);
        assert_eq!(wm.hidden_pane_count(), 2);

        wm.maximize_toggle();
        assert!(!wm.is_maximized());
        assert_eq!(wm.pane_count(), 3);
        assert_eq!(wm.hidden_pane_count(), 0);
    }

    #[test]
    fn navigate_moves_between_panes() {
        let mut wm = WindowManager::new();
        let first = wm.active_pane();
        let _ = wm.split(SplitDirection::Vertical).unwrap();
        // Active is the new (right) pane — navigate left goes back to first
        wm.navigate(Direction::Left, 0);
        assert_eq!(wm.active_pane(), first);
    }

    #[test]
    fn rotate_layout_toggles_direction() {
        let mut wm = WindowManager::new();
        let _ = wm.split(SplitDirection::Vertical).unwrap();
        wm.rotate_layout();
        if let LayoutTree::Split { direction, .. } = wm.layout() {
            assert_eq!(*direction, SplitDirection::Horizontal);
        } else {
            panic!("expected split");
        }
    }

    #[test]
    fn special_view_toggles() {
        let mut wm = WindowManager::new();
        let timeline = wm.open_special_view(PaneKind::Timeline, SplitDirection::Vertical);
        assert_eq!(wm.pane_count(), 2);
        assert_eq!(wm.find_pane_by_kind(&PaneKind::Timeline), Some(timeline));

        // Toggle off
        wm.open_special_view(PaneKind::Timeline, SplitDirection::Vertical);
        assert_eq!(wm.pane_count(), 1);
        assert_eq!(wm.find_pane_by_kind(&PaneKind::Timeline), None);
    }

    #[test]
    fn all_pane_ids_returns_all() {
        let mut wm = WindowManager::new();
        let _ = wm.split(SplitDirection::Vertical).unwrap();
        let ids = wm.all_pane_ids();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn swap_with_next_swaps_children() {
        let mut wm = WindowManager::new();
        let first = wm.active_pane();
        let second = wm.split(SplitDirection::Vertical).unwrap();
        wm.swap_with_next();
        let ids = wm.all_pane_ids();
        // After swap, order should be reversed
        assert_eq!(ids[0], second);
        assert_eq!(ids[1], first);
    }
}
