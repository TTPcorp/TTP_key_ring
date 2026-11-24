# 0003 — Capa criptográfica del `core`

- **Fecha:** 2026-11-24
- **Fase:** 1
- **Commits:** 7ee165d

## Resumen
Se ha implementado `core/src/crypto.rs`: la derivación de la clave con Argon2id y el cifrado autenticado con XChaCha20-Poly1305, según `docs/vault-format.md`. No hay criptografía escrita a mano; solo se conectan crates auditados de RustCrypto.

## Cambios
- **`KdfParams`:** valida los límites del formato antes de ejecutar Argon2.
  - Mínimo: 19 MiB, t=2, p=1.
  - Máximo: 4 GiB, t=100, p=16.
  - Por defecto: 64 MiB, t=3, p=4.
- **`Salt` y `Nonce`:** se generan con el generador aleatorio del sistema operativo. `Nonce` no se puede copiar, para dificultar que se reutilice.
- **`VaultKey`:** la clave se borra de la memoria al liberarse y nunca se muestra (`[REDACTED]`).
- **`encrypt` / `decrypt`:** los datos en claro van en memoria que se borra al liberarse. El espacio del tag se reserva de antemano para no dejar copias sin borrar.
- **`CryptoError`:** errores sin datos. Una contraseña incorrecta y un archivo manipulado dan el mismo error.
- **Dependencias:** `argon2` 0.6, `chacha20poly1305` 0.11, `rand` 0.10 (solo `SysRng`), `secrecy`, `zeroize` y `thiserror`. Están justificadas en la sección 9 de `docs/vault-format.md`.
- **`Cargo.toml` (raíz):** `argon2` y `blake2` se compilan optimizados también en modo depuración.

## Decisiones
- **Sin la opción `parallel` de `argon2`:** se midieron los parámetros por defecto en compilación optimizada (Linux, 4 núcleos):
  - p=4: unos **140 ms** por desbloqueo.
  - p=1: unos 130 ms.

  El coste es muy bajo, así que no compensa añadir `rayon`. Lo aprobó el responsable del proyecto.

## Verificación
- 16 tests nuevos en `core`:
  - Límites de los parámetros.
  - La misma contraseña y la misma sal dan siempre la misma clave.
  - Se detecta la modificación de cualquier byte del texto cifrado, de la cabecera protegida o del nonce.
  - Se rechazan datos truncados o con bytes de más.
  - Dos vectores fijos de regresión.
- Los vectores de Argon2id y XChaCha20-Poly1305 se contrastaron con **implementaciones independientes** (argon2-cffi y libsodium) y coinciden byte a byte.
- `cargo fmt --check`, `cargo clippy -D warnings` y `cargo test` sin errores (Linux).

## Pendiente / siguientes pasos
- `core::model` (entradas) y `core::vault` (cabecera, crear/abrir, guardado atómico, `.bak` y exportación).
