//! Project Intelligence Engine for TerminalVision.
//!
//! Provides deterministic, local, fast, safe, and read-only project root discovery,
//! project type identification, signal extraction, and bounded in-memory caching.

pub mod cache;
pub mod detector;
pub mod fingerprint;
pub mod graph;
pub mod signals;
pub mod types;
pub mod workspace;

pub use cache::{ProjectCache, SharedProjectCache};
pub use detector::{ProjectDetector, detect_project, detect_project_root};
pub use fingerprint::ProjectFingerprint;
pub use graph::{WorkspaceAnalyzer, analyze_workspace};
pub use signals::{ProjectSignal, ProjectSignals};
pub use types::{BuildSystem, DetectionConfidence, Language, ProjectType, SignalKind};
pub use workspace::{
    ClassifiedDirectory, DirectoryRole, ImportantFile, ImportantFileRole, ProjectNode,
    WorkspaceContext,
};
