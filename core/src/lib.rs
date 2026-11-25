//! Portable core of LLavero Digital.
//!
//! Contains the vault file format, encryption, entry model, password
//! generator, password strength estimate, web address checks, auto-type
//! sequences and window matching, and search. This crate must never depend on
//! OS-specific APIs or on Tauri.

pub mod autotype;
pub mod crypto;
pub mod generator;
pub mod model;
pub mod strength;
pub mod vault;
pub mod web;
