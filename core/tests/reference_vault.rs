//! Opens a committed reference vault to detect accidental format changes.
//!
//! The file contains only dummy data. To regenerate it (only when the format
//! version changes on purpose), delete it and run:
//! `cargo test -p llavero-core --test reference_vault -- --ignored`

use std::path::PathBuf;

use llavero_core::crypto::KdfParams;
use llavero_core::model::EntryInput;
use llavero_core::vault::Vault;
use secrecy::{ExposeSecret, SecretString};

const PASSWORD: &str = "contraseña-de-prueba-1";

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/reference-v1.vault")
}

#[test]
fn opens_reference_vault_v1() {
    let vault = Vault::open(fixture(), &SecretString::from(PASSWORD)).unwrap();
    assert_eq!(vault.data().schema(), 1);
    let entries = vault.data().entries();
    assert_eq!(entries.len(), 2);

    let web = &entries[0];
    assert_eq!(web.name(), "Ejemplo web");
    assert_eq!(web.username(), "usuario@example.com");
    assert_eq!(web.password().expose_secret(), "Prueba-ñ€-123");
    assert_eq!(web.urls(), ["https://example.com/login"]);
    assert_eq!(web.notes().expose_secret(), "Nota de prueba");

    let app = &entries[1];
    assert_eq!(app.name(), "Ejemplo programa");
    assert_eq!(app.windows()[0].process.as_deref(), Some("ejemplo.exe"));
    assert_eq!(app.autotype(), Some("{USERNAME}{TAB}{PASSWORD}{ENTER}"));
}

#[test]
fn reference_vault_rejects_wrong_password() {
    assert!(Vault::open(fixture(), &SecretString::from("otra")).is_err());
}

#[test]
#[ignore = "generates the committed fixture; run only on purpose"]
fn generate_reference_vault_v1() {
    let path = fixture();
    let kdf = KdfParams::new(KdfParams::MIN_M_COST_KIB, KdfParams::MIN_T_COST, 1).unwrap();
    let mut vault = Vault::create(&path, &SecretString::from(PASSWORD), kdf).unwrap();

    let mut web = EntryInput::new(
        "Ejemplo web",
        "usuario@example.com",
        SecretString::from("Prueba-ñ€-123"),
    );
    web.urls = vec!["https://example.com/login".into()];
    web.notes = SecretString::from("Nota de prueba");
    vault.data_mut().add_entry(web).unwrap();

    let mut app = EntryInput::new(
        "Ejemplo programa",
        "usuario",
        SecretString::from("Prueba-2"),
    );
    app.windows = vec![llavero_core::model::WindowMatch {
        title: Some("Ejemplo - Acceso".into()),
        process: Some("ejemplo.exe".into()),
    }];
    app.autotype = Some("{USERNAME}{TAB}{PASSWORD}{ENTER}".into());
    vault.data_mut().add_entry(app).unwrap();
    vault.save().unwrap();
    std::fs::remove_file(path.with_extension("vault.bak")).unwrap();
}
