# LLavero Digital

Gestor de contraseñas de escritorio para uso interno. Guarda las credenciales de páginas web y programas de forma **local y cifrada**, y las **rellena automáticamente** en ambos. Permite usar contraseñas largas y únicas sin depender de ningún servicio en la nube: la aplicación no se conecta a internet.

> **Estado:** Fase 0 terminada. Fase 1 (MVP) en marcha. El `core` ya sabe crear, abrir, editar y guardar un llavero cifrado, y la app de escritorio ya permite crear o abrir el llavero, ver y buscar entradas y bloquearlo. Falta poder añadir y editar entradas desde la app. Detalle en [Avances](#avances) y en [`informes/`](informes/).

## Características

Leyenda: ✅ disponible · 🚧 en desarrollo · 📋 prevista

| Característica | Estado |
|---|---|
| Llavero local cifrado (`llavero.vault`) protegido con contraseña maestra | 🚧 Fase 1 — hecho en el `core`; falta la interfaz |
| Alta, edición y borrado de entradas (nombre, usuario, contraseña, URL/programa, notas) | 🚧 Fase 1 — hecho en el `core`; falta la interfaz |
| Generador de contraseñas seguras | 🚧 Fase 1 — hecho en el `core`; falta la interfaz |
| Copia al portapapeles con limpieza automática (15–30 s) | 📋 Fase 1 |
| Bloqueo automático por inactividad o al bloquear la sesión de Windows | 📋 Fase 1 |
| Lista, búsqueda y formulario de entradas | 🚧 Fase 1 — lista y búsqueda hechas; falta el formulario |
| Guardado atómico y copia de seguridad del archivo cifrado | 🚧 Fase 1 — hecho en el `core`; falta la interfaz |
| Auto-type en programas con atajo de teclado global | 📋 Fase 2 |
| Extensión de navegador con autorrelleno y coincidencia exacta de dominio | 📋 Fase 3 |
| Importación desde navegadores (CSV), carpetas y etiquetas | 📋 Fase 4 |
| Auditoría de contraseñas débiles o repetidas | 📋 Fase 4 |
| Soporte para Linux y macOS | 📋 Fase 4 |

## Principios de seguridad

- **Todo local.** Ni la aplicación ni el native host hacen conexiones de red.
- **Sin algoritmos propios.** El formato de archivo es propio, pero las primitivas son estándar y auditadas; no hay criptografía escrita a mano.
- **Derivación de clave:** Argon2id con sal aleatoria por llavero. Los parámetros se guardan en la cabecera del archivo para poder endurecerlos en el futuro.
- **Cifrado autenticado:** XChaCha20-Poly1305 (preferido) o AES-256-GCM, con un nonce aleatorio nuevo en cada guardado.
- **Memoria:** las claves y contraseñas se guardan en tipos que se borran al liberarse (`zeroize` / `secrecy`) y nunca aparecen en logs ni en mensajes de error.
- **Guardado atómico:** se escribe en un archivo temporal y se renombra, para que un fallo a mitad no corrompa el llavero.
- **Antiphishing:** la extensión solo rellena si el dominio coincide exactamente.
- **Seguridad antes que comodidad:** ante la duda, la opción más segura.

> ⚠️ **Sin recuperación.** La contraseña maestra no se guarda en ningún sitio y no hay ninguna forma de recuperarla. **Si pierdes la contraseña maestra o el archivo `llavero.vault`, pierdes tus datos.** Haz copias de seguridad del archivo cifrado con regularidad.

## Arquitectura

```
Extensión navegador  ◄── Native Messaging (local) ──►  App de escritorio (Tauri)
                                                             │
                                                       llavero.vault (cifrado)
```

| Carpeta | Crate | Responsabilidad |
|---|---|---|
| `core/` | `llavero-core` | Lógica pura y portable: formato del llavero, cifrado, modelo de entradas, generador de contraseñas, búsqueda y coincidencia. Sin dependencias del SO ni de Tauri. Con tests unitarios. |
| `platform/` | `llavero-platform` | Traits comunes e implementaciones por sistema (`windows` ahora; `linux` y `macos` en el futuro). |
| `app/` | `llavero-app` | Aplicación Tauri: interfaz y comandos. |
| `native-host/` | `llavero-native-host` | Puente Native Messaging entre la extensión y la app. |
| `extension/` | — | Extensión de navegador (WebExtensions MV3; no es un crate de Rust). |

La especificación del formato del archivo cifrado está en [`docs/vault-format.md`](docs/vault-format.md).

**Regla de portabilidad:** todo lo específico del sistema operativo (atajos globales, simulación de teclado, ventana activa, portapapeles, registro de Native Messaging, almacenamiento seguro del SO) va detrás de un trait en `platform/`, con una implementación por sistema seleccionada con `#[cfg(target_os = "...")]`. El `core` nunca llama a APIs de Windows.

## Requisitos de desarrollo

- **Rust estable** (edición 2024) con `rustfmt` y `clippy`. El archivo `rust-toolchain.toml` los selecciona automáticamente con `rustup`.
- **Requisitos de Tauri 2 en Windows:**
  - [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) (Build Tools de Visual Studio) con la carga de trabajo «Desarrollo para el escritorio con C++».
  - **WebView2** (incluido en Windows 10 actualizado y Windows 11).
- **Node.js** 22 o superior, para la interfaz (Svelte 5 + Vite, en `app/ui/`).
- **Solo en Linux**, las librerías de sistema de Tauri (Ubuntu/Debian): `sudo apt install libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`.
- **cargo-deny** (opcional en local; la CI siempre lo ejecuta): `cargo install cargo-deny --locked`.

## Puesta en marcha

```bash
git clone <url-del-repositorio>
cd TTP_key_ring
```

Compilar:

```bash
cargo build --workspace
```

Ejecutar los tests:

```bash
cargo test --workspace
```

Comprobaciones antes de enviar cambios (las mismas que ejecuta la integración continua en Windows y Linux, `.github/workflows/ci.yml`):

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny --locked check
```

`cargo deny` revisa que ninguna dependencia tenga vulnerabilidades publicadas, que todas las licencias sean permisivas y que todo venga de crates.io (configuración en `deny.toml`).

Comprobaciones de la interfaz (desde la carpeta `app/`):

```bash
npm ci
```

```bash
npm audit --audit-level=low
```

```bash
npm run check
```

`npm ci` instala las versiones exactas de `package-lock.json` sin ejecutar scripts de instalación (lo impide `app/.npmrc`, como protección frente a paquetes maliciosos).

Lanzar la app en modo desarrollo (desde `app/`, después de `npm ci`). Abre la ventana y recarga la interfaz al guardar cambios:

```bash
npm run tauri dev
```

Generar el ejecutable y los instaladores de producción (desde `app/`):

```bash
npm run tauri build
```

La app guarda el llavero en la carpeta de datos del usuario: `%APPDATA%\LLavero Digital\llavero.vault` en Windows. En modo desarrollo usa otra carpeta, `LLavero Digital (desarrollo)`, para que las pruebas nunca toquen el llavero real.

## Avances

Leyenda: ✅ hecho · 🚧 en curso · ⬜ pendiente. Cada avance tiene su informe en [`informes/`](informes/).

**Fase 0 — Preparación**
- ✅ Repositorio y workspace de Cargo ([0001](informes/0001-2026-11-24-esqueleto-workspace.md))
- ✅ README, CLAUDE.md y reglas de contribución ([0001](informes/0001-2026-11-24-esqueleto-workspace.md))
- ✅ Diseño del formato `.vault` ([0002](informes/0002-2026-11-24-ci-y-formato-vault.md), [`docs/vault-format.md`](docs/vault-format.md))
- ✅ CI (build, tests, `clippy`, `fmt`) en Windows y Linux, en verde ([0002](informes/0002-2026-11-24-ci-y-formato-vault.md), [0006](informes/0006-2026-11-24-cierre-fase-0.md))
- ✅ Seguridad de dependencias: `cargo-deny` en la CI (más una revisión semanal) y Dependabot ([0006](informes/0006-2026-11-24-cierre-fase-0.md))
- ✅ Carpeta de informes y seguimiento de avances ([0004](informes/0004-2026-11-24-informes-y-avances.md))
- ✅ Framework de interfaz elegido: Svelte 5 + Vite, sin SvelteKit ([0006](informes/0006-2026-11-24-cierre-fase-0.md))

**Fase 1 — MVP**
- ✅ Cifrado: Argon2id + XChaCha20-Poly1305 en `core::crypto` ([0003](informes/0003-2026-11-24-capa-criptografica.md))
- ✅ Modelo de entradas y búsqueda (`core::model`) ([0005](informes/0005-2026-11-24-modelo-y-archivo-vault.md))
- ✅ Crear/abrir llavero, cabecera del archivo, guardado atómico, `.bak`, exportación y cambio de contraseña maestra (`core::vault`) ([0005](informes/0005-2026-11-24-modelo-y-archivo-vault.md))
- ✅ Generador de contraseñas (`core::generator`) ([0007](informes/0007-2026-11-24-generador-contrasenas.md))
- ⬜ Bloqueo automático y limpieza del portapapeles
- 🚧 Interfaz: app Tauri 2 + Svelte con crear/abrir llavero, lista, búsqueda y bloqueo manual ([0008](informes/0008-2026-11-24-app-tauri-svelte.md)); falta el formulario de entradas

**Fases 2–4:** sin empezar.

## Hoja de ruta

Importancia: **Crítica** (bloquea lo demás o compromete la seguridad) · **Alta** · **Media** · **Baja**.

**Fase 0 — Preparación**
- Crítica: repositorio y workspace de Cargo; diseño del formato `.vault` (cabecera, versión, parámetros KDF).
- Alta: CI (build, tests, `clippy`, `fmt`); README y guía de contribución.
- Media: elegir framework de interfaz.

**Fase 1 — MVP**
- Crítica: cifrado (Argon2id + XChaCha20-Poly1305); crear/abrir llavero con contraseña maestra; alta, edición y borrado de entradas; guardado atómico y copia de seguridad.
- Alta: generador de contraseñas; bloqueo automático y limpieza del portapapeles; interfaz con lista, búsqueda y formulario.

**Fase 2 — Auto-type en programas**
- Crítica: traits de plataforma e implementación para Windows.
- Alta: atajo global; detección de ventana activa; escritura de la secuencia configurable (por defecto: usuario, Tab, contraseña, Enter).
- Media: asociar entradas a ventanas.

**Fase 3 — Extensión de navegador**
- Crítica: native host y su registro; canal seguro app ↔ extensión con emparejamiento; coincidencia exacta de dominio.
- Alta: detección de formularios de login.
- Media: Chrome/Edge primero, luego Firefox.

**Fase 4 — Extras**
- Media: importar desde navegadores (CSV); carpetas y etiquetas.
- Baja: auditoría de contraseñas débiles o repetidas; port a Linux y macOS.

**Transversal:** tests del `core` y revisión de seguridad en cada fase.

## Cómo contribuir

- Respeta las decisiones de plataforma, tecnología, seguridad y arquitectura descritas arriba. Si crees que alguna debería cambiar, propónlo y espera confirmación.
- Ninguna dependencia nueva de criptografía sin justificarla. Nunca se implementan primitivas criptográficas a mano.
- Nada de `unwrap()` / `expect()` en código de producción del `core`; errores tipados con `thiserror`.
- Ningún secreto en logs, mensajes de error, tests con datos reales ni commits.
- Todo cambio en `core` va acompañado de tests. `cargo fmt`, `cargo clippy -- -D warnings` y `cargo test` deben pasar antes de dar una tarea por terminada.
- Ninguna llamada de red desde la app ni desde el native host.
- Código y comentarios en inglés; documentación de usuario en español.
- Cada cambio añade un informe en [`informes/`](informes/) (ver la plantilla en su README) y actualiza la sección [Avances](#avances) de este README.

Como referencia de buenas prácticas (formato de archivo, integración con navegador, auto-type) se puede consultar KeePassXC, pero no se copia su código.

## Licencia

Por definir.
