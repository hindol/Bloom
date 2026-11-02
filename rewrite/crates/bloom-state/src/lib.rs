//! Durable version snapshots and group-commit persistence for Bloom.

mod store;

pub use store::{
    CommitReceipt, Origin, PageId, StateError, StateSaveGuard, StateStore, StateStoreIdentity,
    SubmitError, TransactionId, Version, VersionRecord,
};
