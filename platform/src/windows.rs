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

/// Reads the lock state of the current session with
/// `WTSQuerySessionInformationW(WTSSessionInfoEx)`, the documented way
/// to ask Windows 10/11 whether the session is locked.
///
/// This is the only `unsafe` code in the project: Windows exposes this
/// only through its C API and no safe crate wraps it. The exception was
/// approved by the project owner (report 0013) and is limited to this
/// function.
#[allow(unsafe_code)]
pub(crate) fn is_session_locked() -> Option<bool> {
    use windows_sys::Win32::System::RemoteDesktop::{
        WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTS_SESSIONSTATE_LOCK,
        WTS_SESSIONSTATE_UNLOCK, WTSFreeMemory, WTSINFOEXW, WTSQuerySessionInformationW,
        WTSSessionInfoEx,
    };

    let mut buffer: windows_sys::core::PWSTR = std::ptr::null_mut();
    let mut bytes: u32 = 0;
    // SAFETY: both out-pointers are valid, writable locals. On success
    // Windows allocates `buffer`, which is freed below exactly once.
    let ok = unsafe {
        WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut bytes,
        )
    };
    if ok == 0 || buffer.is_null() {
        return None;
    }

    let mut state = None;
    if bytes as usize >= std::mem::size_of::<WTSINFOEXW>() {
        // SAFETY: `buffer` is non-null and holds at least
        // `size_of::<WTSINFOEXW>()` bytes; for `WTSSessionInfoEx` Windows
        // returns a `WTSINFOEXW`. `read_unaligned` makes no alignment
        // assumption, and the struct is plain data (no pointers).
        let info = unsafe { std::ptr::read_unaligned(buffer.cast::<WTSINFOEXW>()) };
        if info.Level == 1 {
            // SAFETY: `Level == 1` means the union holds `WTSInfoExLevel1`
            // (its only variant).
            let flags = unsafe { info.Data.WTSInfoExLevel1.SessionFlags };
            // -1 (WTS_SESSIONSTATE_UNKNOWN) and any other value: unknown.
            state = match u32::try_from(flags) {
                Ok(WTS_SESSIONSTATE_LOCK) => Some(true),
                Ok(WTS_SESSIONSTATE_UNLOCK) => Some(false),
                _ => None,
            };
        }
    }
    // SAFETY: `buffer` was allocated by `WTSQuerySessionInformationW` and
    // is not used after this call.
    unsafe { WTSFreeMemory(buffer.cast()) };
    state
}
