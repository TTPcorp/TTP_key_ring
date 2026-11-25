//! Whether the user's OS session is locked (Win+L on Windows), so the
//! app can lock the vault at the same moment.

/// `Some(true)` if the current session is locked, `Some(false)` if it is
/// not, `None` if it cannot be known (unsupported OS or API failure).
/// Callers must treat `None` as "not locked" and rely on the inactivity
/// timeout instead.
pub fn is_session_locked() -> Option<bool> {
    crate::os::is_session_locked()
}

#[cfg(test)]
mod tests {
    #[test]
    fn query_never_panics() {
        // On a desktop session this is Some(false); on CI or headless
        // machines it may be None. Either way it must not panic.
        let _ = super::is_session_locked();
    }
}
