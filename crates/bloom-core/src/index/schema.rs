use rusqlite::Connection;

use crate::error::BloomError;

pub(crate) fn create_tables(conn: &Connection) -> Result<(), BloomError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS pages (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created TEXT NOT NULL,
            path TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS tags (
            page_id TEXT NOT NULL,
            tag TEXT NOT NULL,
            PRIMARY KEY (page_id, tag),
            FOREIGN KEY (page_id) REFERENCES pages(id)
        );

        CREATE TABLE IF NOT EXISTS links (
            from_page TEXT NOT NULL,
            to_page TEXT NOT NULL,
            display_hint TEXT,
            line INTEGER,
            FOREIGN KEY (from_page) REFERENCES pages(id)
        );

        CREATE TABLE IF NOT EXISTS block_links (
            from_page TEXT NOT NULL,
            to_block_id TEXT NOT NULL,
            display_hint TEXT,
            line INTEGER,
            FOREIGN KEY (from_page) REFERENCES pages(id)
        );

        CREATE TABLE IF NOT EXISTS tasks (
            page_id TEXT NOT NULL,
            line INTEGER NOT NULL,
            text TEXT NOT NULL,
            done INTEGER NOT NULL DEFAULT 0,
            due_date TEXT,
            start_date TEXT,
            FOREIGN KEY (page_id) REFERENCES pages(id)
        );

        CREATE TABLE IF NOT EXISTS block_ids (
            block_id TEXT NOT NULL,
            page_id TEXT NOT NULL,
            line INTEGER NOT NULL,
            PRIMARY KEY (block_id, page_id),
            FOREIGN KEY (page_id) REFERENCES pages(id)
        );
        CREATE INDEX IF NOT EXISTS idx_block_ids_page ON block_ids(page_id);
        CREATE INDEX IF NOT EXISTS idx_block_ids_block ON block_ids(block_id);

        -- Retired block IDs are never reused. Survives index rebuilds.
        CREATE TABLE IF NOT EXISTS retired_block_ids (
            block_id TEXT PRIMARY KEY,
            retired_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS file_fingerprints (
            path TEXT PRIMARY KEY,
            mtime_secs INTEGER NOT NULL,
            size_bytes INTEGER NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS pages_fts USING fts5(
            title, content, page_id UNINDEXED
        );

        -- page_access is USER DATA, not index-derived. It stores frecency
        -- scores accumulated over time. It must NEVER be cleared or dropped
        -- during index rebuilds. Orphaned rows (page deleted) are harmless.
        CREATE TABLE IF NOT EXISTS page_access (
            page_id TEXT PRIMARY KEY,
            visit_count INTEGER NOT NULL DEFAULT 0,
            last_accessed_ms INTEGER NOT NULL DEFAULT 0,
            frecency_score REAL NOT NULL DEFAULT 0.0
        );

        -- Persistent undo tree. Serialized on session save, restored on launch.
        -- Pruned when a buffer is closed or after 24 hours.
        CREATE TABLE IF NOT EXISTS undo_tree (
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
        );
        ",
    )
    .map_err(|e| BloomError::IndexError(e.to_string()))
}
