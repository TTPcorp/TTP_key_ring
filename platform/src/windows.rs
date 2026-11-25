//! Windows implementation of the platform traits.

use arboard::SetExtWindows;

/// Copies text marked with `ExcludeClipboardContentFromMonitorProcessing`:
/// Windows keeps it out of the clipboard history (Win+V) and the cloud
/// clipboard, and other clipboard monitors are asked to ignore it.
pub(crate) fn set_secret_text(
    clipboard: &mut arboard::Clipboard,
    text: &str,
) -> Result<(), arboard::Error> {
    clipboard.set().exclude_from_monitoring().text(text)
}
