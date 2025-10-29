//! OAuth 2.0 client credentials flow for Esplora Enterprise API

use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct OAuthConfig {
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: String,
}

impl OAuthConfig {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            token_url: "https://login.blockstream.com/realms/blockstream-public/protocol/openid-connect/token".to_string(),
            client_id,
            client_secret,
            scope: "openid".to_string(),
        }
    }

    pub fn with_token_url(mut self, url: String) -> Self {
        self.token_url = url;
        self
    }

    pub fn with_scope(mut self, scope: String) -> Self {
        self.scope = scope;
        self
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TokenInfo {
    token: String,
    expires_at: Option<SystemTime>,
}

impl TokenInfo {
    fn is_valid(&self) -> bool {
        match self.expires_at {
            Some(expires) => SystemTime::now() < expires,
            None => true,
        }
    }
}

/// Thread-safe OAuth token manager
#[derive(Clone, Debug)]
pub struct OAuthManager {
    inner: Arc<Mutex<OAuthManagerInner>>,
}

#[derive(Debug)]
struct OAuthManagerInner {
    config: OAuthConfig,
    token_info: Option<TokenInfo>,
}

/// OAuth error types
#[derive(Debug)]
pub enum OAuthError {
    /// OAuth authentication failed: {0}
    OAuth(String),
    
    /// Error during HTTP request (blocking)
    #[cfg(feature = "blocking")]
    HttpBlocking(minreq::Error),
    
    /// Error during HTTP request (async)
    #[cfg(feature = "async")]
    HttpAsync(reqwest::Error),
    
    /// JSON deserialization error
    Deserialize(serde_json::Error),
}

impl Display for OAuthError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            OAuthError::OAuth(msg) => write!(f, "OAuth authentication failed: {}", msg),
            #[cfg(feature = "blocking")]
            OAuthError::HttpBlocking(e) => write!(f, "HTTP error (blocking): {}", e),
            #[cfg(feature = "async")]
            OAuthError::HttpAsync(e) => write!(f, "HTTP error (async): {}", e),
            OAuthError::Deserialize(e) => write!(f, "JSON deserialization error: {}", e),
        }
    }
}

impl StdError for OAuthError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            OAuthError::OAuth(_) => None,
            #[cfg(feature = "blocking")]
            OAuthError::HttpBlocking(e) => Some(e),
            #[cfg(feature = "async")]
            OAuthError::HttpAsync(e) => Some(e),
            OAuthError::Deserialize(e) => Some(e),
        }
    }
}

#[cfg(feature = "blocking")]
impl From<minreq::Error> for OAuthError {
    fn from(e: minreq::Error) -> Self {
        OAuthError::HttpBlocking(e)
    }
}

#[cfg(feature = "async")]
impl From<reqwest::Error> for OAuthError {
    fn from(e: reqwest::Error) -> Self {
        OAuthError::HttpAsync(e)
    }
}

impl From<serde_json::Error> for OAuthError {
    fn from(e: serde_json::Error) -> Self {
        OAuthError::Deserialize(e)
    }
}

impl OAuthManager {
    pub fn new(config: OAuthConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(OAuthManagerInner {
                config,
                token_info: None,
            })),
        }
    }

    /// Get a valid access token, refreshing if necessary
    pub fn get_access_token(&self) -> Result<String, OAuthError> {
        let mut inner = self.inner.lock().unwrap();

        // Check if we have a valid token
        if let Some(ref token_info) = inner.token_info {
            if token_info.is_valid() {
                return Ok(token_info.token.clone());
            }
        }

        // Need to fetch a new token
        #[cfg(feature = "blocking")]
        {
            let response = self.fetch_token_blocking(&inner.config)?;
            let token_info = TokenInfo {
                token: response.access_token.clone(),
                expires_at: response.expires_in.map(|seconds| {
                    SystemTime::now() + Duration::from_secs(seconds)
                }),
            };
            inner.token_info = Some(token_info.clone());
            Ok(token_info.token)
        }

        #[cfg(not(feature = "blocking"))]
        {
            unreachable!()
        }
    }

    #[cfg(feature = "async")]
    /// Get a valid access token asynchronously
    pub async fn get_access_token_async(&self) -> Result<String, OAuthError> {
        let mut inner = self.inner.lock().unwrap();

        // Check if we have a valid token
        if let Some(ref token_info) = inner.token_info {
            if token_info.is_valid() {
                return Ok(token_info.token.clone());
            }
        }

        // Need to fetch a new token
        let response = self.fetch_token_async(&inner.config).await?;
        let token_info = TokenInfo {
            token: response.access_token.clone(),
            expires_at: response.expires_in.map(|seconds| {
                SystemTime::now() + Duration::from_secs(seconds)
            }),
        };
        inner.token_info = Some(token_info.clone());
        Ok(token_info.token)
    }

    #[cfg(feature = "blocking")]
    fn fetch_token_blocking(&self, config: &OAuthConfig) -> Result<TokenResponse, OAuthError> {
        use minreq::Method;

        let mut params = HashMap::new();
        params.insert("client_id", config.client_id.as_str());
        params.insert("client_secret", config.client_secret.as_str());
        params.insert("grant_type", "client_credentials");
        params.insert("scope", config.scope.as_str());

        let body = params.iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let response = minreq::Request::new(Method::Post, &config.token_url)
            .with_header("Content-Type", "application/x-www-form-urlencoded")
            .with_body(body)
            .send()?;

        if response.status_code != 200 {
            return Err(OAuthError::OAuth(format!(
                "Failed to fetch token: status {}, body: {}",
                response.status_code,
                response.as_str().unwrap_or_default()
            )));
        }

        serde_json::from_str(response.as_str()?).map_err(OAuthError::Deserialize)
    }

    #[cfg(feature = "async")]
    async fn fetch_token_async(&self, config: &OAuthConfig) -> Result<TokenResponse, OAuthError> {
        use reqwest::Client;

        let client = Client::new();

        let params = vec![
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("grant_type", "client_credentials"),
            ("scope", config.scope.as_str()),
        ];

        let response = client
            .post(&config.token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            return Err(OAuthError::OAuth(format!(
                "Failed to fetch token: status {}, body: {}",
                status, body
            )));
        }

        let text = response.text().await?;
        serde_json::from_str(&text).map_err(OAuthError::Deserialize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_info_validity() {
        let token = TokenInfo {
            token: "test".to_string(),
            expires_at: None,
        };
        assert!(token.is_valid());

        let token = TokenInfo {
            token: "test".to_string(),
            expires_at: Some(SystemTime::now() + Duration::from_secs(3600)),
        };
        assert!(token.is_valid());

        let token = TokenInfo {
            token: "test".to_string(),
            expires_at: Some(SystemTime::now() - Duration::from_secs(3600)),
        };
        assert!(!token.is_valid());
    }
}
