# TerminalVision Architecture

TerminalVision is a lightweight native terminal file manager written in Rust.

This document defines the module boundaries, responsibilities, and layering principles of TerminalVision, including the **Universal Action + Cross-Platform Shortcut System** (Signature Update Phase 1.1).

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
| `app` | `state`, `actions`, `modes` | Holds central application state (`App`), dual file panes, tab state (`PaneTab`), interaction mode state machine (`Mode`), centralized Action Registry (`ActionRegistry`, `ActionMetadata`, `ActionContext`), directory bookmarks, clipboard buffers (`ClipboardBuffer`), and status notification messages. |
| `filesystem` | `entry`, `metadata`, `navigation`, `operations` | Standardized directory entries (`DirEntry`), file metadata formatting (`FileMetadata`), directory listing & navigation (`DirectoryWatcher`), non-destructive file operations (`create`, `rename`, `copy`, `move`, `delete`), and non-recursive symlink handling. |
| `input` | `keyboard`, `mouse`, `shortcut`, `platform` | Centralized Shortcut Registry (`ShortcutRegistry`, `KeyChord`, `Modifiers`, `ShortcutBinding`), platform detection (`Platform`), and translation of raw Crossterm key and mouse events into high-level semantic [`Action`] events without direct state mutation or filesystem I/O. |
| `layout` | `geometry`, `responsive` | Terminal dimension layout calculations (`ScreenLayout`, `MainLayout`). Dynamically computes single-pane (compact < 80 cols), dual-pane (80+ cols), and 3-column preview layouts (160+ cols). |
| `ui` | `theme`, `header`, `panes`, `file_list`, `preview`, `footer`, `dialogs` | Centralized styling theme (`Theme`, `Symbols`, `Spacing`), render functions mapping pre-computed state onto Ratatui frames (`Frame`). Draws header with project identity, file list entries, tab bars, preview pane, status bar, and modal popups (help, command palette, search, confirm, bookmarks). |
| `search` | `matcher`, `rank`, `mode`, `cancel` | Substring and fuzzy matching algorithms (`Matcher`), rank scoring (`MatchRank`), search mode flags (`SearchMode`), and thread-safe cancellation tokens (`CancelToken`) for background tree walks. |
| `preview` | `language`, `metadata`, `syntax`, `mod.rs` | Read-only inspection of source code and text files: language detection from extension/filename, formatted metadata generation, syntax tokenization, terminal escape sequence sanitization, and 1 MB read boundary safety. |
| `commands` | `palette` | Searchable command palette registry (`Command`) dynamically derived from `ActionRegistry` and `ShortcutRegistry` with substring/fuzzy filtering. |
| `git` | `detector`, `status`, `project` | Pure in-process Git repository discovery (reading `.git/HEAD` and `.git/index`), file modification status parsing (`M`, `?`, `A`), and project type identification (Rust, Node, Python, Go, Git) without subprocess execution. |
| `config` | `settings` | Application configuration discovery and persistent JSON serialization for settings and directory bookmarks (`~/.config/terminalvision` on Unix, `%APPDATA%\terminalvision` on Windows). |
| `utils` | `date`, `path` | Cross-platform path helpers and localized date/time formatting utilities. |

---

## Universal Action + Shortcut Architecture

### 1. Action Registry (`src/app/actions.rs`)
Every user capability is modeled as a discrete, semantic [`Action`]. An action represents user intent (e.g. `Copy`, `NewDirectory`, `RefreshDirectory`) independently of how it was triggered.

Each action possesses metadata:
- **`Action`**: Unique enum discriminant
- **`name`**: User-facing display title
- **`description`**: Human-readable explanation of the action
- **`category`**: Grouping (`Navigation`, `Files`, `Tabs`, `View`, `Preview`, `Search`, `Bookmarks`, `Git`, `Project`, `Terminal`, `Application`)
- **`context`**: Default [`ActionContext`] in which the action is valid

All action executions funnel through a single point: `app.handle_action(action)`. Whether an action is triggered by a keyboard shortcut, mouse click, command palette, or future context menu/action bar, the exact same handler executes.

### 2. Shortcut Registry & Cross-Platform Modifiers (`src/input/shortcut.rs`, `src/input/platform.rs`)
Shortcuts are registered in a centralized `ShortcutRegistry`. The registry abstracts platform-specific modifier conventions:
- **macOS**: Command (`⌘` / `KeyModifiers::SUPER`) is the primary modifier for application shortcuts (e.g., `⌘C`, `⌘X`, `⌘V`, `⌘A`, `⌘F`, `⌘R`, `⌘K`, `⌘Shift+N`, `⌘L`).
- **Windows & Linux**: Control (`Ctrl` / `KeyModifiers::CONTROL`) is the primary modifier (e.g., `Ctrl+C`, `Ctrl+X`, `Ctrl+V`, `Ctrl+A`, `Ctrl+F`, `Ctrl+R`, `Ctrl+K`, `Ctrl+Shift+N`, `Ctrl+L`).

### 3. Focus & Context Model
The active [`Mode`] of the application dictates the current [`ActionContext`]:
- `FileManager`: Dual-pane file list navigation and operations.
- `Terminal`: Interactive PTY shell session. All terminal input (Ctrl+C SIGINT, Ctrl+D EOF, arrow keys, shell commands) passes directly to the PTY shell without application interception, except for global terminal focus toggles (`Ctrl+T` / `F12`).
- `Search`: Live query input for file filtering.
- `Dialog`: Modal text input (Create, Rename, Jump to Path) or confirmation prompts.
- `CommandPalette`: Searchable command list.
- `Preview` & `Help`: Read-only viewers.
- `Global`: Cross-cutting application controls.

### 4. Generated Shortcut Help
The help overlay (`?`) and command palette derive their shortcut labels dynamically from the `ShortcutRegistry` for the active `Platform`. There are no manual or duplicated shortcut strings in documentation or UI widgets.

### 5. Conflict Detection
`ShortcutRegistry::detect_conflicts()` audits the entire shortcut binding table across all supported platforms (macOS, Windows, Linux) to ensure no two distinct actions claim the same key chord in overlapping contexts.

### 6. Adding New Actions & Shortcuts in Future Phases
To introduce a new action (e.g., in future signature phases such as Quick Switcher, Themes, or Storage Vision):
1. Add the enum variant to `Action` in `src/app/actions.rs`.
2. Register its metadata in `ActionRegistry::build_default()`.
3. Register its primary/secondary shortcut in `ShortcutRegistry::build_default()` using `KeyChord::primary(...)` or `KeyChord::plain(...)`.
4. Handle the action execution in `App::handle_action()` in `src/app/state.rs`.
5. The command palette, help screen, conflict detection, and keyboard routing automatically adapt with zero additional boilerplate.

---

---

## Unified Terminal ↔ File Manager State (Signature Update Phase 1.2)

### 1. Single Active Location Model (`ActiveLocation`, `SyncOrigin`)
The application defines a single, coherent concept of active filesystem location across both views:
- **`ActiveLocation`**: Encapsulates `path`, `active_pane`, `last_sync_origin`, and `synchronized` status.
- **Rule**: `File Manager location == Terminal shell cwd == Active synchronized location`.
- **`SyncOrigin`**: Tracks the origin of synchronization events (`Startup`, `FileManagerNavigation`, `ShellCwdChange`, `ActivePaneSwitch`, `FilesystemMutation`, `InternalRefresh`) to ensure precise propagation and prevent circular loops.

### 2. Startup Synchronization
1. `terminalvision`: File manager starts in current process working directory (`cwd`); embedded terminal shell spawns with that exact real `cwd`.
2. `terminalvision <directory>`: Active pane and embedded terminal start in `<directory>`.
3. `terminalvision <file>`: Active pane opens parent directory with `<file>` selected and previewed; terminal spawns in parent directory.

