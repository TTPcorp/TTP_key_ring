//! OS abstraction layer of LLavero Digital.
//!
//! Every OS-specific capability (global hotkeys, keyboard simulation, active
//! window detection, clipboard, Native Messaging registration, OS secure
//! storage) is exposed here as a trait, with one implementation per OS
//! selected via `#[cfg(target_os = "...")]`.

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {}
}
