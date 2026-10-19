//! OIDC validation uses a JWT library; decoding a JWT is not verification.
use super::store::Account;
use anyhow::{Context, Result, ensure};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use reqwest::Client;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

pub const RESOURCE: &str = "https://api.openai.com/v1";
const ISSUER: &str = "https://auth.openai.com";
pub const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";

#[derive(Deserialize)]
pub struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub revocation_endpoint: Option<String>,
}

pub async fn discovery(client: &Client) -> Result<Discovery> {
    let metadata: Discovery = client
        .get(format!("{ISSUER}/.well-known/openid-configuration"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure!(metadata.issuer == ISSUER, "Unexpected OAuth issuer");
    for endpoint in [
        &metadata.authorization_endpoint,
        &metadata.token_endpoint,
        &metadata.jwks_uri,
    ]
    .into_iter()
    .chain(metadata.revocation_endpoint.iter())
    {
        let url = reqwest::Url::parse(endpoint)?;
        ensure!(
            url.origin().ascii_serialization() == ISSUER,
            "Untrusted OAuth endpoint"
        );
    }
    Ok(metadata)
}

#[derive(Deserialize)]
pub struct Identity {
    pub sub: String,
    pub email: Option<String>,
    pub nonce: Option<String>,
}

pub async fn verify(
    client: &Client,
    metadata: &Discovery,
    token: &str,
    client_id: &str,
    nonce: Option<&str>,
) -> Result<Identity> {
    let keys: JwkSet = client
        .get(&metadata.jwks_uri)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    validate(&keys, token, client_id, nonce)
}

fn validate(keys: &JwkSet, token: &str, client_id: &str, nonce: Option<&str>) -> Result<Identity> {
    let header = decode_header(token)?;
    ensure!(
        matches!(header.alg, Algorithm::RS256 | Algorithm::ES256),
        "Unexpected ID-token algorithm"
    );
    let jwk = keys
        .find(header.kid.as_deref().context("ID token has no kid")?)
        .context("Unknown ID-token signing key")?;
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&[ISSUER]);
    validation.set_audience(&[client_id]);
    validation.set_required_spec_claims(&["sub", "exp", "iat", "iss", "aud"]);
    validation.leeway = 5;
    let identity = decode::<Identity>(token, &DecodingKey::from_jwk(jwk)?, &validation)?.claims;
    ensure!(!identity.sub.is_empty(), "Missing account identity");
    if let Some(nonce) = nonce {
        ensure!(
            identity.nonce.as_deref() == Some(nonce),
            "ID-token nonce mismatch"
        );
    }
    Ok(identity)
}

#[cfg(test)]
#[path = "token_tests.rs"]
mod tests;

#[derive(Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub scope: Option<String>,
    pub expires_in: u64,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn check_scope(scope: &str) -> Result<()> {
    ensure!(
        scope.split_whitespace().any(|s| s == PLAN_SCOPE),
        "ChatGPT plan usage was not authorized; run login again and grant plan access"
    );
    Ok(())
}

pub async fn refresh(client: &Client, account: &mut Account) -> Result<()> {
    let refresh = account
        .refresh_token
        .as_deref()
        .context("Run nano-codex login to renew the session")?;
    let metadata = discovery(client).await?;
    let response = client
        .post(&metadata.token_endpoint)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", account.client_id.as_str()),
            ("refresh_token", refresh),
            ("resource", RESOURCE),
        ])
        .send()
        .await?
        .error_for_status()
        .context("Token refresh failed; run nano-codex login")?;
    let tokens: TokenResponse = response.json().await?;
    if let Some(token) = &tokens.id_token {
        let identity = verify(client, &metadata, token, &account.client_id, None).await?;
        ensure!(
            identity.sub == account.subject,
            "Refreshed account identity changed"
        );
    }
    let scope = tokens.scope.unwrap_or_else(|| account.scope.clone());
    check_scope(&scope)?;
    ensure!(
        !tokens.access_token.is_empty() && tokens.expires_in > 0,
        "Invalid refreshed token"
    );
    account.access_token = Some(tokens.access_token);
    if tokens.refresh_token.is_some() {
        account.refresh_token = tokens.refresh_token;
    }
    if tokens.id_token.is_some() {
        account.id_token = tokens.id_token;
    }
    account.scope = scope;
    account.expires_at = now().saturating_add(tokens.expires_in);
    Ok(())
}
