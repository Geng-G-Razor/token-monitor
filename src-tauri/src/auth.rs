use keyring::Entry;

const SERVICE: &str = "com.token-monitor.app";
const ACCESS_USER: &str = "fufei-account";
const REFRESH_USER: &str = "fufei-refresh";
const BASEURL_USER: &str = "fufei-baseurl";
const DEFAULT_BASE_URL: &str = "https://fufei.mossx.ai";

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

pub fn store_base_url(url: &str) -> Result<(), AuthError> {
    Entry::new(SERVICE, BASEURL_USER)?.set_password(url)?;
    Ok(())
}

pub fn get_base_url() -> String {
    Entry::new(SERVICE, BASEURL_USER)
        .ok()
        .and_then(|e| e.get_password().ok())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

pub fn clear_tokens() -> Result<(), AuthError> {
    let _ = Entry::new(SERVICE, ACCESS_USER)?.delete_credential();
    let _ = Entry::new(SERVICE, REFRESH_USER)?.delete_credential();
    let _ = Entry::new(SERVICE, BASEURL_USER)?.delete_credential();
    Ok(())
}
