//! System clipboard for copying user names and passwords.
//!
//! Secrets are copied with the OS hints that keep them out of clipboard
//! history and cloud sync where the OS supports it (see the per-OS
//! modules), and are cleared after a delay by the caller using
//! [`clear_if_unchanged`], which never wipes something the user copied
//! afterwards.

use std::sync::Mutex;

use zeroize::Zeroizing;

/// Clipboard errors. No clipboard content is ever included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardError {
    /// The clipboard could not be opened or is in use by another program.
    #[error("the clipboard is not available")]
    Unavailable,
}

/// Clipboard operations used by the app.
pub trait Clipboard: Send + Sync {
    /// Puts `text` on the clipboard, hidden from clipboard history and
    /// cloud sync where the OS allows.
    fn set_secret(&self, text: &str) -> Result<(), ClipboardError>;

    /// Current text on the clipboard, if any.
    fn get_text(&self) -> Result<Option<Zeroizing<String>>, ClipboardError>;

    /// Empties the clipboard.
    fn clear(&self) -> Result<(), ClipboardError>;
}

/// Clears the clipboard only if it still holds `expected`, i.e. the user
/// has not copied anything else since. Returns whether it was cleared.
pub fn clear_if_unchanged(
    clipboard: &dyn Clipboard,
    expected: &str,
) -> Result<bool, ClipboardError> {
    match clipboard.get_text()? {
        Some(current) if current.as_str() == expected => {
            clipboard.clear()?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

/// The real system clipboard.
///
/// The handle is kept open: on Linux (X11) the copied text is served by
/// it and would vanish if it were dropped.
#[derive(Default)]
pub struct SystemClipboard {
    handle: Mutex<Option<arboard::Clipboard>>,
}

impl SystemClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<T>(
        &self,
        f: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
    ) -> Result<T, ClipboardError> {
        let mut guard = self
            .handle
            .lock()
            .map_err(|_| ClipboardError::Unavailable)?;
        if guard.is_none() {
            *guard = Some(arboard::Clipboard::new().map_err(|_| ClipboardError::Unavailable)?);
        }
        let clipboard = guard.as_mut().ok_or(ClipboardError::Unavailable)?;
        f(clipboard).map_err(|_| ClipboardError::Unavailable)
    }
}

impl Clipboard for SystemClipboard {
    fn set_secret(&self, text: &str) -> Result<(), ClipboardError> {
        self.with(|c| crate::os::set_secret_text(c, text))
    }

    fn get_text(&self) -> Result<Option<Zeroizing<String>>, ClipboardError> {
        self.with(|c| match c.get_text() {
            Ok(text) => Ok(Some(Zeroizing::new(text))),
            // Empty clipboard or non-text content (an image, files...).
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(e) => Err(e),
        })
    }

    fn clear(&self) -> Result<(), ClipboardError> {
        self.with(|c| c.clear())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory clipboard for tests.
    #[derive(Default)]
    pub struct FakeClipboard {
        pub content: Mutex<Option<String>>,
    }

    impl Clipboard for FakeClipboard {
        fn set_secret(&self, text: &str) -> Result<(), ClipboardError> {
            *self.content.lock().unwrap() = Some(text.to_owned());
            Ok(())
        }
        fn get_text(&self) -> Result<Option<Zeroizing<String>>, ClipboardError> {
            Ok(self.content.lock().unwrap().clone().map(Zeroizing::new))
        }
        fn clear(&self) -> Result<(), ClipboardError> {
            *self.content.lock().unwrap() = None;
            Ok(())
        }
    }

    #[test]
    fn clears_only_its_own_content() {
        let c = FakeClipboard::default();
        c.set_secret("secret-1").unwrap();
        assert!(clear_if_unchanged(&c, "secret-1").unwrap());
        assert_eq!(c.get_text().unwrap(), None);

        c.set_secret("secret-1").unwrap();
        // The user copied something else meanwhile: leave it alone.
        c.set_secret("user text").unwrap();
        assert!(!clear_if_unchanged(&c, "secret-1").unwrap());
        assert_eq!(
            c.get_text().unwrap().as_deref().map(String::as_str),
            Some("user text")
        );

        c.clear().unwrap();
        assert!(!clear_if_unchanged(&c, "secret-1").unwrap());
    }

    #[test]
    fn errors_carry_no_content() {
        assert_eq!(
            ClipboardError::Unavailable.to_string(),
            "the clipboard is not available"
        );
    }
}
