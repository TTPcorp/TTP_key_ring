//! Linux implementation of the platform traits (basic; full port in Phase 4).

use arboard::SetExtLinux;

/// Copies text with the `x-kde-passwordManagerHint` hint, which clipboard
/// managers that follow it keep out of their history.
pub(crate) fn set_secret_text(
    clipboard: &mut arboard::Clipboard,
    text: &str,
) -> Result<(), arboard::Error> {
    clipboard.set().exclude_from_history().text(text)
}

/// Not implemented yet on Linux (Phase 4): the inactivity timeout still
/// locks the vault.
pub(crate) fn is_session_locked() -> Option<bool> {
    None
}

use crate::autotype::{ActiveWindow, AutoTypeError, Backend, SpecialKey, WindowId};

/// Auto-type is not implemented yet on Linux (Phase 4).
pub(crate) fn active_window() -> Option<ActiveWindow> {
    None
}

/// Auto-type backend that refuses everything (Phase 4).
pub(crate) struct OsBackend;

impl Backend for OsBackend {
    fn foreground(&self) -> Option<WindowId> {
        None
    }
    fn modifiers_pressed(&self) -> bool {
        false
    }
    fn send_char(&self, _ch: char) -> Result<(), AutoTypeError> {
        Err(AutoTypeError::Unsupported)
    }
    fn send_key(&self, _key: SpecialKey) -> Result<(), AutoTypeError> {
        Err(AutoTypeError::Unsupported)
    }
    fn pause(&self) {}
}

use crate::hotkey::{Hotkey, HotkeyError};

/// Global shortcuts are not implemented yet on Linux (Phase 4). No value
/// of this type can exist.
#[derive(Debug)]
pub(crate) enum HotkeyHandle {}

pub(crate) fn register_hotkey(
    _hotkey: Hotkey,
    _on_press: Box<dyn Fn() + Send>,
) -> Result<HotkeyHandle, HotkeyError> {
    Err(HotkeyError::Unsupported)
}
