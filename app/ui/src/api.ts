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
}

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
export const deleteEntry = (id: string) => invoke<void>("delete_entry", { id });
export const generatePassword = (options: GeneratorOptions) =>
  invoke<GeneratedPassword>("generate_password", { options });