### 3. File Manager → Terminal Synchronization
When the active File Manager pane navigates (via `Enter` on directory, `Backspace` / `GoParent`, `GoBack`, `GoForward`, tab switches, smart jump, bookmarks):
- `App` sends ` cd "<escaped_path>"\n` to the PTY shell via `term.cd_to_path()`.
- The real shell process genuinely changes its directory. `pwd` in the terminal returns the actual current path without interception or output rewriting.

### 4. Terminal → File Manager Synchronization & Shell CWD Detection
When the user executes directory changes in the terminal shell (`cd ..`, `cd /`, `cd ~`, `cd -`, `cd <path>` with spaces/Unicode, aliases, functions):
- **macOS Detection**: Queries kernel process CWD via `proc_pidinfo(PROC_PIDVNODEPATHINFO)`.
- **Linux Detection**: Queries `/proc/<pid>/cwd` symlink.
- **OSC 7 Shell Integration**: The ANSI terminal emulator parses OSC 7 sequences (`\x1b]7;file://...\x07` or `\x1b\\`), strips them from visible screen cells, decodes URL-encoded paths, and updates `tracked_cwd`.
- When a verified shell CWD change is detected, `App::handle_shell_cwd_change()` navigates the active pane to the new directory, preserving selection and updating preview.
- If an invalid path is entered (e.g. `cd /does/not/exist`), the shell errors naturally; the File Manager rejects the non-existent path and remains at its valid location.

### 5. Filesystem Watcher & Event Loop Integration (`FilesystemWatcher`)
- Tracks watched directories (active and inactive panes), previewed files, and Git repositories via lightweight timestamp (`mtime`), entry count, and file length fingerprints.
- Integrated into the main event loop via non-blocking `poll_sync_and_filesystem()`.
- When files are created, modified, renamed, or deleted from the terminal (or external tools):
  - Affected directory listings are refreshed while preserving user selection and scroll position.
  - Preview file modifications immediately invalidate and reload the preview pane.
  - Git repository updates (`git commit`, `git checkout`, `git add`) trigger Git status refresh.
- If the current directory is deleted on disk, `safe_fallback_directory()` walks up to an existing parent or home directory without crashing.

### 6. Active Pane Rule
- The embedded terminal always follows the **currently active** File Manager pane.
- Switching active pane (via `Tab` or mouse click) triggers `SyncOrigin::ActivePaneSwitch`, updating the terminal shell cwd to the newly focused pane's directory while preserving terminal history and process state.

### 7. Loop Prevention
- Synchronizations are guarded by `paths_are_equivalent()` and `last_synced_terminal_cwd`.
- If a shell CWD change originated from File Manager navigation, redundant shell `cd` commands are suppressed.
- If File Manager updates from a shell `cd`, reverse shell synchronization is skipped.

### 8. Platform Limitations & Fallbacks
- **Symlink Aliases**: On macOS, `/var` is a symlink to `/private/var`, and `/tmp` is `/private/tmp`. `paths_are_equivalent()` canonicalizes paths when comparing kernel-reported vnode paths against user-entered paths.
- **PTY Process Isolation**: On non-Unix platforms where `/proc` or `proc_pidinfo` is unavailable, OSC 7 shell integration provides seamless CWD tracking.

---

## Architectural Rules & Guarantees

1. **Downwards Dependency Flow**: Terminal-facing modules (`main`, `ui`, `layout`, `input`) depend on the application and domain layers. Domain layer modules (`filesystem`, `search`, `git`) have zero dependencies on `ui`, `layout`, Crossterm, or Ratatui.
2. **Input Isolation**: `input` translates raw Crossterm events into high-level semantic [`Action`] enums using the `ShortcutRegistry`. It never mutates application state or performs disk I/O directly.
3. **UI Purity**: `ui` never performs filesystem I/O or background search scanning. It consumes pre-computed application state and emits actions.
4. **PTY Terminal Integrity**: In `Terminal` mode, the real interactive terminal is never intercepted or faked. Shell commands, control sequences, and signals flow unimpeded.
5. **No Subprocess Invocations for FM Operations**: All file manager operations—including Git status parsing, project marker detection, directory enumeration, and file preview—operate directly on disk files without executing external shell commands (`git`, `sh`, `cp`, `mv`, `rm`).
6. **No Global Mutable State**: Application state is held explicitly in `App` and passed down through clean ownership or mutable references.
7. **Single Action Implementation**: Every action has one ID, one definition in `ActionRegistry`, and one handler in `App::handle_action`.
8. **Real Filesystem Synchronization**: File Manager and Terminal shell represent the identical physical filesystem location without faking `pwd` or rewriting shell output.
9. **Selection & Action Uniformity**: File selection and contextual actions are driven by stable path identities and centralized action dispatch; mouse, keyboard, and context menus execute identical action handlers.

---

## Smart Selection + Context Menu + Adaptive Action UI (Signature Update Phase 1.3)

### 1. Selection State Model (`Tab`, `Pane`)
The selection model supports single, multi, and contiguous range selections on a per-tab / per-pane basis:
- **Single Selection**: `selected_index: usize` tracks the active cursor position in the directory list.
- **Multi-Selection**: `selected_paths: HashSet<PathBuf>` stores discrete, arbitrarily selected file paths.
- **Range Selection**: `selection_anchor: Option<usize>` records the pivot index when starting a range selection (`Shift+Up/Down` or `Shift+Click`). Range selection computes a contiguous slice `[min(anchor, target)..=max(anchor, target)]` into `selected_paths`.
- **Toggling**: `Cmd+Click` (macOS) / `Ctrl+Click` (Windows/Linux) toggles individual item membership in `selected_paths`.
- **Select All**: Primary Modifier + A (`⌘A` / `Ctrl+A`) populates `selected_paths` with all visible items.
- **Deselect**: `Esc` or clicking empty directory space clears multi-selection (`deselect_all()`).

### 2. Stable Selection Identity & Resiliency
To prevent invalid index bugs, stale pointers, and out-of-bounds panics:
- Selection state uses **stable path identity** (`PathBuf`) rather than volatile numeric indices.
- **Rename Operations**: Renaming `old.txt` to `new.txt` automatically updates `selected_paths` and repositions `selected_index` directly onto `new.txt`.
- **Delete Operations**: Deleting items removes them from `selected_paths` and clamps `selected_index` safely within the new bounds of the directory.
- **Sorting & Searching**: When directory sorting changes or search filter queries update, selections are preserved by matching path identities against the newly ordered entries.
- **Filesystem Watcher Events**: Background file creations/deletions from the terminal shell revalidate `selected_paths` against current disk entries and clamp `selected_index`.

### 3. Contextual Actions & Action Registry Integration
All UI actions funnel strictly through the centralized [`ActionRegistry`]:
- **Bulk Operations**: When multiple items are selected, `Copy`, `Cut`, `Move`, and `Delete` operate on all selected paths simultaneously. Deletion prompts display safe scope confirmation (e.g., `"Delete 5 selected items?"`).
- **Native Filesystem Operations**: Copy, move, and delete actions use native Rust standard library APIs (`std::fs`), never spawning external shell commands (`cp`, `mv`, `rm`).
- **Context Awareness**: Actions dynamically check whether 0 items, 1 file, 1 folder, or multiple items are selected to enable/disable contextual options.

