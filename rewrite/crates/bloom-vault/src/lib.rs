//! Synthetic-vault page loading and compare-and-swap persistence.

#[cfg(unix)]
use std::fs::File;
use std::{
    error::Error,
    fmt, fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, Weak},
};

use bloom_state::{PageId, StateError, StateStore, Version};
use tempfile::Builder;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SaveConflict {
    ExternalChange {
        base: ContentHash,
        observed: ContentHash,
    },
    DeletedExternally {
        base: ContentHash,
    },
    ChangedDuringWrite {
        expected: ContentHash,
        observed: ContentHash,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SaveOutcome {
    Saved { version: Version, hash: ContentHash },
    Conflict(SaveConflict),
}

#[derive(Debug)]
pub enum VaultError {
    Io {
        operation: &'static str,
        source: io::Error,
    },
    PostReplace {
        operation: &'static str,
        source: io::Error,
    },
    State(StateError),
    StateVersionMissing {
        page_id: PageId,
        version: Version,
    },
    InvalidPagePath,
    OutsideVault,
    PageIsNotFile,
    RootIsNotDirectory,
    DifferentVault,
    PagePathChanged,
    InvalidUtf8(std::string::FromUtf8Error),
    VersionRegression {
        saved: Version,
        requested: Version,
    },
    SavedMarkerAfterWrite(StateError),
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => write!(f, "{operation}: {source}"),
            Self::PostReplace { operation, source } => {
                write!(f, "replacement occurred but {operation} failed: {source}")
            }
            Self::State(error) => write!(f, "state store: {error}"),
            Self::StateVersionMissing { page_id, version } => {
                write!(
                    f,
                    "state has no version {} for page {}",
                    version.get(),
                    page_id.as_str()
                )
            }
            Self::InvalidPagePath => {
                f.write_str("page path must be relative and contain only normal components")
            }
            Self::OutsideVault => f.write_str("resolved page path is outside the vault"),
            Self::PageIsNotFile => f.write_str("page path does not resolve to a regular file"),
            Self::RootIsNotDirectory => f.write_str("vault root is not a directory"),
            Self::DifferentVault => f.write_str("page sync state belongs to another vault"),
            Self::PagePathChanged => {
                f.write_str("page path no longer resolves to its originally loaded file")
            }
            Self::InvalidUtf8(error) => write!(f, "page is not valid UTF-8: {error}"),
            Self::VersionRegression { saved, requested } => write!(
                f,
                "refusing to save version {} behind already-saved version {}",
                requested.get(),
                saved.get()
            ),
            Self::SavedMarkerAfterWrite(error) => write!(
                f,
                "vault file was replaced and verified, but the State saved marker failed: {error}"
            ),
        }
    }
}

impl Error for VaultError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::PostReplace { source, .. } => Some(source),
            Self::State(error) | Self::SavedMarkerAfterWrite(error) => Some(error),
            Self::InvalidUtf8(error) => Some(error),
            _ => None,
        }
    }
}

impl From<StateError> for VaultError {
    fn from(error: StateError) -> Self {
        Self::State(error)
    }
}

#[derive(Debug)]
pub struct PageSync {
    actor: Weak<()>,
    page_id: PageId,
    root: PathBuf,
    relative_path: PathBuf,
    base: ContentHash,
    saved_version: Option<Version>,
    conflict: Option<SaveConflict>,
}

impl PageSync {
    pub fn base(&self) -> ContentHash {
        self.base
    }

    pub fn saved_version(&self) -> Option<Version> {
        self.saved_version
    }

    pub fn conflict(&self) -> Option<&SaveConflict> {
        self.conflict.as_ref()
    }

    pub fn is_dirty(&self, document_version: Version) -> bool {
        self.saved_version != Some(document_version)
    }
}

#[derive(Debug)]
pub struct LoadedPage {
    page_id: PageId,
    text: String,
    sync: PageSync,
}

impl LoadedPage {
    pub fn page_id(&self) -> &PageId {
        &self.page_id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn sync(&self) -> &PageSync {
        &self.sync
    }

    pub fn sync_mut(&mut self) -> &mut PageSync {
        &mut self.sync
    }
}

pub struct VaultStore {
    root: PathBuf,
    actor: Arc<()>,
}

impl VaultStore {
    /// Opens an existing synthetic-vault root and pins its canonical path.
    #[allow(clippy::disallowed_methods)]
    pub fn open(root: impl AsRef<Path>) -> Result<Self, VaultError> {
        let root = fs::canonicalize(root).map_err(|source| VaultError::Io {
            operation: "canonicalize vault root",
            source,
        })?;
        let metadata = fs::metadata(&root).map_err(|source| VaultError::Io {
            operation: "inspect vault root",
            source,
        })?;
        if !metadata.is_dir() {
            return Err(VaultError::RootIsNotDirectory);
        }
        Ok(Self {
            root,
            actor: Arc::new(()),
        })
    }

