# CLAUDE.md — LLavero Digital

Local, encrypted desktop password manager with web autofill (browser extension) and auto-type into programs. See `README.md` for the full picture.

## Fixed decisions (do not change without explicit confirmation)

- **Platform:** Windows 10/11 x64 first. Linux/macOS later — the architecture must allow it without rewriting the core.
- **Stack:** Rust (latest stable edition), Tauri 2, HTML/CSS/TypeScript UI with **Svelte 5 + Vite** (decided 2026-11-24, report 0006). **No SvelteKit**, no UI component libraries, no assets loaded from the network; strict CSP. Keep npm dependencies to the minimum and justify each new one.
- **Extension:** WebExtensions Manifest V3, Chrome/Edge first, Firefox later. Extension ↔ app over Native Messaging (local).
- **Crypto:** Argon2id KDF (random per-vault salt, parameters stored in the file header); XChaCha20-Poly1305 (preferred) or AES-256-GCM with a fresh random nonce on every save. Crates: `argon2`, `chacha20poly1305` / `aes-gcm`, `zeroize`, `secrecy`, `rand`.
- **Memory:** keys and passwords live in types that zero on drop (`zeroize` / `secrecy`).
- **Atomic save:** write to a temp file, then rename.
- **No master-password recovery.** No backdoors.
- **Workspace layout:**
  - `core/` (`llavero-core`): pure, portable logic. No OS or Tauri dependencies. Unit-tested.
  - `platform/` (`llavero-platform`): OS traits + per-OS impls selected with `#[cfg(target_os = "...")]`.
  - `app/` (`llavero-app`): Tauri application. Rust in `app/src/`, Svelte frontend in `app/ui/`, npm project (`package.json`, `.npmrc` with `ignore-scripts`) in `app/`, IPC permissions in `app/capabilities/`.
  - `native-host/` (`llavero-native-host`): Native Messaging bridge.
  - `extension/`: browser extension (not a Rust crate).
- **Portability rule:** anything OS-specific (global hotkeys, keyboard simulation, active window, clipboard, Native Messaging registration, OS secure storage) goes behind a trait in `platform/`. `core` never calls Windows APIs. Paths always via `std::path` and `dirs`, never hand-written.

If you think any of the above should change, propose it and wait for confirmation.

## Working rules

- No new cryptography dependency without justification. **Never** hand-roll cryptographic primitives.
- No `unsafe` code (`unsafe_code = "deny"` in the workspace). The only approved exception is `is_session_locked` in `platform/src/windows.rs` (Windows session-lock query, approved by the project owner on 2026-11-25, report 0013), with a local `#[allow(unsafe_code)]` and `SAFETY` comments. Any new exception needs the owner's explicit approval.
- No `unwrap()` / `expect()` in `core` production code; use typed errors (`thiserror`). (Enforced workspace-wide by clippy lints; allowed in tests via `clippy.toml`.)
- No secrets in logs, error messages, tests with real data, or commits.
- Every change in `core` comes with tests.
- Before considering a task done, all of these must pass:
  ```
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cargo deny --locked check   # if cargo-deny is installed; CI always runs it
  # when app/ui or app/package*.json change (run in app/):
  npm ci && npm audit --audit-level=low && npm run check && npm run build
  ```
- No network calls from the app or the native host.
- **Tauri IPC:** every new command must be added to the app manifest in `app/build.rs` and granted explicitly in `app/capabilities/main.json`. Never send passwords or notes to the frontend unless the user explicitly asks to reveal/copy one. Errors reach the UI as codes only. Do not add Tauri plugins or loosen the CSP without asking.
- **Toolchain:** the exact Rust version is pinned in `rust-toolchain.toml`. Bump it deliberately in its own PR (with report) after checking fmt/clippy/tests on the new version. Cargo manifest lints such as `unused_dependencies` are denied in the workspace `Cargo.toml`: do not declare dependencies before they are used.
- **Supply chain:** dependencies only from crates.io (enforced by `deny.toml`); new licenses must be added to `deny.toml` deliberately. GitHub Actions: first-party only, pinned to a full commit SHA with the version in a comment.
- Security over convenience: when in doubt, choose the safer option.
- Code and code comments in English; user documentation (README) in Spanish.
- **Reports (mandatory):** every modification adds a report in `informes/` (Spanish, named `NNNN-YYYY-MM-DD-short-title.md`, following the template in `informes/README.md`), adds it to the index there, and updates the "Avances" section (and the feature table / status line if relevant) of `README.md`. Do this in the same commit as the change.
- **Pull requests:** `main` is protected. Work on a branch and open a PR. Claude may merge its own PR (merge commit) once every CI check is green, cargo-deny/npm audit report nothing, and there are no conflicts or unresolved review comments; otherwise fix it or report to the project owner first.
- When instructions are ambiguous or conflict with the repository, ask before deciding.
