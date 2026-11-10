use anyhow::Result;
use clap::{Parser, Subcommand};
use codex_session_board::{config, mcp, observer, service};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(about = "Codex 本地会话看板", version)]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    Serve,
    Stop,
    Mcp,
    Scan,
    Migrate {
        #[arg(long)]
        source_config: PathBuf,
    },
}

fn main() -> Result<()> {
    let arguments = Cli::parse();
    let defaults = config::default_paths()?;
    let path = arguments.config.as_deref().unwrap_or(&defaults.0);
    match arguments.command.unwrap_or(Action::Serve) {
        Action::Mcp => mcp::serve(Some(path)),
        Action::Serve => serve(path),
        Action::Scan => scan(path),
        Action::Stop => {
            println!("{}", service::stop(path)?);
            Ok(())
        }
        Action::Migrate { source_config } => {
            println!("{}", service::migrate(path, &source_config)?);
            Ok(())
        }
    }
}

fn serve(path: &Path) -> Result<()> {
    match service::open(path)? {
        service::OpenedBoard::Reused { url, config } => {
            println!("{}", board_status(&url, false, &config));
            Ok(())
        }
        service::OpenedBoard::Owned(board) => {
            println!("{}", board_status(&board.url, true, &board.config));
            std::io::Write::flush(&mut std::io::stdout())?;
            board.run()
        }
    }
}

fn board_status(url: &str, started: bool, config: &Path) -> Value {
    json!({
        "url": url,
        "running": true,
        "started": started,
        "config": config
    })
}

fn scan(path: &Path) -> Result<()> {
    let config = config::load(Some(path))?;
    let app = codex_session_board::app::App::new(config)?;
    let mut reader = codex_session_board::rpc::StdioReader::connect(
        &app.config.codex_command,
        std::time::Duration::from_secs(app.config.rpc_timeout_seconds),
    )?;
    observer::scan_history(&app, &mut reader)?;
    println!("{}", app.snapshot()?);
    Ok(())
}
