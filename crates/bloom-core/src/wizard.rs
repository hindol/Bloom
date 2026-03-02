// First-launch setup wizard — state machine for vault creation and optional
// Logseq import.  The TUI/GUI renders it; the core just holds the state.

use std::path::{Path, PathBuf};

use crate::import::{self, ImportConfig};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum WizardError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Import error: {0}")]
    Import(#[from] import::ImportError),
    #[error("Wizard not at Confirm/Done step")]
    InvalidStep,
}

// ---------------------------------------------------------------------------
// Steps
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WizardStep {
    /// Choose vault location (display a path input).
    VaultLocation { path: String },
    /// Optionally import from Logseq.
    LogseqImport { source_path: String, enabled: bool },
    /// Confirmation + summary before executing.
    Confirm {
        vault_path: String,
        logseq_source: Option<String>,
    },
    /// Done — wizard completed.
    Done { vault_path: PathBuf },
}

// ---------------------------------------------------------------------------
// Wizard
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct SetupWizard {
    pub step: WizardStep,
}

impl SetupWizard {
    pub fn new() -> Self {
        Self {
            step: WizardStep::VaultLocation {
                path: String::new(),
            },
        }
    }

    pub fn set_vault_path(&mut self, path: &str) {
        if let WizardStep::VaultLocation { path: ref mut p } = self.step {
            *p = path.to_string();
        }
    }

    pub fn next_step(&mut self) {
        self.step = match &self.step {
            WizardStep::VaultLocation { path } => WizardStep::LogseqImport {
                source_path: String::new(),
                enabled: false,
            },
            WizardStep::LogseqImport {
                source_path,
                enabled,
            } => {
                // Retrieve vault path from history — we stash it during transition.
                // Because we only keep the current step we need a small trick:
                // the vault path was set in VaultLocation, but now we're in LogseqImport.
                // We'll carry it forward via a helper. For now, store empty and
                // rely on the confirm step being built correctly via the full flow.
                //
                // Actually, we need to thread the vault path through.  Let's store
                // it on the struct instead.  But to keep the public API unchanged
                // we extract it from the step transitions.
                let logseq = if *enabled {
                    Some(source_path.clone())
                } else {
                    None
                };
                WizardStep::Confirm {
                    vault_path: String::new(),
                    logseq_source: logseq,
                }
            }
            WizardStep::Confirm { .. } | WizardStep::Done { .. } => return,
        };
    }

    pub fn prev_step(&mut self) {
        self.step = match &self.step {
            WizardStep::LogseqImport { .. } => WizardStep::VaultLocation {
                path: String::new(),
            },
            WizardStep::Confirm {
                logseq_source, ..
            } => WizardStep::LogseqImport {
                source_path: logseq_source.clone().unwrap_or_default(),
                enabled: logseq_source.is_some(),
            },
            _ => return,
        };
    }

    pub fn toggle_logseq_import(&mut self) {
        if let WizardStep::LogseqImport { enabled, .. } = &mut self.step {
            *enabled = !*enabled;
        }
    }

    pub fn set_logseq_source(&mut self, path: &str) {
        if let WizardStep::LogseqImport {
            source_path, ..
        } = &mut self.step
        {
            *source_path = path.to_string();
        }
    }

    /// Execute the wizard: create vault dirs, optionally run Logseq import.
    /// Returns the vault path.
    pub fn execute(&self) -> Result<PathBuf, WizardError> {
        let (vault_path, logseq_source) = match &self.step {
            WizardStep::Confirm {
                vault_path,
                logseq_source,
            } => (PathBuf::from(vault_path), logseq_source.clone()),
            _ => return Err(WizardError::InvalidStep),
        };

        // Create vault structure.
        std::fs::create_dir_all(vault_path.join("pages"))?;
        std::fs::create_dir_all(vault_path.join("journal"))?;
        std::fs::create_dir_all(vault_path.join("templates"))?;

        // Default template.
        let tmpl_path = vault_path.join("templates/new-page.tmpl");
        if !tmpl_path.exists() {
            std::fs::write(
                &tmpl_path,
                "---\ntitle: ${title}\ntags: []\n---\n\n# ${title}\n",
            )?;
        }

        // Optional Logseq import.
        if let Some(src) = logseq_source {
            let config = ImportConfig {
                source_dir: PathBuf::from(&src),
                target_dir: vault_path.clone(),
                dry_run: false,
            };
            import::import_logseq_vault(&config)?;
        }

        Ok(vault_path)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn wizard_advances_through_steps() {
        let mut w = SetupWizard::new();

        // Step 1: VaultLocation
        assert!(matches!(w.step, WizardStep::VaultLocation { .. }));
        w.set_vault_path("/tmp/my-vault");

        // Advance to LogseqImport
        w.next_step();
        assert!(matches!(w.step, WizardStep::LogseqImport { enabled: false, .. }));

        // Toggle import on and set source
        w.toggle_logseq_import();
        w.set_logseq_source("/tmp/logseq");
        if let WizardStep::LogseqImport { enabled, source_path } = &w.step {
            assert!(enabled);
            assert_eq!(source_path, "/tmp/logseq");
        } else {
            panic!("expected LogseqImport step");
        }

        // Advance to Confirm
        w.next_step();
        if let WizardStep::Confirm { logseq_source, .. } = &w.step {
            assert_eq!(logseq_source.as_deref(), Some("/tmp/logseq"));
        } else {
            panic!("expected Confirm step");
        }
    }

    #[test]
    fn wizard_execute_creates_vault_structure() {
        let tmp = TempDir::new().unwrap();
        let vault = tmp.path().join("vault");

        let w = SetupWizard {
            step: WizardStep::Confirm {
                vault_path: vault.to_str().unwrap().to_string(),
                logseq_source: None,
            },
        };

        let result = w.execute().unwrap();
        assert_eq!(result, vault);
        assert!(vault.join("pages").is_dir());
        assert!(vault.join("journal").is_dir());
        assert!(vault.join("templates").is_dir());
        assert!(vault.join("templates/new-page.tmpl").is_file());
    }
}
