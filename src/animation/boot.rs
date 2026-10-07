//! Vision Boot — Signature Startup Experience model and state machine (Phase 2.2).
//!
//! Models the chronological lifecycle of TerminalVision's signature startup sequence:
//! Wake -> Identity -> System Readiness -> Project Awareness -> Interface Construction -> Vision Pulse -> Ready.
//!
//! Ensures 100% real readiness reporting without artificial delays, fake progress, or blocking sleeps.

use std::path::PathBuf;
use std::time::Duration;

use crate::animation::easing::Easing;
use crate::animation::preferences::{MotionMode, StartupMotionMode};

/// Default duration for Cinematic startup (~1.8 seconds).
pub const CINEMATIC_BOOT_DURATION: Duration = Duration::from_millis(1800);

/// Default duration for Minimal startup (~400 milliseconds).
pub const MINIMAL_BOOT_DURATION: Duration = Duration::from_millis(400);

/// Default duration when Reduced Motion is active (~350 milliseconds).
pub const REDUCED_BOOT_DURATION: Duration = Duration::from_millis(350);

/// The distinct chronological phases of the Vision Boot sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BootPhase {
    /// Phase 1: Minimal central visual signal / expanding pulse (0.00..0.20)
    Wake,
    /// Phase 2: Progressive reveal of TerminalVision identity (0.20..0.40)
    Identity,
    /// Phase 3: Real system readiness checklist (0.40..0.65)
    SystemReadiness,
    /// Phase 4: Project context or workspace summary (0.65..0.85)
    ProjectAwareness,
    /// Phase 5: Interface construction & signature Vision Pulse (0.85..1.00)
    VisionPulse,
    /// Phase 6: Boot sequence complete; normal UI is interactive (1.00)
    Ready,
}

impl BootPhase {
    /// Maps normalized global progress (`0.0..=1.0`) and startup mode to the corresponding [`BootPhase`].
    pub fn from_progress(progress: f32, mode: StartupMotionMode) -> Self {
        let p = progress.clamp(0.0, 1.0);
        if p >= 1.0 {
            return Self::Ready;
        }

        match mode {
            StartupMotionMode::Off => Self::Ready,
            StartupMotionMode::Minimal => {
                if p < 0.45 {
                    Self::SystemReadiness
                } else if p < 0.85 {
                    Self::ProjectAwareness
                } else {
                    Self::VisionPulse
                }
            }
            StartupMotionMode::Cinematic => {
                if p < 0.20 {
                    Self::Wake
                } else if p < 0.40 {
                    Self::Identity
                } else if p < 0.65 {
                    Self::SystemReadiness
                } else if p < 0.85 {
                    Self::ProjectAwareness
                } else {
                    Self::VisionPulse
                }
            }
        }
    }

    /// Computes local phase progress (`0.0..=1.0`) within this phase.
    pub fn local_progress(self, global_progress: f32, mode: StartupMotionMode) -> f32 {
        let p = global_progress.clamp(0.0, 1.0);
        match (mode, self) {
            (StartupMotionMode::Off, _) | (_, Self::Ready) => 1.0,
            (StartupMotionMode::Minimal, Self::SystemReadiness) => (p / 0.45).clamp(0.0, 1.0),
            (StartupMotionMode::Minimal, Self::ProjectAwareness) => {
                ((p - 0.45) / 0.40).clamp(0.0, 1.0)
            }
            (StartupMotionMode::Minimal, Self::VisionPulse) => ((p - 0.85) / 0.15).clamp(0.0, 1.0),
            (StartupMotionMode::Minimal, _) => 1.0,
            (StartupMotionMode::Cinematic, Self::Wake) => (p / 0.20).clamp(0.0, 1.0),
            (StartupMotionMode::Cinematic, Self::Identity) => ((p - 0.20) / 0.20).clamp(0.0, 1.0),
            (StartupMotionMode::Cinematic, Self::SystemReadiness) => {
                ((p - 0.40) / 0.25).clamp(0.0, 1.0)
            }
            (StartupMotionMode::Cinematic, Self::ProjectAwareness) => {
                ((p - 0.65) / 0.20).clamp(0.0, 1.0)
            }
            (StartupMotionMode::Cinematic, Self::VisionPulse) => {
                ((p - 0.85) / 0.15).clamp(0.0, 1.0)
            }
        }
    }
}

