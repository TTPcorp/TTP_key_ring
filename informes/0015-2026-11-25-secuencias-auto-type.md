# 0015 — Inicio de la Fase 2: secuencias de auto-type y asociación con ventanas

- **Fecha:** 2026-11-25
- **Fase:** 2
- **Commits:** commit «Auto-type sequences and window matching in core» (PR hacia `main`)

## Resumen
Primer paso de la Fase 2 (auto-type en programas). Se añade al `core` la parte portable, que no depende de Windows:
- **Qué escribir:** se lee la secuencia de auto-type de una entrada y se convierte en pasos (texto, teclas Tab/Enter y pausas).
- **Dónde escribirlo:** se decide si una entrada está asociada a una ventana concreta (título y ejecutable).

Todavía no se pulsa ninguna tecla: eso es la parte de `platform/` (siguiente paso).

## Cambios

**`core/src/autotype.rs` (nuevo)**
- **Sintaxis de la secuencia** (los nombres no distinguen mayúsculas):

  | Marcador | Significado |
  |---|---|
  | `{USERNAME}` | usuario de la entrada |
  | `{PASSWORD}` | contraseña de la entrada |
  | `{TAB}` / `{ENTER}` | teclas Tab / Intro |
  | `{DELAY n}` | pausa de `n` ms (de 1 a 10 000) |
  | `{{}` / `{}}` | llave `{` / `}` literal (como en KeePass) |
  | cualquier otro texto | se escribe tal cual |

- **Secuencia por defecto:** `{USERNAME}{TAB}{PASSWORD}{ENTER}`, si la entrada no tiene una propia.
- **`Sequence::parse`** valida la secuencia y devuelve un error con la posición si algo está mal: llave sin cerrar, `}` suelta, marcador desconocido, pausa no válida, vacía o de más de 1000 caracteres.
- **`Sequence::resolve`** genera los pasos para una entrada. El texto va en `SecretString`: se borra de memoria al liberarse y no aparece en `Debug`. Un usuario o una contraseña vacíos no generan paso.
- **`WindowMatch::matches`** y **`VaultData::entries_for_window`**: entradas asociadas a la ventana activa.
  - **Título:** sin distinguir mayúsculas; `*` es comodín. Sin `*` tiene que coincidir el título entero.
  - **Programa:** se compara solo el nombre del ejecutable (`ejemplo.exe`), sin distinguir mayúsculas e ignorando la carpeta.
  - Si se rellenan los dos, tienen que coincidir los dos.

**Otros**
- `core/src/model.rs` y `core/src/lib.rs`: comentarios actualizados.
- `docs/vault-format.md`: reglas de los campos `windows` y `autotype`.

## Decisiones
- **Un marcador desconocido es un error, no texto.** Si alguien escribe `{PASWORD}` por error, es mejor que la app se niegue a escribir que teclear ese texto en un formulario de inicio de sesión.
- **Una asociación con título y programa vacíos no coincide con ninguna ventana.** Antes el modelo decía que un campo vacío vale para cualquier ventana; con los dos vacíos, la contraseña podría acabar en cualquier ventana activa. Se elige la opción segura.
- **Título completo por defecto, no «contiene».** «Banco» no debe coincidir con «Phishing Banco falso»; quien quiera algo más flexible usa `*` de forma explícita.
- **Sintaxis compatible con KeePass** para los marcadores básicos y las llaves literales, pensando en la importación de la Fase 4. No se incluyen repeticiones (`{TAB 2}`) ni otras teclas por ahora: se añadirán si hacen falta.
- **Pausa máxima de 10 s** por marcador, para que una secuencia mal escrita no deje la escritura colgada.
- Los errores solo llevan la posición, nunca el texto de la secuencia.
- Sin dependencias nuevas.

## Verificación
- `cargo fmt --all --check` y `cargo clippy --workspace --all-targets -- -D warnings`: correctos.
- `cargo test --workspace`: todo en verde. El `core` pasa de 82 a 97 tests. Los nuevos cubren:
  - Secuencia por defecto (entrada sin secuencia o en blanco) y secuencias propias con texto, pausas y mayúsculas/minúsculas mezcladas.
  - Llaves literales y texto con tildes y símbolos.
  - Campos vacíos que no generan paso.
  - Todos los errores de sintaxis, con su posición, y los límites de longitud y de pausa.
  - Que ni los errores ni el `Debug` de los pasos muestran la contraseña.
  - Coincidencia de título (completo, comodines), de programa (con y sin ruta), de los dos a la vez, y que una asociación vacía no coincide con nada.
  - Búsqueda de entradas por ventana, en el orden del llavero.
- `cargo deny`: no instalado en local; no hay dependencias nuevas y la CI lo ejecuta.

## Pendiente / siguientes pasos
- **Traits de plataforma e implementación para Windows** (prioridad crítica): leer la ventana activa (título y ejecutable), escribir texto y pulsar teclas, y el atajo de teclado global. Necesita decidir con el responsable del proyecto cómo hacerlo en Windows (API de Windows con `unsafe` aprobado, o una librería existente), porque cualquiera de las dos opciones toca las reglas del proyecto.
- Comando en la app que una todo: atajo → ventana activa → elegir entrada → escribir.
- Campos de ventanas asociadas y de secuencia en el formulario de entradas.
- Comprobar los programas asociados (que el ejecutable exista y su firma digital).
