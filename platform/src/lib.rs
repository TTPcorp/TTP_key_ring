//! OS abstraction layer of LLavero Digital.
//!
//! Every OS-specific capability (global hotkeys, keyboard simulation, active
//! window detection, clipboard, Native Messaging registration, OS secure
//! storage) is exposed here as a trait, with one implementation per OS
//! selected via `#[cfg(target_os = "...")]`.

pub mod autotype;
pub mod clipboard;
pub mod hotkey;
pub mod session;

pub use autotype::{
    ActiveWindow, AutoType, AutoTypeError, KEY_DELAY, SpecialKey, SystemAutoType, WindowId,
};
pub use clipboard::{Clipboard, ClipboardError, SystemClipboard, clear_if_unchanged};
pub use hotkey::{Hotkey, HotkeyError, HotkeyRegistration, Modifiers, register_hotkey};
pub use session::is_session_locked;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod os;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod os;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod os;
