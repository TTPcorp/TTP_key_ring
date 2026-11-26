# 0020 — Nueva Fase 5 en la hoja de ruta: puesta a prueba

- **Fecha:** 2026-11-26
- **Fase:** 5
- **Commits:** commit «Add phase 5 (testing and polish) to the roadmap» (PR hacia `main`)

## Resumen
A petición del responsable del proyecto se añade una última fase a la hoja de ruta, **Fase 5 — Puesta a prueba**, para preparar la app antes de la primera versión pública: acabado gráfico, facilidad de uso y pruebas beta con usuarios cercanos. Solo cambia la documentación; no hay cambios de código.

## Cambios
- `README.md`: nueva «Fase 5 — Puesta a prueba» en la hoja de ruta, con tres apartados de importancia alta:
  - Preparación gráfica: diseño visual coherente (iconos, colores, tipografía, icono de la app) y accesibilidad, sin recursos cargados de la red.
  - Adaptación para que sea fácil de usar: primer arranque guiado (incluido el aviso de que la contraseña maestra no se puede recuperar), textos y errores claros y ayuda breve.
  - Pruebas beta con usuarios cercanos: instalador para Windows, guías, comentarios enviados a mano (sin telemetría) y corrección de lo encontrado.
- `README.md`, sección «Avances»: «Fases 3–4: sin empezar» pasa a «Fases 3–5: sin empezar».
- `informes/README.md`: la plantilla admite la Fase 5 y el índice incluye este informe.

## Decisiones
- Fase pedida por el responsable del proyecto (2026-11-26).
- Las pruebas beta no añaden telemetría ni envíos automáticos, para respetar la regla de que la app no hace llamadas de red; los comentarios se recogen a mano.
- Queda anotada también, solo en la conversación y como opción futura sin fase asignada, la idea de que el generador respete las reglas de contraseña de cada web.

## Verificación
- Solo cambia documentación en Markdown; la CI se ejecuta en el PR.

## Pendiente / siguientes pasos
- Concretar las tareas de la Fase 5 cuando terminen las Fases 2–4.
