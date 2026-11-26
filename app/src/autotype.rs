//! Auto-type flow: what happens when the global shortcut is pressed.
//!
//! 1. The window in the foreground is captured at once: it is the only
//!    window auto-type will ever type into for this press.
//! 2. If the vault is locked, the user is asked to unlock it and press the
//!    shortcut again (decided by the project owner, 2026-11-26).
//! 3. The entries associated with that window are looked up. Exactly one,
//!    through an association that checks the executable, is typed right
//!    away; otherwise the user picks one in the app.
//! 4. Typing waits for the shortcut's modifiers to be released, and stops
//!    if the foreground window changes or a modifier is pressed.
//!
//! The UI learns what happened through [`Notice`]s it polls for. Picking
//! an entry needs the one-time request id of that press, so the page can
//! never start typing on its own, nor choose the target window.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use llavero_core::autotype::{AutoTypeChoice, Key, Sequence, Step, TargetWindow};
use llavero_core::generator::{self, PasswordOptions};
use llavero_core::model::{Entry, VaultData};
use llavero_platform::{ActiveWindow, AutoType, AutoTypeError, SpecialKey, WindowId};
use secrecy::ExposeSecret;
use serde::Serialize;

use crate::state::{AppError, AppState};

/// How long the user has to pick an entry after pressing the shortcut.
pub const REQUEST_TTL: Duration = Duration::from_secs(30);
/// How long to wait for the shortcut's keys to be released.
pub const MODIFIER_WAIT: Duration = Duration::from_secs(2);
/// How long to wait, after hiding the app, for the target window to be
/// in the foreground again.
pub const FOCUS_WAIT: Duration = Duration::from_millis(1500);
/// Polling interval while waiting.
const POLL: Duration = Duration::from_millis(20);

/// An entry the user can pick. No secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    pub username: String,
}

/// What the UI must tell the user after a shortcut press.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Notice {
    /// Another program already uses the shortcut.
    HotkeyUnavailable { hotkey: String },
    /// The vault is locked: unlock it and press the shortcut again.
    Locked { hotkey: String },
    /// No entry is associated with the window.
    NoMatch { window: String },
    /// The user must pick (or confirm) the entry to type.
    Choose {
        request_id: String,
        window: String,
        entries: Vec<Candidate>,
    },
    /// Auto-type failed (nothing, or only part, was typed).
    Failed { error: AppError },
}

/// A press waiting for the user to pick an entry.
struct Pending {
    id: String,
    target: WindowId,
    entries: Vec<String>,
    created: Instant,
}

/// What to do for the captured window, decided with the vault open.
enum Plan {
    NoMatch,
    Direct(Result<Vec<Step>, AppError>),
    Choose(Vec<Candidate>),
}

/// State of the auto-type flow.
pub struct AutoTypeFlow {
    typer: Arc<dyn AutoType>,
    hotkey: String,
    own_exe: Option<PathBuf>,
    ttl: Duration,
    modifier_wait: Duration,
    focus_wait: Duration,
    pending: Mutex<Option<Pending>>,
    notice: Mutex<Option<Notice>>,
    busy: AtomicBool,
}

impl AutoTypeFlow {
    /// `hotkey` is the shortcut as shown to the user (e.g. `Ctrl+Alt+A`).
    pub fn new(typer: Arc<dyn AutoType>, hotkey: String) -> Self {
        Self {
            typer,
            hotkey,
            own_exe: std::env::current_exe().ok(),
            ttl: REQUEST_TTL,
            modifier_wait: MODIFIER_WAIT,
            focus_wait: FOCUS_WAIT,
            pending: Mutex::new(None),
            notice: Mutex::new(None),
            busy: AtomicBool::new(false),
        }
    }

    /// Takes the notice for the UI, if any (each one is shown once).
    pub fn take_notice(&self) -> Option<Notice> {
        self.notice.lock().ok()?.take()
    }

    fn set_notice(&self, notice: Notice) {
        if let Ok(mut slot) = self.notice.lock() {
            *slot = Some(notice);
        }
    }

