# 0019 — Ventanas asociadas y secuencia de auto-type en el formulario

- **Fecha:** 2026-11-26
- **Fase:** 2
- **Commits:** commit «Auto-type windows and sequence in the entry form» (PR hacia `main`)

## Resumen
Quinto paso de la Fase 2. El formulario de entradas tiene una sección **«Auto-type en programas»** para:
- asociar ventanas (título y programa);
- cambiar la secuencia que se escribe.

Con esto, el auto-type ya se puede usar de principio a fin y probar a mano en Windows.

## Cambios

**Interfaz (`EntryForm.svelte`)**
- **Lista de ventanas asociadas,** con «Añadir ventana» y «Quitar». Cada una tiene:
  - **Título de la ventana:** `*` vale por cualquier texto, por ejemplo `Banco*`.
  - **Programa:** el nombre del ejecutable, por ejemplo `programa.exe`.
- **Aviso en las asociaciones que solo tienen título:** cualquier programa o web puede imitar un título, así que se pedirá confirmación antes de escribir. Se recomienda indicar también el programa.
- **Campo «Secuencia»:**
  - vacío significa la secuencia por defecto, `{USERNAME}{TAB}{PASSWORD}{ENTER}`, que se muestra de ejemplo;
  - se comprueba mientras se escribe y explica el fallo y en qué carácter está (llave sin cerrar, marcador desconocido, pausa no válida…);
  - si no hay fallo, muestra la lista de marcadores.

**Backend (`app/src/state.rs`)**
- `EntryForm` y `EntryDetails` incluyen `windows` (título y programa) y `autotype` (vacío = por defecto).
- **Al guardar:**
  - se descartan las asociaciones vacías;
  - se quitan los espacios sobrantes;
  - del programa solo se guarda el nombre del archivo, porque las carpetas no cuentan al comparar (`C:\…\erp.exe` → `erp.exe`);
  - se eliminan las asociaciones repetidas.
- **Una secuencia no válida rechaza el guardado** (`AutoTypeSequenceInvalid`) y no cambia nada.
- **Al editar,** se guardan las ventanas y la secuencia del formulario; antes se conservaban siempre las anteriores, porque el formulario no las mostraba.
- **Comando nuevo `check_autotype_sequence`** (en `app/build.rs` y `capabilities/main.json`): devuelve el motivo del fallo y su posición, sin el texto de la secuencia.

## Decisiones
- **Escribir la ventana y el programa a mano,** sin una lista de las ventanas abiertas para elegir. La lista necesitaría `EnumWindows`, código `unsafe` nuevo fuera de lo aprobado. Se puede añadir más adelante si se aprueba.
- **Guardar solo el nombre del ejecutable.** Así es más claro lo que se compara y no se guardan rutas personales del equipo.

## Verificación
- `cargo fmt --all --check` y `cargo clippy --workspace --all-targets -- -D warnings`: correctos.
- `cargo test --workspace`: todo en verde. `app` pasa de 41 a 44 tests:
  - los campos se guardan limpios y se devuelven;
  - una secuencia vacía vuelve a la de por defecto y las asociaciones se pueden quitar;
  - una secuencia no válida rechaza el alta y la edición sin cambiar nada;
  - comprobación de la secuencia para el formulario.
  - El test anterior, que comprobaba que la edición conservaba estos campos sin mostrarlos, se sustituye por uno que comprueba que lo que devuelve `get`, enviado de vuelta, se conserva.
- En `app/`: `npm ci`, `npm audit --audit-level=low` (0 vulnerabilidades), `npm run check` (0 errores, 0 avisos) y `npm run build`: correctos.
- Capturas del formulario y del selector (tema claro y oscuro) con datos simulados en Chromium: se ven bien.
- `cargo-deny`: no instalado en este entorno; lo ejecuta la CI.
- **Prueba manual en Windows:** pendiente, a cargo del responsable del proyecto (pasos abajo).

## Prueba manual en Windows (pendiente)
1. **Escritura directa:**
   1. Crea una entrada y asocia el programa `notepad.exe`, sin título.
   2. Abre el Bloc de notas, pulsa **Ctrl+Alt+A** y suelta.
   3. Debe escribir usuario, Tab, contraseña e Intro sin preguntar.
2. **Selector:** cambia la asociación para que solo tenga título (`*Bloc de notas*`) y pulsa el atajo. Debe aparecer el selector. Al elegir la entrada, la app se minimiza y escribe en el Bloc de notas.
3. **Llavero bloqueado:** bloquea el llavero y pulsa el atajo. Debe salir el aviso para desbloquear y volver a pulsar.
4. **Interrupciones:**
   - Mantén pulsado Ctrl+Alt más de 2 s: debe cancelar.
   - Cambia de ventana a mitad de una secuencia con `{DELAY 2000}`: debe parar.
5. **Programa como administrador:** abre el Bloc de notas como administrador y pulsa el atajo. Debe salir el error de Windows («no deja escribir»).

## Pendiente / siguientes pasos
- Prueba manual en Windows (arriba).
- Comprobación de los programas asociados (que el ejecutable exista y su firma digital). Necesita código `unsafe` nuevo (`WinVerifyTrust`) y, por tanto, la aprobación del responsable del proyecto.
- Opcional: elegir la ventana de una lista de ventanas abiertas (necesitaría aprobar `EnumWindows`).
