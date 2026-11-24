# Formato del archivo `.vault` — versión 1

> **Estado:** aprobado (decisiones de §8 aceptadas). Implementado completo en el `core`: criptografía (`core/src/crypto.rs`), contenido (`core/src/model.rs`) y archivo (`core/src/vault/`).
> Cualquier cambio en este documento después de publicar la primera versión de la app exige subir `format_version`.

## 1. Objetivos

1. **Confidencialidad e integridad** de todo el contenido del llavero con primitivas estándar: Argon2id para derivar la clave y XChaCha20-Poly1305 para el cifrado autenticado.
2. **Cabecera autenticada:** cualquier modificación de un solo byte del archivo (cabecera o datos) hace que la apertura falle.
3. **Parámetros endurecibles:** los parámetros del KDF van en la cabecera, de modo que se pueden subir sin cambiar el formato.
4. **Análisis simple y estricto:** cabecera de tamaño fijo, sin campos opcionales ni longitudes variables salvo la de los datos cifrados. Todo lo que no encaje se rechaza.
5. **Evolución controlada:** un número de versión del formato y otro del esquema interno.

Fuera de alcance en la v1: archivos de clave (*key files*), desbloqueo con almacenamiento seguro del SO (Windows Hello/DPAPI) y compresión. Si se añaden, será con una nueva versión del formato.

## 2. Estructura general

```
┌──────────────────────────────┐  offset 0
│ Cabecera (92 bytes, en claro)│  ← se pasa entera como AAD al cifrado
├──────────────────────────────┤  offset 92
│ Datos cifrados               │  ciphertext_len bytes
│ (texto cifrado ‖ tag 16 B)   │
└──────────────────────────────┘  fin de archivo (no se admiten bytes extra)
```

Todos los enteros son **little-endian** y sin signo.

## 3. Cabecera

| Offset | Tamaño | Campo | Valor en v1 / reglas |
|---:|---:|---|---|
| 0 | 8 | `magic` | `4C 4C 41 56 45 52 4F 00` (`"LLAVERO\0"`). Si no coincide → «no es un llavero». |
| 8 | 2 | `format_version` | `1`. Cualquier otro valor → «versión no soportada». |
| 10 | 2 | `flags` | `0`. Reservado; cualquier bit activo → rechazo. |
| 12 | 1 | `kdf_id` | `0x01` = Argon2id, versión 0x13. Otro valor → rechazo. |
| 13 | 1 | `cipher_id` | `0x01` = XChaCha20-Poly1305. Otro valor → rechazo. |
| 14 | 2 | `reserved` | `0`. Otro valor → rechazo. |
| 16 | 4 | `argon2_m_cost` | Memoria en KiB. |
| 20 | 4 | `argon2_t_cost` | Iteraciones. |
| 24 | 4 | `argon2_p_cost` | Paralelismo (carriles). |
| 28 | 32 | `salt` | Sal aleatoria del llavero. |
| 60 | 24 | `nonce` | Nonce de XChaCha20-Poly1305, nuevo en **cada** guardado. |
| 84 | 8 | `ciphertext_len` | Longitud de los datos cifrados, incluido el tag de 16 bytes. |

Tamaño total de la cabecera: **92 bytes**.

`cipher_id` y `kdf_id` solo admiten un valor en la v1. Existen para que un archivo de una versión futura con otro algoritmo se identifique sin ambigüedad. Si alguna vez se usara AES-256-GCM (nonce de 12 bytes), sería un formato nuevo con otra disposición de la cabecera, no un `cipher_id` distinto dentro de la v1.

### 3.1 Límites de los parámetros del KDF

Se validan **antes** de ejecutar Argon2. Así un archivo manipulado no puede forzar un consumo enorme de memoria o CPU, ni rebajar el coste por debajo de lo aceptable.

| Parámetro | Mínimo | Por defecto al crear | Máximo admitido al abrir |
|---|---:|---:|---:|
| `m_cost` (KiB) | 19 456 (19 MiB) | 65 536 (64 MiB) | 4 194 304 (4 GiB) |
| `t_cost` | 2 | 3 | 100 |
| `p_cost` | 1 | 4 | 16 |

