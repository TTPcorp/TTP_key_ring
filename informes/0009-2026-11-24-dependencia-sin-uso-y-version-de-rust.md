# 0009 — Dependencia sin uso y versión de Rust fijada

- **Fecha:** 2026-11-24
- **Fase:** 1
- **Commits:** commit «Remove unused dependency, pin Rust 1.100.0, deny unused deps» (PR hacia `main`)

## Resumen
El responsable del proyecto ejecutó los tests en **Windows** con Rust 1.100. Al compilar apareció el aviso `unused dependency llavero-platform` en `app/Cargo.toml`. El aviso era correcto: la app declaraba la dependencia de `platform/` pero todavía no la usa.

Ni Claude ni la CI lo habían visto, por dos motivos:
- **Versiones de Rust distintas.** `rust-toolchain.toml` decía solo `stable`, así que cada máquina usaba la versión estable que tuviera instalada: 1.97 en el entorno de Claude y la del runner en la CI. Este aviso es nuevo en Rust 1.100.
- **`clippy -- -D warnings` no cubre los avisos del `Cargo.toml`**, solo los del código. Aunque se hubiera usado la misma versión, la CI no habría fallado.

## Cambios
- **`app/Cargo.toml`:** se quita `llavero-platform`. Se volverá a añadir cuando la app lo use (portapapeles, auto-type). `Cargo.lock` se actualiza en consecuencia.
- **`rust-toolchain.toml`:** se fija la versión exacta, **`1.100.0`**, en lugar de `stable`. `rustup` la descarga sola. Así la CI, el equipo y Claude compilan con el mismo compilador y ven los mismos avisos.
- **`Cargo.toml` (workspace):** `[workspace.lints.cargo] unused_dependencies = "deny"`. Una dependencia sin usar ahora **hace fallar la compilación**, también en la CI.
- **`CLAUDE.md`:**
  - La versión de Rust se sube a propósito, en un pull request propio y con su informe.
  - No se declaran dependencias antes de usarlas.
- **README:** requisitos actualizados.

- **`.github/dependabot.yml`:** Dependabot ignora TypeScript 7 o superior.
  - Abrió [TTPcorp/TTP_key_ring#4](https://github.com/TTPcorp/TTP_key_ring/pull/4) para subir TypeScript de 6.0.3 a 7.0.2, y el check `frontend` falló.
  - El motivo: `svelte-check` 4 solo admite TypeScript 5 y 6. Por eso se fijó la 6 en el informe 0008.
  - La CI funcionó como debía: bloqueó una actualización incompatible.
  - La regla se quitará cuando `svelte-check` admita TypeScript 7.

## Decisiones
- **Versión exacta en lugar de `stable`:** se gana reproducibilidad (todos ven los mismos avisos y errores) a cambio de tener que subir la versión a mano. Rust publica una versión estable cada 6 semanas. Dependabot no actualiza este archivo, así que se hará en un pull request propio cuando toque.
- El proyecto sigue en la «edición estable más reciente» de Rust (2024), como dicen las instrucciones. Lo que se fija es la versión del compilador.

## Verificación
- **Reproducido en Linux con Rust 1.100.0:** aparecía el mismo aviso que en Windows.
- **Prueba de la regla nueva:** al volver a añadir la dependencia, la compilación falla con `error: unused dependency`. Tras quitarla, compila sin avisos.
- **Comprobaciones con Rust 1.100.0:** `cargo fmt --check`, `cargo clippy -D warnings` (sin lints nuevos), `cargo test` (62 + 2 del `core`, 6 de la app y 1 de `platform`) y `cargo deny --locked check`, todo sin errores.
- **Gracias a la prueba en Windows del responsable del proyecto:** los tests también se compilan y ejecutan allí fuera de la CI.

## Pendiente / siguientes pasos
- **Cerrar el pull request #4 de Dependabot sin fusionarlo** (tarea del responsable del proyecto).
- Formulario de entradas (alta, edición y borrado) con el generador integrado.
