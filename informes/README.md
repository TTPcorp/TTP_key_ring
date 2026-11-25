# Informes de cambios

Cada modificación del proyecto deja aquí un informe. Así se puede seguir qué se ha hecho, por qué y cómo se ha comprobado, sin tener que leer el código ni el historial de git.

## Convenciones

- **Nombre:** `NNNN-AAAA-MM-DD-titulo-corto.md`. `NNNN` es un número correlativo de 4 cifras.
- **Un informe por cambio** (normalmente, uno por commit o grupo de commits de una misma tarea).
- **Idioma:** español.
- **Sin secretos:** nunca se incluyen contraseñas, claves ni datos reales.
- **Índice:** cada informe nuevo se añade a la tabla de abajo, y los avances se reflejan también en el `README.md` principal.

## Plantilla

```markdown
# NNNN — Título

- **Fecha:** AAAA-MM-DD
- **Fase:** 0 / 1 / 2 / 3 / 4
- **Commits:** abc1234

## Resumen
Qué se ha hecho y para qué, en dos o tres frases.

## Cambios
- Archivos o módulos tocados y qué se ha cambiado en cada uno.

## Decisiones
- Decisiones tomadas y su motivo. Indicar si las aprobó el responsable del proyecto.

## Verificación
- Qué se ha comprobado (tests, fmt, clippy, CI, pruebas manuales) y el resultado.

## Pendiente / siguientes pasos
- Lo que queda abierto.
```

## Índice

| Nº | Fecha | Título | Fase |
|---|---|---|---|
| [0001](0001-2026-11-24-esqueleto-workspace.md) | 2026-11-24 | Esqueleto del workspace, README y CLAUDE.md | 0 |
| [0002](0002-2026-11-24-ci-y-formato-vault.md) | 2026-11-24 | Integración continua y especificación del formato `.vault` | 0 |
| [0003](0003-2026-11-24-capa-criptografica.md) | 2026-11-24 | Capa criptográfica del `core` | 1 |
| [0004](0004-2026-11-24-informes-y-avances.md) | 2026-11-24 | Carpeta de informes y seguimiento de avances en el README | 0 |
| [0005](0005-2026-11-24-modelo-y-archivo-vault.md) | 2026-11-24 | Modelo de entradas y archivo `.vault` | 1 |
| [0006](0006-2026-11-24-cierre-fase-0.md) | 2026-11-24 | Cierre de la Fase 0: CI verificada, cargo-deny, Dependabot y framework de interfaz | 0 |
| [0007](0007-2026-11-24-generador-contrasenas.md) | 2026-11-24 | Generador de contraseñas | 1 |
| [0008](0008-2026-11-24-app-tauri-svelte.md) | 2026-11-24 | Primera versión de la app de escritorio (Tauri 2 + Svelte 5) | 1 |
| [0009](0009-2026-11-24-dependencia-sin-uso-y-version-de-rust.md) | 2026-11-24 | Dependencia sin uso y versión de Rust fijada | 1 |
| [0010](0010-2026-11-25-formulario-entradas.md) | 2026-11-25 | Formulario de entradas y generador en la app | 1 |
| [0011](0011-2026-11-25-mejoras-formulario.md) | 2026-11-25 | Modo oscuro, generador por entropía, barra de seguridad (también al escribir), borrado con contraseña y comprobación de webs | 1 |
| [0012](0012-2026-11-25-portapapeles.md) | 2026-11-25 | Copiar usuario y contraseña al portapapeles con borrado automático | 1 |