- El mínimo sigue la recomendación de OWASP para Argon2id (19 MiB, t=2, p=1).
- Los valores por defecto (64 MiB, t=3, p=4) coinciden con los de gestores de contraseñas habituales y tardan menos de 1 s en un equipo de escritorio actual. Se pueden calibrar más adelante, por ejemplo con un botón «endurecer» que mida el tiempo en el equipo del usuario.
- Además, `m_cost ≥ 8 × p_cost` (requisito de Argon2).

### 3.2 Límite de tamaño

- `ciphertext_len` debe ser ≥ 16 (el tamaño del tag) y ≤ 256 MiB.
- `92 + ciphertext_len` debe ser **exactamente** el tamaño del archivo. Si faltan o sobran bytes → «archivo dañado».

## 4. Criptografía

### 4.1 Derivación de clave

```
key (32 bytes) = Argon2id(
    password = contraseña maestra en UTF-8 (sin normalizar, ver §8),
    salt     = header.salt,
    m, t, p  = header.argon2_*,
    version  = 0x13,
    out_len  = 32,
    secret   = (ninguno), ad = (ninguno)
)
```

La clave resultante se usa directamente como clave de XChaCha20-Poly1305. No hace falta un HKDF porque en la v1 solo hay una clave y un uso. La clave vive en un tipo que se borra al liberarse (`Zeroizing<[u8; 32]>` o `SecretBox`), y lo mismo la contraseña maestra (`SecretString`).

### 4.2 Cifrado

```
ciphertext ‖ tag = XChaCha20-Poly1305.Encrypt(
    key   = key,
    nonce = header.nonce,
    aad   = header[0..92],   // la cabecera completa, ciphertext_len incluido
    msg   = payload (§5)
)
```

- **Nonce:** 24 bytes de `OsRng` en cada guardado. Con un nonce de 192 bits, elegirlo al azar es seguro sin necesidad de contador.
- **Sal:** 32 bytes de `OsRng`. Se genera al crear el llavero y **se regenera** al cambiar la contraseña maestra o los parámetros del KDF. En los guardados normales se conserva la sal y se reutiliza la clave ya derivada, para no repetir Argon2 cada vez.
- Como la cabecera es AAD, manipular los parámetros, la sal, la versión o los flags hace fallar el descifrado.

### 4.3 Errores al abrir

El descifrado autenticado no permite distinguir entre «contraseña incorrecta» y «datos manipulados», y no se intentará distinguirlos. Los errores tipados serán:

| Error | Causa |
|---|---|
| `NotAVault` | `magic` incorrecto o archivo de menos de 92 bytes. |
| `UnsupportedVersion` | `format_version` desconocido. |
| `InvalidHeader` | `flags` / `reserved` / `kdf_id` / `cipher_id` no válidos, o longitud incoherente. |
| `InvalidKdfParams` | Parámetros fuera de los límites de §3.1. |
| `WrongPasswordOrCorrupted` | Falla la verificación del tag. |
| `InvalidPayload` | El tag es válido pero el contenido no cumple el esquema (§5). No debería ocurrir salvo por un error de programación. |
| `UnsupportedSchema` | `schema` del contenido es más nuevo que el que soporta la app. |

Otros errores de la implementación: `TooLarge` (el contenido supera el máximo al guardar), `EmptyPassword` (contraseña maestra vacía), `AlreadyExists` (no se sobrescribe un archivo existente al crear o exportar), `InvalidPath` e `Io` (solo guarda el tipo de error del sistema de archivos, nunca la ruta).

Ningún mensaje de error incluye contraseñas, claves, contenido descifrado ni la ruta completa del archivo en los logs.

## 5. Contenido cifrado (payload)

**Propuesta:** JSON en UTF-8, serializado con `serde` / `serde_json`.

