use std::process::ExitCode;

use bird_mcp::config::Config;
use bird_mcp::server::Bird;

const HELP: &str = "\
bird-mcp — MCP server for the BIRD Internet Routing Daemon

Usage:
  bird-mcp                 Serve MCP over stdio (default)
  bird-mcp --check         Validate the configuration and print the routers
  bird-mcp --config PATH   Use PATH as the router configuration file
  bird-mcp --version       Print the version
  bird-mcp --help          Show this help

Configuration:
  BIRD_MCP_ROUTERS         JSON array of routers (overrides the config file)
  BIRD_MCP_CONFIG          TOML config file, default ~/.config/bird-mcp/config.toml
  BIRD_MCP_DOCS_DIR        BIRD doc directory for the documentation tools
  BIRD_MCP_TIMEOUT         Default command timeout in seconds (20)
  BIRD_MCP_BIRDC           birdc binary for the default local router
  BIRD_MCP_SOCKET          Control socket for the default local router
  BIRD_MCP_CONFIG_FILE     Config file on the router (/etc/bird/bird.conf)
  BIRD_MCP_SSH_BIN         ssh binary used for ssh routers (ssh)

Example ~/.config/bird-mcp/config.toml:

  docs_dir = \"/srv/bird/doc\"

  [[routers]]
  name = \"r1\"
  transport = \"ssh\"
  host = \"router1.lab.example.net\"
  user = \"root\"
  socket = \"/run/bird/bird.ctl\"
  read_only = true

  [[routers]]
  name = \"local\"
  transport = \"local\"

Routers are read-only unless read_only = false is set explicitly.
";

enum Command {
    Serve,
    Check,
    Help,
    Version,
}

fn main() -> ExitCode {
    let mut command = Command::Serve;
    let mut config_path: Option<String> = None;
    let mut args = std::env::args().skip(1).peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => command = Command::Help,
            "--version" | "-V" => command = Command::Version,
            "--check" => command = Command::Check,
            "--config" => match args.next() {
                Some(path) => config_path = Some(path),
                None => {
                    eprintln!("bird-mcp: --config needs a path\n\nTry `bird-mcp --help`.");
                    return ExitCode::from(2);
                }
            },
            other => {
                eprintln!("bird-mcp: unknown argument '{other}'\n\nTry `bird-mcp --help`.");
                return ExitCode::from(2);
            }
        }
    }

    match command {
        Command::Help => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Command::Version => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Command::Check | Command::Serve => {}
    }

    let mut pairs: Vec<(String, String)> = std::env::vars().collect();
    if let Some(path) = config_path {
        pairs.retain(|(key, _)| key != "BIRD_MCP_CONFIG");
        pairs.push(("BIRD_MCP_CONFIG".to_string(), path));
    }
    let borrowed: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let config = match Config::from_pairs(&borrowed) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("bird-mcp: {err}");
            return ExitCode::FAILURE;
        }
    };

    if matches!(command, Command::Check) {
        println!("{}", config.summary());
        for router in &config.routers {
            println!(
                "  {}: {} via {}{}",
                router.name,
                router.target(),
                router.birdc,
                router
                    .socket
                    .as_ref()
                    .map(|socket| format!(", socket {socket}"))
                    .unwrap_or_default()
            );
        }
        return ExitCode::SUCCESS;
    }

    serve(config)
}

fn serve(config: Config) -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "bird_mcp=info".into()),
        )
        .init();

    tracing::info!("{}", config.summary());
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("bird-mcp: cannot start async runtime: {err}");
            return ExitCode::FAILURE;
        }
    };

    let result: anyhow::Result<()> = runtime.block_on(async {
        let service = rmcp::serve_server(Bird::new(config), rmcp::transport::stdio()).await?;
        tracing::info!("stdio transport ready");
        service.waiting().await?;
        Ok(())
    });

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("bird-mcp: {err}");
            ExitCode::FAILURE
        }
    }
}
