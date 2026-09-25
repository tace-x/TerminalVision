//! TerminalVision: a lightweight native terminal file manager written in Rust.
//!
//! The layers live here rather than in the binary, so that everything except
//! taking over the terminal can be built and tested on its own:
//!
//! ```text
//! Input -> Action -> Application -> Domain / Filesystem -> Operating system
//! ```
//!
//! The rendering layer is not connected yet. The binary owns the terminal
//! lifecycle and the application loop, and nothing else.

pub mod app;
pub mod commands;
pub mod config;
pub mod filesystem;
pub mod git;
pub mod input;
pub mod layout;
pub mod preview;
pub mod search;
pub mod terminal;
pub mod ui;
pub mod utils;
