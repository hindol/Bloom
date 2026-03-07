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
        let index_path = vault_root.join(".index.db");
        self.index = Some(index::Index::open(&index_path)?);
        self.journal = Some(journal::Journal::new(vault_root));
        let file_store = store::local::LocalFileStore::new(vault_root.to_path_buf())?;
        // Grab the watcher receiver once — must not call watch() repeatedly
        {
            use store::traits::NoteStore;
            self.watcher_rx = Some(file_store.watch());
        }
        self.note_store = Some(file_store);
        let templates_dir = vault_root.join("templates");
        self.template_engine = Some(template::TemplateEngine::new(&templates_dir));
        self.vault_root = Some(vault_root.to_path_buf());

        // Start auto-save disk writer thread
        let (writer, tx, ack_rx) =
            store::disk_writer::DiskWriter::new(self.config.autosave_debounce_ms);
        self.autosave_tx = Some(tx);
        self.write_complete_rx = Some(ack_rx);
        std::thread::Builder::new()
            .name("bloom-disk-writer".into())
            .spawn(move || writer.start())
            .ok();

        // Mark index file as hidden on Windows
        #[cfg(windows)]
        {
            let _ = set_hidden_attribute(&index_path);
        }

        // Spawn long-lived background indexer thread
        let (idx_completion_tx, idx_completion_rx) = crossbeam::channel::bounded(4);
        self.indexer_rx = Some(idx_completion_rx);
        self.indexing = true;
        let indexer_tx =
            index::indexer::spawn_indexer(vault_root.to_path_buf(), index_path, idx_completion_tx);
        self.indexer_tx = Some(indexer_tx);

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
                if self.restore_session().is_err() || self.active_page.is_none() {
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
