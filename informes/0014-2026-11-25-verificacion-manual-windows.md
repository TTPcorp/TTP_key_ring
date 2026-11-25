# 0014 — Verificación manual en Windows del portapapeles y del bloqueo automático

- **Fecha:** 2026-11-25
- **Fase:** 1
- **Commits:** commit «informes: record manual Windows verification of clipboard and auto-lock» (PR hacia `main`)

## Resumen
El responsable del proyecto probó en Windows, con la app real, dos funciones que no se podían comprobar desde el entorno de desarrollo (Linux sin escritorio): la copia al portapapeles (informe 0012) y el bloqueo automático (informe 0013). Todo funciona como se esperaba. Se actualizan esos informes.

## Cambios
- **`informes/0012-2026-11-25-portapapeles.md`:** la prueba manual pendiente pasa a constar como correcta.
- **`informes/0013-2026-11-25-bloqueo-automatico.md`:** ídem.
- **`README.md`:** se añade este avance a la Fase 1.
- No hay cambios de código.

## Decisiones
- Se deja constancia en un informe propio, como exige `CLAUDE.md` para toda modificación, aunque solo se toque documentación.

## Verificación
Pruebas manuales hechas por el responsable del proyecto en Windows:
- **Portapapeles:**
  - La contraseña copiada se pega en el Bloc de notas.
  - No aparece en el historial de Win+V.
  - Se borra a los 20 s y al bloquear el llavero.
- **Bloqueo automático:**
  - El llavero se bloquea al pulsar Win+L.
  - También se bloquea tras 5 minutos sin actividad.

## Pendiente / siguientes pasos
- **Fase 2 (auto-type en programas):** se trabajará en otra sesión.
