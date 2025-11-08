//! Platform detection for cross-platform dotfile management
//!
//! Provides OS and architecture information using standard Unix conventions:
//! - macOS → `"darwin"` (kernel name)
//! - Linux → `"linux"`
//! - Windows → `"windows"`
//!
//! Platform info is cached on first access for optimal performance.

use std::sync::LazyLock;

/// Current platform information (cached)
///
/// # Example
/// ```
/// use guisu_core::platform::CURRENT_PLATFORM;
///
/// let platform_dir = format!(".guisu/{}/variables", CURRENT_PLATFORM.os);
/// ```
pub static CURRENT_PLATFORM: LazyLock<Platform> = LazyLock::new(Platform::detect);

/// Platform information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    /// OS: "darwin" (macOS), "linux", "windows", "unknown"
    pub os: &'static str,
    /// CPU architecture: "x86_64", "aarch64", etc.
    pub arch: &'static str,
}

impl Platform {
    pub fn detect() -> Self {
        Self {
            os: Self::detect_os(),
            arch: std::env::consts::ARCH,
        }
    }

    const fn detect_os() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            "darwin"
        }

        #[cfg(target_os = "linux")]
        {
            "linux"
        }

        #[cfg(target_os = "windows")]
        {
            "windows"
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            "unknown"
        }
    }
}
