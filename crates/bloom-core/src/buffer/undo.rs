use crate::types::UndoNodeId;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct UndoNode {
    id: UndoNodeId,
    parent: Option<UndoNodeId>,
    children: Vec<UndoNodeId>,
    snapshot: ropey::Rope,
    timestamp: Instant,
    /// Epoch milliseconds — for persistence (Instant can't be serialized).
    epoch_ms: i64,
    description: String,
}

/// Branching undo tree. Persisted to SQLite on session save, restored on launch.
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
            epoch_ms: now_epoch_ms(),
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
            epoch_ms: now_epoch_ms(),
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

    /// Number of nodes in the tree.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    // -- Persistence --

    /// Save the undo tree to SQLite. Creates/replaces all rows for this page.
    pub fn save_to_db(
        &self,
        conn: &rusqlite::Connection,
        page_id: &str,
    ) -> Result<(), rusqlite::Error> {
        // Clear existing entries for this page.
        conn.execute("DELETE FROM undo_tree WHERE page_id = ?1", [page_id])?;

        let mut stmt = conn.prepare(
            "INSERT INTO undo_tree (page_id, node_id, parent_id, content, timestamp_ms, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;

        for node in &self.nodes {
            let content: String = node.snapshot.to_string();
            let parent_id: Option<i64> = node.parent.map(|p| p as i64);
            stmt.execute(rusqlite::params![
                page_id,
                node.id as i64,
                parent_id,
                content,
                node.epoch_ms,
                node.description,
            ])?;
        }

        // Store the current node ID.
        conn.execute(
            "INSERT OR REPLACE INTO undo_tree_state (page_id, current_node_id)
             VALUES (?1, ?2)",
            rusqlite::params![page_id, self.current as i64],
        )?;

        Ok(())
    }

    /// Load an undo tree from SQLite. Returns None if no data exists for this page.
    pub fn load_from_db(
        conn: &rusqlite::Connection,
        page_id: &str,
    ) -> Result<Option<Self>, rusqlite::Error> {
        // Get the current node ID.
        let current: Option<i64> = conn
            .query_row(
                "SELECT current_node_id FROM undo_tree_state WHERE page_id = ?1",
                [page_id],
                |row| row.get(0),
            )
            .optional()?;

        let Some(current_id) = current else {
            return Ok(None);
        };

        // Load all nodes.
        let mut stmt = conn.prepare(
            "SELECT node_id, parent_id, content, timestamp_ms, description
             FROM undo_tree WHERE page_id = ?1 ORDER BY node_id ASC",
        )?;

        let rows = stmt.query_map([page_id], |row| {
            let node_id: i64 = row.get(0)?;
            let parent_id: Option<i64> = row.get(1)?;
            let content: String = row.get(2)?;
            let timestamp_ms: i64 = row.get(3)?;
            let description: String = row.get(4)?;
            Ok((node_id, parent_id, content, timestamp_ms, description))
        })?;

        let mut nodes: Vec<UndoNode> = Vec::new();
        for row in rows {
            let (node_id, parent_id, content, timestamp_ms, description) = row?;
            nodes.push(UndoNode {
                id: node_id as UndoNodeId,
                parent: parent_id.map(|p| p as UndoNodeId),
                children: Vec::new(),
                snapshot: ropey::Rope::from_str(&content),
                timestamp: Instant::now(), // approximate — real time is lost
                epoch_ms: timestamp_ms,
                description,
            });
        }

        if nodes.is_empty() {
            return Ok(None);
        }

        // Rebuild children from parent pointers.
        let len = nodes.len();
        for i in 0..len {
            if let Some(parent) = nodes[i].parent {
                let child_id = nodes[i].id;
                nodes[parent as usize].children.push(child_id);
            }
        }

        Ok(Some(UndoTree {
            nodes,
            current: current_id as UndoNodeId,
        }))
    }
}

use rusqlite::OptionalExtension;

