//! Vault initialization and startup.
//!
//! Opens or creates the vault directory, sets up the SQLite index, journal,
//! template engine, file store, background disk-writer thread, and file watcher.
//! Routes startup to the setup wizard, journal, or session restore as appropriate.

use crate::*;

/// Set the hidden file attribute on Windows.
#[cfg(windows)]
fn set_hidden_attribute(path: &std::path::Path) -> std::io::Result<()> {
    std::process::Command::new("attrib")
        .arg("+H")
        .arg(path.as_os_str())
        .output()?;
    Ok(())
}

impl BloomEditor {
    /// Initialize with a vault path — sets up index, journal, template engine.
    /// Spawns the background indexer thread (non-blocking).
    pub fn init_vault(&mut self, vault_root: &std::path::Path) -> Result<(), error::BloomError> {
        tracing::info!(vault = %vault_root.display(), "vault initializing");

        // Auto-migrate legacy vault layout before acquiring the lock.
        vault::migrate::migrate_if_needed(vault_root);

        // Acquire single-instance lock before touching any vault state.
        let lock = vault::lock::VaultLock::acquire(vault_root)
            .map_err(|e| error::BloomError::VaultError(e.to_string()))?;
        self.vault_lock = Some(lock);

        let index_path = vault::paths::index_db(vault_root);
        // Ensure the database file exists with schema before opening read-only.
        // The indexer thread will own the read-write connection for all mutations.
        {
            let rw = index::Index::open(&index_path)?;
            drop(rw); // close the read-write connection immediately
        }
        // UI thread gets a read-only connection. The indexer thread owns the
        // read-write connection and handles all writes (index, undo trees).
        self.index = Some(index::Index::open_readonly(&index_path)?);

        self.journal = Some(journal::Journal::new(vault_root));
        let file_store = bloom_store::local::LocalFileStore::new(vault_root.to_path_buf())?;
        // Grab the watcher receiver once — must not call watch() repeatedly
        {
            use bloom_store::traits::NoteStore;
            self.watcher_rx = Some(file_store.watch());
        }
        self.note_store = Some(file_store);
        let templates_dir = vault_root.join("templates");
        self.template_engine = Some(template::TemplateEngine::new(&templates_dir));
        self.vault_root = Some(vault_root.to_path_buf());

        // Start auto-save disk writer thread
        let (writer, tx, ack_rx) =
            bloom_store::disk_writer::DiskWriter::new(self.config.autosave_debounce_ms);
        self.autosave_tx = Some(tx);
        self.write_complete_rx = Some(ack_rx);
        std::thread::Builder::new()
            .name("bloom-disk-writer".into())
            .spawn(move || writer.start())
            .ok();

        // Mark .index directory as hidden on Windows
        #[cfg(windows)]
        {
            let _ = set_hidden_attribute(&vault::paths::index_dir(vault_root));
        }

        // Spawn long-lived background indexer thread
        let (idx_completion_tx, idx_completion_rx) = crossbeam::channel::bounded(4);
        self.indexer_rx = Some(idx_completion_rx);
        self.indexing = true;
        let indexer_tx =
            index::indexer::spawn_indexer(vault_root.to_path_buf(), index_path, idx_completion_tx);
        self.indexer_tx = Some(indexer_tx);

        // Prune undo trees older than 24 hours on startup.
        if let Some(tx) = &self.indexer_tx {
            use std::time::{SystemTime, UNIX_EPOCH};
            let cutoff_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
                - 24 * 60 * 60 * 1000;
            let _ = tx.send(index::indexer::IndexRequest::PruneUndoBefore(cutoff_ms));
        }

        // Spawn long-lived history thread (git-backed time travel)
        let (hist_completion_tx, hist_completion_rx) = crossbeam::channel::bounded(4);
        self.history_rx = Some(hist_completion_rx);
        let history_tx = history::spawn_history_thread(
            vault::paths::index_dir(vault_root),
            self.config.history.auto_commit_idle_minutes,
            self.config.history.max_commit_interval_minutes,
            hist_completion_tx,
        );
        self.history_tx = Some(history_tx);

        Ok(())
    }

    /// Check if the setup wizard should run (no vault at default path).
    pub fn needs_setup(&self) -> bool {
        let default = default_vault_path();
        let root = std::path::Path::new(&default);
        !root.join("config.toml").exists()
    }

    /// Start the setup wizard.
    pub fn start_wizard(&mut self) {
        self.wizard = Some(SetupWizardState::new());
    }

    /// Whether the wizard is currently active.
    pub fn wizard_active(&self) -> bool {
        self.wizard.is_some()
    }

    /// Perform startup according to config. Guarantees `active_page` is `Some` on return.
    pub fn startup(&mut self) {
        match self.config.startup.mode {
            config::StartupMode::Journal => self.open_journal_today(),
            config::StartupMode::Restore => {
                if self.restore_session().is_err() || self.active_page().is_none() {
                    self.open_scratch_buffer();
                }
            }
            config::StartupMode::Blank => self.open_scratch_buffer(),
        }
    }

    pub(crate) fn complete_wizard(&mut self) {
        let vault_path = self
            .wizard
            .as_ref()
            .map(|w| expand_tilde(&w.vault_path))
            .unwrap_or_else(default_vault_path);
        self.wizard = None;

        let root = std::path::PathBuf::from(&vault_path);
        let _ = self.init_vault(&root);
        self.startup();
    }
}