/// Returns the duration and easing curve for Vision Boot according to active modes.
pub fn boot_duration_and_easing(
    startup_mode: StartupMotionMode,
    motion_mode: MotionMode,
) -> Option<(Duration, Easing)> {
    if startup_mode == StartupMotionMode::Off || motion_mode == MotionMode::Off {
        return None;
    }

    let duration = match (startup_mode, motion_mode) {
        (StartupMotionMode::Cinematic, MotionMode::Full) => CINEMATIC_BOOT_DURATION,
        (StartupMotionMode::Cinematic, MotionMode::Reduced) => REDUCED_BOOT_DURATION,
        (StartupMotionMode::Minimal, MotionMode::Full) => MINIMAL_BOOT_DURATION,
        (StartupMotionMode::Minimal, MotionMode::Reduced) => {
            REDUCED_BOOT_DURATION.min(MINIMAL_BOOT_DURATION)
        }
        _ => Duration::ZERO,
    };

    if duration.is_zero() {
        return None;
    }

    let easing = match motion_mode {
        MotionMode::Reduced => Easing::Linear,
        _ => Easing::EaseInOut,
    };

    Some((duration, easing))
}

/// Authoritative snapshot of application and system state for Vision Boot presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootState {
    /// Active startup motion mode.
    pub startup_mode: StartupMotionMode,
    /// Active motion preference mode.
    pub motion_mode: MotionMode,
    /// Whether the filesystem directory listing succeeded.
    pub filesystem_ready: bool,
    /// Active working directory path.
    pub filesystem_path: PathBuf,
    /// Number of items loaded in the active directory.
    pub filesystem_entry_count: usize,
    /// Whether the embedded PTY terminal initialized successfully.
    pub terminal_ready: bool,
    /// Whether configuration / settings loaded cleanly.
    pub config_ready: bool,
    /// Detected project name (if any).
    pub project_name: Option<String>,
    /// Detected project type display name (e.g., "Rust", "TypeScript", "Python").
    pub project_type: Option<String>,
    /// Semantic indicators (e.g. `["Cargo", "Rust", "Git"]`).
    pub project_indicators: Vec<String>,
    /// Key project structure elements (e.g. `[("src/", true), ("tests/", true), ("Cargo.toml", true)]`).
    pub project_structure_items: Vec<(String, bool)>,
    /// Detected Git branch name (if any).
    pub git_branch: Option<String>,
    /// Whether the Git working tree has uncommitted modifications.
    pub git_dirty: bool,
    /// Optional non-fatal error notice.
    pub error_notice: Option<String>,
}

impl BootState {
    /// Creates a new `BootState` with explicit parameters.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        startup_mode: StartupMotionMode,
        motion_mode: MotionMode,
        filesystem_ready: bool,
        filesystem_path: PathBuf,
        filesystem_entry_count: usize,
        terminal_ready: bool,
        config_ready: bool,
        project_name: Option<String>,
        project_type: Option<String>,
        project_indicators: Vec<String>,
        project_structure_items: Vec<(String, bool)>,
        git_branch: Option<String>,
        git_dirty: bool,
        error_notice: Option<String>,
    ) -> Self {
        Self {
            startup_mode,
            motion_mode,
            filesystem_ready,
            filesystem_path,
            filesystem_entry_count,
            terminal_ready,
            config_ready,
            project_name,
            project_type,
            project_indicators,
            project_structure_items,
            git_branch,
            git_dirty,
            error_notice,
        }
    }

    /// Whether a project was recognized in the active directory.
    pub fn has_project(&self) -> bool {
        self.project_name.is_some() || self.project_type.is_some()
    }

    /// Returns a short display path (shortening home directory with `~` if applicable).
    pub fn display_path(&self) -> String {
        let path_str = self.filesystem_path.to_string_lossy();
        if let Some(home) = crate::utils::path::home_dir() {
            let home_str = home.to_string_lossy();
            if path_str.starts_with(home_str.as_ref()) {
                return format!("~{}", &path_str[home_str.len()..]);
            }
        }
        path_str.to_string()
    }
}
