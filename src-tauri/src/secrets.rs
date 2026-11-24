//! Claves de API en el Llavero de macOS (nunca en la base de datos).

use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

const SERVICE: &str = "dev.susurro.app";

pub fn get(account: &str) -> Option<String> {
    get_generic_password(SERVICE, account).ok().and_then(|b| String::from_utf8(b).ok())
}

pub fn set(account: &str, secret: &str) -> anyhow::Result<()> {
    if secret.trim().is_empty() {
        let _ = delete_generic_password(SERVICE, account);
        return Ok(());
    }
    set_generic_password(SERVICE, account, secret.trim().as_bytes())?;
    Ok(())
}

pub fn has(account: &str) -> bool {
    get(account).is_some()
}
