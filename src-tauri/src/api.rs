use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Shared HTTP client. Built once and reused so connections are pooled and
/// keep-alive'd across the periodic polling, instead of being
/// torn down and rebuilt on every request.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("failed to build reqwest client")
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("unauthorized (401)")]
    Unauthorized,
    #[error("api error ({code}): {message}")]
    Api { code: i64, message: String },
    #[error("unexpected status {0}: {1}")]
    Status(u16, String),
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    code: serde_json::Value,
    message: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    pub token_type: Option<String>,
    pub user: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct LoginEnvelope {
    data: LoginResponse,
}

// ---- Dashboard stats types ------------------------------------------------

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct PlatformStat {
    pub platform: String,
    pub total_requests: u64,
    pub total_tokens: u64,
    pub total_actual_cost: f64,
    pub today_requests: u64,
    pub today_tokens: u64,
    pub today_actual_cost: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct StatsData {
    pub total_api_keys: u64,
    pub active_api_keys: u64,
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_tokens: u64,
    pub total_cost: f64,
    pub total_actual_cost: f64,
    pub today_requests: u64,
    pub today_input_tokens: u64,
    pub today_output_tokens: u64,
    pub today_cache_creation_tokens: u64,
    pub today_cache_read_tokens: u64,
    pub today_tokens: u64,
    pub today_cost: f64,
    pub today_actual_cost: f64,
    pub average_duration_ms: f64,
    pub rpm: u64,
    pub tpm: u64,
    #[serde(default)]
    pub by_platform: Vec<PlatformStat>,
}

#[derive(Debug, Deserialize)]
struct StatsEnvelope {
    data: StatsData,
}

// ---- Subscription types ---------------------------------------------------

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Group {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub platform: String,
    pub status: String,
    pub subscription_type: Option<String>,
    pub daily_limit_usd: Option<f64>,
    pub weekly_limit_usd: Option<f64>,
    pub monthly_limit_usd: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Subscription {
    pub id: i64,
    pub starts_at: String,
    pub expires_at: String,
    pub status: String,
    pub daily_usage_usd: Option<f64>,
    pub weekly_usage_usd: Option<f64>,
    pub monthly_usage_usd: Option<f64>,
    pub group: Option<Group>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionsEnvelope {
    #[allow(dead_code)]
    code: i64,
    #[allow(dead_code)]
    message: String,
    data: Vec<Subscription>,
}

// ---- Auth endpoints -------------------------------------------------------

/// Login with email/password. Returns tokens on success.
pub async fn login(base_url: &str, email: &str, password: &str) -> Result<LoginResponse, ApiError> {
    let resp = client()
        .post(format!("{}/api/v1/auth/login", base_url))
        .json(&serde_json::json!({ "email": email, "password": password }))
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::OK {
        serde_json::from_str::<LoginEnvelope>(&text)
            .map(|v| v.data)
            .map_err(|e| ApiError::Status(200, format!("bad json: {e}")))
    } else if status.as_u16() == 401 {
        Err(ApiError::Unauthorized)
    } else {
        let msg = serde_json::from_str::<ErrorBody>(&text)
            .map(|b| b.message)
            .unwrap_or_else(|_| text.clone());
        let code = serde_json::from_str::<ErrorBody>(&text)
            .ok()
            .and_then(|b| b.code.as_i64())
            .unwrap_or(status.as_u16() as i64);
        Err(ApiError::Api { code, message: msg })
    }
}

/// Refresh access token using the refresh token.
pub async fn refresh(base_url: &str, refresh_token: &str) -> Result<LoginResponse, ApiError> {
    let resp = client()
        .post(format!("{}/api/v1/auth/refresh", base_url))
        .json(&serde_json::json!({ "refresh_token": refresh_token }))
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::OK {
        serde_json::from_str::<LoginEnvelope>(&text)
            .map(|v| v.data)
            .map_err(|e| ApiError::Status(200, format!("bad json: {e}")))
    } else if status.as_u16() == 401 {
        Err(ApiError::Unauthorized)
    } else {
        Err(ApiError::Status(status.as_u16(), text))
    }
}

// ---- Dashboard stats endpoint ---------------------------------------------

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct UserInfo {
    pub balance: f64,
}

#[derive(Debug, Deserialize)]
struct UserInfoEnvelope {
    data: UserInfo,
}

/// Fetch dashboard stats with a given bearer token.
pub async fn fetch_stats(base_url: &str, access_token: &str) -> Result<StatsData, ApiError> {
    let resp = client()
        .get(format!(
            "{}/api/v1/usage/dashboard/stats?timezone=Asia/Shanghai",
            base_url
        ))
        .bearer_auth(access_token)
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::OK {
        serde_json::from_str::<StatsEnvelope>(&text)
            .map(|v| v.data)
            .map_err(|e| ApiError::Status(200, format!("bad json: {e}")))
    } else if status.as_u16() == 401 {
        Err(ApiError::Unauthorized)
    } else {
        Err(ApiError::Status(status.as_u16(), text))
    }
}

/// Fetch user info (balance, etc.) with a given bearer token.
pub async fn fetch_user_info(base_url: &str, access_token: &str) -> Result<UserInfo, ApiError> {
    let resp = client()
        .get(format!(
            "{}/api/v1/auth/me?timezone=Asia/Shanghai",
            base_url
        ))
        .bearer_auth(access_token)
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::OK {
        serde_json::from_str::<UserInfoEnvelope>(&text)
            .map(|v| v.data)
            .map_err(|e| ApiError::Status(200, format!("bad json: {e}")))
    } else if status.as_u16() == 401 {
        Err(ApiError::Unauthorized)
    } else {
        Err(ApiError::Status(status.as_u16(), text))
    }
}

// ---- Subscriptions endpoint ------------------------------------------------

/// Fetch active subscriptions with a given bearer token.
pub async fn fetch_subscriptions(base_url: &str, access_token: &str) -> Result<Vec<Subscription>, ApiError> {
    let resp = client()
        .get(format!(
            "{}/api/v1/subscriptions/active?timezone=Asia/Shanghai",
            base_url
        ))
        .bearer_auth(access_token)
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::OK {
        serde_json::from_str::<SubscriptionsEnvelope>(&text)
            .map(|v| v.data)
            .map_err(|e| ApiError::Status(200, format!("bad json: {e}")))
    } else if status.as_u16() == 401 {
        Err(ApiError::Unauthorized)
    } else {
        Err(ApiError::Status(status.as_u16(), text))
    }
}
