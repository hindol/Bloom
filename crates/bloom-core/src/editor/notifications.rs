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
        self.buffer_mgr.open_read_only(&id, "[messages]", &content);
        self.set_active_page(Some(id));
        self.set_cursor(0);
    }

    pub(crate) fn open_log_buffer(&mut self) {
        let log_path = self
            .vault_root
            .as_ref()
            .map(|r| r.join(".bloom").join("logs").join("bloom.log"));
        if let Some(path) = log_path {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                let content = format_log_lines(&raw);
                let id = crate::uuid::generate_hex_id();
                self.buffer_mgr.open_read_only(&id, "[log]", &content);
                self.set_active_page(Some(id.clone()));
                // Scroll to the last line
                if let Some(buf) = self.buffer_mgr.get(&id) {
                    let len = buf.len_chars();
                    self.set_cursor(len);
                }
            } else {
                self.push_notification(
                    "No log file found".to_string(),
                    render::NotificationLevel::Warning,
                );
            }
        }
    }

    pub(crate) fn open_config_buffer(&mut self) {
        let config_path = self.vault_root.as_ref().map(|r| r.join("config.toml"));
        if let Some(path) = config_path {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let id = crate::uuid::generate_hex_id();
            self.open_page_with_content(&id, "[config]", &path, &content);
        }
    }
}

/// Parse JSON log lines into a human-readable format.
fn format_log_lines(raw: &str) -> String {
    let mut lines = Vec::new();
    for line in raw.lines() {
        lines.push(format_log_line(line));
    }
    lines.join("\n")
}

fn format_log_line(line: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return line.to_string();
    };

    let ts = v["timestamp"]
        .as_str()
        .and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s).ok().map(|dt| {
                dt.with_timezone(&chrono::Local)
                    .format("%H:%M:%S")
                    .to_string()
            })
        })
        .unwrap_or_else(|| "??:??:??".to_string());
    let level = v["level"].as_str().unwrap_or("?");
    let target = v["target"]
        .as_str()
        .unwrap_or("")
        .rsplit("::")
        .next()
        .unwrap_or("");
    let msg = v["fields"]["message"].as_str().unwrap_or("");

    // Collect extra fields (not "message")
    let extras: Vec<String> = v["fields"]
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter(|(k, _)| *k != "message")
                .map(|(k, v)| {
                    let val = if let Some(s) = v.as_str() {
                        s.to_string()
                    } else {
                        v.to_string()
                    };
                    format!("{k}={val}")
                })
                .collect()
        })
        .unwrap_or_default();

    let level_icon = match level {
        "TRACE" => "·",
        "DEBUG" => "⊙",
        "INFO" => "✓",
        "WARN" => "⚠",
        "ERROR" => "✗",
        _ => "?",
    };

    if extras.is_empty() {
        format!("{ts} {level_icon} {target}: {msg}")
    } else {
        format!("{ts} {level_icon} {target}: {msg}  {}", extras.join(" "))
    }
}