### 4. Context Menu Architecture (`ContextMenuState`, `ContextMenuItem`)
A mouse- and keyboard-accessible context popup menu (`Mode::ContextMenu`):
- **Activation**: Right-click anywhere in the file manager or keyboard shortcut `Shift+F10` (`Action::ContextMenu`).
- **Dynamic Building**: Context items are constructed based on selection type (File, Folder, Multi-selection, or Empty Space). Secondary commands are placed in nested `More ›` submenus.
- **Viewport Clamping**: `calculate_context_menu_rect` and `calculate_context_submenu_rect` dynamically clamp menu bounds to the active terminal viewport dimensions, preventing clipping or overflow near right/bottom screen edges.
- **Navigation**: Supports `Up`/`Down` arrow navigation, mouse hover highlighting, `Enter` / click to execute, and `Esc` / click outside to dismiss.

### 5. Adaptive Action Area & Minimal UI
- Embedded in the application footer (`src/ui/footer.rs`), exposing minimal, relevant quick actions for the active selection.
- **Responsive Width Collapsing**: Dynamically scales based on available terminal width. Wide terminals show full action sets (e.g., `Open Copy Move Rename Delete More`), while narrow terminals collapse into minimal sets (e.g., `Open More`) where `More` opens the contextual action menu.
- **Aggregate Status Indicator**: Displays selection counts and aggregate sizes when items are selected (e.g., `[3 sel • 24.8 MB]`) without blocking UI renders.

### 6. Mouse Hit Testing Architecture
- Uses actual rendered layout rectangles (`Rect`) computed during the render pass rather than hardcoded coordinates.
- Handles row clicks, range selection (`Shift+Click`), multi-selection toggles (`⌘/Ctrl+Click`), empty space deselection, footer action button clicks, and context menu item/submenu selection.

---

## Command Center + Quick Switcher Architecture (Signature Update Phase 2.1)

### 1. Command Center ("What can I do?")
- **Primary Shortcut**: `⌘K` (macOS) / `Ctrl+K` (Windows/Linux) via the centralized `ShortcutRegistry`.
- **UI Discoverability**: A subtle search/action entry point `[ ⌕ Search / Actions ⌘K ]` in the application header bar (`src/ui/header.rs`). Mouse click directly activates the Command Center.
- **Data Source & Action Dispatch**: Searches all actions from the centralized `ActionRegistry` (`src/app/actions.rs`). Selecting an action dispatches `app.handle_action(action)` without duplicating application logic.
- **Context Filtering**: Dynamically filters/prioritizes actions based on application context (`ContextFilter`):
  - File selected: Prioritizes `Open`, `Preview`, `Copy`, `Rename`, `Delete`, `Get Info`.
  - Folder selected: Prioritizes `Open`, `Copy`, `Cut`, `Rename`, `Delete`, `Get Info`.
  - Empty directory: Prioritizes `New File`, `New Directory`, `Search`, `Refresh`.
  - Clipboard populated: Elevates `Paste`.
- **Accessible File Search**: Automatically includes entries from the active directory pane and recent file history for immediate file navigation directly from the Command Center.

### 2. Quick Switcher ("Where do I want to go?")
- **Primary Shortcut**: `⌘P` (macOS) / `Ctrl+P` (Windows/Linux), secondary `Shift+J` (`Action::SmartJump`).
- **Purpose**: Fast jump navigation to locations and files across the workspace.
- **Target Aggregation**:
  - Recent folders (`RecentLocations`)
  - Recent files (`RecentLocations`)
  - Open tabs across left and right panes
  - Saved bookmarks
  - Git repository root
  - Project root
  - User home directory (`~`) and filesystem root (`/`)
- **Navigation Behavior**:
  - Selecting a folder: Navigates active pane directly to that directory.
  - Selecting a file: Navigates active pane to the file's parent directory and selects the file in the pane without external execution.

### 3. Lightweight Fuzzy Matching (`src/commands/fuzzy.rs`)
- Zero external dependencies.
- Evaluates exact matches (`score 10000`), prefix matches (`score 5000`), substring boundaries (`score 3500`), and acronym/subsequence matches with consecutive character bonuses, word boundary rewards, and span compactness bonuses.
- Evaluates suffixes starting at each word separator boundary to prevent sub-optimal greedy matching traps on multi-word strings.
- Action display names receive priority weighting over loose description matches.

### 4. Recent Navigation History & Persistence (`RecentLocations`, `Settings`)
- Lightweight tracking of meaningful navigation events (directory changes and file selections) with deduplication and bounded capacity (50 entries).
- Automatically cleans up and ignores stale/deleted paths on activation without crashing or generating invalid navigation states.
- Persisted to application configuration under `[history]` (`recent_locations` and `recent_files`).

### 5. Responsive Minimal UI & Terminal Protection
- Rendered using Ratatui overlays with double borders, search query inputs, category badges, and keyboard shortcuts.
- Fully responsive across 80×24, 100×30, 120×30, 160×40, 180×40, and 200×60 terminal viewports without negative Rect geometry.
- Full mouse support: item click selection, scrolling up/down, hover highlighting, and click outside dialog to dismiss.
- Complete terminal isolation: terminal keyboard input flows directly to the PTY shell when terminal is focused.

---

## File Intelligence, Universal Quick Preview & Smart Operations (Signature Update Phase 2.2)

### 1. File Intelligence Layer (`src/filesystem/classification.rs`)
- **Pure Local Execution**: 100% offline classification without external APIs, background uploaders, or cloud telemetry.
- **13 Standardized Categories**: `Directory`, `Image`, `Video`, `Audio`, `Text`, `SourceCode`, `Document`, `Pdf`, `Archive`, `Executable`, `Config`, `Data`, `Unknown`.
- **Classification Engine**: Fast extension mapping with magic byte validation (PNG, JPEG, GIF, BMP, WEBP, TIFF, PDF, ZIP, TAR, GZ, BZ2, XZ, 7Z, ELF/Mach-O/PE executables, script shebangs).
- **Metadata Extraction (`FileIntelligence`)**: Extracts byte size, modification timestamp, creation timestamp (where supported), POSIX octal/symbolic permissions, executable bit, symlink status, and MIME hints asynchronously/lazily.

### 2. Universal Quick Preview (`src/preview/`)
- **Primary Shortcut**: `Space` to open Quick Preview modal (`Mode::Preview`), `Esc` or `Space` to close. Never steals terminal focus or corrupts pane selection state.
- **Preview Types**:
  1. **Text / Source Code (`src/preview/syntax.rs`)**: High-performance line tokenization with keyword highlighting, line numbering, UTF-8 safety checks, and strict 1 MB bounded read buffers.
  2. **Image Preview (`src/preview/image.rs`)**: Pure in-memory header decoder for PNG, JPEG, GIF, BMP, WEBP, and TIFF formats (dimensions, color channels, bit depth). Detects terminal graphics support (`Kitty`, `iTerm2`, `Sixel`) with graceful metadata card fallback.
  3. **PDF Preview (`src/preview/pdf.rs`)**: Safe, bounded (128 KiB) metadata scanner extracting specification version, page counts, title, author, creator, producer, and text snippets without heavy rendering libraries.
  4. **Archive Preview (`src/preview/archive.rs`)**: In-memory directory inspector enumerating entries and sizes for `.zip`, `.tar`, `.tar.gz`, `.tgz`, `.tar.bz2`, `.tar.xz`, and `.7z` without disk extraction.
  5. **Directory Preview (`src/preview/directory.rs`)**: Immediate non-recursive statistics (direct child counts, file count, subfolder count, symlink count, aggregated byte size, and Git repository context).
  6. **Unknown / Binary Files (`src/preview/metadata.rs`)**: Robust fallback card displaying paths, categories, timestamps, permissions, and byte sizes without error states.
- **Performance & Stale Invalidation**: Bounded chunk reads prevent freezes. Rapid selection changes (`A → B → C → D`) immediately invalidate and replace background preview loading.

