//! UUID-based git history repository.
//!
//! Files are stored in the git tree under `{uuid}.md`. The working tree
//! is never used — all operations go through gix's object database directly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bstr::BString;
use gix::date::Time as GitTime;
use gix::objs::tree::EntryKind;

/// Information about a single commit in the history.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub oid: String,
    pub message: String,
    /// Author timestamp (Unix seconds).
    pub timestamp: i64,
    /// UUID filenames changed in this commit (e.g. `["8f3a1b2c.md"]`).
    pub changed_files: Vec<String>,
}

/// Errors from history operations.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("git error: {0}")]
    Git(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// A git repository storing vault history under `.index/.git/`.
///
/// Files are stored by UUID (e.g. `8f3a1b2c.md`) — never by filesystem path.
/// This makes history immune to page renames.
pub struct HistoryRepo {
    index_dir: PathBuf,
}

impl HistoryRepo {
    /// Open or initialize the history repository.
    pub fn open(index_dir: &Path) -> Result<Self, HistoryError> {
        let git_dir = index_dir.join(".git");
        if !git_dir.exists() {
            Self::init_repo(index_dir)?;
            tracing::info!(path = %git_dir.display(), "initialized history repository");
        }
        Ok(HistoryRepo {
            index_dir: index_dir.to_path_buf(),
        })
    }

    /// Stage files (by UUID) and create a commit.
    ///
    /// `files`: `(uuid_hex, content)` pairs — stored as `{uuid}.md`.
    /// Files not in the list are preserved from the parent tree.
    /// Returns `None` if there were no changes.
    pub fn commit_all(
        &self,
        files: &[(&str, &str)],
        message: &str,
        timestamp: Option<i64>,
    ) -> Result<Option<String>, HistoryError> {
        let repo = self.open_gix()?;

        let parent_tree_id = self.head_tree_id(&repo)?;
        let new_tree_id = self.build_tree(&repo, parent_tree_id, files)?;

        // No changes — skip commit.
        if Some(new_tree_id) == parent_tree_id {
            tracing::debug!("no changes to commit — tree unchanged");
            return Ok(None);
        }

        let head_commit = self.head_commit_id(&repo)?;
        let oid = self.create_commit(&repo, new_tree_id, head_commit, message, timestamp)?;

        tracing::info!(oid = %oid, files = files.len(), "history commit created");
        Ok(Some(oid))
    }

    /// List commits touching a specific UUID file, newest first.
    /// If `uuid` is `None`, returns all commits. Returns up to `limit` entries.
    pub fn page_history(
        &self,
        uuid: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CommitInfo>, HistoryError> {
        let repo = self.open_gix()?;
        let head = match self.head_commit_id(&repo)? {
            Some(id) => id,
            None => return Ok(vec![]),
        };

        let filename = uuid.map(|u| format!("{u}.md"));
        let mut results = Vec::new();

        let walk = repo
            .rev_walk([head])
            .all()
            .map_err(|e| HistoryError::Git(e.to_string()))?;

        for info in walk {
            if results.len() >= limit {
                break;
            }
            let info = info.map_err(|e| HistoryError::Git(e.to_string()))?;
            let commit_obj = info
                .id()
                .object()
                .map_err(|e| HistoryError::Git(e.to_string()))?;
            let commit = commit_obj.into_commit();

            let tree_id: gix::ObjectId = commit
                .tree_id()
                .map_err(|e| HistoryError::Git(e.to_string()))?
                .detach();

            let parent_tree_id: Option<gix::ObjectId> = commit
                .parent_ids()
                .next()
                .and_then(|pid| {
                    pid.object()
                        .ok()?
                        .into_commit()
                        .tree_id()
                        .ok()
                        .map(|id| id.detach())
                });

            let changed = self.diff_tree_ids(&repo, parent_tree_id, tree_id)?;

            if let Some(ref fname) = filename {
                if !changed.iter().any(|f| f == fname) {
                    continue;
                }
            }

            let message = commit
                .message_raw()
                .map(|m| m.to_string())
                .unwrap_or_default();
            let time = commit.time().map_err(|e| HistoryError::Git(e.to_string()))?;

            results.push(CommitInfo {
                oid: info.id().to_string(),
                message,
                timestamp: time.seconds,
                changed_files: changed,
            });
        }

        Ok(results)
    }

    /// Retrieve the content of a UUID file at a specific commit.
    pub fn blob_at(&self, oid: &str, uuid: &str) -> Result<Option<String>, HistoryError> {
        let repo = self.open_gix()?;
        let commit_id = gix::ObjectId::from_hex(oid.as_bytes())
            .map_err(|e| HistoryError::Git(e.to_string()))?;
        let commit = repo
            .find_object(commit_id)
            .map_err(|e| HistoryError::Git(e.to_string()))?
            .into_commit();
        let tree = commit
            .tree()
            .map_err(|e| HistoryError::Git(e.to_string()))?;

        let filename = format!("{uuid}.md");
        let entry = tree
            .lookup_entry_by_path(&filename)
            .map_err(|e| HistoryError::Git(e.to_string()))?;

        match entry {
            Some(e) => {
                let blob = e.object().map_err(|e| HistoryError::Git(e.to_string()))?;
                Ok(Some(String::from_utf8_lossy(&blob.data).to_string()))
            }
            None => Ok(None),
        }
    }

    /// Run `git gc` (repack loose objects). Placeholder for now.
    pub fn gc(&self) -> Result<(), HistoryError> {
        tracing::debug!("history gc: no-op (placeholder)");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn open_gix(&self) -> Result<gix::Repository, HistoryError> {
        gix::open(&self.index_dir).map_err(|e| HistoryError::Git(e.to_string()))
    }

    fn init_repo(index_dir: &Path) -> Result<(), HistoryError> {
        std::fs::create_dir_all(index_dir)?;

        // gix::init(path) creates a repo with work tree at `path` and
        // git dir at `path/.git/`. We init at index_dir so the git repo
        // lives at `.index/.git/`.
        let repo = gix::init(index_dir).map_err(|e| HistoryError::Git(e.to_string()))?;

        // Write an empty tree.
        let empty_tree_id = repo
            .write_object(gix::objs::Tree::empty())
            .map_err(|e| HistoryError::Git(e.to_string()))?;

        let sig = gix::actor::Signature {
            name: "Bloom".into(),
            email: "bloom@local".into(),
            time: GitTime::now_local_or_utc(),
        };

        let commit = gix::objs::Commit {
            tree: empty_tree_id.detach(),
            parents: Default::default(),
            author: sig.clone(),
            committer: sig,
            encoding: None,
            message: "initial empty commit".into(),
            extra_headers: vec![],
        };
        let commit_id = repo
            .write_object(&commit)
            .map_err(|e| HistoryError::Git(e.to_string()))?;

        let ref_name: gix::refs::FullName = "refs/heads/main"
            .try_into()
            .map_err(|e: gix::validate::reference::name::Error| HistoryError::Git(e.to_string()))?;
        repo.reference(
            ref_name,
            commit_id,
            gix::refs::transaction::PreviousValue::MustNotExist,
            "initial commit",
        )
        .map_err(|e| HistoryError::Git(e.to_string()))?;

        let git_dir = index_dir.join(".git");
        std::fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n")?;
        Ok(())
    }

    fn head_commit_id(
        &self,
        repo: &gix::Repository,
    ) -> Result<Option<gix::ObjectId>, HistoryError> {
        match repo.head_id() {
            Ok(id) => Ok(Some(id.detach())),
            Err(_) => Ok(None),
        }
    }

    fn head_tree_id(
        &self,
        repo: &gix::Repository,
    ) -> Result<Option<gix::ObjectId>, HistoryError> {
        let head = match self.head_commit_id(repo)? {
            Some(id) => id,
            None => return Ok(None),
        };
        let commit = repo
            .find_object(head)
            .map_err(|e| HistoryError::Git(e.to_string()))?
            .into_commit();
        let tree_id = commit
            .tree_id()
            .map_err(|e| HistoryError::Git(e.to_string()))?;
        Ok(Some(tree_id.detach()))
    }

    fn read_tree_entries(
        &self,
        repo: &gix::Repository,
        tree_id: gix::ObjectId,
    ) -> Result<BTreeMap<BString, (EntryKind, gix::ObjectId)>, HistoryError> {
        let tree_obj = repo
            .find_object(tree_id)
            .map_err(|e| HistoryError::Git(e.to_string()))?
            .into_tree();
        let decoded = tree_obj
            .decode()
            .map_err(|e| HistoryError::Git(e.to_string()))?;

        let mut entries = BTreeMap::new();
        for entry in &decoded.entries {
            entries.insert(
                BString::from(entry.filename.to_vec()),
                (entry.mode.kind(), entry.oid.into()),
            );
        }
        Ok(entries)
    }

    fn build_tree(
        &self,
        repo: &gix::Repository,
        parent_tree_id: Option<gix::ObjectId>,
        files: &[(&str, &str)],
    ) -> Result<gix::ObjectId, HistoryError> {
        let mut entries: BTreeMap<BString, (EntryKind, gix::ObjectId)> = match parent_tree_id {
            Some(id) => self.read_tree_entries(repo, id)?,
            None => BTreeMap::new(),
        };

        for (uuid, content) in files {
            let filename = format!("{uuid}.md");
            let blob_id = repo
                .write_blob(content.as_bytes())
                .map_err(|e| HistoryError::Git(e.to_string()))?;
            entries.insert(
                BString::from(filename.into_bytes()),
                (EntryKind::Blob, blob_id.detach()),
            );
        }

        let tree = gix::objs::Tree {
            entries: entries
                .into_iter()
                .map(|(name, (kind, oid))| gix::objs::tree::Entry {
                    mode: kind.into(),
                    filename: name,
                    oid,
                })
                .collect(),
        };

        let tree_id = repo
            .write_object(&tree)
            .map_err(|e| HistoryError::Git(e.to_string()))?;
        Ok(tree_id.detach())
    }

    fn create_commit(
        &self,
        repo: &gix::Repository,
        tree_id: gix::ObjectId,
        parent: Option<gix::ObjectId>,
        message: &str,
        timestamp: Option<i64>,
    ) -> Result<String, HistoryError> {
        let time = match timestamp {
            Some(ts) => GitTime::new(ts, 0),
            None => GitTime::now_local_or_utc(),
        };

        let sig = gix::actor::Signature {
            name: "Bloom".into(),
            email: "bloom@local".into(),
            time,
        };

        let parents: smallvec::SmallVec<[gix::ObjectId; 1]> = match parent {
            Some(p) => smallvec::smallvec![p],
            None => smallvec::SmallVec::new(),
        };

        let commit = gix::objs::Commit {
            tree: tree_id,
            parents,
            author: sig.clone(),
            committer: sig,
            encoding: None,
            message: message.into(),
            extra_headers: vec![],
        };

        let commit_id = repo
            .write_object(&commit)
            .map_err(|e| HistoryError::Git(e.to_string()))?;

        let ref_name: gix::refs::FullName = "refs/heads/main"
            .try_into()
            .map_err(|e: gix::validate::reference::name::Error| HistoryError::Git(e.to_string()))?;
        repo.reference(
            ref_name,
            commit_id,
            gix::refs::transaction::PreviousValue::Any,
            message,
        )
        .map_err(|e| HistoryError::Git(e.to_string()))?;

        Ok(commit_id.to_string())
    }

    fn diff_tree_ids(
        &self,
        repo: &gix::Repository,
        parent_tree_id: Option<gix::ObjectId>,
        current_tree_id: gix::ObjectId,
    ) -> Result<Vec<String>, HistoryError> {
        let current = self.read_tree_entries(repo, current_tree_id)?;

        match parent_tree_id {
            None => Ok(current.keys().map(|k| k.to_string()).collect()),
            Some(parent_id) => {
                let parent = self.read_tree_entries(repo, parent_id)?;
                let mut changed = Vec::new();

                for (name, (_, oid)) in &current {
                    match parent.get(name) {
                        Some((_, parent_oid)) if parent_oid == oid => {}
                        _ => changed.push(name.to_string()),
                    }
                }
                for name in parent.keys() {
                    if !current.contains_key(name) {
                        changed.push(name.to_string());
                    }
                }

                Ok(changed)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, HistoryRepo) {
        let dir = TempDir::new().unwrap();
        let index_dir = dir.path().join(".index");
        std::fs::create_dir_all(&index_dir).unwrap();
        let repo = HistoryRepo::open(&index_dir).unwrap();
        (dir, repo)
    }

    #[test]
    fn init_creates_git_dir() {
        let (dir, _repo) = setup();
        assert!(dir.path().join(".index/.git").exists());
    }

    #[test]
    fn commit_and_revwalk() {
        let (_dir, repo) = setup();

        let oid = repo
            .commit_all(
                &[("aabbccdd", "# Hello\nWorld")],
                "added hello page",
                Some(1000),
            )
            .unwrap();
        assert!(oid.is_some());

        let oid2 = repo
            .commit_all(
                &[("aabbccdd", "# Hello\nWorld\nMore content")],
                "updated hello page",
                Some(2000),
            )
            .unwrap();
        assert!(oid2.is_some());

        // 3 total commits (initial empty + 2 real).
        let history = repo.page_history(None, 100).unwrap();
        assert_eq!(history.len(), 3);

        // Filtered to one UUID — 2 commits.
        let page_hist = repo.page_history(Some("aabbccdd"), 100).unwrap();
        assert_eq!(page_hist.len(), 2);
        assert_eq!(page_hist[0].message, "updated hello page");
        assert_eq!(page_hist[1].message, "added hello page");
    }

    #[test]
    fn commit_no_changes_returns_none() {
        let (_dir, repo) = setup();

        let oid1 = repo
            .commit_all(&[("deadbeef", "content")], "first", Some(1000))
            .unwrap();
        assert!(oid1.is_some());

        let oid2 = repo
            .commit_all(&[("deadbeef", "content")], "duplicate", Some(2000))
            .unwrap();
        assert!(oid2.is_none());
    }

    #[test]
    fn blob_at_retrieves_content() {
        let (_dir, repo) = setup();

        let oid = repo
            .commit_all(&[("abcd1234", "version 1")], "v1", Some(1000))
            .unwrap()
            .unwrap();

        repo.commit_all(&[("abcd1234", "version 2")], "v2", Some(2000))
            .unwrap();

        let content = repo.blob_at(&oid, "abcd1234").unwrap();
        assert_eq!(content.as_deref(), Some("version 1"));
    }

    #[test]
    fn blob_at_missing_file_returns_none() {
        let (_dir, repo) = setup();

        let oid = repo
            .commit_all(&[("abcd1234", "content")], "commit", Some(1000))
            .unwrap()
            .unwrap();

        let content = repo.blob_at(&oid, "nonexist").unwrap();
        assert!(content.is_none());
    }

    #[test]
    fn multiple_files_in_one_commit() {
        let (_dir, repo) = setup();

        let oid = repo
            .commit_all(
                &[
                    ("aaaa0001", "page one"),
                    ("aaaa0002", "page two"),
                    ("aaaa0003", "page three"),
                ],
                "batch commit",
                Some(1000),
            )
            .unwrap()
            .unwrap();

        assert_eq!(repo.blob_at(&oid, "aaaa0001").unwrap().as_deref(), Some("page one"));
        assert_eq!(repo.blob_at(&oid, "aaaa0002").unwrap().as_deref(), Some("page two"));
        assert_eq!(repo.blob_at(&oid, "aaaa0003").unwrap().as_deref(), Some("page three"));
    }

    #[test]
    fn previous_files_preserved_across_commits() {
        let (_dir, repo) = setup();

        repo.commit_all(&[("aaaa0001", "content A")], "add A", Some(1000))
            .unwrap();

        let oid2 = repo
            .commit_all(&[("aaaa0002", "content B")], "add B", Some(2000))
            .unwrap()
            .unwrap();

        assert_eq!(repo.blob_at(&oid2, "aaaa0001").unwrap().as_deref(), Some("content A"));
        assert_eq!(repo.blob_at(&oid2, "aaaa0002").unwrap().as_deref(), Some("content B"));
    }
}