- **Ventajas:** fácil de depurar con datos de prueba, ampliamente usado, y deja que el esquema evolucione con campos opcionales.
- **Limitación asumida:** `serde_json` crea copias intermedias que no se borran de forma garantizada. Se mitiga con estas medidas:
  - El búfer descifrado va en `Zeroizing<Vec<u8>>`.
  - Los campos secretos se deserializan directamente en `SecretString`.
  - El búfer serializado antes de cifrar también es `Zeroizing`.

  La protección de la memoria es «lo mejor posible», no absoluta (igual que en otros gestores).

Esquema indicativo (el modelo exacto se fija en la tarea `core::model`):

```json
{
  "schema": 1,
  "vault_id": "uuid-v4",
  "created_at": "2026-11-24T10:00:00Z",
  "modified_at": "2026-11-24T10:00:00Z",
  "entries": [
    {
      "id": "uuid-v4",
      "name": "Ejemplo",
      "username": "usuario",
      "password": "…",
      "urls": ["https://ejemplo.com/login"],
      "windows": [{ "title": "Ejemplo - Login", "process": "ejemplo.exe" }],
      "autotype": "{USERNAME}{TAB}{PASSWORD}{ENTER}",
      "notes": "",
      "created_at": "…",
      "modified_at": "…"
    }
  ]
}
```

Reglas:

- `schema` es la versión del contenido, independiente de `format_version`. Si es mayor que la soportada → `UnsupportedSchema`: la app **no abre** el llavero, para no perder datos al guardarlo sin los campos nuevos.
- Los campos desconocidos se rechazan (`deny_unknown_fields`) por el mismo motivo.
- Las fechas van en UTC (RFC 3339).

## 6. Guardado atómico y copias de seguridad

1. Serializar y cifrar en memoria con un nonce nuevo.
2. Escribir en `llavero.vault.tmp`, en el **mismo directorio** que el llavero, para que el renombrado sea atómico al estar en el mismo volumen.
3. Forzar la escritura a disco con `File::sync_all`.
4. Copiar el archivo actual a `llavero.vault.bak`, sustituyendo el anterior (ver §8).
5. Renombrar `llavero.vault.tmp` → `llavero.vault` con `std::fs::rename`, que en Windows sustituye el destino de forma atómica (`MoveFileExW` con `MOVEFILE_REPLACE_EXISTING`).
6. En Unix, hacer `fsync` del directorio.

Si algo falla antes del paso 5, el llavero original queda intacto y se borra el `.tmp`. Al abrir, un `.tmp` huérfano se ignora y no se lee nunca.

**Exportar copia de seguridad:** copiar el archivo cifrado tal cual a la ruta que elija el usuario, con el aviso de que sin la contraseña maestra la copia no sirve.

Detalles de la implementación:
- **`.bak` sin riesgo de corrupción:** la copia se hace también de forma atómica (`llavero.vault.bak.tmp`, que luego se renombra a `.bak`). Si falla, no se toca el llavero.
- **Permisos en Unix:** los archivos que se escriben son solo legibles por su propietario (`0600`). En Windows se heredan los permisos de la carpeta del perfil del usuario.
- **Crear no sobrescribe nunca:** si ya existe un archivo en esa ruta, crear el llavero falla.
- **Exportar no sobrescribe:** exportar a una ruta existente también falla. Además, la copia corresponde al último estado **guardado**, sin los cambios pendientes.
- **Cambio de contraseña maestra:** exige la contraseña actual, que se comprueba descifrando el archivo del disco. Genera una sal nueva, permite cambiar los parámetros del KDF y guarda de inmediato.
- **Tamaño máximo:** al abrir, un archivo de más de 92 bytes + 256 MiB se rechaza sin leerlo.
- **Pendiente:** no hay bloqueo contra dos instancias de la app escribiendo el mismo llavero a la vez.

## 7. Evolución del formato

- `format_version` cambia solo si cambia la disposición binaria o la criptografía.
- `schema` cambia si cambia la estructura del contenido.
- Al abrir un archivo de una versión anterior soportada, la app lo migra en memoria y lo guarda en la versión actual tras avisar al usuario (la copia `.bak` conserva la versión antigua).
- No se escriben versiones antiguas (no hay *downgrade*).

