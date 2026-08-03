mod broker;
mod doctor;
mod install;
mod mcp;
mod settings;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "effector", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the Chrome Native Messaging broker.
    NativeHost,
    /// Register this executable as Chrome's per-user Native Messaging host.
    Install {
        /// The 32-character Chrome extension ID shown on chrome://extensions.
        #[arg(long)]
        extension_id: String,
    },
    /// Check whether an Effector broker is currently reachable.
    Doctor,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Chrome appends the calling extension origin when it launches a native
    // host. The native host manifest cannot include a subcommand, so recognize
    // that invocation before asking clap to parse normal CLI arguments.
    if std::env::args()
        .nth(1)
        .is_some_and(|arg| arg.starts_with("chrome-extension://"))
    {
        return broker::run().await;
    }

    match Cli::parse().command {
        Command::NativeHost => broker::run().await,
        Command::Install { extension_id } => install::run(&extension_id),
        Command::Doctor => doctor::run().await,
    }
}
