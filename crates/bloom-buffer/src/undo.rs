use crate::{Cursor, EditDelta, UndoNodeData, UndoNodeId, UndoPersistData};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const KEYFRAME_INTERVAL: usize = 50;

struct UndoNode {
    id: UndoNodeId,
    parent: Option<UndoNodeId>,
    children: Vec<UndoNodeId>,
    snapshot: ropey::Rope,
    before_cursor: Cursor,
    after_cursor: Cursor,
    timestamp: Instant,
    /// Epoch milliseconds — for persistence (Instant can't be serialized).
    epoch_ms: i64,
    description: String,
    edit_delta: Option<EditDelta>,
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
    pub fn new(initial_snapshot: ropey::Rope) -> Self {
        let initial_cursor = Cursor::new(0);
        let root = UndoNode {
            id: 0,
            parent: None,
            children: Vec::new(),
            snapshot: initial_snapshot,
            before_cursor: initial_cursor,
            after_cursor: initial_cursor,
            timestamp: Instant::now(),
            epoch_ms: now_epoch_ms(),
            description: String::from("initial"),
            edit_delta: None,
        };
        UndoTree {
            nodes: vec![root],
            current: 0,
        }
    }

    pub fn current(&self) -> UndoNodeId {
        self.current
    }

    /// Get the content of the current node as a string (for comparison).
    pub fn current_snapshot_string(&self) -> String {
        self.nodes[self.current as usize].snapshot.to_string()
    }