    /// Loads one existing page and verifies its bytes against State's saved marker.
    #[allow(clippy::disallowed_methods)]
    pub fn load_page(
        &mut self,
        state: &StateStore,
        page_id: PageId,
        relative_path: impl AsRef<Path>,
    ) -> Result<LoadedPage, VaultError> {
        let path = self.resolve_existing(relative_path.as_ref())?;
        let bytes = fs::read(&path).map_err(|source| VaultError::Io {
            operation: "read page",
            source,
        })?;
        let text = String::from_utf8(bytes.clone()).map_err(VaultError::InvalidUtf8)?;
        let relative_path = path
            .strip_prefix(&self.root)
            .map_err(|_| VaultError::OutsideVault)?
            .to_path_buf();
        let disk_hash = hash(&bytes);
        let saved_record = state.saved_record(page_id.clone())?;
        let (base, saved_version, conflict) = match saved_record {
            Some(record) => {
                let version = record.version();
                let base = hash(record.content().as_bytes());
                if base == disk_hash {
                    (base, Some(version), None)
                } else {
                    (
                        base,
                        None,
                        Some(SaveConflict::ExternalChange {
                            base,
                            observed: disk_hash,
                        }),
                    )
                }
            }
            None => (disk_hash, None, None),
        };
        Ok(LoadedPage {
            page_id: page_id.clone(),
            text,
            sync: PageSync {
                actor: Arc::downgrade(&self.actor),
                page_id: page_id.clone(),
                root: self.root.clone(),
                relative_path,
                base,
                saved_version,
                conflict,
            },
        })
    }

    /// Commits the exact State snapshot to its page using observed-base CAS.
    ///
    /// The State lookup commits queued snapshots before returning. State's
    /// saved marker is advanced only after replacement, durability, and
    /// readback all succeed.
    pub fn save_page(
        &mut self,
        sync: &mut PageSync,
        state: &StateStore,
        version: Version,
    ) -> Result<SaveOutcome, VaultError> {
        self.save_page_with(sync, state, version, |_| Ok(()))
    }

    #[allow(clippy::disallowed_methods)]
    fn save_page_with(
        &mut self,
        sync: &mut PageSync,
        state: &StateStore,
        version: Version,
        before_replace: impl FnOnce(&Path) -> io::Result<()>,
    ) -> Result<SaveOutcome, VaultError> {
        if sync.root != self.root || !Weak::ptr_eq(&sync.actor, &Arc::downgrade(&self.actor)) {
            return Err(VaultError::DifferentVault);
        }
        if let Some(saved) = sync.saved_version {
            if version < saved {
                return Err(VaultError::VersionRegression {
                    saved,
                    requested: version,
                });
            }
        }

        let record = state
            .get_version(sync.page_id.clone(), version)?
            .ok_or_else(|| VaultError::StateVersionMissing {
                page_id: sync.page_id.clone(),
                version,
            })?;
        let path = self.root.join(&sync.relative_path);
        let resolved = match self.resolve_existing(&sync.relative_path) {
            Ok(resolved) => resolved,
            Err(VaultError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
                let conflict = SaveConflict::DeletedExternally { base: sync.base };
                sync.conflict = Some(conflict.clone());
                return Ok(SaveOutcome::Conflict(conflict));
            }
            Err(error) => return Err(error),
        };
        if resolved != path {
            return Err(VaultError::PagePathChanged);
        }
        let current = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let conflict = SaveConflict::DeletedExternally { base: sync.base };
                sync.conflict = Some(conflict.clone());
                return Ok(SaveOutcome::Conflict(conflict));
            }
            Err(source) => {
                return Err(VaultError::Io {
                    operation: "read page before compare-and-swap",
                    source,
                });
            }
        };
        let current_hash = hash(&current);
        if current_hash != sync.base {
            let conflict = SaveConflict::ExternalChange {
                base: sync.base,
                observed: current_hash,
            };
            sync.conflict = Some(conflict.clone());
            return Ok(SaveOutcome::Conflict(conflict));
        }

        let target_hash = hash(record.content().as_bytes());
        replace_file(&path, record.content().as_bytes(), before_replace)?;
        crash_test_hook("before-readback");
        let readback = fs::read(&path).map_err(|source| VaultError::PostReplace {
            operation: "read back page",
            source,
        })?;
        crash_test_hook("after-readback");
        let observed_hash = hash(&readback);
        if observed_hash != target_hash {
            let conflict = SaveConflict::ChangedDuringWrite {
                expected: target_hash,
                observed: observed_hash,
            };
            sync.conflict = Some(conflict.clone());
            return Ok(SaveOutcome::Conflict(conflict));
        }

        sync.base = target_hash;
        sync.saved_version = Some(version);
        sync.conflict = None;
        crash_test_hook("before-saved-marker");
        state
            .mark_saved(sync.page_id.clone(), version)
            .map_err(VaultError::SavedMarkerAfterWrite)?;
        crash_test_hook("after-saved-marker");
        Ok(SaveOutcome::Saved {
            version,
            hash: target_hash,
        })
    }

    #[allow(clippy::disallowed_methods)]
    fn resolve_existing(&self, relative_path: &Path) -> Result<PathBuf, VaultError> {
        if relative_path.as_os_str().is_empty()
            || !relative_path
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        {
            return Err(VaultError::InvalidPagePath);
        }
        let path = self.root.join(relative_path);
        let resolved = fs::canonicalize(path).map_err(|source| VaultError::Io {
            operation: "resolve page path",
            source,
        })?;
        if !resolved.starts_with(&self.root) {
            return Err(VaultError::OutsideVault);
        }
        if !fs::metadata(&resolved)
            .map_err(|source| VaultError::Io {
                operation: "inspect page",
                source,
            })?
            .is_file()
        {
            return Err(VaultError::PageIsNotFile);
        }
        Ok(resolved)
    }
}

fn hash(bytes: &[u8]) -> ContentHash {
    ContentHash(*blake3::hash(bytes).as_bytes())
}

