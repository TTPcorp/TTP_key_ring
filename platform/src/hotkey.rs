//! Global keyboard shortcut that starts auto-type.
//!
//! The shortcut works system-wide, whatever window is in the foreground.
//! Registering it returns a [`HotkeyRegistration`]; dropping it removes the
//! shortcut again. The callback receives no data: the app reads the active
//! window itself when the shortcut fires.

use std::fmt;

/// Modifier keys of a shortcut.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

/// A global keyboard shortcut: modifiers plus one letter or digit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    modifiers: Modifiers,
    key: char,
}

impl Hotkey {
    /// Ctrl+Alt+A, the default auto-type shortcut (chosen by the project
    /// owner, 2026-11-26; also the KeePass default). On layouts with AltGr
    /// (such as Spanish), AltGr+A triggers it too; it types no character
    /// there.
    pub const DEFAULT: Hotkey = Hotkey {
        modifiers: Modifiers {
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
        },
        key: 'A',
    };

    /// Builds a shortcut. `key` must be an ASCII letter or digit (letters
    /// are stored in upper case), and Ctrl, Alt or Win must be part of it:
    /// a plain key or Shift+key would swallow ordinary typing.
    pub fn new(modifiers: Modifiers, key: char) -> Result<Self, HotkeyError> {
        if !key.is_ascii_alphanumeric() || !(modifiers.ctrl || modifiers.alt || modifiers.win) {
            return Err(HotkeyError::Invalid);
        }
        Ok(Self {
            modifiers,
            key: key.to_ascii_uppercase(),
        })
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// The key, an upper-case ASCII letter or a digit. On Windows its code
    /// is also the virtual-key code.
    pub fn key(&self) -> char {
        self.key
    }
}

impl fmt::Display for Hotkey {
    /// Human-readable form, such as `Ctrl+Alt+A`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.modifiers;
        for (on, name) in [
            (m.ctrl, "Ctrl"),
            (m.alt, "Alt"),
            (m.shift, "Shift"),
            (m.win, "Win"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", self.key)
    }
}

/// Errors registering a global shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    /// Global shortcuts are not implemented on this OS yet.
    #[error("global shortcuts are not supported on this system")]
    Unsupported,
    /// Not a valid shortcut (see [`Hotkey::new`]).
    #[error("invalid shortcut")]
    Invalid,
    /// Another program (or the system) already uses this shortcut.
    #[error("the shortcut is already in use")]
    AlreadyInUse,
    /// The system refused to register the shortcut for another reason.
    #[error("the shortcut could not be registered")]
    Failed,
}

/// A registered global shortcut. Dropping it unregisters the shortcut and
/// stops its listener thread.
#[derive(Debug)]
pub struct HotkeyRegistration {
    _inner: crate::os::HotkeyHandle,
}

/// Registers `hotkey` system-wide and calls `on_press` each time it is
/// pressed (holding it down does not repeat).
///
/// `on_press` runs on a dedicated listener thread and should return
/// quickly: hand the work to another thread (for example, through a
/// channel). A panic inside it is caught and the shortcut keeps working.
pub fn register_hotkey<F>(hotkey: Hotkey, on_press: F) -> Result<HotkeyRegistration, HotkeyError>
where
    F: Fn() + Send + 'static,
{
    crate::os::register_hotkey(hotkey, Box::new(on_press))
        .map(|inner| HotkeyRegistration { _inner: inner })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL_ALT: Modifiers = Modifiers {
        ctrl: true,
        alt: true,
        shift: false,
        win: false,
    };

    #[test]
    fn default_is_ctrl_alt_a() {
        assert_eq!(Hotkey::DEFAULT.to_string(), "Ctrl+Alt+A");
        assert_eq!(Hotkey::new(CTRL_ALT, 'a'), Ok(Hotkey::DEFAULT));
    }

    #[test]
    fn letters_are_upper_case_and_digits_allowed() {
        let hotkey = Hotkey::new(CTRL_ALT, 'k').unwrap();
        assert_eq!(hotkey.key(), 'K');
        assert_eq!(hotkey.modifiers(), CTRL_ALT);
        assert_eq!(Hotkey::new(CTRL_ALT, '7').unwrap().key(), '7');
    }

    #[test]
    fn display_lists_every_modifier() {
        let all = Modifiers {
            ctrl: true,
            alt: true,
            shift: true,
            win: true,
        };
        assert_eq!(
            Hotkey::new(all, 'z').unwrap().to_string(),
            "Ctrl+Alt+Shift+Win+Z"
        );
        let win = Modifiers {
            win: true,
            ..Modifiers::default()
        };
        assert_eq!(Hotkey::new(win, '1').unwrap().to_string(), "Win+1");
    }

    #[test]
    fn rejects_keys_that_are_not_letters_or_digits() {
        for key in ['ñ', '€', ' ', '+', '\t', 'é'] {
            assert_eq!(Hotkey::new(CTRL_ALT, key), Err(HotkeyError::Invalid));
        }
    }

    #[test]
    fn rejects_shortcuts_that_would_swallow_typing() {
        assert_eq!(
            Hotkey::new(Modifiers::default(), 'A'),
            Err(HotkeyError::Invalid)
        );
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(Hotkey::new(shift, 'A'), Err(HotkeyError::Invalid));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn unsupported_outside_windows() {
        assert_eq!(
            register_hotkey(Hotkey::DEFAULT, || {}).map(|_| ()),
            Err(HotkeyError::Unsupported)
        );
    }

    /// Registers a rarely used shortcut for real, then drops it. Nothing
    /// is pressed. Some environments (such as a CI service session) may
    /// refuse global shortcuts altogether; then only the error is checked.
    #[cfg(target_os = "windows")]
    #[test]
    fn real_registration_is_exclusive_and_released_on_drop() {
        let all = Modifiers {
            ctrl: true,
            alt: true,
            shift: true,
            win: true,
        };
        let hotkey = Hotkey::new(all, '9').unwrap();
        match register_hotkey(hotkey, || {}) {
            Ok(first) => {
                // Taken while registered, even from this same process...
                assert_eq!(
                    register_hotkey(hotkey, || {}).map(|_| ()),
                    Err(HotkeyError::AlreadyInUse)
                );
                // ...and free again once dropped (the drop waits for the
                // listener thread to unregister it).
                drop(first);
                let again = register_hotkey(hotkey, || {});
                assert!(again.is_ok());
            }
            Err(error) => assert!(matches!(
                error,
                HotkeyError::AlreadyInUse | HotkeyError::Failed
            )),
        }
    }
}
