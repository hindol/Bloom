# Bloom 🌱 — Debuggability

> Structured logging, log rotation, and diagnostic tools. See [ARCHITECTURE.md](ARCHITECTURE.md) for threading model.

---

## Principles

1. **Every error must be visible.** Silent failures (swallowed errors, empty results indistinguishable from success) are bugs. If something fails, the user sees a notification and the log captures the full context.
2. **Logs are structured.** Key-value spans, not printf strings. Enables grep, filtering, and future log viewers.
3. **Logs are always written.** The file subscriber is always active — no flag to enable. Verbosity is configurable, but the file is always there.
4. **Logs are bounded.** Rotation prevents unbounded growth. Old logs are deleted automatically.
5. **Zero overhead when idle.** Tracing's compile-time filtering ensures disabled levels cost nothing.

---

## Log Levels

| Level | What goes here | Examples |
|-------|---------------|----------|
| `error` | Failures that lose data or break functionality | Index open failed, disk write failed, WAL recovery failed |
| `warn` | Degraded behavior the user should know about | Fingerprint mismatch, orphaned link, stale index |
| `info` | Significant lifecycle events | Vault opened, index rebuilt (with timing), session restored, MCP connected |
| `debug` | Detailed operational flow | File event received, buffer opened/closed, picker opened, template expanded |
| `trace` | Hot-path internals (off by default) | Every key event, motion execution, render frame timing, channel send/recv |

Default level: `info` for file output. Status bar notifications surface `error` and `warn` to the user.

---

## Log File Location

```
~/bloom/logs/
├── bloom.log          ← current log (append)
├── bloom.1.log        ← previous rotation
├── bloom.2.log
└── bloom.3.log        ← oldest, deleted on next rotation
```

- **Path:** `{vault_root}/logs/` — colocated with the vault, travels with it.
- **Rotation trigger:** File exceeds 5 MB or on startup.
- **Retention:** 3 rotated files (≤20 MB total). Oldest deleted on rotation.
- **Format:** One JSON object per line (machine-parseable, grep-friendly).

---

## Log Format

Each line is a self-contained JSON object:

```json
{"ts":"2026-03-07T07:10:16.301Z","level":"info","target":"bloom_core::index::indexer","span":"startup_scan","msg":"incremental scan complete","files_scanned":10365,"files_changed":10000,"duration_ms":842}
{"ts":"2026-03-07T07:10:16.302Z","level":"error","target":"bloom_core::index::indexer","msg":"indexer startup error","error":"SqliteFailure(SQLITE_BUSY)","vault":"/home/user/bloom"}
```

Fields:
- `ts` — ISO 8601 timestamp with milliseconds
- `level` — error/warn/info/debug/trace
- `target` — Rust module path (automatic from `tracing`)
- `span` — active span name (optional, from `#[instrument]`)
- `msg` — human-readable message
- Additional key-value pairs from structured fields

---

## Subscriber Configuration

Single `tracing_subscriber` setup in `bloom-tui/src/main.rs` (and `bloom-gui` equivalent):

```rust
use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};
use tracing_appender::rolling;

let log_dir = vault_root.join(".bloom").join("logs");
let file_appender = rolling::Builder::new()
    .rotation(rolling::Rotation::NEVER)     // we rotate manually on size
    .max_log_files(4)
    .filename_prefix("bloom")
    .filename_suffix("log")
    .build(&log_dir)
    .expect("failed to create log appender");

let file_layer = fmt::layer()
    .json()
    .with_writer(file_appender)
    .with_target(true)
    .with_span_events(fmt::format::FmtSpan::CLOSE);

let filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new("info"));

tracing_subscriber::registry()
    .with(filter)
    .with(file_layer)
    .init();
```

Override with `BLOOM_LOG=debug` or `BLOOM_LOG=bloom_core::index=trace` for targeted debugging.

---

## Instrumentation Plan

### Thread Lifecycle (info)

Every long-lived thread logs on start and stop:

```rust
// indexer thread
tracing::info!(vault = %vault_root.display(), "indexer thread started");
// ... on clean exit or error:
tracing::info!("indexer thread stopped");
```

Threads: `bloom-indexer`, `bloom-disk-writer`, `bloom-input`, file watcher (via `notify`).

### Index Operations (info + error)

```rust
// Successful scan
tracing::info!(files_scanned, files_changed, duration_ms, "incremental scan complete");

// Successful rebuild
tracing::info!(pages, links, tags, duration_ms, "full rebuild complete");

// Failure — MUST be error, not silently swallowed
tracing::error!(?err, "indexer failed to open database");
tracing::error!(?err, "incremental scan failed");
```

