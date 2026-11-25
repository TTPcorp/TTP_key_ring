// Typed wrappers around the Tauri commands defined in app/src/commands.rs.
// Passwords travel to Rust once and are never stored on the JS side.

import { invoke } from "@tauri-apps/api/core";

export interface VaultStatus {
  exists: boolean;
  unlocked: boolean;
}

export interface EntrySummary {
  id: string;
  name: string;
  username: string;
  urls: string[];
}

export interface EntryDetails {
  id: string;
  name: string;
  username: string;
  urls: string[];
  hasPassword: boolean;
  hasNotes: boolean;
}

/**
 * Entry form sent to the backend. A `null` password or notes means
 * "not touched": on update the stored value is kept.
 */
export interface EntryForm {
  name: string;
  username: string;
  password: string | null;
  urls: string[];
  notes: string | null;
}

export type SecretField = "password" | "notes";

export interface GeneratorOptions {
  /** Entropy target (64, 128, 256...); `null` means a fixed `length`. */
  targetBits: number | null;
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  excludeAmbiguous: boolean;
}

export interface GeneratedPassword {
  password: string;
  entropyBits: number;
  length: number;
}

export type WebWarning =
  | "notEncrypted"
  | "ipAddress"
  | "lookalikeCharacters"
  | "singleLabelHost"
  | "looksLikeProgram";

export interface WebAddressInfo {
  url: string;
  host: string;
  warnings: WebWarning[];
}

export const webWarningMessages: Record<WebWarning, string> = {
  notEncrypted: "Usa http: la conexión no va cifrada. Comprueba si existe la versión https.",
  ipAddress: "Es una dirección IP, no un nombre de dominio. Asegúrate de que es el equipo correcto.",
  lookalikeCharacters:
    "El dominio tiene caracteres especiales que pueden imitar a otra web (por ejemplo, una «а» cirílica en lugar de una «a»). Comprueba que es la web real.",
  singleLabelHost: "Nombre sin dominio: solo funciona dentro de una red local.",
  looksLikeProgram: "Parece el nombre de un programa, no una dirección web.",
};

export type StrengthWarning =
  | "containsEntryData"
  | "commonPassword"
  | "keyboardPattern"
  | "repeats"
  | "sequence"
  | "dates"
  | "singleWord"
  | "names";

export interface StrengthEstimate {
  bits: number;
  warning: StrengthWarning | null;
}

export interface StrengthContext {
  name: string;
  username: string;
  urls: string[];
}

export const strengthWarningMessages: Record<StrengthWarning, string> = {
  containsEntryData: "Contiene el nombre, el usuario o la web de la entrada: es lo primero que se prueba.",
  commonPassword: "Es una contraseña muy común o muy parecida a una.",
  keyboardPattern: "Sigue un patrón del teclado (como «qwerty»).",
  repeats: "Repite caracteres o bloques (como «aaa» o «abcabc»).",
  sequence: "Contiene secuencias (como «abc» o «1234»).",
  dates: "Contiene fechas o años, fáciles de adivinar.",
  singleWord: "Una sola palabra es fácil de adivinar.",
  names: "Contiene nombres o apellidos comunes.",
};

/** Error codes returned by the backend (see AppError in state.rs). */
export type ErrorCode =
  | "NotAVault"
  | "UnsupportedVersion"
  | "Damaged"
  | "WrongPasswordOrCorrupted"
  | "UnsupportedSchema"
  | "EmptyPassword"
  | "AlreadyExists"
  | "Locked"
  | "EmptyName"
  | "EntryNotFound"
  | "InvalidGeneratorOptions"
  | "NotAWebAddress"
  | "UnsupportedScheme"
  | "EmbeddedCredentials"
  | "ClipboardUnavailable"
  | "NothingToCopy"
  | "Io"
  | "Internal";

const messages: Record<ErrorCode, string> = {
  NotAVault: "El archivo no es un llavero.",
  UnsupportedVersion: "El llavero es de una versión más nueva de la aplicación.",
  Damaged: "El archivo del llavero está dañado.",
  WrongPasswordOrCorrupted: "Contraseña maestra incorrecta o llavero dañado.",
  UnsupportedSchema: "El llavero se creó con una versión más nueva de la aplicación.",
  EmptyPassword: "La contraseña maestra no puede estar vacía.",
  AlreadyExists: "Ya existe un llavero en esa ubicación.",
  Locked: "El llavero está bloqueado.",
  EmptyName: "El nombre de la entrada no puede estar vacío.",
  EntryNotFound: "La entrada ya no existe.",
  InvalidGeneratorOptions:
    "Opciones del generador no válidas: elige al menos un tipo de carácter y una longitud suficiente.",
  NotAWebAddress: "Hay una dirección que no es una web válida.",
  UnsupportedScheme: "Solo se admiten direcciones web http o https.",
  ClipboardUnavailable: "No se ha podido usar el portapapeles. Inténtalo de nuevo.",
  NothingToCopy: "Esta entrada no tiene ese dato.",
  EmbeddedCredentials:
    "Hay una dirección con «usuario@» delante del dominio, un truco típico de suplantación. Quítalo.",
  Io: "No se ha podido leer o escribir el archivo del llavero.",
  Internal: "Error interno de la aplicación.",
};

/** Turns any rejected invoke() into a user-facing Spanish message. */
export function errorMessage(err: unknown): string {
  if (typeof err === "object" && err !== null && "code" in err) {
    const code = (err as { code: string }).code;
    if (code in messages) {
      return messages[code as ErrorCode];
    }
  }
  return messages.Internal;
}

export const vaultStatus = () => invoke<VaultStatus>("vault_status");
export const createVault = (password: string) => invoke<void>("create_vault", { password });
export const unlockVault = (password: string) => invoke<void>("unlock_vault", { password });
export const lockVault = () => invoke<void>("lock_vault");
export const listEntries = (query: string) => invoke<EntrySummary[]>("list_entries", { query });
export const getEntry = (id: string) => invoke<EntryDetails>("get_entry", { id });
/** Only call on an explicit user action ("Mostrar"). */
export const revealSecret = (id: string, field: SecretField) =>
  invoke<string>("reveal_secret", { id, field });
export const addEntry = (entry: EntryForm) => invoke<string>("add_entry", { entry });
export const updateEntry = (id: string, entry: EntryForm) =>
  invoke<void>("update_entry", { id, entry });
/** Requires the master password, as a guard against accidental deletes. */
export const deleteEntry = (id: string, password: string) =>
  invoke<void>("delete_entry", { id, password });
/** Estimates the strength of a password being typed (never stored). */
export const estimateStrength = (password: string, context: StrengthContext) =>
  invoke<StrengthEstimate>("estimate_strength", { password, context });
export const checkUrl = (input: string) => invoke<WebAddressInfo>("check_url", { input });
export const generatePassword = (options: GeneratorOptions) =>
  invoke<GeneratedPassword>("generate_password", { options });
export type CopyField = "username" | "password";
/** Copies in Rust (the value never reaches the page); returns the seconds until it is cleared. */
export const copyToClipboard = (id: string, field: CopyField) =>
  invoke<number>("copy_to_clipboard", { id, field });
