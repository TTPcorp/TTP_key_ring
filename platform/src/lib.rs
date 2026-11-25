//! OS abstraction layer of LLavero Digital.
//!
//! Every OS-specific capability (global hotkeys, keyboard simulation, active
//! window detection, clipboard, Native Messaging registration, OS secure
//! storage) is exposed here as a trait, with one implementation per OS
//! selected via `#[cfg(target_os = "...")]`.

pub mod clipboard;

pub use clipboard::{Clipboard, ClipboardError, SystemClipboard, clear_if_unchanged};

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod os;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod os;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod os;
