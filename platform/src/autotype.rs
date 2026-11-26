//! Auto-type support: reading the active window and typing into it.
//!
//! Every keystroke is guarded: before each character or key, the window
//! in the foreground must still be the one auto-type started with, and no
//! modifier key (Shift, Ctrl, Alt, Win) may be held down. If either check
//! fails, typing stops at once, so a secret never keeps flowing into a
//! window the user switched to, nor gets turned into keyboard shortcuts.

use std::path::PathBuf;
use std::time::Duration;

/// Pause between characters. Some programs drop characters that arrive
/// too fast; 10 ms is the KeePass default and is barely noticeable.
pub const KEY_DELAY: Duration = Duration::from_millis(10);

/// Opaque identifier of a top-level window. Only compared, never used to
/// act on the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(usize);

impl WindowId {
    /// Wraps a raw OS window handle. Public so that tests elsewhere can
    /// simulate windows; it grants nothing, as ids are only compared.
    pub fn from_raw(raw: usize) -> Self {
        Self(raw)
    }
}

/// The window in the foreground.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveWindow {
    /// Identifier, to check that the foreground has not changed.
    pub id: WindowId,
    /// Window title (may be empty).
    pub title: String,
    /// Full path of the program's executable, if the OS allows reading
    /// it (it may not for programs running as administrator).
    pub executable: Option<PathBuf>,
}

/// A special key that auto-type can press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialKey {
    Tab,
    Enter,
}

/// Auto-type errors. No typed text is ever included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AutoTypeError {
    /// Auto-type is not implemented on this OS yet.
    #[error("auto-type is not supported on this system")]
    Unsupported,
    /// Another window came to the foreground; typing was stopped.
    #[error("the target window is no longer in the foreground")]
    WindowChanged,
    /// A modifier key is held down; typing was stopped.
    #[error("a modifier key is pressed")]
    ModifierPressed,
    /// The OS rejected the simulated input (for example, the target runs
    /// with higher privileges than this app).
    #[error("the system blocked the simulated keyboard input")]
    Blocked,
}

/// Auto-type operations used by the app.
pub trait AutoType: Send + Sync {
    /// The window in the foreground, if any.
    fn active_window(&self) -> Option<ActiveWindow>;

    /// Whether Shift, Ctrl, Alt or Win is held down. The app waits for
    /// them to be released (the user is still holding the hotkey) before
    /// typing.
    fn modifiers_pressed(&self) -> bool;

    /// Types `text` into `target`, one character at a time.
    fn type_text(&self, target: WindowId, text: &str) -> Result<(), AutoTypeError>;

    /// Presses and releases `key` in `target`.
    fn press_key(&self, target: WindowId, key: SpecialKey) -> Result<(), AutoTypeError>;
}

/// Low-level OS operations. Kept separate so the guarding logic above
/// them can be tested without sending real keystrokes.
pub(crate) trait Backend {
    fn foreground(&self) -> Option<WindowId>;
    fn modifiers_pressed(&self) -> bool;
    fn send_char(&self, ch: char) -> Result<(), AutoTypeError>;
    fn send_key(&self, key: SpecialKey) -> Result<(), AutoTypeError>;
    fn pause(&self);
}

fn check(backend: &impl Backend, target: WindowId) -> Result<(), AutoTypeError> {
    if backend.foreground() != Some(target) {
        return Err(AutoTypeError::WindowChanged);
    }
    if backend.modifiers_pressed() {
        return Err(AutoTypeError::ModifierPressed);
    }
    Ok(())
}

pub(crate) fn guarded_type_text(
    backend: &impl Backend,
    target: WindowId,
    text: &str,
) -> Result<(), AutoTypeError> {
    for ch in text.chars() {
        check(backend, target)?;
        backend.send_char(ch)?;
        backend.pause();
    }
    Ok(())
}

pub(crate) fn guarded_press_key(
    backend: &impl Backend,
    target: WindowId,
    key: SpecialKey,
) -> Result<(), AutoTypeError> {
    check(backend, target)?;
    backend.send_key(key)?;
    backend.pause();
    Ok(())
}

/// The real keyboard and window system.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemAutoType;

impl SystemAutoType {
    pub fn new() -> Self {
        Self
    }
}

impl AutoType for SystemAutoType {
    fn active_window(&self) -> Option<ActiveWindow> {
        crate::os::active_window()
    }

    fn modifiers_pressed(&self) -> bool {
        crate::os::OsBackend.modifiers_pressed()
    }

    fn type_text(&self, target: WindowId, text: &str) -> Result<(), AutoTypeError> {
        guarded_type_text(&crate::os::OsBackend, target, text)
    }

