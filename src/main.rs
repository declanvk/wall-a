use std::path::PathBuf;

use argh::FromArgs;
use tracing_subscriber::{filter::EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{append::AppendCommand, read::ReadCommand};

mod append;
mod archive;
mod read;
mod staging;
mod value;

/// WALL•A is a tool for incrementally storing JSON data and then
/// compacting it once it reaches a certain size.
#[derive(Debug, PartialEq, FromArgs)]
struct Command {
    /// the path to the data directory
    #[argh(option)]
    data_dir: PathBuf,

    #[argh(subcommand)]
    subcommand: Subcommand,
}

impl Command {
    fn execute(self) -> anyhow::Result<()> {
        self.subcommand.execute(self.data_dir)
    }
}

#[derive(Debug, PartialEq, FromArgs)]
#[argh(subcommand)]
enum Subcommand {
    Read(ReadCommand),
    Append(AppendCommand),
}

impl Subcommand {
    fn execute(self, data_dir: PathBuf) -> anyhow::Result<()> {
        match self {
            Self::Read(sub) => sub.execute(data_dir),
            Self::Append(sub) => sub.execute(data_dir),
        }
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_env("WALLA_LOG"))
        .init();

    let command: Command = argh::from_env();
    tracing::debug!("{command:?}");

    command.execute()
}
