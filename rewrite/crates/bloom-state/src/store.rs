use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

const SCHEMA_VERSION: i64 = 1;
const COMMAND_QUEUE_CAPACITY: usize = 64;
const MAX_PENDING_VERSIONS: usize = 64;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PageId(String);

impl PageId {
    pub fn new(value: impl Into<String>) -> Result<Self, StateError> {
        let value = value.into();
        if value.is_empty() || value.contains('\0') {
            return Err(StateError::InvalidIdentifier("page id"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Version(i64);

impl Version {
    pub fn new(value: u64) -> Result<Self, StateError> {
        i64::try_from(value)
            .ok()
            .filter(|value| *value > 0)
            .map(Self)
            .ok_or(StateError::InvalidVersion)
    }

    pub fn get(self) -> u64 {
        self.0 as u64
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TransactionId(String);

impl TransactionId {
    pub fn new(value: impl Into<String>) -> Result<Self, StateError> {
        let value = value.into();
        if value.is_empty() || value.contains('\0') {
            return Err(StateError::InvalidIdentifier("transaction id"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Origin {
    User,
    Agent,
}

impl Origin {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
        }
    }

    fn parse(value: &str) -> Result<Self, StateError> {
        match value {
            "user" => Ok(Self::User),
            "agent" => Ok(Self::Agent),
            _ => Err(StateError::InvalidStoredValue("version origin")),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionRecord {
    page_id: PageId,
    version: Version,
    parent: Option<Version>,
    timestamp_ms: i64,
    transaction_id: TransactionId,
    origin: Origin,
    content: String,
    cursor_anchor: Option<Vec<u8>>,
}

impl VersionRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        page_id: PageId,
        version: Version,
        parent: Option<Version>,
        timestamp_ms: i64,
        transaction_id: TransactionId,
        origin: Origin,
        content: String,
        cursor_anchor: Option<Vec<u8>>,
    ) -> Result<Self, StateError> {
        if parent.is_some_and(|parent| parent >= version) {
            return Err(StateError::InvalidParentVersion);
        }
        Ok(Self {
            page_id,
            version,
            parent,
            timestamp_ms,
            transaction_id,
            origin,
            content,
            cursor_anchor,
        })
    }

    pub fn page_id(&self) -> &PageId {
        &self.page_id
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn parent(&self) -> Option<Version> {
        self.parent
    }

    pub fn timestamp_ms(&self) -> i64 {
        self.timestamp_ms
    }

    pub fn transaction_id(&self) -> &TransactionId {
        &self.transaction_id
    }

    pub fn origin(&self) -> Origin {
        self.origin
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn cursor_anchor(&self) -> Option<&[u8]> {
        self.cursor_anchor.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateError {
    InvalidIdentifier(&'static str),
    InvalidVersion,
    InvalidParentVersion,
    InvalidStoredValue(&'static str),
    QueueFull,
    WorkerStopped,
    WorkerPanicked,
    UnsupportedSchema(i64),
    UnversionedDatabase,
    IntegrityCheck(String),
    VersionNotFound,
    SavedTagRegression,
    Sqlite(String),
    ThreadSpawn(String),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier(kind) => write!(f, "invalid {kind}"),
            Self::InvalidVersion => f.write_str("version must be a positive SQLite integer"),
            Self::InvalidParentVersion => {
                f.write_str("parent version must be less than the new version")
            }
            Self::InvalidStoredValue(kind) => write!(f, "invalid stored {kind}"),
            Self::QueueFull => f.write_str("state writer queue is full"),
            Self::WorkerStopped => f.write_str("state writer is not available"),
            Self::WorkerPanicked => f.write_str("state writer thread panicked"),
            Self::UnsupportedSchema(version) => {
                write!(f, "state database schema {version} is not supported")
            }
            Self::UnversionedDatabase => {
                f.write_str("refusing to migrate a non-empty unversioned database")
            }
            Self::IntegrityCheck(message) => {
                write!(f, "state database integrity check failed: {message}")
            }
            Self::VersionNotFound => f.write_str("version does not exist"),
            Self::SavedTagRegression => f.write_str("saved version cannot move backwards"),
            Self::Sqlite(message) => write!(f, "SQLite error: {message}"),
            Self::ThreadSpawn(message) => write!(f, "could not start state writer: {message}"),
        }
    }
}

impl std::error::Error for StateError {}

#[derive(Debug)]
pub struct SubmitError {
    error: StateError,
    record: Box<VersionRecord>,
}

impl SubmitError {
    pub fn error(&self) -> &StateError {
        &self.error
    }

    pub fn into_parts(self) -> (StateError, VersionRecord) {
        (self.error, *self.record)
    }
}

impl fmt::Display for SubmitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl std::error::Error for SubmitError {}

#[derive(Clone, Copy)]
struct GroupCommitPolicy {
    idle_delay: Duration,
    maximum_delay: Duration,
}

impl Default for GroupCommitPolicy {
    fn default() -> Self {
        Self {
            idle_delay: Duration::from_millis(300),
            maximum_delay: Duration::from_secs(2),
        }
    }
}

/// Owns the sole SQLite writer for one state database.
///
/// Call [`shutdown`](Self::shutdown) to observe final flush and worker errors;
/// dropping the store only performs a best-effort drain.
pub struct StateStore {
    sender: Option<SyncSender<Command>>,
    worker: Option<JoinHandle<Result<(), StateError>>>,
}

impl StateStore {
    /// Opens or migrates `state.db` and starts its single SQLite writer.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StateError> {
        Self::open_with(path.as_ref().to_path_buf(), GroupCommitPolicy::default())
    }

    // The State adapter owns the persistence worker; the pure kernel does not.
    #[allow(clippy::disallowed_methods)]
    fn open_with(path: PathBuf, policy: GroupCommitPolicy) -> Result<Self, StateError> {
        let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let (ready_sender, ready_receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("bloom-state-writer".to_owned())
            .spawn(move || {
                let mut connection = match open_connection(&path) {
                    Ok(connection) => connection,
                    Err(error) => {
                        let _ = ready_sender.send(Err(error.clone()));
                        return Err(error);
                    }
                };
                if ready_sender.send(Ok(())).is_err() {
                    return Ok(());
                }
                run_worker(&mut connection, receiver, policy)
            })
            .map_err(|error| StateError::ThreadSpawn(error.to_string()))?;

        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                sender: Some(sender),
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => {
                let _ = worker.join();
                Err(StateError::WorkerStopped)
            }
        }
    }

    /// Enqueues a full snapshot without waiting for SQLite durability.
    ///
    /// The returned receipt resolves only after the containing SQLite
    /// transaction commits. A full queue is reported instead of blocking the
    /// input path.
    // The adapter samples monotonic time; the kernel still receives time as data.
    #[allow(clippy::disallowed_methods)]
    pub fn try_submit(&self, record: VersionRecord) -> Result<CommitReceipt, SubmitError> {
        let key = VersionKey {
            page_id: record.page_id.clone(),
            version: record.version,
        };
        let submitted_at = Instant::now();
        let (ack_sender, ack_receiver) = mpsc::channel();
        let command = Command::Append {
            record,
            acknowledgement: ack_sender,
            submitted_at,
        };
        let sender = match self.sender() {
            Ok(sender) => sender,
            Err(error) => {
                let Command::Append { record, .. } = command else {
                    unreachable!("try_submit sends only append commands");
                };
                return Err(SubmitError {
                    error,
                    record: Box::new(record),
                });
            }
        };
        match sender.try_send(command) {
            Ok(()) => Ok(CommitReceipt {
                key,
                receiver: ack_receiver,
                cached: Mutex::new(None),
            }),
            Err(TrySendError::Full(Command::Append { record, .. })) => Err(SubmitError {
                error: StateError::QueueFull,
                record: Box::new(record),
            }),
            Err(TrySendError::Disconnected(Command::Append { record, .. })) => Err(SubmitError {
                error: StateError::WorkerStopped,
                record: Box::new(record),
            }),
            Err(_) => unreachable!("try_submit sends only append commands"),
        }
    }

    /// Forces pending versions through SQLite and waits for commit.
    pub fn flush(&self) -> Result<(), StateError> {
        let (reply, response) = mpsc::channel();
        self.sender()?
            .send(Command::Flush { reply })
            .map_err(|_| StateError::WorkerStopped)?;
        response.recv().map_err(|_| StateError::WorkerStopped)?
    }

    /// Records that a vault write was confirmed for this version.
    ///
    /// Callers must invoke this only after the Vault Store confirms its write.
    pub fn mark_saved(&self, page_id: PageId, version: Version) -> Result<(), StateError> {
        let (reply, response) = mpsc::channel();
        self.sender()?
            .send(Command::MarkSaved {
                page_id,
                version,
                reply,
            })
            .map_err(|_| StateError::WorkerStopped)?;
        response.recv().map_err(|_| StateError::WorkerStopped)?
    }

    /// Returns an exact committed snapshot, first committing any queued versions.
    pub fn get_version(
        &self,
        page_id: PageId,
        version: Version,
    ) -> Result<Option<VersionRecord>, StateError> {
        let (reply, response) = mpsc::channel();
        self.sender()?
            .send(Command::GetVersion {
                page_id,
                version,
                reply,
            })
            .map_err(|_| StateError::WorkerStopped)?;
        response.recv().map_err(|_| StateError::WorkerStopped)?
    }

    /// Returns the durable saved snapshot after committing queued versions.
    pub fn saved_record(&self, page_id: PageId) -> Result<Option<VersionRecord>, StateError> {
        let (reply, response) = mpsc::channel();
        self.sender()?
            .send(Command::SavedRecord { page_id, reply })
            .map_err(|_| StateError::WorkerStopped)?;
        response.recv().map_err(|_| StateError::WorkerStopped)?
    }

    /// Returns the newest version newer than the saved marker, if any.
    pub fn recovery_candidate(&self, page_id: PageId) -> Result<Option<VersionRecord>, StateError> {
        let (reply, response) = mpsc::channel();
        self.sender()?
            .send(Command::RecoveryCandidate { page_id, reply })
            .map_err(|_| StateError::WorkerStopped)?;
        response.recv().map_err(|_| StateError::WorkerStopped)?
    }

    /// Flushes pending versions, closes the writer, and reports shutdown errors.
    pub fn shutdown(mut self) -> Result<(), StateError> {
        let (reply, response) = mpsc::channel();
        let result = match self.sender.take() {
            Some(sender) => {
                sender
                    .send(Command::Shutdown { reply })
                    .map_err(|_| StateError::WorkerStopped)?;
                response.recv().map_err(|_| StateError::WorkerStopped)?
            }
            None => Ok(()),
        };
        drop(self.sender.take());
        let joined = self.join_worker();
        result.and(joined)
    }

    fn sender(&self) -> Result<&SyncSender<Command>, StateError> {
        self.sender.as_ref().ok_or(StateError::WorkerStopped)
    }

    fn join_worker(&mut self) -> Result<(), StateError> {
        match self.worker.take() {
            Some(worker) => worker.join().map_err(|_| StateError::WorkerPanicked)?,
            None => Ok(()),
        }
    }
}

impl Drop for StateStore {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VersionKey {
    page_id: PageId,
    version: Version,
}

pub struct CommitReceipt {
    key: VersionKey,
    receiver: Receiver<Result<(), StateError>>,
    cached: Mutex<Option<Result<(), StateError>>>,
}

impl fmt::Debug for CommitReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommitReceipt")
            .field("page_id", &self.key.page_id)
            .field("version", &self.key.version)
            .finish_non_exhaustive()
    }
}

impl CommitReceipt {
    pub fn page_id(&self) -> &PageId {
        &self.key.page_id
    }

    pub fn version(&self) -> Version {
        self.key.version
    }

    pub fn try_result(&self) -> Result<Option<()>, StateError> {
        let mut cached = self.cached.lock().map_err(|_| StateError::WorkerPanicked)?;
        if let Some(result) = cached.as_ref() {
            return result.clone().map(Some);
        }
        match self.receiver.try_recv() {
            Ok(result) => {
                *cached = Some(result.clone());
                result.map(Some)
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(StateError::WorkerStopped),
        }
    }

    pub fn wait(self) -> Result<(), StateError> {
        let cached = self
            .cached
            .into_inner()
            .map_err(|_| StateError::WorkerPanicked)?;
        match cached {
            Some(result) => result,
            None => self
                .receiver
                .recv()
                .map_err(|_| StateError::WorkerStopped)?,
        }
    }
}

enum Command {
    Append {
        record: VersionRecord,
        acknowledgement: mpsc::Sender<Result<(), StateError>>,
        submitted_at: Instant,
    },
    Flush {
        reply: mpsc::Sender<Result<(), StateError>>,
    },
    MarkSaved {
        page_id: PageId,
        version: Version,
        reply: mpsc::Sender<Result<(), StateError>>,
    },
    GetVersion {
        page_id: PageId,
        version: Version,
        reply: mpsc::Sender<Result<Option<VersionRecord>, StateError>>,
    },
    SavedRecord {
        page_id: PageId,
        reply: mpsc::Sender<Result<Option<VersionRecord>, StateError>>,
    },
    RecoveryCandidate {
        page_id: PageId,
        reply: mpsc::Sender<Result<Option<VersionRecord>, StateError>>,
    },
    Shutdown {
        reply: mpsc::Sender<Result<(), StateError>>,
    },
}

struct PendingRecord {
    record: VersionRecord,
    acknowledgement: mpsc::Sender<Result<(), StateError>>,
}

// The worker owns its monotonic group-commit clock.
#[allow(clippy::disallowed_methods)]
fn run_worker(
    connection: &mut Connection,
    receiver: Receiver<Command>,
    policy: GroupCommitPolicy,
) -> Result<(), StateError> {
    run_worker_with_clock(connection, receiver, policy, Instant::now)
}

fn run_worker_with_clock(
    connection: &mut Connection,
    receiver: Receiver<Command>,
    policy: GroupCommitPolicy,
    mut now: impl FnMut() -> Instant,
) -> Result<(), StateError> {
    let mut pending = Vec::new();
    let mut first_pending_at: Option<Instant> = None;
    let mut last_pending_at: Option<Instant> = None;

    loop {
        let timeout = match (first_pending_at, last_pending_at) {
            (Some(first), Some(last)) => {
                let deadline = (first + policy.maximum_delay).min(last + policy.idle_delay);
                let current = now();
                if current >= deadline {
                    commit_pending(connection, &mut pending)?;
                    first_pending_at = None;
                    last_pending_at = None;
                    continue;
                }
                Some(deadline.duration_since(current))
            }
            _ => None,
        };
        let command = match timeout {
            Some(timeout) => match receiver.recv_timeout(timeout) {
                Ok(command) => command,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    commit_pending(connection, &mut pending)?;
                    first_pending_at = None;
                    last_pending_at = None;
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            },
            None => match receiver.recv() {
                Ok(command) => command,
                Err(_) => break,
            },
        };
        match command {
            Command::Append {
                record,
                acknowledgement,
                submitted_at,
            } => {
                first_pending_at =
                    Some(first_pending_at.map_or(submitted_at, |first| first.min(submitted_at)));
                last_pending_at =
                    Some(last_pending_at.map_or(submitted_at, |last| last.max(submitted_at)));
                pending.push(PendingRecord {
                    record,
                    acknowledgement,
                });
                if pending.len() == MAX_PENDING_VERSIONS {
                    commit_pending(connection, &mut pending)?;
                    first_pending_at = None;
                    last_pending_at = None;
                }
            }
            Command::Flush { reply } => {
                let result = commit_pending(connection, &mut pending);
                first_pending_at = None;
                last_pending_at = None;
                let _ = reply.send(result.clone());
                result?;
            }
            Command::MarkSaved {
                page_id,
                version,
                reply,
            } => {
                let flush_result = commit_pending(connection, &mut pending);
                first_pending_at = None;
                last_pending_at = None;
                if let Err(error) = flush_result {
                    let _ = reply.send(Err(error.clone()));
                    return Err(error);
                }
                let result = mark_saved(connection, &page_id, version);
                let _ = reply.send(result);
            }
            Command::GetVersion {
                page_id,
                version,
                reply,
            } => {
                let flush_result = commit_pending(connection, &mut pending);
                first_pending_at = None;
                last_pending_at = None;
                if let Err(error) = flush_result {
                    let _ = reply.send(Err(error.clone()));
                    return Err(error);
                }
                let result = get_version(connection, &page_id, version);
                let _ = reply.send(result);
            }
            Command::SavedRecord { page_id, reply } => {
                let flush_result = commit_pending(connection, &mut pending);
                first_pending_at = None;
                last_pending_at = None;
                if let Err(error) = flush_result {
                    let _ = reply.send(Err(error.clone()));
                    return Err(error);
                }
                let result = saved_record(connection, &page_id);
                let _ = reply.send(result);
            }
            Command::RecoveryCandidate { page_id, reply } => {
                let flush_result = commit_pending(connection, &mut pending);
                first_pending_at = None;
                last_pending_at = None;
                if let Err(error) = flush_result {
                    let _ = reply.send(Err(error.clone()));
                    return Err(error);
                }
                let result = recovery_candidate(connection, &page_id);
                let _ = reply.send(result);
            }
            Command::Shutdown { reply } => {
                let result = commit_pending(connection, &mut pending);
                let _ = reply.send(result.clone());
                return result;
            }
        }
    }

    commit_pending(connection, &mut pending)
}

fn commit_pending(
    connection: &mut Connection,
    pending: &mut Vec<PendingRecord>,
) -> Result<(), StateError> {
    if pending.is_empty() {
        return Ok(());
    }

    let result = (|| {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for item in pending.iter() {
            insert_version(&transaction, &item.record)?;
        }
        crash_test_hook("before-commit");
        transaction.commit()?;
        crash_test_hook("after-commit");
        Ok::<(), StateError>(())
    })();

    match result {
        Ok(()) => {
            for item in pending.drain(..) {
                let _ = item.acknowledgement.send(Ok(()));
            }
            Ok(())
        }
        Err(error) => {
            for item in pending.drain(..) {
                let _ = item.acknowledgement.send(Err(error.clone()));
            }
            Err(error)
        }
    }
}

fn insert_version(transaction: &Transaction<'_>, record: &VersionRecord) -> Result<(), StateError> {
    transaction.execute(
        "INSERT INTO versions (
            page_id, version, parent_version, timestamp_ms, transaction_id,
            origin, content, cursor_anchor
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            record.page_id.as_str(),
            record.version.0,
            record.parent.map(|parent| parent.0),
            record.timestamp_ms,
            record.transaction_id.as_str(),
            record.origin.as_str(),
            record.content,
            record.cursor_anchor,
        ],
    )?;
    Ok(())
}

fn mark_saved(
    connection: &mut Connection,
    page_id: &PageId,
    version: Version,
) -> Result<(), StateError> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let exists: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM versions WHERE page_id = ?1 AND version = ?2)",
        params![page_id.as_str(), version.0],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StateError::VersionNotFound);
    }
    let saved: Option<i64> = transaction
        .query_row(
            "SELECT version FROM tags WHERE page_id = ?1 AND kind = 'saved'",
            [page_id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    if saved.is_some_and(|saved| version.0 < saved) {
        return Err(StateError::SavedTagRegression);
    }
    transaction.execute(
        "INSERT INTO tags (page_id, version, kind)
         VALUES (?1, ?2, 'saved')
         ON CONFLICT(page_id, kind) DO UPDATE SET version = excluded.version",
        params![page_id.as_str(), version.0],
    )?;
    crash_test_hook("before-saved-commit");
    transaction.commit()?;
    crash_test_hook("after-saved-commit");
    Ok(())
}

fn recovery_candidate(
    connection: &Connection,
    page_id: &PageId,
) -> Result<Option<VersionRecord>, StateError> {
    let latest = read_version(
        connection,
        "SELECT page_id, version, parent_version, timestamp_ms, transaction_id,
                origin, content, cursor_anchor
         FROM versions WHERE page_id = ?1 ORDER BY version DESC LIMIT 1",
        [page_id.as_str()],
    )?;
    let Some(latest) = latest else {
        return Ok(None);
    };
    let saved: Option<i64> = connection
        .query_row(
            "SELECT version FROM tags WHERE page_id = ?1 AND kind = 'saved'",
            [page_id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    if saved.is_none_or(|saved| latest.version.0 > saved) {
        Ok(Some(latest))
    } else {
        Ok(None)
    }
}

fn get_version(
    connection: &Connection,
    page_id: &PageId,
    version: Version,
) -> Result<Option<VersionRecord>, StateError> {
    read_version(
        connection,
        "SELECT page_id, version, parent_version, timestamp_ms, transaction_id,
                origin, content, cursor_anchor
         FROM versions WHERE page_id = ?1 AND version = ?2",
        params![page_id.as_str(), version.0],
    )
}

fn saved_version(connection: &Connection, page_id: &PageId) -> Result<Option<Version>, StateError> {
    connection
        .query_row(
            "SELECT version FROM tags WHERE page_id = ?1 AND kind = 'saved'",
            [page_id.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|version| Version::new(version as u64))
        .transpose()
}

fn saved_record(
    connection: &Connection,
    page_id: &PageId,
) -> Result<Option<VersionRecord>, StateError> {
    match saved_version(connection, page_id)? {
        Some(version) => get_version(connection, page_id, version),
        None => Ok(None),
    }
}

fn read_version<P: rusqlite::Params>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> Result<Option<VersionRecord>, StateError> {
    connection
        .query_row(sql, params, |row| {
            let page_id: String = row.get(0)?;
            let version: i64 = row.get(1)?;
            let parent: Option<i64> = row.get(2)?;
            let timestamp_ms: i64 = row.get(3)?;
            let transaction_id: String = row.get(4)?;
            let origin: String = row.get(5)?;
            let content: String = row.get(6)?;
            let cursor_anchor: Option<Vec<u8>> = row.get(7)?;
            VersionRecord::new(
                PageId::new(page_id).map_err(to_sql_conversion_error)?,
                Version::new(version as u64).map_err(to_sql_conversion_error)?,
                parent
                    .map(|parent| Version::new(parent as u64))
                    .transpose()
                    .map_err(to_sql_conversion_error)?,
                timestamp_ms,
                TransactionId::new(transaction_id).map_err(to_sql_conversion_error)?,
                Origin::parse(&origin).map_err(to_sql_conversion_error)?,
                content,
                cursor_anchor,
            )
            .map_err(to_sql_conversion_error)
        })
        .optional()
        .map_err(Into::into)
}

fn to_sql_conversion_error(error: StateError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn open_connection(path: &Path) -> Result<Connection, StateError> {
    let mut connection = Connection::open(path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    verify_integrity(&connection)?;
    migrate(&mut connection)?;

    let journal_mode: String =
        connection.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        return Err(StateError::Sqlite(format!(
            "requested WAL mode but SQLite selected {journal_mode}"
        )));
    }
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.pragma_update(None, "foreign_keys", true)?;
    verify_foreign_keys(&connection)?;
    Ok(connection)
}

fn verify_integrity(connection: &Connection) -> Result<(), StateError> {
    let mut statement = connection.prepare("PRAGMA integrity_check")?;
    let messages = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if messages.len() == 1 && messages[0] == "ok" {
        Ok(())
    } else {
        Err(StateError::IntegrityCheck(messages.join("; ")))
    }
}

fn verify_foreign_keys(connection: &Connection) -> Result<(), StateError> {
    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    let violations = statement
        .query_map([], |row| {
            Ok(format!(
                "{} rowid {:?} references {}",
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if violations.is_empty() {
        Ok(())
    } else {
        Err(StateError::IntegrityCheck(violations.join("; ")))
    }
}

fn migrate(connection: &mut Connection) -> Result<(), StateError> {
    let schema_version: i64 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if !(0..=SCHEMA_VERSION).contains(&schema_version) {
        return Err(StateError::UnsupportedSchema(schema_version));
    }
    if schema_version == SCHEMA_VERSION {
        validate_schema(connection)?;
        return Ok(());
    }

    let object_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    if object_count != 0 {
        return Err(StateError::UnversionedDatabase);
    }

    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE versions (
            page_id TEXT NOT NULL CHECK(length(page_id) > 0),
            version INTEGER NOT NULL CHECK(version > 0),
            parent_version INTEGER CHECK(parent_version IS NULL OR
                                         (parent_version > 0 AND parent_version < version)),
            timestamp_ms INTEGER NOT NULL,
            transaction_id TEXT NOT NULL CHECK(length(transaction_id) > 0),
            origin TEXT NOT NULL CHECK(origin IN ('user', 'agent')),
            content TEXT NOT NULL,
            cursor_anchor BLOB,
            PRIMARY KEY(page_id, version),
            FOREIGN KEY(page_id, parent_version)
                REFERENCES versions(page_id, version)
                DEFERRABLE INITIALLY DEFERRED
        );
        CREATE INDEX versions_by_page_desc
            ON versions(page_id, version DESC);
        CREATE TABLE tags (
            page_id TEXT NOT NULL,
            version INTEGER NOT NULL CHECK(version > 0),
            kind TEXT NOT NULL CHECK(kind = 'saved'),
            PRIMARY KEY(page_id, kind),
            FOREIGN KEY(page_id, version)
                REFERENCES versions(page_id, version)
                ON DELETE RESTRICT
        );",
    )?;
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()?;
    validate_schema(connection)
}

fn validate_schema(connection: &Connection) -> Result<(), StateError> {
    connection.prepare(
        "SELECT page_id, version, parent_version, timestamp_ms, transaction_id,
                origin, content, cursor_anchor
         FROM versions LIMIT 0",
    )?;
    connection.prepare("SELECT page_id, version, kind FROM tags LIMIT 0")?;
    Ok(())
}

impl From<rusqlite::Error> for StateError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error.to_string())
    }
}

#[cfg(any(test, all(feature = "test-hooks", debug_assertions)))]
fn crash_test_hook(point: &str) {
    if std::env::var("BLOOM_STATE_CRASH_AT").as_deref() == Ok(point) {
        std::process::exit(86);
    }
}

#[cfg(not(any(test, all(feature = "test-hooks", debug_assertions))))]
fn crash_test_hook(_point: &str) {}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        process::{Command, ExitStatus},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };

    use bloom_kernel::identity::{
        begin_visible_edit, commit_planned_visible_edit, plan_split_text, BlockId,
    };
    use bloom_text::{BlockIdMarker, Doc};
    use rusqlite::Connection;

    use super::*;

    static NEXT_DB_ID: AtomicU64 = AtomicU64::new(0);

    struct TempDb(PathBuf);

    impl TempDb {
        fn new() -> Self {
            let id = NEXT_DB_ID.fetch_add(1, Ordering::Relaxed);
            Self(std::env::temp_dir().join(format!("bloom-state-{}-{id}.db", std::process::id())))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            for suffix in ["", "-wal", "-shm"] {
                let path = PathBuf::from(format!("{}{suffix}", self.0.display()));
                let _ = fs::remove_file(path);
            }
        }
    }

    fn policy() -> GroupCommitPolicy {
        GroupCommitPolicy {
            idle_delay: Duration::from_millis(20),
            maximum_delay: Duration::from_millis(100),
        }
    }

    fn record(version: u64, parent: Option<u64>, content: &str) -> VersionRecord {
        VersionRecord::new(
            PageId::new("page-a").unwrap(),
            Version::new(version).unwrap(),
            parent.map(|parent| Version::new(parent).unwrap()),
            version as i64 * 100,
            TransactionId::new(format!("txn-{version}")).unwrap(),
            Origin::User,
            content.to_owned(),
            None,
        )
        .unwrap()
    }

    fn open_test_store(path: &Path) -> StateStore {
        StateStore::open_with(path.to_path_buf(), policy()).unwrap()
    }

    fn planned_identity_snapshots() -> [String; 3] {
        let original = "before ^other\r\nαβ ^split1\r\nref ^=split1\r\n".to_owned();
        let projection = Doc::new(&original).visible_projection();
        let split_at = projection.text()[..projection.text().find('β').unwrap()]
            .chars()
            .count();
        let split = plan_split_text(
            &original,
            split_at,
            &BlockId::new("split1").unwrap(),
            &BlockId::new("split2").unwrap(),
        )
        .unwrap();
        assert_eq!(
            split,
            "before ^other\r\nα ^split1\r\nβ ^split2\r\nref ^=split1\r\n"
        );

        let mut alternative = original.clone();
        let context = begin_visible_edit(&mut alternative);
        let visible = context.projection().text();
        let start = visible[..visible.find('β').unwrap()].chars().count();
        let range = context.projection().range(start, start + 1).unwrap();
        let edit = context.plan(range, "γ").unwrap();
        commit_planned_visible_edit(edit);
        assert_eq!(
            alternative,
            "before ^other\r\nαγ ^split1\r\nref ^=split1\r\n"
        );

        [original, split, alternative]
    }

    fn in_memory_state() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "temp_store", "MEMORY")
            .unwrap();
        migrate(&mut connection).unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        connection
    }

    fn pending_batch(
        records: &[VersionRecord],
    ) -> (Vec<PendingRecord>, Vec<Receiver<Result<(), StateError>>>) {
        let mut pending = Vec::new();
        let mut acknowledgements = Vec::new();
        for record in records {
            let (sender, receiver) = mpsc::channel();
            pending.push(PendingRecord {
                record: record.clone(),
                acknowledgement: sender,
            });
            acknowledgements.push(receiver);
        }
        (pending, acknowledgements)
    }

    #[test]
    fn blank_database_migrates_once_to_wal_schema_and_reopens() {
        let db = TempDb::new();
        let store = open_test_store(db.path());
        store.shutdown().unwrap();

        let connection = Connection::open(db.path()).unwrap();
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert_eq!(
            connection
                .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
                .unwrap()
                .to_lowercase(),
            "wal"
        );
        drop(connection);

        open_test_store(db.path()).shutdown().unwrap();
    }

    #[test]
    fn unknown_unversioned_database_is_rejected_without_dropping_tables() {
        let db = TempDb::new();
        let connection = Connection::open(db.path()).unwrap();
        connection
            .execute("CREATE TABLE user_data (value TEXT NOT NULL)", [])
            .unwrap();
        drop(connection);

        assert_eq!(
            StateStore::open_with(db.path().to_path_buf(), policy()).err(),
            Some(StateError::UnversionedDatabase)
        );
        let connection = Connection::open(db.path()).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='user_data'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn newer_schema_is_rejected() {
        let db = TempDb::new();
        let connection = Connection::open(db.path()).unwrap();
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        drop(connection);
        assert_eq!(
            StateStore::open_with(db.path().to_path_buf(), policy()).err(),
            Some(StateError::UnsupportedSchema(SCHEMA_VERSION + 1))
        );
    }

    #[test]
    fn corrupt_database_is_reported_without_replacement() {
        let db = TempDb::new();
        let corrupt_bytes = b"not a SQLite database";
        fs::write(db.path(), corrupt_bytes).unwrap();
        assert!(matches!(
            StateStore::open_with(db.path().to_path_buf(), policy()),
            Err(StateError::Sqlite(_)) | Err(StateError::IntegrityCheck(_))
        ));
        assert_eq!(fs::read(db.path()).unwrap(), corrupt_bytes);
    }

    #[test]
    fn receipts_ack_only_after_group_commit_and_recovery_excludes_saved_versions() {
        let db = TempDb::new();
        let store = open_test_store(db.path());
        let first = store.try_submit(record(1, None, "draft")).unwrap();
        let second = store
            .try_submit(record(2, Some(1), "draft with changes"))
            .unwrap();
        assert_eq!(first.try_result().unwrap(), None);
        assert_eq!(second.try_result().unwrap(), None);
        assert_eq!(first.wait(), Ok(()));
        second.wait().unwrap();
        assert_eq!(
            store
                .recovery_candidate(PageId::new("page-a").unwrap())
                .unwrap()
                .unwrap()
                .content(),
            "draft with changes"
        );
        store
            .mark_saved(PageId::new("page-a").unwrap(), Version::new(2).unwrap())
            .unwrap();
        assert_eq!(
            store
                .recovery_candidate(PageId::new("page-a").unwrap())
                .unwrap(),
            None
        );
        store.shutdown().unwrap();
    }

    #[test]
    fn exact_version_read_commits_pending_snapshot_and_reports_missing_version() {
        let db = TempDb::new();
        let store = open_test_store(db.path());
        let expected = record(1, None, "committed before read");
        store.try_submit(expected.clone()).unwrap();

        assert_eq!(
            store
                .get_version(expected.page_id().clone(), expected.version())
                .unwrap(),
            Some(expected.clone())
        );
        assert_eq!(
            store
                .get_version(expected.page_id().clone(), Version::new(2).unwrap())
                .unwrap(),
            None
        );
        assert_eq!(
            store.saved_record(expected.page_id().clone()).unwrap(),
            None
        );
        store
            .mark_saved(expected.page_id().clone(), expected.version())
            .unwrap();
        assert_eq!(
            store.saved_record(expected.page_id().clone()).unwrap(),
            Some(expected.clone())
        );
        store.shutdown().unwrap();
    }

    #[test]
    fn complete_version_record_round_trips() {
        let db = TempDb::new();
        let store = open_test_store(db.path());
        store
            .try_submit(record(1, None, "initial"))
            .unwrap()
            .wait()
            .unwrap();
        let expected = VersionRecord::new(
            PageId::new("page-a").unwrap(),
            Version::new(2).unwrap(),
            Some(Version::new(1).unwrap()),
            1_700_000_000_000,
            TransactionId::new("txn-2").unwrap(),
            Origin::Agent,
            "updated 🪺".to_owned(),
            Some(vec![2, 4, 8]),
        )
        .unwrap();
        store.try_submit(expected.clone()).unwrap().wait().unwrap();
        assert_eq!(
            store
                .recovery_candidate(PageId::new("page-a").unwrap())
                .unwrap(),
            Some(expected)
        );
        store.shutdown().unwrap();
    }

    #[test]
    fn identity_snapshot_branches_round_trip_through_in_memory_state() {
        let [original, split, alternative] = planned_identity_snapshots();
        let records = [
            record(1, None, &original),
            record(2, Some(1), &split),
            record(3, Some(1), &alternative),
        ];
        let mut connection = in_memory_state();
        let (mut pending, acknowledgements) = pending_batch(&records);
        commit_pending(&mut connection, &mut pending).unwrap();
        assert!(pending.is_empty());
        for acknowledgement in acknowledgements {
            assert_eq!(acknowledgement.recv().unwrap(), Ok(()));
        }

        let expected_tokens = [
            vec![
                (BlockIdMarker::Block, "other"),
                (BlockIdMarker::Block, "split1"),
                (BlockIdMarker::Mirror, "split1"),
            ],
            vec![
                (BlockIdMarker::Block, "other"),
                (BlockIdMarker::Block, "split1"),
                (BlockIdMarker::Block, "split2"),
                (BlockIdMarker::Mirror, "split1"),
            ],
            vec![
                (BlockIdMarker::Block, "other"),
                (BlockIdMarker::Block, "split1"),
                (BlockIdMarker::Mirror, "split1"),
            ],
        ];
        for index in [1, 0, 1, 2] {
            let expected = &records[index];
            let actual = get_version(&connection, expected.page_id(), expected.version())
                .unwrap()
                .unwrap();
            assert_eq!(&actual, expected);
            assert_eq!(actual.content().as_bytes(), expected.content().as_bytes());
            let projection = Doc::new(actual.content()).visible_projection();
            assert_eq!(
                projection
                    .tokens()
                    .iter()
                    .map(|token| (token.marker(), token.id()))
                    .collect::<Vec<_>>(),
                expected_tokens[index]
            );
        }
        assert_eq!(
            recovery_candidate(&connection, &PageId::new("page-a").unwrap()).unwrap(),
            Some(records[2].clone())
        );
    }

    #[test]
    fn idle_commit_acknowledges_after_the_debounce_window() {
        let db = TempDb::new();
        let store = open_test_store(db.path());
        let start = Instant::now();
        store
            .try_submit(record(1, None, "idle"))
            .unwrap()
            .wait()
            .unwrap();
        assert!(start.elapsed() >= policy().idle_delay);
        store.shutdown().unwrap();
    }

    #[test]
    fn receipt_polling_remains_stable_after_acknowledgement() {
        let db = TempDb::new();
        let store = open_test_store(db.path());
        let receipt = store.try_submit(record(1, None, "stable")).unwrap();
        store.flush().unwrap();
        assert_eq!(receipt.try_result(), Ok(Some(())));
        assert_eq!(receipt.try_result(), Ok(Some(())));
        store.shutdown().unwrap();
    }

    #[test]
    fn maximum_commit_delay_bounds_continuous_submissions() {
        let mut connection = in_memory_state();
        let (sender, receiver) = mpsc::channel();
        let start = Instant::now();
        let mut acknowledgements = Vec::new();
        for version in 1..=22 {
            let (acknowledgement, response) = mpsc::channel();
            sender
                .send(super::Command::Append {
                    record: record(
                        version,
                        (version > 1).then_some(version - 1),
                        &format!("v{version}"),
                    ),
                    acknowledgement,
                    submitted_at: start + Duration::from_millis((version - 1) * 5),
                })
                .unwrap();
            acknowledgements.push(response);
        }
        let (reply, shutdown_response) = mpsc::channel();
        sender.send(super::Command::Shutdown { reply }).unwrap();
        drop(sender);

        let mut ticks = 0;
        let mut first_during_input = None;
        let mut later_before_arrival = None;
        run_worker_with_clock(&mut connection, receiver, policy(), || {
            ticks += 1;
            if ticks <= 20 {
                assert_eq!(
                    acknowledgements[0].try_recv(),
                    Err(TryRecvError::Empty),
                    "first commit happened before the maximum deadline"
                );
            }
            if ticks == 21 {
                first_during_input = Some(acknowledgements[0].try_recv());
                later_before_arrival = Some(acknowledgements[21].try_recv());
            }
            start + Duration::from_millis(ticks * 5)
        })
        .unwrap();
        assert_eq!(ticks, 22);
        assert_eq!(first_during_input, Some(Ok(Ok(()))));
        assert_eq!(later_before_arrival, Some(Err(TryRecvError::Empty)));
        assert_eq!(shutdown_response.try_recv(), Ok(Ok(())));
        for acknowledgement in acknowledgements.iter().skip(1) {
            assert_eq!(acknowledgement.try_recv(), Ok(Ok(())));
        }
    }

    #[test]
    fn full_submission_queue_returns_the_exact_record_for_retry() {
        let (sender, receiver) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let store = StateStore {
            sender: Some(sender),
            worker: None,
        };
        let last_version = COMMAND_QUEUE_CAPACITY as u64;
        let retry_version = last_version + 1;
        for version in 1..=last_version {
            store
                .try_submit(record(version, None, &format!("v{version}")))
                .unwrap();
        }
        let rejected_record = record(retry_version, None, "retry me");
        let (error, returned) = store
            .try_submit(rejected_record.clone())
            .unwrap_err()
            .into_parts();
        assert_eq!(error, StateError::QueueFull);
        assert_eq!(returned, rejected_record);

        let super::Command::Append { record: first, .. } = receiver.try_recv().unwrap() else {
            panic!("expected an append command");
        };
        assert_eq!(first, record(1, None, "v1"));
        let receipt = store.try_submit(returned).unwrap();
        assert_eq!(receipt.version(), Version::new(retry_version).unwrap());
        for version in 2..=retry_version {
            let super::Command::Append { record: queued, .. } = receiver.try_recv().unwrap() else {
                panic!("expected an append command");
            };
            let expected = if version == retry_version {
                rejected_record.clone()
            } else {
                record(version, None, &format!("v{version}"))
            };
            assert_eq!(queued, expected);
        }
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn saved_tag_cannot_move_backwards_and_missing_versions_cannot_be_saved() {
        let db = TempDb::new();
        let store = open_test_store(db.path());
        store
            .try_submit(record(1, None, "one"))
            .unwrap()
            .wait()
            .unwrap();
        store
            .try_submit(record(2, Some(1), "two"))
            .unwrap()
            .wait()
            .unwrap();
        let page = PageId::new("page-a").unwrap();
        store
            .mark_saved(page.clone(), Version::new(2).unwrap())
            .unwrap();
        assert_eq!(
            store.mark_saved(page.clone(), Version::new(1).unwrap()),
            Err(StateError::SavedTagRegression)
        );
        assert_eq!(
            store.mark_saved(page, Version::new(3).unwrap()),
            Err(StateError::VersionNotFound)
        );
        store.shutdown().unwrap();
    }

    #[test]
    fn parent_foreign_key_rejects_missing_parent_without_acknowledging() {
        let db = TempDb::new();
        let store = open_test_store(db.path());
        let receipt = store.try_submit(record(2, Some(1), "orphan")).unwrap();
        assert!(matches!(receipt.wait(), Err(StateError::Sqlite(_))));
        let (error, retryable) = store
            .try_submit(record(3, None, "after failure"))
            .unwrap_err()
            .into_parts();
        assert_eq!(error, StateError::WorkerStopped);
        assert_eq!(retryable.content(), "after failure");
    }

    #[test]
    fn process_crash_before_and_after_commit_preserves_sqlite_atomicity() {
        for (point, should_exist) in [("before-commit", false), ("after-commit", true)] {
            let db = TempDb::new();
            open_test_store(db.path()).shutdown().unwrap();
            let status = run_crashing_child(db.path(), point);
            assert!(!status.success(), "child should abort at {point}");

            let store = open_test_store(db.path());
            let recovered = store
                .recovery_candidate(PageId::new("page-a").unwrap())
                .unwrap();
            assert_eq!(recovered.is_some(), should_exist);
            store.shutdown().unwrap();
        }
    }

    #[test]
    fn rejected_identity_snapshot_batch_rolls_back_and_reports_every_failure() {
        let [original, split, alternative] = planned_identity_snapshots();
        let mut connection = in_memory_state();
        let initial = record(1, None, &original);
        let (mut pending, acknowledgements) = pending_batch(std::slice::from_ref(&initial));
        commit_pending(&mut connection, &mut pending).unwrap();
        for acknowledgement in acknowledgements {
            assert_eq!(acknowledgement.recv().unwrap(), Ok(()));
        }

        let rejected = [record(2, Some(1), &split), record(4, Some(3), &alternative)];
        let (mut pending, acknowledgements) = pending_batch(&rejected);
        let error = commit_pending(&mut connection, &mut pending).unwrap_err();
        assert!(matches!(error, StateError::Sqlite(_)));
        assert!(pending.is_empty());
        for acknowledgement in acknowledgements {
            assert_eq!(acknowledgement.recv().unwrap(), Err(error.clone()));
        }
        let page = initial.page_id();
        assert_eq!(
            get_version(&connection, page, initial.version()).unwrap(),
            Some(initial.clone())
        );
        for record in rejected {
            assert_eq!(
                get_version(&connection, page, record.version()).unwrap(),
                None
            );
        }
        assert_eq!(
            recovery_candidate(&connection, page).unwrap(),
            Some(initial)
        );
    }

    #[test]
    fn crash_writer_child() {
        let Ok(path) = std::env::var("BLOOM_STATE_CRASH_DB") else {
            return;
        };
        let store = StateStore::open(path).unwrap();
        store.try_submit(record(1, None, "crash-test")).unwrap();
        store.flush().unwrap();
    }

    fn run_crashing_child(path: &Path, point: &str) -> ExitStatus {
        Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("store::tests::crash_writer_child")
            .arg("--nocapture")
            .env("BLOOM_STATE_CRASH_DB", path)
            .env("BLOOM_STATE_CRASH_AT", point)
            .status()
            .unwrap()
    }
}
