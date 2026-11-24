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

/** Error codes returned by the backend (see AppError in commands.rs). */
export type ErrorCode =
  | "NotAVault"
  | "UnsupportedVersion"
  | "Damaged"
  | "WrongPasswordOrCorrupted"
  | "UnsupportedSchema"
  | "EmptyPassword"
  | "AlreadyExists"
  | "Locked"
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
