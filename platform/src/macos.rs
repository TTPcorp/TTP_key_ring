//! macOS implementation of the platform traits (basic; full port in Phase 4).

use arboard::SetExtApple;

/// Copies text marked as concealed (nspasteboard.org convention), which
/// clipboard history tools that follow it ignore.
pub(crate) fn set_secret_text(
    clipboard: &mut arboard::Clipboard,
    text: &str,
) -> Result<(), arboard::Error> {
    clipboard.set().exclude_from_history().text(text)
}

/// Not implemented yet on macOS (Phase 4): the inactivity timeout still
/// locks the vault.
pub(crate) fn is_session_locked() -> Option<bool> {
    None
}