### 3. Open vs. Quick Preview
- **Quick Preview (`Space`)**: Non-destructive in-application modal viewer.
- **Open (`Enter`)**:
  - Folders / Enterable Symlinks: Navigates inside the directory within the active file pane.
  - Regular Files: Executes native OS process APIs (`open` on macOS, `xdg-open` on Linux, `start` / native shell execution on Windows) without blocking the TUI event loop or launching arbitrary shell commands.

### 4. Smart File Operations & Operation Center (`src/operations/`)
- **Operation Manager (`src/operations/manager.rs`)**: Centralized coordinator executing chunked (64 KiB) file transfers with real-time throughput metrics.
- **Progress Tracking (`src/operations/progress.rs`)**: Computes exact percentage, completed/total bytes, transfer speeds (KB/s, MB/s), ETA countdowns, and rendered progress bars.
- **Cooperative Cancellation & Safe Pause**: Thread-safe `Arc<AtomicBool>` flags enable responsive cancellation (with automatic cleanup of partial destination files) and safe pause/resume.
- **Interactive Conflict Resolution (`src/operations/conflict.rs`)**: Interactive modal dialog on destination file collisions offering `Replace`, `Skip`, `Rename` (auto-generating unique `(1)`, `(2)` suffixes), `Replace All`, `Skip All`, and `Cancel`.
- **Error Recovery (`src/operations/manager.rs`)**: Interactive error dialog on I/O failures providing `Retry`, `Skip`, and `Cancel` options without application crashes.
- **Operation History (`src/operations/history.rs`)**: Bounded in-memory ring buffer (30 records) logging recent operation summaries, timestamps, and outcomes.
- **Selection Preservation**: Reconciles file pane selections after rename and delete operations without resetting the entire directory view.

---

## Smart Breadcrumb, Favorites / Quick Access & Storage Vision (Signature Update Phase 2.3)

### 1. Smart Breadcrumb & Path Navigator (`src/navigation/breadcrumb.rs`)
- **Top Header Placement**: Persistent in the top application header bar, strictly isolated from the embedded bottom terminal.
- **Hierarchical Path Deconstruction**: Breaks absolute paths down into navigable segments (`Home / Projects / TerminalVision / src / app`), highlighting the active directory with accent styling.
- **Responsive Width Formatting**: Adapts dynamically from spaced hierarchies on wide screens (`Home / Projects / ...`) to compact representations on narrow screens (`~/Projects/...`).
- **Clickable Hit-Testing (`breadcrumb_hit_segment`)**: Computes exact cell coordinate boundaries for each segment; mouse clicks immediately navigate the active file manager pane directly to the target segment without launching duplicate navigation engines.
- **Terminal ↔ Breadcrumb Sync**: Seamlessly updates when shell processes change working directory or when file manager navigates.

### 2. Favorites / Quick Access System (`src/navigation/favorites.rs`)
- **User-Controlled Pinning**: Allows users to pin, rename, reorder (`move_up`, `move_down`), remove, and quickly access frequently visited directories (`★ Projects`, `★ Downloads`).
- **Disk Validation & Broken Target Detection**: Validates favorite paths on display and access. Broken or deleted target paths display a subtle warning badge (`⚠ OldProject`) without crashing the application.
- **Persistence**: Saved across restarts using standard application settings (`Settings::bookmarks`).
- **Command Center & Quick Switcher Integration**: Exposed through Command Center (`⌘K`), Quick Switcher (`⌘P`), and Action Registry (`Action::ToggleFavorite`, `Action::MoveFavoriteUp`, `Action::MoveFavoriteDown`).

### 3. Storage Vision & Directory Heatmap (`src/storage/`)
- **On-Demand Workspace (`Mode::StorageVision`)**: On-demand modal view invoked via Command Center or `Action::StorageVision` analyzing the active scope directory.
- **Asynchronous Non-Blocking Scanner (`src/storage/scanner.rs`)**: Runs recursive directory analysis on a dedicated background worker thread with atomic progress counters (`items_scanned`, `bytes_scanned`) and live UI updates without freezing the TUI event loop.
- **Symlink & Permissions Safety**: Ignores directory symlinks during recursive traversals to prevent infinite loops; skips unreadable directories gracefully with skipped counts.
- **File Category Breakdown**: Groups storage consumption into 13 `FileCategory` categories (Source code, Archives, Documents, Images, Audio/Video, Build artifacts, Binaries, etc.) based on the Phase 2.2 classification engine.
- **Proportional Unicode Heatmaps (`src/storage/heatmap.rs`)**: Renders clean horizontal bar charts (`████████░░ 80%`) for categories and direct subdirectories without requiring external graphics protocols.
- **Interactive Drill-Down & History**: Users can select subdirectories, press `Enter` to drill down into sub-scopes, press `Backspace`/`h` to navigate back through the history stack, or press `o` to open the analyzed scope directly in the file manager.

---

## Signature UI + Minimal Motion Architecture (Signature Update Phase 3.1)

### 1. Distinctive Terminal-Native Identity
- **Minimal, Not Brutal**: Eliminates rainbow dashboards, permanent clutter, and oversized toolbars. Interface remains visually quiet until the user invokes an action.
- **Signature Header (`src/ui/header.rs`)**: Displays application identity (`TerminalVision`), chevron hierarchical delimiters (`›`), active directory path, Git / project status context (`main • clean`), and responsive Command Center (`⌘K Commands • ⌘P Switcher`) affordances.
- **Signature File Rows (`src/ui/file_list.rs`)**: Unmistakable selection styling combining prominent non-color selection markers (`▸ `), active/inactive background tints, bold typography, type-specific icon indicators, and Git state.

### 2. Multi-Surface Focus System
- **Input Routing Awareness**: Every interactive surface clearly communicates where keyboard input will route:
  - `FILE MANAGER`: dual-pane navigation and file operations.
  - `SHELL (PTY)`: direct input pass-through to real pseudo-terminal emulator.
  - `COMMAND CENTER`: search query filtering and command dispatch.
  - `PREVIEW`: scrollable inspection.
  - `MODALS / DIALOGS`: structured input, confirmation, or conflict recovery.
- **Visual Focus Indicators**: Active surfaces feature clear bullet markers (`●`), thick borders, and distinct title treatments rather than relying on color alone.

### 3. Semantic Theme Architecture Preparation (`src/ui/theme.rs`)
- **Centralized Semantic Tokens**: Prepared for Phase 3.2 Dynamic Theme Engine by exposing clean semantic accessors:
  - `theme.primary()`, `theme.secondary()`, `theme.selection()`, `theme.success()`, `theme.warning()`, `theme.error()`, `theme.muted()`, `theme.border()`, `theme.focus()`, `theme.focus_inactive()`.
- **Non-Color Accessibility Symbols**: Universal fallback and standard indicators (`✓`, `✕`, `!`, `▸`, `›`, `●`, `○`) ensuring complete usability in monochrome or restricted terminal environments.

### 4. Frame-Based Minimal Motion Engine (`src/ui/motion.rs`)
- **Lightweight Transitions**: Frame-based mathematical progress calculations (`Instant` elapsed deltas) providing subtle 180–250ms transitions on startup and modal dialog entrances without background timers or CPU overhead when idle.
- **Zero-Blocking Architecture**: Animations never delay filesystem operations, terminal PTY throughput, or keyboard event handling.
- **Reduced Motion Support**: Configurable via `Settings::reduced_motion` (`[settings] reduced_motion = true`), instantly bypassing all transition frames while preserving functional feedback.

---

## Dynamic Theme Engine (Signature Update Phase 3.2)

