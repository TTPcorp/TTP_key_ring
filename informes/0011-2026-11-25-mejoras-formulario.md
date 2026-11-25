# 0011 — Modo oscuro, generador por entropía, barra de seguridad (también al escribir), borrado con contraseña y comprobación de webs

- **Fecha:** 2026-11-25
- **Fase:** 1
- **Commits:**
  - «app: dark mode, entropy targets, strength bar, password-confirmed delete, offline URL checks»
  - «Live strength estimate while typing (zxcvbn + random-stretch entropy)»
  - Ambos en el mismo PR hacia `main`.

## Resumen
Mejoras pedidas por el responsable del proyecto tras probar la app:
- Selector de tema: claro, oscuro o el del sistema.
- Generador por objetivo de entropía (64, 128 o 256 bits). Por defecto, 128 bits.
- Barra de seguridad de color: de rojo a verde, y azul con un escudo cuando la seguridad es muy alta.
- **La barra también aparece mientras se escribe una contraseña**, tanto en el formulario de entradas como al crear el llavero.
- Para borrar una entrada hay que escribir la contraseña maestra.
- Comprobación sin conexión de las direcciones web.

Lo que no tiene sentido hacer ahora se ha añadido a la hoja de ruta (ver «Pendiente»).

| Formulario (claro) | Formulario (oscuro) | Borrar con contraseña |
|---|---|---|
| ![Claro](img/0011-formulario-claro.png) | ![Oscuro](img/0011-formulario-oscuro.png) | ![Borrar](img/0011-borrar.png) |

Barra con seguridad media (64 bits): ![Barra media](img/0011-barra-media.png)

| Al escribir: contiene datos de la entrada | Al crear el llavero |
|---|---|
| ![Datos de la entrada](img/0011-escribiendo.png) | ![Crear llavero](img/0011-crear-llavero.png) |

*Capturas de la interfaz compilada en Chromium con la misma CSP que la app en producción, el backend de Tauri simulado y datos de prueba.*

## Cambios

**`core` (con tests)**
- **`generator::length_for_entropy`:** calcula la longitud mínima que alcanza un número de bits con los tipos de carácter elegidos.
  - Si se exige un carácter de cada tipo, pide 1 bit de margen, porque esa regla reduce un poco la entropía real. Así la contraseña nunca se queda por debajo del objetivo.
  - Si el objetivo no se puede alcanzar con la longitud máxima (256), devuelve un error en lugar de una contraseña más corta.
  - Con los cuatro tipos de carácter: 64 bits → 11 caracteres, 128 → 20, 256 → 40.
  - Nueva constante `DEFAULT_TARGET_BITS = 128`.
- **`Vault::verify_password`:** comprueba una contraseña contra el archivo del disco con la derivación Argon2 completa. Es tan lenta como abrir el llavero, así que no facilita los ataques por fuerza bruta.
- **`web` (módulo nuevo): `check_web_address`**, sin ninguna conexión de red.
  - **Normaliza** la dirección: minúsculas, `https://` si falta y dominios internacionales en formato `xn--`.
  - **Rechaza:**
    - Textos que no son una web (con espacios, extensión final inválida, etc.).
    - Esquemas distintos de http/https (`javascript:`, `file:`, `data:`, `ftp:`…).
    - Direcciones con `usuario@` delante del dominio (`https://banco.example@malo.example`), un truco típico de suplantación.
  - **Avisa, sin bloquear el guardado, de:**
    - `http` sin cifrar.
    - Direcciones IP.
    - Dominios con caracteres que pueden imitar a otra web (`xn--`, por ejemplo una «а» cirílica en lugar de una «a»).
    - Nombres sin dominio (`intranet`).
    - Nombres de programa (`notepad.exe`).
  - **Usa el crate `url`** (estándar WHATWG), sin análisis escrito a mano. Ya estaba en el árbol de dependencias a través de Tauri, con licencias ya aprobadas. Se añade al `core` porque la coincidencia exacta de dominio de la Fase 3 también lo necesitará.