## 8. Decisiones (aceptadas)

Todas las propuestas siguientes se aceptaron tal como están.

1. **Normalización Unicode de la contraseña maestra.**
   - **Propuesta:** no normalizar (como KeePassXC). Normalizar a NFC evitaría problemas si la misma contraseña se teclea en sistemas que generan secuencias Unicode distintas (por ejemplo, macOS frente a Windows con tildes), pero añade una dependencia y cambia la contraseña efectiva.
   - Si se quiere NFC, hay que decidirlo **antes** de la v1.
2. **Formato del contenido:** JSON (propuesto) frente a un formato binario como CBOR. La seguridad no cambia, porque todo va cifrado y autenticado.
3. **Copia `.bak` automática** del estado anterior en cada guardado.
   - **Propuesta:** sí, una sola copia. Protege frente a errores de la app, aunque también conserva una versión anterior de las contraseñas en disco, cifrada con la misma clave.
4. **Relleno (*padding*) del contenido** para ocultar el número aproximado de entradas.
   - **Propuesta:** no en la v1. El tamaño del archivo solo revela de forma aproximada cuántos datos hay.
5. **Parámetros por defecto del KDF** (64 MiB, t=3, p=4) frente a algo más alto, como 256 MiB. Más memoria encarece los ataques por fuerza bruta, pero ralentiza el desbloqueo en equipos modestos.

## 9. Dependencias criptográficas

| Crate | Para qué | Notas |
|---|---|---|
| `argon2` 0.6 | Argon2id (derivación de clave) | RustCrypto. Sin la función `password-hash` (no se necesita formato PHC). Con `zeroize`. |
| `chacha20poly1305` 0.11 | XChaCha20-Poly1305 | RustCrypto. Con `zeroize` para borrar la clave interna del cifrador. |
| `rand` 0.10 | Sal y nonce | Solo `SysRng` (generador del SO, vía `getrandom`), sin generadores en espacio de usuario. |
| `zeroize` / `secrecy` | Borrado de secretos en memoria | `secrecy` para la contraseña maestra (`SecretString`). |
| `thiserror` | Errores tipados | No es criptografía. |
| `serde` / `serde_json` | Contenido en JSON (§5) | No es criptografía. El JSON se escribe en un búfer de tamaño exacto que se borra al liberarse. |
| `uuid` | Identificadores de llavero y entradas | Sin generador propio: los UUID v4 se crean con bytes de `SysRng`. |
| `time` | Fechas RFC 3339 en UTC | No es criptografía. |

En compilaciones de depuración, `argon2` y `blake2` se compilan optimizados (`[profile.dev.package.*]` en el `Cargo.toml` raíz); si no, cada derivación tardaría varios segundos en los tests y en la app en modo desarrollo.

Medición con los parámetros por defecto, en compilación optimizada y en un equipo Linux de 4 núcleos: unos 140 ms con p=4 y unos 130 ms con p=1. Como el coste es bajo, no se activa la opción `parallel` de `argon2`, que añadiría la dependencia `rayon`.

Los vectores de regresión de los tests (Argon2id y XChaCha20-Poly1305) se han contrastado con implementaciones independientes: argon2-cffi (implementación de referencia en C) y libsodium.

## 10. Verificación

Cuando se implemente, `core` incluirá:

- Ida y vuelta: crear → guardar → abrir con la misma contraseña.
- Contraseña incorrecta → `WrongPasswordOrCorrupted`.
- Cambiar un byte en cualquier posición de la cabecera o del contenido → error.
- Archivos truncados o con bytes extra → error.
- Parámetros del KDF fuera de límites → `InvalidKdfParams`, sin ejecutar Argon2.
- Dos guardados seguidos producen nonces distintos.
- Un **vector de prueba fijo** (archivo `.vault` de ejemplo con una contraseña de prueba conocida y datos ficticios) en `core/tests/data/`, para detectar cambios accidentales del formato. Ya existe: es `core/tests/data/reference-v1.vault` y lo comprueba `core/tests/reference_vault.rs`.
