//! This module contains the implementation of the `read` CLI command

use std::path::PathBuf;

use argh::FromArgs;

/// The `read` sub-command reads and merges all the archived JSON data
/// into a single object and outputs it to stdout.
#[derive(Debug, PartialEq, FromArgs)]
#[argh(subcommand, name = "read")]
pub struct ReadCommand {}

impl ReadCommand {
    /// This function executes the read command.
    #[tracing::instrument]
    pub fn execute(self, data_dir: PathBuf) -> anyhow::Result<()> {
        todo!()
    }
}