- **`strength` (módulo nuevo): `estimate`**, una estimación de la fuerza de una contraseña escrita a mano.
  - **Método híbrido:**
    - **Patrones** (contraseñas comunes, palabras, nombres, fechas, teclado, repeticiones, secuencias y datos de la propia entrada): cuentan lo que estima **zxcvbn**, el estimador de referencia que también usan KeePassXC y Dropbox.
    - **Tramos aleatorios:** cuentan su entropía real, longitud × log2 del conjunto de caracteres usado.
  - **Por qué no zxcvbn sola:**
    - Tiene un techo de 64 bits.
    - Trata los caracteres aleatorios como si fueran dígitos: una contraseña aleatoria de 20 caracteres le sale con 64 bits en vez de unos 130.
    - Cuando un tramo pasa de 2^64, deja de buscar patrones. Para evitarlo, ese tramo se divide por el separador más cercano al centro y cada mitad se analiza por separado; así una frase de cuatro palabras no se confunde con texto aleatorio.
  - **Límites:** un patrón nunca cuenta más que el mismo texto tratado como aleatorio. Solo se analizan los 100 primeros caracteres (protección de zxcvbn frente a entradas enormes); el resto cuenta como aleatorio.
  - **Valores orientativos:**

    | Contraseña | Estimación |
    |---|---|
    | `Password123!` | unos 14 bits, «muy común» |
    | `Santander2024!` en una entrada «Banco Santander» | muy baja, «contiene datos de la entrada» |
    | `correct horse battery staple` | unos 61 bits |
    | Aleatoria de 20 caracteres | unos 131 bits |

  - **Dependencia nueva: `zxcvbn` 3** (sin las funciones por defecto). Trae 4 crates más: `fancy-regex`, `itertools`, `lazy_static` y `either`. Todas son MIT o Apache-2.0, todas de crates.io y no añaden duplicados. `cargo deny` pasa. No es criptografía: solo estima.

**App (`app/src/`)**
- `generate_password` acepta `targetBits` y devuelve también la longitud usada.
- `delete_entry` exige la contraseña maestra.
  - Es un comando `async`, porque ejecuta Argon2.
  - Si la contraseña es incorrecta, no se borra nada.
- `add_entry` y `update_entry` comprueban y normalizan las direcciones web, y quitan las duplicadas. Una dirección inválida impide guardar, con un código de error concreto (`NotAWebAddress`, `UnsupportedScheme` o `EmbeddedCredentials`).
- Comando nuevo `check_url`, para avisar mientras se escribe. Está declarado en `build.rs` y concedido en `capabilities/main.json`.
- Comando nuevo `estimate_strength`, también declarado y concedido de forma explícita.
  - Es `async`, para no bloquear la ventana.
  - Recibe la contraseña que se está escribiendo (no la guarda) y el nombre, el usuario y las webs de la entrada.
  - Con esos campos forma la lista de palabras a evitar: los campos completos y sus trozos de 4 o más caracteres, sin `www`, `https`, etc.

**Interfaz (`app/ui/`)**
- **Modo oscuro:**
  - Selector «Sistema / Claro / Oscuro» arriba a la derecha.
  - La elección se guarda en el almacenamiento local de la ventana. Es una preferencia de visualización, nunca un secreto.
  - Se aplica antes de pintar la página, para que no haya un parpadeo con el tema equivocado.
- **Generador:**
  - Selector «Seguridad» con 64 bits, 128 bits (recomendado, el valor por defecto) o 256 bits, o bien «Longitud fija».
  - Muestra cuántos caracteres ha generado.
- **Barra de seguridad (`StrengthMeter.svelte`):** se llena hasta 128 bits.

  | Bits | Nivel | Color |
  |---|---|---|
  | Menos de 40 | Muy baja | rojo |
  | 40 a 59 | Baja | naranja |
  | 60 a 79 | Media | ámbar |
  | 80 a 99 | Buena | verde claro |
  | 100 a 127 | Alta | verde |
  | 128 o más | Muy alta | azul, con escudo |

  - Es una barra `<progress>` con colores por clase, sin estilos en línea, así que funciona con la CSP estricta.
- **Barra mientras se escribe:**
  - En el formulario de entradas y en la pantalla de crear llavero, se actualiza a los 150 ms de dejar de teclear. Si llegan respuestas fuera de orden, se descartan las antiguas.
  - Al pulsar «Mostrar» en una entrada existente, también se evalúa la contraseña guardada.
  - Indica «(estimación)» y, si encuentra un punto débil, lo explica en una línea («Es una contraseña muy común…», «Contiene el nombre, el usuario o la web de la entrada…»).
  - Las contraseñas generadas siguen mostrando su entropía exacta.
- **Borrar:** abre un cuadro con aviso en rojo que pide la contraseña maestra. El botón «Borrar definitivamente» está desactivado hasta que se escribe algo, y el campo se vacía después de cada intento.
- **Direcciones web:** debajo del campo se muestra, para cada línea:
  - ✓ y el dominio, si todo está bien.
  - ⚠ y los avisos, si hay riesgos.
  - ✗ y el motivo, si no es válida.

