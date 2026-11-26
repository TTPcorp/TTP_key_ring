# 0018 — Flujo de auto-type en la app: atajo, selector de entradas y avisos

- **Fecha:** 2026-11-26
- **Fase:** 2
- **Commits:** commit «Auto-type flow in the app: hotkey, entry picker and notices» (PR hacia `main`)

## Resumen
Cuarto paso de la Fase 2. La app ya une todas las piezas: al pulsar **Ctrl+Alt+A** en un programa, busca las entradas asociadas a esa ventana y escribe el usuario y la contraseña, o pide elegir la entrada.

Todavía **no se pueden asociar ventanas desde el formulario** de entradas. Es el paso siguiente; hasta entonces, el atajo responderá «Ninguna entrada está asociada a la ventana».

## Cambios

**`core/src/autotype.rs`**
- **`VaultData::autotype_choice`** decide qué hacer con la ventana activa:
  - **Ninguna entrada:** no se escribe nada.
  - **Escribir directamente:** solo si hay **una única entrada** y coincide a través de una asociación que **comprueba el ejecutable**.
  - **Elegir:** en cualquier otro caso (varias entradas, o una que solo coincide por el título).
- **`WindowMatch::checks_process`:** dice si una asociación incluye el ejecutable.

**`app/src/autotype.rs` (nuevo): el flujo al pulsar el atajo**
1. **Se guarda la ventana activa en ese momento.** Es la única en la que se escribirá por esa pulsación.
2. **Si el llavero está bloqueado,** se muestra la app con el aviso: «desbloquea, vuelve a la ventana del programa y pulsa Ctrl+Alt+A otra vez».
3. **Si la ventana es la propia app** o no hay ventana activa, no se hace nada.
4. **Según `autotype_choice`:**
   - escribe directamente;
   - muestra el selector de entradas;
   - o avisa de que no hay entradas asociadas.
5. **Antes de escribir:**
   - espera a que se suelten las teclas del atajo, hasta 2 s; si no, cancela;
   - escribe con las protecciones de `platform` (se para si cambia la ventana o se pulsa un modificador).
6. **Solo un auto-type a la vez:** mientras se escribe, el atajo se ignora.

**El selector de entradas**
- Cada pulsación que necesita elegir crea una **petición de un solo uso**:
  - identificador aleatorio de 24 caracteres, del mismo generador seguro que las contraseñas;
  - caduca a los 30 s.
- **El selector solo puede escribir:**
  - una de las entradas ofrecidas para esa ventana;
  - en la ventana guardada al pulsar el atajo.
  
  La interfaz nunca elige la ventana de destino ni puede iniciar un auto-type por su cuenta.
- **Al elegir:**
  1. la app se minimiza;
  2. se espera, hasta 1,5 s, a que la ventana del programa vuelva a estar delante;
  3. si no vuelve, se cancela sin escribir y la app reaparece con el error.

**Comandos de Tauri nuevos** (en `app/build.rs` y `capabilities/main.json`)
- `autotype_notice`: la interfaz lo consulta cada segundo y al recibir el foco. Devuelve cada aviso una sola vez.
- `autotype_choose`: escribe la entrada elegida.
- `autotype_cancel`: descarta la petición.

**Avisos y datos**
- Nunca viajan contraseñas a la interfaz: solo el nombre y el usuario de las entradas candidatas, y el título de la ventana.
- Errores nuevos, solo como códigos:
  - `AutoTypeSequenceInvalid`, `AutoTypeExpired`;
  - `AutoTypeWindowChanged`, `AutoTypeModifierPressed`;
  - `AutoTypeBlocked`, `AutoTypeUnsupported`.

**`app/src/main.rs`**
- Registra el atajo al arrancar. Su hilo solo pasa la pulsación a un hilo de trabajo, que ejecuta el flujo y muestra la ventana si hay algo que decir.
- **Si otro programa ya usa Ctrl+Alt+A,** la app avisa al abrirse y funciona igual, sin auto-type.

**Interfaz**
- `AutoTypePicker.svelte` (nuevo): ventana modal con las entradas candidatas.
  - Se elige con un clic o con Intro (la primera tiene el foco).
  - Esc cancela.
  - Si solo hay una entrada, recuerda que coincide por el título, que cualquier programa o web puede imitar.
- `App.svelte`: muestra los avisos del auto-type (bloqueado, sin entradas, error, atajo ocupado).

**`platform`:** `WindowId::from_raw` pasa a ser público, para simular ventanas en los tests de la app. Solo envuelve un número y no da acceso a nada: los identificadores solo se comparan.

## Decisiones
- **Escribir sin confirmación solo con una única entrada que coincida por el ejecutable**, aprobado por el responsable del proyecto (2026-11-26). Motivo: el título de una ventana lo puede poner cualquier programa o página web.
- **Llavero bloqueado: avisar y volver a pulsar el atajo (opción A)**, aprobado por el responsable del proyecto (2026-11-26). Se descartó desbloquear y escribir después. Necesitaría `SetForegroundWindow`, código `unsafe` nuevo, y la ventana podría haber cambiado mientras se desbloquea.
- **Volver a la ventana del programa minimizando la app**, en lugar de activarla con la API de Windows. No necesita código `unsafe` nuevo. Si Windows no devuelve el foco a esa ventana, no se escribe nada.
- **Avisos por consulta (cada segundo y al recibir el foco)** en lugar de eventos de Tauri. Los eventos exigen añadir permisos `core:event` a la capacidad; así no se amplían los permisos de la interfaz.

## Verificación
- `cargo fmt --all --check` y `cargo clippy --workspace --all-targets -- -D warnings`: correctos.
- `cargo test --workspace`: todo en verde.
  - **`core`** pasa de 97 a 102 tests: escribir directamente solo con una coincidencia por ejecutable, elegir si solo coincide por el título o hay varias, y sin coincidencias.
  - **`app`** pasa de 28 a 41 tests, con un teclado y un sistema de ventanas simulados:
    - escribe directamente;
    - llavero bloqueado;
    - sin entradas;
    - el selector escribe en la ventana original y la petición es de un solo uso;
    - varias entradas;
    - no escribe si la ventana no vuelve;
    - rechaza peticiones inventadas, caducadas o canceladas y entradas no ofrecidas;
    - falla si el llavero se bloqueó mientras se elegía;
    - cancela si hay teclas pulsadas;
    - secuencia no válida;
    - no hace nada dentro de la propia app;
    - formato de los avisos e identificadores aleatorios.
- En `app/`: `npm ci`, `npm audit --audit-level=low` (0 vulnerabilidades), `npm run check` (0 errores, 0 avisos) y `npm run build`: correctos.
- `cargo-deny`: no instalado en este entorno; lo ejecuta la CI.
- **Prueba manual en Windows:** pendiente. Se hará con los campos del formulario (paso siguiente), que permiten asociar ventanas.

## Pendiente / siguientes pasos
- **Campos de ventanas asociadas y de secuencia en el formulario** de entradas, con comprobación de la secuencia mientras se escribe.
- **Prueba manual en Windows de todo el flujo**, incluido un programa ejecutado como administrador (error `AutoTypeBlocked`).
- Comprobación de los programas asociados (que el ejecutable exista y su firma digital).
