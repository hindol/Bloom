use crate::types::UndoNodeId;
use std::time::Instant;

struct UndoNode {
    id: UndoNodeId,
    parent: Option<UndoNodeId>,
    children: Vec<UndoNodeId>,
    snapshot: ropey::Rope,
    timestamp: Instant,
    description: String,
}

/// Branching undo tree. RAM-only (not persisted).
pub struct UndoTree {
    nodes: Vec<UndoNode>,
    current: UndoNodeId,
}

pub struct UndoNodeInfo {
    pub id: UndoNodeId,
    pub timestamp: Instant,
    pub description: String,
}

impl UndoTree {
    /// Create a new undo tree with the given initial rope snapshot.
    pub(crate) fn new(initial_snapshot: ropey::Rope) -> Self {
        let root = UndoNode {
            id: 0,
            parent: None,
            children: Vec::new(),
            snapshot: initial_snapshot,
            timestamp: Instant::now(),
            description: String::from("initial"),
        };
        UndoTree {
            nodes: vec![root],
            current: 0,
        }
    }

    pub fn current(&self) -> UndoNodeId {
        self.current
    }

    pub fn parent(&self, node: UndoNodeId) -> Option<UndoNodeId> {
        self.nodes[node as usize].parent
    }

    pub fn children(&self, node: UndoNodeId) -> &[UndoNodeId] {
        &self.nodes[node as usize].children
    }

    /// Return all branches as sequences of node IDs for visualization.
    pub fn branches(&self) -> Vec<Vec<UndoNodeId>> {
        let mut result = Vec::new();
        self.collect_branches(0, &mut Vec::new(), &mut result);
        result
    }

    fn collect_branches(
        &self,
        node_id: UndoNodeId,
        path: &mut Vec<UndoNodeId>,
        result: &mut Vec<Vec<UndoNodeId>>,
    ) {
        path.push(node_id);
        let node = &self.nodes[node_id as usize];
        if node.children.is_empty() {
            result.push(path.clone());
        } else {
            for &child in &node.children {
                self.collect_branches(child, path, result);
            }
        }
        path.pop();
    }

    pub fn node_info(&self, node: UndoNodeId) -> UndoNodeInfo {
        let n = &self.nodes[node as usize];
        UndoNodeInfo {
            id: n.id,
            timestamp: n.timestamp,
            description: n.description.clone(),
        }
    }

    /// Push a new snapshot as a child of the current node. Returns the new node's ID.
    pub(crate) fn push(&mut self, snapshot: ropey::Rope, description: String) -> UndoNodeId {
        let new_id = self.nodes.len() as UndoNodeId;
        let new_node = UndoNode {
            id: new_id,
            parent: Some(self.current),
            children: Vec::new(),
            snapshot,
            timestamp: Instant::now(),
            description,
        };
        self.nodes.push(new_node);
        self.nodes[self.current as usize].children.push(new_id);
        self.current = new_id;
        new_id
    }

    /// Move to parent node. Returns the parent's rope snapshot if successful.
    pub(crate) fn undo(&mut self) -> Option<ropey::Rope> {
        let parent = self.nodes[self.current as usize].parent?;
        self.current = parent;
        Some(self.nodes[parent as usize].snapshot.clone())
    }

    /// Move to the most recent child. Returns the child's rope snapshot if successful.
    pub(crate) fn redo(&mut self) -> Option<ropey::Rope> {
        let children = &self.nodes[self.current as usize].children;
        let &last_child = children.last()?;
        self.current = last_child;
        Some(self.nodes[last_child as usize].snapshot.clone())
    }

    /// Restore to an arbitrary node. Returns that node's rope snapshot.
    pub(crate) fn restore(&mut self, node_id: UndoNodeId) -> ropey::Rope {
        assert!((node_id as usize) < self.nodes.len(), "invalid UndoNodeId");
        self.current = node_id;
        self.nodes[node_id as usize].snapshot.clone()
    }
}