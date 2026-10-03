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


