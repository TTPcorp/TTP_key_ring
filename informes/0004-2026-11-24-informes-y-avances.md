# 0004 — Carpeta de informes y seguimiento de avances en el README

- **Fecha:** 2026-11-24
- **Fase:** 0
- **Commits:** (este cambio)

## Resumen
A petición del responsable del proyecto, se ha creado la carpeta `informes/` con un informe por cada modificación, y el `README.md` muestra ahora los avances.

## Cambios
- **`informes/README.md`:** convenciones, plantilla e índice.
- **Informes 0001–0003:** recogen los cambios ya hechos.
- **`README.md`:**
  - Estado actual del proyecto.
  - Nueva sección «Avances», con la lista de tareas por fase y enlaces a los informes.
  - Columna de estado actualizada en la tabla de características.
  - Regla de informes en «Cómo contribuir».
- **`CLAUDE.md`:** nueva regla obligatoria: cada cambio añade un informe y actualiza los avances del README.
- **`docs/vault-format.md`:** se ha documentado la medición de tiempos de Argon2 y la decisión de no usar `parallel`.

## Decisiones
- **Formato de nombre** de los informes: `NNNN-AAAA-MM-DD-titulo.md`.
- **Informes en español:** son documentación para personas, igual que el README.

## Verificación
- Solo hay cambios de documentación; `cargo fmt`, `clippy` y `test` siguen sin errores.

## Pendiente / siguientes pasos
- `core::model` y `core::vault`.
