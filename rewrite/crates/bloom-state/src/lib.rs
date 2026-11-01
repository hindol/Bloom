//! Durable version snapshots and group-commit persistence for Bloom.

mod store;

pub use store::{
    CommitReceipt, Origin, PageId, StateError, StateStore, SubmitError, TransactionId, Version,
    VersionRecord,
};