    /// Records that the shortcut could not be registered.
    pub fn hotkey_unavailable(&self) {
        self.set_notice(Notice::HotkeyUnavailable {
            hotkey: self.hotkey.clone(),
        });
    }

    /// Handles a press of the shortcut. Returns whether the app window
    /// must be shown (there is a notice for the user).
    pub fn on_hotkey(&self, app: &AppState) -> bool {
        if self.busy.load(Ordering::SeqCst) {
            return false;
        }
        let Some(window) = self.typer.active_window() else {
            return false;
        };
        // Pressed inside the app itself: nothing to type into.
        if self.is_own(&window) {
            return false;
        }
        let target = TargetWindow {
            title: window.title.clone(),
            process: window
                .executable
                .as_deref()
                .and_then(Path::file_name)
                .map(|name| name.to_string_lossy().into_owned()),
        };
        let notice = match app.with_data(|data| plan_for(data, &target)) {
            Err(AppError::Locked) => Notice::Locked {
                hotkey: self.hotkey.clone(),
            },
            Err(error) | Ok(Plan::Direct(Err(error))) => Notice::Failed { error },
            Ok(Plan::NoMatch) => Notice::NoMatch {
                window: window.title,
            },
            Ok(Plan::Direct(Ok(steps))) => {
                app.touch();
                match self.run(window.id, &steps) {
                    Ok(()) => return false,
                    Err(error) => Notice::Failed { error },
                }
            }
            Ok(Plan::Choose(entries)) => {
                let Ok(request_id) = new_request_id() else {
                    self.set_notice(Notice::Failed {
                        error: AppError::Internal,
                    });
                    return true;
                };
                if let Ok(mut slot) = self.pending.lock() {
                    *slot = Some(Pending {
                        id: request_id.clone(),
                        target: window.id,
                        entries: entries.iter().map(|c| c.id.clone()).collect(),
                        created: Instant::now(),
                    });
                }
                Notice::Choose {
                    request_id,
                    window: window.title,
                    entries,
                }
            }
        };
        self.set_notice(notice);
        true
    }

    /// Types the entry the user picked for a pending press. `hide_app`
    /// must take the app out of the way (minimize it) so that the target
    /// window returns to the foreground; typing only starts if it does.
    pub fn choose(
        &self,
        app: &AppState,
        request_id: &str,
        entry_id: &str,
        hide_app: impl FnOnce(),
    ) -> Result<(), AppError> {
        let pending = {
            let mut slot = self.pending.lock().map_err(|_| AppError::Internal)?;
            match slot.take() {
                Some(p) if p.id == request_id => p,
                other => {
                    *slot = other;
                    return Err(AppError::AutoTypeExpired);
                }
            }
        };
        if pending.created.elapsed() > self.ttl {
            return Err(AppError::AutoTypeExpired);
        }
        if !pending.entries.iter().any(|id| id == entry_id) {
            return Err(AppError::EntryNotFound);
        }
        let steps = app.with_data(|data| {
            data.entries()
                .iter()
                .find(|e| e.id().to_string() == entry_id)
                .ok_or(AppError::EntryNotFound)
                .and_then(steps_for)
        })??;
        app.touch();
        hide_app();
        let target = pending.target;
        if !self.wait_for(self.focus_wait, || {
            self.typer.active_window().map(|w| w.id) == Some(target)
        }) {
            return Err(AppError::AutoTypeWindowChanged);
        }
        self.run(target, &steps)
    }

    /// Drops a pending press (the user closed the picker).
    pub fn cancel(&self, request_id: &str) {
        if let Ok(mut slot) = self.pending.lock()
            && slot.as_ref().is_some_and(|p| p.id == request_id)
        {
            *slot = None;
        }
    }

    fn is_own(&self, window: &ActiveWindow) -> bool {
        match (&self.own_exe, &window.executable) {
            (Some(own), Some(exe)) => {
                own.to_string_lossy().to_lowercase() == exe.to_string_lossy().to_lowercase()
            }
            _ => false,
        }
    }

