# 0002 — Integración continua y especificación del formato `.vault`

- **Fecha:** 2026-11-24
- **Fase:** 0
- **Commits:** 616f2fc

## Resumen
Se ha añadido la integración continua (CI) en GitHub Actions y el documento que define el formato binario del llavero (`docs/vault-format.md`).

## Cambios
- `.github/workflows/ci.yml`:
  - Comprueba primero el formato con `cargo fmt`.
  - Después compila, pasa `clippy -D warnings` y ejecuta los tests en **Windows y Linux**.
  - Solo tiene permiso de lectura y no usa acciones de terceros.
- `docs/vault-format.md`:
  - Cabecera fija de 92 bytes, protegida por el cifrado (cualquier cambio impide abrir el llavero).
  - Límites de los parámetros de Argon2id.
  - Tipos de error.
  - Contenido en JSON.
  - Guardado atómico con copia `.bak`.
  - Reglas de versionado y tests previstos.
- `.gitignore`: excluye `*.vault.bak`, con una excepción para los archivos de prueba de `core/tests/data/`.
- `README.md`: enlaces al formato y a la CI.

## Decisiones
Las decisiones abiertas de la sección 8 del formato se **aceptaron tal como se propusieron**:
- Sin normalización Unicode de la contraseña maestra.
- Contenido en JSON.
- Una copia `.bak` automática en cada guardado.
- Sin relleno para ocultar el número de entradas.
- Parámetros por defecto de 64 MiB, t=3, p=4.

## Verificación
- El YAML de la CI se ha validado sintácticamente.
- La CI todavía **no se ha ejecutado**: solo arranca con cambios en `main` o en un pull request.

## Pendiente / siguientes pasos
- Abrir un pull request hacia `main` para que la CI se ejecute en Windows.