#[allow(clippy::disallowed_methods)]
fn replace_file(
    target: &Path,
    contents: &[u8],
    before_replace: impl FnOnce(&Path) -> io::Result<()>,
) -> Result<(), VaultError> {
    let parent = target.parent().ok_or(VaultError::InvalidPagePath)?;
    let mut temporary = Builder::new()
        .prefix(".bloom-")
        .suffix(".bloom-tmp")
        .tempfile_in(parent)
        .map_err(|source| VaultError::Io {
            operation: "create unique temporary page",
            source,
        })?;
    temporary
        .write_all(contents)
        .map_err(|source| VaultError::Io {
            operation: "write temporary page",
            source,
        })?;
    crash_test_hook("before-temp-sync");
    temporary
        .as_file()
        .sync_all()
        .map_err(|source| VaultError::Io {
            operation: "sync temporary page",
            source,
        })?;
    crash_test_hook("after-temp-sync");
    before_replace(temporary.path()).map_err(|source| VaultError::Io {
        operation: "prepare page replacement",
        source,
    })?;
    crash_test_hook("before-replace");
    durable_replace(temporary.path(), target).map_err(|failure| match failure {
        ReplaceFailure::BeforeReplace(source) => VaultError::Io {
            operation: "atomically replace page",
            source,
        },
        #[cfg(unix)]
        ReplaceFailure::AfterReplace(source) => VaultError::PostReplace {
            operation: "sync parent directory",
            source,
        },
    })?;
    crash_test_hook("after-replace");
    Ok(())
}

#[cfg(test)]
fn crash_test_hook(point: &str) {
    if std::env::var("BLOOM_VAULT_CRASH_AT").as_deref() == Ok(point) {
        std::process::exit(86);
    }
}

#[cfg(not(test))]
fn crash_test_hook(_point: &str) {}

enum ReplaceFailure {
    BeforeReplace(io::Error),
    #[cfg(unix)]
    AfterReplace(io::Error),
}

#[cfg(unix)]
#[allow(clippy::disallowed_methods)]
fn durable_replace(source: &Path, target: &Path) -> Result<(), ReplaceFailure> {
    fs::rename(source, target).map_err(ReplaceFailure::BeforeReplace)?;
    let parent = target.parent().ok_or_else(|| {
        ReplaceFailure::AfterReplace(io::Error::new(
            io::ErrorKind::InvalidInput,
            "target has no parent directory",
        ))
    })?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(ReplaceFailure::AfterReplace)
}

