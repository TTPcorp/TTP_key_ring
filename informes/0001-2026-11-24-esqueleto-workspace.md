# 0001 — Esqueleto del workspace, README y CLAUDE.md

- **Fecha:** 2026-11-24
- **Fase:** 0
- **Commits:** e6b5c4a

## Resumen
Se ha creado la estructura inicial del repositorio, que estaba vacío: un workspace de Cargo con los cuatro crates vacíos que compilan, el `README.md` y el `CLAUDE.md` con las reglas de trabajo.

## Cambios
- `Cargo.toml` (raíz): workspace con edición 2024, y lints que prohíben `unsafe`, `unwrap`, `expect`, `panic!` y `dbg!` en todo el workspace.
- **Crates:** en `core/`, `platform/`, `app/` y `native-host/`; se llaman `llavero-core`, `llavero-platform`, `llavero-app` y `llavero-native-host`.
- `platform/src/{windows,linux,macos}.rs`: módulos vacíos que se seleccionan con `#[cfg(target_os)]`.
- `extension/README.md`: marcador para la futura extensión.
- `rust-toolchain.toml`, `clippy.toml` (permite `unwrap`/`expect` solo en tests) y `.gitignore` (excluye `*.vault`).
- `README.md` en español y `CLAUDE.md` en inglés.

## Decisiones
- **Nombres de los paquetes `llavero-*`:** un crate no puede llamarse `core` porque choca con la librería estándar de Rust.
- **`app/` aún no depende de Tauri:** se integrará cuando se elija el framework de interfaz.
- **Lints anti-`unwrap` en todo el workspace,** no solo en `core`: es más estricto de lo pedido y se puede relajar en `app/` o en `platform/windows` si hace falta.

## Verificación
- `cargo build`, `cargo test`, `cargo fmt --check` y `cargo clippy -D warnings` sin errores (Linux).

## Pendiente / siguientes pasos
- CI y especificación del formato `.vault` (informe 0002).
