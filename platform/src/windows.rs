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
/// Windows exposes this only through its C API and no safe crate wraps
/// it. The `unsafe` exception was approved by the project owner (report
/// 0013) and is limited to this function.
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

// ---------------------------------------------------------------------
// Auto-type (Phase 2).
//
// These functions call the Windows C API through `windows-sys`. Each
// `unsafe` block is as small as possible and carries a `SAFETY` comment.
// The exception was approved by the project owner (report 0016), who
// prefers our own audited calls over third-party input crates.
// ---------------------------------------------------------------------

use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RETURN, VK_RWIN,
    VK_SHIFT, VK_TAB,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
};
use zeroize::{DefaultIsZeroes, Zeroizing};

use crate::autotype::{ActiveWindow, AutoTypeError, Backend, KEY_DELAY, SpecialKey, WindowId};

/// Longest window title read, in UTF-16 units.
const MAX_TITLE_UNITS: usize = 4096;
/// Longest executable path read (the Windows long-path limit).
const MAX_PATH_UNITS: usize = 32_768;

/// `INPUT` wrapper so the events (which hold the typed characters) can be
/// wiped with `zeroize` once sent. `INPUT::default()` is all zeroes.
#[repr(transparent)]
#[derive(Clone, Copy, Default)]
struct RawInput(INPUT);

impl DefaultIsZeroes for RawInput {}

fn key_event(vk: VIRTUAL_KEY, scan: u16, flags: u32) -> RawInput {
    RawInput(INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    })
}

/// Sends the events with `SendInput`, which inserts them as one block
/// that other input cannot interleave with.
#[allow(unsafe_code)]
fn send_inputs(inputs: &[RawInput]) -> Result<(), AutoTypeError> {
    let count = u32::try_from(inputs.len()).map_err(|_| AutoTypeError::Blocked)?;
    let size = i32::try_from(std::mem::size_of::<INPUT>()).map_err(|_| AutoTypeError::Blocked)?;
    // SAFETY: `RawInput` is `repr(transparent)` over `INPUT`, so the slice
    // is `count` valid, initialized `INPUT` values; Windows only reads them
    // during the call.
    let sent = unsafe { SendInput(count, inputs.as_ptr().cast::<INPUT>(), size) };
    // UIPI (a target with higher privileges) makes the call insert nothing.
    if sent == count {
        Ok(())
    } else {
        Err(AutoTypeError::Blocked)
    }
}

/// `GetForegroundWindow` and `GetAsyncKeyState` + `SendInput` behind the
/// guarded auto-type logic in `crate::autotype`.
pub(crate) struct OsBackend;

impl Backend for OsBackend {
    #[allow(unsafe_code)]
    fn foreground(&self) -> Option<WindowId> {
        // SAFETY: no arguments; returns a handle (possibly null) that is
        // only compared, never dereferenced.
        let hwnd: HWND = unsafe { GetForegroundWindow() };
        (!hwnd.is_null()).then(|| WindowId::from_raw(hwnd as usize))
    }

    #[allow(unsafe_code)]
    fn modifiers_pressed(&self) -> bool {
        [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN]
            .into_iter()
            .any(|vk| {
                // SAFETY: plain query of a virtual-key code; no pointers.
                let state = unsafe { GetAsyncKeyState(i32::from(vk)) };
                // The high bit (negative value) means "down right now".
                state < 0
            })
    }

