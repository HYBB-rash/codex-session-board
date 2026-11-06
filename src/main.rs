use anyhow::Result;
use clap::{Parser, Subcommand};
use codex_session_board::{app::App, config, http, mcp, observer, service};
use std::path::PathBuf;
use std::sync::Arc;

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
    Launch,
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
        Action::Launch => {
            config::initialize_config(path, &defaults.1)?;
            println!("{}", service::launch(path)?);
            Ok(())
        }
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

fn serve(path: &std::path::Path) -> Result<()> {
    let (default_path, data) = config::default_paths()?;
    if path == default_path {
        config::initialize_config(path, &data)?;
    }
    let config = config::load(Some(path))?;
    let (server, port) = http::bind(config.port)?;
    let app = Arc::new(App::new(config)?);
    service::register(&app.config, port)?;
    observer::start(&app);
    println!("http://127.0.0.1:{port}");
    std::io::Write::flush(&mut std::io::stdout())?;
    http::run(server, port, app)
}

fn scan(path: &std::path::Path) -> Result<()> {
    let config = config::load(Some(path))?;
    let app = App::new(config)?;
    let mut reader = codex_session_board::rpc::StdioReader::connect(
        &app.config.codex_command,
        std::time::Duration::from_secs(app.config.rpc_timeout_seconds),
    )?;
    observer::scan_history(&app, &mut reader)?;
    println!("{}", app.snapshot()?);
    Ok(())
}