    /// Get the content of a specific node as a string.
    pub fn node_snapshot_string(&self, node_id: UndoNodeId) -> String {
        self.nodes[node_id as usize].snapshot.to_string()
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
    /// Backward-compat wrapper — calls `push_with_delta` with no delta.
    pub fn push(
        &mut self,
        snapshot: ropey::Rope,
        cursor_pos: usize,
        description: String,
    ) -> UndoNodeId {
        let cursor = Cursor::new(cursor_pos);
        self.push_with_delta(snapshot, cursor, cursor, description, None)
    }

    /// Push a new snapshot with an optional edit delta.
    pub fn push_with_delta(
        &mut self,
        snapshot: ropey::Rope,
        before_cursor: Cursor,
        after_cursor: Cursor,
        description: String,
        edit_delta: Option<EditDelta>,
    ) -> UndoNodeId {
        let new_id = self.nodes.len() as UndoNodeId;
        let new_node = UndoNode {
            id: new_id,
            parent: Some(self.current),
            children: Vec::new(),
            snapshot,
            before_cursor,
            after_cursor,
            timestamp: Instant::now(),
            epoch_ms: now_epoch_ms(),
            description,
            edit_delta,
        };
        self.nodes.push(new_node);
        self.nodes[self.current as usize].children.push(new_id);
        self.current = new_id;
        new_id
    }

    /// Move to parent node. Returns `(rope, cursor)` using the undone edge's `before` snapshot.
    pub fn undo(&mut self) -> Option<(ropey::Rope, Cursor)> {
        let child = &self.nodes[self.current as usize];
        let parent = child.parent?;
        let restore_cursor = child.before_cursor;
        self.current = parent;
        let node = &self.nodes[parent as usize];
        Some((node.snapshot.clone(), restore_cursor))
    }

    /// Move to the most recent child. Returns `(rope, cursor)` using the redone edge's `after` snapshot.
    pub fn redo(&mut self) -> Option<(ropey::Rope, Cursor)> {
        let children = &self.nodes[self.current as usize].children;
        let &last_child = children.last()?;
        self.current = last_child;
        let node = &self.nodes[last_child as usize];
        Some((node.snapshot.clone(), node.after_cursor))
    }

    /// Restore to an arbitrary node. Returns `(rope, cursor)` using the node's canonical landing.
    pub fn restore(&mut self, node_id: UndoNodeId) -> (ropey::Rope, Cursor) {
        assert!((node_id as usize) < self.nodes.len(), "invalid UndoNodeId");
        self.current = node_id;
        let node = &self.nodes[node_id as usize];
        (node.snapshot.clone(), node.after_cursor)
    }

    /// Number of nodes in the tree.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    // -- Persistence --

    /// Serialize the undo tree to data for the indexer to persist.
    pub fn to_persist_data(&self, page_id: &str) -> UndoPersistData {
        let nodes = self
            .nodes
            .iter()
            .map(|n| {
                let is_root = n.parent.is_none();
                let is_keyframe = (n.id as usize) % KEYFRAME_INTERVAL == 0;
                if is_root || is_keyframe {
                    UndoNodeData {
                        node_id: n.id as i64,
                        parent_id: n.parent.map(|p| p as i64),
                        content: Some(n.snapshot.to_string()),
                        delta_offset: None,
                        delta_del_len: None,
                        delta_insert: None,
                        before_cursor_pos: Some(n.before_cursor.position as i64),
                        before_cursor_anchor: n.before_cursor.anchor.map(|a| a as i64),
                        after_cursor_pos: Some(n.after_cursor.position as i64),
                        after_cursor_anchor: n.after_cursor.anchor.map(|a| a as i64),
                        timestamp_ms: n.epoch_ms,
                        description: n.description.clone(),
                    }
                } else if let Some(ref delta) = n.edit_delta {
                    UndoNodeData {
                        node_id: n.id as i64,
                        parent_id: n.parent.map(|p| p as i64),
                        content: None,
                        delta_offset: Some(delta.offset as i64),
                        delta_del_len: Some(delta.delete_len as i64),
                        delta_insert: Some(delta.insert_text.clone()),
                        before_cursor_pos: Some(n.before_cursor.position as i64),
                        before_cursor_anchor: n.before_cursor.anchor.map(|a| a as i64),
                        after_cursor_pos: Some(n.after_cursor.position as i64),
                        after_cursor_anchor: n.after_cursor.anchor.map(|a| a as i64),
                        timestamp_ms: n.epoch_ms,
                        description: n.description.clone(),
                    }
                } else {
                    // Fallback: no delta available (old push() path), store content
                    UndoNodeData {
                        node_id: n.id as i64,
                        parent_id: n.parent.map(|p| p as i64),
                        content: Some(n.snapshot.to_string()),
                        delta_offset: None,
                        delta_del_len: None,
                        delta_insert: None,
                        before_cursor_pos: Some(n.before_cursor.position as i64),
                        before_cursor_anchor: n.before_cursor.anchor.map(|a| a as i64),
                        after_cursor_pos: Some(n.after_cursor.position as i64),
                        after_cursor_anchor: n.after_cursor.anchor.map(|a| a as i64),
                        timestamp_ms: n.epoch_ms,
                        description: n.description.clone(),
                    }
                }
            })
            .collect();
        UndoPersistData {
            page_id: page_id.to_string(),
            nodes,
            current_node_id: self.current as i64,
        }
    }

    /// Save the undo tree to SQLite.
    pub fn save_to_db(
        &self,
        conn: &rusqlite::Connection,
        page_id: &str,
    ) -> Result<(), rusqlite::Error> {
        conn.execute("DELETE FROM undo_tree WHERE page_id = ?1", [page_id])?;

        let mut stmt = conn.prepare(
            "INSERT INTO undo_tree (page_id, node_id, parent_id, content, delta_offset, delta_del_len, delta_insert, before_cursor_pos, before_cursor_anchor, after_cursor_pos, after_cursor_anchor, timestamp_ms, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?;

        for node in &self.nodes {
            let parent_id: Option<i64> = node.parent.map(|p| p as i64);
            let is_root = node.parent.is_none();
            let is_keyframe = (node.id as usize) % KEYFRAME_INTERVAL == 0;

            if is_root || is_keyframe {
                stmt.execute(rusqlite::params![
                    page_id,
                    node.id as i64,
                    parent_id,
                    node.snapshot.to_string(),
                    Option::<i64>::None,
                    Option::<i64>::None,
                    Option::<String>::None,
                    node.before_cursor.position as i64,
                    node.before_cursor.anchor.map(|a| a as i64),
                    node.after_cursor.position as i64,
                    node.after_cursor.anchor.map(|a| a as i64),
                    node.epoch_ms,
                    node.description,
                ])?;
            } else if let Some(ref delta) = node.edit_delta {
                stmt.execute(rusqlite::params![
                    page_id,
                    node.id as i64,
                    parent_id,
                    Option::<String>::None,
                    delta.offset as i64,
                    delta.delete_len as i64,
                    delta.insert_text,
                    node.before_cursor.position as i64,
                    node.before_cursor.anchor.map(|a| a as i64),
                    node.after_cursor.position as i64,
                    node.after_cursor.anchor.map(|a| a as i64),
                    node.epoch_ms,
                    node.description,
                ])?;
            } else {
                // Fallback: no delta available, store full content
                stmt.execute(rusqlite::params![
                    page_id,
                    node.id as i64,
                    parent_id,
                    node.snapshot.to_string(),
                    Option::<i64>::None,
                    Option::<i64>::None,
                    Option::<String>::None,
                    node.before_cursor.position as i64,
                    node.before_cursor.anchor.map(|a| a as i64),
                    node.after_cursor.position as i64,
                    node.after_cursor.anchor.map(|a| a as i64),
                    node.epoch_ms,
                    node.description,
                ])?;
            }
        }

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
        use rusqlite::OptionalExtension;

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

        let mut stmt = conn.prepare(
            "SELECT node_id, parent_id, content, delta_offset, delta_del_len, delta_insert, before_cursor_pos, before_cursor_anchor, after_cursor_pos, after_cursor_anchor, timestamp_ms, description
             FROM undo_tree WHERE page_id = ?1 ORDER BY node_id ASC",
        )?;

        struct RawRow {
            node_id: i64,
            parent_id: Option<i64>,
            content: Option<String>,
            delta_offset: Option<i64>,
            delta_del_len: Option<i64>,
            delta_insert: Option<String>,
            before_cursor_pos: Option<i64>,
            before_cursor_anchor: Option<i64>,
            after_cursor_pos: Option<i64>,
            after_cursor_anchor: Option<i64>,
            timestamp_ms: i64,
            description: String,
        }

        let rows: Vec<RawRow> = stmt
            .query_map([page_id], |row| {
                Ok(RawRow {
                    node_id: row.get(0)?,
                    parent_id: row.get(1)?,
                    content: row.get(2)?,
                    delta_offset: row.get(3)?,
                    delta_del_len: row.get(4)?,
                    delta_insert: row.get(5)?,
                    before_cursor_pos: row.get(6)?,
                    before_cursor_anchor: row.get(7)?,
                    after_cursor_pos: row.get(8)?,
                    after_cursor_anchor: row.get(9)?,
                    timestamp_ms: row.get(10)?,
                    description: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if rows.is_empty() {
            return Ok(None);
        }

        // Phase 1: Build nodes; populate snapshots where content is available.
        let mut nodes: Vec<UndoNode> = Vec::with_capacity(rows.len());
        let mut has_rope: Vec<bool> = Vec::with_capacity(rows.len());
        let mut deltas: Vec<Option<EditDelta>> = Vec::with_capacity(rows.len());

        for r in &rows {
            let before_cursor = Cursor {
                position: r.before_cursor_pos.unwrap_or(0) as usize,
                anchor: r.before_cursor_anchor.map(|a| a as usize),
            };
            let after_cursor_missing = r.after_cursor_pos.is_none();
            let after_cursor = Cursor {
                position: r.after_cursor_pos.unwrap_or(before_cursor.position as i64) as usize,
                anchor: if after_cursor_missing {
                    before_cursor.anchor
                } else {
                    r.after_cursor_anchor.map(|a| a as usize)
                },
            };
            let (snapshot, delta) = if let Some(ref content) = r.content {
                (ropey::Rope::from_str(content), None)
            } else if let (Some(off), Some(del), Some(ref ins)) =
                (r.delta_offset, r.delta_del_len, &r.delta_insert)
            {
                let delta = EditDelta {
                    offset: off as usize,
                    delete_len: del as usize,
                    insert_text: ins.clone(),
                };
                // Placeholder rope — will be replaced by BFS
                (ropey::Rope::from_str(""), Some(delta))
            } else {
                // Corrupted row: no content and no delta
                eprintln!(
                    "bloom-buffer: node {} has no content and no delta; using empty rope",
                    r.node_id
                );
                (ropey::Rope::from_str(""), None)
            };

            has_rope.push(r.content.is_some());
            deltas.push(delta.clone());

            nodes.push(UndoNode {
                id: r.node_id as UndoNodeId,
                parent: r.parent_id.map(|p| p as UndoNodeId),
                children: Vec::new(),
                snapshot,
                before_cursor,
                after_cursor,
                timestamp: Instant::now(),
                epoch_ms: r.timestamp_ms,
                description: r.description.clone(),
                edit_delta: delta,
            });
        }

        // Phase 2: Rebuild children from parent pointers.
        let len = nodes.len();
        for i in 0..len {
            if let Some(parent) = nodes[i].parent {
                let child_id = nodes[i].id;
                nodes[parent as usize].children.push(child_id);
            }
        }

        // Phase 3: BFS to reconstruct ropes for delta nodes.
        let mut queue = std::collections::VecDeque::new();
        for i in 0..len {
            if has_rope[i] {
                queue.push_back(i);
            }
        }

        while let Some(idx) = queue.pop_front() {
            let child_ids: Vec<UndoNodeId> = nodes[idx].children.clone();
            for &child_id in &child_ids {
                let ci = child_id as usize;
                if !has_rope[ci] {
                    if let Some(ref delta) = deltas[ci] {
                        let mut child_rope = nodes[idx].snapshot.clone();
                        let del_end = delta.offset + delta.delete_len;
                        if del_end <= child_rope.len_chars() {
                            child_rope.remove(delta.offset..del_end);
                        }
                        if !delta.insert_text.is_empty() && delta.offset <= child_rope.len_chars() {
                            child_rope.insert(delta.offset, &delta.insert_text);
                        }
                        nodes[ci].snapshot = child_rope;
                    }
                    has_rope[ci] = true;
                    queue.push_back(ci);
                }
            }
        }

        Ok(Some(UndoTree {
            nodes,
            current: current_id as UndoNodeId,
        }))
    }
}

fn now_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Compute the minimal single-edit delta between two ropes.
pub fn compute_diff(old: &ropey::Rope, new: &ropey::Rope) -> EditDelta {
    let old_len = old.len_chars();
    let new_len = new.len_chars();
    let prefix = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let max_suffix = old_len.min(new_len) - prefix;
    let suffix = (0..max_suffix)
        .take_while(|&i| old.char(old_len - 1 - i) == new.char(new_len - 1 - i))
        .count();
    EditDelta {
        offset: prefix,
        delete_len: old_len - prefix - suffix,
        insert_text: new.slice(prefix..new_len - suffix).to_string(),
    }
}

/// Create the undo persistence tables if they don't exist.
pub fn create_undo_tables(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS undo_tree (
            page_id        TEXT NOT NULL,
            node_id        INTEGER NOT NULL,
            parent_id      INTEGER,
            content        TEXT,
            delta_offset   INTEGER,
            delta_del_len  INTEGER,
            delta_insert   TEXT,
            before_cursor_pos INTEGER,
            before_cursor_anchor INTEGER,
            after_cursor_pos INTEGER,
            after_cursor_anchor INTEGER,
            timestamp_ms   INTEGER NOT NULL,
            description    TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (page_id, node_id)
        );
        CREATE TABLE IF NOT EXISTS undo_tree_state (
            page_id         TEXT PRIMARY KEY,
            current_node_id INTEGER NOT NULL
        );",
    )?;
    let _ = conn.execute_batch("ALTER TABLE undo_tree ADD COLUMN before_cursor_pos INTEGER");
    let _ = conn.execute_batch("ALTER TABLE undo_tree ADD COLUMN before_cursor_anchor INTEGER");
    let _ = conn.execute_batch("ALTER TABLE undo_tree ADD COLUMN after_cursor_pos INTEGER");
    let _ = conn.execute_batch("ALTER TABLE undo_tree ADD COLUMN after_cursor_anchor INTEGER");
    Ok(())
}

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
        tree.push(ropey::Rope::from_str("after edit 1"), 0, "edit 1".into());
        tree.push(ropey::Rope::from_str("after edit 2"), 0, "edit 2".into());
        tree.save_to_db(&conn, "page1").unwrap();

        let restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();
        assert_eq!(restored.node_count(), 3);
        assert_eq!(restored.current(), 2);
    }

    #[test]
    fn round_trip_with_branching() {
        let conn = setup_db();
        let mut tree = UndoTree::new(ropey::Rope::from_str("root"));
        tree.push(ropey::Rope::from_str("branch A"), 0, "edit A".into());
        tree.undo();
        tree.push(ropey::Rope::from_str("branch B"), 0, "edit B".into());
        assert_eq!(tree.children(0).len(), 2);

        tree.save_to_db(&conn, "page1").unwrap();
        let restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();
        assert_eq!(restored.children(0).len(), 2);
    }

    #[test]
    fn load_nonexistent_returns_none() {
        let conn = setup_db();
        assert!(UndoTree::load_from_db(&conn, "nonexistent")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_delta_round_trip() {
        let conn = setup_db();
        let initial = ropey::Rope::from_str("hello world");
        let mut tree = UndoTree::new(initial.clone());
        let cursor = Cursor::new(0);

        let after1 = ropey::Rope::from_str("hello brave world");
        let delta1 = compute_diff(&initial, &after1);
        tree.push_with_delta(
            after1.clone(),
            cursor,
            cursor,
            "insert brave".into(),
            Some(delta1),
        );

        let after2 = ropey::Rope::from_str("hello brave new world");
        let delta2 = compute_diff(&after1, &after2);
        tree.push_with_delta(after2, cursor, cursor, "insert new".into(), Some(delta2));

        tree.save_to_db(&conn, "page1").unwrap();
        let restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();

        assert_eq!(restored.node_count(), 3);
        assert_eq!(restored.node_snapshot_string(0), "hello world");
        assert_eq!(restored.node_snapshot_string(1), "hello brave world");
        assert_eq!(restored.node_snapshot_string(2), "hello brave new world");
    }

    #[test]
    fn test_edit_group_delta() {
        let old = ropey::Rope::from_str("abcdef");
        let new = ropey::Rope::from_str("abXYZef");
        let delta = compute_diff(&old, &new);
        assert_eq!(delta.offset, 2);
        assert_eq!(delta.delete_len, 2); // "cd" removed
        assert_eq!(delta.insert_text, "XYZ");
    }

    #[test]
    fn test_keyframe_persistence() {
        let conn = setup_db();
        let mut tree = UndoTree::new(ropey::Rope::from_str("root"));

        // Push 60 nodes (ids 1..=60)
        let mut prev = ropey::Rope::from_str("root");
        let cursor = Cursor::new(0);
        for i in 1..=60 {
            let text = format!("edit {i}");
            let next = ropey::Rope::from_str(&text);
            let delta = compute_diff(&prev, &next);
            tree.push_with_delta(
                next.clone(),
                cursor,
                cursor,
                format!("edit {i}"),
                Some(delta),
            );
            prev = next;
        }

        tree.save_to_db(&conn, "page1").unwrap();

        // Verify: nodes 0 and 50 should have content (keyframes), node 1 should have delta
        let has_content_0: Option<String> = conn
            .query_row(
                "SELECT content FROM undo_tree WHERE page_id='page1' AND node_id=0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(has_content_0.is_some());

        let has_content_50: Option<String> = conn
            .query_row(
                "SELECT content FROM undo_tree WHERE page_id='page1' AND node_id=50",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(has_content_50.is_some());

        let has_content_1: Option<String> = conn
            .query_row(
                "SELECT content FROM undo_tree WHERE page_id='page1' AND node_id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(has_content_1.is_none());

        let delta_offset_1: Option<i64> = conn
            .query_row(
                "SELECT delta_offset FROM undo_tree WHERE page_id='page1' AND node_id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(delta_offset_1.is_some());

        // Verify round-trip correctness
        let restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();
        assert_eq!(restored.node_count(), 61);
        assert_eq!(restored.node_snapshot_string(0), "root");
        assert_eq!(restored.node_snapshot_string(50), "edit 50");
        assert_eq!(restored.node_snapshot_string(60), "edit 60");
    }

    #[test]
    fn test_branching_delta() {
        let conn = setup_db();
        let initial = ropey::Rope::from_str("root");
        let mut tree = UndoTree::new(initial.clone());
        let cursor = Cursor::new(0);

        let branch_a = ropey::Rope::from_str("root-A");
        let delta_a = compute_diff(&initial, &branch_a);
        tree.push_with_delta(branch_a, cursor, cursor, "branch A".into(), Some(delta_a));

        tree.undo(); // back to root

        let branch_b = ropey::Rope::from_str("root-B");
        let delta_b = compute_diff(&initial, &branch_b);
        tree.push_with_delta(branch_b, cursor, cursor, "branch B".into(), Some(delta_b));

        tree.save_to_db(&conn, "page1").unwrap();
        let restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();

        assert_eq!(restored.node_count(), 3);
        assert_eq!(restored.children(0).len(), 2);
        assert_eq!(restored.node_snapshot_string(0), "root");
        assert_eq!(restored.node_snapshot_string(1), "root-A");
        assert_eq!(restored.node_snapshot_string(2), "root-B");
    }

    #[test]
    fn test_backward_compat() {
        let conn = setup_db();
        // Insert data in old format: all nodes have content, no delta columns
        conn.execute(
            "INSERT INTO undo_tree (page_id, node_id, parent_id, content, timestamp_ms, description)
             VALUES ('page1', 0, NULL, 'root text', 1000, 'initial')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO undo_tree (page_id, node_id, parent_id, content, timestamp_ms, description)
             VALUES ('page1', 1, 0, 'after edit', 1001, 'edit 1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO undo_tree_state (page_id, current_node_id) VALUES ('page1', 1)",
            [],
        )
        .unwrap();

        let restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();
        assert_eq!(restored.node_count(), 2);
        assert_eq!(restored.node_snapshot_string(0), "root text");
        assert_eq!(restored.node_snapshot_string(1), "after edit");
        assert_eq!(restored.current(), 1);
    }

    #[test]
    fn test_compute_diff_identical() {
        let rope = ropey::Rope::from_str("hello world");
        let delta = compute_diff(&rope, &rope);
        assert_eq!(delta.delete_len, 0);
        assert_eq!(delta.insert_text, "");
    }

    #[test]
    fn test_compute_diff_prefix_only() {
        let old = ropey::Rope::from_str("hello");
        let new = ropey::Rope::from_str("hello world");
        let delta = compute_diff(&old, &new);
        assert_eq!(delta.offset, 5);
        assert_eq!(delta.delete_len, 0);
        assert_eq!(delta.insert_text, " world");
    }

    #[test]
    fn test_compute_diff_suffix_only() {
        let old = ropey::Rope::from_str("world");
        let new = ropey::Rope::from_str("hello world");
        let delta = compute_diff(&old, &new);
        assert_eq!(delta.offset, 0);
        assert_eq!(delta.delete_len, 0);
        assert_eq!(delta.insert_text, "hello ");
    }

    #[test]
    fn test_deep_chain_with_keyframes() {
        let conn = setup_db();
        let mut tree = UndoTree::new(ropey::Rope::from_str("node0"));
        let cursor = Cursor::new(0);

        let mut prev = ropey::Rope::from_str("node0");
        for i in 1..=120 {
            let text = format!("node{i}");
            let next = ropey::Rope::from_str(&text);
            let delta = compute_diff(&prev, &next);
            tree.push_with_delta(
                next.clone(),
                cursor,
                cursor,
                format!("edit {i}"),
                Some(delta),
            );
            prev = next;
        }

        tree.save_to_db(&conn, "page1").unwrap();

        // Verify keyframes at 0, 50, 100 have content
        for kf in [0, 50, 100] {
            let content: Option<String> = conn
                .query_row(
                    &format!(
                        "SELECT content FROM undo_tree WHERE page_id='page1' AND node_id={kf}"
                    ),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(content.is_some(), "keyframe node {kf} should have content");
        }

        // Verify non-keyframe node 25 does NOT have content
        let content_25: Option<String> = conn
            .query_row(
                "SELECT content FROM undo_tree WHERE page_id='page1' AND node_id=25",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            content_25.is_none(),
            "non-keyframe node 25 should not have content"
        );

        // Round-trip verify all 121 nodes
        let restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();
        assert_eq!(restored.node_count(), 121);
        assert_eq!(restored.node_snapshot_string(0), "node0");
        assert_eq!(restored.node_snapshot_string(50), "node50");
        assert_eq!(restored.node_snapshot_string(100), "node100");
        assert_eq!(restored.node_snapshot_string(120), "node120");
    }

    #[test]
    fn undo_uses_child_before_cursor_and_redo_uses_child_after_cursor() {
        let mut tree = UndoTree::new(ropey::Rope::from_str("root"));
        tree.push_with_delta(
            ropey::Rope::from_str("after"),
            Cursor {
                position: 4,
                anchor: Some(2),
            },
            Cursor {
                position: 1,
                anchor: None,
            },
            "edit".into(),
            None,
        );

        let (undo_snapshot, undo_cursor) = tree.undo().unwrap();
        assert_eq!(undo_snapshot.to_string(), "root");
        assert_eq!(undo_cursor.position, 4);
        assert_eq!(undo_cursor.anchor, Some(2));

        let (redo_snapshot, redo_cursor) = tree.redo().unwrap();
        assert_eq!(redo_snapshot.to_string(), "after");
        assert_eq!(redo_cursor.position, 1);
        assert_eq!(redo_cursor.anchor, None);
    }

    #[test]
    fn round_trip_preserves_cursor_snapshots() {
        let conn = setup_db();
        let mut tree = UndoTree::new(ropey::Rope::from_str("root"));
        tree.push_with_delta(
            ropey::Rope::from_str("after"),
            Cursor {
                position: 3,
                anchor: Some(1),
            },
            Cursor {
                position: 5,
                anchor: None,
            },
            "edit".into(),
            None,
        );
        tree.save_to_db(&conn, "page1").unwrap();

        let mut restored = UndoTree::load_from_db(&conn, "page1").unwrap().unwrap();
        let (_, undo_cursor) = restored.undo().unwrap();
        assert_eq!(undo_cursor.position, 3);
        assert_eq!(undo_cursor.anchor, Some(1));

        let (_, redo_cursor) = restored.redo().unwrap();
        assert_eq!(redo_cursor.position, 5);
        assert_eq!(redo_cursor.anchor, None);
    }
}