    fn send_char(&self, ch: char) -> Result<(), AutoTypeError> {
        // KEYEVENTF_UNICODE types the character itself, whatever the
        // keyboard layout. Characters outside the BMP are two UTF-16
        // units, each sent as its own press and release.
        let mut units = Zeroizing::new([0u16; 2]);
        let encoded_len = ch.encode_utf16(&mut *units).len();
        let mut inputs = Zeroizing::new([RawInput::default(); 4]);
        for (i, unit) in units.iter().take(encoded_len).enumerate() {
            inputs[2 * i] = key_event(0, *unit, KEYEVENTF_UNICODE);
            inputs[2 * i + 1] = key_event(0, *unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
        }
        send_inputs(inputs.get(..2 * encoded_len).unwrap_or_default())
    }

    fn send_key(&self, key: SpecialKey) -> Result<(), AutoTypeError> {
        let vk = match key {
            SpecialKey::Tab => VK_TAB,
            SpecialKey::Enter => VK_RETURN,
        };
        send_inputs(&[key_event(vk, 0, 0), key_event(vk, 0, KEYEVENTF_KEYUP)])
    }

    fn pause(&self) {
        std::thread::sleep(KEY_DELAY);
    }
}

/// Foreground window with its title and executable path.
#[allow(unsafe_code)]
pub(crate) fn active_window() -> Option<ActiveWindow> {
    // SAFETY: no arguments; the handle is only passed back to the
    // read-only queries below, which accept stale handles (they fail).
    let hwnd: HWND = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return None;
    }
    Some(ActiveWindow {
        id: WindowId::from_raw(hwnd as usize),
        title: window_title(hwnd),
        executable: window_executable(hwnd),
    })
}

#[allow(unsafe_code)]
fn window_title(hwnd: HWND) -> String {
    // SAFETY: read-only query on a window handle; returns 0 on failure.
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    let Ok(len) = usize::try_from(len) else {
        return String::new();
    };
    let capacity = len.min(MAX_TITLE_UNITS) + 1;
    let mut buffer = vec![0u16; capacity];
    let Ok(max) = i32::try_from(capacity) else {
        return String::new();
    };
    // SAFETY: `buffer` has room for `max` UTF-16 units including the
    // terminating null, which is what `nmaxcount` declares.
    let copied = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), max) };
    let copied = usize::try_from(copied).unwrap_or(0).min(capacity);
    String::from_utf16_lossy(buffer.get(..copied).unwrap_or_default())
}

#[allow(unsafe_code)]
fn window_executable(hwnd: HWND) -> Option<PathBuf> {
    let mut pid: u32 = 0;
    // SAFETY: `pid` is a valid, writable local.
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    if pid == 0 {
        return None;
    }
    // SAFETY: plain call; the least privileged right that allows reading
    // the image name. Fails (null) for protected processes.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0u16; MAX_PATH_UNITS];
    let mut size = u32::try_from(buffer.len()).unwrap_or(0);
    // SAFETY: `process` is a valid handle opened above; `buffer` has room
    // for `size` units and `size` is a writable local updated with the
    // number of units written (without the null).
    let ok = unsafe {
        QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut size)
    };
    // SAFETY: `process` was opened above and is closed exactly once.
    unsafe { CloseHandle(process) };
    if ok == 0 {
        return None;
    }
    let len = usize::try_from(size).unwrap_or(0).min(buffer.len());
    let units = buffer.get(..len)?;
    Some(PathBuf::from(std::ffi::OsString::from_wide(units)))
}

// ---------------------------------------------------------------------
// Global shortcut for auto-type (Phase 2), within the same approval
// (report 0016). `RegisterHotKey` with no window posts `WM_HOTKEY` to the
// registering thread, so a dedicated thread owns the shortcut and runs a
// message loop; posting `WM_QUIT` to it unregisters and ends it.
// ---------------------------------------------------------------------

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{ERROR_HOTKEY_ALREADY_REGISTERED, GetLastError};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_HOTKEY, WM_QUIT, WM_USER,
};

use crate::hotkey::{Hotkey, HotkeyError};

/// Identifier of our shortcut within the listener thread.
const HOTKEY_ID: i32 = 1;

/// The listener thread of a registered shortcut.
#[derive(Debug)]
pub(crate) struct HotkeyHandle {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl Drop for HotkeyHandle {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: plain call with integer arguments; `thread_id` is the
        // listener thread, whose message queue exists (created before its
        // id was handed out).
        let posted = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
        let Some(thread) = self.thread.take() else {
            return;
        };
        // Wait for the thread to unregister the shortcut. Never wait if the
        // message could not be posted (it would wait forever) or if the
        // handle is dropped from the callback itself (the thread would wait
        // for itself); in both cases the thread is left to end on its own.
        if posted != 0 && thread.thread().id() != std::thread::current().id() {
            let _ = thread.join();
        }
    }
}

