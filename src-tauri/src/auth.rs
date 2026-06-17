use keyring::Entry;

const SERVICE: &str = "ai.mossx.fufei-monitor";
const ACCESS_USER: &str = "fufei-account";
const REFRESH_USER: &str = "fufei-refresh";

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("keyring error: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("no stored token")]
    NotFound,
}

/// Store access + refresh tokens in the macOS Keychain.
pub fn store_tokens(access: &str, refresh: Option<&str>) -> Result<(), AuthError> {
    Entry::new(SERVICE, ACCESS_USER)?
        .set_password(access)?;
    if let Some(r) = refresh {
        Entry::new(SERVICE, REFRESH_USER)?.set_password(r)?;
    }
    Ok(())
}

pub fn get_access_token() -> Result<String, AuthError> {
    Entry::new(SERVICE, ACCESS_USER)?
        .get_password()
        .map_err(|e| match e {
            keyring::Error::NoEntry => AuthError::NotFound,
            other => AuthError::Keyring(other),
        })
}

pub fn get_refresh_token() -> Option<String> {
    Entry::new(SERVICE, REFRESH_USER)
        .ok()?
        .get_password()
        .ok()
}

pub fn clear_tokens() -> Result<(), AuthError> {
    let _ = Entry::new(SERVICE, ACCESS_USER)?.delete_credential();
    let _ = Entry::new(SERVICE, REFRESH_USER)?.delete_credential();
    Ok(())
}
