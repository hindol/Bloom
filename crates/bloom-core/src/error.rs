use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BloomError {
    #[error("page not found: {0}")]
    PageNotFound(String),

    #[error("ambiguous match for '{query}': {candidates:?}")]
    AmbiguousMatch {
        query: String,
        candidates: Vec<String>,
    },

    #[error("text not found: '{old_text}'")]
    TextNotFound { old_text: String },

    #[error("ambiguous text '{old_text}': found {count} occurrences")]
    AmbiguousText { old_text: String, count: usize },

    #[error("read-only mode")]
    ReadOnly,

    #[error("merge conflict in {0}")]
    MergeConflict(PathBuf),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("index error: {0}")]
    IndexError(String),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("invalid page ID: {0}")]
    InvalidPageId(String),

    #[error("pane too small to split")]
    PaneTooSmall,

    #[error("cannot close last pane")]
    LastPane,

    #[error("config error: {0}")]
    ConfigError(String),

    #[error("watcher error: {0}")]
    WatcherError(#[from] notify::Error),
}