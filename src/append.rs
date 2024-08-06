//! This module contains the implementation of the `append` CLI command

use std::{
    io::{self, BufRead, StdinLock, Write},
    ops::ControlFlow,
    path::PathBuf,
};

use anyhow::Context;
use argh::FromArgs;
use serde_json::Value;
use uom::si::{
    information::{byte, megabyte},
    u64::Information,
};

use super::{
    archive::archive_value,
    convert::json_to_cbor,
    staging::{delete_staging_file, StagingFileReader, StagingFileWriter},
};

fn default_staging_limit() -> Information {
    Information::new::<megabyte>(1)
}

/// The `append` sub-command reads new lines of JSON data from stdin
/// and archives it.
///
/// If the total amount of data in the staging area passes a configurable
/// limit, then the staging file is converted to a binary format and
/// compressed.
#[derive(Debug, PartialEq, FromArgs)]
#[argh(subcommand, name = "append")]
pub struct AppendCommand {
    /// this option gives the maximum size that the staging file reach
    /// before it is archived and a new staging file is created.
    #[argh(option, default = "default_staging_limit()")]
    staging_limit: Information,
}

impl AppendCommand {
    /// This function executes the append command.
    #[tracing::instrument]
    pub fn execute(self, data_dir: PathBuf) -> anyhow::Result<()> {
        let staging_limit_bytes = self.staging_limit.get::<byte>();
        let stdin = io::stdin();
        let handle = stdin.lock();

        let mut state = State::new(data_dir, staging_limit_bytes, handle);

        loop {
            match state.read_and_append() {
                Ok(ControlFlow::Continue(())) => {
                    continue;
                }
                Ok(ControlFlow::Break(())) => {
                    StagingFileWriter::flush_if_present(&mut state.staging_file)?;

                    break Ok(());
                }
                Err(err) => {
                    StagingFileWriter::flush_if_present(&mut state.staging_file)?;

                    break Err(err);
                }
            }
        }
    }
}

#[derive(Debug)]
struct State {
    data_dir: PathBuf,
    handle: StdinLock<'static>,
    line: String,
    line_bytes: Vec<u8>,
    staging_file: Option<StagingFileWriter>,
    added_bytes: u64,
    staging_limit_bytes: u64,
}

impl State {
    fn new(data_dir: PathBuf, staging_limit_bytes: u64, handle: StdinLock<'static>) -> Self {
        Self {
            data_dir,
            handle,
            line: String::new(),
            line_bytes: Vec::new(),
            staging_file: None,
            added_bytes: 0,
            staging_limit_bytes,
        }
    }

    fn read_and_append(&mut self) -> anyhow::Result<ControlFlow<()>> {
        self.line.clear();
        self.line_bytes.clear();

        let num_bytes = self
            .handle
            .read_line(&mut self.line)
            .context("reading line from stdin")?;
        tracing::debug!(%num_bytes, "Read line with non-zero bytes");
        if num_bytes == 0 {
            tracing::debug!("Reached EOF in stdin");
            return Ok(ControlFlow::Break(()));
        }

        let value: Value =
            serde_json::from_str(&self.line).context("converting line to JSON value")?;
        tracing::trace!(?value, "Got JSON value");

        serde_json::to_writer(&mut self.line_bytes, &value)
            .context("converting JSON value to bytes")?;
        self.line_bytes.push(b'\n');
        let line_num_bytes = self.line_bytes.len() as u64;
        tracing::trace!(num_bytes = ?line_num_bytes, "Converted JSON value back to bytes");

        let staging_file =
            StagingFileWriter::get_mut_or_open(&mut self.staging_file, &self.data_dir)
                .context("accessing staging file")?;
        let staging_initial_len = staging_file.initial_len();

        staging_file
            .writer()
            .write_all(&self.line_bytes)
            .context("writing JSON bytes to staging")?;
        self.added_bytes += line_num_bytes;
        tracing::debug!(%self.added_bytes, %line_num_bytes, "Wrote JSON bytes with newline to staging file");

        if staging_initial_len + self.added_bytes > self.staging_limit_bytes {
            tracing::info!(
                staging_file_length_bytes = %staging_initial_len,
                %self.added_bytes,
                %self.staging_limit_bytes,
                "Staging file size has increased past provided limit, going to archive"
            );

            staging_file
                .writer()
                .flush()
                .context("flushing staging file before archiving")?;

            self.archive_staging_file()
                .context("archiving staging file")?;
        }

        Ok(ControlFlow::Continue(()))
    }

    /// Take the current contents of the staging file and buffered updates
    fn archive_staging_file(&mut self) -> anyhow::Result<()> {
        // Drop the append-only staging file reference if it exists
        drop(self.staging_file.take());

        let staging_value = StagingFileReader::read_merged_value(&self.data_dir)
            .context("opening staging file for archiving")?;

        let Some(staging_value) = staging_value else {
            // No values in staging file
            tracing::warn!("Staging file was empty, not continuing with archiving");
            return Ok(());
        };

        let cbor_value =
            json_to_cbor(staging_value).context("converting staging value from JSON to CBOR")?;

        archive_value(&self.data_dir, cbor_value).context("writing CBOR value to archive")?;

        delete_staging_file(&self.data_dir).context("cleaning up staging file")?;

        Ok(())
    }
}