    /// Polls `done` until it holds or `limit` passes.
    fn wait_for(&self, limit: Duration, mut done: impl FnMut() -> bool) -> bool {
        let start = Instant::now();
        loop {
            if done() {
                return true;
            }
            if start.elapsed() >= limit {
                return false;
            }
            std::thread::sleep(POLL);
        }
    }

    /// Types the steps into `target`, one auto-type at a time.
    fn run(&self, target: WindowId, steps: &[Step]) -> Result<(), AppError> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(AppError::Internal);
        }
        let _busy = BusyGuard(&self.busy);
        // The user is usually still holding the shortcut.
        if !self.wait_for(self.modifier_wait, || !self.typer.modifiers_pressed()) {
            return Err(AppError::AutoTypeModifierPressed);
        }
        for step in steps {
            match step {
                Step::Text(text) => self.typer.type_text(target, text.expose_secret()),
                Step::Key(key) => self.typer.press_key(target, special_key(*key)),
                Step::Delay(delay) => {
                    std::thread::sleep(*delay);
                    Ok(())
                }
            }
            .map_err(autotype_error)?;
        }
        Ok(())
    }
}

/// Clears the busy flag when typing ends, however it ends.
struct BusyGuard<'a>(&'a AtomicBool);

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn plan_for(data: &VaultData, target: &TargetWindow) -> Plan {
    match data.autotype_choice(target) {
        AutoTypeChoice::NoMatch => Plan::NoMatch,
        AutoTypeChoice::TypeDirectly(entry) => Plan::Direct(steps_for(entry)),
        AutoTypeChoice::Choose(entries) => Plan::Choose(
            entries
                .into_iter()
                .map(|e| Candidate {
                    id: e.id().to_string(),
                    name: e.name().to_owned(),
                    username: e.username().to_owned(),
                })
                .collect(),
        ),
    }
}

fn steps_for(entry: &Entry) -> Result<Vec<Step>, AppError> {
    let sequence = Sequence::for_entry(entry).map_err(|_| AppError::AutoTypeSequenceInvalid)?;
    Ok(sequence.resolve(entry))
}

fn special_key(key: Key) -> SpecialKey {
    match key {
        Key::Tab => SpecialKey::Tab,
        Key::Enter => SpecialKey::Enter,
    }
}

fn autotype_error(error: AutoTypeError) -> AppError {
    match error {
        AutoTypeError::Unsupported => AppError::AutoTypeUnsupported,
        AutoTypeError::WindowChanged => AppError::AutoTypeWindowChanged,
        AutoTypeError::ModifierPressed => AppError::AutoTypeModifierPressed,
        AutoTypeError::Blocked => AppError::AutoTypeBlocked,
    }
}

