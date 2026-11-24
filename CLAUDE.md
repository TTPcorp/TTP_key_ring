# CLAUDE.md — LLavero Digital

Local, encrypted desktop password manager with web autofill (browser extension) and auto-type into programs. See `README.md` for the full picture.

## Fixed decisions (do not change without explicit confirmation)

- **Platform:** Windows 10/11 x64 first. Linux/macOS later — the architecture must allow it without rewriting the core.
- **Stack:** Rust (latest stable edition), Tauri 2, HTML/CSS/TypeScript UI. The UI framework is **not decided yet**: do not pick one without asking.
- **Extension:** WebExtensions Manifest V3, Chrome/Edge first, Firefox later. Extension ↔ app over Native Messaging (local).
- **Crypto:** Argon2id KDF (random per-vault salt, parameters stored in the file header); XChaCha20-Poly1305 (preferred) or AES-256-GCM with a fresh random nonce on every save. Crates: `argon2`, `chacha20poly1305` / `aes-gcm`, `zeroize`, `secrecy`, `rand`.
- **Memory:** keys and passwords live in types that zero on drop (`zeroize` / `secrecy`).
- **Atomic save:** write to a temp file, then rename.
- **No master-password recovery.** No backdoors.
- **Workspace layout:**
  - `core/` (`llavero-core`): pure, portable logic. No OS or Tauri dependencies. Unit-tested.
  - `platform/` (`llavero-platform`): OS traits + per-OS impls selected with `#[cfg(target_os = "...")]`.
  - `app/` (`llavero-app`): Tauri application.
  - `native-host/` (`llavero-native-host`): Native Messaging bridge.
  - `extension/`: browser extension (not a Rust crate).
- **Portability rule:** anything OS-specific (global hotkeys, keyboard simulation, active window, clipboard, Native Messaging registration, OS secure storage) goes behind a trait in `platform/`. `core` never calls Windows APIs. Paths always via `std::path` and `dirs`, never hand-written.

If you think any of the above should change, propose it and wait for confirmation.

## Working rules

- No new cryptography dependency without justification. **Never** hand-roll cryptographic primitives.
- No `unwrap()` / `expect()` in `core` production code; use typed errors (`thiserror`). (Enforced workspace-wide by clippy lints; allowed in tests via `clippy.toml`.)
- No secrets in logs, error messages, tests with real data, or commits.
- Every change in `core` comes with tests.
- Before considering a task done, all of these must pass:
  ```
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```
- No network calls from the app or the native host.
- Security over convenience: when in doubt, choose the safer option.
- Code and code comments in English; user documentation (README) in Spanish.
- When instructions are ambiguous or conflict with the repository, ask before deciding.
