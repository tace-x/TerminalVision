# ⚡ TerminalVision

> See your filesystem. Understand your project. Control everything without leaving the terminal.

[![Rust](https://img.shields.io/badge/Rust-2024_Edition-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-macOS_%7C_Linux_%7C_Windows-lightgrey.svg?style=flat-square)](https://github.com/tace-x/TerminalVision)
[![Terminal](https://img.shields.io/badge/Terminal-PTY_%26_Ratatui-cyan.svg?style=flat-square)](https://github.com/tace-x/TerminalVision)

**TerminalVision** is a high-performance, native Rust terminal file manager, developer cockpit, and integrated interactive shell environment. It combines side-by-side dual-pane navigation, syntax-highlighted code preview, image and media inspection, in-process Git analytics, an asynchronous storage analyzer, and a **real, live PTY terminal** always visible and ready for seamless keyboard and mouse control.

---

## ✨ Features

- 🚀 **VISION BOOT**: Cinematic yet purposeful initialization sequence (Wake → Identity → Readiness → Project Awareness → Interface Construction → Pulse → Ready) with instant `Esc` / click skipping and configurable motion modes (`Full`, `Reduced`, `Off`).
- 🖱️ **Interactive Context Menu**: Context-aware right-click and `Shift+F10` modal menu with live action execution, keyboard navigation, type-to-select filtering, submenus, and shortcut keybinding badges synchronized with the Universal Action Registry.
- 🖥️ **Integrated Real Interactive Terminal**: Bottom panel contains a live Unix pseudo-terminal (PTY) running your system shell (`zsh`, `bash`, `fish`, `sh`). Type commands, run `nano`, `vim`, `python`, `top`, or `ssh` with instant focus toggling (`Ctrl+T` or `F12`).
- 📁 **Dual-Pane File Manager**: Side-by-side directory browsing with independent tabs, active pane indicators, multi-item selection (`Space`, `Shift+Up`/`Down`, `⌘A`/`Ctrl+A`, `*`), and sort modes (Name, Size, Modified Date, Type).
- 🧭 **Interactive Smart Breadcrumb**: Top-level directory path with clickable hierarchy segments and intelligent responsive truncation.
- ⚡ **Command Center & Quick Switcher**: Unified fuzzy search for all application actions (`⌘K`/`Ctrl+K`) and instantaneous file/folder switching (`⌘P`/`Ctrl+P`).
- 🖼️ **Universal Quick Preview & Image Inspection**: Instant dimension, bit depth, color model, and aspect ratio decoding for **PNG**, **JPEG**, **GIF**, **BMP**, and **WEBP** images with graphics protocol support (Kitty, iTerm2, Sixel) and technical card fallback.
- 📖 **Safe Code & Text Preview**: Syntax-highlighted read-only viewer for 20+ programming languages, Markdown formatting, JSON trees, PDF structural inspection, and ZIP archive contents with safe bounded byte limits.
- 🗂️ **Smart File Operations**: Background copy, move, and delete with chunked data transfer, non-blocking UI, real progress tracking, and conflict resolution (Replace, Skip, Auto-Rename).
- 📊 **Storage Vision**: Asynchronous directory size scanner, disk usage heatmap bars, top largest files analyzer, drill-down navigation, and instant cancellation safety (`⌘S`/`Ctrl+S`).
- 🎨 **Dynamic Theme Engine**: 10 built-in color themes with live preview, settings persistence, and terminal ANSI palette synchronization.
- 🔎 **Live & Recursive Search**: Instant case-insensitive filtering (`/`) with multi-mode cycling (`Tab`): Basic, Recursive background walk, Fuzzy matching, and Deep Recursive Fuzzy.
- 🔄 **Bidirectional Directory Sync**: Synchronize terminal shell working directory to file manager panes (`pwd` ↔ file manager) and file manager navigation to shell (`cd`), with live refresh (`⌘R`/`Ctrl+R` or `F5`).
- 🧠 **Project Intelligence & Developer Cockpit**:
  - **Project Cockpit (`P`)**: Detects project roots (Cargo, npm, Python, Go, Java), manifests, README, license, and source trees.
  - **Git Status Panel (`G`)**: Live in-process repo inspection showing modified, added, deleted, renamed, and untracked files.
  - **File Radar (`F`)**: Instant directory breakdown with file/dir/symlink/hidden counts, byte sizing, and extension distribution.
  - **Reveal Context (`C`)**: Hierarchical context inspector (`File` → `Directory` → `Project Root` → `Git Root`).
  - **Favorites (`⌘D`/`Ctrl+D`)**: Fast persistent pinning of key directories with broken-link detection.
- 🐭 **Full Mouse & Keyboard Parity**: Left-click to select rows and tabs, double-click to navigate directories or preview files, right-click context menu, mouse-wheel scrolling, and click-to-focus on the terminal panel.
- 🔒 **Zero Unintended Execution**: Selecting or opening files in the file manager never executes arbitrary code or scripts. Commands only execute when you explicitly type them into the embedded terminal.

---

## 🖥️ Integrated Interactive Terminal

TerminalVision features a permanent, responsive bottom terminal panel backed by a true Unix PTY (`libc::openpty`) and ANSI terminal emulator:

```
┌────────────────────────────────────────────────────────────┐
│ TerminalVision v0.1.0                     [NORMAL] [LEFT]  │
├─────────────────────┬───────────────────┬──────────────────┤
│ LEFT PANE           │ RIGHT PANE        │ PREVIEW          │
│ > src/              │   Cargo.toml      │ fn main() {      │
│   docs/             │   README.md       │     run()?;      │
│   tests/            │   target/         │ }                │
├─────────────────────┴───────────────────┴──────────────────┤
│ ⚡ TERMINAL (zsh: ~/TerminalVision)        [Ctrl+T to focus]│
│ user@Mac ~/TerminalVision % ls -la                         │
│ drwxr-xr-x  src                                            │
│ -rw-r--r--  Cargo.toml                                     │
│ user@Mac ~/TerminalVision % echo "Hello TerminalVision!"   │
│ Hello TerminalVision!                                      │
├────────────────────────────────────────────────────────────┤
│ NORMAL │ LEFT │ 1 / 3 │ ~/TerminalVision                   │
└────────────────────────────────────────────────────────────┘
```

### Supported Shell & Interactive Capabilities:
- **Default Shell**: Spawns your `$SHELL` (or `/bin/zsh`, `/bin/bash`, `/bin/fish`, `/bin/sh`).
- **Interactive Applications**: Runs `nano`, `vim`, `python`, `top`, `git`, `ssh`, and full TUI tools inside the PTY.
- **Dynamic Resize**: Resizing the window automatically computes new columns and rows and sends `TIOCSWINSZ` to the child shell.
- **Terminal Scrollback**: Bounded scrollback buffer (up to 2,000 lines) with dedicated keyboard and mouse-wheel scrolling (`ScrollTerminalUp` / `ScrollTerminalDown`).
- **Focus Separation**: Press `Ctrl+T` or `F12` (or click inside the terminal panel) to focus the terminal for direct typing. Press `Ctrl+T` or click on any file pane to return to file manager navigation.
- **CWD Synchronization**: Terminal `pwd` matches the active pane; changing directories in the file manager or shell keeps both sides synchronized.

---

## 🚀 VISION BOOT Startup & Motion Engine

TerminalVision launches with **VISION BOOT**, a deterministic, purposeful initialization sequence:

1. **Wake**: Terminal detection, ANSI palette negotiation, and alternate screen initialization.
2. **Identity**: Product brand typography and version confirmation.
3. **Readiness**: Subsystem verification (filesystem access, PTY allocation, settings store).
4. **Project Awareness**: Bounded, read-only fingerprinting of the active workspace (Rust, Node, Python, Java, Go, C/C++, Git).
5. **Interface Construction**: Dual-pane grid, breadcrumbs, status bars, and preview panel layout geometry.
6. **Vision Pulse**: Subtle accent sweep signaling interactive readiness.
7. **Ready**: Seamless handoff to active file manager navigation.

### Motion Control:
- **Instant Skip**: Press `Esc` or click anywhere to bypass the boot sequence immediately.
- **Motion Modes**:
  - `Full`: Smooth eased transitions, pulse feedback, and animated micro-interactions.
  - `Reduced`: Snappy instantaneous transitions with clean highlight indicators.
  - `Off`: Zero animation frames for pure instant TUI rendering.
- Toggle motion modes via settings or the Command Center (`⌘K` / `Ctrl+K`).

---

## 🖱️ Interactive Context Menu

The native Context Menu provides complete mouse and keyboard parity:

```
┌───────────────────────────────────────┐
│ ⚡ Context Menu (2 items selected)    │
├───────────────────────────────────────┤
│ 📋 Copy                     ⌘C / Ctrl+C│
│ ✂️  Cut                      ⌘X / Ctrl+X│
│ 🗑️  Delete                     Delete │
│ 🏷️  Rename                         F2 │
│ ───────────────────────────────────── │
│ 📦 Open in Terminal            Ctrl+T │
│ 🔍 Preview                          v │
│ 🗂️  Reveal in Project               P │
│ ▶ More Options                      ▶ │
└───────────────────────────────────────┘
```

- **Triggering**: Right-click any file/folder or press `Shift+F10` (or `Menu` key).
- **Selection Awareness**: Adapts options dynamically for single files, directories, or multi-selected items.
- **Type-to-Select**: Type characters while the menu is open to instantly jump to matching menu items.
- **Submenus**: Navigate into submenus with `Right Arrow` / hover, and return with `Left Arrow`.
- **Dismissal**: Press `Esc` or click outside the menu to dismiss and restore previous focus.
- **Authoritative Keybindings**: Shortcut labels are automatically formatted from the Universal Action Registry.

---

## 🖼️ Universal Quick Preview & Image Inspection

TerminalVision inspects binary headers to decode image metadata and dimensions without external dependencies:

| Format | Extensions | Detected Metadata |
| --- | --- | --- |
| **PNG** | `.png` | Width, Height, Bit Depth, Color Type (RGBA/RGB/Grayscale), Aspect Ratio |
| **JPEG** | `.jpg`, `.jpeg` | Width, Height, Precision, Channels (sRGB / YCbCr / CMYK), Aspect Ratio |
| **GIF** | `.gif` | Width, Height, Version (`GIF87a` / `GIF89a`), Aspect Ratio |
| **BMP** | `.bmp` | Width, Height, Planes, Bits Per Pixel (24-bit / 32-bit), Aspect Ratio |
| **WEBP** | `.webp` | Width, Height, VP8 format (Simple / Extended canvas), Aspect Ratio |
| **PDF** | `.pdf` | Version, Object Count, Page Count, Linearization status |
| **ZIP** | `.zip`, `.jar`, `.tar` | Archive entry count, compressed/uncompressed sizes, inner file tree |
| **Code/Text** | `.rs`, `.py`, `.js`, `.json`, `.md` | Syntax highlighting, line numbers, word count, encoding |

*Image Protocols*: Where supported (Kitty, iTerm2, Sixel), graphic rendering protocols are used. In text terminals, TerminalVision displays an informative technical specification card with canvas dimensions and aspect ratio framing.

---

## 🎨 Dynamic Theme Engine

TerminalVision includes 10 built-in, hand-crafted themes accessible via the Theme Selector (`F5` or via Command Center):

1. **TerminalVision (Default)**: Signature technical identity with cyan, purple, and electric blue accents.
2. **Midnight**: Deep obsidian dark background with restrained cool slate accents.
3. **Cyberpunk**: High-energy neon yellow, cyan, and hot magenta palette.
4. **Ocean**: Submerged deep oceanic blues and crisp teal highlights.
5. **Dracula**: Classic gothic dark with purple, pink, and vibrant green accents.
6. **Nord**: Arctic cool blue, frost, and aurora borealis palette.
7. **Matrix**: Retro CRT phosphor green and deep pitch black matrix.
8. **Solarized Dark**: Precision engineered Solarized dark color structure.
9. **Monochrome**: Minimalist pure grayscale with high typography focus.
10. **High Contrast**: WCAG AAA compliant high-distinction accessible palette.

Themes persist across sessions in `~/.config/terminalvision/settings.json` and synchronize ANSI palettes to the embedded terminal.

---

## ⚡ Installation & Building

### Requirements
- **Rust Toolchain**: Stable compiler targeting Rust 2024 edition (Rust 1.85+)
- **Cargo**: Rust package manager
- **OS**: macOS, Linux, or Windows (with Unix PTY emulation / ConPTY support)

### 1. Clone Repository
```bash
git clone https://github.com/tace-x/TerminalVision.git
cd TerminalVision
```

### 2. Build from Source
```bash
cargo build --release
```
The compiled binary will be located at `target/release/TerminalVision` (or `terminalvision.exe` on Windows).

### 3. Install to Cargo Path
```bash
cargo install --path .
```
Ensure `~/.cargo/bin` (or `%USERPROFILE%\.cargo\bin` on Windows) is included in your system `PATH`.

### 4. Launch TerminalVision
```bash
# Launch in current working directory
terminalvision

# Launch in a specific directory
terminalvision ~/Projects/TerminalVision

# Launch targeting a specific file (opens parent directory and highlights the file)
terminalvision src/main.rs

# Display command-line options and version
terminalvision --help
terminalvision --version
```

### 5. Updating
```bash
cd <path-to-TerminalVision>
git pull origin main
cargo install --path . --force
terminalvision
```

### 6. Uninstalling
```bash
cargo uninstall TerminalVision
```

---

## 🎮 Controls & Shortcuts

The Universal Action Registry ensures consistent shortcuts across all platforms. macOS uses `⌘` (Command), while Windows and Linux use `Ctrl`.

### Focus Management
| macOS | Windows / Linux | Action | Description |
| --- | --- | --- | --- |
| `Ctrl+T` / `F12` | `Ctrl+T` / `F12` | Toggle Focus | Switch focus between File Manager and Embedded Terminal |
| `Left Click` | `Left Click` | Focus Click | Click file pane or terminal panel to switch focus |

### File Manager (Normal Mode)
| macOS | Windows / Linux | Action | Description |
| --- | --- | --- | --- |
| `Up` / `k` | `Up` / `k` | Move Up | Move selection up |
| `Down` / `j` | `Down` / `j` | Move Down | Move selection down |
| `Left` / `h` | `Left` / `h` | Move Left | Navigate left / parent |
| `Right` / `l` | `Right` / `l` | Move Right | Navigate right / open |
| `Enter` | `Enter` | Open | Open selected directory (never executes files) |
| `Backspace` | `Backspace` | Parent Dir | Navigate to parent directory |
| `⌥Left` | `Alt+Left` | History Back | Navigate back in directory history |
| `⌥Right` | `Alt+Right` | History Forward | Navigate forward in directory history |
| `Home` / `End` | `Home` / `End` | First / Last | Jump to first / last entry |
| `PageUp` / `PageDown` | `PageUp` / `PageDown` | Page Scroll | Scroll by visible page |
| `Tab` | `Tab` | Switch Pane | Toggle active pane (Left / Right) |
| `Shift+Tab` | `Shift+Tab` | Switch Pane Back | Toggle active pane in reverse |
| `n` | `n` | New File | Create a new file |
| `⌘⇧N` / `Shift+N` | `Ctrl+Shift+N` / `Shift+N` | New Directory | Create a new directory |
| `F2` / `r` | `F2` / `r` | Rename | Rename selected entry |
| `⌘C` / `y` | `Ctrl+C` / `y` | Copy | Copy selected entries to clipboard |
| `⌘X` / `x` | `Ctrl+X` / `x` | Cut | Cut selected entries to clipboard |
| `⌘V` / `p` | `Ctrl+V` / `p` | Paste | Paste clipboard entries into directory |
| `Delete` / `d` | `Delete` / `d` | Delete | Delete entry with confirmation |
| `Space` | `Space` | Toggle Select / Preview | Multi-select item or toggle quick preview |
| `Shift+Up` / `Shift+Down` | `Shift+Up` / `Shift+Down` | Range Selection | Extend multi-selection upward / downward |
| `⌘A` | `Ctrl+A` | Select All | Select all entries in active pane |
| `u` | `u` | Deselect All | Clear multi-selection |
| `*` | `*` | Invert Selection | Invert selected entries in pane |
| `.` | `.` | Toggle Hidden | Show/hide hidden dotfiles |
| `s` | `s` | Change Sort | Cycle sort (Name → Size → Modified → Type) |
| `v` | `v` | Quick Preview | Open read-only preview overlay |
| `z` / `Z` | `z` / `Z` | Focus Mode | Full-width single pane mode |
| `⌘L` / `g` | `Ctrl+L` / `g` | Jump to Path | Direct path input dialog |
| `⌘P` / `Shift+J` | `Ctrl+P` / `Shift+J` | Quick Switcher / Smart Jump | Fuzzy file & location switcher |
| `⌘K` | `Ctrl+K` | Command Center | Searchable action and command launcher |
| `⌘D` / `b` | `Ctrl+D` / `b` | Favorite / Bookmark | Add current location to favorites |
| `⌘B` / `Shift+B` | `Ctrl+B` / `Shift+B` | Open Favorites | Open saved favorites manager |
| `⌘S` | `Ctrl+S` | Storage Vision | Interactive storage analyzer & heatmap |
| `⌘R` / `F5` | `Ctrl+R` / `F5` | Refresh | Reload directory and Git state |
| `Shift+F10` | `Shift+F10` | Context Menu | Open context menu for active selection |
| `⌘I` | `Ctrl+I` | Get Info | Detailed metadata properties modal |
| `P` | `P` | Project Cockpit | Developer project overview |
| `G` | `G` | Git Status | In-process Git repository changes |
| `F` | `F` | File Radar | Directory statistics and size metrics |
| `C` | `C` | Reveal Context | Context hierarchy inspector |
| `t` / `w` / `[` / `]` | `t` / `w` / `[` / `]` | Tabs | New tab (`t`), close (`w`), prev (`[`), next (`]`) |
| `/` | `/` | Search | Live search filter (`Tab` to cycle modes) |
| `?` | `?` | Show Shortcuts | Interactive cheatsheet generated from registry |
| `q` | `q` | Quit | Exit TerminalVision cleanly |

### Terminal Mode (When Terminal is Focused)
| Shortcut | Action | Description |
| --- | --- | --- |
| `Printable Keys` | Shell Input | Characters sent directly to PTY shell process |
| `Enter` | Execute | Sends carriage return / newline to shell |
| `Backspace` | Delete | Sends `0x7F` backspace to shell |
| `Arrow Keys` | Cursor / History | Navigation in shell and interactive programs |
| `Ctrl+C` | SIGINT | Sent to foreground process in shell |
| `Ctrl+D` | EOF | Sent to shell |
| `Ctrl+T` / `F12` | Toggle Focus | Return focus to File Manager |
| `Mouse Wheel` | Terminal Scroll | Scroll terminal scrollback history |

### Mouse Controls
| Interaction | Context | Action |
| --- | --- | --- |
| **Left Click** | File Row | Selects entry and activates pane |
| **Double Click** | Directory Row | Navigates into directory |
| **Double Click** | File Row | Opens file preview |
| **Right Click** | File / Directory Row | Opens contextual action menu |
| **Left Click** | Breadcrumb Segment | Navigates directly to clicked directory level |
| **Left Click** | Tab Header | Switches to clicked tab |
| **Left Click** | Header Search Button | Opens Command Center (`⌘K`/`Ctrl+K`) |
| **Left Click** | Terminal Panel | Focuses interactive terminal |
| **Mouse Wheel** | File Pane | Scrolls file list |
| **Mouse Wheel** | Terminal Panel | Scrolls terminal scrollback |

---

## 🖥️ Supported Terminals

TerminalVision runs in any modern terminal emulator supporting ANSI escape codes and raw mode:
- **macOS**: Ghostty, WezTerm, Alacritty, iTerm2, Kitty, Apple Terminal.
- **Linux**: Ghostty, WezTerm, Alacritty, Kitty, GNOME Terminal, Konsole, Foot, XFCE Terminal.
- **Windows**: Windows Terminal, WezTerm, Alacritty.

---

## 🔧 Troubleshooting

### 1. `command not found: terminalvision` / PATH not configured
- **Solution**: Ensure Cargo's binary directory is in your `PATH`:
  - **macOS / Linux**: Add `export PATH="$HOME/.cargo/bin:$PATH"` to `~/.zshrc` or `~/.bashrc`.
  - **Windows**: Add `%USERPROFILE%\.cargo\bin` to your User Environment `PATH`.

### 2. Limited Color Support / Colors look washed out
- **Solution**: Ensure your terminal declares truecolor support:
  ```bash
  export COLORTERM=truecolor
  export TERM=xterm-256color
  ```

### 3. Image Preview Unavailable
- **Solution**: When running in a terminal emulator without Kitty/Sixel graphics protocol support (e.g., standard Apple Terminal), TerminalVision automatically renders a detailed technical metadata card with canvas dimensions and aspect ratio framing. For graphical rendering, use Ghostty, Kitty, WezTerm, or iTerm2.

### 4. Shell Detection Issues
- **Solution**: TerminalVision inspects your `$SHELL` environment variable. If undefined, it defaults to `/bin/zsh` on macOS and `/bin/sh` on Linux. You can customize the active shell by setting `export SHELL=/path/to/shell` before launching.

### 5. Permission Denied Errors
- **Solution**: If inspecting system directories with restricted permissions (e.g., `/root`, `/private/var`), TerminalVision displays a non-fatal warning badge in the status bar. It will never crash on inaccessible paths.

### 6. Terminal Scrolling / Focus
- **Solution**: Press `Ctrl+T` or `F12` to toggle focus between the File Manager and the Embedded Terminal. When the terminal is focused, keystrokes are routed directly to the child shell.

---

## 🔒 Security & Architecture

1. **Zero Shell Injection in File Manager**: File operations (`create`, `rename`, `copy`, `delete`) execute exclusively via safe Rust standard library `std::fs` calls.
2. **Explicit Terminal Execution Only**: Shell commands execute strictly through the PTY when explicitly typed by the user in the focused terminal panel.
3. **No Background Execution**: Selecting, highlighting, or previewing files never executes scripts or binaries.
4. **ANSI Sanitization**: Preview text sanitizes escape codes to prevent terminal control exploits.
5. **Bounded Resource Limits**: Previews enforce strict file size caps and bounded line lengths to eliminate UI freezes.
6. **Graceful Panic & Signal Recovery**: TerminalVision installs a custom panic hook that restores raw mode, leaves alternate screens, and unhides the cursor cleanly.

---

## 🛠️ Development & Quality Assurance

```bash
# Code formatting check
cargo fmt --check

# Type checking
cargo check

# Complete test suite (800+ unit, integration, and fuzz tests)
cargo test

# Zero-warning Clippy verification
cargo clippy --all-targets --all-features -- -D warnings

# Optimized release build
cargo build --release
```

---

## 📄 License

MIT License. Designed and maintained by [tace-x](https://github.com/tace-x).