/// Unguessable one-time id (24 random lowercase letters and digits, from
/// the same CSPRNG as the password generator).
fn new_request_id() -> Result<String, AppError> {
    let options = PasswordOptions {
        length: 24,
        lowercase: true,
        uppercase: false,
        digits: true,
        symbols: false,
        exclude_ambiguous: false,
        require_each_class: false,
    };
    let id = generator::generate(&options).map_err(|_| AppError::Internal)?;
    Ok(id.expose_secret().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::state;
    use llavero_core::model::{EntryInput, WindowMatch};
    use secrecy::SecretString;

    const TARGET: usize = 7;
    const APP: usize = 1;

    /// Fake keyboard and window system: records what is typed.
    struct FakeTyper {
        foreground: Mutex<Option<ActiveWindow>>,
        modifiers: AtomicBool,
        typed: Mutex<String>,
    }

    impl FakeTyper {
        fn new(foreground: Option<ActiveWindow>) -> Arc<Self> {
            Arc::new(Self {
                foreground: Mutex::new(foreground),
                modifiers: AtomicBool::new(false),
                typed: Mutex::new(String::new()),
            })
        }
        fn set_foreground(&self, window: Option<ActiveWindow>) {
            *self.foreground.lock().unwrap() = window;
        }
        fn typed(&self) -> String {
            self.typed.lock().unwrap().clone()
        }
        fn check(&self, target: WindowId) -> Result<(), AutoTypeError> {
            match self.foreground.lock().unwrap().as_ref() {
                Some(w) if w.id == target => Ok(()),
                _ => Err(AutoTypeError::WindowChanged),
            }
        }
    }

    impl AutoType for FakeTyper {
        fn active_window(&self) -> Option<ActiveWindow> {
            self.foreground.lock().unwrap().clone()
        }
        fn modifiers_pressed(&self) -> bool {
            self.modifiers.load(Ordering::SeqCst)
        }
        fn type_text(&self, target: WindowId, text: &str) -> Result<(), AutoTypeError> {
            self.check(target)?;
            self.typed.lock().unwrap().push_str(text);
            Ok(())
        }
        fn press_key(&self, target: WindowId, key: SpecialKey) -> Result<(), AutoTypeError> {
            self.check(target)?;
            let name = match key {
                SpecialKey::Tab => "<TAB>",
                SpecialKey::Enter => "<ENTER>",
            };
            self.typed.lock().unwrap().push_str(name);
            Ok(())
        }
    }

    fn erp_window() -> ActiveWindow {
        ActiveWindow {
            id: WindowId::from_raw(TARGET),
            title: "ERP - Login".to_owned(),
            executable: Some(PathBuf::from(r"C:\Program Files\ERP\erp.exe")),
        }
    }

    fn app_window(flow: &AutoTypeFlow) -> ActiveWindow {
        ActiveWindow {
            id: WindowId::from_raw(APP),
            title: "LLavero Digital".to_owned(),
            executable: flow.own_exe.clone(),
        }
    }

    fn matcher(title: Option<&str>, process: Option<&str>) -> WindowMatch {
        WindowMatch {
            title: title.map(str::to_owned),
            process: process.map(str::to_owned),
        }
    }

    /// Unlocked app state with one entry per `(name, associations)`.
    fn app_with(dir: &Path, entries: Vec<(&str, Vec<WindowMatch>)>) -> AppState {
        let app = state(dir);
        app.create(&SecretString::from("master password!")).unwrap();
        app.with_vault(|vault| {
            for (name, windows) in entries {
                let mut input =
                    EntryInput::new(name, format!("{name}-user"), SecretString::from("pw"));
                input.windows = windows;
                vault.data_mut().add_entry(input).unwrap();
            }
        })
        .unwrap();
        app
    }

    fn flow(typer: &Arc<FakeTyper>) -> AutoTypeFlow {
        let mut flow = AutoTypeFlow::new(typer.clone(), "Ctrl+Alt+A".to_owned());
        flow.modifier_wait = Duration::from_millis(60);
        flow.focus_wait = Duration::from_millis(60);
        flow
    }

    fn take_choose(flow: &AutoTypeFlow) -> (String, Vec<Candidate>) {
        match flow.take_notice() {
            Some(Notice::Choose {
                request_id,
                window,
                entries,
            }) => {
                assert_eq!(window, "ERP - Login");
                (request_id, entries)
            }
            other => panic!("expected a picker, got {other:?}"),
        }
    }

    #[test]
    fn single_match_by_executable_is_typed_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(
            dir.path(),
            vec![("ERP", vec![matcher(None, Some("ERP.EXE"))])],
        );
        let typer = FakeTyper::new(Some(erp_window()));
        let flow = flow(&typer);
        assert!(!flow.on_hotkey(&app), "nothing to show");
        assert_eq!(typer.typed(), "ERP-user<TAB>pw<ENTER>");
        assert_eq!(flow.take_notice(), None);
    }

    #[test]
    fn locked_vault_asks_to_unlock_and_press_again() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(
            dir.path(),
            vec![("ERP", vec![matcher(None, Some("erp.exe"))])],
        );
        app.lock().unwrap();
        let typer = FakeTyper::new(Some(erp_window()));
        let flow = flow(&typer);
        assert!(flow.on_hotkey(&app));
        assert_eq!(
            flow.take_notice(),
            Some(Notice::Locked {
                hotkey: "Ctrl+Alt+A".to_owned()
            })
        );
        assert_eq!(flow.take_notice(), None, "shown once");
        assert_eq!(typer.typed(), "");
    }

    #[test]
    fn no_match_types_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(
            dir.path(),
            vec![("Other", vec![matcher(None, Some("x.exe"))])],
        );
        let typer = FakeTyper::new(Some(erp_window()));
        let flow = flow(&typer);
        assert!(flow.on_hotkey(&app));
        assert_eq!(
            flow.take_notice(),
            Some(Notice::NoMatch {
                window: "ERP - Login".to_owned()
            })
        );
        assert_eq!(typer.typed(), "");
    }

    #[test]
    fn title_only_match_waits_for_the_user_then_types_into_the_original_window() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(dir.path(), vec![("ERP", vec![matcher(Some("ERP*"), None)])]);
        let typer = FakeTyper::new(Some(erp_window()));
        let flow = flow(&typer);
        assert!(flow.on_hotkey(&app));
        assert_eq!(typer.typed(), "", "nothing typed before the user confirms");
        let (request_id, entries) = take_choose(&flow);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "ERP");
        assert_eq!(entries[0].username, "ERP-user");

        // The picker is in the foreground; hiding it brings the target back.
        typer.set_foreground(Some(app_window(&flow)));
        flow.choose(&app, &request_id, &entries[0].id, || {
            typer.set_foreground(Some(erp_window()));
        })
        .unwrap();
        assert_eq!(typer.typed(), "ERP-user<TAB>pw<ENTER>");

        // The request is single use.
        assert_eq!(
            flow.choose(&app, &request_id, &entries[0].id, || {}),
            Err(AppError::AutoTypeExpired)
        );
    }

    #[test]
    fn several_matches_offer_all_of_them() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(
            dir.path(),
            vec![
                ("A", vec![matcher(None, Some("erp.exe"))]),
                ("B", vec![matcher(None, Some("erp.exe"))]),
            ],
        );
        let typer = FakeTyper::new(Some(erp_window()));
        let flow = flow(&typer);
        assert!(flow.on_hotkey(&app));
        let (request_id, entries) = take_choose(&flow);
        let names: Vec<&str> = entries.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["A", "B"]);
        flow.choose(&app, &request_id, &entries[1].id, || {})
            .unwrap();
        assert_eq!(typer.typed(), "B-user<TAB>pw<ENTER>");
    }

    #[test]
    fn choose_never_types_if_the_target_does_not_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(dir.path(), vec![("ERP", vec![matcher(Some("ERP*"), None)])]);
        let typer = FakeTyper::new(Some(erp_window()));
        let flow = flow(&typer);
        flow.on_hotkey(&app);
        let (request_id, entries) = take_choose(&flow);
        typer.set_foreground(Some(app_window(&flow)));
        assert_eq!(
            flow.choose(&app, &request_id, &entries[0].id, || {}),
            Err(AppError::AutoTypeWindowChanged)
        );
        assert_eq!(typer.typed(), "");
    }

    #[test]
    fn choose_rejects_unknown_requests_entries_and_expired_ones() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(
            dir.path(),
            vec![
                ("ERP", vec![matcher(Some("ERP*"), None)]),
                ("Unrelated", vec![]),
            ],
        );
        let unrelated = app.with_data(|d| d.entries()[1].id().to_string()).unwrap();
        let typer = FakeTyper::new(Some(erp_window()));
        let mut flow = flow(&typer);

        flow.on_hotkey(&app);
        let (request_id, entries) = take_choose(&flow);
        assert_eq!(
            flow.choose(&app, "guessed", &entries[0].id, || {}),
            Err(AppError::AutoTypeExpired)
        );
        // An entry that was not offered for this window is refused.
        assert_eq!(
            flow.choose(&app, &request_id, &unrelated, || {}),
            Err(AppError::EntryNotFound)
        );

        flow.ttl = Duration::ZERO;
        flow.on_hotkey(&app);
        let (request_id, entries) = take_choose(&flow);
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(
            flow.choose(&app, &request_id, &entries[0].id, || {}),
            Err(AppError::AutoTypeExpired)
        );

        flow.ttl = REQUEST_TTL;
        flow.on_hotkey(&app);
        let (request_id, entries) = take_choose(&flow);
        flow.cancel(&request_id);
        assert_eq!(
            flow.choose(&app, &request_id, &entries[0].id, || {}),
            Err(AppError::AutoTypeExpired)
        );
        assert_eq!(typer.typed(), "");
    }

    #[test]
    fn choose_fails_if_the_vault_was_locked_meanwhile() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(dir.path(), vec![("ERP", vec![matcher(Some("ERP*"), None)])]);
        let typer = FakeTyper::new(Some(erp_window()));
        let flow = flow(&typer);
        flow.on_hotkey(&app);
        let (request_id, entries) = take_choose(&flow);
        app.lock().unwrap();
        assert_eq!(
            flow.choose(&app, &request_id, &entries[0].id, || {}),
            Err(AppError::Locked)
        );
        assert_eq!(typer.typed(), "");
    }

    #[test]
    fn held_modifiers_cancel_typing() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(
            dir.path(),
            vec![("ERP", vec![matcher(None, Some("erp.exe"))])],
        );
        let typer = FakeTyper::new(Some(erp_window()));
        typer.modifiers.store(true, Ordering::SeqCst);
        let flow = flow(&typer);
        assert!(flow.on_hotkey(&app));
        assert_eq!(
            flow.take_notice(),
            Some(Notice::Failed {
                error: AppError::AutoTypeModifierPressed
            })
        );
        assert_eq!(typer.typed(), "");
    }

    #[test]
    fn invalid_sequence_is_reported_without_typing() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(dir.path(), vec![]);
        app.with_vault(|vault| {
            let mut input = EntryInput::new("ERP", "u", SecretString::from("pw"));
            input.windows = vec![matcher(None, Some("erp.exe"))];
            input.autotype = Some("{USERNAME}{TABB}".to_owned());
            vault.data_mut().add_entry(input).unwrap();
        })
        .unwrap();
        let typer = FakeTyper::new(Some(erp_window()));
        let flow = flow(&typer);
        assert!(flow.on_hotkey(&app));
        assert_eq!(
            flow.take_notice(),
            Some(Notice::Failed {
                error: AppError::AutoTypeSequenceInvalid
            })
        );
        assert_eq!(typer.typed(), "");
    }

    #[test]
    fn pressing_the_shortcut_inside_the_app_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(dir.path(), vec![("Any", vec![matcher(Some("*"), None)])]);
        let typer = FakeTyper::new(None);
        let flow = flow(&typer);
        typer.set_foreground(Some(app_window(&flow)));
        assert!(!flow.on_hotkey(&app));
        assert_eq!(flow.take_notice(), None);
        // No foreground window at all: nothing either.
        typer.set_foreground(None);
        assert!(!flow.on_hotkey(&app));
        assert_eq!(typer.typed(), "");
    }

    #[test]
    fn notices_serialize_for_the_ui_without_secrets() {
        let notice = Notice::Choose {
            request_id: "r".to_owned(),
            window: "W".to_owned(),
            entries: vec![Candidate {
                id: "1".to_owned(),
                name: "N".to_owned(),
                username: "u".to_owned(),
            }],
        };
        assert_eq!(
            serde_json::to_string(&notice).unwrap(),
            r#"{"kind":"choose","requestId":"r","window":"W","entries":[{"id":"1","name":"N","username":"u"}]}"#
        );
        let failed = Notice::Failed {
            error: AppError::AutoTypeBlocked,
        };
        assert_eq!(
            serde_json::to_string(&failed).unwrap(),
            r#"{"kind":"failed","error":{"code":"AutoTypeBlocked"}}"#
        );
    }

    #[test]
    fn request_ids_are_random() {
        let a = new_request_id().unwrap();
        let b = new_request_id().unwrap();
        assert_eq!(a.len(), 24);
        assert_ne!(a, b);
    }
}
