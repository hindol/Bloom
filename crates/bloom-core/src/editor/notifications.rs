//! Notification stack and history.
//!
//! Info notifications auto-expire after 4 s, warnings after 8 s, and errors
//! persist until dismissed. At most 3 notifications are visible at once; older
//! ones are pushed into a rolling 100-entry history for the stats display.

use std::time::Instant;

use crate::*;

impl BloomEditor {
    /// Push a notification, keeping max 3 visible and recording in history.
    pub(crate) fn push_notification(&mut self, message: String, level: render::NotificationLevel) {
        let expires_at = match level {
            render::NotificationLevel::Info => {
                Some(Instant::now() + std::time::Duration::from_secs(4))
            }
            render::NotificationLevel::Warning => {
                Some(Instant::now() + std::time::Duration::from_secs(8))
            }
            render::NotificationLevel::Error => None,
        };
        let notif = render::Notification {
            message,
            level,
            expires_at,
            created_at: Instant::now(),
        };
        self.notification_history.push(notif.clone());
        if self.notification_history.len() > 100 {
            self.notification_history.remove(0);
        }
        // Evict oldest auto-expiring notification if at capacity
        if self.notifications.len() >= 3 {
            if let Some(pos) = self
                .notifications
                .iter()
                .position(|n| n.expires_at.is_some())
            {
                self.notifications.remove(pos);
            } else {
                self.notifications.remove(0);
            }
        }
        self.notifications.push(notif);
    }

    /// Dismiss all persistent (error) notifications. Called on Esc.
    pub fn dismiss_notifications(&mut self) {
        self.notifications.retain(|n| n.expires_at.is_some());
    }

    /// Show vault and index stats as a notification.
    pub(crate) fn show_stats(&mut self) {
        let mut parts: Vec<String> = Vec::new();

        // Page/journal counts from index
        if let Some(idx) = &self.index {
            let pages = idx.list_pages(None).len();
            let tags = idx.all_tags().len();
            let tasks = idx.all_open_tasks().len();
            parts.push(format!(
                "{pages} pages  │  {tags} tags  │  {tasks} open tasks"
            ));
        }

        // Last index timing
        if let Some(t) = &self.last_index_timing {
            parts.push(format!(
                "Index: {}ms total  │  scan {}ms  │  read {}ms  │  write {}ms  │  {} scanned, {} changed",
                t.total_ms, t.scan_ms, t.read_parse_ms, t.write_ms, t.files_scanned, t.files_changed,
            ));
        } else if self.indexing {
            parts.push("Indexing in progress…".to_string());
        } else {
            parts.push("No index stats yet".to_string());
        }

        // Open buffers
        let buf_count = self.buffer_mgr.open_buffers().len();
        parts.push(format!("{buf_count} open buffers"));

        let message = parts.join("  ·  ");
        self.push_notification(message, render::NotificationLevel::Info);
    }

    pub(crate) fn open_messages_buffer(&mut self) {
        let mut lines = Vec::new();
        for n in self.notification_history.iter().rev() {
            let icon = match n.level {
                render::NotificationLevel::Info => "\u{2713}",
                render::NotificationLevel::Warning => "\u{26a0}",
                render::NotificationLevel::Error => "\u{2717}",
            };
            let elapsed = n.created_at.elapsed();
            let secs = elapsed.as_secs();
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            let s = secs % 60;
            lines.push(format!("[{h:02}:{m:02}:{s:02}] {icon} {}", n.message));
        }
        if lines.is_empty() {
            lines.push("No notifications yet.".to_string());
        }
        let content = lines.join("\n");
        let id = crate::uuid::generate_hex_id();
        self.open_page_with_content(
            &id,
            "[messages]",
            std::path::Path::new("[messages]"),
            &content,
        );
    }

    pub(crate) fn open_log_buffer(&mut self) {
        let log_path = self
            .vault_root
            .as_ref()
            .map(|r| r.join(".bloom").join("logs").join("bloom.log"));
        if let Some(path) = log_path {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let id = crate::uuid::generate_hex_id();
                self.open_page_with_content(&id, "[log]", std::path::Path::new("[log]"), &content);
            } else {
                self.push_notification(
                    "No log file found".to_string(),
                    render::NotificationLevel::Warning,
                );
            }
        }
    }
}
