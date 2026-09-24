pub mod detector;
pub mod project;
pub mod status;

pub use detector::{GitRepository, detect_repository};
pub use project::{
    ProjectInfo, ProjectType, detect_project, detect_project_for_path, detect_project_root,
};
pub use status::{FileStatus, GitBranch, GitStatus, compute_status};