    fn press_key(&self, target: WindowId, key: SpecialKey) -> Result<(), AutoTypeError> {
        guarded_press_key(&crate::os::OsBackend, target, key)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    #[derive(Debug, PartialEq)]
    enum Event {
        Char(char),
        Key(SpecialKey),
        Pause,
    }

    /// Fake OS: the foreground switches to another window after
    /// `switch_after` sent keystrokes; modifiers are pressed after
    /// `press_after`.
    struct Fake {
        events: RefCell<Vec<Event>>,
        sent: Cell<usize>,
        switch_after: usize,
        press_after: usize,
        block: bool,
    }

    const TARGET: WindowId = WindowId(7);

    impl Fake {
        fn new() -> Self {
            Self {
                events: RefCell::new(Vec::new()),
                sent: Cell::new(0),
                switch_after: usize::MAX,
                press_after: usize::MAX,
                block: false,
            }
        }
        fn typed(&self) -> String {
            self.events
                .borrow()
                .iter()
                .filter_map(|e| match e {
                    Event::Char(c) => Some(*c),
                    _ => None,
                })
                .collect()
        }
        fn record(&self, event: Event) -> Result<(), AutoTypeError> {
            if self.block {
                return Err(AutoTypeError::Blocked);
            }
            self.events.borrow_mut().push(event);
            self.sent.set(self.sent.get() + 1);
            Ok(())
        }
    }

    impl Backend for Fake {
        fn foreground(&self) -> Option<WindowId> {
            Some(if self.sent.get() < self.switch_after {
                TARGET
            } else {
                WindowId(8)
            })
        }
        fn modifiers_pressed(&self) -> bool {
            self.sent.get() >= self.press_after
        }
        fn send_char(&self, ch: char) -> Result<(), AutoTypeError> {
            self.record(Event::Char(ch))
        }
        fn send_key(&self, key: SpecialKey) -> Result<(), AutoTypeError> {
            self.record(Event::Key(key))
        }
        fn pause(&self) {
            self.events.borrow_mut().push(Event::Pause);
        }
    }

    #[test]
    fn types_every_character_with_pauses() {
        let fake = Fake::new();
        guarded_type_text(&fake, TARGET, "añ€😀").unwrap();
        guarded_press_key(&fake, TARGET, SpecialKey::Enter).unwrap();
        assert_eq!(fake.typed(), "añ€😀");
        let events = fake.events.borrow();
        assert_eq!(events.len(), 10, "one pause after each keystroke");
        assert_eq!(events[8], Event::Key(SpecialKey::Enter));
        assert_eq!(events[9], Event::Pause);
    }

    #[test]
    fn stops_when_the_window_changes() {
        let fake = Fake {
            switch_after: 3,
            ..Fake::new()
        };
        let result = guarded_type_text(&fake, TARGET, "secret");
        assert_eq!(result, Err(AutoTypeError::WindowChanged));
        assert_eq!(fake.typed(), "sec");
        assert_eq!(
            guarded_press_key(&fake, TARGET, SpecialKey::Tab),
            Err(AutoTypeError::WindowChanged)
        );
    }

    #[test]
    fn never_types_into_another_window() {
        let fake = Fake::new();
        assert_eq!(
            guarded_type_text(&fake, WindowId(99), "secret"),
            Err(AutoTypeError::WindowChanged)
        );
        assert!(fake.events.borrow().is_empty());
    }

    #[test]
    fn stops_when_a_modifier_is_pressed() {
        let fake = Fake {
            press_after: 2,
            ..Fake::new()
        };
        assert_eq!(
            guarded_type_text(&fake, TARGET, "secret"),
            Err(AutoTypeError::ModifierPressed)
        );
        assert_eq!(fake.typed(), "se");
    }

    #[test]
    fn os_rejection_is_reported() {
        let fake = Fake {
            block: true,
            ..Fake::new()
        };
        assert_eq!(
            guarded_type_text(&fake, TARGET, "x"),
            Err(AutoTypeError::Blocked)
        );
        assert_eq!(
            guarded_press_key(&fake, TARGET, SpecialKey::Tab),
            Err(AutoTypeError::Blocked)
        );
    }

    #[test]
    fn empty_text_sends_nothing() {
        let fake = Fake {
            switch_after: 0,
            ..Fake::new()
        };
        assert_eq!(guarded_type_text(&fake, TARGET, ""), Ok(()));
        assert!(fake.events.borrow().is_empty());
    }

    #[test]
    fn real_queries_never_panic() {
        // Read-only: on CI there may be no foreground window at all.
        // Nothing is typed here.
        let system = SystemAutoType::new();
        let _ = system.active_window();
        let _ = system.modifiers_pressed();
    }
}
