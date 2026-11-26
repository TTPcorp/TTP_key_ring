# 0016 — Ventana activa y escritura simulada en Windows (auto-type)

- **Fecha:** 2026-11-26
- **Fase:** 2
- **Commits:** commit «Active window and guarded simulated typing on Windows» (PR hacia `main`)

## Resumen
Segundo paso de la Fase 2. `platform/` ya sabe hacer dos cosas en Windows:
- **Leer la ventana activa:** su título y la ruta del programa.
- **Escribir en ella** texto, Tab e Intro, como si se tecleara.

La escritura se detiene en el acto si cambia la ventana activa o si se pulsa Mayús, Ctrl, Alt o Win. Todavía no hay atajo de teclado ni botón en la app: eso son los pasos siguientes.

## Cambios

**`platform/src/autotype.rs` (nuevo)**
- **Trait `AutoType`**, con la implementación real `SystemAutoType`:
  - `active_window()` devuelve un identificador opaco, el título y la ruta del ejecutable (si Windows deja leerla).
  - `modifiers_pressed()` dice si hay Mayús, Ctrl, Alt o Win pulsadas.
  - `type_text(ventana, texto)` y `press_key(ventana, tecla)` escriben solo en la ventana indicada.
- **Protecciones antes de cada carácter o tecla:**
  - La ventana activa tiene que seguir siendo la de destino. Si no, error `WindowChanged` y no se escribe nada más.
  - No puede haber ningún modificador pulsado. Si no, error `ModifierPressed`, para que el texto no se convierta en atajos de teclado (por ejemplo, Ctrl + letra).
- **Pausa de 10 ms entre caracteres** (como KeePass), porque algunos programas pierden teclas si llegan demasiado rápido.
- **Errores sin datos:** `Unsupported`, `WindowChanged`, `ModifierPressed` y `Blocked` (Windows rechaza la escritura, por ejemplo, si el programa se ejecuta como administrador). Nunca incluyen el texto.

**`platform/src/windows.rs`**
- **Ventana activa:** `GetForegroundWindow` y `GetWindowTextW` para el título. Para la ruta del programa, `GetWindowThreadProcessId`, `OpenProcess` con el permiso mínimo (`PROCESS_QUERY_LIMITED_INFORMATION`) y `QueryFullProcessImageNameW`.
- **Escritura:** `SendInput` con `KEYEVENTF_UNICODE`. Escribe el carácter exacto sea cual sea la distribución del teclado (tildes, ñ, €, emojis). Tab e Intro se envían como teclas.
- **Modificadores:** `GetAsyncKeyState`.
- Los eventos de teclado y el carácter en UTF-16 se **borran de memoria** (`zeroize`) después de enviarlos.

**Otros archivos**
- **`linux.rs` / `macos.rs`:** el auto-type devuelve `Unsupported` y no hay ventana activa (port en la Fase 4).
- **`platform/Cargo.toml`:** se activan tres módulos más de `windows-sys`, que ya era una dependencia del proyecto. No hay dependencias nuevas.
- **`CLAUDE.md`:** se documenta el alcance de la excepción `unsafe` aprobada.

## Decisiones
- **Código `unsafe` propio en lugar de librerías de terceros, aprobado por el responsable del proyecto (2026-11-26).** Se descartaron `enigo` (teclado), `global-hotkey` (atajo) y crates de ventana activa. Motivo: el responsable prefiere que el código `unsafe` sea nuestro, para tenerlo controlado al 100 %.
  - **La aprobación cubre** las llamadas a la API de Windows del auto-type: ventana activa, escritura simulada y el futuro atajo global.
  - **Cómo se acota:**
    - Solo en `platform/src/windows.rs`, con `#[allow(unsafe_code)]` función a función.
    - Un comentario `SAFETY` en cada bloque.
    - Los identificadores de ventana solo se comparan, nunca se usan para actuar sobre la ventana.
    - Los procesos se abren con el permiso mínimo y se cierran una sola vez.
    - Si algo falla, devuelve «sin datos» o un error; nunca hace fallar la app.
- **La comprobación de ventana y modificadores está en código portable**, separada de las llamadas a Windows. Así se puede probar con un sistema simulado, sin pulsar teclas de verdad.
- **Un carácter cada vez, con su propia comprobación,** en lugar de enviar todo el texto de golpe. Es un poco más lento (10 ms por carácter), pero si el usuario cambia de ventana a mitad, la contraseña deja de escribirse en ese momento.
- **Programas como administrador:** Windows no deja que una app normal escriba en ellos. Se informa con `Blocked`; no se pedirán permisos de administrador.

## Verificación
- `cargo fmt --all --check` y `cargo clippy --workspace --all-targets -- -D warnings`: correctos.
- `clippy` de `core` y `platform` compilando para Windows (`x86_64-pc-windows-msvc`): sin avisos.
- `cargo test --workspace`: todo en verde. `platform` pasa de 3 a 10 tests. Los nuevos cubren, con un sistema simulado:
  - Escribe todos los caracteres (incluidos ñ, € y emoji), con una pausa tras cada uno.
  - Se detiene si cambia la ventana a mitad, y no escribe nada si la ventana no es la de destino desde el principio.
  - Se detiene si se pulsa un modificador.
  - Informa del rechazo de Windows (`Blocked`).
  - Un texto vacío no envía nada.
  - La consulta real de la ventana activa y de los modificadores no falla nunca. En la CI de Windows llama de verdad a la API, pero sin escribir nada.
- Se ha revisado que todo el `unsafe` está en `platform/src/windows.rs`.
- **Prueba manual en Windows:** pendiente. Se hará cuando el auto-type esté conectado a la app, con el atajo global.

## Pendiente / siguientes pasos
- **Atajo de teclado global** en Windows (`RegisterHotKey` en un hilo propio), dentro de la misma aprobación.
- **Comando en la app que une todo:** atajo → ventana activa → entradas asociadas (`core::autotype`) → esperar a que se suelten los modificadores → escribir los pasos.
- Campos de ventanas asociadas y de secuencia en el formulario de entradas.
- Comprobación de los programas asociados (que el ejecutable exista y su firma digital).