## Decisiones
- **«Bits múltiplos de 2» = potencias de 2.** Confirmado por el responsable del proyecto: por defecto 128 bits, y opciones de 64, 128 y 256. La entropía real nunca sale exacta (20 caracteres dan unos 131 bits), así que se garantiza llegar al menos al objetivo.
- **Comprobación de webs sin conexión.** Confirmado por el responsable del proyecto: la app no se conecta a internet, una regla del proyecto que evita revelar a terceros qué webs se guardan. Por eso no puede saber si una web existe o es fraudulenta; solo detecta direcciones mal formadas y trucos conocidos.
- **Los dominios internacionales legítimos** (por ejemplo `españa.es`) también reciben el aviso de caracteres especiales. Es un aviso y no un bloqueo: la app no puede distinguirlos de una imitación sin una lista externa.
- **Barra al escribir: método híbrido** en lugar de contar caracteres. Contar caracteres sobrestimaría muchísimo contraseñas como `Password123!`, y zxcvbn sola infravalora las aleatorias. Se adelanta así a la Fase 1 lo que estaba previsto para la Fase 4.
- **La contraseña que se escribe viaja de la interfaz al proceso de Rust** para estimarla. Es local, nunca sale del equipo y no se guarda. zxcvbn hace copias internas temporales que no se borran de memoria de forma explícita; es un riesgo aceptado, porque la interfaz ya tiene esa misma contraseña en memoria mientras se escribe.
- **Limitación conocida:** los diccionarios de zxcvbn están en inglés. Las frases en español (por ejemplo `MiPerroSeLlamaToby!`, unos 92 bits) salen más fuertes de lo que son. Queda en la hoja de ruta.
- **El tema solo cambia el contenido.** La barra de título de Windows sigue el tema del sistema: cambiarla requeriría un permiso de ventana adicional en Tauri, y las instrucciones piden no ampliar permisos sin preguntar.

## Verificación
- `cargo fmt --all --check` y `cargo clippy --workspace --all-targets -- -D warnings`: correctos.
- `cargo test --workspace`: todo en verde. El `core` pasa de 62 a 82 tests y la app de 15 a 19. Los nuevos cubren:
  - **Longitud por entropía:** valores conocidos, que siempre se alcanza el objetivo, que la longitud es la mínima posible y los límites.
  - **`verify_password`:** contraseña correcta, incorrecta y vacía.
  - **Direcciones web:**
    - Normalización.
    - Rechazos: textos que no son web, esquemas y credenciales.
    - Avisos, incluido un dominio con una «а» cirílica.
  - **Borrado con contraseña:** con una contraseña incorrecta no se borra nada.
  - **Formulario con una dirección inválida:** no se guarda nada.
  - **Estimador de fuerza:**
    - Contraseñas comunes muy débiles, con aviso.
    - Las aleatorias de 64, 128 y 256 bits reciben al menos el 80 % de su entropía real y nunca más del máximo posible.
    - Las frases de palabras quedan muy por debajo de un texto aleatorio de la misma longitud.
    - Los datos de la entrada bajan la puntuación y generan aviso.
    - Cuentan los caracteres más allá de los 100 analizados.
    - Acepta caracteres no ASCII.
    - La lista de palabras de contexto se forma correctamente.
- `cargo deny --locked check`: correcto. `npm ci`, `npm audit --audit-level=low` (0 vulnerabilidades), `npm run check` (0 errores, 0 avisos) y `npm run build`: correctos.
- **Prueba de la barra mientras se escribe** (Playwright, CSP de producción): se han tecleado varias contraseñas carácter a carácter. La barra y los avisos son correctos, solo se hace una consulta por ráfaga de teclas y no hay errores.
- **Prueba de la interfaz** en Chromium con Playwright, la **CSP de producción** y el backend simulado, en tema claro y oscuro. Se ha recorrido generar con 64 y 128 bits, comprobar direcciones, borrar con contraseña incorrecta y luego correcta, y forzar el tema y recargar. Sin errores de consola ni bloqueos de la CSP. La preferencia de tema se mantiene tras recargar.

## Pendiente / siguientes pasos
Añadido a la hoja de ruta del README:
- **Fase 2:** comprobar los programas asociados a una entrada (que el ejecutable exista y, si es posible, su firma digital), con un trait en `platform/`.
- **Fase 3:** lista pública de sufijos (Public Suffix List) incluida en la app, para reconocer el dominio real (`banco.example.co.uk`) y avisar de dominios que no existen.
- **Fase 4:**
  - Diccionarios en español (palabras, nombres y contraseñas comunes) para el estimador de fuerza.
  - Auditoría de contraseñas débiles o repetidas, que reutilizará este estimador.
  - Mover el selector de tema a un menú de ajustes, o convertirlo en iconos (sol, luna, pantalla), para que sea más discreto. Lo pidió el responsable del proyecto.

Siguen pendientes en la Fase 1 la copia al portapapeles con limpieza automática y el bloqueo automático.
