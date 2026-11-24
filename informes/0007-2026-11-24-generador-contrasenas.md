# 0007 — Generador de contraseñas

- **Fecha:** 2026-11-24
- **Fase:** 1
- **Commits:** commit «core: add password generator» (PR hacia `main`)

## Resumen
El `core` ya genera contraseñas aleatorias seguras y configurables (`core/src/generator.rs`). Los caracteres salen del generador aleatorio del sistema operativo, y el sorteo y la mezcla los hace la librería `rand`, que está auditada.

## Cambios
- **`PasswordOptions`:** las opciones del generador.
  - Longitud: entre 8 y 256 caracteres.
  - Tipos de caracteres: minúsculas, mayúsculas, dígitos y símbolos.
  - Excluir caracteres que se confunden (`I l 1 | O 0 o`).
  - Garantizar al menos un carácter de cada tipo elegido.
- **Valores por defecto:** 20 caracteres de todos los tipos, con un carácter de cada tipo garantizado. Son unos **130 bits de entropía**: imposible de adivinar por fuerza bruta.
- **`generate()`:** devuelve la contraseña como `SecretString` (se borra de la memoria al liberarse y nunca aparece en los logs). La contraseña se construye en memoria que también se borra y que tiene exactamente el tamaño necesario, para no dejar copias sin borrar.
- **`entropy_bits()` y `alphabet_size()`:** estiman la fortaleza para mostrarla en la interfaz.
- **Errores tipados:** longitud fuera de rango, ningún tipo de carácter elegido, longitud menor que el número de tipos, y fallo del generador aleatorio.
- **Sin dependencias nuevas.**

## Decisiones
- **Símbolos:** se usan 28 símbolos (`!#$%&()*+,-./:;<=>?@[]^_{|}~`). Se excluyen las comillas, la barra invertida, el acento grave y el espacio, porque a menudo dan problemas en formularios web, en la línea de comandos y en archivos de configuración. En total el alfabeto tiene 90 caracteres.
- **Mínimo de 8 caracteres:** algunos programas antiguos no admiten más. La interfaz avisará cuando la entropía sea baja.
- **«Un carácter de cada tipo» activado por defecto,** porque muchas webs lo exigen. Reduce la entropía en menos de un bit con la configuración habitual.
- **Nada de muestreo escrito a mano:**
  - El sorteo uniforme de cada carácter (`choose`) y la mezcla final (`shuffle`, el algoritmo de Fisher–Yates) son de `rand`.
  - La librería espera un generador que no falle, pero el del sistema puede fallar. Para no usar el adaptador de `rand`, que provoca un `panic`, hay un adaptador propio: si el generador falla, anota el fallo y se devuelve el error `RandomUnavailable`. La contraseña afectada se descarta.
- **Frases de contraseña (passphrases), del tipo «caballo-grapa-batería-correcto»:** no se incluyen todavía. Necesitarían una lista de palabras con su propia licencia, así que se proponen como mejora posterior.

## Verificación
- **11 tests nuevos.** Comprueban que:
  - Cada contraseña generada contiene todos los tipos elegidos (repetido 200 veces).
  - Las longitudes en los límites se aceptan, y fuera de ellos se rechazan.
  - Solo se usan los tipos elegidos.
  - Elegir ningún tipo da error.
  - Los caracteres confusos se excluyen cuando se pide.
  - Solo salen caracteres ASCII imprimibles y sin comillas.
  - 1000 contraseñas son todas distintas.
  - **Distribución uniforme:** una prueba estadística (chi cuadrado) sobre más de 100 000 caracteres. Se ejecutó 5 veces seguidas sin fallos, con un margen amplio para que no falle por azar.
  - La estimación de entropía es correcta.
  - **Un fallo del generador aleatorio** se notifica como error y no se oculta.
  - La contraseña no aparece en la salida de depuración.
- `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` (62 tests unitarios y 2 de integración en `core`) y `cargo deny --locked check`: sin errores.

## Pendiente / siguientes pasos
- **Frases de contraseña** (opcional, más adelante).
- **Integrar Tauri 2 + Svelte en `app/`:** primera ventana para crear o abrir un llavero y ver la lista.
