//! Portable core of LLavero Digital.
//!
//! Contains the vault file format, encryption, entry model, password
//! generator, password strength estimate, web address checks and
//! search/matching logic. This crate must never depend on
//! OS-specific APIs or on Tauri.

pub mod crypto;
pub mod generator;
pub mod model;
pub mod strength;
pub mod vault;
pub mod web;
