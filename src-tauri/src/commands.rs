use crate::api::{self, ApiError, StatsData, Subscription, UserInfo};
use crate::auth;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("{0}")]
    Api(#[from] ApiError),
    #[error("auth: {0}")]
    Auth(#[from] auth::AuthError),
    #[error("not logged in")]
    NotLoggedIn,
}

impl serde::Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.to_string().as_ref())
    }
}

type CmdResult<T> = std::result::Result<T, CommandError>;

/// Log in with email + password and persist tokens to Keychain.
#[tauri::command]
pub async fn login(email: String, password: String, base_url: String) -> CmdResult<bool> {
    let base = base_url.trim();
    let base = if base.is_empty() { "https://fufei.mossx.ai" } else { base };
    auth::store_base_url(base)?;
    let resp = api::login(base, &email, &password).await?;
    auth::store_tokens(&resp.access_token, resp.refresh_token.as_deref())?;
    Ok(true)
}

/// Log in with a Bearer token directly (no refresh token stored).
#[tauri::command]
pub async fn login_with_token(token: String, base_url: String) -> CmdResult<bool> {
    let base = base_url.trim();
    let base = if base.is_empty() { "https://fufei.mossx.ai" } else { base };
    auth::store_base_url(base)?;
    // Strip "Bearer " prefix if the user pasted it alongside the token
    let token = token
        .strip_prefix("Bearer ")
        .or_else(|| token.strip_prefix("bearer "))
        .unwrap_or(&token)
        .to_string();
    // Validate the token by calling stats — return error on 401.
    match api::fetch_stats(base, &token).await {
        Ok(_) => {
            auth::store_tokens(&token, None)?;
            Ok(true)
        }
        Err(ApiError::Unauthorized) => Err(CommandError::Api(ApiError::Unauthorized)),
        Err(e) => Err(e.into()),
    }
}

/// Log out: clear stored tokens.
#[tauri::command]
pub fn logout() -> CmdResult<bool> {
    auth::clear_tokens()?;
    Ok(true)
}

/// Whether a token is stored (best-effort; validity checked on fetch).
#[tauri::command]
pub fn is_logged_in() -> bool {
    auth::get_access_token().is_ok()
}

// ---- Two data-fetch commands ----------------------------------------------

/// Fetch dashboard stats; auto-refresh the access token once on 401.
#[tauri::command]
pub async fn fetch_stats() -> CmdResult<StatsData> {
    let access = auth::get_access_token().map_err(|_| CommandError::NotLoggedIn)?;
    let base = auth::get_base_url();

    match api::fetch_stats(&base, &access).await {
        Ok(data) => Ok(data),
        Err(ApiError::Unauthorized) => {
            if let Some(refresh) = auth::get_refresh_token() {
                let new = api::refresh(&base, &refresh).await?;
                auth::store_tokens(&new.access_token, new.refresh_token.as_deref())?;
                Ok(api::fetch_stats(&base, &new.access_token).await?)
            } else {
                Err(CommandError::NotLoggedIn)
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Fetch active subscriptions; auto-refresh the access token once on 401.
#[tauri::command]
pub async fn fetch_subscriptions() -> CmdResult<Vec<Subscription>> {
    let access = auth::get_access_token().map_err(|_| CommandError::NotLoggedIn)?;
    let base = auth::get_base_url();

    match api::fetch_subscriptions(&base, &access).await {
        Ok(data) => Ok(data),
        Err(ApiError::Unauthorized) => {
            if let Some(refresh) = auth::get_refresh_token() {
                let new = api::refresh(&base, &refresh).await?;
                auth::store_tokens(&new.access_token, new.refresh_token.as_deref())?;
                Ok(api::fetch_subscriptions(&base, &new.access_token).await?)
            } else {
                Err(CommandError::NotLoggedIn)
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Fetch user info (balance, etc.).
#[tauri::command]
pub async fn fetch_user_info() -> CmdResult<UserInfo> {
    let access = auth::get_access_token().map_err(|_| CommandError::NotLoggedIn)?;
    let base = auth::get_base_url();
    Ok(api::fetch_user_info(&base, &access).await?)
}