pub(crate) fn register_hotkey(
    hotkey: Hotkey,
    on_press: Box<dyn Fn() + Send>,
) -> Result<HotkeyHandle, HotkeyError> {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("llavero-hotkey".into())
        .spawn(move || hotkey_thread(hotkey, on_press, ready_tx))
        .map_err(|_| HotkeyError::Failed)?;
    match ready_rx.recv() {
        Ok(Ok(thread_id)) => Ok(HotkeyHandle {
            thread_id,
            thread: Some(thread),
        }),
        // The thread has already returned (or is about to).
        Ok(Err(error)) => {
            let _ = thread.join();
            Err(error)
        }
        Err(_) => {
            let _ = thread.join();
            Err(HotkeyError::Failed)
        }
    }
}

fn win32_modifiers(hotkey: Hotkey) -> HOT_KEY_MODIFIERS {
    let m = hotkey.modifiers();
    // MOD_NOREPEAT: holding the shortcut down fires it only once.
    [
        (m.ctrl, MOD_CONTROL),
        (m.alt, MOD_ALT),
        (m.shift, MOD_SHIFT),
        (m.win, MOD_WIN),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .fold(MOD_NOREPEAT, |acc, (_, flag)| acc | flag)
}

/// Body of the listener thread: registers the shortcut, reports the
/// outcome through `ready`, then calls `on_press` on every `WM_HOTKEY`
/// until `WM_QUIT` arrives.
#[allow(unsafe_code)]
fn hotkey_thread(
    hotkey: Hotkey,
    on_press: Box<dyn Fn() + Send>,
    ready: Sender<Result<u32, HotkeyError>>,
) {
    let mut msg = MSG::default();
    // SAFETY: `msg` is a valid, writable local. Peeking creates this
    // thread's message queue, so a `WM_QUIT` posted as soon as the id is
    // handed out below is never lost. Nothing is removed from the queue.
    unsafe {
        PeekMessageW(
            &mut msg,
            std::ptr::null_mut(),
            WM_USER,
            WM_USER,
            PM_NOREMOVE,
        )
    };
    // SAFETY: no arguments.
    let thread_id = unsafe { GetCurrentThreadId() };

    // Letters (upper case) and digits share their ASCII code with their
    // virtual-key code; `Hotkey::new` allows nothing else.
    let vk = u32::from(hotkey.key());
    // SAFETY: a null window means "post WM_HOTKEY to this thread"; the
    // other arguments are plain integers.
    let registered =
        unsafe { RegisterHotKey(std::ptr::null_mut(), HOTKEY_ID, win32_modifiers(hotkey), vk) };
    if registered == 0 {
        // SAFETY: no arguments; reads the error of the call just above.
        let code = unsafe { GetLastError() };
        let error = if code == ERROR_HOTKEY_ALREADY_REGISTERED {
            HotkeyError::AlreadyInUse
        } else {
            HotkeyError::Failed
        };
        let _ = ready.send(Err(error));
        return;
    }

    if ready.send(Ok(thread_id)).is_ok() {
        loop {
            // SAFETY: `msg` is a valid, writable local; a null window and
            // zero filters read every message of this thread.
            let got = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            // 0 is WM_QUIT; -1 is an error. Either way, stop listening.
            if got <= 0 {
                break;
            }
            if msg.message == WM_HOTKEY && msg.wParam == HOTKEY_ID as usize {
                // A panicking callback must not leave the shortcut
                // registered with nobody listening.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&on_press));
            }
        }
    }

    // SAFETY: unregisters the shortcut this same thread registered above
    // (Windows requires the registering thread).
    unsafe { UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID) };
}
