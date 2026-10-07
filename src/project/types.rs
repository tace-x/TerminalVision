//! Strongly typed domain models for Project Intelligence.

/// Supported high-level project types identified by project root marker files and directory layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProjectType {
    Rust,
    Node,
    JavaScript,
    TypeScript,
    Python,
    Java,
    Go,
    C,
    Cpp,
    Php,
    Ruby,
    DotNet,
    GenericGit,
    GenericWorkspace,
    Generic,
}

impl ProjectType {
    /// Human-readable display name for this project type.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Node => "Node.js",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Python => "Python",
            Self::Java => "Java",
            Self::Go => "Go",
            Self::C => "C",
            Self::Cpp => "C/C++",
            Self::Php => "PHP",
            Self::Ruby => "Ruby",
            Self::DotNet => ".NET",
            Self::GenericGit => "Git Repository",
            Self::GenericWorkspace => "Workspace",
            Self::Generic => "Generic",
        }
    }

    /// Alias for display_name.
    pub fn name(self) -> &'static str {
        self.display_name()
    }

    /// Whether this project type represents a compiled language.
    pub fn is_compiled(self) -> bool {
        matches!(
            self,
            Self::Rust | Self::Go | Self::C | Self::Cpp | Self::Java | Self::DotNet
        )
    }

    /// Whether this project type represents an interpreted/scripting language.
    pub fn is_interpreted(self) -> bool {
        matches!(
            self,
            Self::Python | Self::JavaScript | Self::Php | Self::Ruby
        )
    }
}

/// Programming and markup languages recognized within projects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Java,
    Kotlin,
    Go,
    C,
    Cpp,
    Php,
    Ruby,
    CSharp,
    Html,
    Css,
    Shell,
    Generic,
}

impl Language {
    /// Human-readable display name for the language.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Python => "Python",
            Self::Java => "Java",
            Self::Kotlin => "Kotlin",
            Self::Go => "Go",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::Php => "PHP",
            Self::Ruby => "Ruby",
            Self::CSharp => "C#",
            Self::Html => "HTML",
            Self::Css => "CSS",
            Self::Shell => "Shell",
            Self::Generic => "Generic",
        }
    }

    /// Common file extensions associated with this language.
    pub fn file_extensions(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["rs"],
            Self::JavaScript => &["js", "mjs", "cjs", "jsx"],
            Self::TypeScript => &["ts", "mts", "cts", "tsx"],
            Self::Python => &["py", "pyi", "pyw"],
            Self::Java => &["java", "jar"],
            Self::Kotlin => &["kt", "kts"],
            Self::Go => &["go"],
            Self::C => &["c", "h"],
            Self::Cpp => &["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
            Self::Php => &["php", "phtml"],
            Self::Ruby => &["rb", "erb"],
            Self::CSharp => &["cs"],
            Self::Html => &["html", "htm"],
            Self::Css => &["css", "scss", "sass", "less"],
            Self::Shell => &["sh", "bash", "zsh", "fish"],
            Self::Generic => &[],
        }
    }
}

/// Package managers and build orchestration systems recognized within projects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BuildSystem {
    Cargo,
    Npm,
    Yarn,
    Pnpm,
    Bun,
    Pip,
    Poetry,
    Pipenv,
    Maven,
    Gradle,
    GoModules,
    CMake,
    Make,
    Composer,
    Bundler,
    DotNetCli,
    Generic,
}

impl BuildSystem {
    /// Human-readable display name for the build system.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Cargo => "Cargo",
            Self::Npm => "npm",
            Self::Yarn => "Yarn",
            Self::Pnpm => "pnpm",
            Self::Bun => "Bun",
            Self::Pip => "pip",
            Self::Poetry => "Poetry",
            Self::Pipenv => "Pipenv",
            Self::Maven => "Maven",
            Self::Gradle => "Gradle",
            Self::GoModules => "Go Modules",
            Self::CMake => "CMake",
            Self::Make => "Make",
            Self::Composer => "Composer",
            Self::Bundler => "Bundler",
            Self::DotNetCli => ".NET CLI",
            Self::Generic => "Generic",
        }
    }
}

/// Confidence level for project root and type detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum DetectionConfidence {
    /// No confident markers found.
    #[default]
    None,
    /// Weak/loose indicators (e.g. generic README, single source file).
    Low,
    /// Moderate indicators (e.g. Makefile, requirements.txt, or Git repository with source tree).
    Medium,
    /// High-confidence indicators (e.g. unambiguous manifest like `Cargo.toml`, `package.json`, `go.mod`).
    High,
    /// Definitive project root with matching manifest and source hierarchy.
    Definitive,
}

impl DetectionConfidence {
    /// Returns a normalized numeric score in the range `[0.0, 1.0]`.
    pub fn score(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Low => 0.25,
            Self::Medium => 0.55,
            Self::High => 0.85,
            Self::Definitive => 1.0,
        }
    }
}

/// Categories of detected project signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalKind {
    Manifest,
    SourceDir,
    TestDir,
    DocFile,
    DocDir,
    ConfigFile,
    ConfigDir,
    CiCd,
    Container,
    BuildArtifact,
    GitRepo,
    GitWorktree,
}
