# TerminalVision

> See your filesystem. Understand your project. Control everything without leaving the terminal.

TerminalVision is a high-performance, native Rust terminal file manager and developer intelligence tool designed for keyboard-first, mouse-capable filesystem navigation. It runs completely local and offline, with zero shell or external binary execution.

---

## Technical Overview

- **Language & Runtime**: 100% Rust (2024 edition, stable toolchain)
- **TUI & Rendering Engine**: [Ratatui](https://crates.io/crates/ratatui) (0.30)
- **Terminal Input & Backend**: [Crossterm](https://crates.io/crates/crossterm) (0.29)
- **Architecture**: Layered, event-driven design (`UI` → `Application` → `Domain / Filesystem` → `OS`)
- **Dependencies**: Ultra-minimal, zero subprocess execution (`cp`, `mv`, `rm`, `git`, `sh` are never spawned)
- **Binary Footprint**: Single self-contained binary (< 1.5 MB release build)

---

## Core Characteristics

- **Native Terminal Application**: Runs cleanly directly inside standard terminal emulators without webviews, Electron, Node.js, or browser runtimes.
- **Cross-Platform Compatibility**: Fully abstracted path handling and filesystem operations supporting macOS, Linux, and Windows.
- **Pure Filesystem API Integration**: Performs file and directory operations using Rust's standard library `std::fs` and `std::path` APIs instead of invoking shell utilities.
- **Keyboard-First Design**: Complete Vim-style (`h`/`j`/`k`/`l`) and arrow-key navigation with shortcuts for every file management action.
- **Mouse Interaction**: Full mouse support including single-click selection, double-click to navigate/preview, tab switching, and scroll-wheel browsing.
- **Responsive TUI**: Dynamic screen geometry calculation that automatically adapts from compact single-pane view (< 80 columns) to dual-pane view and 3-column preview layout (160+ columns).
- **Distraction-Free Focus Mode**: Single-key toggle (`Z` or `Ctrl+F`) to maximize the active pane across the entire terminal width.
- **In-Process Git & Project Intelligence**: Pure in-process Git repository inspection (reading `.git` HEAD and index directly) and multi-ecosystem project detection without subprocess calls.

---

## Features

### Dual-Pane File Navigation
Browse two directories side by side with independent tab state, active pane switching (`Tab`), and entry sorting (by Name, Size, Modified Date, or File Type). Toggle hidden dotfiles instantly (`.`).

### Safe File Operations
Create files (`n`), create directories (`N`), rename entries (`r`), copy (`y`), cut (`x`), and paste (`p`) with conflict detection to prevent accidental overwrites. Delete (`d`) opens a confirmation modal before removing entries.

### Multi-Item Selection
Select multiple entries using `Space`, select all (`Ctrl+A`), invert selection (`*`), or deselect all (`u`). Perform batch copy, cut, or delete operations seamlessly.

### Read-Only Code & File Preview
Inspect text files and source code (`v` or double-click) with lightweight syntax highlighting (Rust, JavaScript, TypeScript, Python, Java, C, C++, Go, PHP, Ruby, C#, HTML, CSS, JSON, TOML, YAML, Markdown, Shell). Previews enforce strict safety bounds (max 1 MB read limit, line limits) and sanitize terminal escape sequences.

### Live & Recursive Search
Filter current directory listings in real time (`/`). Cycle through search modes with `Tab`:
1. **Basic**: Fast case-insensitive substring search.
2. **Recursive**: Non-blocking background directory tree traversal.
3. **Fuzzy**: Approximate matching for quick file discovery.
4. **Recursive + Fuzzy**: Deep fuzzy matching across the full directory hierarchy.

### Multi-Tab & Directory Bookmarks
Open independent directory tabs per pane (`t`, `w`, `[`, `]`, `T` to duplicate). Save frequently accessed directories to persistent bookmarks (`b`, `B`) stored in standard user configuration directories.

### Developer Intelligence & Power Tools
- **Project Cockpit (`P`)**: Developer overview modal showing project ecosystem, root, branch, manifest (`Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `pom.xml`, etc.), README, LICENSE, source directory, and quick actions.
- **Git Status Panel (`G`)**: Interactive overview of all modified (`M`), added (`A`), deleted (`D`), renamed (`R`), and untracked (`?`) files with direct navigation to changed items.
- **File Radar & Directory Insights (`F`)**: Instant directory breakdown with file/dir/symlink/hidden counts, total byte sizing, and top file type distribution.
- **Reveal Context (`C`)**: Hierarchical context inspector displaying the chain from `Current File` → `Parent Directory` → `Project Root` → `Git Repository Root` with quick jump navigation.
- **Smart Jump (`J`)**: Unified fuzzy location picker aggregating Git root, project root, home, bookmarks, and tab history.

### Command Palette & Interactive Help
Press `Ctrl+P` to launch a fuzzy-filterable Command Palette to discover and execute any application action. Press `?` for an interactive cheat-sheet of keyboard shortcuts and mouse controls.

---

## Requirements & Platform Support

### Supported Platforms

| Platform | Target Architecture | Build & Test Status |
| --- | --- | --- |
| **macOS** | `aarch64-apple-darwin` / `x86_64-apple-darwin` | **Fully Verified** |
| **Linux** | `x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu` | **Fully Verified** |
| **Windows** | `x86_64-pc-windows-msvc` / `x86_64-pc-windows-gnu` | **Fully Verified** |

### Supported Terminals
- macOS: Terminal.app, iTerm2, Alacritty, Kitty, Ghostty, WezTerm
- Linux: GNOME Terminal, Konsole, Alacritty, Kitty, WezTerm, foot, xterm
- Windows: Windows Terminal, PowerShell, Command Prompt, WezTerm, Alacritty

---

## Installation & Building

### Prerequisites

- [Rust Toolchain](https://www.rust-lang.org/) (Stable compiler targeting Rust 2024 edition, 1.85+)
- Cargo package manager

### Build from Source

```bash
# Clone the repository
git clone https://github.com/immanuelmelbin/TerminalVision.git
cd TerminalVision

# Build optimized release binary
cargo build --release
```

The compiled release binary is located at:
- `target/release/TerminalVision` (macOS / Linux)
- `target/release/TerminalVision.exe` (Windows)

### Cargo Local Installation

```bash
cargo install --path .
```

Ensure `~/.cargo/bin` (or `%USERPROFILE%\.cargo\bin` on Windows) is in your system `PATH`.

---

## Launching TerminalVision

```bash
# Launch in the current working directory
TerminalVision

# Launch starting in a specific directory
TerminalVision /path/to/directory
```

---

## Keyboard Shortcuts

### Normal Mode (Navigation & File Browsing)

| Shortcut | Action | Description |
| --- | --- | --- |
| `Up` / `k` | Move Up | Move selection up one entry |
| `Down` / `j` | Move Down | Move selection down one entry |
| `Left` / `h` | Move Left | Navigate left / select |
| `Right` / `l` | Move Right | Navigate right / select |
| `Enter` | Open | Open selected directory |
| `Backspace` | Go Parent | Navigate up to parent directory |
| `Home` | Jump to First | Jump to first entry in current directory |
| `End` | Jump to Last | Jump to last entry in current directory |
| `PageUp` | Page Up | Scroll selection up by one visible page |
| `PageDown` | Page Down | Scroll selection down by one visible page |
| `Tab` | Switch Pane | Toggle active pane focus (Left / Right) |
| `g` | Jump to Path | Open path jump input modal (supports `~`, absolute, relative) |
| `J` / `Shift+J` | Smart Jump | Open unified Smart Jump picker (Roots, Bookmarks, History, Tabs) |
| `Alt+Left` | History Back | Navigate back in tab directory history |
| `Alt+Right` | History Forward | Navigate forward in tab directory history |
| `n` | New File | Open modal to create a new file |
| `N` / `Shift+N` | New Directory | Open modal to create a new directory |
| `r` | Rename | Open modal to rename selected entry |
| `y` | Copy | Copy selected entry to clipboard buffer |
| `x` | Cut | Cut selected entry to clipboard buffer |
| `p` | Paste | Paste clipboard entries into active directory |
| `d` | Delete | Prompt confirmation to delete selected entry |
| `Space` | Toggle Select | Toggle multi-selection on current item |
| `Ctrl+A` | Select All | Select all entries in active pane |
| `u` | Deselect All | Clear all selected entries in active pane |
| `*` | Invert Selection | Invert item selection in active pane |
| `.` | Toggle Hidden | Show or hide hidden (dot) files |
| `s` | Change Sort | Cycle sort order (Name → Size → Modified → Type) |
| `v` | Preview | Toggle read-only file preview overlay |
| `Z` / `Ctrl+F` | Focus Mode | Toggle distraction-free full-width focus mode |
| `P` / `Shift+P` | Project Cockpit | Open Project Cockpit with project details and quick navigation |
| `G` / `Shift+G` | Git Status | Open Git Status Panel with changed files list |
| `F` / `Shift+F` | File Radar | Open File Radar directory metrics & extension distribution |
| `C` / `Shift+C` | Reveal Context | Open Reveal Context hierarchy inspector (File → Dir → Project → Git) |
| `b` | Add Bookmark | Save active directory to bookmarks |
| `B` / `Shift+B` | Open Bookmarks | Open directory bookmarks manager modal |
| `t` | New Tab | Open a new tab in the active pane |
| `T` / `Shift+T` | Duplicate Tab | Duplicate the active tab and its state |
| `w` | Close Tab | Close the active tab in the active pane |
| `]` | Next Tab | Switch to the next tab in active pane |
| `[` | Previous Tab | Switch to the previous tab in active pane |
| `/` | Search | Activate live in-directory search |
| `Ctrl+P` | Command Palette | Open fuzzy categorized command palette overlay |
| `?` | Help | Show keyboard shortcuts and mouse help modal |
| `q` | Quit | Exit TerminalVision cleanly |
| `Esc` | Cancel | Clear active selection, notification, or close modal |
| `Ctrl+C` | Force Quit | Unconditional immediate exit request |

### Search Mode

| Shortcut | Action |
| --- | --- |
| Printable Chars | Append character to live search query (supports `*.rs`, `ext:rs`, subpaths) |
| `Backspace` | Remove last character from search query |
| `Tab` | Cycle search mode (Basic → Recursive → Fuzzy → Recursive + Fuzzy) |
| `Enter` | Confirm search selection and return to Normal mode |
| `Esc` | Cancel search and restore full directory listing |

### Modals & Text Input

| Mode | Shortcuts & Behavior |
| --- | --- |
| **Create / Rename** | Type text, `Left`/`Right` to move cursor, `Backspace` to delete, `Enter` to confirm, `Esc` to cancel. |
| **Path Jump (`g`)** | Type path (`~`, relative, or absolute), `Enter` to navigate directly, `Esc` to cancel. |
| **Smart Jump (`J`)** | Live filter locations across Git Root, Project, Bookmarks, History, and Tabs. `Up`/`Down` to navigate, `Enter` to jump, `Esc` to close. |
| **Project Cockpit (`P`)** | `Up`/`k` & `Down`/`j` to select action (Go Root, Open Manifest, Open README, Open License, Go Source), `Enter` to execute, `Esc` to close. |
| **Git Status Panel (`G`)** | `Up`/`k` & `Down`/`j` to browse changed repository files, `Enter` to jump to file in pane, `Esc` to close. |
| **File Radar (`F`)** | Inspect visible entries, file/dir counts, bytes, and top file type distribution. `Esc`/`q` to close. |
| **Reveal Context (`C`)** | `Up`/`k` & `Down`/`j` to inspect hierarchical context layers (File → Parent → Project → Git), `Enter` to navigate, `Esc` to close. |
| **Confirm Modal** | `Enter` to execute, `Tab`/`Left`/`Right`/`h`/`l` to toggle selection, `y`/`Y` for Yes, `n`/`N` for No, `Esc` to cancel. |
| **Command Palette** | Filter categorized commands with shortcuts, `Up`/`Down` to navigate list, `Enter` to execute command, `Esc` to close. |
| **Bookmarks Modal** | `Up`/`k` & `Down`/`j` to select bookmark, `Enter` to jump, `d`/`x`/`Delete` to remove bookmark, `q`/`Esc` to close. |

---

## Mouse Controls

| Interaction | Context | Action |
| --- | --- | --- |
| **Left Click** | Entry row | Selects entry and activates pane |
| **Double Left Click** | Directory entry | Navigates into directory |
| **Double Left Click** | File entry | Opens read-only file preview |
| **Left Click** | Tab in pane header | Switches to clicked tab |
| **Left Click** | Pane border / area | Activates clicked pane |
| **Right Click** | Entry row | Selects entry and activates pane |
| **Mouse Wheel Up** | Pane area | Moves item selection up |
| **Mouse Wheel Down** | Pane area | Moves item selection down |

---

## Configuration & Storage

TerminalVision stores bookmarks and configuration in standard platform directories without touching your project trees:
- **macOS / Linux**: `~/.config/terminalvision/bookmarks.json`
- **Windows**: `%APPDATA%\terminalvision\bookmarks.json`

---

## Troubleshooting

- **`terminalvision` command not found**: Ensure your Cargo binary directory (`$HOME/.cargo/bin` or `%USERPROFILE%\.cargo\bin`) is added to your environment `PATH`.
- **Unicode / Box Drawing Characters**: Verify that your terminal emulator is configured with a modern Unicode font (e.g., FiraCode, JetBrains Mono, Hack, Cascadia Code).
- **Small Terminal Window**: TerminalVision dynamically adapts down to minimal sizes; resize your terminal window to 80x24 or larger for optimal dual-pane viewing.
- **Git Status Not Detected**: TerminalVision reads `.git` directories and worktree pointer files directly. Ensure your user account has read permissions for `.git` metadata.

---

## Verification & Testing

```bash
# Run code formatting check
cargo fmt --check

# Run static type and borrow checks
cargo check

# Run complete test suite (unit, integration, responsive, and performance tests)
cargo test

# Run strict Clippy linter
cargo clippy --all-targets --all-features -- -D warnings

# Build release bundle
cargo build --release
```

---

## Security & Safety Architecture

1. **Zero Shell Execution**: No subprocesses (`sh`, `bash`, `cmd.exe`, `cp`, `mv`, `rm`, `git`) are ever spawned. File management and Git parsing run purely in-process using native APIs.
2. **No File Auto-Execution**: Highlighting, selecting, or opening files never executes scripts or binaries. Previews open strictly in read-only mode.
3. **Bounded File Preview**: File preview reads are capped at a maximum 1 MB limit and truncated to line boundaries to prevent out-of-memory errors on massive files.
4. **ANSI Sanitization**: Raw control sequences and terminal escape codes inside file contents are sanitized before rendering to prevent terminal hijacking.
5. **Symlink Depth Protection**: Filesystem traversal enforces maximum recursion depth limits to prevent infinite loops caused by cyclic symlinks.
6. **Non-Destructive Operations**: Copy and move operations perform destination checks to prevent silent file overwrites, prompting explicit user confirmation when conflicts arise.

---

## License

MIT License. See [LICENSE](LICENSE) for details.