### 1. Theme Model & Semantic Token Architecture (`src/ui/theme.rs`)
- **Centralized Definition**: Eliminates hardcoded styles across all rendering modules. All visual components (header, panes, file rows, selection markers, tabs, preview, dialogs, command center, quick switcher, storage vision, terminal, notifications) resolve colors and typography dynamically through `app.theme()`.
- **Semantic Palette Tokens (`ThemePalette`)**:
  - Surfaces: `background`, `surface`, `surface_alt`, `overlay`
  - Text & Accents: `foreground`, `foreground_muted`, `primary`, `secondary`, `accent`, `focus`, `disabled`
  - Selection: `selection`, `selection_foreground`
  - Structural: `border`, `border_active`
  - Status & Git: `success`, `warning`, `error`, `info`, `git_clean`, `git_modified`, `git_added`, `git_deleted`, `git_renamed`, `git_untracked`
  - File Types: `directory`, `symlink`, `executable`, `preview`
  - Embedded Terminal ANSI: 16-color ANSI palette (`ansi_black`..`ansi_white`, `ansi_bright_black`..`ansi_bright_white`) and `cursor`

### 2. 10 Curated Built-In Themes
The engine includes 10 built-in themes spanning signature, modern dark, retro, and accessibility palettes:
1. **`TerminalVision`**: Signature cyan, purple, and blue technical identity.
2. **`Midnight`**: Deep dark obsidian background with restrained cool accents.
3. **`Cyberpunk`**: High-energy neon yellow and hot magenta cyberpunk aesthetic.
4. **`Ocean`**: Submerged oceanic blues and crisp teal highlights.
5. **`Dracula`**: Classic gothic dark with purple and pink accents.
6. **`Nord`**: Arctic cool blue, frost, and aurora borealis palette.
7. **`Matrix`**: Retro CRT phosphor green and deep pitch black matrix.
8. **`SolarizedDark`**: Precision-engineered Solarized dark color structure.
9. **`Monochrome`**: Minimalist pure grayscale with maximum typographic clarity.
10. **`HighContrast`**: WCAG AAA compliant high-distinction accessible palette.

### 3. Theme Registry (`ThemeRegistry`)
- **Lookup & Resolution**: `ThemeRegistry::get(id: ThemeId)` and `ThemeRegistry::get_by_str(id_str: &str)` resolve themes case-insensitively with automatic hyphen/underscore normalizations.
- **Fail-Safe Fallback**: Any unrecognized or corrupted theme identifier safely falls back to `ThemeId::TerminalVision` without crashing or panicking.
- **Cycling**: `ThemeRegistry::next(id)` and `ThemeRegistry::prev(id)` cycle seamlessly across all 10 themes in canonical order.

### 4. Live Preview & Interactive Theme Selector (`Mode::ThemeSelector`)
- **Non-Destructive Live Preview**: As the user navigates the Theme Selector modal (`Up`/`Down`/mouse hover), `app.theme()` instantly renders the highlighted theme across the entire application interface in real time via `preview_theme`.
- **Apply vs. Cancel**:
  - **Apply (`Enter` / Apply button)**: Commits the selected theme to `active_theme`, clears preview state, saves to persistent settings, and posts a confirmation toast.
  - **Cancel (`Esc` / Cancel button / click outside)**: Clears `preview_theme` and instantly restores the previous `active_theme` without persistent mutation.
- **Responsive Layout**: Renders swatches, active indicators (`●`), selection arrows (`▶`), and action buttons across all required terminal resolutions (from 40x10 up to 240x80).

### 5. Embedded Terminal ANSI Palette Synchronization
- **Color Mapping (`theme.map_terminal_color(...)`)**: Maps standard ANSI 16 colors output by terminal shell programs directly to the active theme's synchronized palette.
- **PTY Session Continuity**: Theme switching instantly re-styles the terminal panel and buffer cell colors without restarting the shell process or dropping scrollback history.
- **Limited Capability Fallback (`Theme::to_16_color(...)`)**: Automatically maps 24-bit RGB colors to nearest 16-color ANSI equivalents on restricted terminal emulators.

### 6. Settings Persistence & Action Integration
- **Configuration Serialization**: Persisted under `[settings] theme = "<theme_id>"` in `config.tv`.
- **Action Registry & Shortcuts**:
  - `Action::ThemeSelector`: Opens interactive Theme Selector modal (registered in Command Center, menu, and keybindings).
  - `Action::NextTheme` / `Action::PrevTheme`: Quick theme cycling actions.

---

## Project Intelligence Engine (Phase 1.1)

The Project Intelligence Engine (`src/project/`) transitions TerminalVision from treating directories as anonymous file folders to recognizing and fingerprinting workspace boundaries, project ecosystems, and development topologies.

```
Filesystem -> Project Intelligence (Detector + Signals) -> ProjectFingerprint -> Application State
```

