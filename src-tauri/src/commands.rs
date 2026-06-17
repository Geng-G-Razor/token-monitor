use crate::api::{self, ApiError, StatsData, Subscription};
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
pub async fn login(email: String, password: String) -> CmdResult<bool> {
    let resp = api::login(&email, &password).await?;
    auth::store_tokens(&resp.access_token, resp.refresh_token.as_deref())?;
    Ok(true)
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

    match api::fetch_stats(&access).await {
        Ok(data) => Ok(data),
        Err(ApiError::Unauthorized) => {
            if let Some(refresh) = auth::get_refresh_token() {
                let new = api::refresh(&refresh).await?;
                auth::store_tokens(&new.access_token, new.refresh_token.as_deref())?;
                Ok(api::fetch_stats(&new.access_token).await?)
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

    match api::fetch_subscriptions(&access).await {
        Ok(data) => Ok(data),
        Err(ApiError::Unauthorized) => {
            if let Some(refresh) = auth::get_refresh_token() {
                let new = api::refresh(&refresh).await?;
                auth::store_tokens(&new.access_token, new.refresh_token.as_deref())?;
                Ok(api::fetch_subscriptions(&new.access_token).await?)
            } else {
                Err(CommandError::NotLoggedIn)
            }
        }
        Err(e) => Err(e.into()),
    }
}
