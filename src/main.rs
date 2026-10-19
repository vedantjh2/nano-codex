//! CLI wiring. All agent behavior lives in the library's small, named modules.
use anyhow::{Result, ensure};
use clap::Parser;
use nano_codex::{
    agent, auth,
    config::{Action, Cli, Config},
    provider::Provider,
    session::Session,
    setup,
    ui::{Ui, visible},
};
use serde_json::json;
use std::time::Duration;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    // Dropping the task cancels HTTP and kills the shell's process group.
    let result = tokio::select! {
        result = start(cli) => Some(result),
        _ = tokio::signal::ctrl_c() => {
            eprintln!("\nInterrupted. Resume the session to inspect and continue.");
            None
        }
    };
    let code = match result {
        Some(Ok(())) => 0,
        Some(Err(error)) => {
            eprintln!("Error: {}", visible(&format!("{error:#}")));
            1
        }
        None => 130,
    };
    // Tokio's stdin reader can block runtime shutdown. All work is flushed above.
    std::process::exit(code);
}

async fn start(cli: Cli) -> Result<()> {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    match &cli.command {
        Some(Action::Login {
            command,
            from_codex,
            device_auth,
            standalone,
        }) => {
            return auth::commands::login(&http, command, *from_codex, *device_auth, *standalone)
                .await;
        }
        Some(Action::Logout) => return auth::commands::logout(&http).await,
        _ => {}
    }
    let mut ui = Ui::new(cli.options.json);
    if matches!(cli.command, Some(Action::Setup)) {
        return setup::run(&mut ui, &http).await;
    }
    if ui.terminal && cli.command.is_none() && setup::needed(&cli.options)? {
        setup::run(&mut ui, &http).await?;
    }
    let config = Config::load(cli.options)?;
    let provider = Provider::new(&config)?;
    if matches!(cli.command, Some(Action::Models)) {
        for model in provider.models().await? {
            println!("{}", visible(&model));
        }
        return Ok(());
    }
    ensure!(
        !config.model.is_empty(),
        "Local inference requires --model (use the server's served model name)"
    );
    let (resume, initial, one_shot) = match cli.command {
        Some(Action::Exec { prompt }) => (None, Some(prompt), true),
        Some(Action::Resume { id, prompt }) => (Some(id), prompt, !ui.terminal),
        None => (None, cli.prompt, !ui.terminal),
        _ => unreachable!(),
    };
    ensure!(
        ui.terminal || initial.is_some(),
        "Supply a prompt: nano-codex exec 'your task'"
    );
    let mut session = Session::open(&config, resume.as_deref())?;
    ui.event(
        "session",
        json!({"id":session.id,"model":config.model,"cwd":config.cwd}),
    );
    if let Some(prompt) = initial {
        task(&config, &provider, &mut session, &mut ui, &prompt).await?;
    }
    if one_shot {
        return Ok(());
    }
    eprintln!("nano-codex — /help for commands; Ctrl-C to stop.");
    while let Some(prompt) = ui.line("\nnano> ").await? {
        match prompt.trim() {
            "" => {},
            "/exit" | "/quit" => break,
            "/help" => eprintln!("/new starts a session; /status shows state; /exit quits.\nType a task to run the agent. Multi-line tasks can be passed to exec as one quoted argument."),
            "/new" => {
                session = Session::open(&config, None)?;
                ui.event("session", json!({"id":session.id}));
            }
            "/status" => ui.event("status", json!({"session":session.id,"messages":session.messages.len(),"model":config.model})),
            _ => if let Err(error) = task(&config, &provider, &mut session, &mut ui, &prompt).await {
                ui.event("error", json!(format!("{error:#}")));
            },
        }
    }
    Ok(())
}

async fn task(
    config: &Config,
    provider: &Provider,
    session: &mut Session,
    ui: &mut Ui,
    prompt: &str,
) -> Result<()> {
    ensure!(!prompt.trim().is_empty(), "Task must not be empty");
    let result = agent::run(config, provider, session, ui, prompt).await;
    if result.is_err() {
        session.repair_pending()?;
    }
    result
}