### 1. Architectural Principles & Guarantees
- **Read-Only Safety**: The engine NEVER creates, writes, mutates, or deletes any files, directories, or Git repositories. It never executes builds, invokes package managers, or spawns subprocesses.
- **Bounded Latency & Performance**: Unbounded recursive scans are strictly forbidden. The engine inspects only the root folder (capped at 128 shallow entries) and queries immediate existence for standard directories (`src/`, `tests/`, `docs/`, `config/`, `.github/`, etc.).
- **Deterministic Root Resolution**: Starts from any arbitrary file or directory path and walks safely upward (capped at 32 hops) to find the nearest primary manifest (`Cargo.toml`, `package.json`, `go.mod`, `pom.xml`, etc.), secondary manifest (`Makefile`, `requirements.txt`), Git boundary (`.git`), or generic documentation marker (`README.md`, `LICENSE`).
- **Cross-Platform**: Operates identically on macOS, Linux, and Windows using normalized path abstractions, safely handling filesystem roots (`/`, `C:\`), symlinks, missing paths, and permission restrictions.

### 2. Supported Project Types, Languages & Build Systems
- **Project Types**: `Rust`, `Node`, `JavaScript`, `TypeScript`, `Python`, `Java`, `Go`, `C`, `Cpp`, `Php`, `Ruby`, `DotNet`, `GenericGit`, `GenericWorkspace`, `Generic`.
- **Languages**: `Rust`, `JavaScript`, `TypeScript`, `Python`, `Java`, `Kotlin`, `Go`, `C`, `Cpp`, `Php`, `Ruby`, `CSharp`, `Html`, `Css`, `Shell`, `Generic`.
- **Build Systems**: `Cargo`, `npm`, `Yarn`, `pnpm`, `Bun`, `pip`, `Poetry`, `Pipenv`, `Maven`, `Gradle`, `Go Modules`, `CMake`, `Make`, `Composer`, `Bundler`, `.NET CLI`, `Generic`.

### 3. Rich Signal Taxonomy (`ProjectSignals`)
The shallow inspector records structured signals without traversing heavy directory trees:
- **Source Signals**: `src/`, `app/`, `lib/`, `packages/`, `crates/`, `cmd/`, `pkg/`, `internal/`, `include/`, `sources/`.
- **Test Signals**: `tests/`, `test/`, `__tests__/`, `spec/`, `testing/`.
- **Documentation Signals**: `README.md`, `README`, `README.rst`, `docs/`, `doc/`, `LICENSE`, `CHANGELOG.md`, `CONTRIBUTING.md`.
- **Configuration Signals**: `.env`, `.env.example`, `.editorconfig`, `.gitignore`, `tsconfig.json`, `pnpm-workspace.yaml`, `lerna.json`, `turbo.json`, `go.work`.
- **CI/CD Signals**: `.github/`, `.gitlab-ci.yml`, `Jenkinsfile`, `.circleci/`, `.travis.yml`, `azure-pipelines.yml`.
- **Container Signals**: `Dockerfile`, `docker-compose.yml`, `compose.yml`, `Containerfile`, `.dockerignore`.
- **Build / Cache Indicators**: `target/`, `node_modules/`, `dist/`, `build/`, `out/`, `.next/`, `bin/`, `obj/`, `vendor/`.

### 4. Project Fingerprint Representation (`ProjectFingerprint`)
A structured, immutable domain model containing:
- `root: Option<PathBuf>`
- `name: Option<String>`
- `project_types: Vec<ProjectType>`
- `languages: Vec<Language>`
- `build_systems: Vec<BuildSystem>`
- `manifests: Vec<PathBuf>`
- `signals: ProjectSignals`
- `is_git: bool`, `git_root: Option<PathBuf>`, `is_git_worktree: bool`
- `confidence: DetectionConfidence` (`None`, `Low`, `Medium`, `High`, `Definitive`)
- `parent_workspace: Option<PathBuf>` & `subprojects: Vec<PathBuf>` & `is_workspace: bool`

### 5. In-Memory Cache & Invalidation Lifecycle (`ProjectCache`, `SharedProjectCache`)
- **Normalized Keys**: Canonicalized path indexing prevents duplicate detection overhead across relative/symlinked paths.
- **TTL Expiration**: Configurable TTL (default 5 seconds) ensures project fingerprints remain fresh without stale state persisting indefinitely.
- **Capacity Bounds**: Maximum entry cap (default 256) prevents memory accumulation.
- **Granular Invalidation**: Supports `invalidate(path)`, `invalidate_root(root)`, and `clear()`.

### 6. Monorepo & Multi-Project Preparation
The architecture distinguishes between standalone projects, nested packages, and root workspaces (such as Cargo workspaces, pnpm workspaces, Turborepo, Lerna, and Go workspaces), preparing the foundation for future project-aware features.

---

## Workspace Structure & Project Graph (Phase 1.2)

Phase 1.2 upgrades Project Intelligence from recognizing a project's identity to understanding the structural topology and internal graph of the entire workspace.

```
Workspace Root / Path -> WorkspaceAnalyzer -> WorkspaceContext -> [ ProjectNode Tree + Classified Directories + Important Files ]
```

### 1. Structural Graph Models
- **`WorkspaceContext` (`src/project/workspace.rs`)**:
  - `root: Option<PathBuf>`
  - `name: Option<String>`
  - `is_monorepo: bool`
  - `primary_project: Option<ProjectNode>`
  - `projects: Vec<ProjectNode>`
  - `directories: Vec<ClassifiedDirectory>`
  - `important_files: Vec<ImportantFile>`
  - `signals: ProjectSignals`
- **`ProjectNode` (`src/project/workspace.rs`)**:
  - `id: String`, `name: String`, `root: PathBuf`
  - `project_type: ProjectType`, `languages: Vec<Language>`
  - Partitioned structural segments: `manifests`, `source_directories`, `test_directories`, `documentation_directories`, `configuration_directories`, `build_output`, `generated_output`, `dependencies`, `important_files`.
  - `parent_workspace: Option<PathBuf>`, `is_root_project: bool`.

### 2. Directory Role Classification (`DirectoryRole`)
- **Source**: `src/`, `app/`, `lib/`, `packages/`, `components/`, `crates/`, `cmd/`, `pkg/`, `internal/`, `include/`, `sources/`.
- **Tests**: `tests/`, `test/`, `__tests__/`, `spec/`, `testing/`.
- **Documentation**: `docs/`, `doc/`, `documentation/`.
- **Configuration**: `config/`, `.config/`, `.vscode/`, `.idea/`, `.settings/`.
- **BuildOutput**: `target/`, `dist/`, `build/`, `out/`, `bin/`, `obj/`.
- **Generated**: `.next/`, `.nuxt/`, `.turbo/`, `generated/`.
- **Cache**: `.cache/`, `.pytest_cache/`, `.mypy_cache/`, `.cargo-cache/`.
- **Dependencies**: `node_modules/`, `vendor/`, `third_party/`.
- **CI**: `.github/`, `.gitlab/`, `.circleci/`, `.buildkite/`.
- **Tooling**: `scripts/`, `tools/`, `.husky/`.
- **Assets**: `assets/`, `static/`, `public/`, `media/`, `templates/`.

### 3. Important File Classification (`ImportantFileRole`, `ImportantFile`)
- **Manifest**: `Cargo.toml`, `package.json`, `pyproject.toml`, `requirements.txt`, `go.mod`, `pom.xml`, `build.gradle`, `CMakeLists.txt`, `composer.json`, `Gemfile`, `*.csproj`, `pnpm-workspace.yaml`, `lerna.json`, `turbo.json`, `go.work`.
- **Documentation**: `README.md`, `CHANGELOG.md`, `CONTRIBUTING.md`, `ARCHITECTURE.md`.
- **License**: `LICENSE`, `LICENSE.md`, `COPYING`.
- **Configuration**: `tsconfig.json`, `jsconfig.json`, `.editorconfig`, `.gitignore`, `.env`, `.env.example`.
- **Build**: `Makefile`, `CMakeLists.txt`, `build.rs`.
- **CI**: `.gitlab-ci.yml`, `Jenkinsfile`, `.travis.yml`, `azure-pipelines.yml`.
- **Container**: `Dockerfile`, `docker-compose.yml`, `compose.yml`, `Containerfile`.
- **EntryPoint**: `main.rs`, `index.ts`, `index.js`, `main.py`, `app.py`, `main.go`, `App.java`.

### 4. Scan Boundaries, Symlink Protection & Performance
- **Bounded Shallow Inspection**: Only inspects shallow immediate directory entries (capped at 128 entries per directory level, max 32 subprojects discovered).
- **Hard Scan Boundaries**: Recursive scanning into `node_modules/`, `target/`, `.git/`, `dist/`, `build/`, `.cache/`, `vendor/` is strictly avoided.
- **Symlink Cycle Protection**: Visited canonical filesystem paths are tracked via `HashSet<PathBuf>`, preventing infinite loops in circular symlinks (`A -> B -> A`).
- **Non-Fatal Permissions**: Inaccessible directories degrade safely with zero panics.
- **Cache Integration**: `ProjectCache` and `SharedProjectCache` cache both `ProjectFingerprint` and `WorkspaceContext` by normalized path with automatic TTL and root invalidation.

---

## 18.3 Project-Aware UI & Command Center Integration (Phase 1.3)

Phase 1.3 connects Project Intelligence and Workspace Intelligence directly to TerminalVision's user interface, Command Center, Quick Switcher, Breadcrumbs, Preview, and Action Registry.

```
Filesystem (Detection & Bounded Scan)
    ↓
Project Intelligence Engine (ProjectFingerprint & WorkspaceContext)
    ↓
Application State (PaneState / TabState / App)
    ↓
UI Presentation:
    ├── Signature Header (Subtle Project & Monorepo identity)
    ├── Interactive Breadcrumb (Visually highlighted Project Root with click navigation)
    ├── Command Center (Context-aware project actions & fuzzy match boosts)
    ├── Quick Switcher (Project structure & classified files priority)
    ├── Project Overview Dialog (Compact, responsive, terminal-native modal)
    ├── Metadata Preview (Project & Directory Role context tags)
    └── Universal Action Registry (Fully registered keyboard & mouse accessible actions)
```

### 1. Authoritative Application State Flow
- **Single Source of Truth**: All UI components receive `WorkspaceContext` and `ProjectFingerprint` through `PaneState` / `App`. No UI component performs independent filesystem scanning.
- **Dynamic Context Refresh**: As the user navigates between projects, into nested projects, or into non-project folders (e.g., `~/Downloads`), the authoritative project context automatically updates and synchronizes with preview and action filtering.
- **Graceful Fallback**: Non-project folders display clean paths without synthetic `"Project: Unknown"` or `"Project: None"` text.

### 2. Signature Header Project Context
- **Subtle Branding**: Embeds compact, semantic project indicators into the right-aligned status section of the header.
  - Examples: `Rust · Git · main*`, `TypeScript · main`, `[ws: Monorepo · frontend]`.
- **Responsive Degradation**: When terminal width is constrained, secondary metadata cleanly collapses to compact forms (`[proj: name]`) or gracefully hides before clipping navigation breadcrumbs.

### 3. Command Center & Universal Action Registry Integration
- **Context-Aware Dynamic Filtering**: The Command Center (`Ctrl/Cmd+K`) dynamically inspects `ContextFilter` to surface only actions that exist:
  - `Action::GoProjectRoot` (when inside a project)
  - `Action::ProjectCockpit` (Project Overview modal)
  - `Action::GoSourceDir` (only when source directory exists)
  - `Action::GoTestsDir` (only when tests directory exists)
  - `Action::GoDocsDir` (only when documentation directory exists)
  - `Action::OpenManifest` (only when manifest exists)
  - `Action::OpenReadme` (only when README exists)
  - `Action::OpenLicense` (only when LICENSE exists)
- **Fuzzy Search Boosts**: Keywords such as `"source"`, `"tests"`, `"docs"`, `"manifest"`, `"readme"`, `"overview"`, and `"root"` boost project actions to the top of palette searches.

### 4. Quick Switcher (`Ctrl/Cmd+P`) Project Structure Surface
- Surfaces structured workspace navigation targets with distinct category badges:
  - `PROJECT`: Project root directory
  - `SOURCE`: Primary source directories (`src/`, `lib/`, etc.)
  - `TESTS`: Test suites (`tests/`, `spec/`, etc.)
  - `DOCS`: Documentation directories (`docs/`, etc.)
  - `IMPORTANT`: Key project files (`Cargo.toml`, `README.md`, `LICENSE`, etc.)
  - `RECENT` / `CURRENT` / `PARENT` / `GIT ROOT`: Preserved recent locations and contextual paths.

### 5. Compact Project Overview Modal
- **Terminal-Native & Responsive**: Accessible via `Action::ProjectCockpit` (Command Center or shortcut), rendering project identity, workspace context, languages, git status, and immediate navigation actions.
- **Input Handling**: Full keyboard (Arrows, Enter, Esc) and mouse click support with bounding hit testing.

### 6. Breadcrumb & Preview Integration
- **Interactive Breadcrumb**: Highlights the project root segment in the path hierarchy; clicking the root segment immediately navigates to the project root.
- **Metadata Preview**: When inspecting source files or directories, metadata preview includes associated Project and Directory Role context without performing expensive parsing.

---

## 20. Signature Terminal Motion — Animation Engine & Motion Infrastructure

TerminalVision incorporates a lightweight, deterministic, non-blocking, terminal-native animation engine (`src/animation/`) designed to power rich terminal transitions without compromising responsiveness or performance.

> **Developer Rule**: *"Animations communicate state; they do not replace state."*

### 1. Architectural Model & Responsibilities
- **Frame-Based Execution**: Runs synchronously within the existing application event loop. No secondary render threads, fake loading sleeps, or busy loops are used.
- **Event Loop Integration**: 
  - `main.rs` dynamically queries `app.has_active_animations()`.
  - When animations are active, polling timeout switches to the targeted frame interval (30–60 FPS, default ~16ms).
  - When idle (no animations running), polling immediately drops back to the power-saving default (100ms), consuming near-zero CPU.
- **PTY & Filesystem Isolation**: Animations never block or intercept PTY input/output, filesystem operations, shell execution, or directory scanning.
- **Deterministic Clocks**: The engine abstracts time through the `AnimationClock` trait:
  - `RealClock`: Standard monotonic production clock using `std::time::Instant`.
  - `ManualClock`: Thread-safe controllable clock for sub-millisecond, deterministic, non-flaky test execution without real-time delays.

### 2. Core Abstractions (`src/animation/`)
- **`AnimationEngine<C>`**: Tracks active, completed, and cancelled animation instances, advances progress via `.tick()`, prunes completed tracks, and coordinates tag-based lookup and cancellation.
- **`Animation` & `AnimationId`**: Individual animation track containing start time, duration, progress, state, tag, and optional typed payload.
- **`AnimationTag`**: Semantic tags (e.g., `VisionBoot`, `PanelTransition`, `SelectionTransition`, `ModalTransition`, `VisionPulse`, `Feedback`) allowing targeted lifecycle operations.
- **`AnimationState`**: Explicit lifecycle states: `Created` → `Running` → `Completed` / `Cancelled`.
- **`AnimationProgress`**: Strictly clamped and sanitized progress value in `[0.0, 1.0]`. Protects against `NaN`, `Infinity`, underflow, and overflow.
- **`Easing`**: Monotonically clamped mathematical curves:
  - `Linear`
  - `EaseIn` (quadratic)
  - `EaseOut` (quadratic)
  - `EaseInOut` (quadratic)
  - `CubicEaseInOut`
  - `SmoothStep` (Hermite polynomial)
- **`Transition<T>`**: Generic start-to-end interpolation over eased progress for types like `f32` and `u16`.
- **`Geometry` Utilities**: Safe Rect interpolation (`interpolate_rect`), center expansion (`expand_rect_from_center`), directional sliding (`slide_rect_x`, `slide_rect_y`), and bounding-box clamping (`clamp_rect_to_bounds`) guaranteeing non-negative dimensions and zero panic conditions.

### 3. Motion Preferences & Accessibility
TerminalVision treats animation as progressive enhancement:
- **`MotionMode`**:
  - `Full`: Normal durations and full transition effects (target 60 FPS).
  - `Reduced`: Transitions are compressed to $\le 20\%$ duration or rendered with instantaneous jump transitions (target 30 FPS).
  - `Off`: Animations are bypassed or completed in 0ms; final state is rendered immediately.
- **`StartupMotionMode`**: Configuration preparation for startup sequences (`Cinematic`, `Minimal`, `Off`).
- **Accessible State Parity**: Disabling motion renders the exact same destination state and indicators instantly without missing visual cues or broken focus.

### 4. Cancellation & Resize Safety
- **Immediate Cancellation**: User input (such as pressing `Esc` or initiating an action) can immediately cancel running animations via `engine.cancel_all()` or `engine.cancel_by_tag()`.
- **Clean Fallback**: Cancellation leaves the UI in its stable destination state with no lingering overlays or corrupt layouts.
- **Dynamic Geometry Recalculation**: Animations never persist absolute screen coordinates across frames. All visual bounding boxes are calculated from the current frame's `terminal.size()` or layout Rect, preventing layout tearing or panics during terminal resizing.

### 5. Vision Boot Sequence (`src/animation/boot.rs`, `src/ui/boot.rs`)
TerminalVision's signature startup experience visually constructs the workspace environment and presents real system readiness:
- **Phase Sequence**:
  1. `Wake` (0.00..0.20): Minimal central beacon and subtle horizontal expansion (`●` / `─────●─────`).
  2. `Identity` (0.20..0.40): Progressive reveal of `TERMINALVISION` identity and signature tagline `SEE · UNDERSTAND · CONTROL`.
  3. `SystemReadiness` (0.40..0.65): Real-time readiness reporting across Filesystem, Terminal PTY, Configuration, Project, and Git subsystems.
  4. `ProjectAwareness` (0.65..0.85): Workspace structure card (project identity, language, tools, manifest/readme detection, or workspace item counts).
  5. `VisionPulse` (0.85..1.00): Progressive construction of live UI layers (header, panes, preview, footer, terminal) overlaid with an accent pulse and ready cursor `$ _`.
- **Real Initialization Invariant**: The boot sequence never invents fake project types or git branches. If launched in a non-project directory (e.g. `~/Downloads`), it presents `WORKSPACE READY` and entry totals.
- **Instant Skip**: Pressing any key (`Esc`, `Enter`, `Space`, `q`) or clicking the mouse terminates the boot sequence instantly and restores interactive focus to the file manager.
- **Startup Motion Modes**:
  - `Cinematic`: Full ~1.8s startup presentation.
  - `Minimal`: ~400ms accelerated startup.
  - `Off` / `Reduced`: Bypassed directly to normal UI (0ms) or compressed to ~350ms with linear presentation.
- **Terminal Capability & Small Screen Safety**: Terminals under 60 cols or 14 rows automatically degrade to single/double-line compact status without border tearing, text clipping, or coordinate panics.

### 6. Signature Motion & Micro-Interactions (`src/animation/micro.rs`)
Post-startup interactions across TerminalVision are enriched with purposeful, subtle, non-blocking micro-interactions:

> **Developer Rule**: *"Never add animation merely because animation is possible."*

- **Standard Durations & Curves**:
  - **Navigation** (`AnimationTag::Navigation`): ~140ms (`Easing::EaseOut`), applied on directory traversal and path jumping.
  - **Selection** (`AnimationTag::Selection`): ~100ms (`Easing::EaseOut`), applied on keyboard/mouse cursor changes.
  - **Focus** (`AnimationTag::Custom("Focus")`): ~120ms (`Easing::EaseOut`), applied on active pane / terminal switching.
  - **Command Center** (`AnimationTag::CommandCenter`): ~150ms (`Easing::EaseOut`), applied on `Ctrl/Cmd+K` palette opening.
  - **Quick Switcher** (`AnimationTag::QuickSwitcher`): ~150ms (`Easing::EaseOut`), applied on `Ctrl/Cmd+P` switcher opening.
  - **Modal Dialogs** (`AnimationTag::Dialog`): ~150ms (`Easing::EaseOut`), applied on dialog entrance (Confirm, Input, Rename, Create, Help, Project Cockpit, Git Status, Radar, Reveal, Storage Vision, Theme Selector).
  - **Context Menu** (`AnimationTag::ContextMenu`): ~120ms (`Easing::EaseOut`), applied on popup context menu invocation.
  - **Preview** (`AnimationTag::Custom("Preview")`): ~120ms (`Easing::EaseOut`), applied on preview panel content changes.
  - **Operation Feedback** (`AnimationTag::Operation`): ~160ms (`Easing::EaseOut`), applied during file copy, move, delete, rename progress.
  - **Vision Pulse** (`AnimationTag::VisionPulse`): ~220ms (`Easing::CubicEaseInOut`), applied upon significant state milestones (project analysis complete, batch operation done, workspace refreshed).
- **Rapid Input Superseding**:
  - Animations are strictly non-queuing. Holding `Down`, `Up`, or rapid key sequences instantly replaces and supersedes previous animation tracks with 0 latency.
- **PTY Terminal Safety**:
  - Shell keystrokes and raw terminal output streams are never intercepted, delayed, buffered, or modified by animation routines.
- **Accessibility & Motion Fallbacks**:
  - When `MotionMode::Reduced` is selected: All transition durations are compressed to $\le 60$ms with `Easing::Linear`, disabling scaling movements.
  - When `MotionMode::Off` is selected: All transitions execute in 0ms (instant state updates without animation frames).
  - Every visual state retains non-animated textual and structural equivalents (borders, checkmarks `✓`, error crosses `✕`, warnings `!`).

---

## 21. Interactive Context Menu Foundation (Phase 3.1)

TerminalVision provides a fully native, contextual popup action menu (`src/commands/context_menu.rs`, `src/ui/dialogs.rs`) integrating deeply with the Universal Action Registry.

```
Target Selection (File / Dir / Multi / Empty)
    ↓
ContextMenuTarget & build_context_menu_items()
    ↓
ContextMenuItem / ContextMenuGroup
    ↓
Action Registry (Action::* Dispatch)
    ↓
App::handle_action() Execution
```

### 1. Architectural Model & Layer Separation
- **Target Extraction**: `ContextMenuTarget` isolates target context:
  - `File { path, is_image, is_executable, is_source }`
  - `Directory { path, is_project_root }`
  - `Multiple { paths, count, dir_count, file_count }`
  - `EmptyPane { current_dir }`
- **Zero Filesystem Logic**: The context menu creates NO duplicate filesystem handlers. Every action item holds an [`Action`] discriminant resolved directly by `App::handle_action()`.
- **Dynamic Grouping (`ContextMenuGroup`)**: Menus are segmented into standard semantic groups (`Primary`, `ClipboardOperations`, `FilesystemOperations`, `Inspection`, `Advanced`, `Project`).

### 2. Selection & Right-Click Semantics
- **Unselected Item Click**: Right-clicking an item outside the current selection shifts cursor selection to that single item and targets it.
- **Selected Item Click**: Right-clicking an item within an active multi-selection preserves the entire multi-selection and displays aggregate bulk actions (e.g. `Copy 7 Items`, `Delete 7 Items`).
- **Empty Space Click**: Right-clicking empty space targets the directory background (`New File`, `New Directory`, `Paste`, `Refresh`).

### 3. Positioning & Viewport Clamping Algorithm
- `calculate_context_menu_rect` and `calculate_context_submenu_rect` calculate popups near cursor position:
  - Right-edge collision: Repositions popup horizontally to the left (`x = x.saturating_sub(width)`).
  - Bottom-edge collision: Repositions popup vertically upwards (`y = y.saturating_sub(height)`).
  - Four-boundary bounding: Ensures rectangle never extends beyond terminal bounds or into negative dimensions.
- **Scroll Windowing**: When menu item counts exceed terminal viewport height, rendering uses a dynamic visible sliding window keeping the highlighted item centered without UI overflow.

### 4. Submenu & Dismissal Model
- **Submenus (`More ›`)**: Secondary actions (`Copy Path`, `Copy Name`, `Reveal in Terminal`, `Open in New Tab`, `Storage Vision`) nest cleanly in submenus.
- **Two-Stage Dismissal**:
  - `Esc` with submenu open -> Closes submenu and focuses parent item.
  - `Esc` with main menu open -> Closes context menu and restores pane focus.
  - Left click outside popup bounds dismisses the menu immediately.

### 5. Terminal Focus Safety
- When the embedded terminal has focus (`Mode::Terminal`), right-clicking within the terminal pane passes through to terminal/PTTY behavior.
- Context menus never pop over the interactive PTY shell or corrupt terminal standard input.

### 6. Mouse + Keyboard Action Experience (Phase 3.2)
- **True Parity**: Every action is reachable identically via mouse, keyboard, and `ActionRegistry`.
- **Keyboard Entry**: `Shift+F10` and dedicated `Menu` key summon the context menu relative to the current active pane and cursor row.
- **Advanced Navigation**:
  - `Up` / `Down` with cyclic boundary wrapping.
  - `Home` / `End` to jump directly to first / last selectable items.
  - `PageUp` / `PageDown` to jump by 5 items.
  - `Right` (`→`) or `Enter` on `More ›` to expand submenus; `Left` (`←`) or `Esc` to return to parent menu.
- **Lightweight Type-to-Select**: Typing characters (e.g. `d`, `del`, `c`) instantly jumps to the matching selectable menu item by prefix; repeated typing of the same single character cycles through all matching items.
- **Dynamic Shortcut Synchronization**: Menu items dynamically resolve shortcut display strings via `ShortcutRegistry::global().primary_shortcut(action, platform)`.
- **Disabled Action Non-Interactivity**: Inactive items (`Paste` when clipboard empty) are visually distinct and cannot be selected or activated via mouse or keyboard.









