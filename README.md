# ⚡ TerminalVision

> See your filesystem. Understand your project. Control everything without leaving the terminal.

[![Rust](https://img.shields.io/badge/Rust-2024_Edition-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-macOS_%7C_Linux_%7C_Windows-lightgrey.svg?style=flat-square)](https://github.com/tace-x/TerminalVision)
[![Terminal](https://img.shields.io/badge/Terminal-PTY_%26_Ratatui-cyan.svg?style=flat-square)](https://github.com/tace-x/TerminalVision)

**TerminalVision** is a high-performance, native Rust terminal file manager, developer cockpit, and integrated interactive shell environment. It combines side-by-side dual-pane navigation, syntax-highlighted code preview, image inspection, in-process Git analytics, and a **real, live PTY terminal** always visible and ready for keyboard control.

---

## ✨ Features

- 🖥️ **Integrated Real Interactive Terminal**: Bottom panel contains a live Unix pseudo-terminal (PTY) running your system shell (`zsh`, `bash`, `fish`, `sh`). Type commands, run `nano`, `vim`, `python`, `top`, or `ssh` with instant focus toggling (`Ctrl+T` or `F12`).
- 📁 **Dual-Pane File Manager**: Side-by-side directory browsing with independent tabs, active pane indicators, multi-item selection (`Space`, `Ctrl+A`, `*`), and sort modes (Name, Size, Modified Date, Type).
- 🖼️ **Image & Media Inspection**: Instant dimension, bit depth, color model, and aspect ratio decoding for **PNG**, **JPEG**, **GIF**, **BMP**, and **WEBP** images with adaptive canvas preview frames.
- 📖 **Safe Code & Text Preview**: Syntax-highlighted read-only viewer for 20+ programming languages and configuration formats with line numbers, safe byte limits, and ANSI sanitization.
- 🔎 **Live & Recursive Search**: Instant case-insensitive filtering (`/`) with multi-mode cycling (`Tab`): Basic, Recursive background walk, Fuzzy matching, and Deep Recursive Fuzzy.
- 🔄 **Bidirectional Directory Sync**: Synchronize terminal shell working directory to file manager panes (`Action::SyncTerminalToDirectory`) and file manager to shell (`Action::SyncDirectoryToTerminal`), plus live refresh (`Action::RefreshDirectory`).
- 🧠 **Developer Intelligence**:
  - **Project Cockpit (`P`)**: Detects project roots (Cargo, npm, Python, Go, Java), manifests, README, license, and source trees.
  - **Git Status Panel (`G`)**: Live in-process repo inspection showing modified, added, deleted, renamed, and untracked files.
  - **File Radar (`F`)**: Instant directory breakdown with file/dir/symlink/hidden counts, byte sizing, and extension distribution.
  - **Reveal Context (`C`)**: Hierarchical context inspector (`File` → `Directory` → `Project Root` → `Git Root`).
  - **Smart Jump (`J`)**: Unified fuzzy location picker across Git roots, bookmarks, history, and tabs.
- 🎮 **Command Palette (`Ctrl+P`)**: Fuzzy searchable command launcher with keyboard shortcut discovery.
- 🐭 **Full Mouse Support**: Left-click to select rows and tabs, double-click to navigate directories or preview files, right-click to inspect, mouse-wheel scrolling, and click-to-focus on the terminal panel.
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
- **Default Shell**: Spawns your `$SHELL` (or `/bin/zsh`, `/bin/bash`, `/bin/sh`).
- **Interactive Applications**: Supports `nano`, `vim`, `python`, `top`, `git`, `ssh`, and full TUI tools inside the PTY.
- **Dynamic Resize**: Resizing the window automatically computes new columns and rows and sends `TIOCSWINSZ` to the child shell.
- **Terminal Scrollback**: Bounded scrollback buffer (up to 2,000 lines) with dedicated keyboard and mouse-wheel scrolling (`Action::ScrollTerminalUp` / `ScrollTerminalDown`).
- **Focus Separation**: Press `Ctrl+T` or `F12` (or click inside the terminal panel) to focus the terminal for direct typing. Press `Ctrl+T` or click on any file pane to return to file manager navigation.

---

## 🖼️ Image Preview

TerminalVision inspects binary headers to decode image metadata and dimensions without external dependencies:

| Format | Extensions | Detected Metadata |
| --- | --- | --- |
| **PNG** | `.png` | Width, Height, Bit Depth, Color Type (RGBA/RGB/Grayscale), Aspect Ratio |
| **JPEG** | `.jpg`, `.jpeg` | Width, Height, Precision, Channels (sRGB / YCbCr / CMYK), Aspect Ratio |
| **GIF** | `.gif` | Width, Height, Version (`GIF87a` / `GIF89a`), Aspect Ratio |
| **BMP** | `.bmp` | Width, Height, Planes, Bits Per Pixel (24-bit / 32-bit), Aspect Ratio |
| **WEBP** | `.webp` | Width, Height, VP8 format (Simple / Extended canvas), Aspect Ratio |

*Fallback*: If the terminal emulator does not support inline graphics protocols (Kitty/Sixel), TerminalVision renders a clean, styled technical specification card with canvas dimensions and aspect ratio framing.

---

## ⚡ Installation & Building

### Prerequisites
- [Rust Toolchain](https://www.rust-lang.org/) (Stable compiler targeting Rust 2024 edition, 1.85+)
- Cargo package manager

### 1. Clone Repository
```bash
git clone https://github.com/tace-x/TerminalVision.git
cd TerminalVision
```

### 2. Build Release Binary
```bash
cargo build --release
```
The compiled binary will be at `target/release/TerminalVision` (or `terminalvision.exe` on Windows).

### 3. Install to Cargo Path
```bash
cargo install --path .
```
Ensure `~/.cargo/bin` (or `%USERPROFILE%\.cargo\bin`) is in your system `PATH`.

### 4. Launch TerminalVision
```bash
# Launch in current working directory
terminalvision

# Launch in a specific directory
terminalvision ~/Projects/TerminalVision

# Launch targeting a specific file (opens parent directory and selects the file)
terminalvision src/main.rs

# Display command line flags
terminalvision --help
terminalvision --version
```

### 5. Updating
```bash
git pull origin main
cargo install --path . --force
```

### 6. Uninstalling
```bash
cargo uninstall terminalvision
```

---

## 🎮 Controls & Shortcuts

### Focus Management
| Shortcut | Action | Description |
| --- | --- | --- |
| `Ctrl+T` / `F12` | Toggle Focus | Switch focus between File Manager and Embedded Terminal |
| `Left Click` | Focus Click | Click file pane or terminal panel to switch focus |

### File Manager (Normal Mode)
| Shortcut | Action | Description |
| --- | --- | --- |
| `Up` / `k` | Move Up | Move selection up |
| `Down` / `j` | Move Down | Move selection down |
| `Left` / `h` | Move Left | Navigate left / select |
| `Right` / `l` | Move Right | Navigate right / select |
| `Enter` | Open | Open selected directory (never executes files) |
| `Backspace` | Parent Dir | Navigate to parent directory |
| `Home` / `End` | First / Last | Jump to first / last entry |
| `PageUp` / `PageDown` | Page Scroll | Scroll by visible page |
| `Tab` | Switch Pane | Toggle active pane (Left / Right) |
| `n` / `N` | New File / Dir | Create file (`n`) or directory (`N` / `Shift+N`) |
| `r` | Rename | Rename selected entry |
| `y` / `x` / `p` | Copy / Cut / Paste | Clipboard operations |
| `d` | Delete | Delete entry with confirmation |
| `Space` | Toggle Select | Multi-select item |
| `Ctrl+A` / `u` / `*` | Select All / Clear / Invert | Batch selection |
| `.` | Toggle Hidden | Show/hide hidden dotfiles |
| `s` | Change Sort | Cycle sort (Name → Size → Modified → Type) |
| `v` | Preview | Open read-only preview overlay |
| `Z` / `Ctrl+F` | Focus Mode | Full-width single pane mode |
| `g` / `J` | Jump / Smart Jump | Jump to path (`g`) or unified location picker (`J`) |
| `P` | Project Cockpit | Developer project intelligence overview |
| `G` | Git Status | In-process Git repository changes |
| `F` | File Radar | Directory statistics and size metrics |
| `C` | Reveal Context | Context hierarchy inspector |
| `b` / `B` | Bookmarks | Add bookmark (`b`) or open bookmarks (`B`) |
| `t` / `w` / `[` / `]` | Tabs | New tab (`t`), close (`w`), prev (`[`), next (`]`) |
| `/` | Search | Live search filter (`Tab` to cycle search modes) |
| `Ctrl+P` | Command Palette | Discover and execute commands |
| `?` | Help | Interactive cheat-sheet |
| `q` / `Ctrl+C` | Quit | Exit TerminalVision cleanly |

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
| **Left Click** | File Pane Row | Selects entry and activates pane |
| **Double Click** | Directory Row | Navigates into directory |
| **Double Click** | File Row | Opens file preview |
| **Left Click** | Tab Header | Switches to clicked tab |
| **Left Click** | Terminal Panel | Focuses interactive terminal |
| **Mouse Wheel** | File Pane | Scrolls file list |
| **Mouse Wheel** | Terminal Panel | Scrolls terminal scrollback |

---

## 🔒 Security & Architecture

1. **Zero Shell Injection in File Manager**: File operations (`create`, `rename`, `copy`, `delete`) execute exclusively via safe Rust standard library `std::fs` calls.
2. **Explicit Terminal Execution Only**: Shell commands execute strictly through the PTY when explicitly typed by the user in the focused terminal panel.
3. **No Background Execution**: Selecting, highlighting, or previewing files never executes scripts or binaries.
4. **ANSI Sanitization**: Preview text sanitizes escape codes to prevent terminal control exploits.
5. **Bounded Resource Limits**: Previews enforce strict file size caps (max 1 MB) and line length limits.

---

## 🛠️ Development & Quality Assurance

```bash
# Code formatting
cargo fmt --check

# Type checking
cargo check

# Complete test suite (790+ unit, integration, and fuzz tests)
cargo test

# Zero-warning Clippy verification
cargo clippy --all-targets --all-features -- -D warnings

# Optimized release build
cargo build --release
```

---

## 📄 License

MIT License. Designed and maintained by [tace-x](https://github.com/tace-x).
