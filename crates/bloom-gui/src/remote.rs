//! Remote session detection and rendering hints.
//!
//! Detects RDP, SSH, and other remote sessions to adapt rendering:
//! - Disable cursor animation (lerp is wasted over high-latency links)
//! - Lower tick rate (no 120Hz animation subscription)
//! - Skip scroll bar cursor tick (minor detail, not worth the repaint)
//! - Use opaque scrim instead of alpha-blended (compositing can be slow)

/// Rendering adjustments for remote sessions.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RemoteHints {
    /// True when running over RDP, SSH, or another remote protocol.
    pub is_remote: bool,
}

impl RemoteHints {
    /// Probe the environment once at startup.
    pub fn detect() -> Self {
        Self {
            is_remote: is_remote_session(),
        }
    }

    /// Whether cursor animation should be disabled (instant jumps).
    pub fn skip_animation(&self) -> bool {
        self.is_remote
    }

    /// Whether the overlay scrim should be fully opaque (no alpha blending).
    #[allow(dead_code)]
    pub fn opaque_scrim(&self) -> bool {
        self.is_remote
    }

    /// Whether the scroll-bar cursor tick should be hidden.
    #[allow(dead_code)]
    pub fn skip_scroll_tick(&self) -> bool {
        self.is_remote
    }
}

/// Detect whether the current session is remote (RDP, SSH, etc.).
fn is_remote_session() -> bool {
    // Windows: GetSystemMetrics(SM_REMOTESESSION)
    #[cfg(target_os = "windows")]
    {
        const SM_REMOTESESSION: i32 = 0x1000;

        #[link(name = "user32")]
        extern "system" {
            fn GetSystemMetrics(index: i32) -> i32;
        }

        if unsafe { GetSystemMetrics(SM_REMOTESESSION) } != 0 {
            tracing::info!("remote session detected (RDP)");
            return true;
        }
    }

    // SSH_CLIENT / SSH_TTY — works on all platforms.
    if std::env::var_os("SSH_CLIENT").is_some() || std::env::var_os("SSH_TTY").is_some() {
        tracing::info!("remote session detected (SSH)");
        return true;
    }

    false
}