### File I/O (debug + error)

```rust
// DiskWriter
tracing::debug!(path = %path.display(), size_bytes, "atomic write complete");
tracing::error!(path = %path.display(), ?err, "atomic write failed");

// File watcher events
tracing::debug!(path = %path.display(), kind = "modified", "file event received");
```

### Buffer Operations (debug)

```rust
tracing::debug!(page_id = %id, title, "buffer opened");
tracing::debug!(page_id = %id, "buffer closed");
tracing::debug!(page_id = %id, "buffer marked clean");
tracing::debug!(page_id = %id, "buffer reloaded from disk");
```

### Self-Write Detection (debug)

```rust
tracing::debug!(path = %path.display(), "self-write detected via fingerprint");
tracing::debug!(path = %path.display(), "self-write detected via content match");
tracing::debug!(path = %path.display(), "external change detected, prompting user");
```

### Key Events (trace — off by default)

```rust
tracing::trace!(key = ?key, mode = ?mode, "key event processed");
tracing::trace!(action = ?action, "action dispatched");
```

### Startup Sequence (info)

```rust
tracing::info!(vault = %vault_root.display(), "vault initialized");
tracing::info!(config = %config_path.display(), "config loaded");
tracing::info!(theme = active_theme, "theme applied");
tracing::info!(session_restored = restored, buffers = count, "startup complete");
```

---

## Notification UX

### Rendering

Notifications appear in the **bottom-right corner**, above the status bar. Up to 3 are visible simultaneously, stacked upward (newest at bottom):

```
                                              ┌─────────────────────────────┐
                                              │ ⚠ Fingerprint mismatch      │  ← older, fading
                                              ├─────────────────────────────┤
                                              │ ✓ Index ready — 10365 files │  ← newest
                                              └─────────────────────────────┘
═══════════════════════════════════════════════════════════════════════════════
 NORMAL  notes.md                                                  3:12  utf-8
```

### Severity Behavior

| Level | Icon | Background | Lifetime |
|-------|------|-----------|----------|
| Info | `✓` | `subtle` | Auto-expires after 4 seconds |
| Warning | `⚠` | `accent_yellow` (dimmed) | Auto-expires after 8 seconds |
| Error | `✗` | `critical` (dimmed) | **Persists** until dismissed with `Esc` |

Errors demand attention. Info is ambient. The icon prefix ensures severity is visible even without color (e.g., in a monochrome SSH session).

### Stack & Overflow

- Maximum **3 visible** notifications. If a 4th arrives, the oldest auto-expiring notification is evicted.
- Errors are never auto-evicted — they can only be dismissed manually or replaced by a newer error.
- If all 3 slots are errors, the oldest error is evicted when a new one arrives.

### History

All notifications from the current session are retained in memory (capped at 100). Accessible via `:messages` — opens a read-only buffer listing all notifications with timestamps, newest first:

```
[07:10:16] ✗ Index error: SQLITE_BUSY
[07:10:16] ✓ Vault initialized — ~/bloom
[07:09:58] ✓ Session restored — 3 buffers
```

---

## Error Surfacing

Every `tracing::error!` should also produce a user-visible notification:

```rust
// Pattern: log + notify
tracing::error!(?err, "indexer startup error");
self.notifications.push(Notification {
    message: format!("Index error: {err}"),
    level: NotificationLevel::Error,
    expires_at: None,  // errors persist until dismissed
});
```

Errors that occur on background threads (indexer, disk writer) send the error message through the completion/ack channel so the UI thread can surface it.

---

## Diagnostic Commands

| Command | Description |
|---------|-------------|
| `:log` | Open the current log file in a read-only buffer |
| `:log-level <level>` | Change runtime log level (persists until restart) |
| `:messages` | Show notification history (all notifications from session) |
| `:stats` | Show index/vault statistics (already implemented) |

---

## Configuration

```toml
[logging]
level = "info"           # default log level (overridden by BLOOM_LOG env var)
max_file_size_mb = 5     # rotate when file exceeds this
max_files = 4            # keep this many rotated files
```

---

## Dependencies

| Crate | Purpose |
|-------|---------|
| `tracing` | Structured logging facade (already in all crates) |
| `tracing-subscriber` | Subscriber setup, filtering, formatting (already in bloom-tui) |
| `tracing-appender` | File appender with rotation (new dependency) |

---

## Cross-References

| Document | Section |
|----------|---------|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Communication Pattern — channel-based threading model |
| [CRATE_STRUCTURE.md](CRATE_STRUCTURE.md) | `tracing` instrumentation on state transitions |
| [GOALS.md](GOALS.md) | G22 — Index Rebuild |