fn now_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Create the undo persistence tables if they don't exist.
pub fn create_undo_tables(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS undo_tree (
            page_id      TEXT NOT NULL,
            node_id      INTEGER NOT NULL,
            parent_id    INTEGER,
            content      TEXT NOT NULL,
            timestamp_ms INTEGER NOT NULL,
            description  TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (page_id, node_id)
        );
        CREATE TABLE IF NOT EXISTS undo_tree_state (
            page_id         TEXT PRIMARY KEY,
            current_node_id INTEGER NOT NULL
        );",
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_undo_tables(&conn).unwrap();
        conn
    }

    #[test]
    fn round_trip_simple_tree() {
        let conn = setup_db();
        let mut tree = UndoTree::new(ropey::Rope::from_str("initial"));
        tree.push(ropey::Rope::from_str("after edit 1"), "edit 1".into());
        tree.push(ropey::Rope::from_str("after edit 2"), "edit 2".into());

        tree.save_to_db(&conn, "page1").unwrap();

        let restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();
        assert_eq!(restored.node_count(), 3);
        assert_eq!(restored.current(), 2);

        // Verify content at current node.
        let snapshot = restored.nodes[restored.current() as usize].snapshot.to_string();
        assert_eq!(snapshot, "after edit 2");
    }

    #[test]
    fn round_trip_with_branching() {
        let conn = setup_db();
        let mut tree = UndoTree::new(ropey::Rope::from_str("root"));
        tree.push(ropey::Rope::from_str("branch A"), "edit A".into());
        // Undo back to root, then create a branch.
        tree.undo();
        tree.push(ropey::Rope::from_str("branch B"), "edit B".into());

        assert_eq!(tree.node_count(), 3);
        // Current is node 2 (branch B).
        assert_eq!(tree.current(), 2);
        // Root (node 0) has 2 children.
        assert_eq!(tree.children(0).len(), 2);

        tree.save_to_db(&conn, "page1").unwrap();

        let restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();
        assert_eq!(restored.node_count(), 3);
        assert_eq!(restored.current(), 2);
        assert_eq!(restored.children(0).len(), 2);

        // Verify branch B content.
        let snapshot = restored.nodes[2].snapshot.to_string();
        assert_eq!(snapshot, "branch B");
    }

    #[test]
    fn round_trip_preserves_undo_redo() {
        let conn = setup_db();
        let mut tree = UndoTree::new(ropey::Rope::from_str("v0"));
        tree.push(ropey::Rope::from_str("v1"), "edit 1".into());
        tree.push(ropey::Rope::from_str("v2"), "edit 2".into());

        // Undo to v1.
        tree.undo();
        assert_eq!(tree.current(), 1);

        tree.save_to_db(&conn, "page1").unwrap();

        let mut restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();
        assert_eq!(restored.current(), 1);

        // Redo should work — move to v2.
        let snapshot = restored.redo().unwrap();
        assert_eq!(snapshot.to_string(), "v2");
        assert_eq!(restored.current(), 2);

        // Undo twice — back to v0.
        restored.undo();
        let snapshot = restored.undo().unwrap();
        assert_eq!(snapshot.to_string(), "v0");
    }

    #[test]
    fn load_nonexistent_returns_none() {
        let conn = setup_db();
        let result = UndoTree::load_from_db(&conn, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn save_overwrites_previous() {
        let conn = setup_db();
        let mut tree = UndoTree::new(ropey::Rope::from_str("first"));
        tree.push(ropey::Rope::from_str("second"), "e1".into());
        tree.save_to_db(&conn, "page1").unwrap();

        // New tree with different content.
        let tree2 = UndoTree::new(ropey::Rope::from_str("replaced"));
        tree2.save_to_db(&conn, "page1").unwrap();

        let restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();
        assert_eq!(restored.node_count(), 1);
        assert_eq!(restored.nodes[0].snapshot.to_string(), "replaced");
    }
}
