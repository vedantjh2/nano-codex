//! First-run setup persists only connection settings; authentication owns secrets.
use crate::{
    auth,
    cli::Options,
    config::{self, Config, ProviderKind},
    provider::Provider,
    ui::Ui,
};
use anyhow::{Context, Result, ensure};
use reqwest::Client;

pub fn needed(options: &Options) -> Result<bool> {
    if options.provider.is_some() || options.base_url.is_some() || options.model.is_some() {
        return Ok(false);
    }
    let path = config::home()?.join("config.toml");
    if !path.exists() {
        return Ok(true);
    }
    let saved: Options = toml::from_str(&std::fs::read_to_string(path)?)?;
    Ok(saved.provider.is_none())
}

async fn ask(ui: &mut Ui, prompt: &str, default: &str) -> Result<String> {
    let answer = ui.line(prompt).await?.context("Setup cancelled")?;
    Ok(if answer.trim().is_empty() {
        default.into()
    } else {
        answer.trim().into()
    })
}

pub fn local_url(input: &str, default_port: u16) -> Result<String> {
    let input = input.trim();
    let input = if input.is_empty() {
        default_port.to_string()
    } else {
        input.into()
    };
    let root = if input.chars().all(|c| c.is_ascii_digit()) {
        let port: u16 = input.parse().context("Port must be between 1 and 65535")?;
        ensure!(port > 0, "Port must be between 1 and 65535");
        format!("http://127.0.0.1:{port}/v1")
    } else if input.contains("://") {
        input.trim_end_matches('/').into()
    } else {
        format!("http://{input}/v1")
    };
    let mut url = reqwest::Url::parse(&root)?;
    ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "Use an HTTP(S) endpoint"
    );
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Endpoint must not contain credentials or query parameters"
    );
    if url.path() == "/" {
        url.set_path("/v1");
    }
    Ok(url.to_string().trim_end_matches('/').into())
}

async fn choose_model(ui: &mut Ui, provider: &Provider) -> Result<String> {
    let models = provider.models().await?;
    ensure!(!models.is_empty(), "Server returned no available models");
    for (index, model) in models.iter().enumerate() {
        eprintln!("  {}. {}", index + 1, crate::ui::visible(model));
    }
    let selection = ask(ui, "Model number or name [1]: ", "1").await?;
    if let Ok(index) = selection.parse::<usize>() {
        return models
            .get(index.wrapping_sub(1))
            .cloned()
            .context("Model number is out of range");
    }
    ensure!(
        models.contains(&selection),
        "Choose a model served by this endpoint"
    );
    Ok(selection)
}

pub async fn run(ui: &mut Ui, client: &Client) -> Result<()> {
    ensure!(
        ui.terminal,
        "Setup needs a terminal; use --provider, --model and --base-url for scripts"
    );
    eprintln!(
        "Welcome to nano-codex. Choose your model connection:\n  1. OpenAI / ChatGPT account\n  2. vLLM (local or cluster)\n  3. SGLang (local or cluster)"
    );
    let choice = ask(ui, "Connection [1]: ", "1").await?;
    let (kind, url) = match choice.as_str() {
        "1" => {
            eprintln!(
                "  1. Use an existing Codex login\n  2. Sign in with browser\n  3. Sign in with device code\n  4. Native OAuth example (independent registration)\n  5. OpenAI API key from OPENAI_API_KEY"
            );
            let default = if auth::codex::available() { "1" } else { "2" };
            let choice = ask(ui, &format!("Account method [{default}]: "), default).await?;
            match choice.as_str() {
                "1" => auth::codex::login(true, false).await?,
                "2" => auth::codex::login(false, false).await?,
                "3" => auth::codex::login(false, true).await?,
                "4" => auth::commands::login(client, &None, false, false, true).await?,
                "5" => {
                    ensure!(
                        std::env::var("OPENAI_API_KEY").is_ok_and(|s| !s.is_empty()),
                        "Set OPENAI_API_KEY and run nano-codex setup again"
                    );
                }
                _ => anyhow::bail!("Choose an account method from 1 to 5"),
            }
            if choice == "5" {
                (ProviderKind::Openai, "https://api.openai.com/v1".into())
            } else {
                let config = Config::load(Options::default())?;
                (config.provider, config.base_url)
            }
        }
        "2" | "3" => {
            let (kind, port) = if choice == "2" {
                (ProviderKind::Vllm, 8000)
            } else {
                (ProviderKind::Sglang, 30000)
            };
            let input = ask(
                ui,
                &format!("Server port or API URL [{port}]: "),
                &port.to_string(),
            )
            .await?;
            (kind, local_url(&input, port)?)
        }
        _ => anyhow::bail!("Choose a connection from 1 to 3"),
    };
    let config = Config::load(Options {
        provider: Some(kind),
        base_url: Some(url.clone()),
        ..Default::default()
    })?;
    let model = choose_model(ui, &Provider::new(&config)?).await?;
    config::save_connection(kind, &model, &url)?;
    eprintln!("Saved connection. Type a task to begin.");
    Ok(())
}
