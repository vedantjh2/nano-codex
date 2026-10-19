//! Public-client OAuth: loopback listener + state + nonce + PKCE, no client secret.
use super::{
    store::{Account, Store},
    tokens::{self, RESOURCE, TokenResponse},
};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use reqwest::{Client, Url};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn random() -> String {
    let mut bytes = [0; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

async fn callback(listener: &TcpListener, state: &str) -> Result<HashMap<String, String>> {
    loop {
        let (mut socket, _) = listener.accept().await?;
        let mut request = Vec::new();
        // Ignore unrelated browser requests, and bound idle sockets and headers.
        let read = tokio::time::timeout(Duration::from_secs(5), async {
            let mut byte = [0];
            while request.len() < 16_384 && !request.ends_with(b"\r\n\r\n") {
                if socket.read(&mut byte).await? == 0 {
                    break;
                }
                request.push(byte[0]);
            }
            Ok::<_, std::io::Error>(())
        })
        .await;
        if !matches!(read, Ok(Ok(()))) {
            continue;
        }
        let request = String::from_utf8_lossy(&request);
        let mut first = request.lines().next().unwrap_or("").split_whitespace();
        let method = first.next().unwrap_or("");
        let target = first.next().unwrap_or("/");
        let url = Url::parse(&format!("http://127.0.0.1{target}"))?;
        let parameters: HashMap<_, _> = url.query_pairs().into_owned().collect();
        let valid = method == "GET"
            && url.path() == "/auth/callback"
            && parameters
                .get("state")
                .is_some_and(|returned| returned == state);
        let response = if valid {
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nReturn to nano-codex to finish sign-in."
        } else {
            "HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\nInvalid callback."
        };
        let _ = socket.write_all(response.as_bytes()).await;
        if valid {
            return Ok(parameters);
        }
    }
}

pub async fn login(client: &Client) -> Result<()> {
    let store = Store::open()?;
    let saved = store.load()?;
    let host = store.host_id()?;
    let metadata = tokens::discovery(client).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let redirect = format!(
        "http://127.0.0.1:{}/auth/callback",
        listener.local_addr()?.port()
    );
    let (state, nonce, verifier) = (random(), random(), random());
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let client_id = saved
        .as_ref()
        .map_or("dynamic_agent_client", |a| a.client_id.as_str());
    let mut authorize = Url::parse(&metadata.authorization_endpoint)?;
    authorize.query_pairs_mut().extend_pairs([
        ("client_id", client_id),
        ("ext_agent_host_id", host.as_str()),
        ("response_type", "code"),
        ("redirect_uri", redirect.as_str()),
        ("resource", RESOURCE),
        ("state", state.as_str()),
        ("nonce", nonce.as_str()),
        ("code_challenge_method", "S256"),
        ("code_challenge", challenge.as_str()),
        (
            "scope",
            "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct",
        ),
    ]);
    if saved.is_none() {
        authorize
            .query_pairs_mut()
            .append_pair("agent_name_hint", "nano-codex");
    }
    eprintln!(
        "Continue with ChatGPT. Open this URL in your browser:\n{authorize}\n\nWaiting up to 5 minutes. ChatGPT plan access depends on account availability."
    );
    let parameters = tokio::time::timeout(Duration::from_secs(300), callback(&listener, &state))
        .await
        .context("Sign-in timed out")??;
    ensure!(
        !parameters.contains_key("error"),
        "Sign-in was declined or failed"
    );
    let issued = parameters
        .get("client_id")
        .map(String::as_str)
        .or_else(|| saved.as_ref().map(|a| a.client_id.as_str()))
        .context("Registration did not return a client ID")?;
    ensure!(issued != "dynamic_agent_client", "Missing issued client ID");
    if let Some(account) = &saved {
        ensure!(issued == account.client_id, "Returning client ID changed");
    }
    let code = parameters
        .get("code")
        .context("Missing authorization code")?;
    let tokens: TokenResponse = client
        .post(&metadata.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", issued),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("resource", RESOURCE),
        ])
        .send()
        .await?
        .error_for_status()
        .context("Code exchange failed; start a fresh login")?
        .json()
        .await?;
    let identity = tokens::verify(
        client,
        &metadata,
        tokens.id_token.as_deref().context("Missing ID token")?,
        issued,
        Some(&nonce),
    )
    .await?;
    if let Some(account) = &saved {
        ensure!(
            account.subject == identity.sub,
            "Different ChatGPT identity; use a separate NANO_CODEX_HOME for another account"
        );
    }
    let scope = tokens.scope.context("Missing granted scopes")?;
    tokens::check_scope(&scope)?;
    ensure!(
        !tokens.access_token.is_empty() && tokens.expires_in > 0,
        "Invalid access token"
    );
    let account = Account {
        client_id: issued.into(),
        subject: identity.sub,
        email: identity.email,
        access_token: Some(tokens.access_token),
        refresh_token: tokens.refresh_token,
        id_token: tokens.id_token,
        scope,
        expires_at: tokens::now().saturating_add(tokens.expires_in),
    };
    store.save(&account)?;
    eprintln!(
        "ChatGPT linked. Run nano-codex --provider chatgpt. Use ChatGPT Settings → Usage to manage access."
    );
    Ok(())
}

#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;
