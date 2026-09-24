# TerminalVision Architecture

TerminalVision is a lightweight native terminal file manager written in Rust.

This document defines the module boundaries, responsibilities, and layering principles of TerminalVision after Phases 1–16.1. All modules across the architectural layers are fully implemented, independently tested, and hardened against hostile inputs and failure conditions.

---

## Architectural Layers

Dependencies point strictly in one direction: from the terminal-facing UI layer downwards toward the operating system.

```
UI  ->  Application  ->  Domain / Filesystem  ->  Operating System
```

| Layer | Modules | Responsibility |
| --- | --- | --- |
| **UI** | `ui`, `layout` | Translating prepared application state into Ratatui terminal widgets and computing screen geometry. |
| **Application** | `app`, `input`, `commands`, `preview`, `config` | Managing state transitions, translating events into actions, and coordinating domain services. |
| **Domain / Filesystem** | `filesystem`, `search`, `git` | Handling directory navigation, metadata, entry listing, pure search matching rules, and in-process Git / project detection. |
| **Utilities** | `utils` | Shared pure helper functions (e.g., date formatting, path helpers). |

---

## Detailed Module Responsibilities

| Module | Submodules / Files | Responsibility |
| --- | --- | --- |
| `app` | `state`, `actions`, `modes`, `clipboard` | Holds central application state (`App`), dual file panes, tab state (`PaneTab`), interaction mode state machine (`Mode`), directory bookmarks, clipboard buffers (`ClipboardBuffer`), and status notification messages. |
| `filesystem` | `entry`, `metadata`, `navigation`, `operations` | Standardized directory entries (`DirEntry`), file metadata formatting (`FileMetadata`), directory listing & navigation (`DirectoryWatcher`), non-destructive file operations (`create`, `rename`, `copy`, `move`, `delete`), and non-recursive symlink handling. |
| `input` | `keyboard`, `mouse` | Translates raw terminal Crossterm key presses and mouse clicks/scrolls into high-level semantic [`Action`] events without direct state mutation or filesystem I/O. Handles double-click detection and mouse hit-testing. |
| `layout` | `geometry`, `responsive` | Terminal dimension layout calculations (`ScreenLayout`, `MainLayout`). Dynamically computes single-pane (compact < 80 cols), dual-pane (80+ cols), and 3-column preview layouts (160+ cols). |
| `ui` | `theme`, `header`, `panes`, `file_list`, `preview`, `footer`, `dialogs` | Centralized styling theme (`Theme`, `Symbols`, `Spacing`), render functions mapping pre-computed state onto Ratatui frames (`Frame`). Draws header with project identity, file list entries, tab bars, preview pane, status bar, and modal popups (help, command palette, search, confirm, bookmarks). |
| `search` | `matcher`, `rank`, `mode`, `cancel` | Substring and fuzzy matching algorithms (`Matcher`), rank scoring (`MatchRank`), search mode flags (`SearchMode`), and thread-safe cancellation tokens (`CancelToken`) for background tree walks. |
| `preview` | `language`, `metadata`, `syntax`, `mod.rs` | Read-only inspection of source code and text files: language detection from extension/filename, formatted metadata generation, syntax tokenization, terminal escape sequence sanitization, and 1 MB read boundary safety. |
| `commands` | `palette` | Searchable command palette registry (`Command::ALL`) mapping user-facing descriptions to application actions with substring/fuzzy filtering. |
| `git` | `detector`, `status`, `project` | Pure in-process Git repository discovery (reading `.git/HEAD` and `.git/index`), file modification status parsing (`M`, `?`, `A`), and project type identification (Rust, Node, Python, Go, Git) without subprocess execution. |
| `config` | `settings` | Application configuration discovery and persistent JSON serialization for settings and directory bookmarks (`~/.config/terminalvision` on Unix, `%APPDATA%\terminalvision` on Windows). |
| `utils` | `date`, `path` | Cross-platform path helpers and localized date/time formatting utilities. |

---

## Architectural Rules & Guarantees

1. **Downwards Dependency Flow**: Terminal-facing modules (`main`, `ui`, `layout`, `input`) depend on the application and domain layers. Domain layer modules (`filesystem`, `search`, `git`) have zero dependencies on `ui`, `layout`, Crossterm, or Ratatui.
2. **Input Isolation**: `input` translates raw Crossterm events into high-level semantic [`Action`] enums. It never mutates application state or performs disk I/O directly.
3. **UI Purity**: `ui` never performs filesystem I/O or background search scanning. It consumes pre-computed application state and emits actions.
4. **No Subprocess Invocations**: All operations—including Git status parsing, project marker detection, directory enumeration, and file preview—operate directly on disk files without executing external shell commands (`git`, `sh`, `cp`, `mv`, `rm`).
5. **No Global Mutable State**: Application state is held explicitly in `App` and passed down through clean ownership or mutable references.
6. **Independent Testability**: Domain, search, preview, git, and configuration logic are 100% testable in unit tests without initializing a terminal or TUI backend.
