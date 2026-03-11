//! Git-backed history for Bloom vaults.
//!
//! Provides [`HistoryRepo`] — a thin wrapper around `gix` that manages
//! a UUID-based git repository inside `.index/.git/`. Files are stored
//! under their page UUID (e.g. `8f3a1b2c.md`), making history immune
//! to renames.

mod repo;

pub use repo::{CommitInfo, HistoryError, HistoryRepo};
