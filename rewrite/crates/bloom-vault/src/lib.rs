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
        let readback = fs::read(&path).map_err(|source| VaultError::PostReplace {
            operation: "read back page",
            source,
        })?;
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
        state
            .mark_saved(sync.page_id.clone(), version)
            .map_err(VaultError::SavedMarkerAfterWrite)?;
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
    temporary
        .as_file()
        .sync_all()
        .map_err(|source| VaultError::Io {
            operation: "sync temporary page",
            source,
        })?;
    before_replace(temporary.path()).map_err(|source| VaultError::Io {
        operation: "prepare page replacement",
        source,
    })?;
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
    })
}

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
