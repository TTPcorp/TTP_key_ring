# 0005 — Modelo de entradas y archivo `.vault`

- **Fecha:** 2026-11-24
- **Fase:** 1
- **Commits:** ver el historial de `core/src/vault/` (commit «core: add entry model and .vault file handling»)

## Resumen
El `core` ya puede crear un llavero, abrirlo con la contraseña maestra, añadir, editar, borrar y buscar entradas, y guardarlo de forma atómica con copia `.bak`. También puede exportar una copia de seguridad y cambiar la contraseña maestra. Sigue exactamente `docs/vault-format.md`. Todavía no hay interfaz: es la lógica sobre la que se construirá la app.

## Cambios

**`core/src/model.rs`: el contenido del llavero.**
- **`VaultData`:** el llavero descifrado. Guarda el esquema, el identificador, las fechas y las entradas.
- **`Entry`:** cada entrada tiene nombre, usuario, contraseña, URLs, ventanas asociadas (para auto-type), una secuencia de auto-type opcional, notas y fechas.
  - La **contraseña y las notas** se tratan como secretas: se borran de la memoria al liberarse y nunca aparecen en los logs.
- **Operaciones:** alta, edición y borrado de entradas. Un nombre vacío no se admite.
- **Búsqueda:** no distingue mayúsculas y mira el nombre, el usuario, las URLs y la ventana o el programa. **Nunca busca dentro de contraseñas ni notas.**
- **Campos desconocidos:** se rechazan al leer el contenido, para que una versión antigua de la app no borre datos de una versión más nueva al guardar.

**`core/src/vault/header.rs`: la cabecera de 92 bytes.**
- Escribe y lee la cabecera tal como está definida en el formato.
- Hace todas las comprobaciones de la especificación **antes** de ejecutar Argon2: firma, versión, campos reservados, longitud exacta del archivo y límites de los parámetros.

**`core/src/vault/storage.rs`: escritura en disco.**
- Guardado atómico: archivo `.tmp`, escritura forzada a disco y renombrado.
- La copia `.bak` también se hace de forma atómica.
- En Linux y macOS los archivos se crean con permisos `0600` (solo el propietario puede leerlos).
- Al abrir, un archivo demasiado grande se rechaza sin leerlo.

**`core/src/vault/mod.rs`: el llavero desbloqueado (`Vault`).**
- **Crear:** nunca sobrescribe un archivo existente y no admite contraseña vacía.
- **Abrir.**
- **Guardar:** cada guardado usa un nonce nuevo, y la versión anterior queda en `.bak`.
- **Indicador de cambios sin guardar.**
- **Cambiar la contraseña maestra:** exige la actual y genera una sal nueva; permite endurecer los parámetros.
- **Exportar copia:** no sobrescribe archivos existentes.
- Los errores no contienen contraseñas, datos ni rutas.

**Archivos de apoyo.**
- `core/tests/data/reference-v1.vault` y `core/tests/reference_vault.rs`: un llavero de referencia con **datos ficticios** y una contraseña de prueba. Sirve para detectar cualquier cambio accidental del formato.
- `.gitattributes`: marca `*.vault` como binario, para que git en Windows no lo altere.

**Dependencias nuevas, ninguna criptográfica:**
- `serde` y `serde_json` (contenido en JSON).
- `uuid` (identificadores; se generan con el aleatorio del sistema).
- `time` (fechas en UTC).
- `tempfile`, solo para los tests.

## Decisiones
- **Contraseña maestra vacía:** no se admite. No se impone una longitud mínima: la interfaz mostrará la fortaleza de la contraseña.
- **Las notas se tratan como secretas,** igual que la contraseña, porque pueden contener datos sensibles.
- **Exportar** copia el último estado guardado. Si hay cambios pendientes, la interfaz deberá guardar antes.
- **Cambiar la contraseña maestra** comprueba la actual contra el archivo del disco. Así no hace falta comparar claves en memoria ni añadir dependencias.
- **Tests con los parámetros mínimos** que acepta el formato (19 MiB, t=2, p=1). No hay ninguna puerta trasera de «modo test» en el código de producción.

## Verificación
- **51 tests unitarios y 2 de integración** en `core`, todos en verde. Entre otros, comprueban que:
  - Se puede crear, guardar y volver a abrir un llavero.
  - El archivo en disco no contiene texto en claro.
  - Una contraseña incorrecta o vacía se rechaza.
  - Crear no sobrescribe un archivo existente.
  - Cada guardado usa un nonce nuevo.
  - La copia `.bak` contiene la versión anterior.
  - **Cambiar cualquier byte del archivo** impide abrirlo.
  - Un archivo truncado o con bytes de más se rechaza.
  - Un archivo que no es un llavero se identifica como tal.
  - Un esquema más nuevo se detecta como «versión más nueva».
  - El cambio de contraseña maestra funciona y la exportación no sobrescribe.
  - Los secretos nunca aparecen en la salida de depuración.
  - El llavero de referencia se abre correctamente.
- `cargo fmt --check`, `cargo clippy -D warnings` y `cargo test` sin errores (Linux).
- La CI en Windows sigue sin ejecutarse hasta que haya un pull request hacia `main`.

## Pendiente / siguientes pasos
- **Bloqueo frente a dos instancias** de la app escribiendo el mismo llavero.
- **Generador de contraseñas** (`core::generator`).
- **Elegir el framework de interfaz** para empezar con la app Tauri.