#[cfg(windows)]
fn durable_replace(source: &Path, target: &Path) -> Result<(), ReplaceFailure> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(ReplaceFailure::BeforeReplace(io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn durable_replace(_source: &Path, _target: &Path) -> Result<(), ReplaceFailure> {
    Err(ReplaceFailure::BeforeReplace(io::Error::new(
        io::ErrorKind::Unsupported,
        "durable atomic replacement is not implemented for this platform",
    )))
}

#[cfg(test)]
#[path = "../../bloom-kernel/tests/support/mod.rs"]
mod watchdog_support;

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use std::{fs, io, path::PathBuf};

    use bloom_state::{Origin, TransactionId, VersionRecord};
    use tempfile::tempdir;

    use super::*;

    fn record(page_id: &PageId, version: u64, parent: Option<u64>, text: &str) -> VersionRecord {
        VersionRecord::new(
            page_id.clone(),
            Version::new(version).unwrap(),
            parent.map(|parent| Version::new(parent).unwrap()),
            version as i64,
            TransactionId::new(format!("txn-{version}")).unwrap(),
            Origin::User,
            text.to_owned(),
            None,
        )
        .unwrap()
    }

    fn setup() -> (tempfile::TempDir, VaultStore, PageId, StateStore, PathBuf) {
        let directory = tempdir().unwrap();
        let root = directory.path().join("vault");
        fs::create_dir(&root).unwrap();
        let page_path = root.join("note.md");
        fs::write(&page_path, "original").unwrap();
        let vault = VaultStore::open(&root).unwrap();
        let page_id = PageId::new("page-a").unwrap();
        let state = StateStore::open(directory.path().join("state.db")).unwrap();
        (directory, vault, page_id, state, page_path)
    }

    fn seed_initial(
        state: &StateStore,
        page_id: &PageId,
        initial: &str,
        next: &str,
    ) -> (Version, Version) {
        let first = Version::new(1).unwrap();
        let second = Version::new(2).unwrap();
        state
            .try_submit(record(page_id, 1, None, initial))
            .unwrap()
            .wait()
            .unwrap();
        state.mark_saved(page_id.clone(), first).unwrap();
        state.try_submit(record(page_id, 2, Some(1), next)).unwrap();
        (first, second)
    }

    #[test]
    fn load_and_save_replace_atomically_then_mark_exact_version_saved() {
        let (_directory, mut vault, page_id, state, page_path) = setup();
        let (_, second) = seed_initial(&state, &page_id, "original", "edited");
        let mut loaded = vault.load_page(&state, page_id.clone(), "note.md").unwrap();

        assert_eq!(loaded.text(), "original");
        assert!(loaded.sync().is_dirty(second));
        assert_eq!(
            vault.save_page(loaded.sync_mut(), &state, second).unwrap(),
            SaveOutcome::Saved {
                version: second,
                hash: hash(b"edited")
            }
        );
        assert_eq!(fs::read_to_string(&page_path).unwrap(), "edited");
        assert!(!loaded.sync().is_dirty(second));
        assert_eq!(loaded.sync().base(), hash(b"edited"));
        assert!(state.recovery_candidate(page_id).unwrap().is_none());
        state.shutdown().unwrap();
    }

    #[test]
    fn external_edit_returns_conflict_and_preserves_external_bytes() {
        let (_directory, mut vault, page_id, state, page_path) = setup();
        let (first, second) = seed_initial(&state, &page_id, "original", "ours");
        let mut loaded = vault.load_page(&state, page_id.clone(), "note.md").unwrap();
        fs::write(&page_path, "external").unwrap();

        let outcome = vault.save_page(loaded.sync_mut(), &state, second).unwrap();
        assert_eq!(
            outcome,
            SaveOutcome::Conflict(SaveConflict::ExternalChange {
                base: hash(b"original"),
                observed: hash(b"external")
            })
        );
        assert_eq!(fs::read(&page_path).unwrap(), b"external");
        assert_eq!(loaded.sync().saved_version(), Some(first));
        assert!(state.recovery_candidate(page_id).unwrap().is_some());
        state.shutdown().unwrap();
    }

    #[test]
    fn load_detects_disk_bytes_that_disagree_with_saved_state() {
        let (_directory, mut vault, page_id, state, page_path) = setup();
        let (_, second) = seed_initial(&state, &page_id, "original", "ours");
        fs::write(&page_path, "external before load").unwrap();

        let mut loaded = vault.load_page(&state, page_id.clone(), "note.md").unwrap();
        assert_eq!(loaded.text(), "external before load");
        assert_eq!(loaded.sync().saved_version(), None);
        assert_eq!(
            loaded.sync().conflict(),
            Some(&SaveConflict::ExternalChange {
                base: hash(b"original"),
                observed: hash(b"external before load")
            })
        );
        assert_eq!(
            vault.save_page(loaded.sync_mut(), &state, second).unwrap(),
            SaveOutcome::Conflict(SaveConflict::ExternalChange {
                base: hash(b"original"),
                observed: hash(b"external before load")
            })
        );
        assert_eq!(fs::read(&page_path).unwrap(), b"external before load");
        state.shutdown().unwrap();
    }

    #[test]
    fn deletion_returns_conflict_without_recreating_page() {
        let (_directory, mut vault, page_id, state, page_path) = setup();
        let (_, second) = seed_initial(&state, &page_id, "original", "ours");
        let mut loaded = vault.load_page(&state, page_id, "note.md").unwrap();
        fs::remove_file(&page_path).unwrap();

        assert_eq!(
            vault.save_page(loaded.sync_mut(), &state, second).unwrap(),
            SaveOutcome::Conflict(SaveConflict::DeletedExternally {
                base: hash(b"original")
            })
        );
        assert!(!page_path.exists());
        state.shutdown().unwrap();
    }

    #[test]
    fn injected_pre_replace_failure_preserves_disk_and_cleans_temp_file() {
        let (_directory, mut vault, page_id, state, page_path) = setup();
        let (first, second) = seed_initial(&state, &page_id, "original", "ours");
        let mut loaded = vault.load_page(&state, page_id.clone(), "note.md").unwrap();

        let error = vault
            .save_page_with(loaded.sync_mut(), &state, second, |temp| {
                assert!(temp.exists());
                Err(io::Error::other("injected replace failure"))
            })
            .unwrap_err();
        assert!(matches!(error, VaultError::Io { .. }));
        assert_eq!(fs::read(&page_path).unwrap(), b"original");
        assert!(loaded.sync().is_dirty(second));
        assert_eq!(loaded.sync().saved_version(), Some(first));
        assert!(!fs::read_dir(page_path.parent().unwrap())
            .unwrap()
            .map(Result::unwrap)
            .any(|entry| entry.file_name().to_string_lossy().ends_with(".bloom-tmp")));
        assert!(state.recovery_candidate(page_id).unwrap().is_some());
        state.shutdown().unwrap();
    }

    #[test]
    fn unknown_state_version_does_not_change_vault() {
        let (_directory, mut vault, page_id, state, page_path) = setup();
        let mut loaded = vault.load_page(&state, page_id, "note.md").unwrap();

        assert!(matches!(
            vault.save_page(loaded.sync_mut(), &state, Version::new(2).unwrap()),
            Err(VaultError::StateVersionMissing { .. })
        ));
        assert_eq!(fs::read(&page_path).unwrap(), b"original");
        state.shutdown().unwrap();
    }

    #[test]
    fn stale_version_and_other_vault_state_are_rejected_before_io() {
        let (_directory, mut vault, page_id, state, page_path) = setup();
        let (first, second) = seed_initial(&state, &page_id, "original", "ours");
        let mut loaded = vault.load_page(&state, page_id.clone(), "note.md").unwrap();

        assert!(matches!(
            vault.save_page(loaded.sync_mut(), &state, second),
            Ok(SaveOutcome::Saved { .. })
        ));
        assert!(matches!(
            vault.save_page(loaded.sync_mut(), &state, first),
            Err(VaultError::VersionRegression { .. })
        ));
        assert_eq!(loaded.sync().saved_version(), Some(second));

        let mut other_vault = VaultStore::open(page_path.parent().unwrap()).unwrap();
        assert!(matches!(
            other_vault.save_page(loaded.sync_mut(), &state, second),
            Err(VaultError::DifferentVault)
        ));
        assert_eq!(fs::read(&page_path).unwrap(), b"ours");
        state.shutdown().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn save_rejects_a_page_path_redirected_to_another_file() {
        use std::os::unix::fs::symlink;

        let (_directory, mut vault, page_id, state, page_path) = setup();
        let (first, second) = seed_initial(&state, &page_id, "original", "ours");
        let mut loaded = vault.load_page(&state, page_id, "note.md").unwrap();
        let other_page = page_path.with_file_name("other.md");
        fs::write(&other_page, "other page").unwrap();
        fs::remove_file(&page_path).unwrap();
        symlink(&other_page, &page_path).unwrap();

        assert!(matches!(
            vault.save_page(loaded.sync_mut(), &state, second),
            Err(VaultError::PagePathChanged)
        ));
        assert_eq!(fs::read(&other_page).unwrap(), b"other page");
        assert_eq!(loaded.sync().saved_version(), Some(first));
        state.shutdown().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn save_rejects_a_page_path_redirected_outside_vault() {
        use std::os::unix::fs::symlink;

        let (directory, mut vault, page_id, state, page_path) = setup();
        let (_, second) = seed_initial(&state, &page_id, "original", "ours");
        let mut loaded = vault.load_page(&state, page_id, "note.md").unwrap();
        let outside_page = directory.path().join("outside.md");
        fs::write(&outside_page, "outside").unwrap();
        fs::remove_file(&page_path).unwrap();
        symlink(&outside_page, &page_path).unwrap();

        assert!(matches!(
            vault.save_page(loaded.sync_mut(), &state, second),
            Err(VaultError::OutsideVault)
        ));
        assert_eq!(fs::read(&outside_page).unwrap(), b"outside");
        state.shutdown().unwrap();
    }

    #[test]
    fn invalid_paths_are_rejected() {
        let (_directory, mut vault, _page_id, state, page_path) = setup();
        assert!(matches!(
            vault.load_page(&state, PageId::new("page-b").unwrap(), "../outside.md"),
            Err(VaultError::InvalidPagePath)
        ));
        assert!(matches!(
            vault.load_page(
                &state,
                PageId::new("page-b").unwrap(),
                page_path.canonicalize().unwrap(),
            ),
            Err(VaultError::InvalidPagePath)
        ));
        state.shutdown().unwrap();
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod journey_tests {
    use std::{
        env,
        error::Error,
        fmt, fs,
        io::Write,
        path::{Path, PathBuf},
        process::Command,
        time::Duration,
    };

    use bloom_kernel::{
        simulation::harness::{
            BufferSnapshot, ScreenSnapshot, SimInput, SimInputBackend, SimInputError,
            SimInputEvent, UnavailableReason,
        },
        update::{check_view_purity, update, Effect, Event, Model},
    };
    use bloom_state::{CommitReceipt, Origin, TransactionId, VersionRecord};
    use rusqlite::Connection;
    use tempfile::{tempdir, TempDir};

    use super::*;
    use crate::watchdog_support::{run_watchdog, WatchdogLimits, WatchdogOutcome};

    const INITIAL_TEXT: &str = "";
    const DRAFT_TEXT: &str = "synthetic note";
    const EXPECTED_EXIT: i32 = 86;

    #[derive(Debug)]
    struct JourneyError(String);

    impl fmt::Display for JourneyError {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(&self.0)
        }
    }

    impl Error for JourneyError {}

    impl JourneyError {
        fn from_display(error: impl fmt::Display) -> Self {
            Self(error.to_string())
        }
    }

    struct JourneyBackend {
        model: Model,
        state: StateStore,
        vault: VaultStore,
        loaded: LoadedPage,
        page_id: PageId,
        page_path: PathBuf,
        version: Version,
        clock_ms: u64,
        receipts: Vec<CommitReceipt>,
        events: Vec<SimInputEvent>,
    }

    impl JourneyBackend {
        fn open(root: &Path, database: &Path) -> Result<Self, JourneyError> {
            let page_id = PageId::new("synthetic-page").map_err(JourneyError::from_display)?;
            let state = StateStore::open(database).map_err(JourneyError::from_display)?;
            let mut vault = VaultStore::open(root).map_err(JourneyError::from_display)?;
            let loaded = vault
                .load_page(&state, page_id.clone(), "note.md")
                .map_err(JourneyError::from_display)?;
            let model = Model::new(loaded.text());
            Ok(Self {
                model,
                state,
                vault,
                loaded,
                page_id,
                page_path: root.join("note.md"),
                version: Version::new(1).map_err(JourneyError::from_display)?,
                clock_ms: 0,
                receipts: Vec::new(),
                events: Vec::new(),
            })
        }

        fn flush_state(&mut self) -> Result<(), JourneyError> {
            self.state.flush().map_err(JourneyError::from_display)?;
            for receipt in self.receipts.drain(..) {
                receipt.wait().map_err(JourneyError::from_display)?;
            }
            Ok(())
        }

        fn check_kernel_invariants(&self) -> Result<(), JourneyError> {
            let now = Duration::from_millis(self.clock_ms);
            if self
                .model
                .next_deadline()
                .is_some_and(|deadline| deadline <= now)
            {
                return Err(JourneyError(
                    "I10: an armed timer is due after the update".to_owned(),
                ));
            }
            check_view_purity(&self.model).map_err(JourneyError::from_display)
        }

        fn check_page_sync(&self) -> Result<(), JourneyError> {
            let disk = fs::read(&self.page_path).map_err(JourneyError::from_display)?;
            if !self.loaded.sync().is_dirty(self.version)
                && hash(&disk) != self.loaded.sync().base()
            {
                return Err(JourneyError(
                    "I5: clean page does not match the observed disk base".to_owned(),
                ));
            }
            if let Some(SaveConflict::ExternalChange { base, observed }) =
                self.loaded.sync().conflict()
            {
                if *base != self.loaded.sync().base() || *observed != hash(&disk) {
                    return Err(JourneyError(
                        "I5: conflict does not describe the observed disk bytes".to_owned(),
                    ));
                }
            }
            Ok(())
        }

        fn save(&mut self) -> Result<(), JourneyError> {
            let before = fs::read(&self.page_path).map_err(JourneyError::from_display)?;
            let base_matches = hash(&before) == self.loaded.sync().base();
            let outcome = self
                .vault
                .save_page(self.loaded.sync_mut(), &self.state, self.version)
                .map_err(JourneyError::from_display)?;
            let after = fs::read(&self.page_path).map_err(JourneyError::from_display)?;

            if base_matches {
                match outcome {
                    SaveOutcome::Saved {
                        version,
                        hash: saved_hash,
                    } if version == self.version && saved_hash == hash(&after) => {}
                    other => {
                        return Err(JourneyError(format!(
                            "I9: save did not persist its version on the observed base: {other:?}"
                        )));
                    }
                }
            } else if !matches!(outcome, SaveOutcome::Conflict(_)) || after != before {
                return Err(JourneyError(
                    "I9: save overwrote bytes that differed from its observed base".to_owned(),
                ));
            }

            for receipt in self.receipts.drain(..) {
                receipt.wait().map_err(JourneyError::from_display)?;
            }
            Ok(())
        }
    }

    impl SimInputBackend for JourneyBackend {
        type Error = JourneyError;

        fn apply(&mut self, event: SimInputEvent) -> Result<(), Self::Error> {
            self.events.push(event.clone());
            match event {
                SimInputEvent::Text(text) => {
                    update(
                        &mut self.model,
                        Event::InsertText(text),
                        Duration::from_millis(self.clock_ms),
                    )
                    .map_err(|error| JourneyError(format!("{error:?}")))?;
                    let parent = self.version;
                    self.version =
                        Version::new(parent.get() + 1).map_err(JourneyError::from_display)?;
                    let record = VersionRecord::new(
                        self.page_id.clone(),
                        self.version,
                        Some(parent),
                        i64::try_from(self.clock_ms).map_err(JourneyError::from_display)?,
                        TransactionId::new(format!("journey-{}", self.version.get()))
                            .map_err(JourneyError::from_display)?,
                        Origin::User,
                        self.model.text(),
                        None,
                    )
                    .map_err(JourneyError::from_display)?;
                    let receipt = self
                        .state
                        .try_submit(record)
                        .map_err(JourneyError::from_display)?;
                    self.receipts.push(receipt);
                }
                SimInputEvent::AdvanceTime(millis) => {
                    self.clock_ms = self
                        .clock_ms
                        .checked_add(millis)
                        .ok_or_else(|| JourneyError("logical clock overflow".to_owned()))?;
                    let effects = update(
                        &mut self.model,
                        Event::TimerWake,
                        Duration::from_millis(self.clock_ms),
                    )
                    .map_err(|error| JourneyError(format!("{error:?}")))?;
                    if effects
                        .iter()
                        .any(|effect| matches!(effect, Effect::TimerFired(_)))
                    {
                        self.flush_state()?;
                    }
                }
                SimInputEvent::FlushBackground => self.flush_state()?,
                SimInputEvent::Keys(keys) if keys == ":w" => self.save()?,
                SimInputEvent::Keys(keys) => {
                    return Err(JourneyError(format!(
                        "unsupported synthetic key sequence {keys:?}"
                    )));
                }
            }

            self.check_kernel_invariants()?;
            self.check_page_sync()?;
            emit_progress_if_watched();
            Ok(())
        }

        fn screen(&mut self, _width: u16, _height: u16) -> Result<ScreenSnapshot, Self::Error> {
            Ok(ScreenSnapshot::Unavailable(
                UnavailableReason::LayoutNotImplemented,
            ))
        }

        fn buffer_text(&self) -> Result<BufferSnapshot, Self::Error> {
            Ok(BufferSnapshot::Available(self.model.text()))
        }
    }

    fn recorded_journey() -> Vec<SimInputEvent> {
        vec![
            SimInputEvent::Text(DRAFT_TEXT.to_owned()),
            SimInputEvent::AdvanceTime(300),
            SimInputEvent::FlushBackground,
            SimInputEvent::Keys(":w".to_owned()),
        ]
    }

    fn dispatch_trace(
        sim: &mut SimInput<JourneyBackend>,
        trace: &[SimInputEvent],
    ) -> Result<(), SimInputError<JourneyError>> {
        for event in trace {
            match event {
                SimInputEvent::Keys(keys) => {
                    sim.keys(keys)?;
                }
                SimInputEvent::Text(text) => {
                    sim.type_text(text)?;
                }
                SimInputEvent::AdvanceTime(millis) => {
                    sim.tick(*millis)?;
                }
                SimInputEvent::FlushBackground => {
                    sim.flush_background()?;
                }
            }
        }
        Ok(())
    }

    fn initialize_fixture(directory: &TempDir) -> (PathBuf, PathBuf) {
        let root = directory.path().join("vault");
        let database = directory.path().join("state.db");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("note.md"), INITIAL_TEXT).unwrap();

        let page_id = PageId::new("synthetic-page").unwrap();
        let state = StateStore::open(&database).unwrap();
        let first = Version::new(1).unwrap();
        state
            .try_submit(
                VersionRecord::new(
                    page_id.clone(),
                    first,
                    None,
                    0,
                    TransactionId::new("initial").unwrap(),
                    Origin::User,
                    INITIAL_TEXT.to_owned(),
                    None,
                )
                .unwrap(),
            )
            .unwrap()
            .wait()
            .unwrap();
        state.mark_saved(page_id, first).unwrap();
        state.shutdown().unwrap();
        (root, database)
    }

    fn run_trace(
        root: &Path,
        database: &Path,
        trace: &[SimInputEvent],
    ) -> Result<(String, Vec<SimInputEvent>, Version), JourneyError> {
        let backend = JourneyBackend::open(root, database)?;
        let mut sim = SimInput::new(backend);
        dispatch_trace(&mut sim, trace).map_err(JourneyError::from_display)?;
        let screen = sim.screen(80, 24).map_err(JourneyError::from_display)?;
        if screen.snapshot()
            != &ScreenSnapshot::Unavailable(UnavailableReason::LayoutNotImplemented)
        {
            return Err(JourneyError(
                "test backend fabricated layout before the GUI adapter exists".to_owned(),
            ));
        }
        let text = sim.buffer_text().map_err(JourneyError::from_display)?;
        let recorded = sim.backend().events.clone();
        let version = sim.backend().version;
        if recorded != trace {
            return Err(JourneyError(
                "replayed SimInput events differ from the recorded trace".to_owned(),
            ));
        }
        Ok((text, recorded, version))
    }

    fn create_child(test_name: &str) -> Command {
        let mut command = Command::new(env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg(test_name)
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env("BLOOM_JOURNEY_WATCHDOG", "1");
        command
    }

    fn limits() -> WatchdogLimits {
        WatchdogLimits {
            timeout: Duration::from_secs(15),
            max_steps: 16,
            max_queue_depth: 1,
            max_retained_documents: 1,
        }
    }

    fn crash_command(point: &str, root: &Path, database: &Path) -> Command {
        let mut command = create_child("journey_tests::crash_worker");
        command
            .env("BLOOM_JOURNEY_CRASH_POINT", point)
            .env("BLOOM_JOURNEY_ROOT", root)
            .env("BLOOM_JOURNEY_DB", database);
        match point {
            "version-before-commit" => {
                command.env("BLOOM_STATE_CRASH_AT", "before-commit");
            }
            "version-after-commit" => {
                command.env("BLOOM_STATE_CRASH_AT", "after-commit");
            }
            "saved-before-commit" => {
                command.env("BLOOM_STATE_CRASH_AT", "before-saved-commit");
            }
            "saved-after-commit" => {
                command.env("BLOOM_STATE_CRASH_AT", "after-saved-commit");
            }
            "temp-before-sync" => {
                command.env("BLOOM_VAULT_CRASH_AT", "before-temp-sync");
            }
            "temp-after-sync" => {
                command.env("BLOOM_VAULT_CRASH_AT", "after-temp-sync");
            }
            "replace-before" => {
                command.env("BLOOM_VAULT_CRASH_AT", "before-replace");
            }
            "replace-after" => {
                command.env("BLOOM_VAULT_CRASH_AT", "after-replace");
            }
            "readback-before" => {
                command.env("BLOOM_VAULT_CRASH_AT", "before-readback");
            }
            "readback-after" => {
                command.env("BLOOM_VAULT_CRASH_AT", "after-readback");
            }
            _ => panic!("unknown crash point {point}"),
        };
        command
    }

    fn recovery_command(point: &str, root: &Path, database: &Path) -> Command {
        let mut command = create_child("journey_tests::recovery_and_replay_worker");
        command
            .env("BLOOM_JOURNEY_CRASH_POINT", point)
            .env("BLOOM_JOURNEY_ROOT", root)
            .env("BLOOM_JOURNEY_DB", database);
        command
    }

    fn emit_progress_if_watched() {
        if env::var_os("BLOOM_JOURNEY_WATCHDOG").is_none() {
            return;
        }
        let stdout = std::io::stdout();
        let mut output = stdout.lock();
        writeln!(output, "BLOOM_WATCHDOG/1 PROGRESS queue=0 retained=1 due=0").unwrap();
        output.flush().unwrap();
    }

    fn emit(message: &str) {
        let stdout = std::io::stdout();
        let mut output = stdout.lock();
        writeln!(output, "BLOOM_WATCHDOG/1 {message}").unwrap();
        output.flush().unwrap();
    }

    fn assert_database_integrity(database: &Path) {
        let connection = Connection::open(database).unwrap();
        let result: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(result, "ok");
    }

    #[test]
    fn process_isolated_crash_recovery_matrix_and_replay() {
        let crash_points = [
            "version-before-commit",
            "version-after-commit",
            "temp-before-sync",
            "temp-after-sync",
            "replace-before",
            "replace-after",
            "readback-before",
            "readback-after",
            "saved-before-commit",
            "saved-after-commit",
        ];

        for point in crash_points {
            let directory = tempdir().unwrap();
            let (root, database) = initialize_fixture(&directory);

            assert_eq!(
                run_watchdog(crash_command(point, &root, &database), limits()),
                WatchdogOutcome::ExitedWithoutOutcome {
                    exit_code: Some(EXPECTED_EXIT)
                },
                "crash injection did not stop at {point}"
            );
            assert_eq!(
                run_watchdog(recovery_command(point, &root, &database), limits()),
                WatchdogOutcome::Succeeded { steps: 4 },
                "recovery or replay failed after {point}"
            );
        }
    }

    #[test]
    fn crash_worker() {
        let Ok(point) = env::var("BLOOM_JOURNEY_CRASH_POINT") else {
            return;
        };
        let root = PathBuf::from(env::var_os("BLOOM_JOURNEY_ROOT").unwrap());
        let database = PathBuf::from(env::var_os("BLOOM_JOURNEY_DB").unwrap());
        assert!([
            "version-before-commit",
            "version-after-commit",
            "temp-before-sync",
            "temp-after-sync",
            "replace-before",
            "replace-after",
            "readback-before",
            "readback-after",
            "saved-before-commit",
            "saved-after-commit",
        ]
        .contains(&point.as_str()));

        let mut sim = SimInput::new(JourneyBackend::open(&root, &database).unwrap());
        dispatch_trace(&mut sim, &recorded_journey()).unwrap();
        panic!("crash hook {point} was not reached");
    }

    #[test]
    fn recovery_and_replay_worker() {
        let Ok(point) = env::var("BLOOM_JOURNEY_CRASH_POINT") else {
            return;
        };
        let root = PathBuf::from(env::var_os("BLOOM_JOURNEY_ROOT").unwrap());
        let database = PathBuf::from(env::var_os("BLOOM_JOURNEY_DB").unwrap());
        let page_id = PageId::new("synthetic-page").unwrap();
        let first = Version::new(1).unwrap();
        let second = Version::new(2).unwrap();
        let candidate_expected = point != "version-before-commit" && point != "saved-after-commit";
        let disk_has_draft = matches!(
            point.as_str(),
            "replace-after"
                | "readback-before"
                | "readback-after"
                | "saved-before-commit"
                | "saved-after-commit"
        );
        let saved_version = if point == "saved-after-commit" {
            second
        } else {
            first
        };

        let state = StateStore::open(&database).unwrap();
        let candidate = state.recovery_candidate(page_id.clone()).unwrap();
        assert_eq!(candidate.is_some(), candidate_expected, "point={point}");
        assert_eq!(
            state
                .saved_record(page_id.clone())
                .unwrap()
                .unwrap()
                .version(),
            saved_version,
            "point={point}"
        );
        if let Some(candidate) = &candidate {
            assert_eq!(candidate.version(), second);
            assert_eq!(candidate.content(), DRAFT_TEXT);
        }

        let page_path = root.join("note.md");
        let disk = fs::read(&page_path).unwrap();
        assert_eq!(
            disk.as_slice(),
            if disk_has_draft {
                DRAFT_TEXT.as_bytes()
            } else {
                INITIAL_TEXT.as_bytes()
            },
            "I2: torn or unexpected page bytes after {point}"
        );

        let mut vault = VaultStore::open(&root).unwrap();
        let mut loaded = vault.load_page(&state, page_id.clone(), "note.md").unwrap();
        let disk_hash = hash(&disk);
        if saved_version == second {
            assert_eq!(loaded.sync().saved_version(), Some(second));
            assert_eq!(loaded.sync().base(), disk_hash);
            assert!(loaded.sync().conflict().is_none());
        } else if disk_has_draft {
            assert_eq!(loaded.sync().saved_version(), None);
            assert_eq!(loaded.sync().base(), hash(INITIAL_TEXT.as_bytes()));
            assert_eq!(
                loaded.sync().conflict(),
                Some(&SaveConflict::ExternalChange {
                    base: hash(INITIAL_TEXT.as_bytes()),
                    observed: disk_hash
                })
            );
        } else {
            assert_eq!(loaded.sync().saved_version(), Some(first));
            assert_eq!(loaded.sync().base(), disk_hash);
            assert!(loaded.sync().conflict().is_none());
        }

        if let Some(candidate) = candidate {
            let before = fs::read(&page_path).unwrap();
            let outcome = vault
                .save_page(loaded.sync_mut(), &state, candidate.version())
                .unwrap();
            if disk_has_draft {
                assert!(matches!(outcome, SaveOutcome::Conflict(_)));
                assert_eq!(
                    fs::read(&page_path).unwrap(),
                    before,
                    "I9: recovery overwrote observed bytes after {point}"
                );
            } else {
                assert!(matches!(outcome, SaveOutcome::Saved { version, .. } if version == second));
                assert_eq!(fs::read(&page_path).unwrap(), DRAFT_TEXT.as_bytes());
                assert_eq!(loaded.sync().saved_version(), Some(second));
            }
        }

        state.shutdown().unwrap();
        assert_database_integrity(&database);
        drop(loaded);
        drop(vault);

        for point in ["temp-before-sync", "temp-after-sync", "replace-before"] {
            if point == env::var("BLOOM_JOURNEY_CRASH_POINT").unwrap() {
                assert!(fs::read_dir(&root)
                    .unwrap()
                    .map(Result::unwrap)
                    .any(|entry| entry.file_name().to_string_lossy().ends_with(".bloom-tmp")));
            }
        }

        let replay_directory = tempdir().unwrap();
        let (replay_root, replay_database) = initialize_fixture(&replay_directory);
        let trace = recorded_journey();
        let (text, events, version) = run_trace(&replay_root, &replay_database, &trace).unwrap();
        assert_eq!(text, DRAFT_TEXT);
        assert_eq!(events, trace);
        assert_eq!(version, second);

        let replay_state = StateStore::open(&replay_database).unwrap();
        assert!(replay_state
            .recovery_candidate(page_id.clone())
            .unwrap()
            .is_none());
        assert_eq!(
            replay_state
                .saved_record(page_id)
                .unwrap()
                .unwrap()
                .version(),
            second
        );
        replay_state.shutdown().unwrap();
        assert_database_integrity(&replay_database);
        assert_eq!(
            fs::read(replay_root.join("note.md")).unwrap(),
            DRAFT_TEXT.as_bytes()
        );

        emit("SUCCEEDED");
    }
}
